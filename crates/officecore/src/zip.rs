//! Minimal, hardened ZIP container support.
//!
//! Office document formats (DOCX/XLSX/PPTX, ODT/ODS/ODP) are ZIP packages.
//! We implement the small subset we need (deflate + stored entries, central
//! directory) ourselves so that:
//!
//! * limits stay under our control (ZIP bomb protection),
//! * we do not depend on a ZIP crate whose API changes between majors,
//! * every extraction goes through one audited code path.

use crate::error::{ErrorCode, OfficeError, OfficeResult};
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::collections::HashMap;
use std::io::{Read, Write};

const LOCAL_SIG: u32 = 0x0403_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const EOCD_SIG: u32 = 0x0605_4b50;

/// Extraction limits. Generous for real documents, tight enough to stop bombs.
#[derive(Debug, Clone, Copy)]
pub struct ZipLimits {
    pub max_entries: usize,
    pub max_entry_size: u64,
    pub max_total_size: u64,
    pub max_ratio: u64,
}

impl Default for ZipLimits {
    fn default() -> Self {
        Self {
            max_entries: 8_192,
            max_entry_size: 256 * 1024 * 1024,
            max_total_size: 1024 * 1024 * 1024,
            max_ratio: 400,
        }
    }
}

struct CentralEntry {
    name: String,
    method: u16,
    crc: u32,
    compressed_size: u64,
    uncompressed_size: u64,
    local_offset: u64,
    flags: u16,
}

pub struct ZipReader {
    data: Vec<u8>,
    entries: Vec<CentralEntry>,
    index: HashMap<String, usize>,
}

fn rd_u16(data: &[u8], at: usize) -> OfficeResult<u16> {
    data.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(|| OfficeError::corrupt("Truncated ZIP structure"))
}

fn rd_u32(data: &[u8], at: usize) -> OfficeResult<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| OfficeError::corrupt("Truncated ZIP structure"))
}

/// Reads one entry name. Only the `len` bytes of the name are touched: the
/// previous version copied everything from `at` to the end of the package for
/// every entry, which made opening a 20 MB / 100 entry archive take 13 seconds
/// (and an 8192 entry one minutes) for no reason at all.
pub fn read_entry_name(data: &[u8], at: usize, len: usize) -> String {
    let end = at.saturating_add(len);
    String::from_utf8_lossy(data.get(at..end).unwrap_or_default()).into_owned()
}

impl ZipReader {
    pub fn open(bytes: Vec<u8>) -> OfficeResult<Self> {
        Self::open_with_limits(bytes, ZipLimits::default())
    }

    pub fn open_with_limits(data: Vec<u8>, limits: ZipLimits) -> OfficeResult<Self> {
        let eocd_at = find_eocd(&data).ok_or_else(|| OfficeError::corrupt("Not a ZIP package (no end-of-central-directory record)"))?;
        let count = rd_u16(&data, eocd_at + 10)? as usize;
        let central_offset = rd_u32(&data, eocd_at + 16)? as u64;
        if count > limits.max_entries {
            return Err(OfficeError::new(ErrorCode::ZipBomb, "The package contains too many entries."));
        }
        let mut entries = Vec::with_capacity(count);
        let mut cursor = central_offset as usize;
        for _ in 0..count {
            if rd_u32(&data, cursor)? != CENTRAL_SIG {
                return Err(OfficeError::corrupt("Damaged ZIP central directory"));
            }
            let flags = rd_u16(&data, cursor + 8)?;
            let method = rd_u16(&data, cursor + 10)?;
            let crc = rd_u32(&data, cursor + 16)?;
            let compressed_size = rd_u32(&data, cursor + 20)? as u64;
            let uncompressed_size = rd_u32(&data, cursor + 24)? as u64;
            let name_len = rd_u16(&data, cursor + 28)? as usize;
            let extra_len = rd_u16(&data, cursor + 30)? as usize;
            let comment_len = rd_u16(&data, cursor + 32)? as usize;
            let local_offset = rd_u32(&data, cursor + 42)? as u64;
            let name = read_entry_name(&data, cursor + 46, name_len);
            cursor = cursor.saturating_add(46 + name_len + extra_len + comment_len);
            entries.push(CentralEntry { name, method, crc, compressed_size, uncompressed_size, local_offset, flags });
        }
        let mut index = HashMap::new();
        for (position, entry) in entries.iter().enumerate() {
            // A duplicate name used to be resolved by overwriting the index, so
            // the *last* entry silently won. Two entries with one name is
            // exactly the shape an archive-confusion attack needs, so refuse it.
            if index.insert(entry.name.clone(), position).is_some() {
                return Err(OfficeError::corrupt(format!("The package contains the entry {} twice.", entry.name)));
            }
        }
        Ok(ZipReader { data, entries, index })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.name.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    /// Reads a single entry; enforces per-entry and ratio limits.
    pub fn read(&self, name: &str) -> OfficeResult<Vec<u8>> {
        self.read_with_limits(name, ZipLimits::default())
    }

    pub fn read_with_limits(&self, name: &str, limits: ZipLimits) -> OfficeResult<Vec<u8>> {
        let entry = self
            .index
            .get(name)
            .map(|position| &self.entries[*position])
            .ok_or_else(|| OfficeError::new(ErrorCode::NotFound, format!("Package entry not found: {name}")))?;
        self.read_entry(entry, limits)
    }

    fn read_entry(&self, entry: &CentralEntry, limits: ZipLimits) -> OfficeResult<Vec<u8>> {
        // The declared uncompressed size comes from whoever built the archive,
        // so it is a hint and never a guarantee: the published bomb declares a
        // kilobyte and inflates to hundreds of megabytes while the old check -
        // which ran *after* `read_to_end` had materialised everything - happily
        // blew the memory budget first. The cap is applied to the stream now.
        let cap = limits.max_entry_size;
        let at = entry.local_offset as usize;
        if rd_u32(&self.data, at)? != LOCAL_SIG {
            return Err(OfficeError::corrupt("Damaged ZIP local header"));
        }
        let name_len = rd_u16(&self.data, at + 26)? as usize;
        let extra_len = rd_u16(&self.data, at + 28)? as usize;
        let start = at + 30 + name_len + extra_len;
        let end = start
            .checked_add(entry.compressed_size as usize)
            .ok_or_else(|| OfficeError::corrupt("ZIP entry offset overflow"))?;
        let raw = self.data.get(start..end).ok_or_else(|| OfficeError::corrupt("Truncated ZIP entry data"))?;
        let out = match entry.method {
            0 => {
                if raw.len() as u64 > cap {
                    return Err(OfficeError::new(ErrorCode::ZipBomb, format!("Entry {} is larger than the safe limit.", entry.name)));
                }
                raw.to_vec()
            }
            8 => {
                let mut decoder = DeflateDecoder::new(raw);
                let mut out = Vec::with_capacity(entry.uncompressed_size.min(1024 * 1024) as usize);
                // One byte past the cap is enough to prove the stream keeps
                // expanding, and only that byte is ever materialised.
                let mut limited = decoder.by_ref().take(cap.saturating_add(1));
                limited
                    .read_to_end(&mut out)
                    .map_err(|error| OfficeError::corrupt(format!("Could not decompress {}: {error}", entry.name)))?;
                if out.len() as u64 > cap {
                    return Err(OfficeError::new(ErrorCode::ZipBomb, format!("Entry {} expands beyond the safe limit.", entry.name)));
                }
                out
            }
            other => {
                return Err(OfficeError::unsupported(format!("Unsupported ZIP compression method {other}")));
            }
        };
        if !raw.is_empty() && out.len() as u64 > raw.len() as u64 * limits.max_ratio.max(1) && out.len() as u64 > 1024 * 1024 {
            return Err(OfficeError::new(ErrorCode::ZipBomb, format!("Suspicious compression ratio in {}.", entry.name)));
        }
        // The CRC is the only integrity check the container offers. Skipping it
        // meant a corrupted part decoded into plausible-looking text and the
        // user only noticed when the numbers no longer added up.
        let actual = crc32(&out);
        if actual != entry.crc {
            return Err(OfficeError::corrupt(format!(
                "Entry {} failed its CRC check (stored {:08x}, computed {:08x}).",
                entry.name, entry.crc, actual
            )));
        }
        let _ = entry.flags;
        Ok(out)
    }

    /// Reads all entries, enforcing the total size limit.
    pub fn read_all(&self, limits: ZipLimits) -> OfficeResult<Vec<(String, Vec<u8>)>> {
        let mut out = Vec::with_capacity(self.entries.len());
        let mut total = 0u64;
        for entry in &self.entries {
            // Never let one entry inflate past what is left of the whole-package
            // budget: the total was only checked after the fact before.
            let remaining = limits.max_total_size.saturating_sub(total);
            let entry_limits = ZipLimits { max_entry_size: limits.max_entry_size.min(remaining), ..limits };
            let data = self.read_entry(entry, entry_limits)?;
            total += data.len() as u64;
            if total > limits.max_total_size {
                return Err(OfficeError::new(ErrorCode::ZipBomb, "The package expands beyond the safe total size."));
            }
            out.push((entry.name.clone(), data));
        }
        Ok(out)
    }

    pub fn read_text(&self, name: &str) -> OfficeResult<String> {
        let bytes = self.read(name)?;
        decode_utf8(&bytes, name)
    }
}

pub fn decode_utf8(bytes: &[u8], name: &str) -> OfficeResult<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        // Strip a UTF-8 BOM when present.
        return Ok(text.strip_prefix('\u{feff}').unwrap_or(text).to_string());
    }
    // Legacy 8-bit text: decode as Windows-1254/1252 (see `encoding`) so old
    // documents still open instead of failing outright. Casting each byte to a
    // `char` used to mangle Turkish letters and the C1 punctuation.
    let _ = name;
    Ok(crate::encoding::decode_legacy(bytes))
}

fn find_eocd(data: &[u8]) -> Option<usize> {
    if data.len() < 22 {
        return None;
    }
    let start = data.len().saturating_sub(22 + 65_535);
    let mut at = data.len() - 22;
    loop {
        if rd_u32(data, at).ok()? == EOCD_SIG {
            return Some(at);
        }
        if at == start {
            return None;
        }
        at -= 1;
    }
}

struct WriteEntry {
    name: String,
    crc: u32,
    uncompressed_size: u32,
    method: u16,
    data: Vec<u8>,
}

/// Streaming ZIP writer. Entries are compressed as they are added.
pub struct ZipWriter {
    entries: Vec<WriteEntry>,
}

impl Default for ZipWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ZipWriter {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn add(&mut self, name: &str, data: &[u8]) {
        let crc = crc32(data);
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(6));
        let compressed = encoder.write_all(data).and_then(|_| encoder.finish()).unwrap_or_default();
        let (method, payload) = if compressed.len() < data.len() && data.len() > 64 {
            (8u16, compressed)
        } else {
            (0u16, data.to_vec())
        };
        self.entries.push(WriteEntry {
            name: name.to_string(),
            crc,
            uncompressed_size: data.len() as u32,
            method,
            data: payload,
        });
    }

    pub fn add_text(&mut self, name: &str, text: &str) {
        self.add(name, text.as_bytes());
    }

    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        let (dos_time, dos_date) = dos_now();
        for entry in &self.entries {
            let offset = out.len() as u32;
            write_u32(&mut out, LOCAL_SIG);
            write_u16(&mut out, 20);
            write_u16(&mut out, 0x0800); // UTF-8 names
            write_u16(&mut out, entry.method);
            write_u16(&mut out, dos_time);
            write_u16(&mut out, dos_date);
            write_u32(&mut out, entry.crc);
            write_u32(&mut out, entry.data.len() as u32);
            write_u32(&mut out, entry.uncompressed_size);
            write_u16(&mut out, entry.name.len() as u16);
            write_u16(&mut out, 0);
            out.extend_from_slice(entry.name.as_bytes());
            out.extend_from_slice(&entry.data);

            write_u32(&mut central, CENTRAL_SIG);
            write_u16(&mut central, 20);
            write_u16(&mut central, 20);
            write_u16(&mut central, 0x0800);
            write_u16(&mut central, entry.method);
            write_u16(&mut central, dos_time);
            write_u16(&mut central, dos_date);
            write_u32(&mut central, entry.crc);
            write_u32(&mut central, entry.data.len() as u32);
            write_u32(&mut central, entry.uncompressed_size);
            write_u16(&mut central, entry.name.len() as u16);
            write_u16(&mut central, 0);
            write_u16(&mut central, 0);
            write_u16(&mut central, 0);
            write_u16(&mut central, 0);
            write_u32(&mut central, 0);
            write_u32(&mut central, offset);
            central.extend_from_slice(entry.name.as_bytes());
        }
        let central_offset = out.len() as u32;
        let central_size = central.len() as u32;
        out.extend_from_slice(&central);
        write_u32(&mut out, EOCD_SIG);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        write_u16(&mut out, self.entries.len() as u16);
        write_u16(&mut out, self.entries.len() as u16);
        write_u32(&mut out, central_size);
        write_u32(&mut out, central_offset);
        write_u16(&mut out, 0);
        out
    }
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

fn dos_now() -> (u16, u16) {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    dos_from_unix(secs)
}

/// Packs a Unix timestamp into the MS-DOS date/time pair ZIP stores.
///
/// The previous version was wrong twice over: it added the 719_468 day epoch
/// offset once in the caller and again inside `civil_from_days`, and it stored
/// `seconds_since_midnight / 2` as the raw time word instead of packing hours,
/// minutes and seconds into their bit fields. A file written on 2026-09-29 was
/// stamped 2076-07-31, with a nonsense clock as well.
fn dos_from_unix(secs: u64) -> (u16, u16) {
    let seconds_of_day = secs % 86_400;
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let time = (((seconds_of_day / 3_600) as u16) << 11)
        | ((((seconds_of_day % 3_600) / 60) as u16) << 5)
        | (((seconds_of_day % 60) / 2) as u16);
    let date = (((year - 1980).clamp(0, 127) as u16) << 9) | ((month as u16) << 5) | day as u16;
    (time, date)
}

/// Inverse of `dos_from_unix`: (year, month, day, hour, minute, second).
/// Only the tests read a stored timestamp back; nothing in the app does yet.
#[cfg(test)]
fn dos_to_parts(time: u16, date: u16) -> (i64, u32, u32, u32, u32, u32) {
    let year = 1980 + ((date >> 9) & 0x7f) as i64;
    let month = ((date >> 5) & 0x0f) as u32;
    let day = (date & 0x1f) as u32;
    let hour = ((time >> 11) & 0x1f) as u32;
    let minute = ((time >> 5) & 0x3f) as u32;
    let second = ((time & 0x1f) * 2) as u32;
    (year, month, day, hour, minute, second)
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Instant, SystemTime, UNIX_EPOCH};

    #[test]
    fn roundtrip_zip() {
        let mut writer = ZipWriter::new();
        writer.add_text("hello.txt", "Hello, world!");
        writer.add("data.bin", &vec![7u8; 10_000]);
        let bytes = writer.finish();
        let reader = ZipReader::open(bytes).unwrap();
        assert!(reader.contains("hello.txt"));
        assert_eq!(reader.read_text("hello.txt").unwrap(), "Hello, world!");
        assert_eq!(reader.read("data.bin").unwrap().len(), 10_000);
        assert_eq!(reader.names().count(), 2);
    }

    #[test]
    fn rejects_non_zip() {
        assert!(ZipReader::open(b"not a zip file at all".to_vec()).is_err());
    }

    #[test]
    fn rejects_high_ratio_bomb() {
        let mut writer = ZipWriter::new();
        writer.add("bomb.xml", &vec![0u8; 4 * 1024 * 1024]);
        let bytes = writer.finish();
        let reader = ZipReader::open(bytes).unwrap();
        let limits = ZipLimits { max_ratio: 5, max_entry_size: 1024 * 1024, ..ZipLimits::default() };
        assert!(reader.read_with_limits("bomb.xml", limits).is_err());
    }

    /// Rewrites the declared uncompressed size in the local header and in the
    /// central directory, the way a crafted archive would.
    fn declare_uncompressed_size(bytes: &mut [u8], value: u32) {
        bytes[22..26].copy_from_slice(&value.to_le_bytes());
        let eocd = bytes.windows(4).rposition(|window| window == EOCD_SIG.to_le_bytes()).expect("eocd");
        let central = u32::from_le_bytes([bytes[eocd + 16], bytes[eocd + 17], bytes[eocd + 18], bytes[eocd + 19]]) as usize;
        bytes[central + 24..central + 28].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn a_lying_header_cannot_smuggle_a_bomb() {
        let mut writer = ZipWriter::new();
        // 32 MB of zeros compress to a few kilobytes: 596 KB is enough to
        // allocate hundreds of megabytes when the limit is only checked after
        // the whole stream has been inflated.
        writer.add("bomb.xml", &vec![0u8; 32 * 1024 * 1024]);
        let mut bytes = writer.finish();
        assert!(bytes.len() < 128 * 1024, "the bomb should stay small on disk, got {}", bytes.len());
        declare_uncompressed_size(&mut bytes, 1024);

        let reader = match ZipReader::open(bytes) {
            Ok(reader) => reader,
            Err(error) => panic!("the header lie must not stop the open: {error}"),
        };
        let limits = ZipLimits { max_entry_size: 1024 * 1024, max_total_size: 4 * 1024 * 1024, ..ZipLimits::default() };
        let error = match reader.read_with_limits("bomb.xml", limits) {
            Err(error) => error,
            Ok(_) => panic!("the bomb must be refused"),
        };
        assert_eq!(error.code, "zip_bomb", "unexpected error: {error}");
    }

    #[test]
    fn a_corrupted_entry_is_rejected_by_its_crc() {
        let mut writer = ZipWriter::new();
        writer.add("a.txt", b"hello world");
        let mut bytes = writer.finish();
        // Short data stays stored, so the payload sits right behind the 30 byte
        // local header plus the name.
        let payload_at = 30 + "a.txt".len();
        bytes[payload_at] ^= 0x20;

        let reader = ZipReader::open(bytes).unwrap();
        let error = match reader.read("a.txt") {
            Err(error) => error,
            Ok(_) => panic!("a corrupted entry must not decode"),
        };
        assert_eq!(error.code, "corrupt_document", "unexpected error: {error}");
        assert!(error.message.contains("CRC"), "unexpected message: {}", error.message);
    }

    #[test]
    fn duplicate_entry_names_are_rejected() {
        let mut writer = ZipWriter::new();
        writer.add("dup.txt", b"first");
        writer.add("dup.txt", b"second");
        let error = match ZipReader::open(writer.finish()) {
            Err(error) => error,
            Ok(_) => panic!("duplicate names must be refused"),
        };
        assert!(error.message.contains("twice"), "unexpected message: {}", error.message);
    }

    #[test]
    fn entry_names_are_read_without_the_rest_of_the_package() {
        // The old reader pushed the whole remaining buffer through
        // String::from_utf8_lossy and only then sliced the name out of it,
        // which copied the tail once per entry and sliced a lossy string by a
        // byte length (an invalid byte turns into three bytes of replacement
        // character, so the name came back empty).
        let mut data = vec![0xffu8; 4 * 1024 * 1024];
        data[0] = 0xE4;
        data[1] = b'.';
        let name = read_entry_name(&data, 0, 2);
        assert_eq!(name.chars().count(), 2, "name: {name:?}");
        assert!(name.ends_with('.'), "name: {name:?}");
    }

    #[test]
    fn dos_timestamps_are_calendar_dates() {
        // 2026-09-29T12:34:56Z. The old code applied the 719_468 day epoch
        // offset twice and stored seconds/2 as the raw time word: this instant
        // came out as 2076-07-31.
        let (time, date) = dos_from_unix(1_790_685_296);
        assert_eq!(dos_to_parts(time, date), (2026, 9, 29, 12, 34, 56));
    }

    #[test]
    fn written_archives_carry_todays_date() {
        let mut writer = ZipWriter::new();
        writer.add_text("a.txt", "a");
        let bytes = writer.finish();
        let time = u16::from_le_bytes([bytes[10], bytes[11]]);
        let date = u16::from_le_bytes([bytes[12], bytes[13]]);
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let (year, month, day) = civil_from_days((secs / 86_400) as i64);
        let (stored_year, stored_month, stored_day, ..) = dos_to_parts(time, date);
        assert_eq!((stored_year, stored_month, stored_day), (year, month, day));
    }

    #[test]
    fn opening_a_large_archive_stays_fast() {
        // Regression guard, not a benchmark: reading the central directory used
        // to copy everything from each entry to the end of the package. The
        // budget is loose on purpose, the old shape cannot make it.
        let mut payload = vec![0u8; 16 * 1024 * 1024];
        let mut state = 0x2545_f491_4f6c_dd1du64;
        for byte in payload.iter_mut() {
            // xorshift64: incompressible, so the archive really is ~16 MB.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = (state >> 24) as u8;
        }
        let mut writer = ZipWriter::new();
        writer.add("word/document.xml", &payload);
        for index in 0..400 {
            writer.add(&format!("word/part{index}.xml"), b"<x/>");
        }
        let bytes = writer.finish();
        assert!(bytes.len() > 16 * 1024 * 1024, "archive should be large, got {}", bytes.len());

        let started = Instant::now();
        let reader = ZipReader::open(bytes).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(reader.names().count(), 401);
        assert!(elapsed.as_secs_f64() < 2.0, "opening the central directory took {elapsed:?}");
    }
}

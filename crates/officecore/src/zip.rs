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

pub fn read_entry_name(data: &[u8], at: usize) -> String {
    String::from_utf8_lossy(data.get(at..).unwrap_or_default()).into_owned()
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
            let compressed_size = rd_u32(&data, cursor + 20)? as u64;
            let uncompressed_size = rd_u32(&data, cursor + 24)? as u64;
            let name_len = rd_u16(&data, cursor + 28)? as usize;
            let extra_len = rd_u16(&data, cursor + 30)? as usize;
            let comment_len = rd_u16(&data, cursor + 32)? as usize;
            let local_offset = rd_u32(&data, cursor + 42)? as u64;
            let name = read_entry_name(&data, cursor + 46).get(..name_len).map(str::to_string).unwrap_or_default();
            cursor += 46 + name_len + extra_len + comment_len;
            entries.push(CentralEntry { name, method, compressed_size, uncompressed_size, local_offset, flags });
        }
        let mut index = HashMap::new();
        for (position, entry) in entries.iter().enumerate() {
            index.insert(entry.name.clone(), position);
        }
        let reader = ZipReader { data, entries, index };
        let _ = limits;
        Ok(reader)
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
        if entry.uncompressed_size > limits.max_entry_size {
            return Err(OfficeError::new(ErrorCode::ZipBomb, format!("Entry {} is too large to open safely.", entry.name)));
        }
        let at = entry.local_offset as usize;
        if rd_u32(&self.data, at)? != LOCAL_SIG {
            return Err(OfficeError::corrupt("Damaged ZIP local header"));
        }
        let name_len = rd_u16(&self.data, at + 26)? as usize;
        let extra_len = rd_u16(&self.data, at + 28)? as usize;
        let start = at + 30 + name_len + extra_len;
        let end = start + entry.compressed_size as usize;
        let raw = self
            .data
            .get(start..end)
            .ok_or_else(|| OfficeError::corrupt("Truncated ZIP entry data"))?;
        let out = match entry.method {
            0 => raw.to_vec(),
            8 => {
                let mut decoder = DeflateDecoder::new(raw);
                let mut out = Vec::with_capacity(entry.uncompressed_size.min(8 * 1024 * 1024) as usize);
                decoder
                    .read_to_end(&mut out)
                    .map_err(|error| OfficeError::corrupt(format!("Could not decompress {}: {error}", entry.name)))?;
                out
            }
            other => {
                return Err(OfficeError::unsupported(format!("Unsupported ZIP compression method {other}")));
            }
        };
        if out.len() as u64 > limits.max_entry_size {
            return Err(OfficeError::new(ErrorCode::ZipBomb, format!("Entry {} expands beyond the safe limit.", entry.name)));
        }
        if !raw.is_empty() && out.len() as u64 > raw.len() as u64 * limits.max_ratio.max(1) && out.len() as u64 > 1024 * 1024 {
            return Err(OfficeError::new(ErrorCode::ZipBomb, format!("Suspicious compression ratio in {}.", entry.name)));
        }
        let _ = entry.flags;
        Ok(out)
    }

    /// Reads all entries, enforcing the total size limit.
    pub fn read_all(&self, limits: ZipLimits) -> OfficeResult<Vec<(String, Vec<u8>)>> {
        let mut out = Vec::with_capacity(self.entries.len());
        let mut total = 0u64;
        for entry in &self.entries {
            let data = self.read_entry(entry, limits)?;
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
    // Legacy 8-bit text: decode as Windows-1252 / Latin-1 so old documents
    // still open instead of failing outright.
    let _ = name;
    Ok(bytes.iter().map(|&byte| byte as char).collect())
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
    let days = secs / 86_400;
    let time = ((secs % 86_400) / 2) as u16;
    let (year, month, day) = civil_from_days(days as i64 + 719_468);
    let dos_date = (((year - 1980).max(0) as u16) << 9) | ((month as u16) << 5) | day as u16;
    (time, dos_date)
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
}

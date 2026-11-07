//! The OLE2/CFB container that carries `.xls` and `.xlsb` is parsed from
//! attacker-supplied bytes. RUSTSEC-2026-0436 (calamine 0.37, fixed in the
//! 0.38 the workspace vendors) was a panic and an unbounded allocation in that
//! parser; these cases pin the behaviour the fix has to keep: every malformed
//! container comes back as an error, promptly and with bounded memory.
//!
//! A regression here does not fail politely: the old code exhausted memory or
//! panicked, so the test binary itself is the signal.

use officecore::xlsx::read_workbook_bytes;

/// The eight magic bytes every OLE2 compound file starts with.
const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// A 512-byte CFB header with the fields a hostile file would lie about.
fn header(fat_sectors: u32, difat_sectors: u32, mini_cutoff: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; 512];
    bytes[0..8].copy_from_slice(&MAGIC);
    // Minor and major version, little-endian.
    bytes[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    bytes[26..28].copy_from_slice(&0x0003u16.to_le_bytes());
    // Byte order mark (0xFFFE = little-endian) and the sector shift (9 = 512).
    bytes[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    bytes[30..32].copy_from_slice(&9u16.to_le_bytes());
    bytes[32..34].copy_from_slice(&6u16.to_le_bytes());
    // Number of directory sectors (0 in version 3).
    bytes[40..44].copy_from_slice(&0u32.to_le_bytes());
    // The sizes a crafted file inflates.
    bytes[44..48].copy_from_slice(&fat_sectors.to_le_bytes());
    bytes[48..52].copy_from_slice(&1u32.to_le_bytes()); // first directory sector
    bytes[56..60].copy_from_slice(&mini_cutoff.to_le_bytes());
    bytes[60..64].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes()); // first mini FAT
    bytes[68..72].copy_from_slice(&0xFFFF_FFFEu32.to_le_bytes()); // first DIFAT
    bytes[72..76].copy_from_slice(&difat_sectors.to_le_bytes());
    // Every DIFAT entry is "free", so nothing points at a real sector.
    for index in 0..109 {
        let at = 76 + index * 4;
        bytes[at..at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    }
    bytes
}

fn rejects(bytes: Vec<u8>, what: &str) {
    let result = read_workbook_bytes(&bytes);
    assert!(result.is_err(), "{what} was accepted");
}

#[test]
fn a_truncated_container_is_an_error() {
    rejects(MAGIC.to_vec(), "the magic alone");
    let mut short = header(0, 0, 4096);
    short.truncate(256);
    rejects(short, "half a header");
}

#[test]
fn a_header_that_claims_endless_sectors_is_an_error() {
    // The historical denial of service: a FAT count far beyond the file size
    // made the parser allocate for sectors that were never there.
    rejects(header(u32::MAX, 0, 4096), "u32::MAX FAT sectors");
    rejects(header(0x00FF_FFFF, 0x00FF_FFFF, 4096), "endless DIFAT sectors");
    rejects(header(1, 0, u32::MAX), "an endless mini-stream cutoff");
}

#[test]
fn a_header_without_a_sector_behind_it_is_an_error() {
    // A well-shaped header followed by nothing: the directory sector it points
    // at does not exist.
    rejects(header(1, 0, 4096), "no sectors behind the header");
    let mut zero_filled = header(1, 0, 4096);
    zero_filled.extend(std::iter::repeat_n(0u8, 512));
    rejects(zero_filled, "a zero-filled first sector");
}

#[test]
fn random_bytes_behind_the_magic_are_an_error() {
    // Deterministic pseudo-random body: a crash would have to come from the
    // parser, not from the test's own input generator.
    let mut state = 0x2545_F491u32;
    for length in [512usize, 4096, 65_536] {
        let mut bytes = header(1, 0, 4096);
        bytes.truncate(512);
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            bytes.push((state & 0xFF) as u8);
        }
        rejects(bytes, "a random sector body");
    }
}

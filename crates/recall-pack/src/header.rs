use crate::error::{PackError, PackResult};

/// Magic bytes: `VRCALL01`
pub const MAGIC: [u8; 8] = *b"VRCALL01";
pub const FORMAT_VERSION: u32 = 1;
pub const HEADER_SIZE: usize = 64;

/// Fixed 64-byte pack header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackHeader {
    pub format_version: u32,
    pub flags: u32,
    pub section_count: u32,
    pub content_hash: [u8; 32],
}

impl PackHeader {
    pub fn new(section_count: u32) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            flags: 0,
            section_count,
            content_hash: [0u8; 32],
        }
    }

    pub fn parse(bytes: &[u8]) -> PackResult<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err(PackError::TooShort {
                need: HEADER_SIZE,
                got: bytes.len(),
            });
        }
        if bytes[0..8] != MAGIC {
            return Err(PackError::BadMagic);
        }
        let format_version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        if format_version != FORMAT_VERSION {
            return Err(PackError::UnsupportedVersion(format_version));
        }
        let flags = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        // Version 1 reserves every flag. In particular, do not accept a pack
        // that claims an integrity hash until the reader can verify it.
        if flags != 0 {
            return Err(PackError::UnsupportedFlags(flags));
        }
        let section_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        // Version 1 requires both reserved header regions to be all zero. A
        // non-zero value may acquire meaning in a future format and must not
        // be silently interpreted as v1 today.
        if bytes[20..24] != [0u8; 4] || bytes[56..64] != [0u8; 8] {
            return Err(PackError::BadSectionTable);
        }
        let mut content_hash = [0u8; 32];
        content_hash.copy_from_slice(&bytes[24..56]);
        if content_hash != [0u8; 32] {
            return Err(PackError::UnsupportedFlags(flags | 1));
        }
        Ok(Self {
            format_version,
            flags,
            section_count,
            content_hash,
        })
    }

    pub fn write_to(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&self.format_version.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&self.section_count.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes()); // reserved0
        out.extend_from_slice(&self.content_hash);
        out.extend_from_slice(&0u64.to_le_bytes()); // reserved1
        debug_assert_eq!(out.len() % HEADER_SIZE, 0);
    }
}

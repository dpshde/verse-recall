use crate::error::{PackError, PackResult};

pub const SECTION_ENTRY_SIZE: usize = 32;

/// Known section tags (ASCII, NUL-padded to 8 bytes on disk).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SectionTag {
    Meta,
    Text,
    Lex,
    Post,
    /// Sparse dictionary for verse-aligned, non-canonical context documents.
    Clex,
    /// Sparse postings for verse-aligned, non-canonical context documents.
    Cpost,
    Expand,
    Wvec,
    Lvec,
    /// Passage-level semantic vectors with explicit canonical ranges.
    Pvec,
    Phrase,
    Prior,
}

impl SectionTag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Meta => "META",
            Self::Text => "TEXT",
            Self::Lex => "LEX",
            Self::Post => "POST",
            Self::Clex => "CLEX",
            Self::Cpost => "CPOST",
            Self::Expand => "EXPAND",
            Self::Wvec => "WVEC",
            Self::Lvec => "LVEC",
            Self::Pvec => "PVEC",
            Self::Phrase => "PHRASE",
            Self::Prior => "PRIOR",
        }
    }

    pub fn disk_bytes(self) -> [u8; 8] {
        let s = self.as_str().as_bytes();
        let mut b = [0u8; 8];
        b[..s.len()].copy_from_slice(s);
        b
    }

    pub fn from_disk(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }
        let end = bytes[..8].iter().position(|&c| c == 0).unwrap_or(8);
        if bytes[end..8].iter().any(|&byte| byte != 0) {
            return None;
        }
        match std::str::from_utf8(&bytes[..end]).ok()? {
            "META" => Some(Self::Meta),
            "TEXT" => Some(Self::Text),
            "LEX" => Some(Self::Lex),
            "POST" => Some(Self::Post),
            "CLEX" => Some(Self::Clex),
            "CPOST" => Some(Self::Cpost),
            "EXPAND" => Some(Self::Expand),
            "WVEC" => Some(Self::Wvec),
            "LVEC" => Some(Self::Lvec),
            "PVEC" => Some(Self::Pvec),
            "PHRASE" => Some(Self::Phrase),
            "PRIOR" => Some(Self::Prior),
            _ => None,
        }
    }
}

/// One row in the section table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionEntry {
    pub tag: SectionTag,
    pub offset: u64,
    pub len: u64,
    pub flags: u32,
}

impl SectionEntry {
    pub fn parse(bytes: &[u8]) -> PackResult<Self> {
        if bytes.len() < SECTION_ENTRY_SIZE {
            return Err(PackError::BadSectionTable);
        }
        let tag = SectionTag::from_disk(&bytes[0..8]).ok_or(PackError::BadSectionTable)?;
        let offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        let len = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let flags = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        if bytes[28..32] != [0u8; 4] {
            return Err(PackError::BadSectionTable);
        }
        Ok(Self {
            tag,
            offset,
            len,
            flags,
        })
    }

    pub fn write_to(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.tag.disk_bytes());
        out.extend_from_slice(&self.offset.to_le_bytes());
        out.extend_from_slice(&self.len.to_le_bytes());
        out.extend_from_slice(&self.flags.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
    }
}

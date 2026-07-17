use crate::error::{PackError, PackResult};
use std::collections::HashSet;

/// One verse view into the TEXT section (owned for scaffold simplicity).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verse {
    pub index: u32,
    pub reference: String,
    pub text: String,
}

/// Parsed TEXT section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSection {
    pub verses: Vec<Verse>,
}

impl TextSection {
    pub fn from_verses(verses: Vec<(String, String)>) -> Self {
        let verses = verses
            .into_iter()
            .enumerate()
            .map(|(i, (reference, text))| Verse {
                index: i as u32,
                reference,
                text,
            })
            .collect();
        Self { verses }
    }

    pub fn parse(bytes: &[u8]) -> PackResult<Self> {
        if bytes.len() < 4 {
            return Err(PackError::InvalidText);
        }
        let verse_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let table_bytes = verse_count.checked_mul(16).ok_or(PackError::Overflow)?;
        let table_end = 4usize.checked_add(table_bytes).ok_or(PackError::Overflow)?;
        if bytes.len() < table_end + 4 {
            return Err(PackError::InvalidText);
        }
        let blob_len =
            u32::from_le_bytes(bytes[table_end..table_end + 4].try_into().unwrap()) as usize;
        let blob_start = table_end + 4;
        let blob_end = blob_start
            .checked_add(blob_len)
            .ok_or(PackError::Overflow)?;
        if bytes.len() < blob_end {
            return Err(PackError::InvalidText);
        }
        let blob = &bytes[blob_start..blob_end];

        let mut verses = Vec::with_capacity(verse_count);
        let mut seen_refs = HashSet::with_capacity(verse_count);
        for i in 0..verse_count {
            let base = 4 + i * 16;
            let ref_off = u32::from_le_bytes(bytes[base..base + 4].try_into().unwrap()) as usize;
            let ref_len =
                u32::from_le_bytes(bytes[base + 4..base + 8].try_into().unwrap()) as usize;
            let text_off =
                u32::from_le_bytes(bytes[base + 8..base + 12].try_into().unwrap()) as usize;
            let text_len =
                u32::from_le_bytes(bytes[base + 12..base + 16].try_into().unwrap()) as usize;

            let reference = slice_utf8(blob, ref_off, ref_len, "TEXT.ref")?;
            let text = slice_utf8(blob, text_off, text_len, "TEXT.text")?;
            let normalized_ref = normalized_ref_key(reference);
            if normalized_ref.is_empty()
                || text.trim().is_empty()
                || !seen_refs.insert(normalized_ref)
            {
                return Err(PackError::InvalidText);
            }
            verses.push(Verse {
                index: i as u32,
                reference: reference.to_string(),
                text: text.to_string(),
            });
        }

        Ok(Self { verses })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut blob = Vec::new();
        let mut table: Vec<(u32, u32, u32, u32)> = Vec::with_capacity(self.verses.len());

        for v in &self.verses {
            let ref_off = blob.len() as u32;
            blob.extend_from_slice(v.reference.as_bytes());
            let ref_len = v.reference.len() as u32;
            let text_off = blob.len() as u32;
            blob.extend_from_slice(v.text.as_bytes());
            let text_len = v.text.len() as u32;
            table.push((ref_off, ref_len, text_off, text_len));
        }

        let mut out = Vec::with_capacity(4 + table.len() * 16 + 4 + blob.len());
        out.extend_from_slice(&(self.verses.len() as u32).to_le_bytes());
        for (ro, rl, to, tl) in table {
            out.extend_from_slice(&ro.to_le_bytes());
            out.extend_from_slice(&rl.to_le_bytes());
            out.extend_from_slice(&to.to_le_bytes());
            out.extend_from_slice(&tl.to_le_bytes());
        }
        out.extend_from_slice(&(blob.len() as u32).to_le_bytes());
        out.extend_from_slice(&blob);
        out
    }
}

fn normalized_ref_key(reference: &str) -> String {
    let mut tokens = Vec::new();
    let mut token = String::new();
    for character in reference.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            token.push(character);
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens.join(" ")
}

fn slice_utf8<'a>(
    blob: &'a [u8],
    off: usize,
    len: usize,
    context: &'static str,
) -> PackResult<&'a str> {
    let end = off.checked_add(len).ok_or(PackError::Overflow)?;
    if end > blob.len() {
        return Err(PackError::InvalidText);
    }
    std::str::from_utf8(&blob[off..end]).map_err(|_| PackError::InvalidUtf8 { context })
}

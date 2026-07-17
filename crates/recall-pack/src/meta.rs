use crate::error::{PackError, PackResult};

/// Parsed META section.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaSection {
    pub meta_version: u32,
    pub verse_count: u32,
    pub translation_id: String,
    pub weight_a: f32,
    pub weight_b: f32,
    pub weight_c: f32,
    pub weight_d: f32,
}

impl MetaSection {
    pub fn new(translation_id: impl Into<String>, verse_count: u32) -> Self {
        Self {
            meta_version: 1,
            verse_count,
            translation_id: translation_id.into(),
            weight_a: 0.0,
            weight_b: 0.0,
            weight_c: 0.0,
            weight_d: 0.0,
        }
    }

    pub fn parse(bytes: &[u8]) -> PackResult<Self> {
        if bytes.len() < 10 {
            return Err(PackError::InvalidMeta);
        }
        let meta_version = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if meta_version != 1 {
            return Err(PackError::UnsupportedMetaVersion(meta_version));
        }
        let verse_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let id_len = u16::from_le_bytes(bytes[8..10].try_into().unwrap()) as usize;
        if bytes.len() < 10 + id_len {
            return Err(PackError::InvalidMeta);
        }
        let translation_id = std::str::from_utf8(&bytes[10..10 + id_len])
            .map_err(|_| PackError::InvalidUtf8 {
                context: "META.translation_id",
            })?
            .to_string();
        if translation_id.trim().is_empty() {
            return Err(PackError::InvalidMeta);
        }

        // Optional weights after id, padded to 4-byte alignment then 4×f32.
        let after_id = 10 + id_len;
        let pad = (4 - (after_id % 4)) % 4;
        let w_off = after_id + pad;
        let has_weights = match bytes.len() {
            len if len == after_id => false,
            len if len == w_off + 16 => true,
            _ => return Err(PackError::InvalidMeta),
        };
        if has_weights && bytes[after_id..w_off].iter().any(|byte| *byte != 0) {
            return Err(PackError::InvalidMeta);
        }
        let (weight_a, weight_b, weight_c, weight_d) = if has_weights {
            (
                f32::from_le_bytes(bytes[w_off..w_off + 4].try_into().unwrap()),
                f32::from_le_bytes(bytes[w_off + 4..w_off + 8].try_into().unwrap()),
                f32::from_le_bytes(bytes[w_off + 8..w_off + 12].try_into().unwrap()),
                f32::from_le_bytes(bytes[w_off + 12..w_off + 16].try_into().unwrap()),
            )
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        if [weight_a, weight_b, weight_c, weight_d]
            .iter()
            .any(|weight| {
                *weight != 0.0 && (!weight.is_finite() || *weight <= 0.0 || *weight > 10.0)
            })
        {
            return Err(PackError::InvalidMeta);
        }

        Ok(Self {
            meta_version,
            verse_count,
            translation_id,
            weight_a,
            weight_b,
            weight_c,
            weight_d,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let id = self.translation_id.as_bytes();
        assert!(id.len() <= u16::MAX as usize);
        let mut out = Vec::with_capacity(32 + id.len());
        out.extend_from_slice(&self.meta_version.to_le_bytes());
        out.extend_from_slice(&self.verse_count.to_le_bytes());
        out.extend_from_slice(&(id.len() as u16).to_le_bytes());
        out.extend_from_slice(id);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out.extend_from_slice(&self.weight_a.to_le_bytes());
        out.extend_from_slice(&self.weight_b.to_le_bytes());
        out.extend_from_slice(&self.weight_c.to_le_bytes());
        out.extend_from_slice(&self.weight_d.to_le_bytes());
        out
    }
}

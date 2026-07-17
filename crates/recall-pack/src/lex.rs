use crate::error::{PackError, PackResult};

const MAGIC: &[u8; 4] = b"LEX1";
const FORMAT_VERSION: u16 = 1;
/// Normalizer v2 corrects the v1 suffix rules for words such as `afraid`,
/// `scared`, and `beginning`. Packs built with v1 must fail closed because
/// their dictionary/postings no longer agree with runtime query stems.
pub const LEX_NORMALIZER_VERSION: u16 = 2;
const HEADER_LEN: usize = 24;
const RECORD_LEN: usize = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexTerm {
    pub text: String,
    pub document_frequency: u32,
}

/// Owned build-time representation of the LEX section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexSection {
    document_count: u32,
    terms: Vec<LexTerm>,
}

impl LexSection {
    /// Terms must be non-empty, unique, and strictly sorted by UTF-8 bytes.
    pub fn new(document_count: u32, terms: Vec<LexTerm>) -> PackResult<Self> {
        if document_count == 0 || terms.is_empty() {
            return Err(PackError::InvalidLex);
        }
        if terms.len() > u32::MAX as usize {
            return Err(PackError::Overflow);
        }
        let mut blob_len = 0usize;
        let mut previous: Option<&str> = None;
        for term in &terms {
            if term.text.is_empty()
                || term.text.len() > u16::MAX as usize
                || term.document_frequency == 0
                || term.document_frequency > document_count
                || previous.is_some_and(|value| value >= term.text.as_str())
            {
                return Err(PackError::InvalidLex);
            }
            blob_len = blob_len
                .checked_add(term.text.len())
                .ok_or(PackError::Overflow)?;
            previous = Some(&term.text);
        }
        if blob_len > u32::MAX as usize {
            return Err(PackError::Overflow);
        }
        Ok(Self {
            document_count,
            terms,
        })
    }

    pub fn document_count(&self) -> u32 {
        self.document_count
    }

    pub fn terms(&self) -> &[LexTerm] {
        &self.terms
    }

    pub fn encode(&self) -> Vec<u8> {
        let blob_len: usize = self.terms.iter().map(|term| term.text.len()).sum();
        let mut out = Vec::with_capacity(HEADER_LEN + self.terms.len() * RECORD_LEN + blob_len);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&LEX_NORMALIZER_VERSION.to_le_bytes());
        out.extend_from_slice(&self.document_count.to_le_bytes());
        out.extend_from_slice(&(self.terms.len() as u32).to_le_bytes());
        out.extend_from_slice(&(blob_len as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());

        let mut offset = 0u32;
        for term in &self.terms {
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&(term.text.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&term.document_frequency.to_le_bytes());
            offset += term.text.len() as u32;
        }
        for term in &self.terms {
            out.extend_from_slice(term.text.as_bytes());
        }
        out
    }
}

/// Owned immutable section bytes, validated once at construction.
#[derive(Debug, Clone)]
pub struct OwnedLex {
    bytes: Vec<u8>,
}

impl OwnedLex {
    pub fn new(bytes: Vec<u8>) -> PackResult<Self> {
        LexView::parse(&bytes)?;
        Ok(Self { bytes })
    }

    pub fn view(&self) -> LexView<'_> {
        LexView::from_validated(&self.bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Allocation-free, validated view over a packed LEX section.
#[derive(Debug, Clone, Copy)]
pub struct LexView<'a> {
    bytes: &'a [u8],
    document_count: u32,
    term_count: u32,
    records_start: usize,
    blob_start: usize,
}

impl<'a> LexView<'a> {
    pub fn parse(bytes: &'a [u8]) -> PackResult<Self> {
        if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
            return Err(PackError::InvalidLex);
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(PackError::UnsupportedLexVersion(version));
        }
        let normalizer_version = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        if normalizer_version != LEX_NORMALIZER_VERSION {
            return Err(PackError::UnsupportedLexNormalizerVersion(
                normalizer_version,
            ));
        }
        let document_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let term_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let blob_len = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        if document_count == 0 || term_count == 0 || bytes[20..24] != [0u8; 4] {
            return Err(PackError::InvalidLex);
        }
        let records_len = (term_count as usize)
            .checked_mul(RECORD_LEN)
            .ok_or(PackError::Overflow)?;
        let blob_start = HEADER_LEN
            .checked_add(records_len)
            .ok_or(PackError::Overflow)?;
        let expected_len = blob_start
            .checked_add(blob_len)
            .ok_or(PackError::Overflow)?;
        if bytes.len() != expected_len {
            return Err(PackError::InvalidLex);
        }

        let view = Self {
            bytes,
            document_count,
            term_count,
            records_start: HEADER_LEN,
            blob_start,
        };
        let mut expected_offset = 0usize;
        let mut previous: Option<&str> = None;
        for term_id in 0..term_count {
            let record = view.record(term_id)?;
            if record.offset != expected_offset
                || record.len == 0
                || record.document_frequency == 0
                || record.document_frequency > document_count
                || previous.is_some_and(|value| value >= record.text)
            {
                return Err(PackError::InvalidLex);
            }
            expected_offset = expected_offset
                .checked_add(record.len)
                .ok_or(PackError::Overflow)?;
            previous = Some(record.text);
        }
        if expected_offset != blob_len {
            return Err(PackError::InvalidLex);
        }
        Ok(view)
    }

    fn from_validated(bytes: &'a [u8]) -> Self {
        let document_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let term_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        Self {
            bytes,
            document_count,
            term_count,
            records_start: HEADER_LEN,
            blob_start: HEADER_LEN + term_count as usize * RECORD_LEN,
        }
    }

    pub fn document_count(self) -> u32 {
        self.document_count
    }

    pub fn term_count(self) -> u32 {
        self.term_count
    }

    pub fn normalizer_version(self) -> u16 {
        LEX_NORMALIZER_VERSION
    }

    pub fn term(self, term_id: u32) -> Option<&'a str> {
        self.record(term_id).ok().map(|record| record.text)
    }

    pub fn document_frequency(self, term_id: u32) -> Option<u32> {
        self.record(term_id)
            .ok()
            .map(|record| record.document_frequency)
    }

    /// Binary search over the canonical sorted term dictionary.
    pub fn lookup(self, term: &str) -> Option<u32> {
        let mut left = 0u32;
        let mut right = self.term_count;
        while left < right {
            let middle = left + (right - left) / 2;
            let candidate = self.term(middle)?;
            match candidate.cmp(term) {
                core::cmp::Ordering::Less => left = middle + 1,
                core::cmp::Ordering::Greater => right = middle,
                core::cmp::Ordering::Equal => return Some(middle),
            }
        }
        None
    }

    fn record(self, term_id: u32) -> PackResult<LexRecord<'a>> {
        if term_id >= self.term_count {
            return Err(PackError::InvalidLex);
        }
        let base = self.records_start + term_id as usize * RECORD_LEN;
        let offset = u32::from_le_bytes(self.bytes[base..base + 4].try_into().unwrap()) as usize;
        let len = u16::from_le_bytes(self.bytes[base + 4..base + 6].try_into().unwrap()) as usize;
        if self.bytes[base + 6..base + 8] != [0u8; 2] {
            return Err(PackError::InvalidLex);
        }
        let document_frequency =
            u32::from_le_bytes(self.bytes[base + 8..base + 12].try_into().unwrap());
        let end = offset.checked_add(len).ok_or(PackError::Overflow)?;
        let blob = &self.bytes[self.blob_start..];
        if end > blob.len() {
            return Err(PackError::InvalidLex);
        }
        let text = std::str::from_utf8(&blob[offset..end]).map_err(|_| PackError::InvalidUtf8 {
            context: "LEX.term",
        })?;
        Ok(LexRecord {
            offset,
            len,
            document_frequency,
            text,
        })
    }
}

struct LexRecord<'a> {
    offset: usize,
    len: usize,
    document_frequency: u32,
    text: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> LexSection {
        LexSection::new(
            3,
            vec![
                LexTerm {
                    text: "love".into(),
                    document_frequency: 2,
                },
                LexTerm {
                    text: "shepherd".into(),
                    document_frequency: 1,
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn roundtrip_is_zero_copy_and_searchable() {
        let bytes = section().encode();
        let view = LexView::parse(&bytes).unwrap();
        assert_eq!(view.document_count(), 3);
        assert_eq!(view.lookup("love"), Some(0));
        assert_eq!(view.lookup("shepherd"), Some(1));
        assert_eq!(view.lookup("missing"), None);
        assert_eq!(view.term(1), Some("shepherd"));
        assert_eq!(view.document_frequency(0), Some(2));
    }

    #[test]
    fn rejects_unsorted_duplicate_or_out_of_range_terms() {
        assert!(LexSection::new(
            2,
            vec![
                LexTerm {
                    text: "z".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "a".into(),
                    document_frequency: 1,
                },
            ]
        )
        .is_err());
        assert!(LexSection::new(
            1,
            vec![LexTerm {
                text: "x".into(),
                document_frequency: 2,
            }]
        )
        .is_err());
    }

    #[test]
    fn parser_rejects_reserved_offsets_utf8_and_trailing_data() {
        let bytes = section().encode();

        let mut old_normalizer = bytes.clone();
        old_normalizer[6..8].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(
            LexView::parse(&old_normalizer).unwrap_err(),
            PackError::UnsupportedLexNormalizerVersion(1)
        );

        let mut dirty_reserved = bytes.clone();
        dirty_reserved[20] = 1;
        assert_eq!(
            LexView::parse(&dirty_reserved).unwrap_err(),
            PackError::InvalidLex
        );

        let mut bad_offset = bytes.clone();
        bad_offset[HEADER_LEN] = 1;
        assert_eq!(
            LexView::parse(&bad_offset).unwrap_err(),
            PackError::InvalidLex
        );

        let mut bad_utf8 = bytes.clone();
        let last = bad_utf8.len() - 1;
        bad_utf8[last] = 0xff;
        assert!(matches!(
            LexView::parse(&bad_utf8),
            Err(PackError::InvalidUtf8 {
                context: "LEX.term"
            })
        ));

        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            LexView::parse(&trailing).unwrap_err(),
            PackError::InvalidLex
        );
    }
}

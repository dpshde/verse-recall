use crate::error::{PackError, PackResult};
use crate::lex::LexView;

const MAGIC: &[u8; 4] = b"PST1";
const FORMAT_VERSION: u16 = 1;
const HEADER_LEN: usize = 32;
const RECORD_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Posting {
    pub document_id: u32,
    pub term_frequency: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostTerm {
    pub postings: Vec<Posting>,
}

/// Owned build-time representation of the POST section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostSection {
    document_lengths: Vec<u16>,
    terms: Vec<PostTerm>,
}

impl PostSection {
    pub fn new(document_lengths: Vec<u16>, terms: Vec<PostTerm>) -> PackResult<Self> {
        if document_lengths.is_empty() || document_lengths.contains(&0) || terms.is_empty() {
            return Err(PackError::InvalidPost);
        }
        if document_lengths.len() > u32::MAX as usize || terms.len() > u32::MAX as usize {
            return Err(PackError::Overflow);
        }
        let document_count = document_lengths.len() as u32;
        let mut posting_count = 0usize;
        let mut blob_len = 0usize;
        for term in &terms {
            if term.postings.is_empty() {
                return Err(PackError::InvalidPost);
            }
            let mut previous = None;
            for posting in &term.postings {
                if posting.document_id >= document_count
                    || posting.term_frequency == 0
                    || previous.is_some_and(|value| value >= posting.document_id)
                {
                    return Err(PackError::InvalidPost);
                }
                let delta =
                    previous.map_or(posting.document_id, |value| posting.document_id - value);
                blob_len = blob_len
                    .checked_add(varint_len(delta) + 1)
                    .ok_or(PackError::Overflow)?;
                previous = Some(posting.document_id);
            }
            posting_count = posting_count
                .checked_add(term.postings.len())
                .ok_or(PackError::Overflow)?;
            if term.postings.len() > u32::MAX as usize {
                return Err(PackError::Overflow);
            }
        }
        if posting_count > u32::MAX as usize || blob_len > u32::MAX as usize {
            return Err(PackError::Overflow);
        }
        Ok(Self {
            document_lengths,
            terms,
        })
    }

    pub fn document_lengths(&self) -> &[u16] {
        &self.document_lengths
    }

    pub fn terms(&self) -> &[PostTerm] {
        &self.terms
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut encoded_terms = Vec::with_capacity(self.terms.len());
        let mut postings_blob = Vec::new();
        let mut posting_count = 0u32;
        for term in &self.terms {
            let start = postings_blob.len() as u32;
            let mut previous = 0u32;
            for (index, posting) in term.postings.iter().enumerate() {
                let delta = if index == 0 {
                    posting.document_id
                } else {
                    posting.document_id - previous
                };
                encode_varint(delta, &mut postings_blob);
                postings_blob.push(posting.term_frequency);
                previous = posting.document_id;
            }
            encoded_terms.push((
                start,
                postings_blob.len() as u32 - start,
                term.postings.len() as u32,
            ));
            posting_count += term.postings.len() as u32;
        }

        let doc_bytes = self.document_lengths.len() * 2;
        let records_start = align4(HEADER_LEN + doc_bytes);
        let mut out =
            Vec::with_capacity(records_start + self.terms.len() * RECORD_LEN + postings_blob.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(self.document_lengths.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.terms.len() as u32).to_le_bytes());
        out.extend_from_slice(&posting_count.to_le_bytes());
        out.extend_from_slice(&(postings_blob.len() as u32).to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        for length in &self.document_lengths {
            out.extend_from_slice(&length.to_le_bytes());
        }
        out.resize(records_start, 0);
        for (offset, len, document_frequency) in encoded_terms {
            out.extend_from_slice(&offset.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&document_frequency.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
        }
        out.extend_from_slice(&postings_blob);
        out
    }
}

/// Owned immutable section bytes, validated once at construction.
#[derive(Debug, Clone)]
pub struct OwnedPost {
    bytes: Vec<u8>,
}

impl OwnedPost {
    pub fn new(bytes: Vec<u8>) -> PackResult<Self> {
        PostView::parse(&bytes)?;
        Ok(Self { bytes })
    }

    pub fn view(&self) -> PostView<'_> {
        PostView::from_validated(&self.bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Allocation-free, validated view over packed document lengths and postings.
#[derive(Debug, Clone, Copy)]
pub struct PostView<'a> {
    bytes: &'a [u8],
    document_count: u32,
    term_count: u32,
    posting_count: u32,
    lengths_start: usize,
    records_start: usize,
    blob_start: usize,
}

impl<'a> PostView<'a> {
    pub fn parse(bytes: &'a [u8]) -> PackResult<Self> {
        if bytes.len() < HEADER_LEN || &bytes[..4] != MAGIC {
            return Err(PackError::InvalidPost);
        }
        let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(PackError::UnsupportedPostVersion(version));
        }
        let document_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let term_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let posting_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let blob_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
        if bytes[6..8] != [0u8; 2]
            || bytes[24..32] != [0u8; 8]
            || document_count == 0
            || term_count == 0
            || posting_count == 0
        {
            return Err(PackError::InvalidPost);
        }
        let lengths_len = (document_count as usize)
            .checked_mul(2)
            .ok_or(PackError::Overflow)?;
        let records_start = align4(
            HEADER_LEN
                .checked_add(lengths_len)
                .ok_or(PackError::Overflow)?,
        );
        let records_len = (term_count as usize)
            .checked_mul(RECORD_LEN)
            .ok_or(PackError::Overflow)?;
        let blob_start = records_start
            .checked_add(records_len)
            .ok_or(PackError::Overflow)?;
        let expected_len = blob_start
            .checked_add(blob_len)
            .ok_or(PackError::Overflow)?;
        if bytes.len() != expected_len
            || bytes[HEADER_LEN + lengths_len..records_start]
                .iter()
                .any(|value| *value != 0)
        {
            return Err(PackError::InvalidPost);
        }

        let view = Self {
            bytes,
            document_count,
            term_count,
            posting_count,
            lengths_start: HEADER_LEN,
            records_start,
            blob_start,
        };
        if (0..document_count).any(|document_id| view.document_length(document_id) == Some(0)) {
            return Err(PackError::InvalidPost);
        }

        let mut expected_offset = 0usize;
        let mut decoded_postings = 0u32;
        for term_id in 0..term_count {
            let record = view.record(term_id)?;
            if record.offset != expected_offset || record.len == 0 || record.document_frequency == 0
            {
                return Err(PackError::InvalidPost);
            }
            let bytes = &view.bytes
                [view.blob_start + record.offset..view.blob_start + record.offset + record.len];
            validate_postings(bytes, record.document_frequency, document_count)?;
            expected_offset = expected_offset
                .checked_add(record.len)
                .ok_or(PackError::Overflow)?;
            decoded_postings = decoded_postings
                .checked_add(record.document_frequency)
                .ok_or(PackError::Overflow)?;
        }
        if expected_offset != blob_len || decoded_postings != posting_count {
            return Err(PackError::InvalidPost);
        }
        Ok(view)
    }

    fn from_validated(bytes: &'a [u8]) -> Self {
        let document_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let term_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let posting_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let lengths_len = document_count as usize * 2;
        let records_start = align4(HEADER_LEN + lengths_len);
        Self {
            bytes,
            document_count,
            term_count,
            posting_count,
            lengths_start: HEADER_LEN,
            records_start,
            blob_start: records_start + term_count as usize * RECORD_LEN,
        }
    }

    pub fn document_count(self) -> u32 {
        self.document_count
    }

    pub fn term_count(self) -> u32 {
        self.term_count
    }

    pub fn posting_count(self) -> u32 {
        self.posting_count
    }

    pub fn document_length(self, document_id: u32) -> Option<u16> {
        if document_id >= self.document_count {
            return None;
        }
        let base = self.lengths_start + document_id as usize * 2;
        Some(u16::from_le_bytes(
            self.bytes[base..base + 2].try_into().unwrap(),
        ))
    }

    pub fn document_frequency(self, term_id: u32) -> Option<u32> {
        self.record(term_id)
            .ok()
            .map(|record| record.document_frequency)
    }

    pub fn postings(self, term_id: u32) -> Option<Postings<'a>> {
        let record = self.record(term_id).ok()?;
        let start = self.blob_start + record.offset;
        Some(Postings {
            bytes: &self.bytes[start..start + record.len],
            cursor: 0,
            remaining: record.document_frequency,
            previous: 0,
            first: true,
        })
    }

    fn record(self, term_id: u32) -> PackResult<PostRecord> {
        if term_id >= self.term_count {
            return Err(PackError::InvalidPost);
        }
        let base = self.records_start + term_id as usize * RECORD_LEN;
        let offset = u32::from_le_bytes(self.bytes[base..base + 4].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(self.bytes[base + 4..base + 8].try_into().unwrap()) as usize;
        let document_frequency =
            u32::from_le_bytes(self.bytes[base + 8..base + 12].try_into().unwrap());
        if self.bytes[base + 12..base + 16] != [0u8; 4] {
            return Err(PackError::InvalidPost);
        }
        let blob_len = self.bytes.len() - self.blob_start;
        let end = offset.checked_add(len).ok_or(PackError::Overflow)?;
        if end > blob_len {
            return Err(PackError::InvalidPost);
        }
        Ok(PostRecord {
            offset,
            len,
            document_frequency,
        })
    }
}

struct PostRecord {
    offset: usize,
    len: usize,
    document_frequency: u32,
}

/// Validated posting-list iterator. Parsing has already established that every
/// item is canonical and in range, so iteration requires no allocation.
pub struct Postings<'a> {
    bytes: &'a [u8],
    cursor: usize,
    remaining: u32,
    previous: u32,
    first: bool,
}

impl Iterator for Postings<'_> {
    type Item = Posting;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let delta = decode_varint(self.bytes, &mut self.cursor).ok()?;
        let term_frequency = *self.bytes.get(self.cursor)?;
        self.cursor += 1;
        let document_id = if self.first {
            delta
        } else {
            self.previous.checked_add(delta)?
        };
        self.first = false;
        self.previous = document_id;
        self.remaining -= 1;
        Some(Posting {
            document_id,
            term_frequency,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.remaining as usize;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Postings<'_> {}

pub fn validate_lex_post(lex: LexView<'_>, post: PostView<'_>) -> PackResult<()> {
    if lex.document_count() != post.document_count() || lex.term_count() != post.term_count() {
        return Err(PackError::IncompatibleLexPost);
    }
    for term_id in 0..lex.term_count() {
        if lex.document_frequency(term_id) != post.document_frequency(term_id) {
            return Err(PackError::IncompatibleLexPost);
        }
    }
    Ok(())
}

fn validate_postings(bytes: &[u8], expected_count: u32, document_count: u32) -> PackResult<()> {
    let mut cursor = 0usize;
    let mut previous = 0u32;
    for index in 0..expected_count {
        let delta = decode_varint(bytes, &mut cursor)?;
        let term_frequency = *bytes.get(cursor).ok_or(PackError::InvalidPost)?;
        cursor += 1;
        let document_id = if index == 0 {
            delta
        } else {
            if delta == 0 {
                return Err(PackError::InvalidPost);
            }
            previous.checked_add(delta).ok_or(PackError::Overflow)?
        };
        if document_id >= document_count || term_frequency == 0 {
            return Err(PackError::InvalidPost);
        }
        previous = document_id;
    }
    if cursor != bytes.len() {
        return Err(PackError::InvalidPost);
    }
    Ok(())
}

fn encode_varint(mut value: u32, output: &mut Vec<u8>) {
    while value >= 0x80 {
        output.push((value as u8 & 0x7f) | 0x80);
        value >>= 7;
    }
    output.push(value as u8);
}

const fn varint_len(value: u32) -> usize {
    if value < (1 << 7) {
        1
    } else if value < (1 << 14) {
        2
    } else if value < (1 << 21) {
        3
    } else if value < (1 << 28) {
        4
    } else {
        5
    }
}

fn decode_varint(bytes: &[u8], cursor: &mut usize) -> PackResult<u32> {
    let mut value = 0u32;
    for index in 0..5 {
        let byte = *bytes.get(*cursor).ok_or(PackError::InvalidPost)?;
        *cursor += 1;
        if index == 4 && (byte & 0xf0) != 0 {
            return Err(PackError::InvalidPost);
        }
        value |= u32::from(byte & 0x7f) << (index * 7);
        if byte & 0x80 == 0 {
            if index > 0 && byte == 0 {
                return Err(PackError::InvalidPost);
            }
            return Ok(value);
        }
    }
    Err(PackError::InvalidPost)
}

const fn align4(value: usize) -> usize {
    (value + 3) & !3
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LexSection, LexTerm};

    fn section() -> PostSection {
        PostSection::new(
            vec![8, 5, 7],
            vec![
                PostTerm {
                    postings: vec![
                        Posting {
                            document_id: 0,
                            term_frequency: 2,
                        },
                        Posting {
                            document_id: 2,
                            term_frequency: 1,
                        },
                    ],
                },
                PostTerm {
                    postings: vec![Posting {
                        document_id: 1,
                        term_frequency: 3,
                    }],
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn roundtrip_decodes_delta_postings_without_allocation() {
        let bytes = section().encode();
        let view = PostView::parse(&bytes).unwrap();
        assert_eq!(view.document_count(), 3);
        assert_eq!(view.posting_count(), 3);
        assert_eq!(view.document_length(1), Some(5));
        assert_eq!(
            view.postings(0).unwrap().collect::<Vec<_>>(),
            section().terms()[0].postings
        );
    }

    #[test]
    fn validates_lex_alignment() {
        let post_bytes = section().encode();
        let post = PostView::parse(&post_bytes).unwrap();
        let lex_bytes = LexSection::new(
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
        .encode();
        assert!(validate_lex_post(LexView::parse(&lex_bytes).unwrap(), post).is_ok());

        let mismatched = LexSection::new(
            3,
            vec![
                LexTerm {
                    text: "love".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "shepherd".into(),
                    document_frequency: 1,
                },
            ],
        )
        .unwrap()
        .encode();
        assert_eq!(
            validate_lex_post(LexView::parse(&mismatched).unwrap(), post),
            Err(PackError::IncompatibleLexPost)
        );
    }

    #[test]
    fn parser_rejects_reserved_trailing_and_noncanonical_varints() {
        let bytes = section().encode();
        let mut dirty_reserved = bytes.clone();
        dirty_reserved[24] = 1;
        assert_eq!(
            PostView::parse(&dirty_reserved).unwrap_err(),
            PackError::InvalidPost
        );

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            PostView::parse(&trailing).unwrap_err(),
            PackError::InvalidPost
        );

        let parsed = PostView::parse(&bytes).unwrap();
        let first_record = parsed.records_start;
        let blob_start = parsed.blob_start;
        let mut overlong = bytes;
        // Replace the first one-byte doc delta with an overlong zero and grow
        // the recorded/blob lengths consistently; canonical decoding must fail.
        overlong.insert(blob_start, 0x80);
        let old_record_len = u32::from_le_bytes(
            overlong[first_record + 4..first_record + 8]
                .try_into()
                .unwrap(),
        );
        overlong[first_record + 4..first_record + 8]
            .copy_from_slice(&(old_record_len + 1).to_le_bytes());
        let old_blob_len = u32::from_le_bytes(overlong[20..24].try_into().unwrap());
        overlong[20..24].copy_from_slice(&(old_blob_len + 1).to_le_bytes());
        assert_eq!(
            PostView::parse(&overlong).unwrap_err(),
            PackError::InvalidPost
        );
    }

    #[test]
    fn builder_rejects_unsorted_or_zero_data() {
        assert!(PostSection::new(
            vec![1],
            vec![PostTerm {
                postings: vec![Posting {
                    document_id: 0,
                    term_frequency: 0,
                }]
            }]
        )
        .is_err());
        assert!(PostSection::new(
            vec![1, 1],
            vec![PostTerm {
                postings: vec![
                    Posting {
                        document_id: 1,
                        term_frequency: 1,
                    },
                    Posting {
                        document_id: 0,
                        term_frequency: 1,
                    },
                ]
            }]
        )
        .is_err());
    }
}

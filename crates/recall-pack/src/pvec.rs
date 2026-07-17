use crate::error::{PackError, PackResult};

/// Magic bytes for a passage-vector section body: `VRPVEC01`.
pub const PVEC_MAGIC: [u8; 8] = *b"VRPVEC01";
pub const PVEC_FORMAT_VERSION: u16 = 1;
pub const PVEC_HEADER_LEN: usize = 64;
pub const PVEC_ROW_LEN: usize = 16;
pub const PVEC_EMBEDDING_SPACE_ID_LEN: usize = 32;

/// Provenance class for a passage representation.
///
/// The source kind is deliberately stored on every row. A runtime must not
/// infer provenance from a section filename, pack order, or range shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum PvecSourceKind {
    /// A range defined directly by the canonical corpus producer.
    CanonicalPassage = 1,
    /// A range imported from the Study Bible `passages` table.
    StudyBiblePassage = 2,
}

impl PvecSourceKind {
    pub fn from_code(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::CanonicalPassage),
            2 => Some(Self::StudyBiblePassage),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CanonicalPassage => "canonical-passage",
            Self::StudyBiblePassage => "study-bible-passage",
        }
    }
}

/// One passage vector's explicit canonical target and provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PvecRow {
    pub anchor_index: u32,
    pub start_index: u32,
    pub end_index: u32,
    pub source_kind: PvecSourceKind,
}

impl PvecRow {
    fn ordering_key(self) -> (u32, u32, PvecSourceKind) {
        (self.start_index, self.end_index, self.source_kind)
    }
}

/// Owned, validated PVEC v1 body.
///
/// Vectors are row-major signed int8 values. Build tools must L2-normalize
/// each f32 source row before quantizing it into `[-127, 127]`; `-128` and
/// all-zero rows are rejected so malformed or differently quantized bodies
/// fail closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PvecSection {
    pub dims: u16,
    pub canonical_verse_count: u32,
    pub embedding_space_id: [u8; PVEC_EMBEDDING_SPACE_ID_LEN],
    pub rows: Vec<PvecRow>,
    pub vectors: Vec<i8>,
}

impl PvecSection {
    pub fn new(
        dims: u16,
        canonical_verse_count: u32,
        embedding_space_id: [u8; PVEC_EMBEDDING_SPACE_ID_LEN],
        rows: Vec<PvecRow>,
        vectors: Vec<i8>,
    ) -> PackResult<Self> {
        let section = Self {
            dims,
            canonical_verse_count,
            embedding_space_id,
            rows,
            vectors,
        };
        section.validate()?;
        Ok(section)
    }

    pub fn parse(bytes: &[u8]) -> PackResult<Self> {
        if bytes.len() < PVEC_HEADER_LEN || bytes[..8] != PVEC_MAGIC {
            return Err(PackError::InvalidPvec);
        }
        let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        if version != PVEC_FORMAT_VERSION {
            return Err(PackError::UnsupportedPvecVersion(version));
        }
        let dims = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
        let canonical_verse_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let row_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let row_len = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
        if row_len != PVEC_ROW_LEN || bytes[56..PVEC_HEADER_LEN].iter().any(|byte| *byte != 0) {
            return Err(PackError::InvalidPvec);
        }
        let mut embedding_space_id = [0u8; PVEC_EMBEDDING_SPACE_ID_LEN];
        embedding_space_id.copy_from_slice(&bytes[24..56]);

        let rows_len = row_count
            .checked_mul(PVEC_ROW_LEN)
            .ok_or(PackError::Overflow)?;
        let vectors_len = row_count
            .checked_mul(usize::from(dims))
            .ok_or(PackError::Overflow)?;
        let vectors_start = PVEC_HEADER_LEN
            .checked_add(rows_len)
            .ok_or(PackError::Overflow)?;
        let expected_len = vectors_start
            .checked_add(vectors_len)
            .ok_or(PackError::Overflow)?;
        if bytes.len() != expected_len {
            return Err(PackError::InvalidPvec);
        }

        let mut rows = Vec::with_capacity(row_count);
        for raw in bytes[PVEC_HEADER_LEN..vectors_start].chunks_exact(PVEC_ROW_LEN) {
            if raw[13..16].iter().any(|byte| *byte != 0) {
                return Err(PackError::InvalidPvec);
            }
            rows.push(PvecRow {
                anchor_index: u32::from_le_bytes(raw[0..4].try_into().unwrap()),
                start_index: u32::from_le_bytes(raw[4..8].try_into().unwrap()),
                end_index: u32::from_le_bytes(raw[8..12].try_into().unwrap()),
                source_kind: PvecSourceKind::from_code(raw[12]).ok_or(PackError::InvalidPvec)?,
            });
        }
        let vectors = bytes[vectors_start..]
            .iter()
            .map(|value| *value as i8)
            .collect();
        Self::new(
            dims,
            canonical_verse_count,
            embedding_space_id,
            rows,
            vectors,
        )
    }

    pub fn encode(&self) -> PackResult<Vec<u8>> {
        self.validate()?;
        let row_count = u32::try_from(self.rows.len()).map_err(|_| PackError::Overflow)?;
        let rows_len = self
            .rows
            .len()
            .checked_mul(PVEC_ROW_LEN)
            .ok_or(PackError::Overflow)?;
        let body_len = PVEC_HEADER_LEN
            .checked_add(rows_len)
            .and_then(|value| value.checked_add(self.vectors.len()))
            .ok_or(PackError::Overflow)?;
        let mut out = Vec::with_capacity(body_len);
        out.extend_from_slice(&PVEC_MAGIC);
        out.extend_from_slice(&PVEC_FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&self.dims.to_le_bytes());
        out.extend_from_slice(&self.canonical_verse_count.to_le_bytes());
        out.extend_from_slice(&row_count.to_le_bytes());
        out.extend_from_slice(&(PVEC_ROW_LEN as u32).to_le_bytes());
        out.extend_from_slice(&self.embedding_space_id);
        out.extend_from_slice(&[0u8; 8]);
        debug_assert_eq!(out.len(), PVEC_HEADER_LEN);
        for row in &self.rows {
            out.extend_from_slice(&row.anchor_index.to_le_bytes());
            out.extend_from_slice(&row.start_index.to_le_bytes());
            out.extend_from_slice(&row.end_index.to_le_bytes());
            out.push(row.source_kind as u8);
            out.extend_from_slice(&[0u8; 3]);
        }
        out.extend(self.vectors.iter().map(|value| *value as u8));
        debug_assert_eq!(out.len(), body_len);
        Ok(out)
    }

    fn validate(&self) -> PackResult<()> {
        if self.dims == 0
            || self.canonical_verse_count == 0
            || self.rows.is_empty()
            || self.embedding_space_id.iter().all(|byte| *byte == 0)
        {
            return Err(PackError::InvalidPvec);
        }
        let expected_vectors = self
            .rows
            .len()
            .checked_mul(usize::from(self.dims))
            .ok_or(PackError::Overflow)?;
        if self.vectors.len() != expected_vectors {
            return Err(PackError::InvalidPvec);
        }

        let mut previous = None;
        for row in &self.rows {
            if row.start_index > row.anchor_index
                || row.anchor_index > row.end_index
                || row.end_index >= self.canonical_verse_count
            {
                return Err(PackError::InvalidPvec);
            }
            let key = row.ordering_key();
            if previous.is_some_and(|prior| prior >= key) {
                return Err(PackError::InvalidPvec);
            }
            previous = Some(key);
        }
        for vector in self.vectors.chunks_exact(usize::from(self.dims)) {
            if vector.contains(&i8::MIN) || vector.iter().all(|value| *value == 0) {
                return Err(PackError::InvalidPvec);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPACE_ID: [u8; PVEC_EMBEDDING_SPACE_ID_LEN] = [0x5a; 32];

    fn fixture() -> PvecSection {
        PvecSection::new(
            2,
            6,
            SPACE_ID,
            vec![
                PvecRow {
                    anchor_index: 1,
                    start_index: 0,
                    end_index: 2,
                    source_kind: PvecSourceKind::StudyBiblePassage,
                },
                PvecRow {
                    anchor_index: 4,
                    start_index: 3,
                    end_index: 5,
                    source_kind: PvecSourceKind::StudyBiblePassage,
                },
            ],
            vec![127, 0, 0, 127],
        )
        .unwrap()
    }

    #[test]
    fn round_trip_preserves_ranges_vectors_and_space() {
        let section = fixture();
        assert_eq!(
            PvecSection::parse(&section.encode().unwrap()).unwrap(),
            section
        );
    }

    #[test]
    fn rejects_bad_version_reserved_bytes_and_truncation() {
        let body = fixture().encode().unwrap();

        let mut bad_version = body.clone();
        bad_version[8..10].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(
            PvecSection::parse(&bad_version),
            Err(PackError::UnsupportedPvecVersion(2))
        );

        let mut dirty_reserved = body.clone();
        dirty_reserved[56] = 1;
        assert_eq!(
            PvecSection::parse(&dirty_reserved),
            Err(PackError::InvalidPvec)
        );

        assert_eq!(
            PvecSection::parse(&body[..body.len() - 1]),
            Err(PackError::InvalidPvec)
        );
    }

    #[test]
    fn rejects_unsorted_duplicate_invalid_and_noncanonical_vectors() {
        let first = fixture().rows[0];
        let second = fixture().rows[1];
        for rows in [vec![second, first], vec![first, first]] {
            assert_eq!(
                PvecSection::new(2, 6, SPACE_ID, rows, vec![127, 0, 0, 127]),
                Err(PackError::InvalidPvec)
            );
        }

        let invalid_range = PvecRow {
            anchor_index: 0,
            start_index: 1,
            end_index: 2,
            source_kind: PvecSourceKind::CanonicalPassage,
        };
        assert_eq!(
            PvecSection::new(2, 6, SPACE_ID, vec![invalid_range], vec![127, 0]),
            Err(PackError::InvalidPvec)
        );
        assert_eq!(
            PvecSection::new(2, 6, SPACE_ID, vec![first], vec![0, 0]),
            Err(PackError::InvalidPvec)
        );
        assert_eq!(
            PvecSection::new(2, 6, SPACE_ID, vec![first], vec![i8::MIN, 0]),
            Err(PackError::InvalidPvec)
        );
    }
}

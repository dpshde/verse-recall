use crate::lanes::traits::LaneHit;
use crate::result::Confidence;
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

const MAGIC: &[u8; 4] = b"LVC1";
const V1_HEADER_LEN: usize = 16;
const V2_HEADER_LEN: usize = 48;
const V3_HEADER_LEN: usize = 48;
const EMBEDDING_SPACE_ID_LEN: usize = 32;
const MAX_TABLES: usize = 2;

/// Corpus-wide meaning of one aligned LVEC representation table.
///
/// LVEC v1/v2 did not store roles: their stable implied layout is canonical
/// verse first and optional chapter-bounded window second. LVEC v3 writes
/// explicit role codes so a second Study Note representation cannot be
/// mislabeled as a verse window or acquire fabricated passage provenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DenseTableRole {
    Verse = 1,
    Window = 2,
    StudyNote = 3,
}

impl DenseTableRole {
    fn from_code(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Verse),
            2 => Some(Self::Window),
            3 => Some(Self::StudyNote),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verse => "verse",
            Self::Window => "window",
            Self::StudyNote => "study-note",
        }
    }

    const fn evidence_marker(self) -> &'static str {
        match self {
            Self::Verse => "semantic:verse",
            Self::Window => "semantic:window",
            Self::StudyNote => "semantic:study-note",
        }
    }
}

/// Parsed compatibility metadata for an LVEC body.
///
/// Version 1 predates embedding-space identification. Version 2 and 3 packs
/// carry a nonzero, opaque 32-byte ID derived by the build tooling from the
/// complete encoder contract (model, tokenizer, pooling, prefixes, projection,
/// and normalization). Hosts must present the same ID with every query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DenseMetadata {
    pub version: u16,
    pub dims: usize,
    pub verse_count: usize,
    pub table_count: usize,
    pub embedding_space_id: Option<[u8; EMBEDDING_SPACE_ID_LEN]>,
    /// Roles for the first `table_count` entries. The unused slot is Verse.
    pub table_roles: [DenseTableRole; MAX_TABLES],
}

#[derive(Clone, Copy, Debug)]
struct ScoredVerse {
    verse_index: usize,
    score: f32,
}

impl PartialEq for ScoredVerse {
    fn eq(&self, other: &Self) -> bool {
        self.verse_index == other.verse_index && self.score.total_cmp(&other.score).is_eq()
    }
}

impl Eq for ScoredVerse {}

impl PartialOrd for ScoredVerse {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredVerse {
    fn cmp(&self, other: &Self) -> Ordering {
        // A larger score is better. Canonical verse order is the deterministic
        // tie-break, so a lower verse index is also considered better.
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.verse_index.cmp(&self.verse_index))
    }
}

/// Optional Lane C vectors. Query inference deliberately stays in the host so
/// recall-core remains dependency-free and portable across native and WASM.
pub struct DenseLane {
    metadata: DenseMetadata,
    tables: Vec<Vec<i8>>,
}

impl DenseLane {
    pub fn parse(bytes: &[u8], expected_verse_count: usize) -> Result<Self, String> {
        if bytes.len() < V1_HEADER_LEN || &bytes[..4] != MAGIC {
            return Err("LVEC has an invalid header".into());
        }
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        let header_len = match version {
            1 => V1_HEADER_LEN,
            2 => V2_HEADER_LEN,
            3 => V3_HEADER_LEN,
            _ => return Err(format!("LVEC version {version} is not supported")),
        };
        if bytes.len() < header_len {
            return Err("LVEC has a truncated header".into());
        }

        let dims = u16::from_le_bytes([bytes[6], bytes[7]]) as usize;
        let verse_count =
            u32::from_le_bytes(bytes[8..12].try_into().expect("fixed slice")) as usize;
        let table_count = bytes[12] as usize;
        if dims == 0
            || verse_count != expected_verse_count
            || !(1..=MAX_TABLES).contains(&table_count)
        {
            return Err("LVEC metadata is invalid".into());
        }

        let table_roles = if version <= 2 {
            if bytes[13..V1_HEADER_LEN].iter().any(|byte| *byte != 0) {
                return Err("LVEC metadata is invalid".into());
            }
            [DenseTableRole::Verse, DenseTableRole::Window]
        } else {
            if bytes[15] != 0 {
                return Err("LVEC metadata is invalid".into());
            }
            let first = DenseTableRole::from_code(bytes[13])
                .ok_or_else(|| "LVEC v3 has an invalid first table role".to_string())?;
            if first != DenseTableRole::Verse {
                return Err("LVEC v3 first table must be canonical verse vectors".into());
            }
            let second = if table_count == 2 {
                DenseTableRole::from_code(bytes[14])
                    .filter(|role| *role != DenseTableRole::Verse)
                    .ok_or_else(|| "LVEC v3 has an invalid second table role".to_string())?
            } else {
                if bytes[14] != 0 {
                    return Err("LVEC v3 has a role for a missing second table".into());
                }
                DenseTableRole::Verse
            };
            [first, second]
        };

        let embedding_space_id = if version >= 2 {
            let id: [u8; EMBEDDING_SPACE_ID_LEN] = bytes[V1_HEADER_LEN..V2_HEADER_LEN]
                .try_into()
                .expect("validated modern header length");
            if id.iter().all(|byte| *byte == 0) {
                return Err("LVEC embedding-space ID must not be zero".into());
            }
            Some(id)
        } else {
            None
        };

        let table_len = verse_count
            .checked_mul(dims)
            .ok_or_else(|| "LVEC dimensions overflow".to_string())?;
        let expected_len = header_len
            .checked_add(
                table_len
                    .checked_mul(table_count)
                    .ok_or_else(|| "LVEC table count overflows".to_string())?,
            )
            .ok_or_else(|| "LVEC length overflows".to_string())?;
        if bytes.len() != expected_len {
            return Err("LVEC body length does not match metadata".into());
        }
        let mut tables = Vec::with_capacity(table_count);
        for table_index in 0..table_count {
            let start = header_len + table_index * table_len;
            tables.push(
                bytes[start..start + table_len]
                    .iter()
                    .map(|value| *value as i8)
                    .collect(),
            );
        }
        Ok(Self {
            metadata: DenseMetadata {
                version,
                dims,
                verse_count,
                table_count,
                embedding_space_id,
                table_roles,
            },
            tables,
        })
    }

    /// Encode a version 2 LVEC body from already quantized row-major tables.
    ///
    /// This helper keeps build tools and the runtime parser on one layout. It
    /// does not quantize or hash model artifacts; callers must provide a
    /// nonzero embedding-space ID and normalized int8 tables.
    pub fn encode_v2(
        dims: usize,
        verse_count: usize,
        tables: &[&[i8]],
        embedding_space_id: [u8; EMBEDDING_SPACE_ID_LEN],
    ) -> Result<Vec<u8>, String> {
        if dims == 0 || dims > u16::MAX as usize || verse_count > u32::MAX as usize {
            return Err("LVEC v2 dimensions are out of range".into());
        }
        if !(1..=MAX_TABLES).contains(&tables.len()) {
            return Err("LVEC v2 requires one or two tables".into());
        }
        if embedding_space_id.iter().all(|byte| *byte == 0) {
            return Err("LVEC v2 embedding-space ID must not be zero".into());
        }
        let table_len = verse_count
            .checked_mul(dims)
            .ok_or_else(|| "LVEC v2 dimensions overflow".to_string())?;
        if tables.iter().any(|table| table.len() != table_len) {
            return Err("LVEC v2 table length does not match dimensions".into());
        }
        let body_len = V2_HEADER_LEN
            .checked_add(
                table_len
                    .checked_mul(tables.len())
                    .ok_or_else(|| "LVEC v2 table count overflows".to_string())?,
            )
            .ok_or_else(|| "LVEC v2 body length overflows".to_string())?;
        let mut body = Vec::with_capacity(body_len);
        body.extend_from_slice(MAGIC);
        body.extend_from_slice(&2u16.to_le_bytes());
        body.extend_from_slice(&(dims as u16).to_le_bytes());
        body.extend_from_slice(&(verse_count as u32).to_le_bytes());
        body.push(tables.len() as u8);
        body.extend_from_slice(&[0, 0, 0]);
        body.extend_from_slice(&embedding_space_id);
        for table in tables {
            body.extend(table.iter().map(|value| *value as u8));
        }
        debug_assert_eq!(body.len(), body_len);
        Ok(body)
    }

    /// Encode an LVEC v3 body with explicit aligned-table roles.
    ///
    /// Table 1 must remain canonical verse vectors. Table 2 may be a
    /// chapter-bounded window or independently authored Study Note view.
    pub fn encode_v3(
        dims: usize,
        verse_count: usize,
        tables: &[&[i8]],
        table_roles: &[DenseTableRole],
        embedding_space_id: [u8; EMBEDDING_SPACE_ID_LEN],
    ) -> Result<Vec<u8>, String> {
        if dims == 0 || dims > u16::MAX as usize || verse_count > u32::MAX as usize {
            return Err("LVEC v3 dimensions are out of range".into());
        }
        if !(1..=MAX_TABLES).contains(&tables.len()) || table_roles.len() != tables.len() {
            return Err("LVEC v3 requires one role for each of one or two tables".into());
        }
        if table_roles[0] != DenseTableRole::Verse
            || table_roles
                .get(1)
                .is_some_and(|role| *role == DenseTableRole::Verse)
        {
            return Err("LVEC v3 requires verse first and a distinct auxiliary role second".into());
        }
        if embedding_space_id.iter().all(|byte| *byte == 0) {
            return Err("LVEC v3 embedding-space ID must not be zero".into());
        }
        let table_len = verse_count
            .checked_mul(dims)
            .ok_or_else(|| "LVEC v3 dimensions overflow".to_string())?;
        if tables.iter().any(|table| table.len() != table_len) {
            return Err("LVEC v3 table length does not match dimensions".into());
        }
        let body_len = V3_HEADER_LEN
            .checked_add(
                table_len
                    .checked_mul(tables.len())
                    .ok_or_else(|| "LVEC v3 table count overflows".to_string())?,
            )
            .ok_or_else(|| "LVEC v3 body length overflows".to_string())?;
        let mut body = Vec::with_capacity(body_len);
        body.extend_from_slice(MAGIC);
        body.extend_from_slice(&3u16.to_le_bytes());
        body.extend_from_slice(&(dims as u16).to_le_bytes());
        body.extend_from_slice(&(verse_count as u32).to_le_bytes());
        body.push(tables.len() as u8);
        body.push(table_roles[0] as u8);
        body.push(table_roles.get(1).copied().map_or(0, |role| role as u8));
        body.push(0);
        body.extend_from_slice(&embedding_space_id);
        for table in tables {
            body.extend(table.iter().map(|value| *value as u8));
        }
        debug_assert_eq!(body.len(), body_len);
        Ok(body)
    }

    pub fn metadata(&self) -> DenseMetadata {
        self.metadata
    }

    pub fn embedding_space_id(&self) -> Option<[u8; EMBEDDING_SPACE_ID_LEN]> {
        self.metadata.embedding_space_id
    }

    /// Validate a host encoder against this dense pack.
    ///
    /// Version 1 has no compatibility identity and therefore cannot satisfy a
    /// validated query. It remains searchable through [`Self::search`] solely
    /// for backwards compatibility with existing prototype packs.
    pub fn validate_query_space(
        &self,
        embedding_space_id: &[u8; EMBEDDING_SPACE_ID_LEN],
    ) -> Result<(), String> {
        let expected = self.metadata.embedding_space_id.ok_or_else(|| {
            "LVEC v1 has no embedding-space ID and cannot validate the query encoder".to_string()
        })?;
        if expected != *embedding_space_id {
            return Err("query embedding-space ID does not match LVEC".into());
        }
        Ok(())
    }

    /// Legacy search entry point. This deliberately refuses v2/v3 packs so a host
    /// cannot accidentally bypass their embedding-space check.
    pub fn search(&self, embedding: &[f32], top_k: usize) -> Result<Vec<LaneHit>, String> {
        if self.metadata.embedding_space_id.is_some() {
            return Err("LVEC requires an embedding-space ID; use search_in_space".to_string());
        }
        self.search_vector(embedding, top_k)
    }

    /// Search a v2/v3 pack after fail-closed encoder compatibility validation.
    pub fn search_in_space(
        &self,
        embedding_space_id: &[u8; EMBEDDING_SPACE_ID_LEN],
        embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<LaneHit>, String> {
        self.validate_query_space(embedding_space_id)?;
        self.search_vector(embedding, top_k)
    }

    pub fn validate_embedding(&self, embedding: &[f32]) -> Result<(), String> {
        if embedding.len() != self.metadata.dims || embedding.iter().any(|value| !value.is_finite())
        {
            return Err(format!(
                "query embedding has {} dimensions; LVEC requires {} finite values",
                embedding.len(),
                self.metadata.dims
            ));
        }
        let norm_squared = embedding.iter().map(|value| value * value).sum::<f32>();
        if norm_squared <= f32::EPSILON {
            return Err("query embedding must have non-zero norm".into());
        }
        Ok(())
    }

    fn search_vector(&self, embedding: &[f32], top_k: usize) -> Result<Vec<LaneHit>, String> {
        self.validate_embedding(embedding)?;
        let norm = embedding
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        if norm <= f32::EPSILON {
            return Err("query embedding must have non-zero norm".into());
        }
        let query: Vec<f32> = embedding.iter().map(|value| value / norm).collect();
        // Verse and window cosine distributions are not calibrated to each
        // other. Fuse their within-representation ranks instead of taking the
        // largest raw cosine, which would let the higher-variance table win.
        let mut best_by_verse: Vec<Option<(f32, f32, usize)>> =
            vec![None; self.metadata.verse_count];
        for (table_index, table) in self.tables.iter().enumerate() {
            let scored = top_k_for_table(
                table,
                self.metadata.verse_count,
                self.metadata.dims,
                &query,
                top_k.max(1),
            );
            for (rank, candidate) in scored.into_iter().enumerate() {
                let rank_score = 1.0 / (rank as f32 + 1.0);
                match &mut best_by_verse[candidate.verse_index] {
                    Some((best_rank, best_score, best_table)) if rank_score > *best_rank => {
                        *best_rank = rank_score;
                        *best_score = candidate.score;
                        *best_table = table_index;
                    }
                    None => {
                        best_by_verse[candidate.verse_index] =
                            Some((rank_score, candidate.score, table_index));
                    }
                    _ => {}
                }
            }
        }
        let mut scored: Vec<(u32, f32, f32, usize)> = best_by_verse
            .into_iter()
            .enumerate()
            .filter_map(|(verse_index, value)| {
                value.map(|(rank, score, table)| (verse_index as u32, rank, score, table))
            })
            .collect();
        scored.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| right.2.partial_cmp(&left.2).unwrap_or(Ordering::Equal))
                .then_with(|| left.0.cmp(&right.0))
        });
        Ok(scored
            .into_iter()
            .take(top_k.max(1))
            .map(|(verse_index, _, score, table_index)| LaneHit {
                verse_index,
                score: ((score + 1.0) * 0.5).clamp(0.0, 1.0),
                matched_terms: Vec::new(),
                expanded_terms: vec![self.metadata.table_roles[table_index]
                    .evidence_marker()
                    .into()],
                passages: Vec::new(),
                confidence_hint: Confidence::Related,
                ranking_tier: 0,
                direct_tier: 0,
            })
            .collect())
    }
}

/// Return exactly the same ordered prefix as a full deterministic sort while
/// retaining only `limit` candidates during the corpus scan.
fn top_k_for_table(
    table: &[i8],
    verse_count: usize,
    dims: usize,
    query: &[f32],
    limit: usize,
) -> Vec<ScoredVerse> {
    let limit = limit.max(1).min(verse_count);
    let mut heap: BinaryHeap<Reverse<ScoredVerse>> = BinaryHeap::with_capacity(limit);
    for verse_index in 0..verse_count {
        let start = verse_index * dims;
        let score = query
            .iter()
            .zip(&table[start..start + dims])
            .map(|(query_value, packed)| query_value * (*packed as f32 / 127.0))
            .sum::<f32>();
        let candidate = ScoredVerse { verse_index, score };
        if heap.len() < limit {
            heap.push(Reverse(candidate));
        } else if heap.peek().is_some_and(|Reverse(worst)| candidate > *worst) {
            heap.pop();
            heap.push(Reverse(candidate));
        }
    }
    let mut scored: Vec<ScoredVerse> = heap
        .into_iter()
        .map(|Reverse(candidate)| candidate)
        .collect();
    scored.sort_by(|left, right| right.cmp(left));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPACE_ID: [u8; EMBEDDING_SPACE_ID_LEN] = [0x5a; EMBEDDING_SPACE_ID_LEN];

    fn body() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&[0, 0, 0]);
        bytes.extend_from_slice(&[127, 0, 0, 127]);
        bytes
    }

    #[test]
    fn ranks_quantized_cosine() {
        let lane = DenseLane::parse(&body(), 2).unwrap();
        assert_eq!(lane.metadata().version, 1);
        assert_eq!(lane.embedding_space_id(), None);
        let hits = lane.search(&[0.9, 0.1], 2).unwrap();
        assert_eq!(hits[0].verse_index, 0);
        assert_eq!(hits[0].expanded_terms, ["semantic:verse"]);
    }

    #[test]
    fn v2_round_trip_requires_matching_query_space() {
        let table = [127, 0, 0, 127];
        let body = DenseLane::encode_v2(2, 2, &[&table], SPACE_ID).unwrap();
        let lane = DenseLane::parse(&body, 2).unwrap();
        assert_eq!(
            lane.metadata(),
            DenseMetadata {
                version: 2,
                dims: 2,
                verse_count: 2,
                table_count: 1,
                embedding_space_id: Some(SPACE_ID),
                table_roles: [DenseTableRole::Verse, DenseTableRole::Window],
            }
        );
        assert!(lane.search(&[1.0, 0.0], 2).is_err());
        assert!(lane
            .search_in_space(&[0x33; EMBEDDING_SPACE_ID_LEN], &[1.0, 0.0], 2)
            .is_err());
        let hits = lane.search_in_space(&SPACE_ID, &[1.0, 0.0], 2).unwrap();
        assert_eq!(hits[0].verse_index, 0);
    }

    #[test]
    fn rejects_invalid_v2_metadata() {
        let table = [127, 0, 0, 127];
        assert!(DenseLane::encode_v2(2, 2, &[&table], [0; EMBEDDING_SPACE_ID_LEN]).is_err());
        assert!(DenseLane::encode_v2(2, 2, &[&table[..3]], SPACE_ID).is_err());

        let mut body = DenseLane::encode_v2(2, 2, &[&table], SPACE_ID).unwrap();
        body[16..48].fill(0);
        assert!(DenseLane::parse(&body, 2).is_err());
    }

    #[test]
    fn v3_preserves_explicit_study_note_provenance_without_breaking_v2_window_roles() {
        let verse = [127, 0, 0, 127];
        let auxiliary = [0, 127, 127, 0];
        let v2 = DenseLane::parse(
            &DenseLane::encode_v2(2, 2, &[&verse, &auxiliary], SPACE_ID).unwrap(),
            2,
        )
        .unwrap();
        assert_eq!(v2.metadata().table_roles[1], DenseTableRole::Window);
        assert_eq!(
            v2.search_in_space(&SPACE_ID, &[0.0, 1.0], 1).unwrap()[0].expanded_terms,
            ["semantic:window"]
        );

        let v3 = DenseLane::parse(
            &DenseLane::encode_v3(
                2,
                2,
                &[&verse, &auxiliary],
                &[DenseTableRole::Verse, DenseTableRole::StudyNote],
                SPACE_ID,
            )
            .unwrap(),
            2,
        )
        .unwrap();
        assert_eq!(v3.metadata().version, 3);
        assert_eq!(v3.metadata().table_roles[1], DenseTableRole::StudyNote);
        assert_eq!(
            v3.search_in_space(&SPACE_ID, &[0.0, 1.0], 1).unwrap()[0].expanded_terms,
            ["semantic:study-note"]
        );

        let fallback = DenseLane::parse(
            &DenseLane::encode_v3(
                2,
                2,
                &[&verse, &verse],
                &[DenseTableRole::Verse, DenseTableRole::StudyNote],
                SPACE_ID,
            )
            .unwrap(),
            2,
        )
        .unwrap();
        assert_eq!(
            fallback.search_in_space(&SPACE_ID, &[1.0, 0.0], 1).unwrap()[0].expanded_terms,
            ["semantic:verse"],
            "an exact canonical fallback must not masquerade as Study Note evidence"
        );
    }

    #[test]
    fn v3_rejects_role_shape_mismatches() {
        let table = [127, 0, 0, 127];
        assert!(DenseLane::encode_v3(2, 2, &[&table], &[], SPACE_ID).is_err());
        assert!(DenseLane::encode_v3(
            2,
            2,
            &[&table, &table],
            &[DenseTableRole::Verse, DenseTableRole::Verse],
            SPACE_ID
        )
        .is_err());
    }

    #[test]
    fn bounded_top_k_matches_a_full_sort() {
        let verse_count = 513;
        let dims = 7;
        let table: Vec<i8> = (0..verse_count * dims)
            .map(|index| ((index * 37 + index / 11) % 255) as i16 - 127)
            .map(|value| value as i8)
            .collect();
        let query = [0.3, -0.2, 0.1, 0.4, -0.5, 0.6, 0.7];
        let bounded = top_k_for_table(&table, verse_count, dims, &query, 17);
        let mut complete: Vec<ScoredVerse> = (0..verse_count)
            .map(|verse_index| {
                let start = verse_index * dims;
                let score = query
                    .iter()
                    .zip(&table[start..start + dims])
                    .map(|(query_value, packed)| query_value * (*packed as f32 / 127.0))
                    .sum();
                ScoredVerse { verse_index, score }
            })
            .collect();
        complete.sort_by(|left, right| right.cmp(left));
        complete.truncate(17);
        assert_eq!(
            bounded
                .iter()
                .map(|item| (item.verse_index, item.score.to_bits()))
                .collect::<Vec<_>>(),
            complete
                .iter()
                .map(|item| (item.verse_index, item.score.to_bits()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn rejects_shape_and_query_mismatches() {
        let mut truncated = body();
        truncated.pop();
        assert!(DenseLane::parse(&truncated, 2).is_err());
        let lane = DenseLane::parse(&body(), 2).unwrap();
        assert!(lane.search(&[1.0], 2).is_err());
        assert!(lane.search(&[0.0, 0.0], 2).is_err());
    }
}

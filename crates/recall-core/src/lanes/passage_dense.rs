use crate::lanes::traits::LaneHit;
use crate::result::{Confidence, PassageRange};
use recall_pack::{PvecSection, PvecSourceKind, Verse};
use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PassageDenseMetadata {
    pub dims: usize,
    pub canonical_verse_count: usize,
    pub passage_count: usize,
    pub embedding_space_id: [u8; 32],
}

#[derive(Debug)]
struct PassageTarget {
    anchor_index: u32,
    start_ref: String,
    end_ref: String,
    source_kind: PvecSourceKind,
}

#[derive(Clone, Copy, Debug)]
struct ScoredPassage {
    row_index: usize,
    score: f32,
}

impl PartialEq for ScoredPassage {
    fn eq(&self, other: &Self) -> bool {
        self.row_index == other.row_index && self.score.total_cmp(&other.score).is_eq()
    }
}

impl Eq for ScoredPassage {}

impl PartialOrd for ScoredPassage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredPassage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.row_index.cmp(&self.row_index))
    }
}

/// Passage-level semantic retrieval over explicit canonical ranges.
///
/// Query inference remains host-side, exactly as for LVEC. Every result is
/// permanently `Related`, carries its source-typed range, and has no resolver
/// tier; installing this lane cannot manufacture direct-recovery authority.
#[derive(Debug)]
pub struct PassageDenseLane {
    metadata: PassageDenseMetadata,
    targets: Vec<PassageTarget>,
    vectors: Vec<i8>,
}

impl PassageDenseLane {
    pub fn parse(bytes: &[u8], verses: &[Verse]) -> Result<Self, String> {
        let section = PvecSection::parse(bytes).map_err(|error| error.to_string())?;
        if section.canonical_verse_count as usize != verses.len() {
            return Err(format!(
                "PVEC targets {} canonical verses; runtime has {}",
                section.canonical_verse_count,
                verses.len()
            ));
        }
        if verses
            .iter()
            .enumerate()
            .any(|(index, verse)| verse.index as usize != index)
        {
            return Err("canonical verses are not in index order".into());
        }

        let mut targets = Vec::with_capacity(section.rows.len());
        for row in &section.rows {
            let anchor = verses
                .get(row.anchor_index as usize)
                .ok_or_else(|| "PVEC anchor is outside canonical TEXT".to_string())?;
            let start = verses
                .get(row.start_index as usize)
                .ok_or_else(|| "PVEC range start is outside canonical TEXT".to_string())?;
            let end = verses
                .get(row.end_index as usize)
                .ok_or_else(|| "PVEC range end is outside canonical TEXT".to_string())?;
            let anchor_book = reference_book(&anchor.reference)
                .ok_or_else(|| format!("invalid canonical reference {}", anchor.reference))?;
            let start_book = reference_book(&start.reference)
                .ok_or_else(|| format!("invalid canonical reference {}", start.reference))?;
            let end_book = reference_book(&end.reference)
                .ok_or_else(|| format!("invalid canonical reference {}", end.reference))?;
            if anchor_book != start_book || start_book != end_book {
                return Err(format!(
                    "PVEC range {}..{} crosses canonical books",
                    start.reference, end.reference
                ));
            }
            targets.push(PassageTarget {
                anchor_index: row.anchor_index,
                start_ref: start.reference.clone(),
                end_ref: end.reference.clone(),
                source_kind: row.source_kind,
            });
        }

        Ok(Self {
            metadata: PassageDenseMetadata {
                dims: usize::from(section.dims),
                canonical_verse_count: section.canonical_verse_count as usize,
                passage_count: section.rows.len(),
                embedding_space_id: section.embedding_space_id,
            },
            targets,
            vectors: section.vectors,
        })
    }

    pub fn metadata(&self) -> PassageDenseMetadata {
        self.metadata
    }

    pub fn validate_query_space(&self, embedding_space_id: &[u8; 32]) -> Result<(), String> {
        if self.metadata.embedding_space_id != *embedding_space_id {
            return Err("query embedding-space ID does not match PVEC".into());
        }
        Ok(())
    }

    pub fn validate_embedding(&self, embedding: &[f32]) -> Result<(), String> {
        if embedding.len() != self.metadata.dims || embedding.iter().any(|value| !value.is_finite())
        {
            return Err(format!(
                "query embedding has {} dimensions; PVEC requires {} finite values",
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

    pub fn search_in_space(
        &self,
        embedding_space_id: &[u8; 32],
        embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<LaneHit>, String> {
        self.validate_query_space(embedding_space_id)?;
        self.validate_embedding(embedding)?;
        let norm = embedding
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        let query: Vec<f32> = embedding.iter().map(|value| value / norm).collect();
        let limit = top_k.max(1).min(self.targets.len());
        let mut heap: BinaryHeap<Reverse<ScoredPassage>> = BinaryHeap::with_capacity(limit);
        for row_index in 0..self.targets.len() {
            let start = row_index * self.metadata.dims;
            let score = query
                .iter()
                .zip(&self.vectors[start..start + self.metadata.dims])
                .map(|(query_value, packed)| query_value * (*packed as f32 / 127.0))
                .sum::<f32>();
            let candidate = ScoredPassage { row_index, score };
            if heap.len() < limit {
                heap.push(Reverse(candidate));
            } else if heap.peek().is_some_and(|Reverse(worst)| candidate > *worst) {
                heap.pop();
                heap.push(Reverse(candidate));
            }
        }
        let mut scored: Vec<ScoredPassage> = heap
            .into_iter()
            .map(|Reverse(candidate)| candidate)
            .collect();
        scored.sort_by(|left, right| right.cmp(left));

        Ok(scored
            .into_iter()
            .map(|candidate| {
                let target = &self.targets[candidate.row_index];
                LaneHit {
                    verse_index: target.anchor_index,
                    score: ((candidate.score + 1.0) * 0.5).clamp(0.0, 1.0),
                    matched_terms: Vec::new(),
                    expanded_terms: vec![format!(
                        "semantic:passage:{}",
                        target.source_kind.as_str()
                    )],
                    passages: vec![PassageRange {
                        start_ref: target.start_ref.clone(),
                        end_ref: target.end_ref.clone(),
                    }],
                    confidence_hint: Confidence::Related,
                    ranking_tier: 0,
                    direct_tier: 0,
                }
            })
            .collect())
    }
}

fn reference_book(reference: &str) -> Option<&str> {
    let (book, chapter_and_verse) = reference.rsplit_once(' ')?;
    let (chapter, verse) = chapter_and_verse.split_once(':')?;
    if book.is_empty()
        || chapter.is_empty()
        || verse.is_empty()
        || !chapter.bytes().all(|byte| byte.is_ascii_digit())
        || !verse.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(book)
}

#[cfg(test)]
mod tests {
    use super::*;
    use recall_pack::{PvecRow, PvecSection, TextSection};

    const SPACE_ID: [u8; 32] = [0x44; 32];

    fn verses() -> Vec<Verse> {
        TextSection::from_verses(vec![
            ("Genesis 1:1".into(), "one".into()),
            ("Genesis 1:2".into(), "two".into()),
            ("Genesis 1:3".into(), "three".into()),
            ("Exodus 1:1".into(), "four".into()),
        ])
        .verses
    }

    fn body(rows: Vec<PvecRow>, vectors: Vec<i8>) -> Vec<u8> {
        PvecSection::new(2, 4, SPACE_ID, rows, vectors)
            .unwrap()
            .encode()
            .unwrap()
    }

    #[test]
    fn ranks_passages_and_returns_explicit_source_typed_range() {
        let bytes = body(
            vec![
                PvecRow {
                    anchor_index: 1,
                    start_index: 0,
                    end_index: 2,
                    source_kind: PvecSourceKind::StudyBiblePassage,
                },
                PvecRow {
                    anchor_index: 3,
                    start_index: 3,
                    end_index: 3,
                    source_kind: PvecSourceKind::CanonicalPassage,
                },
            ],
            vec![127, 0, 0, 127],
        );
        let lane = PassageDenseLane::parse(&bytes, &verses()).unwrap();
        let hits = lane.search_in_space(&SPACE_ID, &[0.9, 0.1], 2).unwrap();
        assert_eq!(hits[0].verse_index, 1);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
        assert_eq!(hits[0].ranking_tier, 0);
        assert_eq!(hits[0].direct_tier, 0);
        assert_eq!(hits[0].passages[0].start_ref, "Genesis 1:1");
        assert_eq!(hits[0].passages[0].end_ref, "Genesis 1:3");
        assert_eq!(
            hits[0].expanded_terms,
            ["semantic:passage:study-bible-passage"]
        );
    }

    #[test]
    fn rejects_query_space_shape_and_cross_book_ranges() {
        let valid = body(
            vec![PvecRow {
                anchor_index: 1,
                start_index: 0,
                end_index: 2,
                source_kind: PvecSourceKind::StudyBiblePassage,
            }],
            vec![127, 0],
        );
        let lane = PassageDenseLane::parse(&valid, &verses()).unwrap();
        assert!(lane.search_in_space(&[0x55; 32], &[1.0, 0.0], 1).is_err());
        assert!(lane.search_in_space(&SPACE_ID, &[1.0], 1).is_err());
        assert!(lane.search_in_space(&SPACE_ID, &[0.0, 0.0], 1).is_err());

        let cross_book = body(
            vec![PvecRow {
                anchor_index: 2,
                start_index: 2,
                end_index: 3,
                source_kind: PvecSourceKind::StudyBiblePassage,
            }],
            vec![127, 0],
        );
        assert!(PassageDenseLane::parse(&cross_book, &verses())
            .unwrap_err()
            .contains("crosses canonical books"));
    }
}

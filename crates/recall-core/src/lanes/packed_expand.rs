use crate::lanes::traits::{Lane, LaneHit, LaneId};
use crate::lanes::{PackedLexicalLane, WeightedStem};
use crate::normalize::{stem, NormalizedQuery};
use crate::result::Confidence;
use recall_pack::{ExpandView, OwnedExpand, PackError};
use std::sync::Arc;

/// Provider output kept public so callers can choose a contrast-filtered
/// subset of typed stems before asking for packed neighbors.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedExpansionQuery {
    pub weighted_stems: Vec<WeightedStem>,
    pub expanded_terms: Vec<String>,
}

impl PackedExpansionQuery {
    pub fn has_expansions(&self) -> bool {
        !self.expanded_terms.is_empty()
    }
}

/// Lane B backed by a versioned EXPAND section and the compact lexical index.
///
/// The section owns only a stem graph. Scoring is delegated to the shared
/// `PackedLexicalLane`, avoiding a second postings index or TEXT scan.
pub struct PackedExpandLane {
    expand: OwnedExpand,
    lexical: Arc<PackedLexicalLane>,
}

impl PackedExpandLane {
    /// Compatibility constructor that copies a borrowed EXPAND section body.
    pub fn new(
        expand_bytes: &[u8],
        lexical: Arc<PackedLexicalLane>,
        expected_translation: &str,
        expected_verse_count: usize,
    ) -> Result<Self, PackError> {
        Self::new_owned(
            expand_bytes.to_vec(),
            lexical,
            expected_translation,
            expected_verse_count,
        )
    }

    pub fn new_owned(
        expand_bytes: Vec<u8>,
        lexical: Arc<PackedLexicalLane>,
        expected_translation: &str,
        expected_verse_count: usize,
    ) -> Result<Self, PackError> {
        let expand = OwnedExpand::new(expand_bytes)?;
        Self::from_owned_section(expand, lexical, expected_translation, expected_verse_count)
    }

    pub fn from_owned_section(
        expand: OwnedExpand,
        lexical: Arc<PackedLexicalLane>,
        expected_translation: &str,
        expected_verse_count: usize,
    ) -> Result<Self, PackError> {
        let view = expand.view();
        if expected_translation.trim() != expected_translation
            || !view
                .canonical_translation()?
                .eq_ignore_ascii_case(expected_translation)
            || view.canonical_verse_count() as usize != expected_verse_count
            || lexical.document_count() != expected_verse_count
        {
            return Err(PackError::IncompatibleExpand);
        }
        Ok(Self { expand, lexical })
    }

    pub fn canonical_translation(&self) -> &str {
        self.expand
            .view()
            .canonical_translation()
            .expect("OwnedExpand was validated")
    }

    pub fn canonical_verse_count(&self) -> usize {
        self.expand.view().canonical_verse_count() as usize
    }

    /// Expand exactly the supplied normalized stems. Callers may select these
    /// from the full query first (for example, keeping only the positive side
    /// of “rather than” or “instead of”).
    pub fn expand_stems(&self, original_stems: &[String]) -> PackedExpansionQuery {
        expand_stems(self.expand.view(), original_stems)
    }

    /// Run Lane B for an explicitly selected subset of original stems.
    pub fn search_stems(&self, original_stems: &[String], top_k: usize) -> Vec<LaneHit> {
        let expanded = self.expand_stems(original_stems);
        if !expanded.has_expansions() {
            return Vec::new();
        }
        let mut hits = self
            .lexical
            .search_weighted_stems(&expanded.weighted_stems, top_k);
        for hit in &mut hits {
            hit.score *= 0.85;
            hit.expanded_terms = expanded.expanded_terms.clone();
            // Inferred vocabulary is ranking evidence, never exact recovery.
            hit.confidence_hint = Confidence::Related;
            hit.ranking_tier = 0;
            hit.direct_tier = 0;
        }
        hits
    }
}

impl Lane for PackedExpandLane {
    fn id(&self) -> LaneId {
        LaneId::B
    }

    fn available(&self) -> bool {
        self.expand.view().entry_count() > 0 && self.lexical.available()
    }

    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit> {
        let stems = if query.lemmas.is_empty() {
            query.terms.iter().map(|term| stem(term)).collect()
        } else {
            query.lemmas.clone()
        };
        self.search_stems(&stems, top_k)
    }
}

fn expand_stems(view: ExpandView<'_>, original_stems: &[String]) -> PackedExpansionQuery {
    let mut originals = Vec::with_capacity(original_stems.len());
    for stem in original_stems.iter().filter(|stem| stem.len() > 1) {
        if !originals.contains(stem) {
            originals.push(stem.clone());
        }
    }

    let mut weighted_stems: Vec<_> = originals
        .iter()
        .enumerate()
        .map(|(group, source)| {
            WeightedStem::original_in_group(
                source.clone(),
                u16::try_from(group).expect("normalized queries cannot contain 65k terms"),
            )
        })
        .collect();
    let mut expanded_terms = Vec::new();
    for (group, source) in originals.iter().enumerate() {
        let Some(expansions) = view.expansions(source) else {
            continue;
        };
        for expansion in expansions {
            // If another selected source is also this target, its typed weight
            // and provenance must win over inferred evidence.
            if originals.iter().any(|original| original == expansion.stem) {
                continue;
            }
            weighted_stems.push(WeightedStem::expanded_in_group(
                expansion.stem,
                expansion.weight,
                u16::try_from(group).expect("normalized queries cannot contain 65k terms"),
            ));
            if !expanded_terms
                .iter()
                .any(|existing| existing == expansion.stem)
            {
                expanded_terms.push(expansion.stem.to_string());
            }
        }
    }
    PackedExpansionQuery {
        weighted_stems,
        expanded_terms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recall_pack::{
        ExpandEntry, ExpandSection, ExpandTarget, LexSection, LexTerm, PostSection, PostTerm,
        Posting,
    };

    fn lexical() -> Arc<PackedLexicalLane> {
        let lex = LexSection::new(
            2,
            vec![
                LexTerm {
                    text: "rest".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "strength".into(),
                    document_frequency: 1,
                },
            ],
        )
        .unwrap();
        let post = PostSection::new(
            vec![5, 5],
            vec![
                PostTerm {
                    postings: vec![Posting {
                        document_id: 0,
                        term_frequency: 1,
                    }],
                },
                PostTerm {
                    postings: vec![Posting {
                        document_id: 1,
                        term_frequency: 1,
                    }],
                },
            ],
        )
        .unwrap();
        Arc::new(PackedLexicalLane::new_owned(lex.encode(), post.encode(), 2).unwrap())
    }

    fn expand_bytes() -> Vec<u8> {
        ExpandSection::new(
            "BSB",
            2,
            vec![
                ExpandEntry {
                    source: "exhausted".into(),
                    targets: vec![
                        ExpandTarget {
                            stem: "rest".into(),
                            weight: 0.1,
                        },
                        ExpandTarget {
                            stem: "strength".into(),
                            weight: 0.9,
                        },
                    ],
                },
                ExpandEntry {
                    source: "weary".into(),
                    targets: vec![ExpandTarget {
                        stem: "rest".into(),
                        weight: 0.8,
                    }],
                },
            ],
        )
        .unwrap()
        .encode()
    }

    #[test]
    fn provider_expands_only_caller_selected_stems() {
        let lane = PackedExpandLane::new_owned(expand_bytes(), lexical(), "BSB", 2).unwrap();
        let exhausted = lane.expand_stems(&["exhausted".into()]);
        assert_eq!(exhausted.expanded_terms, ["rest", "strength"]);
        assert_eq!(
            exhausted.weighted_stems[0],
            WeightedStem::original_in_group("exhausted", 0)
        );

        let unrelated = lane.expand_stems(&["unknown".into()]);
        assert!(!unrelated.has_expansions());
        assert_eq!(
            unrelated.weighted_stems,
            [WeightedStem::original_in_group("unknown", 0)]
        );
    }

    #[test]
    fn packed_expansion_uses_weights_and_never_claims_recovery() {
        let lane = PackedExpandLane::new_owned(expand_bytes(), lexical(), "BSB", 2).unwrap();
        let hits = lane.search_stems(&["exhausted".into()], 2);
        assert_eq!(hits[0].verse_index, 1);
        assert_eq!(hits[0].expanded_terms, ["rest", "strength"]);
        assert!(hits[0].matched_terms.is_empty());
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
    }

    #[test]
    fn constructor_rejects_corpus_identity_mismatch() {
        let bytes = expand_bytes();
        assert!(matches!(
            PackedExpandLane::new_owned(bytes.clone(), lexical(), "KJV", 2),
            Err(PackError::IncompatibleExpand)
        ));
        assert!(matches!(
            PackedExpandLane::new_owned(bytes, lexical(), "BSB", 3),
            Err(PackError::IncompatibleExpand)
        ));
    }

    #[test]
    fn lane_is_engine_storable() {
        fn assert_engine_storable<T: Send + Sync + 'static>() {}
        assert_engine_storable::<PackedExpandLane>();
    }
}

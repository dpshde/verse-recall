use crate::lanes::traits::{Lane, LaneHit, LaneId};
use crate::normalize::{stem, NormalizedQuery};
use crate::result::Confidence;
use recall_pack::{validate_lex_post, OwnedLex, OwnedPost, PackError, PostView, Verse};
use std::sync::Arc;

const MAX_RECOVERED_CANDIDATES: usize = 3;

/// One lexical query stem with its scoring weight and provenance.
///
/// Original stems participate in coverage and recovery checks. Expanded stems
/// affect ranking only, so inferred vocabulary cannot manufacture apparent
/// query coverage or a `Recovered` claim.
#[derive(Debug, Clone, PartialEq)]
pub struct WeightedStem {
    pub stem: String,
    pub weight: f32,
    pub is_original: bool,
    /// EXPAND terms that originate from one typed stem share a group. A
    /// document receives only the strongest contribution from that group, so
    /// sixty related words cannot masquerade as sixty independent query cues.
    pub expansion_group: Option<u16>,
}

impl WeightedStem {
    pub fn original(stem: impl Into<String>) -> Self {
        Self {
            stem: stem.into(),
            weight: 1.0,
            is_original: true,
            expansion_group: None,
        }
    }

    pub fn expanded(stem: impl Into<String>, weight: f32) -> Self {
        Self {
            stem: stem.into(),
            weight,
            is_original: false,
            expansion_group: None,
        }
    }

    pub fn original_in_group(stem: impl Into<String>, group: u16) -> Self {
        Self {
            stem: stem.into(),
            weight: 1.0,
            is_original: true,
            expansion_group: Some(group),
        }
    }

    pub fn expanded_in_group(stem: impl Into<String>, weight: f32, group: u16) -> Self {
        Self {
            stem: stem.into(),
            weight,
            is_original: false,
            expansion_group: Some(group),
        }
    }
}

/// BM25 over compact owned LEX/POST section bytes.
///
/// Unlike `TextScanLane`, this view owns no verse strings, token strings,
/// document maps, or vocabulary map. A query allocates fixed arrays indexed by
/// canonical verse id and decodes only the postings for its terms.
pub struct PackedLexicalLane {
    lex: OwnedLex,
    post: OwnedPost,
    /// Canonical text is shared with the engine only for contiguous-phrase
    /// verification. The compact index remains the sole lexical score store.
    verses: Option<Arc<[Verse]>>,
    avgdl: f32,
    k1: f32,
    b: f32,
}

impl PackedLexicalLane {
    /// Compatibility constructor that copies borrowed section bytes.
    pub fn new(
        lex_bytes: &[u8],
        post_bytes: &[u8],
        expected_document_count: usize,
    ) -> Result<Self, PackError> {
        Self::new_owned(
            lex_bytes.to_vec(),
            post_bytes.to_vec(),
            expected_document_count,
        )
    }

    /// Consume validated section bodies so the lane is `'static` and can be
    /// stored directly in `Engine` without a self-referential borrow.
    pub fn new_owned(
        lex_bytes: Vec<u8>,
        post_bytes: Vec<u8>,
        expected_document_count: usize,
    ) -> Result<Self, PackError> {
        let lex = OwnedLex::new(lex_bytes)?;
        let post = OwnedPost::new(post_bytes)?;
        let lex_view = lex.view();
        let post_view = post.view();
        validate_lex_post(lex_view, post_view)?;
        if lex_view.document_count() as usize != expected_document_count {
            return Err(PackError::IncompatibleLexPost);
        }
        let total_length: u64 = (0..post_view.document_count())
            .map(|document_id| u64::from(post_view.document_length(document_id).unwrap_or(0)))
            .sum();
        let avgdl = total_length as f32 / post_view.document_count() as f32;
        Ok(Self {
            lex,
            post,
            verses: None,
            avgdl: avgdl.max(1.0),
            k1: 1.2,
            b: 0.75,
        })
    }

    /// Construct the production lane and share canonical TEXT for exact
    /// phrase verification without rebuilding per-document token maps.
    pub fn new_owned_with_verses(
        lex_bytes: Vec<u8>,
        post_bytes: Vec<u8>,
        verses: Arc<[Verse]>,
    ) -> Result<Self, PackError> {
        let mut lane = Self::new_owned(lex_bytes, post_bytes, verses.len())?;
        lane.verses = Some(verses);
        Ok(lane)
    }

    pub fn from_owned_sections(lex: OwnedLex, post: OwnedPost) -> Result<Self, PackError> {
        let lex_view = lex.view();
        let post_view = post.view();
        validate_lex_post(lex_view, post_view)?;
        let total_length: u64 = (0..post_view.document_count())
            .map(|document_id| u64::from(post_view.document_length(document_id).unwrap_or(0)))
            .sum();
        let avgdl = total_length as f32 / post_view.document_count() as f32;
        Ok(Self {
            lex,
            post,
            verses: None,
            avgdl: avgdl.max(1.0),
            k1: 1.2,
            b: 0.75,
        })
    }

    pub fn document_count(&self) -> usize {
        self.lex.view().document_count() as usize
    }

    /// Shared lexical primitive for canonical and expanded query terms.
    pub fn search_stems(&self, stems: &[String], top_k: usize) -> Vec<LaneHit> {
        let weighted: Vec<_> = stems.iter().cloned().map(WeightedStem::original).collect();
        self.search_weighted_stems(&weighted, top_k)
    }

    /// BM25 over typed and inferred stems without allowing inferred terms to
    /// inflate original-query coverage or recovery confidence.
    pub fn search_weighted_stems(&self, stems: &[WeightedStem], top_k: usize) -> Vec<LaneHit> {
        let lex = self.lex.view();
        let post = self.post.view();
        let mut unique_stems: Vec<WeightedQueryStem<'_>> = Vec::with_capacity(stems.len());
        for value in stems.iter().filter(|value| {
            value.stem.len() > 1
                && value.weight.is_finite()
                && value.weight > 0.0
                && value.weight <= 1.0
        }) {
            if let Some(existing) = unique_stems.iter_mut().find(|existing| {
                existing.text == value.stem && existing.expansion_group == value.expansion_group
            }) {
                existing.weight = existing.weight.max(value.weight);
                existing.is_original |= value.is_original;
            } else {
                unique_stems.push(WeightedQueryStem {
                    text: &value.stem,
                    weight: value.weight,
                    is_original: value.is_original,
                    expansion_group: value.expansion_group,
                });
            }
        }
        if unique_stems.is_empty() {
            return Vec::new();
        }

        // Unknown typed stems remain in this denominator, matching the legacy
        // behavior: expansions do not make an incompletely matched query look
        // complete merely because the inferred terms exist in the index.
        let original_term_count = unique_stems.iter().filter(|term| term.is_original).count();

        let known_terms: Vec<QueryTerm<'_>> = unique_stems
            .iter()
            .filter_map(|term| {
                let term_id = lex.lookup(term.text)?;
                let document_frequency = lex.document_frequency(term_id)?;
                Some(QueryTerm {
                    text: term.text,
                    term_id,
                    idf: idf(lex.document_count(), document_frequency),
                    weight: term.weight,
                    is_original: term.is_original,
                    expansion_group: term.expansion_group,
                })
            })
            .collect();
        if known_terms.is_empty() {
            return Vec::new();
        }

        let document_count = self.document_count();
        let mut scores = vec![0.0f32; document_count];
        let mut original_scores = vec![0.0f32; document_count];
        let mut matched_any_counts = vec![0u32; document_count];
        let mut matched_original_counts = vec![0u32; document_count];
        let mut touched = Vec::new();
        let expansion_group_count = known_terms
            .iter()
            .filter_map(|term| term.expansion_group)
            .max()
            .map_or(0usize, |group| usize::from(group) + 1);
        let mut grouped_scores = vec![0.0f32; expansion_group_count * document_count];

        for term in &known_terms {
            let Some(postings) = post.postings(term.term_id) else {
                continue;
            };
            for posting in postings {
                let index = posting.document_id as usize;
                if matched_any_counts[index] == 0 {
                    touched.push(posting.document_id);
                }
                matched_any_counts[index] += 1;
                if term.is_original {
                    matched_original_counts[index] += 1;
                }
                let term_frequency = f32::from(posting.term_frequency);
                let document_length = f32::from(
                    post.document_length(posting.document_id)
                        .expect("POST was validated"),
                );
                let denominator = term_frequency
                    + self.k1 * (1.0 - self.b + self.b * (document_length / self.avgdl));
                // Expanded vocabulary is capped before weighting so a rare,
                // weak neighbor cannot win solely because it occurs once in
                // the corpus. Typed terms retain their full BM25 IDF.
                let effective_idf = if term.is_original {
                    term.idf
                } else {
                    term.idf.min(EXPANDED_IDF_CAP)
                };
                let contribution =
                    effective_idf * (term_frequency * (self.k1 + 1.0)) / denominator * term.weight;
                if let Some(group) = term.expansion_group {
                    let slot = usize::from(group) * document_count + index;
                    grouped_scores[slot] = grouped_scores[slot].max(contribution);
                } else {
                    scores[index] += contribution;
                }
                if term.is_original {
                    original_scores[index] += contribution;
                }
            }
        }

        if expansion_group_count > 0 {
            for document_id in &touched {
                let index = *document_id as usize;
                for group in 0..expansion_group_count {
                    scores[index] += grouped_scores[group * document_count + index];
                }
            }
        }

        let recovery_is_selective = original_term_count > 0
            && touched
                .iter()
                .filter(|document_id| {
                    (matched_original_counts[**document_id as usize] as usize).saturating_mul(5)
                        >= original_term_count.saturating_mul(3)
                })
                .take(MAX_RECOVERED_CANDIDATES + 1)
                .count()
                <= MAX_RECOVERED_CANDIDATES;

        let mut candidates: Vec<(u32, f32, usize, f32)> = touched
            .into_iter()
            .filter_map(|document_id| {
                let index = document_id as usize;
                let matched_original_count = matched_original_counts[index] as usize;
                let mut score = scores[index];
                let mut original_score = original_scores[index];
                if score <= 0.0 || matched_any_counts[index] == 0 {
                    return None;
                }
                if original_term_count > 0 {
                    let coverage = matched_original_count as f32 / original_term_count as f32;
                    score *= 1.0 + coverage;
                    original_score *= 1.0 + coverage;
                    if matched_original_count == original_term_count {
                        score *= 1.25;
                        original_score *= 1.25;
                    }
                }
                Some((document_id, score, matched_original_count, original_score))
            })
            .collect();
        candidates.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(core::cmp::Ordering::Equal)
                .then_with(|| left.0.cmp(&right.0))
        });
        candidates.truncate(top_k.max(1).saturating_mul(5));

        candidates
            .into_iter()
            .map(
                |(document_id, score, matched_original_count, original_score)| {
                    let matched_terms = known_terms
                        .iter()
                        .filter(|term| {
                            term.is_original && posting_contains(post, term.term_id, document_id)
                        })
                        .map(|term| term.text.to_string())
                        .collect();
                    let recovered = recovery_is_selective
                        && original_term_count >= 3
                        && matched_original_count == original_term_count
                        && original_score >= 2.0;
                    LaneHit {
                        verse_index: document_id,
                        score,
                        matched_terms,
                        expanded_terms: Vec::new(),
                        passages: Vec::new(),
                        confidence_hint: if recovered {
                            Confidence::Recovered
                        } else {
                            Confidence::Related
                        },
                        // Contiguous phrase verification remains a separate TEXT
                        // view concern when the engine adopts this foundation.
                        ranking_tier: 0,
                        direct_tier: 0,
                    }
                },
            )
            .collect()
    }
}

impl Lane for PackedLexicalLane {
    fn id(&self) -> LaneId {
        LaneId::A
    }

    fn available(&self) -> bool {
        self.lex.view().document_count() > 0
    }

    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit> {
        let stems = if query.lemmas.is_empty() {
            query.terms.iter().map(|term| stem(term)).collect()
        } else {
            query.lemmas.clone()
        };
        let mut hits = self.search_stems(&stems, top_k);
        let Some(verses) = &self.verses else {
            return hits;
        };

        // LEX/POST is the score store; TEXT is consulted only for the small
        // amount of structure a bag-of-words index cannot prove.
        let query_surface_tokens = surface_tokens(&query.raw);
        let exact_phrase_indices: Vec<u32> = if query_surface_tokens.len() >= 4 {
            verses
                .iter()
                .filter(|verse| {
                    contains_token_window(&surface_tokens(&verse.text), &query_surface_tokens)
                })
                .map(|verse| verse.index)
                .take(MAX_DIRECT_CANONICAL_PHRASE_MATCHES + 1)
                .collect()
        } else {
            Vec::new()
        };
        let phrase_is_selective = !exact_phrase_indices.is_empty()
            && exact_phrase_indices.len() <= MAX_DIRECT_CANONICAL_PHRASE_MATCHES;
        let phrase_is_unique = exact_phrase_indices.len() == 1;
        let raw_lower = query.raw.to_lowercase();
        let raw_core = raw_lower.trim_matches(|c: char| !c.is_alphanumeric());
        let normalized_phrase = query.terms.join(" ");
        let event_claim_requires_concept = ["birth of ", "death of ", "lifetime of "]
            .iter()
            .any(|prefix| raw_lower.trim_start().starts_with(prefix));

        for hit in &mut hits {
            let Some(verse) = verses.get(hit.verse_index as usize) else {
                continue;
            };
            let text_lower = verse.text.to_lowercase();
            if query.terms.len() >= 2
                && !normalized_phrase.is_empty()
                && text_lower.contains(&normalized_phrase)
            {
                hit.score *= 2.0;
            }
            if raw_core.len() >= 8 && text_lower.contains(raw_core) {
                hit.score *= 1.5;
            }

            let exact = phrase_is_selective && exact_phrase_indices.contains(&hit.verse_index);
            if exact {
                hit.ranking_tier = 3;
                if phrase_is_unique {
                    hit.direct_tier = 3;
                    hit.confidence_hint = Confidence::Recovered;
                }
            } else if event_claim_requires_concept && hit.confidence_hint == Confidence::Recovered {
                hit.confidence_hint = Confidence::Related;
            }
        }
        hits.sort_by(|left, right| {
            right
                .ranking_tier
                .cmp(&left.ranking_tier)
                .then_with(|| {
                    right
                        .score
                        .partial_cmp(&left.score)
                        .unwrap_or(core::cmp::Ordering::Equal)
                })
                .then_with(|| left.verse_index.cmp(&right.verse_index))
        });
        hits.truncate(top_k.max(1).saturating_mul(5));
        hits
    }
}

const MAX_DIRECT_CANONICAL_PHRASE_MATCHES: usize = 3;

fn surface_tokens(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            current.push(character);
        } else if matches!(character, '\'' | '\u{2018}' | '\u{2019}') && !current.is_empty() {
            continue;
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn contains_token_window(haystack: &[String], needle: &[String]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

struct QueryTerm<'a> {
    text: &'a str,
    term_id: u32,
    idf: f32,
    weight: f32,
    is_original: bool,
    expansion_group: Option<u16>,
}

struct WeightedQueryStem<'a> {
    text: &'a str,
    weight: f32,
    is_original: bool,
    expansion_group: Option<u16>,
}

const EXPANDED_IDF_CAP: f32 = 4.0;

fn idf(document_count: u32, document_frequency: u32) -> f32 {
    let n = document_count as f32;
    let df = document_frequency as f32;
    ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0)
}

fn posting_contains(post: PostView<'_>, term_id: u32, document_id: u32) -> bool {
    post.postings(term_id)
        .is_some_and(|mut postings| postings.any(|posting| posting.document_id == document_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use recall_pack::{LexSection, LexTerm, PostSection, PostTerm, Posting};

    fn fixture() -> (Vec<u8>, Vec<u8>) {
        let lex = LexSection::new(
            3,
            vec![
                LexTerm {
                    text: "heart".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "lord".into(),
                    document_frequency: 2,
                },
                LexTerm {
                    text: "sheep".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "shepherd".into(),
                    document_frequency: 2,
                },
                LexTerm {
                    text: "trust".into(),
                    document_frequency: 1,
                },
            ],
        )
        .unwrap();
        let post = PostSection::new(
            vec![5, 9, 6],
            vec![
                PostTerm {
                    postings: vec![Posting {
                        document_id: 1,
                        term_frequency: 1,
                    }],
                },
                PostTerm {
                    postings: vec![
                        Posting {
                            document_id: 0,
                            term_frequency: 1,
                        },
                        Posting {
                            document_id: 1,
                            term_frequency: 1,
                        },
                    ],
                },
                PostTerm {
                    postings: vec![Posting {
                        document_id: 2,
                        term_frequency: 1,
                    }],
                },
                PostTerm {
                    postings: vec![
                        Posting {
                            document_id: 0,
                            term_frequency: 1,
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
                        term_frequency: 1,
                    }],
                },
            ],
        )
        .unwrap();
        (lex.encode(), post.encode())
    }

    fn weighted_fixture() -> (Vec<u8>, Vec<u8>) {
        let lex = LexSection::new(
            2,
            vec![
                LexTerm {
                    text: "anchor".into(),
                    document_frequency: 2,
                },
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
                    postings: vec![
                        Posting {
                            document_id: 0,
                            term_frequency: 1,
                        },
                        Posting {
                            document_id: 1,
                            term_frequency: 1,
                        },
                    ],
                },
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
        (lex.encode(), post.encode())
    }

    #[test]
    fn bm25_ranks_from_compact_owned_postings() {
        let (lex, post) = fixture();
        let lane = PackedLexicalLane::new(&lex, &post, 3).unwrap();
        let query = NormalizedQuery {
            raw: "lord shepherd".into(),
            terms: vec!["lord".into(), "shepherd".into()],
            lemmas: vec!["lord".into(), "shepherd".into()],
        };
        let hits = lane.search(&query, 3);
        assert_eq!(hits[0].verse_index, 0);
        assert_eq!(hits[0].matched_terms, ["lord", "shepherd"]);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
    }

    #[test]
    fn selective_three_term_match_may_recover() {
        let (lex, post) = fixture();
        let lane = PackedLexicalLane::new(&lex, &post, 3).unwrap();
        let hits = lane.search_stems(&["trust".into(), "lord".into(), "heart".into()], 3);
        assert_eq!(hits[0].verse_index, 1);
        assert_eq!(hits[0].confidence_hint, Confidence::Recovered);
    }

    #[test]
    fn expansion_weights_change_ranking_without_changing_typed_coverage() {
        let (lex, post) = weighted_fixture();
        let lane = PackedLexicalLane::new(&lex, &post, 2).unwrap();
        let hits = lane.search_weighted_stems(
            &[
                WeightedStem::original("anchor"),
                WeightedStem::expanded("rest", 0.1),
                WeightedStem::expanded("strength", 0.9),
            ],
            2,
        );
        assert_eq!(hits[0].verse_index, 1);
        assert_eq!(hits[0].matched_terms, ["anchor"]);
        assert_eq!(hits[1].verse_index, 0);
        assert_eq!(hits[1].matched_terms, ["anchor"]);
    }

    #[test]
    fn inferred_matches_cannot_complete_or_recover_original_query() {
        let (lex, post) = fixture();
        let lane = PackedLexicalLane::new(&lex, &post, 3).unwrap();
        let hits = lane.search_weighted_stems(
            &[
                WeightedStem::original("trust"),
                WeightedStem::original("lord"),
                WeightedStem::original("missing"),
                WeightedStem::expanded("heart", 1.0),
            ],
            3,
        );
        assert_eq!(hits[0].verse_index, 1);
        assert_eq!(hits[0].matched_terms, ["trust", "lord"]);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
    }

    #[test]
    fn one_source_expansion_contributes_at_most_one_semantic_cue() {
        let lex = LexSection::new(
            2,
            vec![
                LexTerm {
                    text: "alpha".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "beta".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "gamma".into(),
                    document_frequency: 1,
                },
            ],
        )
        .unwrap();
        let post = PostSection::new(
            vec![2, 2],
            vec![
                PostTerm {
                    postings: vec![Posting {
                        document_id: 0,
                        term_frequency: 1,
                    }],
                },
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
        let lane = PackedLexicalLane::new_owned(lex.encode(), post.encode(), 2).unwrap();
        let hits = lane.search_weighted_stems(
            &[
                WeightedStem::original_in_group("unknown", 0),
                WeightedStem::expanded_in_group("alpha", 0.6, 0),
                WeightedStem::expanded_in_group("beta", 0.6, 0),
                WeightedStem::expanded_in_group("gamma", 0.9, 0),
            ],
            2,
        );
        assert_eq!(hits[0].verse_index, 1);
        assert_eq!(hits[1].verse_index, 0);
    }

    #[test]
    fn rejects_wrong_canonical_document_count() {
        let (lex, post) = fixture();
        assert!(matches!(
            PackedLexicalLane::new(&lex, &post, 4),
            Err(PackError::IncompatibleLexPost)
        ));
    }

    #[test]
    fn lane_is_engine_storable() {
        fn assert_engine_storable<T: Send + Sync + 'static>() {}
        assert_engine_storable::<PackedLexicalLane>();

        let (lex, post) = fixture();
        let lane = PackedLexicalLane::new_owned(lex, post, 3).unwrap();
        assert_eq!(lane.document_count(), 3);
    }
}

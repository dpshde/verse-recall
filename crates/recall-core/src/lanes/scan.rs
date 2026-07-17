use crate::lanes::traits::{Lane, LaneHit, LaneId};
use crate::normalize::{stem, tokenize_text, NormalizedQuery};
use crate::result::Confidence;
use recall_pack::Verse;
use std::collections::HashMap;
use std::sync::Arc;

const MAX_RECOVERED_CANDIDATES: usize = 3;
const MAX_DIRECT_CANONICAL_PHRASE_MATCHES: usize = 3;

/// Lane A: BM25 over TEXT (computed at open when POST/LEX are absent).
///
/// Uses light stems for term identity and IDF from the full corpus so common
/// words like "lord" do not dominate distinctive fragments like "shepherd".
pub struct TextScanLane {
    verses: Arc<[Verse]>,
    /// Per-verse bag of stemmed tokens (with raw tf).
    docs: Vec<HashMap<String, u32>>,
    /// Document frequency per stem.
    df: HashMap<String, u32>,
    /// Average document length (token count).
    avgdl: f32,
    n_docs: u32,
    k1: f32,
    b: f32,
}

impl TextScanLane {
    pub fn new(verses: impl Into<Arc<[Verse]>>) -> Self {
        let verses = verses.into();
        let mut docs: Vec<HashMap<String, u32>> = Vec::with_capacity(verses.len());
        let mut df: HashMap<String, u32> = HashMap::new();
        let mut total_len = 0u64;

        for v in verses.iter() {
            let tokens = tokenize_text(&v.text);
            total_len += tokens.len() as u64;
            let mut bag: HashMap<String, u32> = HashMap::new();
            for t in tokens {
                let s = stem(&t);
                *bag.entry(s).or_insert(0) += 1;
            }
            for term in bag.keys() {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
            docs.push(bag);
        }

        let n_docs = verses.len() as u32;
        let avgdl = if n_docs > 0 {
            total_len as f32 / n_docs as f32
        } else {
            1.0
        };

        Self {
            verses,
            docs,
            df,
            avgdl: avgdl.max(1.0),
            n_docs,
            k1: 1.2,
            b: 0.75,
        }
    }

    fn idf(&self, term: &str) -> f32 {
        let df = *self.df.get(term).unwrap_or(&0) as f32;
        // Lucene-style IDF with floor so unknown query terms still contribute weakly.
        let n = self.n_docs as f32;
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.0)
    }

    fn bm25_doc(&self, doc_idx: usize, query_stems: &[String]) -> (f32, Vec<String>) {
        let bag = &self.docs[doc_idx];
        let dl = bag.values().sum::<u32>() as f32;
        let mut score = 0.0f32;
        let mut matched = Vec::new();

        for stem_t in query_stems {
            let tf = match bag.get(stem_t) {
                Some(&c) => c as f32,
                None => continue,
            };
            let idf = self.idf(stem_t);
            if idf <= 0.0 && tf <= 0.0 {
                continue;
            }
            let denom = tf + self.k1 * (1.0 - self.b + self.b * (dl / self.avgdl));
            score += idf * (tf * (self.k1 + 1.0)) / denom;
            if !matched.iter().any(|m| m == stem_t) {
                matched.push(stem_t.clone());
            }
        }

        // Phrase / multi-term coverage bonus: prefer verses that cover more of the query.
        if !query_stems.is_empty() {
            let coverage = matched.len() as f32 / query_stems.len() as f32;
            score *= 1.0 + coverage;
            if matched.len() == query_stems.len() {
                score *= 1.25;
            }
        }

        // Exact surface phrase bonus against original verse text (case-insensitive).
        // Helps "the Lord is my shepherd" outrank scattered lord+shepherd hits.
        (score, matched)
    }
}

impl Lane for TextScanLane {
    fn id(&self) -> LaneId {
        LaneId::A
    }

    fn available(&self) -> bool {
        !self.verses.is_empty()
    }

    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit> {
        let mut stems: Vec<String> = query
            .lemmas
            .iter()
            .filter(|t| t.len() > 1)
            .cloned()
            .collect();
        // Fall back to surface stems if lemmas empty.
        if stems.is_empty() {
            stems = query.terms.iter().map(|t| stem(t)).collect();
        }
        if stems.is_empty() {
            return Vec::new();
        }

        // Dedup stems preserving order.
        let mut seen = std::collections::HashSet::new();
        stems.retain(|t| seen.insert(t.clone()));

        // `Recovered` means the query identifies a small candidate set, not
        // merely that a generic phrase has good token coverage. Formulaic
        // language and single-keyword queries remain Related.
        let recovery_candidate_count = self
            .docs
            .iter()
            .filter(|bag| {
                let matches = stems.iter().filter(|stem| bag.contains_key(*stem)).count();
                matches * 5 >= stems.len() * 3
            })
            .take(MAX_RECOVERED_CANDIDATES + 1)
            .count();
        let recovery_is_selective = recovery_candidate_count <= MAX_RECOVERED_CANDIDATES;
        let query_surface_tokens = surface_tokens(&query.raw);
        let exact_phrase_indices: Vec<usize> = if query_surface_tokens.len() >= 4 {
            self.verses
                .iter()
                .enumerate()
                .filter(|(_, verse)| {
                    contains_token_window(&surface_tokens(&verse.text), &query_surface_tokens)
                })
                .map(|(index, _)| index)
                .take(MAX_DIRECT_CANONICAL_PHRASE_MATCHES + 1)
                .collect()
        } else {
            Vec::new()
        };
        let canonical_phrase_is_selective = !exact_phrase_indices.is_empty()
            && exact_phrase_indices.len() <= MAX_DIRECT_CANONICAL_PHRASE_MATCHES;
        let canonical_phrase_is_unique = exact_phrase_indices.len() == 1;

        let raw_l = query.raw.to_lowercase();
        let event_claim_requires_concept = ["birth of ", "death of ", "lifetime of "]
            .iter()
            .any(|prefix| raw_l.trim_start().starts_with(prefix));
        let mut hits: Vec<LaneHit> = Vec::new();

        for (i, v) in self.verses.iter().enumerate() {
            let (mut score, matched) = self.bm25_doc(i, &stems);
            if matched.is_empty() || score <= 0.0 {
                continue;
            }

            let hay = v.text.to_lowercase();
            // Contiguous multi-word fragment bonus from the raw query (stop words ok).
            if query.terms.len() >= 2 {
                // Build a short phrase from consecutive content words if present as substring.
                let phrase = query.terms.join(" ");
                if hay.contains(&phrase) {
                    score *= 2.0;
                }
            }
            // Full raw fragment without leading/trailing noise.
            let raw_core = raw_l
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string();
            if raw_core.len() >= 8 && hay.contains(&raw_core) {
                score *= 1.5;
            }

            let coverage = matched.len() as f32 / stems.len() as f32;
            let is_exact_canonical_phrase =
                canonical_phrase_is_selective && exact_phrase_indices.contains(&i);
            let exact_canonical_recovery = is_exact_canonical_phrase && canonical_phrase_is_unique;
            let selective_lexical_recovery = recovery_is_selective
                && !event_claim_requires_concept
                // Two-word lexical matches are too easy to satisfy in an
                // unrelated verse (for example, `tabernacle built`). Let a
                // verified phrase/concept lane recover those; scan-only
                // evidence needs at least three independent query stems.
                && stems.len() >= 3
                // A bag of topical words is not a recovered quotation. Scan
                // may claim recovery only when every normalized query stem is
                // present; phrase/concept lanes handle stronger structure.
                && coverage >= 0.99
                && score >= 2.0;
            let confidence = if exact_canonical_recovery || selective_lexical_recovery {
                Confidence::Recovered
            } else {
                Confidence::Related
            };

            hits.push(LaneHit {
                verse_index: v.index,
                score,
                matched_terms: matched,
                expanded_terms: Vec::new(),
                passages: Vec::new(),
                confidence_hint: confidence,
                // Exact selective canonical text outranks hidden-translation
                // recall, preserving quote precision on cross-version
                // collisions. Repeated canonical phrases rank but stay
                // Related; unique ones may claim direct recovery.
                ranking_tier: if is_exact_canonical_phrase { 3 } else { 0 },
                direct_tier: if is_exact_canonical_phrase && canonical_phrase_is_unique {
                    3
                } else {
                    0
                },
            });
        }

        hits.sort_by(|a, b| {
            b.ranking_tier
                .cmp(&a.ranking_tier)
                .then_with(|| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(core::cmp::Ordering::Equal)
                })
                .then_with(|| a.verse_index.cmp(&b.verse_index))
        });
        hits.truncate(top_k.max(1) * 5);
        hits
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalize::normalize_query;

    #[test]
    fn ambiguous_surface_matches_are_related() {
        let verses: Vec<Verse> = (0..4)
            .map(|index| Verse {
                index,
                reference: format!("Genesis 1:{}", index + 1),
                text: "And it came to pass in those days.".into(),
            })
            .collect();
        let lane = TextScanLane::new(verses);
        let hits = lane.search(&normalize_query("and it came to pass"), 10);

        assert_eq!(hits.len(), 4);
        assert!(hits
            .iter()
            .all(|hit| hit.confidence_hint == Confidence::Related));
    }

    #[test]
    fn event_shaped_noun_phrase_needs_verified_concept_for_recovery() {
        let lane = TextScanLane::new(vec![Verse {
            index: 0,
            reference: "Job 3:1".into(),
            text: "After this, Job opened his mouth and cursed the day of his birth.".into(),
        }]);
        let hits = lane.search(&normalize_query("birth of Job"), 3);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
    }

    #[test]
    fn selective_two_term_surface_match_is_still_related() {
        let lane = TextScanLane::new(vec![Verse {
            index: 0,
            reference: "1 Chronicles 6:32".into(),
            text: "They ministered before the tabernacle until Solomon built the house.".into(),
        }]);
        let hits = lane.search(&normalize_query("tabernacle built"), 3);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
    }

    #[test]
    fn partial_topical_bag_match_is_not_recovered() {
        let lane = TextScanLane::new(vec![Verse {
            index: 0,
            reference: "Psalm 29:11".into(),
            text: "The LORD gives His people strength and blesses them with peace.".into(),
        }]);
        let hits = lane.search(&normalize_query("anxiety strength peace"), 3);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
    }

    #[test]
    fn exact_surface_tier_survives_many_higher_bm25_distractors() {
        let mut verses = vec![Verse {
            index: 0,
            reference: "Genesis 1:1".into(),
            text: "Alpha and the of.".into(),
        }];
        verses.extend((1..=700).map(|index| Verse {
            index,
            reference: format!("Genesis 1:{}", index + 1),
            text: "Alpha alpha alpha alpha alpha alpha.".into(),
        }));
        let lane = TextScanLane::new(verses);
        let hits = lane.search(&normalize_query("alpha and the of"), 1);

        assert_eq!(hits[0].verse_index, 0);
        assert_eq!(hits[0].ranking_tier, 3);
        assert_eq!(hits[0].confidence_hint, Confidence::Recovered);
    }
}

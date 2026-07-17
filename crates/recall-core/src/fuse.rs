use crate::lanes::{LaneHit, LaneId, LaneScores};
use crate::result::{Confidence, PassageRange, SearchResult};
use crate::types::SearchOpts;
use recall_pack::Verse;
use std::collections::{HashMap, HashSet};

/// Fusion weights (RRF + agreement). Zeros in META mean use these defaults.
#[derive(Debug, Clone, Copy)]
pub struct FuseWeights {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub agreement_bonus: f32,
    pub rrf_k: f32,
    pub book_prior_max: f32,
}

impl Default for FuseWeights {
    fn default() -> Self {
        Self {
            a: 1.0,
            b: 0.55,
            c: 0.75,
            d: 1.25,
            agreement_bonus: 0.15,
            rrf_k: 60.0,
            book_prior_max: 1.15,
        }
    }
}

impl FuseWeights {
    pub fn weight(&self, id: LaneId) -> f32 {
        match id {
            LaneId::A => self.a,
            LaneId::B => self.b,
            LaneId::C => self.c,
            LaneId::D => self.d,
        }
    }
}

struct Acc {
    scores: LaneScores,
    matched: Vec<String>,
    expanded: Vec<String>,
    passages: Vec<PassageRange>,
    confidence: Confidence,
    rrf: f32,
    ranking_tier: u8,
}

#[derive(Default)]
struct ShadowEvidence {
    matched: Vec<String>,
    expanded: Vec<String>,
    passages: Vec<PassageRange>,
}

/// Fuse per-lane ranked lists into final results.
pub fn fuse_lanes(
    lane_lists: &[(LaneId, Vec<LaneHit>)],
    verses: &[Verse],
    opts: &SearchOpts,
    weights: FuseWeights,
) -> Vec<SearchResult> {
    let reference_indices: HashMap<&str, u32> = verses
        .iter()
        .map(|verse| (verse.reference.as_str(), verse.index))
        .collect();
    let lane_lists = promote_semantic_topic_agreement(lane_lists, &reference_indices);
    // Score zero is the explicit evidence-only contract.  Preserve its
    // explanation metadata, but remove it before consolidation/RRF so a
    // heuristic topic association can neither create nor promote a result.
    let mut shadow_by_verse: HashMap<u32, ShadowEvidence> = HashMap::new();
    let scored_lane_lists: Vec<(LaneId, Vec<LaneHit>)> = lane_lists
        .iter()
        .map(|(lane_id, hits)| {
            let scored = hits
                .iter()
                .filter_map(|hit| {
                    if hit.score > 0.0 {
                        return Some(hit.clone());
                    }
                    let evidence = shadow_by_verse.entry(hit.verse_index).or_default();
                    merge_terms(&mut evidence.matched, &hit.matched_terms);
                    merge_terms(&mut evidence.expanded, &hit.expanded_terms);
                    merge_passages(&mut evidence.passages, &hit.passages);
                    None
                })
                .collect();
            (*lane_id, scored)
        })
        .collect();
    let lane_lists = consolidate_lane_lists(&scored_lane_lists);
    let mut map: HashMap<u32, Acc> = HashMap::new();

    for (lane_id, hits) in &lane_lists {
        let w = weights.weight(*lane_id);
        if w <= 0.0 {
            continue;
        }
        for (rank, hit) in hits.iter().enumerate() {
            let rrf = w / (weights.rrf_k + rank as f32 + 1.0);
            let entry = map.entry(hit.verse_index).or_insert_with(|| Acc {
                scores: LaneScores::default(),
                matched: Vec::new(),
                expanded: Vec::new(),
                passages: Vec::new(),
                confidence: Confidence::Related,
                rrf: 0.0,
                ranking_tier: 0,
            });
            entry.rrf += rrf;
            // Keep max raw lane score for evidence.
            if hit.score > entry.scores.get(*lane_id) {
                entry.scores.set(*lane_id, hit.score);
            }
            merge_terms(&mut entry.matched, &hit.matched_terms);
            merge_terms(&mut entry.expanded, &hit.expanded_terms);
            merge_passages(&mut entry.passages, &hit.passages);
            entry.confidence = stronger_confidence(entry.confidence, hit.confidence_hint);
            entry.ranking_tier = entry.ranking_tier.max(hit.ranking_tier);
        }
    }

    for (anchor_index, evidence) in shadow_by_verse {
        let matching_scored_indices: Vec<u32> = map
            .keys()
            .copied()
            .filter(|verse_index| {
                *verse_index == anchor_index
                    || evidence.passages.iter().any(|passage| {
                        let Some(start) = reference_indices.get(passage.start_ref.as_str()) else {
                            return false;
                        };
                        let Some(end) = reference_indices.get(passage.end_ref.as_str()) else {
                            return false;
                        };
                        start <= verse_index && verse_index <= end
                    })
            })
            .collect();
        for verse_index in matching_scored_indices {
            let Some(entry) = map.get_mut(&verse_index) else {
                continue;
            };
            merge_terms(&mut entry.matched, &evidence.matched);
            // Shadow evidence may explain a lexical hit, but it must never
            // widen or contradict an already structured curated/event answer.
            if entry.passages.is_empty() {
                merge_terms(&mut entry.expanded, &evidence.expanded);
                merge_passages(&mut entry.passages, &evidence.passages);
            } else {
                let non_passage_evidence: Vec<String> = evidence
                    .expanded
                    .iter()
                    .filter(|term| !term.starts_with("passage:"))
                    .cloned()
                    .collect();
                merge_terms(&mut entry.expanded, &non_passage_evidence);
            }
        }
    }

    // Suppress lone weak semantic (lane C only, low score) — scaffold rule.
    map.retain(|_, acc| {
        let only_c =
            acc.scores.a == 0.0 && acc.scores.b == 0.0 && acc.scores.d == 0.0 && acc.scores.c > 0.0;
        !(only_c && acc.scores.c < 0.35)
    });

    let mut results: Vec<SearchResult> = map
        .into_iter()
        .filter_map(|(idx, mut acc)| {
            let verse = verses.iter().find(|v| v.index == idx)?;
            let agree = acc.scores.agreeing_lanes();
            if agree > 1 {
                acc.rrf *= 1.0 + weights.agreement_bonus * (agree as f32 - 1.0);
            }
            if let Some(hint) = &opts.book_hint {
                if book_matches(&verse.reference, hint) {
                    acc.rrf *= weights.book_prior_max;
                }
            }
            let chips = acc.matched.clone();
            // The integer component is the documented direct-resolution tier;
            // fractional RRF breaks ties inside a tier. Folding it into the
            // public score keeps the returned order honest and monotonic.
            let score = acc.ranking_tier as f32 + acc.rrf;
            Some(SearchResult {
                reference: verse.reference.clone(),
                text: verse.text.clone(),
                score,
                confidence: acc.confidence,
                lanes: acc.scores,
                matched_terms: acc.matched,
                expanded_terms: acc.expanded,
                passages: acc.passages,
                chips,
                verse_index: idx,
            })
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(core::cmp::Ordering::Equal)
            .then_with(|| a.verse_index.cmp(&b.verse_index))
    });
    results.truncate(opts.top_k.max(1));
    results
}

/// Promote a strict topic bridge only when an independent top dense result
/// lands inside its validated passage. This is stronger than either heuristic
/// alone while remaining `Related`: resolver priority never becomes a direct
/// recovery claim.
fn promote_semantic_topic_agreement(
    lane_lists: &[(LaneId, Vec<LaneHit>)],
    reference_indices: &HashMap<&str, u32>,
) -> Vec<(LaneId, Vec<LaneHit>)> {
    const AGREEMENT_DEPTH: usize = 10;
    // The topic graph is heuristic. Dense agreement may grant resolver
    // priority only to strongly supported graph matches; weaker topics remain
    // score-zero explanation metadata.
    const MIN_TOPIC_AGREEMENT_SCORE: f32 = 0.76;

    let mut dense_indices = HashSet::new();
    for (_, hits) in lane_lists
        .iter()
        .filter(|(lane_id, _)| *lane_id == LaneId::C)
    {
        dense_indices.extend(
            hits.iter()
                .filter(|hit| {
                    hit.expanded_terms
                        .iter()
                        .any(|term| term.starts_with("semantic:"))
                })
                .take(AGREEMENT_DEPTH)
                .map(|hit| hit.verse_index),
        );
    }

    let mut promoted = lane_lists.to_vec();
    for (_, hits) in promoted
        .iter_mut()
        .filter(|(lane_id, _)| *lane_id == LaneId::C)
    {
        for hit in hits.iter_mut().filter(|hit| {
            hit.expanded_terms
                .iter()
                .any(|term| term.starts_with("semantic-topic:"))
        }) {
            let agrees = dense_indices.contains(&hit.verse_index)
                || hit.passages.iter().any(|passage| {
                    let Some(start) = reference_indices.get(passage.start_ref.as_str()) else {
                        return false;
                    };
                    let Some(end) = reference_indices.get(passage.end_ref.as_str()) else {
                        return false;
                    };
                    dense_indices
                        .iter()
                        .any(|verse_index| start <= verse_index && verse_index <= end)
                });
            if hit.score >= MIN_TOPIC_AGREEMENT_SCORE && agrees {
                hit.ranking_tier = hit.ranking_tier.max(1);
                let evidence = "semantic-agreement:dense+topic".to_string();
                if !hit.expanded_terms.contains(&evidence) {
                    hit.expanded_terms.push(evidence);
                }
            } else {
                // The topic bridge is derived from the same packed-neighbor
                // graph as Lane B, so it is not an independent retrieval vote.
                // Without top-dense agreement it remains score-zero metadata:
                // useful for explaining an independently retrieved anchor,
                // but unable to create a candidate or manufacture cross-lane
                // agreement from two views of one expansion source.
                hit.score = 0.0;
                hit.ranking_tier = 0;
                hit.direct_tier = 0;
            }
        }
    }
    promoted
}

/// Multiple implementations may contribute to the same conceptual lane
/// (static expansion + alternate text in B, built-in phrase memory + concepts
/// in D). Collapse them before RRF so one lane cannot cast duplicate votes or
/// manufacture cross-lane agreement.
fn consolidate_lane_lists(lane_lists: &[(LaneId, Vec<LaneHit>)]) -> Vec<(LaneId, Vec<LaneHit>)> {
    let mut consolidated = Vec::new();
    for lane_id in [LaneId::A, LaneId::B, LaneId::C, LaneId::D] {
        let mut by_verse: HashMap<u32, (LaneHit, f32)> = HashMap::new();
        for (_, hits) in lane_lists.iter().filter(|(id, _)| *id == lane_id) {
            for (rank, hit) in hits.iter().enumerate() {
                // Raw BM25/phrase scores from different corpora and
                // implementations are not comparable. Use only normalized
                // within-source rank to order the consolidated conceptual lane.
                // Max (not sum) prevents two implementations of one lane from
                // manufacturing an extra vote.
                let rank_score = 1.0 / (rank as f32 + 1.0);
                match by_verse.get_mut(&hit.verse_index) {
                    Some((existing, existing_rank_score)) => {
                        let tier_order = hit.ranking_tier.cmp(&existing.ranking_tier);
                        existing.score = existing.score.max(hit.score);
                        *existing_rank_score = existing_rank_score.max(rank_score);
                        merge_terms(&mut existing.matched_terms, &hit.matched_terms);
                        match tier_order {
                            core::cmp::Ordering::Greater => {
                                // A reviewed/direct mapping owns structured
                                // output over a lower-trust hit at the same
                                // anchor. Keep non-passage provenance from the
                                // lower tier, but replace its range/confidence.
                                existing
                                    .expanded_terms
                                    .retain(|term| !term.starts_with("passage:"));
                                merge_terms(&mut existing.expanded_terms, &hit.expanded_terms);
                                existing.passages.clone_from(&hit.passages);
                                existing.confidence_hint = hit.confidence_hint;
                                existing.direct_tier = hit.direct_tier;
                            }
                            core::cmp::Ordering::Equal => {
                                merge_terms(&mut existing.expanded_terms, &hit.expanded_terms);
                                merge_passages(&mut existing.passages, &hit.passages);
                                existing.confidence_hint = stronger_confidence(
                                    existing.confidence_hint,
                                    hit.confidence_hint,
                                );
                                existing.direct_tier = existing.direct_tier.max(hit.direct_tier);
                            }
                            core::cmp::Ordering::Less => {
                                let non_passage_terms: Vec<String> = hit
                                    .expanded_terms
                                    .iter()
                                    .filter(|term| !term.starts_with("passage:"))
                                    .cloned()
                                    .collect();
                                merge_terms(&mut existing.expanded_terms, &non_passage_terms);
                            }
                        }
                        existing.ranking_tier = existing.ranking_tier.max(hit.ranking_tier);
                    }
                    None => {
                        by_verse.insert(hit.verse_index, (hit.clone(), rank_score));
                    }
                }
            }
        }
        if by_verse.is_empty() {
            continue;
        }
        let mut ranked_hits: Vec<(LaneHit, f32)> = by_verse.into_values().collect();
        ranked_hits.sort_by(|(a, a_rank), (b, b_rank)| {
            b.ranking_tier
                .cmp(&a.ranking_tier)
                .then_with(|| {
                    b_rank
                        .partial_cmp(a_rank)
                        .unwrap_or(core::cmp::Ordering::Equal)
                })
                .then_with(|| a.verse_index.cmp(&b.verse_index))
        });
        let hits = ranked_hits.into_iter().map(|(hit, _)| hit).collect();
        consolidated.push((lane_id, hits));
    }
    consolidated
}

fn merge_terms(dst: &mut Vec<String>, src: &[String]) {
    for t in src {
        if !dst.iter().any(|x| x == t) {
            dst.push(t.clone());
        }
    }
}

fn merge_passages(dst: &mut Vec<PassageRange>, src: &[PassageRange]) {
    for passage in src {
        if !dst.iter().any(|existing| existing == passage) {
            dst.push(passage.clone());
        }
    }
}

fn stronger_confidence(a: Confidence, b: Confidence) -> Confidence {
    use Confidence::*;
    match (a, b) {
        (OftenMisremembered, _) | (_, OftenMisremembered) => OftenMisremembered,
        (Recovered, _) | (_, Recovered) => Recovered,
        _ => Related,
    }
}

fn book_matches(reference: &str, hint: &str) -> bool {
    let Some(reference_book) = reference_book(reference).and_then(canonical_book) else {
        return false;
    };
    let normalized_hint = hint.trim().to_lowercase();
    if normalized_hint.is_empty() {
        return false;
    }

    let h = normalized_hint.as_str();
    if h == "nt" || h == "new testament" {
        return NEW_TESTAMENT_BOOKS.contains(&reference_book);
    }
    if h == "ot" || h == "old testament" {
        return OLD_TESTAMENT_BOOKS.contains(&reference_book);
    }

    canonical_book(h).is_some_and(|hinted_book| hinted_book == reference_book)
}

fn reference_book(reference: &str) -> Option<&str> {
    let (book, location) = reference.trim().rsplit_once(' ')?;
    let (chapter, verse) = location.split_once(':')?;
    if chapter.is_empty()
        || verse.is_empty()
        || !chapter.bytes().all(|byte| byte.is_ascii_digit())
        || !verse
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(book.trim())
}

fn canonical_book(book: &str) -> Option<&'static str> {
    let normalized = book.trim().to_lowercase();
    let canonical = BOOK_TITLE_ALIASES
        .iter()
        .find_map(|(alias, canonical)| (*alias == normalized).then_some(*canonical))
        .unwrap_or(normalized.as_str());
    OLD_TESTAMENT_BOOKS
        .iter()
        .chain(NEW_TESTAMENT_BOOKS)
        .copied()
        .find(|candidate| *candidate == canonical)
}

// Supported alternate English book titles. Book hints otherwise require an
// exact canonical title; in particular, `John` never aliases the Johannine
// epistles.
const BOOK_TITLE_ALIASES: &[(&str, &str)] = &[
    ("psalms", "psalm"),
    ("song of songs", "song of solomon"),
    ("canticles", "song of solomon"),
];

const OLD_TESTAMENT_BOOKS: &[&str] = &[
    "genesis",
    "exodus",
    "leviticus",
    "numbers",
    "deuteronomy",
    "joshua",
    "judges",
    "ruth",
    "1 samuel",
    "2 samuel",
    "1 kings",
    "2 kings",
    "1 chronicles",
    "2 chronicles",
    "ezra",
    "nehemiah",
    "esther",
    "job",
    "psalm",
    "proverbs",
    "ecclesiastes",
    "song of solomon",
    "isaiah",
    "jeremiah",
    "lamentations",
    "ezekiel",
    "daniel",
    "hosea",
    "joel",
    "amos",
    "obadiah",
    "jonah",
    "micah",
    "nahum",
    "habakkuk",
    "zephaniah",
    "haggai",
    "zechariah",
    "malachi",
];

const NEW_TESTAMENT_BOOKS: &[&str] = &[
    "matthew",
    "mark",
    "luke",
    "john",
    "acts",
    "romans",
    "1 corinthians",
    "2 corinthians",
    "galatians",
    "ephesians",
    "philippians",
    "colossians",
    "1 thessalonians",
    "2 thessalonians",
    "1 timothy",
    "2 timothy",
    "titus",
    "philemon",
    "hebrews",
    "james",
    "1 peter",
    "2 peter",
    "1 john",
    "2 john",
    "3 john",
    "jude",
    "revelation",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn verse(index: u32, reference: &str) -> Verse {
        Verse {
            index,
            reference: reference.into(),
            text: reference.into(),
        }
    }

    fn hit(verse_index: u32) -> LaneHit {
        LaneHit {
            verse_index,
            score: 1.0,
            matched_terms: Vec::new(),
            expanded_terms: Vec::new(),
            passages: Vec::new(),
            confidence_hint: Confidence::Related,
            ranking_tier: 0,
            direct_tier: 0,
        }
    }

    #[test]
    fn dense_topic_passage_agreement_promotes_related_anchor() {
        let verses = vec![
            verse(0, "Ephesians 6:10"),
            verse(1, "Ephesians 6:12"),
            verse(2, "Psalm 140:1"),
        ];
        let mut dense_distractor = hit(2);
        dense_distractor.expanded_terms = vec!["semantic:verse".into()];
        let mut dense_in_passage = hit(1);
        dense_in_passage.expanded_terms = vec!["semantic:window".into()];
        let mut topic = hit(0);
        topic.score = 0.8;
        topic.expanded_terms = vec!["semantic-topic:armor".into()];
        topic.passages = vec![PassageRange {
            start_ref: "Ephesians 6:10".into(),
            end_ref: "Ephesians 6:12".into(),
        }];

        let results = fuse_lanes(
            &[
                (LaneId::C, vec![dense_distractor, dense_in_passage]),
                (LaneId::C, vec![topic]),
            ],
            &verses,
            &SearchOpts::default().with_top_k(3),
            FuseWeights::default(),
        );

        assert_eq!(results[0].reference, "Ephesians 6:10");
        assert_eq!(results[0].confidence, Confidence::Related);
        assert!(results[0]
            .expanded_terms
            .iter()
            .any(|term| term == "semantic-agreement:dense+topic"));
    }

    #[test]
    fn semantic_topic_without_dense_agreement_is_shadow_evidence_only() {
        let verses = vec![
            verse(0, "Proverbs 1:1"),
            verse(1, "Proverbs 1:2"),
            verse(2, "Proverbs 1:3"),
        ];
        let independent = hit(1);
        let mut independently_retrieved_topic_anchor = hit(0);
        independently_retrieved_topic_anchor.matched_terms = vec!["typed".into()];
        let mut topic = hit(0);
        topic.score = 0.9;
        topic.expanded_terms = vec!["semantic-topic:derived label".into()];
        topic.passages = vec![PassageRange {
            start_ref: "Proverbs 1:1".into(),
            end_ref: "Proverbs 1:1".into(),
        }];
        let mut topic_only = topic.clone();
        topic_only.verse_index = 2;
        topic_only.passages = vec![PassageRange {
            start_ref: "Proverbs 1:3".into(),
            end_ref: "Proverbs 1:3".into(),
        }];

        let results = fuse_lanes(
            &[
                (
                    LaneId::A,
                    vec![independent, independently_retrieved_topic_anchor],
                ),
                (LaneId::C, vec![topic, topic_only]),
            ],
            &verses,
            &SearchOpts::default().with_top_k(3),
            FuseWeights::default(),
        );

        assert_eq!(results.len(), 2, "a topic alone must not create a result");
        let anchor = results
            .iter()
            .find(|result| result.reference == "Proverbs 1:1")
            .expect("independently retrieved anchor remains available");
        assert_eq!(anchor.lanes.c, 0.0, "topic cannot cast a Lane-C vote");
        assert!(anchor
            .expanded_terms
            .iter()
            .any(|term| term == "semantic-topic:derived label"));
        assert!(!anchor
            .expanded_terms
            .iter()
            .any(|term| term == "semantic-agreement:dense+topic"));
    }

    #[test]
    fn new_testament_hint_only_boosts_new_testament_results() {
        let verses = vec![verse(0, "Genesis 1:1"), verse(1, "John 3:16")];
        let lanes = vec![(LaneId::A, vec![hit(0), hit(1)])];

        let results = fuse_lanes(
            &lanes,
            &verses,
            &SearchOpts::default()
                .with_top_k(2)
                .with_book_hint("New Testament"),
            FuseWeights::default(),
        );

        assert_eq!(results[0].reference, "John 3:16");
        assert_eq!(results[1].reference, "Genesis 1:1");
    }

    #[test]
    fn old_testament_hint_only_boosts_old_testament_results() {
        let verses = vec![verse(0, "Revelation 22:21"), verse(1, "Malachi 4:6")];
        let lanes = vec![(LaneId::A, vec![hit(0), hit(1)])];

        let results = fuse_lanes(
            &lanes,
            &verses,
            &SearchOpts::default().with_top_k(2).with_book_hint("OT"),
            FuseWeights::default(),
        );

        assert_eq!(results[0].reference, "Malachi 4:6");
        assert_eq!(results[1].reference, "Revelation 22:21");
    }

    #[test]
    fn john_hint_does_not_boost_first_john() {
        let verses = vec![verse(0, "1 John 4:8"), verse(1, "John 3:16")];
        let lanes = vec![(LaneId::A, vec![hit(0), hit(1)])];

        let results = fuse_lanes(
            &lanes,
            &verses,
            &SearchOpts::default().with_top_k(2).with_book_hint("John"),
            FuseWeights::default(),
        );

        assert_eq!(results[0].reference, "John 3:16");
        assert_eq!(results[1].reference, "1 John 4:8");
    }

    #[test]
    fn documented_book_title_aliases_match_exact_books() {
        assert!(book_matches("Psalm 23:1", "Psalms"));
        assert!(book_matches("Song of Solomon 2:1", "Song of Songs"));
        assert!(book_matches("Song of Solomon 2:1", "Canticles"));
        assert!(!book_matches("1 John 4:8", "John"));
    }

    #[test]
    fn lower_tier_hit_cannot_widen_reviewed_passage() {
        let verses = vec![verse(0, "Genesis 3:1"), verse(1, "Genesis 3:2")];
        let lower = LaneHit {
            verse_index: 0,
            score: 104.0,
            matched_terms: vec!["fall".into()],
            expanded_terms: vec![
                "event:The Fall".into(),
                "passage:Genesis 3:1–Genesis 3:2".into(),
            ],
            passages: vec![PassageRange {
                start_ref: "Genesis 3:1".into(),
                end_ref: "Genesis 3:2".into(),
            }],
            confidence_hint: Confidence::OftenMisremembered,
            ranking_tier: 4,
            direct_tier: 0,
        };
        let higher = LaneHit {
            verse_index: 0,
            score: 105.0,
            matched_terms: vec!["the".into(), "fall".into()],
            expanded_terms: vec!["curated:Fall of Man".into(), "passage:Genesis 3:1".into()],
            passages: vec![PassageRange {
                start_ref: "Genesis 3:1".into(),
                end_ref: "Genesis 3:1".into(),
            }],
            confidence_hint: Confidence::Recovered,
            ranking_tier: 5,
            direct_tier: 5,
        };

        for lane_lists in [
            vec![
                (LaneId::D, vec![lower.clone()]),
                (LaneId::D, vec![higher.clone()]),
            ],
            vec![
                (LaneId::D, vec![higher.clone()]),
                (LaneId::D, vec![lower.clone()]),
            ],
        ] {
            let results = fuse_lanes(
                &lane_lists,
                &verses,
                &SearchOpts::default().with_top_k(1),
                FuseWeights::default(),
            );
            assert_eq!(results[0].passages, higher.passages);
            assert_eq!(results[0].confidence, Confidence::Recovered);
            assert!(!results[0]
                .expanded_terms
                .iter()
                .any(|term| term.contains("Genesis 3:2")));
        }
    }
}

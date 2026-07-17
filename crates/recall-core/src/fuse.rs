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
    // Prefer one multi-verse answer over several single-verse slots that land
    // in the same structured passage, then fill top_k after collapse.
    results = stitch_passage_results(results, verses);
    results.truncate(opts.top_k.max(1));
    results
}

/// Maximum canonical verses that may be concatenated into one display string.
///
/// Long discourse ranges (Sermon on the Mount, multi-chapter events) keep the
/// anchor verse text and the structured `passages` field only. Short lists and
/// short story units are better as one stitched passage answer.
const MAX_STITCH_VERSES: usize = 16;
const MAX_STITCH_CHARS: usize = 3_000;
/// Contiguous Related hits may form a passage only when scores stay near the
/// cluster head. Exact Recovered quotes never enter this path.
const CONTIGUOUS_SCORE_RATIO: f32 = 0.85;

/// Collapse same-passage duplicates and stitch short multi-verse answers.
///
/// This is a query-independent presentation pass:
/// 1. Results that share an overlapping multi-verse structured range become
///    one answer (highest score wins; evidence merges).
/// 2. Contiguous same-chapter `Related` hits without a Recovered claim may
///    form a short passage when they rank as a unit.
/// 3. Short multi-verse ranges replace single-verse display text with the
///    concatenated BSB passage; long ranges stay anchor-text + `passages`.
pub fn stitch_passage_results(
    results: Vec<SearchResult>,
    verses: &[Verse],
) -> Vec<SearchResult> {
    if results.len() < 2 && results.iter().all(|result| {
        primary_multi_verse_range(result)
            .and_then(|range| passage_span_len(verses, range))
            .is_none_or(|len| len <= 1)
    }) {
        return results
            .into_iter()
            .map(|result| maybe_stitch_result_text(result, verses))
            .collect();
    }

    let reference_indices: HashMap<&str, u32> = verses
        .iter()
        .map(|verse| (verse.reference.as_str(), verse.index))
        .collect();

    let mut results = collapse_shared_passage_results(results, &reference_indices);
    results = stitch_contiguous_related_results(results, verses);
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(core::cmp::Ordering::Equal)
            .then_with(|| a.verse_index.cmp(&b.verse_index))
    });
    results
        .into_iter()
        .map(|result| maybe_stitch_result_text(result, verses))
        .collect()
}

fn collapse_shared_passage_results(
    results: Vec<SearchResult>,
    reference_indices: &HashMap<&str, u32>,
) -> Vec<SearchResult> {
    if results.len() < 2 {
        return results;
    }

    let n = results.len();
    let mut parent: Vec<usize> = (0..n).collect();
    for left in 0..n {
        for right in left + 1..n {
            if results_share_answer_passage(&results[left], &results[right], reference_indices) {
                let left_root = uf_root(&mut parent, left);
                let right_root = uf_root(&mut parent, right);
                if left_root != right_root {
                    parent[right_root] = left_root;
                }
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for index in 0..n {
        groups
            .entry(uf_root(&mut parent, index))
            .or_default()
            .push(index);
    }

    let mut collapsed = Vec::with_capacity(groups.len());
    for mut members in groups.into_values() {
        members.sort_by(|&left, &right| {
            results[right]
                .score
                .partial_cmp(&results[left].score)
                .unwrap_or(core::cmp::Ordering::Equal)
                .then_with(|| results[left].verse_index.cmp(&results[right].verse_index))
        });
        let mut winner = results[members[0]].clone();
        for &member in members.iter().skip(1) {
            merge_stitched_result(&mut winner, &results[member]);
        }
        if members.len() > 1 {
            push_unique_term(&mut winner.expanded_terms, "passage-stitch:collapsed");
        }
        // Prefer the tightest multi-verse range that covers every merged
        // anchor when several overlapping structured ranges exist.
        if winner.passages.len() > 1 {
            retain_preferred_passage(&mut winner, reference_indices);
        }
        collapsed.push(winner);
    }
    collapsed
}

fn results_share_answer_passage(
    left: &SearchResult,
    right: &SearchResult,
    reference_indices: &HashMap<&str, u32>,
) -> bool {
    if left.verse_index == right.verse_index {
        return true;
    }
    let left_ranges = multi_verse_intervals(left, reference_indices);
    let right_ranges = multi_verse_intervals(right, reference_indices);
    if left_ranges.is_empty() || right_ranges.is_empty() {
        return false;
    }
    left_ranges.iter().any(|left_range| {
        right_ranges
            .iter()
            .any(|right_range| intervals_overlap_inclusive(left_range, right_range))
            || interval_contains_index(left_range, right.verse_index)
    }) || right_ranges
        .iter()
        .any(|right_range| interval_contains_index(right_range, left.verse_index))
}

fn stitch_contiguous_related_results(
    results: Vec<SearchResult>,
    verses: &[Verse],
) -> Vec<SearchResult> {
    if results.len() < 2 {
        return results;
    }

    // Work in canonical index order for run detection, then restore score order
    // at the caller. Structured multi-verse answers were already collapsed;
    // this path only invents short ranges from bare Related neighbors.
    let mut ordered: Vec<SearchResult> = results;
    ordered.sort_by_key(|result| result.verse_index);

    let mut output = Vec::with_capacity(ordered.len());
    let mut index = 0;
    while index < ordered.len() {
        if !allows_contiguous_stitch(&ordered[index]) {
            output.push(ordered[index].clone());
            index += 1;
            continue;
        }

        let mut end = index;
        while end + 1 < ordered.len()
            && allows_contiguous_stitch(&ordered[end + 1])
            && ordered[end + 1].verse_index == ordered[end].verse_index + 1
            && same_chapter(
                &verses[ordered[end].verse_index as usize],
                &verses[ordered[end + 1].verse_index as usize],
            )
            && matched_terms_overlap(&ordered[index], &ordered[end + 1])
            && ordered[end + 1].score
                >= ordered[index..=end]
                    .iter()
                    .map(|result| result.score)
                    .fold(0.0, f32::max)
                    * CONTIGUOUS_SCORE_RATIO
            && (end + 1 - index + 1) <= MAX_STITCH_VERSES
        {
            end += 1;
        }

        if end == index {
            output.push(ordered[index].clone());
            index += 1;
            continue;
        }

        let mut winner = ordered[index..=end]
            .iter()
            .max_by(|left, right| {
                left.score
                    .partial_cmp(&right.score)
                    .unwrap_or(core::cmp::Ordering::Equal)
                    .then_with(|| right.verse_index.cmp(&left.verse_index))
            })
            .cloned()
            .expect("non-empty contiguous run");
        for result in &ordered[index..=end] {
            if result.verse_index != winner.verse_index {
                merge_stitched_result(&mut winner, result);
            }
        }
        let start_ref = verses[ordered[index].verse_index as usize]
            .reference
            .clone();
        let end_ref = verses[ordered[end].verse_index as usize].reference.clone();
        let range = PassageRange { start_ref, end_ref };
        if !winner.passages.iter().any(|passage| passage == &range) {
            winner.passages.insert(0, range);
        }
        push_unique_term(&mut winner.expanded_terms, "passage-stitch:contiguous");
        output.push(winner);
        index = end + 1;
    }
    output
}

fn allows_contiguous_stitch(result: &SearchResult) -> bool {
    // Exact recoveries and explicit misquotes are single-claim answers. Hits
    // that already carry a multi-verse structured range were handled by
    // collapse; re-stitching them with bare neighbors invents a narrower
    // competing range.
    matches!(result.confidence, Confidence::Related)
        && primary_multi_verse_range(result).is_none()
        && !result.matched_terms.is_empty()
}

fn matched_terms_overlap(left: &SearchResult, right: &SearchResult) -> bool {
    left.matched_terms
        .iter()
        .any(|term| right.matched_terms.iter().any(|other| other == term))
}

fn maybe_stitch_result_text(mut result: SearchResult, verses: &[Verse]) -> SearchResult {
    let Some(range) = primary_multi_verse_range(&result).cloned() else {
        return result;
    };
    let Some((start, end)) = passage_index_bounds(verses, &range) else {
        return result;
    };
    let span = (end - start + 1) as usize;
    if !(2..=MAX_STITCH_VERSES).contains(&span) {
        return result;
    }
    // Dense ±1 window evidence is useful for ranking but is not a claim that
    // the three-verse neighborhood is the answer unit. Only stitch text when
    // the multi-verse range came from source structure or an earlier stitch.
    let may_replace_text = result.lanes.d > 0.0
        || result.expanded_terms.iter().any(|term| {
            term.starts_with("event:")
                || term.starts_with("topic:")
                || term.starts_with("curated:")
                || term.starts_with("passage-stitch:")
                || term.starts_with("semantic-agreement:verse+passage")
                || term.starts_with("semantic-agreement:dense+topic")
                || term.starts_with("passage:")
        });
    if !may_replace_text {
        return result;
    }

    let mut stitched = String::new();
    for verse in &verses[start as usize..=end as usize] {
        if !stitched.is_empty() {
            stitched.push(' ');
        }
        stitched.push_str(verse.text.trim());
        if stitched.len() > MAX_STITCH_CHARS {
            return result;
        }
    }
    if stitched.is_empty() {
        return result;
    }
    result.text = stitched;
    push_unique_term(&mut result.expanded_terms, "passage-stitch:text");
    result
}

fn primary_multi_verse_range(result: &SearchResult) -> Option<&PassageRange> {
    result
        .passages
        .iter()
        .find(|passage| passage.start_ref != passage.end_ref)
}

fn multi_verse_intervals(
    result: &SearchResult,
    reference_indices: &HashMap<&str, u32>,
) -> Vec<(u32, u32)> {
    result
        .passages
        .iter()
        .filter_map(|passage| {
            if passage.start_ref == passage.end_ref {
                return None;
            }
            let start = *reference_indices.get(passage.start_ref.as_str())?;
            let end = *reference_indices.get(passage.end_ref.as_str())?;
            (start <= end).then_some((start, end))
        })
        .collect()
}

fn passage_index_bounds(verses: &[Verse], range: &PassageRange) -> Option<(u32, u32)> {
    let start = verses
        .iter()
        .find(|verse| verse.reference == range.start_ref)
        .map(|verse| verse.index)?;
    let end = verses
        .iter()
        .find(|verse| verse.reference == range.end_ref)
        .map(|verse| verse.index)?;
    (start <= end).then_some((start, end))
}

fn passage_span_len(verses: &[Verse], range: &PassageRange) -> Option<usize> {
    let (start, end) = passage_index_bounds(verses, range)?;
    Some((end - start + 1) as usize)
}

fn interval_contains_index(range: &(u32, u32), verse_index: u32) -> bool {
    range.0 <= verse_index && verse_index <= range.1
}

fn intervals_overlap_inclusive(left: &(u32, u32), right: &(u32, u32)) -> bool {
    left.0 <= right.1 && right.0 <= left.1
}

fn retain_preferred_passage(
    result: &mut SearchResult,
    reference_indices: &HashMap<&str, u32>,
) {
    let mut multi: Vec<(usize, u32, u32)> = result
        .passages
        .iter()
        .enumerate()
        .filter_map(|(index, passage)| {
            if passage.start_ref == passage.end_ref {
                return None;
            }
            let start = *reference_indices.get(passage.start_ref.as_str())?;
            let end = *reference_indices.get(passage.end_ref.as_str())?;
            (start <= end).then_some((index, start, end))
        })
        .collect();
    if multi.is_empty() {
        return;
    }
    // Prefer the shortest multi-verse range that still covers every merged
    // anchor verse index we know about via the winner's index alone is not
    // enough; keep the shortest range that contains the winner, else the
    // shortest overall multi-verse range.
    multi.sort_by_key(|&(_, start, end)| (end - start, start, end));
    let preferred = multi
        .iter()
        .find(|&&(_, start, end)| start <= result.verse_index && result.verse_index <= end)
        .or_else(|| multi.first())
        .map(|(index, _, _)| *index);
    if let Some(preferred) = preferred {
        let chosen = result.passages[preferred].clone();
        result.passages.retain(|passage| {
            passage == &chosen
                || (passage.start_ref == passage.end_ref
                    && passage.start_ref != chosen.start_ref
                    && passage.start_ref != chosen.end_ref)
        });
        if !result.passages.iter().any(|passage| passage == &chosen) {
            result.passages.insert(0, chosen);
        } else if let Some(position) = result.passages.iter().position(|passage| passage == &chosen)
        {
            if position != 0 {
                let passage = result.passages.remove(position);
                result.passages.insert(0, passage);
            }
        }
    }
}

fn merge_stitched_result(winner: &mut SearchResult, incoming: &SearchResult) {
    winner.score = winner.score.max(incoming.score);
    winner.lanes.a = winner.lanes.a.max(incoming.lanes.a);
    winner.lanes.b = winner.lanes.b.max(incoming.lanes.b);
    winner.lanes.c = winner.lanes.c.max(incoming.lanes.c);
    winner.lanes.d = winner.lanes.d.max(incoming.lanes.d);
    merge_terms(&mut winner.matched_terms, &incoming.matched_terms);
    merge_terms(&mut winner.expanded_terms, &incoming.expanded_terms);
    merge_terms(&mut winner.chips, &incoming.chips);
    merge_passages(&mut winner.passages, &incoming.passages);
    winner.confidence = stronger_confidence(winner.confidence, incoming.confidence);
}

fn same_chapter(left: &Verse, right: &Verse) -> bool {
    chapter_key(&left.reference) == chapter_key(&right.reference)
        && chapter_key(&left.reference).is_some()
}

fn chapter_key(reference: &str) -> Option<(&str, &str)> {
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
    Some((book, chapter))
}

fn push_unique_term(terms: &mut Vec<String>, term: &str) {
    if !terms.iter().any(|existing| existing == term) {
        terms.push(term.to_string());
    }
}

fn uf_root(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
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
    fn shared_structured_passages_collapse_to_one_stitched_answer() {
        let verses = vec![
            verse(0, "Ephesians 6:10"),
            verse(1, "Ephesians 6:11"),
            verse(2, "Ephesians 6:12"),
            verse(3, "Ephesians 6:13"),
        ];
        // Distinct anchors that both point at the armor-of-God unit should not
        // consume two top-k slots.
        let mut first = hit(1);
        first.score = 2.0;
        first.ranking_tier = 1;
        first.matched_terms = vec!["armor".into()];
        first.expanded_terms = vec!["topic:armor of god".into(), "passage:Ephesians 6:10–13".into()];
        first.passages = vec![PassageRange {
            start_ref: "Ephesians 6:10".into(),
            end_ref: "Ephesians 6:13".into(),
        }];
        let mut second = hit(3);
        second.score = 1.5;
        second.ranking_tier = 1;
        second.matched_terms = vec!["armor".into()];
        second.expanded_terms = vec!["topic:armor of god".into(), "passage:Ephesians 6:10–13".into()];
        second.passages = vec![PassageRange {
            start_ref: "Ephesians 6:10".into(),
            end_ref: "Ephesians 6:13".into(),
        }];
        let mut distractor = hit(0);
        distractor.score = 0.5;

        let results = fuse_lanes(
            &[(LaneId::A, vec![first, second, distractor])],
            &verses,
            &SearchOpts::default().with_top_k(3),
            FuseWeights::default(),
        );

        assert_eq!(results.len(), 2, "shared passage must collapse: {results:?}");
        assert_eq!(results[0].reference, "Ephesians 6:11");
        assert!(
            results[0]
                .expanded_terms
                .iter()
                .any(|term| term == "passage-stitch:collapsed"),
            "missing collapse marker: {:?}",
            results[0].expanded_terms
        );
        assert!(
            results[0]
                .expanded_terms
                .iter()
                .any(|term| term == "passage-stitch:text"),
            "short passage should stitch display text: {:?}",
            results[0].expanded_terms
        );
        assert!(
            results[0].text.contains("Ephesians 6:10")
                && results[0].text.contains("Ephesians 6:13"),
            "stitched text should cover the passage: {}",
            results[0].text
        );
        assert_eq!(
            results[0].passages,
            [PassageRange {
                start_ref: "Ephesians 6:10".into(),
                end_ref: "Ephesians 6:13".into(),
            }]
        );
    }

    #[test]
    fn contiguous_related_hits_form_a_short_passage() {
        let verses = vec![
            verse(0, "Lamentations 3:22"),
            verse(1, "Lamentations 3:23"),
            verse(2, "Psalm 1:1"),
        ];
        let mut a = hit(0);
        a.score = 1.0;
        a.matched_terms = vec!["mercies".into()];
        let mut b = hit(1);
        b.score = 0.95;
        b.matched_terms = vec!["mercies".into()];
        let mut c = hit(2);
        c.score = 0.5;

        let results = fuse_lanes(
            &[(LaneId::A, vec![a, b, c])],
            &verses,
            &SearchOpts::default().with_top_k(3),
            FuseWeights::default(),
        );

        assert_eq!(results.len(), 2);
        let passage = results
            .iter()
            .find(|result| {
                result.passages.iter().any(|range| {
                    range.start_ref == "Lamentations 3:22" && range.end_ref == "Lamentations 3:23"
                })
            })
            .expect("contiguous related pair should form a passage");
        assert!(passage
            .expanded_terms
            .iter()
            .any(|term| term == "passage-stitch:contiguous"));
        assert!(passage
            .expanded_terms
            .iter()
            .any(|term| term == "passage-stitch:text"));
    }

    #[test]
    fn recovered_quotes_do_not_absorb_neighboring_related_hits() {
        let verses = vec![
            verse(0, "Matthew 5:3"),
            verse(1, "Matthew 5:4"),
            verse(2, "Matthew 5:5"),
        ];
        let mut recovered = hit(0);
        recovered.score = 3.0;
        recovered.ranking_tier = 3;
        recovered.confidence_hint = Confidence::Recovered;
        recovered.matched_terms = vec!["blessed".into(), "poor".into()];
        let mut neighbor = hit(1);
        neighbor.score = 2.9;
        neighbor.matched_terms = vec!["blessed".into()];
        let mut far = hit(2);
        far.score = 0.4;

        let results = fuse_lanes(
            &[(LaneId::A, vec![recovered, neighbor, far])],
            &verses,
            &SearchOpts::default().with_top_k(3),
            FuseWeights::default(),
        );

        assert_eq!(results[0].reference, "Matthew 5:3");
        assert_eq!(results[0].confidence, Confidence::Recovered);
        assert!(
            results[0].passages.is_empty(),
            "recovered quote must stay a single verse: {:?}",
            results[0].passages
        );
        assert_eq!(results[0].text, "Matthew 5:3");
    }

    #[test]
    fn long_structured_passages_keep_anchor_text() {
        let mut verses = Vec::new();
        for index in 0..20u32 {
            verses.push(verse(index, &format!("Matthew 5:{}", index + 1)));
        }
        let mut event = hit(0);
        event.score = 104.0;
        event.ranking_tier = 4;
        event.expanded_terms = vec![
            "event:Sermon on the Mount".into(),
            "passage:Matthew 5:1–20".into(),
        ];
        event.passages = vec![PassageRange {
            start_ref: "Matthew 5:1".into(),
            end_ref: "Matthew 5:20".into(),
        }];

        let results = fuse_lanes(
            &[(LaneId::D, vec![event])],
            &verses,
            &SearchOpts::default().with_top_k(1),
            FuseWeights::default(),
        );

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].text, "Matthew 5:1");
        assert!(
            !results[0]
                .expanded_terms
                .iter()
                .any(|term| term == "passage-stitch:text"),
            "long discourse ranges must not dump full text"
        );
        assert_eq!(
            results[0].passages,
            [PassageRange {
                start_ref: "Matthew 5:1".into(),
                end_ref: "Matthew 5:20".into(),
            }]
        );
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

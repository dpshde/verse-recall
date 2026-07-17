//! Evaluate generic rank fusions over paired, already-revealed search results.
//!
//! The tool intentionally has no access to sealed holdouts. It consumes the
//! paired export produced by `export_dense_pair` and emits aggregate metrics
//! for a preregisterable grid of query-independent RRF policies.

use clap::Parser;
use recall_core::{Confidence, SearchResponse, SearchResult};
use recall_eval::{
    passage_range_covers, refs_match, score_prompt_at_depth, summarize_system, ExpectedConfidence,
    GoldMode, MetricScore, Prompt, PromptScore,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Evaluate generic dense-view fusion on an exported revealed corpus")]
struct Cli {
    #[arg(long)]
    input: PathBuf,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Deserialize)]
struct PairExport {
    schema_version: u32,
    depth: usize,
    primary_dense: String,
    auxiliary_dense: String,
    rows: Vec<PairRow>,
}

#[derive(Deserialize)]
struct PairRow {
    suite: String,
    prompt: Prompt,
    primary: SearchResponse,
    auxiliary: SearchResponse,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CandidatePool {
    /// Rerank only candidates the production view already retrieved.
    PrimaryOnly,
    /// Permit passage-view-only candidates to enter the result list.
    Union,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
enum Policy {
    Primary,
    Auxiliary,
    Rrf {
        protected_primary_prefix: usize,
        rrf_k: usize,
        auxiliary_weight_percent: usize,
        pool: CandidatePool,
    },
}

#[derive(Serialize)]
struct Evaluation {
    schema_version: u32,
    input_schema_version: u32,
    rows: usize,
    retrieval_rows: usize,
    depth: usize,
    primary_dense: String,
    auxiliary_dense: String,
    methods: Vec<MethodSummary>,
}

#[derive(Serialize)]
struct MethodSummary {
    key: String,
    policy: Policy,
    overall: MetricScore,
    by_suite: BTreeMap<String, MetricScore>,
    by_category: BTreeMap<String, MetricScore>,
    category_macro: MacroMetrics,
    family_macro: MacroMetrics,
    slices: BTreeMap<String, MetricScore>,
    delta_from_primary: DeltaSummary,
}

#[derive(Serialize)]
struct MacroMetrics {
    groups: usize,
    recall_at_1: f64,
    recall_at_3: f64,
    recall_at_10: f64,
    mrr: f64,
    weakest_recall_at_3: f64,
    weakest_recall_at_10: f64,
}

#[derive(Default, Serialize)]
struct DeltaSummary {
    rank_improved: usize,
    rank_worsened: usize,
    hit_at_1_gained: usize,
    hit_at_1_lost: usize,
    hit_at_3_gained: usize,
    hit_at_3_lost: usize,
    hit_at_10_gained: usize,
    hit_at_10_lost: usize,
    suites_recall_at_3_regressed: usize,
    suites_recall_at_10_regressed: usize,
    hard_negative_passes_gained: usize,
    hard_negative_passes_lost: usize,
    passage_passes_gained: usize,
    passage_passes_lost: usize,
    confidence_safety_passes_lost: usize,
    confidence_label_passes_gained: usize,
    confidence_label_passes_lost: usize,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    let input: PairExport = serde_json::from_slice(&fs::read(&cli.input)?)?;
    if input.schema_version != 1 || input.depth != 100 || input.rows.is_empty() {
        return Err("paired export must be nonempty schema v1 at depth 100".into());
    }
    let policies = policy_grid();
    let baseline_scores = score_rows(&input.rows, &Policy::Primary, input.depth);
    let baseline_by_suite = summarize_by_suite(&input.rows, &baseline_scores);
    let mut methods = Vec::with_capacity(policies.len());
    for policy in policies {
        let scores = score_rows(&input.rows, &policy, input.depth);
        let system = summarize_system(&policy_key(&policy), scores.clone());
        let by_suite = summarize_by_suite(&input.rows, &scores);
        let slices = summarize_slices(&input.rows, &scores);
        let category_macro = macro_metrics(system.by_category.values());
        let family_macro = macro_metrics(system.by_family.values());
        methods.push(MethodSummary {
            key: policy_key(&policy),
            policy,
            overall: system.overall,
            by_category: system.by_category,
            category_macro,
            family_macro,
            delta_from_primary: deltas(&baseline_scores, &scores, &baseline_by_suite, &by_suite),
            by_suite,
            slices,
        });
    }
    let evaluation = Evaluation {
        schema_version: 1,
        input_schema_version: input.schema_version,
        rows: input.rows.len(),
        retrieval_rows: input
            .rows
            .iter()
            .filter(|row| !row.prompt.gold_refs.is_empty())
            .count(),
        depth: input.depth,
        primary_dense: input.primary_dense,
        auxiliary_dense: input.auxiliary_dense,
        methods,
    };
    fs::write(&cli.output, serde_json::to_vec_pretty(&evaluation)?)?;
    Ok(())
}

fn policy_grid() -> Vec<Policy> {
    let mut policies = vec![Policy::Primary, Policy::Auxiliary];
    for protected_primary_prefix in [0, 1, 3, 5, 10] {
        for rrf_k in [0, 10, 60] {
            for auxiliary_weight_percent in [5, 10, 20, 30, 40, 50, 75, 100] {
                for pool in [CandidatePool::PrimaryOnly, CandidatePool::Union] {
                    policies.push(Policy::Rrf {
                        protected_primary_prefix,
                        rrf_k,
                        auxiliary_weight_percent,
                        pool,
                    });
                }
            }
        }
    }
    policies
}

fn policy_key(policy: &Policy) -> String {
    match policy {
        Policy::Primary => "primary-verse".into(),
        Policy::Auxiliary => "auxiliary-verse-passage".into(),
        Policy::Rrf {
            protected_primary_prefix,
            rrf_k,
            auxiliary_weight_percent,
            pool,
        } => format!(
            "rrf-p{protected_primary_prefix}-k{rrf_k}-w{auxiliary_weight_percent}-{}",
            match pool {
                CandidatePool::PrimaryOnly => "primary",
                CandidatePool::Union => "union",
            }
        ),
    }
}

fn score_rows(rows: &[PairRow], policy: &Policy, depth: usize) -> Vec<PromptScore> {
    rows.iter()
        .map(|row| {
            let results = apply_policy(policy, &row.primary.results, &row.auxiliary.results, depth);
            score_results(&row.prompt, &results, depth)
        })
        .collect()
}

fn apply_policy(
    policy: &Policy,
    primary: &[SearchResult],
    auxiliary: &[SearchResult],
    depth: usize,
) -> Vec<SearchResult> {
    match policy {
        Policy::Primary => primary.iter().take(depth).cloned().collect(),
        Policy::Auxiliary => auxiliary.iter().take(depth).cloned().collect(),
        Policy::Rrf {
            protected_primary_prefix,
            rrf_k,
            auxiliary_weight_percent,
            pool,
        } => rrf(
            primary,
            auxiliary,
            *protected_primary_prefix,
            *rrf_k,
            *auxiliary_weight_percent as f64 / 100.0,
            pool,
            depth,
        ),
    }
}

fn rrf(
    primary: &[SearchResult],
    auxiliary: &[SearchResult],
    protected_primary_prefix: usize,
    rrf_k: usize,
    auxiliary_weight: f64,
    pool: &CandidatePool,
    depth: usize,
) -> Vec<SearchResult> {
    let primary_rank: HashMap<&str, usize> = primary
        .iter()
        .enumerate()
        .map(|(index, result)| (result.reference.as_str(), index + 1))
        .collect();
    let auxiliary_rank: HashMap<&str, usize> = auxiliary
        .iter()
        .enumerate()
        .map(|(index, result)| (result.reference.as_str(), index + 1))
        .collect();
    let primary_by_ref: HashMap<&str, &SearchResult> = primary
        .iter()
        .map(|result| (result.reference.as_str(), result))
        .collect();
    let auxiliary_by_ref: HashMap<&str, &SearchResult> = auxiliary
        .iter()
        .map(|result| (result.reference.as_str(), result))
        .collect();

    // A protected prefix is a mechanism-level safety guard: it has no query,
    // category, family, or gold awareness.
    let prefix_len = protected_primary_prefix.min(primary.len()).min(depth);
    let mut output: Vec<SearchResult> = primary.iter().take(prefix_len).cloned().collect();
    let protected: HashSet<&str> = output
        .iter()
        .map(|result| result.reference.as_str())
        .collect();

    let mut candidates: HashSet<&str> = primary_rank.keys().copied().collect();
    if matches!(pool, CandidatePool::Union) {
        candidates.extend(auxiliary_rank.keys().copied());
    }
    let mut ranked: Vec<(&str, f64, usize, usize)> = candidates
        .into_iter()
        .filter(|reference| !protected.contains(reference))
        .map(|reference| {
            let p_rank = primary_rank.get(reference).copied().unwrap_or(usize::MAX);
            let a_rank = auxiliary_rank.get(reference).copied().unwrap_or(usize::MAX);
            let p_score = if p_rank != usize::MAX {
                1.0 / (rrf_k + p_rank) as f64
            } else {
                0.0
            };
            let a_score = if a_rank != usize::MAX {
                auxiliary_weight / (rrf_k + a_rank) as f64
            } else {
                0.0
            };
            (reference, p_score + a_score, p_rank, a_rank)
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(&right.3))
            .then_with(|| left.0.cmp(right.0))
    });
    for (reference, _, _, _) in ranked.into_iter().take(depth - output.len()) {
        // Prefer the production-view object for shared candidates. This keeps
        // its confidence and source-backed structured-passage provenance.
        let result = primary_by_ref
            .get(reference)
            .copied()
            .or_else(|| auxiliary_by_ref.get(reference).copied())
            .expect("candidate originated in one of the result lists");
        output.push(result.clone());
    }
    output
}

fn score_results(prompt: &Prompt, results: &[SearchResult], depth: usize) -> PromptScore {
    let scoring_refs: Vec<String> = results.iter().map(result_scoring_ref).collect();
    let confidence_safe = results.iter().take(3).all(|result| {
        result.confidence != Confidence::Recovered
            || prompt
                .gold_refs
                .iter()
                .any(|gold| result_matches_ref(result, gold))
    });
    let mut score = score_prompt_at_depth(prompt, &scoring_refs, Some(confidence_safe), depth);
    score.top_refs = results
        .iter()
        .map(|result| result.reference.clone())
        .collect();
    if let Some(expected) = prompt.expected_confidence {
        let relevant = relevant_results(prompt, results);
        score.confidence_label_ok = Some(
            score.first_relevant_rank.is_some()
                && !relevant.is_empty()
                && relevant
                    .iter()
                    .all(|result| confidence_matches(result.confidence, expected)),
        );
    }
    if !prompt.expected_passages.is_empty() {
        score.passage_output_ok = Some(prompt.expected_passages.iter().all(|expected| {
            results.iter().take(3).any(|result| {
                result.passages.iter().any(|passage| {
                    passage_range_covers(&passage.start_ref, &passage.end_ref, expected)
                })
            })
        }));
    }
    score
}

fn result_matches_ref(result: &SearchResult, expected: &str) -> bool {
    refs_match(&result.reference, expected)
        || result
            .passages
            .iter()
            .any(|passage| passage_range_covers(&passage.start_ref, &passage.end_ref, expected))
}

fn result_scoring_ref(result: &SearchResult) -> String {
    let mut references = vec![result.reference.clone()];
    references.extend(
        result
            .passages
            .iter()
            .map(|passage| compact_passage_range(&passage.start_ref, &passage.end_ref)),
    );
    references.sort();
    references.dedup();
    references.join("; ")
}

fn compact_passage_range(start: &str, end: &str) -> String {
    if refs_match(start, end) {
        return start.to_string();
    }
    let Some((start_book, start_location)) = start.rsplit_once(' ') else {
        return format!("{start}; {end}");
    };
    let Some((end_book, end_location)) = end.rsplit_once(' ') else {
        return format!("{start}; {end}");
    };
    if !start_book.eq_ignore_ascii_case(end_book) {
        return format!("{start}; {end}");
    }
    let Some((start_chapter, _)) = start_location.split_once(':') else {
        return format!("{start}; {end}");
    };
    let Some((end_chapter, end_verse)) = end_location.split_once(':') else {
        return format!("{start}; {end}");
    };
    if start_chapter == end_chapter {
        format!("{start}-{end_verse}")
    } else {
        format!("{start}-{end_location}")
    }
}

fn relevant_results<'a>(prompt: &Prompt, results: &'a [SearchResult]) -> Vec<&'a SearchResult> {
    match prompt.gold_mode {
        GoldMode::Any => results
            .iter()
            .find(|result| {
                prompt
                    .gold_refs
                    .iter()
                    .any(|gold| result_matches_ref(result, gold))
            })
            .into_iter()
            .collect(),
        GoldMode::All => prompt
            .gold_refs
            .iter()
            .filter_map(|gold| {
                results
                    .iter()
                    .find(|result| result_matches_ref(result, gold))
            })
            .collect(),
    }
}

fn confidence_matches(actual: Confidence, expected: ExpectedConfidence) -> bool {
    matches!(
        (actual, expected),
        (Confidence::Recovered, ExpectedConfidence::Recovered)
            | (Confidence::Related, ExpectedConfidence::Related)
            | (
                Confidence::OftenMisremembered,
                ExpectedConfidence::OftenMisremembered
            )
    )
}

fn summarize_by_suite(rows: &[PairRow], scores: &[PromptScore]) -> BTreeMap<String, MetricScore> {
    let mut members: BTreeMap<&str, Vec<PromptScore>> = BTreeMap::new();
    for (row, score) in rows.iter().zip(scores) {
        members
            .entry(row.suite.as_str())
            .or_default()
            .push(score.clone());
    }
    members
        .into_iter()
        .map(|(suite, scores)| (suite.to_string(), summarize_system(suite, scores).overall))
        .collect()
}

fn summarize_slices(rows: &[PairRow], scores: &[PromptScore]) -> BTreeMap<String, MetricScore> {
    let definitions: [(&str, &[&str]); 5] = [
        (
            "meaning",
            &[
                "meaning-only",
                "situation",
                "semantic-unseen",
                "contrast-paraphrase",
            ],
        ),
        (
            "multi-intent",
            &["conflation", "conflated", "multi-intent-unseen"],
        ),
        (
            "event-entity",
            &[
                "event-name",
                "event-alias",
                "event-paraphrase",
                "event-paraphrase-unseen",
                "entity-event-unseen",
            ],
        ),
        (
            "remembered-wording",
            &[
                "near-quote",
                "partial-phrase",
                "rare-phrase",
                "canonical-phrase",
                "truncation",
            ],
        ),
        (
            "translation",
            &[
                "translation-drift",
                "translation-drift-novel",
                "wrong-translation",
                "kjv-drift",
                "translation-hybrid",
            ],
        ),
    ];
    definitions
        .into_iter()
        .map(|(name, categories)| {
            let selected: Vec<PromptScore> = rows
                .iter()
                .zip(scores)
                .filter(|(row, _)| categories.contains(&row.prompt.category.as_str()))
                .map(|(_, score)| score.clone())
                .collect();
            (name.to_string(), summarize_system(name, selected).overall)
        })
        .collect()
}

fn macro_metrics<'a>(metrics: impl Iterator<Item = &'a MetricScore>) -> MacroMetrics {
    let metrics: Vec<&MetricScore> = metrics
        .filter(|metric| metric.retrieval_queries > 0)
        .collect();
    let groups = metrics.len();
    let denominator = groups.max(1) as f64;
    MacroMetrics {
        groups,
        recall_at_1: metrics.iter().map(|metric| metric.recall_at_1).sum::<f64>() / denominator,
        recall_at_3: metrics.iter().map(|metric| metric.recall_at_3).sum::<f64>() / denominator,
        recall_at_10: metrics
            .iter()
            .map(|metric| metric.recall_at_10)
            .sum::<f64>()
            / denominator,
        mrr: metrics.iter().map(|metric| metric.mrr).sum::<f64>() / denominator,
        weakest_recall_at_3: metrics
            .iter()
            .map(|metric| metric.recall_at_3)
            .reduce(f64::min)
            .unwrap_or(0.0),
        weakest_recall_at_10: metrics
            .iter()
            .map(|metric| metric.recall_at_10)
            .reduce(f64::min)
            .unwrap_or(0.0),
    }
}

fn deltas(
    baseline: &[PromptScore],
    candidate: &[PromptScore],
    baseline_by_suite: &BTreeMap<String, MetricScore>,
    candidate_by_suite: &BTreeMap<String, MetricScore>,
) -> DeltaSummary {
    let mut delta = DeltaSummary::default();
    for (before, after) in baseline.iter().zip(candidate) {
        let before_rank = before.first_relevant_rank.unwrap_or(usize::MAX);
        let after_rank = after.first_relevant_rank.unwrap_or(usize::MAX);
        delta.rank_improved += usize::from(after_rank < before_rank);
        delta.rank_worsened += usize::from(after_rank > before_rank);
        delta.hit_at_1_gained += usize::from(!before.hit_at_1 && after.hit_at_1);
        delta.hit_at_1_lost += usize::from(before.hit_at_1 && !after.hit_at_1);
        delta.hit_at_3_gained += usize::from(!before.hit_at_3 && after.hit_at_3);
        delta.hit_at_3_lost += usize::from(before.hit_at_3 && !after.hit_at_3);
        delta.hit_at_10_gained += usize::from(!before.hit_at_10 && after.hit_at_10);
        delta.hit_at_10_lost += usize::from(before.hit_at_10 && !after.hit_at_10);
        delta.hard_negative_passes_gained += usize::from(
            before.hard_negative_ok == Some(false) && after.hard_negative_ok == Some(true),
        );
        delta.hard_negative_passes_lost += usize::from(
            before.hard_negative_ok == Some(true) && after.hard_negative_ok == Some(false),
        );
        delta.passage_passes_gained += usize::from(
            before.passage_output_ok == Some(false) && after.passage_output_ok == Some(true),
        );
        delta.passage_passes_lost += usize::from(
            before.passage_output_ok == Some(true) && after.passage_output_ok == Some(false),
        );
        delta.confidence_safety_passes_lost += usize::from(
            before.confidence_safe == Some(true) && after.confidence_safe == Some(false),
        );
        delta.confidence_label_passes_gained += usize::from(
            before.confidence_label_ok == Some(false) && after.confidence_label_ok == Some(true),
        );
        delta.confidence_label_passes_lost += usize::from(
            before.confidence_label_ok == Some(true) && after.confidence_label_ok == Some(false),
        );
    }
    for (suite, before) in baseline_by_suite {
        let after = &candidate_by_suite[suite];
        delta.suites_recall_at_3_regressed +=
            usize::from(after.recall_at_3 + f64::EPSILON < before.recall_at_3);
        delta.suites_recall_at_10_regressed +=
            usize::from(after.recall_at_10 + f64::EPSILON < before.recall_at_10);
    }
    delta
}

#[cfg(test)]
mod tests {
    use super::*;
    use recall_core::LaneScores;

    fn result(reference: &str) -> SearchResult {
        SearchResult {
            reference: reference.into(),
            text: String::new(),
            score: 0.0,
            confidence: Confidence::Related,
            lanes: LaneScores::default(),
            matched_terms: Vec::new(),
            expanded_terms: Vec::new(),
            passages: Vec::new(),
            chips: Vec::new(),
            verse_index: 0,
        }
    }

    fn references(results: &[SearchResult]) -> Vec<&str> {
        results
            .iter()
            .map(|result| result.reference.as_str())
            .collect()
    }

    #[test]
    fn protected_prefix_is_bit_for_bit_primary_order() {
        let primary = [result("A"), result("B"), result("C"), result("D")];
        let auxiliary = [result("D"), result("C"), result("B"), result("A")];
        let fused = rrf(&primary, &auxiliary, 3, 60, 1.0, &CandidatePool::Union, 4);
        assert_eq!(&references(&fused)[..3], ["A", "B", "C"]);
    }

    #[test]
    fn primary_only_consensus_cannot_import_auxiliary_candidates() {
        let primary = [result("A"), result("B"), result("C")];
        let auxiliary = [result("X"), result("B"), result("C")];
        let fused = rrf(
            &primary,
            &auxiliary,
            0,
            60,
            0.5,
            &CandidatePool::PrimaryOnly,
            3,
        );
        assert_eq!(references(&fused).len(), 3);
        assert!(!references(&fused).contains(&"X"));
    }
}

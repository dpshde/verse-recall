//! Reusable evaluation types and scoring for scripture-recall systems.
//!
//! The JSONL schema intentionally groups query variants into `family` values and
//! assigns the whole family to one `split`. This prevents a translation variant
//! or paraphrase of the same passage from leaking across development and holdout
//! sets.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fmt;

pub const METRIC_CUTOFFS: [usize; 3] = [1, 3, 10];

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GoldMode {
    #[default]
    Any,
    All,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExpectedConfidence {
    Recovered,
    Related,
    OftenMisremembered,
}

/// One JSONL evaluation case.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Prompt {
    pub id: String,
    pub prompt: String,
    pub category: String,
    pub gold_refs: Vec<String>,
    /// `any` for ordinary multi-valid-target queries; `all` for conflations
    /// where every independently remembered passage must be retrieved.
    #[serde(default, skip_serializing_if = "is_default_gold_mode")]
    pub gold_mode: GoldMode,
    /// Hard-negative passages that must rank below the completed gold target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub must_not: Vec<String>,
    /// Optional UX-label contract, chiefly for gold-bearing misquotes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_confidence: Option<ExpectedConfidence>,
    /// Optional structured passage outputs required for named concepts/events.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_passages: Vec<String>,
    /// Stable semantic/passage family, e.g. `philippians-4-13` or `the-fall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Dataset partition. Families must not occur in more than one split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    /// Query-form labels such as `esv`, `kjv`, `event-name`, or `paraphrase`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Human-readable provenance for a demand-derived or translation-derived case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn is_default_gold_mode(mode: &GoldMode) -> bool {
    *mode == GoldMode::Any
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptFileError(String);

impl fmt::Display for PromptFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for PromptFileError {}

/// Parse and validate a JSONL prompt corpus.
pub fn parse_prompts(raw: &str) -> Result<Vec<Prompt>, PromptFileError> {
    let mut prompts = Vec::new();
    let mut ids = HashSet::new();

    for (line_idx, raw_line) in raw.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let prompt: Prompt = serde_json::from_str(line).map_err(|err| {
            PromptFileError(format!(
                "invalid prompt JSONL at line {}: {err}",
                line_idx + 1
            ))
        })?;
        validate_prompt(&prompt, line_idx + 1)?;
        if !ids.insert(prompt.id.clone()) {
            return Err(PromptFileError(format!(
                "duplicate prompt id `{}` at line {}",
                prompt.id,
                line_idx + 1
            )));
        }
        prompts.push(prompt);
    }

    validate_family_splits(&prompts)?;
    Ok(prompts)
}

fn validate_prompt(prompt: &Prompt, line: usize) -> Result<(), PromptFileError> {
    for (field, value) in [
        ("id", prompt.id.as_str()),
        ("prompt", prompt.prompt.as_str()),
        ("category", prompt.category.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(PromptFileError(format!(
                "prompt {field} must not be empty at line {line}"
            )));
        }
    }
    if prompt.family.as_deref().is_some_and(str::is_empty) {
        return Err(PromptFileError(format!(
            "prompt family must not be empty at line {line}"
        )));
    }
    if prompt.split.as_deref().is_some_and(str::is_empty) {
        return Err(PromptFileError(format!(
            "prompt split must not be empty at line {line}"
        )));
    }
    if prompt.gold_mode == GoldMode::All && prompt.gold_refs.is_empty() {
        return Err(PromptFileError(format!(
            "gold_mode=all requires at least one gold reference at line {line}"
        )));
    }
    for gold in &prompt.gold_refs {
        if parse_ref_blob_strict(gold).is_none() {
            return Err(PromptFileError(format!(
                "unparseable gold reference `{gold}` at line {line}"
            )));
        }
    }
    for negative in &prompt.must_not {
        if parse_ref_blob_strict(negative).is_none() {
            return Err(PromptFileError(format!(
                "unparseable hard-negative reference `{negative}` at line {line}"
            )));
        }
        if prompt
            .gold_refs
            .iter()
            .any(|gold| refs_match(negative, gold))
        {
            return Err(PromptFileError(format!(
                "hard-negative reference `{negative}` overlaps gold at line {line}"
            )));
        }
    }
    for passage in &prompt.expected_passages {
        if parse_ref_blob_strict(passage).is_none() {
            return Err(PromptFileError(format!(
                "unparseable expected passage `{passage}` at line {line}"
            )));
        }
    }
    Ok(())
}

/// Reject family leakage across dataset partitions.
pub fn validate_family_splits(prompts: &[Prompt]) -> Result<(), PromptFileError> {
    let mut family_splits: HashMap<&str, String> = HashMap::new();
    for prompt in prompts {
        let Some(family) = prompt.family.as_deref() else {
            continue;
        };
        let split = split_role(prompt.split.as_deref().unwrap_or("unspecified"));
        if let Some(previous) = family_splits.insert(family, split.clone()) {
            if previous != split {
                return Err(PromptFileError(format!(
                    "family `{family}` leaks across splits `{previous}` and `{split}`"
                )));
            }
        }
    }
    Ok(())
}

/// Validate a union of prompt files before revealing a holdout. This catches
/// duplicate ids and families that appear under different split names across
/// separate JSONL files, which per-file parsing alone cannot detect.
pub fn validate_prompt_registry(prompts: &[Prompt]) -> Result<(), PromptFileError> {
    let mut ids = HashSet::new();
    for prompt in prompts {
        if prompt.family.is_none() || prompt.split.is_none() {
            return Err(PromptFileError(format!(
                "prompt `{}` must declare family and split in a registry",
                prompt.id
            )));
        }
        if !ids.insert(prompt.id.as_str()) {
            return Err(PromptFileError(format!(
                "duplicate prompt id `{}` across prompt registry",
                prompt.id
            )));
        }
    }
    validate_family_splits(prompts)?;

    // Literal family labels are editorial and can drift (`road-to-damascus`
    // vs `saul-conversion`). Passage overlap is the stronger leakage key.
    for (index, left) in prompts.iter().enumerate() {
        let left_role = split_role(left.split.as_deref().unwrap_or("unspecified"));
        for right in prompts.iter().skip(index + 1) {
            let right_role = split_role(right.split.as_deref().unwrap_or("unspecified"));
            if left_role == right_role {
                continue;
            }
            if query_fingerprint(&left.prompt) == query_fingerprint(&right.prompt) {
                return Err(PromptFileError(format!(
                    "normalized query leaks across split roles `{left_role}` and `{right_role}`: `{}` vs `{}`",
                    left.id, right.id
                )));
            }
            if left.gold_refs.is_empty() || right.gold_refs.is_empty() {
                continue;
            }
            if left.gold_refs.iter().any(|left_gold| {
                right
                    .gold_refs
                    .iter()
                    .any(|right_gold| refs_match(left_gold, right_gold))
            }) {
                return Err(PromptFileError(format!(
                    "passage family leaks across split roles `{left_role}` and `{right_role}`: `{}` vs `{}`",
                    left.id, right.id
                )));
            }
        }
    }
    Ok(())
}

fn query_fingerprint(query: &str) -> String {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for character in query.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            current.push(character);
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens.join(" ")
}

fn split_role(split: &str) -> String {
    split
        .split_once('-')
        .map(|(role, _)| role)
        .unwrap_or(split)
        .trim()
        .to_ascii_lowercase()
}

#[derive(Debug, Serialize, Clone)]
pub struct PromptScore {
    pub id: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    pub has_gold: bool,
    pub gold_mode: GoldMode,
    pub first_relevant_rank: Option<usize>,
    pub reciprocal_rank: f64,
    pub hit_at_1: bool,
    pub hit_at_3: bool,
    pub hit_at_10: bool,
    /// Present when the result list was evaluated to a depth of at least 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_at_50: Option<bool>,
    /// Present when the result list was evaluated to a depth of at least 100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_at_100: Option<bool>,
    /// Populated only for empty-gold false-memory cases. For verse-recall this
    /// means no top-3 result made a confident `Recovered` claim.
    pub no_gold_safe: Option<bool>,
    /// Whether top-3 confidence claims are safe: every `Recovered` result must
    /// overlap a gold reference; no-gold prompts may have no such claim.
    pub confidence_safe: Option<bool>,
    /// Whether structured passage output met `expected_passages`.
    pub passage_output_ok: Option<bool>,
    /// Whether every declared hard negative ranked below the completed gold
    /// target (or was absent from the retrieved depth).
    pub hard_negative_ok: Option<bool>,
    /// Whether the relevant result carried the explicitly required confidence
    /// label (for example `often-misremembered`).
    pub confidence_label_ok: Option<bool>,
    /// Attribution for the leading result (`lexical`, `expand/alternate`,
    /// `dense`, `phrase/concept`, or `fuse(...)`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane_used: Option<String>,
    pub top_refs: Vec<String>,
    pub gold_refs: Vec<String>,
}

impl PromptScore {
    pub fn passed_primary_check(&self) -> bool {
        let retrieval_or_safety = if self.has_gold {
            self.hit_at_3
        } else {
            self.no_gold_safe.unwrap_or(false)
        };
        retrieval_or_safety
            && self.hard_negative_ok.unwrap_or(true)
            && self.passage_output_ok.unwrap_or(true)
            && self.confidence_label_ok.unwrap_or(true)
    }
}

/// Score one ranked result list. `no_gold_safe` says whether the system avoided
/// a confident scripture claim for an empty-gold prompt. Systems without
/// confidence metadata should conservatively pass `Some(top_refs.is_empty())`.
pub fn score_prompt(
    prompt: &Prompt,
    top_refs: &[String],
    confidence_safe: Option<bool>,
) -> PromptScore {
    score_prompt_at_depth(prompt, top_refs, confidence_safe, top_refs.len())
}

/// Score one ranked result list at an explicit retrieval depth.
///
/// This variant lets evaluators distinguish a miss at a measured deep cutoff
/// from a shallow run that never retrieved enough candidates to measure it.
/// Results beyond `retrieval_depth` are ignored. A system that exhausts its
/// candidates before the requested depth still records a valid miss.
pub fn score_prompt_at_depth(
    prompt: &Prompt,
    top_refs: &[String],
    confidence_safe: Option<bool>,
    retrieval_depth: usize,
) -> PromptScore {
    let top_refs = &top_refs[..top_refs.len().min(retrieval_depth)];
    let has_gold = !prompt.gold_refs.is_empty();
    let gold_ranks: Vec<Option<usize>> = prompt
        .gold_refs
        .iter()
        .map(|gold| {
            top_refs
                .iter()
                .position(|result| refs_match(result, gold))
                .map(|index| index + 1)
        })
        .collect();
    let first_relevant_rank = if has_gold {
        match prompt.gold_mode {
            GoldMode::Any => gold_ranks.iter().flatten().copied().min(),
            GoldMode::All => gold_ranks
                .iter()
                .copied()
                .collect::<Option<Vec<_>>>()
                .and_then(|ranks| ranks.into_iter().max()),
        }
    } else {
        None
    };
    let first_hard_negative_rank = top_refs
        .iter()
        .position(|result| {
            prompt
                .must_not
                .iter()
                .any(|negative| refs_match(result, negative))
        })
        .map(|index| index + 1);
    let hard_negative_ok = if prompt.must_not.is_empty() {
        None
    } else {
        Some(match (first_relevant_rank, first_hard_negative_rank) {
            (_, None) => true,
            (Some(gold_rank), Some(negative_rank)) => negative_rank > gold_rank,
            (None, Some(_)) => false,
        })
    };
    let reciprocal_rank = first_relevant_rank
        .map(|rank| 1.0 / rank as f64)
        .unwrap_or(0.0);

    PromptScore {
        id: prompt.id.clone(),
        category: prompt.category.clone(),
        family: prompt.family.clone(),
        split: prompt.split.clone(),
        has_gold,
        gold_mode: prompt.gold_mode,
        first_relevant_rank,
        reciprocal_rank,
        hit_at_1: first_relevant_rank.is_some_and(|rank| rank <= 1),
        hit_at_3: first_relevant_rank.is_some_and(|rank| rank <= 3),
        hit_at_10: first_relevant_rank.is_some_and(|rank| rank <= 10),
        hit_at_50: (retrieval_depth >= 50)
            .then(|| first_relevant_rank.is_some_and(|rank| rank <= 50)),
        hit_at_100: (retrieval_depth >= 100)
            .then(|| first_relevant_rank.is_some_and(|rank| rank <= 100)),
        no_gold_safe: if has_gold { None } else { confidence_safe },
        confidence_safe,
        passage_output_ok: None,
        hard_negative_ok,
        confidence_label_ok: None,
        lane_used: None,
        top_refs: top_refs.to_vec(),
        gold_refs: prompt.gold_refs.clone(),
    }
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct MetricScore {
    pub queries: usize,
    pub retrieval_queries: usize,
    pub retrieval_queries_at_50: usize,
    pub retrieval_queries_at_100: usize,
    pub no_gold_queries: usize,
    pub hits_at_1: usize,
    pub hits_at_3: usize,
    pub hits_at_10: usize,
    pub hits_at_50: usize,
    pub hits_at_100: usize,
    pub recall_at_1: f64,
    pub recall_at_3: f64,
    pub recall_at_10: f64,
    /// Recall over gold-bearing prompts when every such prompt was evaluated
    /// to at least rank 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recall_at_50: Option<f64>,
    /// Recall over gold-bearing prompts when every such prompt was evaluated
    /// to at least rank 100.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recall_at_100: Option<f64>,
    /// Mean reciprocal rank over gold-bearing prompts and the retrieved depth.
    pub mrr: f64,
    pub safe_no_gold: usize,
    pub no_gold_safety: f64,
    pub safety_queries: usize,
    pub safe_confidence_queries: usize,
    pub confidence_safety: f64,
    pub passage_queries: usize,
    pub passage_passes: usize,
    pub passage_accuracy: f64,
    pub hard_negative_queries: usize,
    pub hard_negative_passes: usize,
    pub hard_negative_accuracy: f64,
    pub confidence_label_queries: usize,
    pub confidence_label_passes: usize,
    pub confidence_label_accuracy: f64,
}

impl MetricScore {
    fn from_scores<'a>(scores: impl Iterator<Item = &'a PromptScore>) -> Self {
        let mut metric = Self::default();
        let mut reciprocal_rank_sum = 0.0;
        for score in scores {
            metric.queries += 1;
            if let Some(safe) = score.confidence_safe {
                metric.safety_queries += 1;
                metric.safe_confidence_queries += usize::from(safe);
            }
            if let Some(ok) = score.passage_output_ok {
                metric.passage_queries += 1;
                metric.passage_passes += usize::from(ok);
            }
            if let Some(ok) = score.hard_negative_ok {
                metric.hard_negative_queries += 1;
                metric.hard_negative_passes += usize::from(ok);
            }
            if let Some(ok) = score.confidence_label_ok {
                metric.confidence_label_queries += 1;
                metric.confidence_label_passes += usize::from(ok);
            }
            if score.has_gold {
                metric.retrieval_queries += 1;
                metric.hits_at_1 += usize::from(score.hit_at_1);
                metric.hits_at_3 += usize::from(score.hit_at_3);
                metric.hits_at_10 += usize::from(score.hit_at_10);
                if let Some(hit) = score.hit_at_50 {
                    metric.retrieval_queries_at_50 += 1;
                    metric.hits_at_50 += usize::from(hit);
                }
                if let Some(hit) = score.hit_at_100 {
                    metric.retrieval_queries_at_100 += 1;
                    metric.hits_at_100 += usize::from(hit);
                }
                reciprocal_rank_sum += score.reciprocal_rank;
            } else if let Some(safe) = score.no_gold_safe {
                metric.no_gold_queries += 1;
                metric.safe_no_gold += usize::from(safe);
            }
        }
        if metric.retrieval_queries > 0 {
            let denominator = metric.retrieval_queries as f64;
            metric.recall_at_1 = metric.hits_at_1 as f64 / denominator;
            metric.recall_at_3 = metric.hits_at_3 as f64 / denominator;
            metric.recall_at_10 = metric.hits_at_10 as f64 / denominator;
            if metric.retrieval_queries_at_50 == metric.retrieval_queries {
                metric.recall_at_50 = Some(metric.hits_at_50 as f64 / denominator);
            }
            if metric.retrieval_queries_at_100 == metric.retrieval_queries {
                metric.recall_at_100 = Some(metric.hits_at_100 as f64 / denominator);
            }
            metric.mrr = reciprocal_rank_sum / denominator;
        }
        if metric.no_gold_queries > 0 {
            metric.no_gold_safety = metric.safe_no_gold as f64 / metric.no_gold_queries as f64;
        }
        if metric.safety_queries > 0 {
            metric.confidence_safety =
                metric.safe_confidence_queries as f64 / metric.safety_queries as f64;
        }
        if metric.passage_queries > 0 {
            metric.passage_accuracy = metric.passage_passes as f64 / metric.passage_queries as f64;
        }
        if metric.hard_negative_queries > 0 {
            metric.hard_negative_accuracy =
                metric.hard_negative_passes as f64 / metric.hard_negative_queries as f64;
        }
        if metric.confidence_label_queries > 0 {
            metric.confidence_label_accuracy =
                metric.confidence_label_passes as f64 / metric.confidence_label_queries as f64;
        }
        metric
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct SystemScores {
    pub name: String,
    pub overall: MetricScore,
    pub by_category: BTreeMap<String, MetricScore>,
    pub by_family: BTreeMap<String, MetricScore>,
    pub per_prompt: Vec<PromptScore>,
}

pub fn summarize_system(name: &str, per_prompt: Vec<PromptScore>) -> SystemScores {
    let overall = MetricScore::from_scores(per_prompt.iter());
    let mut category_members: BTreeMap<String, Vec<&PromptScore>> = BTreeMap::new();
    let mut family_members: BTreeMap<String, Vec<&PromptScore>> = BTreeMap::new();
    for score in &per_prompt {
        category_members
            .entry(score.category.clone())
            .or_default()
            .push(score);
        if let Some(family) = &score.family {
            family_members
                .entry(family.clone())
                .or_default()
                .push(score);
        }
    }
    let by_category = category_members
        .into_iter()
        .map(|(name, scores)| (name, MetricScore::from_scores(scores.into_iter())))
        .collect();
    let by_family = family_members
        .into_iter()
        .map(|(name, scores)| (name, MetricScore::from_scores(scores.into_iter())))
        .collect();

    SystemScores {
        name: name.to_owned(),
        overall,
        by_category,
        by_family,
        per_prompt,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VersePoint {
    chapter: u32,
    verse: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Passage {
    book: String,
    start: VersePoint,
    end: VersePoint,
}

impl Passage {
    fn overlaps(&self, other: &Self) -> bool {
        self.book == other.book
            && point_le(&self.start, &other.end)
            && point_le(&other.start, &self.end)
    }
}

fn point_le(a: &VersePoint, b: &VersePoint) -> bool {
    (a.chapter, a.verse) <= (b.chapter, b.verse)
}

/// Match exact verses, verse/chapter ranges, cross-chapter ranges, aliases, and
/// multi-reference blobs without collapsing `1`/`2`/`3` book prefixes.
pub fn refs_match(a: &str, b: &str) -> bool {
    let a_refs = parse_ref_blob(a);
    let b_refs = parse_ref_blob(b);
    if !a_refs.is_empty() && !b_refs.is_empty() {
        return a_refs
            .iter()
            .any(|left| b_refs.iter().any(|right| left.overlaps(right)));
    }
    loose_ref_key(a) == loose_ref_key(b)
}

/// Require a returned structured range to cover (not merely touch) at least
/// one expected passage. This prevents an orphan anchor verse from satisfying
/// a named-story output requirement.
pub fn passage_range_covers(start_ref: &str, end_ref: &str, expected: &str) -> bool {
    let starts = parse_ref_blob(start_ref);
    let ends = parse_ref_blob(end_ref);
    let expected = parse_ref_blob(expected);
    starts.iter().any(|start| {
        ends.iter().any(|end| {
            if start.book != end.book {
                return false;
            }
            expected.iter().any(|wanted| {
                wanted.book == start.book
                    && point_le(&start.start, &wanted.start)
                    && point_le(&wanted.end, &end.end)
            })
        })
    })
}

/// Return the canonical book names represented by a validated reference blob.
///
/// Evaluation profiles use this to measure target-book diversity without
/// maintaining a second, subtly different Scripture-reference parser.
pub fn reference_books(reference_blob: &str) -> Vec<String> {
    let mut books: Vec<String> = parse_ref_blob(reference_blob)
        .into_iter()
        .map(|passage| passage.book)
        .collect();
    books.sort();
    books.dedup();
    books
}

fn parse_ref_blob(blob: &str) -> Vec<Passage> {
    parse_ref_blob_strict(blob).unwrap_or_default()
}

fn parse_ref_blob_strict(blob: &str) -> Option<Vec<Passage>> {
    let parts: Vec<&str> = blob
        .split([';', ','])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    parts.into_iter().map(parse_passage).collect()
}

fn parse_passage(raw: &str) -> Option<Passage> {
    if raw.is_empty() {
        return None;
    }

    // OSIS forms such as `2CH.7.14` and `GEN.3.1-24` occur in topic data.
    if !raw.contains(':') {
        let osis: Vec<_> = raw.split('.').collect();
        if osis.len() == 3 {
            let humanish = format!("{} {}:{}", osis[0], osis[1], osis[2]);
            return parse_passage(&humanish);
        }
    }

    let normalized = raw.replace(
        [
            '\u{2010}', '\u{2011}', '\u{2012}', '\u{2013}', '\u{2014}', '\u{2212}',
        ],
        "-",
    );
    let tokens: Vec<_> = normalized.split_whitespace().collect();
    let location_index = tokens.iter().rposition(|token| {
        token.chars().any(|ch| ch.is_ascii_digit())
            && token
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == ':' || ch == '-' || ch == '.')
    })?;
    if location_index == 0 {
        return None;
    }
    let book = canonical_book(&tokens[..location_index].join(""))?;
    let location: String = tokens[location_index]
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == ':' || *ch == '-')
        .collect();

    let (start, end) = if let Some((chapter, verse_range)) = location.split_once(':') {
        let chapter: u32 = chapter.parse().ok()?;
        if let Some((start_verse, end_raw)) = verse_range.split_once('-') {
            let start_verse: u32 = start_verse.parse().ok()?;
            let (end_chapter, end_verse) =
                if let Some((end_chapter, end_verse)) = end_raw.split_once(':') {
                    (end_chapter.parse().ok()?, end_verse.parse().ok()?)
                } else {
                    (chapter, end_raw.parse().ok()?)
                };
            (
                VersePoint {
                    chapter,
                    verse: start_verse,
                },
                VersePoint {
                    chapter: end_chapter,
                    verse: end_verse,
                },
            )
        } else {
            let verse: u32 = verse_range.parse().ok()?;
            (VersePoint { chapter, verse }, VersePoint { chapter, verse })
        }
    } else if let Some((start_chapter, end_chapter)) = location.split_once('-') {
        (
            VersePoint {
                chapter: start_chapter.parse().ok()?,
                verse: 1,
            },
            VersePoint {
                chapter: end_chapter.parse().ok()?,
                verse: 999,
            },
        )
    } else {
        let chapter: u32 = location.parse().ok()?;
        (
            VersePoint { chapter, verse: 1 },
            VersePoint {
                chapter,
                verse: 999,
            },
        )
    };
    if !point_le(&start, &end) {
        return None;
    }
    Some(Passage { book, start, end })
}

fn loose_ref_key(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn canonical_book(raw: &str) -> Option<String> {
    let key: String = raw
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    let canonical = match key.as_str() {
        "gen" | "gn" | "genesis" => "genesis",
        "ex" | "exo" | "exod" | "exodus" => "exodus",
        "lev" | "leviticus" => "leviticus",
        "num" | "numbers" => "numbers",
        "deut" | "dt" | "deuteronomy" => "deuteronomy",
        "josh" | "jos" | "joshua" => "joshua",
        "judg" | "jdg" | "judges" => "judges",
        "ruth" | "rut" => "ruth",
        "1sam" | "1sa" | "1samuel" | "firstsamuel" => "1samuel",
        "2sam" | "2sa" | "2samuel" | "secondsamuel" => "2samuel",
        "1kgs" | "1ki" | "1kings" | "firstkings" => "1kings",
        "2kgs" | "2ki" | "2kings" | "secondkings" => "2kings",
        "1chr" | "1ch" | "1chron" | "1chronicles" | "firstchronicles" => "1chronicles",
        "2chr" | "2ch" | "2chron" | "2chronicles" | "secondchronicles" => "2chronicles",
        "ezra" | "ezr" => "ezra",
        "neh" | "nehemiah" => "nehemiah",
        "est" | "esth" | "esther" => "esther",
        "job" => "job",
        "ps" | "psa" | "psalm" | "psalms" => "psalm",
        "prov" | "pr" | "pro" | "proverbs" => "proverbs",
        "eccl" | "ecc" | "ecclesiastes" => "ecclesiastes",
        "song" | "sos" | "songofsongs" | "songofsolomon" => "songofsolomon",
        "isa" | "is" | "isaiah" => "isaiah",
        "jer" | "jeremiah" => "jeremiah",
        "lam" | "lamentations" => "lamentations",
        "ezek" | "ezk" | "eze" | "ezekiel" => "ezekiel",
        "dan" | "daniel" => "daniel",
        "hos" | "hosea" => "hosea",
        "joel" | "jol" => "joel",
        "amos" | "amo" => "amos",
        "obad" | "obadiah" => "obadiah",
        "jon" | "jonah" => "jonah",
        "mic" | "micah" => "micah",
        "nah" | "nahum" => "nahum",
        "hab" | "habakkuk" => "habakkuk",
        "zeph" | "zep" | "zephaniah" => "zephaniah",
        "hag" | "haggai" => "haggai",
        "zech" | "zec" | "zechariah" => "zechariah",
        "mal" | "malachi" => "malachi",
        "matt" | "mat" | "mt" | "matthew" => "matthew",
        "mark" | "mar" | "mk" => "mark",
        "luke" | "luk" | "lk" => "luke",
        "john" | "joh" | "jhn" | "jn" => "john",
        "acts" | "act" | "ac" => "acts",
        "rom" | "ro" | "romans" => "romans",
        "1cor" | "1co" | "1corinthians" | "firstcorinthians" => "1corinthians",
        "2cor" | "2co" | "2corinthians" | "secondcorinthians" => "2corinthians",
        "gal" | "galatians" => "galatians",
        "eph" | "ephesians" => "ephesians",
        "phil" | "php" | "philippians" => "philippians",
        "col" | "colossians" => "colossians",
        "1thess" | "1th" | "1thessalonians" | "firstthessalonians" => "1thessalonians",
        "2thess" | "2th" | "2thessalonians" | "secondthessalonians" => "2thessalonians",
        "1tim" | "1ti" | "1timothy" | "firsttimothy" => "1timothy",
        "2tim" | "2ti" | "2timothy" | "secondtimothy" => "2timothy",
        "titus" | "tit" => "titus",
        "phlm" | "phm" | "philemon" => "philemon",
        "heb" | "hebrews" => "hebrews",
        "jas" | "jam" | "james" => "james",
        "1pet" | "1pe" | "1peter" | "firstpeter" => "1peter",
        "2pet" | "2pe" | "2peter" | "secondpeter" => "2peter",
        "1john" | "1jn" | "1jo" | "firstjohn" => "1john",
        "2john" | "2jn" | "2jo" | "secondjohn" => "2john",
        "3john" | "3jn" | "3jo" | "thirdjohn" => "3john",
        "jude" | "jud" => "jude",
        "rev" | "revelation" => "revelation",
        _ => return None,
    };
    Some(canonical.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(gold_refs: &[&str]) -> Prompt {
        Prompt {
            id: "case".into(),
            prompt: "query".into(),
            category: "test".into(),
            gold_refs: gold_refs
                .iter()
                .map(|reference| (*reference).into())
                .collect(),
            gold_mode: GoldMode::Any,
            must_not: Vec::new(),
            expected_confidence: None,
            expected_passages: Vec::new(),
            family: Some("family".into()),
            split: Some("holdout".into()),
            tags: Vec::new(),
            source: None,
            notes: None,
        }
    }

    #[test]
    fn numbered_books_do_not_collapse() {
        assert!(!refs_match("1 Chronicles 7:14", "2 Chronicles 7:14"));
        assert!(!refs_match("1 Corinthians 13:4", "2 Corinthians 13:4"));
        assert!(refs_match("2 Chr 7:13-15", "2 Chronicles 7:14"));
        assert!(refs_match("2CH.7.13-15", "2 Chronicles 7:14"));
    }

    #[test]
    fn ranges_overlap_in_either_direction() {
        assert!(refs_match("Genesis 3:7", "Genesis 3:1-24"));
        assert!(refs_match("Genesis 3:1-24", "Genesis 3:7"));
        assert!(refs_match("Matthew 6:9", "Matthew 5:1-7:29"));
        assert!(refs_match("Matthew 5-7", "Matthew 6:9-13"));
        assert!(!refs_match("Matthew 8:1", "Matthew 5:1-7:29"));
    }

    #[test]
    fn structured_passage_must_cover_expected_range() {
        assert!(passage_range_covers(
            "Exodus 19:1",
            "Exodus 20:17",
            "Exodus 19:1-20:17"
        ));
        assert!(!passage_range_covers(
            "Exodus 20:6",
            "Exodus 20:6",
            "Exodus 19:1-20:17"
        ));
    }

    #[test]
    fn aliases_and_multi_ref_blobs_match() {
        assert!(refs_match("Ps 23:4", "Psalm 23:1-6"));
        assert!(refs_match("Romans 5:12; Genesis 3:1-24", "Genesis 3:8"));
        assert!(!refs_match("John 3:16", "1 John 3:16"));
        assert_eq!(
            reference_books("Romans 5:12; GEN.3.1-24; Romans 8:1"),
            ["genesis", "romans"]
        );
    }

    #[test]
    fn scoring_computes_rank_cutoffs() {
        let scored = score_prompt(
            &prompt(&["2 Corinthians 12:9"]),
            &[
                "Romans 8:28".into(),
                "Philippians 4:13".into(),
                "2 Corinthians 12:7-10".into(),
            ],
            None,
        );
        assert_eq!(scored.first_relevant_rank, Some(3));
        assert!(!scored.hit_at_1);
        assert!(scored.hit_at_3);
        assert!(scored.hit_at_10);
        assert_eq!(scored.hit_at_50, None);
        assert_eq!(scored.hit_at_100, None);
        assert!((scored.reciprocal_rank - (1.0 / 3.0)).abs() < f64::EPSILON);
    }

    fn ranked_results(depth: usize, relevant_rank: Option<usize>) -> Vec<String> {
        let mut results = vec!["Genesis 1:1".into(); depth];
        if let Some(rank) = relevant_rank {
            results[rank - 1] = "John 3:16".into();
        }
        results
    }

    #[test]
    fn deep_cutoffs_require_an_explicitly_sufficient_depth() {
        let gold = prompt(&["John 3:16"]);
        let results = ranked_results(100, Some(60));

        let shallow = score_prompt_at_depth(&gold, &results, None, 10);
        assert_eq!(shallow.first_relevant_rank, None);
        assert_eq!(shallow.hit_at_50, None);
        assert_eq!(shallow.hit_at_100, None);

        let at_fifty = score_prompt_at_depth(&gold, &results, None, 50);
        assert_eq!(at_fifty.first_relevant_rank, None);
        assert_eq!(at_fifty.hit_at_50, Some(false));
        assert_eq!(at_fifty.hit_at_100, None);

        let at_one_hundred = score_prompt_at_depth(&gold, &results, None, 100);
        assert_eq!(at_one_hundred.first_relevant_rank, Some(60));
        assert_eq!(at_one_hundred.hit_at_50, Some(false));
        assert_eq!(at_one_hundred.hit_at_100, Some(true));
    }

    #[test]
    fn summary_reports_recall_at_50_and_100() {
        let gold = prompt(&["John 3:16"]);
        let scores = [Some(1), Some(42), Some(88), None]
            .into_iter()
            .map(|rank| {
                let results = ranked_results(100, rank);
                score_prompt_at_depth(&gold, &results, None, 100)
            })
            .collect();

        let summary = summarize_system("deep", scores);
        assert_eq!(summary.overall.retrieval_queries_at_50, 4);
        assert_eq!(summary.overall.retrieval_queries_at_100, 4);
        assert_eq!(summary.overall.hits_at_50, 2);
        assert_eq!(summary.overall.hits_at_100, 3);
        assert_eq!(summary.overall.recall_at_50, Some(0.5));
        assert_eq!(summary.overall.recall_at_100, Some(0.75));

        let json = serde_json::to_value(&summary).expect("serialize deep metrics");
        assert_eq!(json["overall"]["recall_at_50"], 0.5);
        assert_eq!(json["overall"]["recall_at_100"], 0.75);
        assert_eq!(json["per_prompt"][2]["hit_at_100"], true);
    }

    #[test]
    fn shallow_json_omits_unmeasured_deep_metrics() {
        let scored = score_prompt(
            &prompt(&["John 3:16"]),
            &["Genesis 1:1".into(), "John 3:16".into()],
            None,
        );
        let summary = summarize_system("shallow", vec![scored]);
        assert_eq!(summary.overall.recall_at_50, None);
        assert_eq!(summary.overall.recall_at_100, None);

        let json = serde_json::to_value(&summary).expect("serialize shallow metrics");
        assert!(json["overall"].get("recall_at_50").is_none());
        assert!(json["overall"].get("recall_at_100").is_none());
        assert!(json["per_prompt"][0].get("hit_at_50").is_none());
        assert!(json["per_prompt"][0].get("hit_at_100").is_none());
    }

    #[test]
    fn all_gold_mode_and_hard_negatives_are_enforced() {
        let mut conflation = prompt(&["John 3:16", "Genesis 1:1"]);
        conflation.gold_mode = GoldMode::All;
        conflation.must_not = vec!["Romans 8:28".into()];

        let incomplete = score_prompt(
            &conflation,
            &["John 3:16".into(), "Romans 8:28".into()],
            None,
        );
        assert_eq!(incomplete.first_relevant_rank, None);
        assert_eq!(incomplete.hard_negative_ok, Some(false));

        let complete = score_prompt(
            &conflation,
            &[
                "John 3:16".into(),
                "Genesis 1:1".into(),
                "Romans 8:28".into(),
            ],
            None,
        );
        assert_eq!(complete.first_relevant_rank, Some(2));
        assert_eq!(complete.hard_negative_ok, Some(true));
    }

    #[test]
    fn prompt_parser_rejects_unknown_books_and_malformed_segments() {
        for gold in ["Narnia 3:16", "Genesis 3:1; typo"] {
            let raw = format!(
                "{{\"id\":\"bad\",\"prompt\":\"query\",\"category\":\"test\",\"gold_refs\":[{gold:?}]}}"
            );
            assert!(parse_prompts(&raw)
                .expect_err("invalid gold must fail")
                .to_string()
                .contains("unparseable gold reference"));
        }
    }

    #[test]
    fn prompt_parser_rejects_unknown_fields_and_overlapping_hard_negatives() {
        let typo = r#"{"id":"bad","prompt":"query","category":"test","gold_refs":["John 3:16"],"gold_modes":"all"}"#;
        assert!(parse_prompts(typo)
            .expect_err("unknown field must fail")
            .to_string()
            .contains("unknown field"));

        let overlap = r#"{"id":"bad","prompt":"query","category":"test","gold_refs":["John 3:16"],"must_not":["John 3:1-20"]}"#;
        assert!(parse_prompts(overlap)
            .expect_err("gold/hard-negative overlap must fail")
            .to_string()
            .contains("overlaps gold"));
    }

    #[test]
    fn metrics_exclude_empty_gold_from_recall() {
        let hit = score_prompt(&prompt(&["John 3:16"]), &["John 3:16".into()], None);
        let mut no_gold_prompt = prompt(&[]);
        no_gold_prompt.id = "no-gold".into();
        let safe = score_prompt(&no_gold_prompt, &["Proverbs 1:1".into()], Some(true));
        let summary = summarize_system("test", vec![hit, safe]);
        assert_eq!(summary.overall.queries, 2);
        assert_eq!(summary.overall.retrieval_queries, 1);
        assert_eq!(summary.overall.recall_at_1, 1.0);
        assert_eq!(summary.overall.no_gold_safety, 1.0);
    }

    #[test]
    fn summary_reports_all_cutoffs_and_mrr() {
        let gold = prompt(&["John 3:16"]);
        let ranked = [
            vec!["John 3:16".into()],
            vec!["Genesis 1:1".into(), "John 3:16".into()],
            vec![
                "Genesis 1:1".into(),
                "Exodus 1:1".into(),
                "Leviticus 1:1".into(),
                "Numbers 1:1".into(),
                "Deuteronomy 1:1".into(),
                "Joshua 1:1".into(),
                "John 3:16".into(),
            ],
            vec!["Genesis 1:1".into()],
        ];
        let scores = ranked
            .iter()
            .map(|results| score_prompt(&gold, results, None))
            .collect();
        let summary = summarize_system("test", scores);
        assert_eq!(summary.overall.recall_at_1, 0.25);
        assert_eq!(summary.overall.recall_at_3, 0.5);
        assert_eq!(summary.overall.recall_at_10, 0.75);
        let expected_mrr = (1.0 + 0.5 + (1.0 / 7.0)) / 4.0;
        assert!((summary.overall.mrr - expected_mrr).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_family_leakage_across_splits() {
        let raw = r#"
{"id":"a","prompt":"one","category":"phrase","gold_refs":["John 3:16"],"family":"john-3-16","split":"dev"}
{"id":"b","prompt":"two","category":"phrase","gold_refs":["John 3:16"],"family":"john-3-16","split":"holdout"}
"#;
        let err = parse_prompts(raw).unwrap_err().to_string();
        assert!(err.contains("leaks across splits"));
    }

    #[test]
    fn registry_rejects_leakage_across_separate_files() {
        let dev = parse_prompts(
            r#"{"id":"dev-a","prompt":"one","category":"phrase","gold_refs":["John 3:16"],"family":"john-3-16","split":"regression"}"#,
        )
        .unwrap();
        let holdout = parse_prompts(
            r#"{"id":"holdout-a","prompt":"two","category":"phrase","gold_refs":["John 3:16"],"family":"john-3-16","split":"holdout-v2"}"#,
        )
        .unwrap();
        let registry: Vec<Prompt> = dev.into_iter().chain(holdout).collect();

        let err = validate_prompt_registry(&registry).unwrap_err().to_string();
        assert!(err.contains("leaks across splits"));
    }

    #[test]
    fn registry_rejects_normalized_query_leakage_even_without_gold() {
        let regression = parse_prompts(
            r#"{"id":"dev-empty","prompt":"God helps those who help themselves!","category":"false-memory","gold_refs":[],"family":"no-gold-dev","split":"regression"}"#,
        )
        .unwrap();
        let holdout = parse_prompts(
            r#"{"id":"holdout-empty","prompt":"god helps those who help themselves","category":"false-memory","gold_refs":[],"family":"no-gold-holdout","split":"holdout"}"#,
        )
        .unwrap();
        let error =
            validate_prompt_registry(&regression.into_iter().chain(holdout).collect::<Vec<_>>())
                .expect_err("normalized duplicate query must fail");
        assert!(error.to_string().contains("normalized query leaks"));
    }

    #[test]
    fn registry_rejects_overlapping_passages_with_different_family_labels() {
        let regression = parse_prompts(
            r#"{"id":"a","prompt":"road","category":"event","gold_refs":["Acts 9:1-19"],"family":"road-to-damascus","split":"regression-v1"}"#,
        )
        .unwrap();
        let holdout = parse_prompts(
            r#"{"id":"b","prompt":"conversion","category":"event","gold_refs":["Acts 9:1-43"],"family":"saul-conversion","split":"holdout-v2"}"#,
        )
        .unwrap();
        let registry: Vec<Prompt> = regression.into_iter().chain(holdout).collect();
        let error = validate_prompt_registry(&registry).unwrap_err().to_string();
        assert!(error.contains("passage family leaks"));
    }
}

//! Iterative red-team evaluation for scripture recall.
//!
//! Kill criteria:
//! - top-3 contains gold ref
//! - false-memory category never presented as confidence=recovered for empty-gold
//!
//! Also builds an offline keyword baseline on the same pack TEXT and optional
//! live-tool score card for repeatable red-team comparison.

use clap::Parser;
use recall_core::{
    plan_query_segments, Confidence, DenseLane, Engine, QuerySegmentEmbedding, SearchOpts,
    SearchResult, MAX_TOP_K, QUERY_SEGMENT_PLANNER,
};
use recall_eval::{
    parse_prompts, passage_range_covers, reference_books, refs_match, score_prompt_at_depth,
    summarize_system, validate_prompt_registry, ExpectedConfidence, GoldMode, MetricScore, Prompt,
    PromptScore, SystemScores,
};
use recall_pack::{PackReader, SectionTag};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "recall-eval",
    about = "Iterative Recall@1/3/10/50/100 and MRR red-team evaluation"
)]
struct Cli {
    /// Pack file
    #[arg(long, short)]
    pack: PathBuf,
    /// Overlay pack; repeat for hidden translation/concept lanes.
    #[arg(long)]
    overlay: Vec<PathBuf>,
    /// Prompts JSONL (default: eval/tot-prompts.jsonl)
    #[arg(long, default_value = "eval/tot-prompts.jsonl")]
    prompts: PathBuf,
    /// Retrieval depth. Values below 10 are promoted to 10. R@50 and R@100
    /// are reported when the requested depth reaches those cutoffs.
    #[arg(long, default_value_t = 10)]
    top_k: usize,
    /// Evaluate only this dataset split (for example `holdout`).
    #[arg(long)]
    split: Option<String>,
    /// Existing prompt corpus used only to reject cross-file family/id leakage;
    /// repeat for every regression/dev registry file when revealing a holdout.
    #[arg(long)]
    family_registry: Vec<PathBuf>,
    /// Optional path to write markdown comparison report
    #[arg(long)]
    report: Option<PathBuf>,
    /// Optional JSONL of live-tool results:
    /// {"id","tool","top_refs":["..."]} per line
    #[arg(long)]
    live_results: Option<PathBuf>,
    /// Optional prompt-bound JSONL `{ "id", "prompt_utf8_sha256", "embedding" }` for Lane C.
    #[arg(long)]
    query_embeddings: Option<PathBuf>,
    /// LVEC v2 encoder fingerprint (exactly 64 hexadecimal characters).
    /// Omit only when evaluating a legacy LVEC v1 artifact.
    #[arg(long)]
    embedding_space_id: Option<String>,
    /// Report the raw LVEC candidate recall before lexical fusion. This is a
    /// diagnostic for encoder selection, not a release configuration.
    #[arg(long)]
    dense_diagnostic: bool,
    /// Report the CLEX/CPOST Study Note sparse lane without canonical lexical,
    /// expansion, overlay, or dense retrieval.
    #[arg(long)]
    context_diagnostic: bool,
    /// Also print offline keyword baseline scores
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    baseline: bool,
    /// Enforce absolute release bars: R@1 >= 60%, R@3 >= 85%, R@10 >= 95%,
    /// 100% safety/correction checks, and the sealed semantic profile.
    #[arg(
        long,
        requires_all = [
            "release_profile",
            "query_embeddings",
            "embedding_space_id",
            "family_registry"
        ],
        conflicts_with = "compatibility_gate"
    )]
    release_gate: bool,
    /// Strict JSON profile that binds the release artifacts and minimum suite
    /// diversity. This is valid only with `--release-gate`.
    #[arg(long, requires = "release_gate")]
    release_profile: Option<PathBuf>,
    /// Legacy/revealed-suite compatibility floor. Unlike `--release-gate`,
    /// this does not establish generalized semantic quality.
    #[arg(long, conflicts_with = "release_gate")]
    compatibility_gate: bool,
}

const RELEASE_RECALL_AT_1: f64 = 0.60;
const RELEASE_RECALL_AT_3: f64 = 0.85;
const RELEASE_RECALL_AT_10: f64 = 0.95;
const RELEASE_MIN_TOTAL_PROMPTS: usize = 80;
const RELEASE_MIN_RETRIEVAL_PROMPTS: usize = 60;
const RELEASE_MIN_NO_GOLD_PROMPTS: usize = 8;
const RELEASE_MIN_HARD_NEGATIVE_PROMPTS: usize = 8;
const RELEASE_MIN_CONFIDENCE_LABEL_PROMPTS: usize = 4;
const RELEASE_MIN_STRUCTURED_PASSAGE_PROMPTS: usize = 8;
const RELEASE_MIN_DISTINCT_FAMILIES: usize = 70;
const RELEASE_MIN_DISTINCT_CATEGORIES: usize = 10;
const RELEASE_MIN_DISTINCT_TARGET_BOOKS: usize = 40;
const RELEASE_CATEGORY_FLOORS: [(&str, usize); 6] = [
    ("meaning-only", 12),
    ("multi-intent", 8),
    ("event-description", 8),
    ("near-quote", 5),
    ("translation-drift", 5),
    ("entity-event", 5),
];

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct SemanticReleaseProfile {
    schema_version: u32,
    base_pack_sha256: String,
    overlay_sha256: Vec<String>,
    embedding_space_id: String,
    minimums: ReleaseCoverageMinimums,
    required_categories: BTreeMap<String, usize>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReleaseCoverageMinimums {
    total_prompts: usize,
    retrieval_prompts: usize,
    no_gold_prompts: usize,
    hard_negative_prompts: usize,
    confidence_label_prompts: usize,
    structured_passage_prompts: usize,
    distinct_families: usize,
    distinct_categories: usize,
    distinct_target_books: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ReleaseCoverage {
    total_prompts: usize,
    retrieval_prompts: usize,
    no_gold_prompts: usize,
    hard_negative_prompts: usize,
    confidence_label_prompts: usize,
    structured_passage_prompts: usize,
    distinct_families: usize,
    distinct_categories: usize,
    distinct_target_books: usize,
    categories: BTreeMap<String, usize>,
}

#[derive(Debug, Default, Clone)]
struct SemanticRuntimeEvidence {
    base_pack_sha256: String,
    overlay_sha256: Vec<String>,
    cli_embedding_space_id: Option<String>,
    dense_embedding_space_id: Option<String>,
    dense_dimensions: Option<usize>,
    query_embedding_count: usize,
    query_embedding_dimensions: Vec<usize>,
    query_embedding_ids: HashSet<String>,
    family_registry_count: usize,
    holdout_only: bool,
}

#[derive(Debug, Deserialize)]
struct LiveResult {
    id: String,
    tool: String,
    top_refs: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QueryEmbeddingRecord {
    id: String,
    prompt_utf8_sha256: String,
    embedding: Vec<f32>,
    #[serde(default)]
    planner: Option<String>,
    #[serde(default)]
    segments: Vec<QuerySegmentRecord>,
}

#[derive(Debug, Deserialize)]
struct QuerySegmentRecord {
    text: String,
    embedding: Vec<f32>,
}

#[derive(Debug)]
struct QueryEmbedding {
    prompt_utf8_sha256: [u8; 32],
    embedding: Vec<f32>,
    segments: Vec<QuerySegmentRecord>,
}

fn parse_prompt_utf8_sha256(value: &str) -> Result<[u8; 32], &'static str> {
    if value.len() != 64
        || value
            .as_bytes()
            .iter()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("prompt_utf8_sha256 must be 64 lowercase hexadecimal characters");
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = (pair[0] as char)
            .to_digit(16)
            .ok_or("invalid hexadecimal digest")?;
        let low = (pair[1] as char)
            .to_digit(16)
            .ok_or("invalid hexadecimal digest")?;
        digest[index] = ((high << 4) | low) as u8;
    }
    Ok(digest)
}

fn prompt_utf8_sha256(prompt: &str) -> [u8; 32] {
    Sha256::digest(prompt.as_bytes()).into()
}

fn parse_query_embeddings(
    bytes: Option<&[u8]>,
) -> Result<HashMap<String, QueryEmbedding>, Box<dyn std::error::Error>> {
    let Some(bytes) = bytes else {
        return Ok(HashMap::new());
    };
    let raw = std::str::from_utf8(bytes)?;
    let mut records = HashMap::new();
    let mut dimensions = None;
    for (line_index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: QueryEmbeddingRecord = serde_json::from_str(line)
            .map_err(|error| format!("query embeddings line {}: {error}", line_index + 1))?;
        let prompt_utf8_sha256 = parse_prompt_utf8_sha256(&record.prompt_utf8_sha256)
            .map_err(|error| format!("query embeddings line {}: {error}", line_index + 1))?;
        if record.id.trim().is_empty()
            || record.embedding.is_empty()
            || record.embedding.iter().any(|value| !value.is_finite())
        {
            return Err(format!(
                "query embeddings line {} has an invalid id or vector",
                line_index + 1
            )
            .into());
        }
        if let Some(expected) = dimensions {
            if record.embedding.len() != expected {
                return Err(format!(
                    "query embeddings line {} has inconsistent dimensions",
                    line_index + 1
                )
                .into());
            }
        } else {
            dimensions = Some(record.embedding.len());
        }
        if !record.segments.is_empty() && record.planner.as_deref() != Some(QUERY_SEGMENT_PLANNER) {
            return Err(format!(
                "query embeddings line {} has segments without the supported planner",
                line_index + 1
            )
            .into());
        }
        if record.segments.iter().any(|segment| {
            segment.text.trim().is_empty()
                || segment.embedding.len() != record.embedding.len()
                || segment.embedding.iter().any(|value| !value.is_finite())
        }) {
            return Err(format!(
                "query embeddings line {} has an invalid segment text or vector",
                line_index + 1
            )
            .into());
        }
        let id = record.id.clone();
        if records
            .insert(
                id.clone(),
                QueryEmbedding {
                    prompt_utf8_sha256,
                    embedding: record.embedding,
                    segments: record.segments,
                },
            )
            .is_some()
        {
            return Err(format!("duplicate query embedding id {id}").into());
        }
    }
    Ok(records)
}

fn validate_query_embedding_plans(
    prompts: &[Prompt],
    records: &HashMap<String, QueryEmbedding>,
) -> Result<(), Box<dyn std::error::Error>> {
    for prompt in prompts {
        let Some(record) = records.get(&prompt.id) else {
            continue;
        };
        if record.prompt_utf8_sha256 != prompt_utf8_sha256(&prompt.prompt) {
            return Err(format!(
                "query embedding {} prompt_utf8_sha256 does not match the exact prompt UTF-8 bytes",
                prompt.id
            )
            .into());
        }
        if record.segments.is_empty() {
            continue;
        }

        let expected = plan_query_segments(&prompt.prompt).segments;
        if expected.len() != record.segments.len()
            || !expected
                .iter()
                .map(String::as_str)
                .eq(record.segments.iter().map(|segment| segment.text.as_str()))
        {
            return Err(format!(
                "query embedding {} segment texts do not exactly match the canonical ordered query-only planner output",
                prompt.id
            )
            .into());
        }
    }
    Ok(())
}

fn parse_semantic_release_profile(
    bytes: &[u8],
) -> Result<SemanticReleaseProfile, Box<dyn std::error::Error>> {
    let profile: SemanticReleaseProfile = serde_json::from_slice(bytes)
        .map_err(|error| format!("invalid semantic release profile: {error}"))?;
    validate_release_profile_strength(&profile).map_err(|error| -> Box<dyn std::error::Error> {
        format!("invalid semantic release profile: {error}").into()
    })?;
    Ok(profile)
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_release_profile_strength(profile: &SemanticReleaseProfile) -> Result<(), String> {
    if profile.schema_version != 1 {
        return Err(format!(
            "unsupported schema_version {}; expected 1",
            profile.schema_version
        ));
    }
    if !is_lower_sha256(&profile.base_pack_sha256) {
        return Err("base_pack_sha256 must be 64 lowercase hexadecimal characters".into());
    }
    if profile.overlay_sha256.is_empty() {
        return Err("overlay_sha256 must bind at least the dense artifact".into());
    }
    let overlay_digests: HashSet<&str> =
        profile.overlay_sha256.iter().map(String::as_str).collect();
    if overlay_digests.len() != profile.overlay_sha256.len() {
        return Err("overlay_sha256 must not contain duplicate artifacts".into());
    }
    if profile
        .overlay_sha256
        .iter()
        .any(|digest| !is_lower_sha256(digest))
    {
        return Err(
            "every overlay_sha256 value must be 64 lowercase hexadecimal characters".into(),
        );
    }
    if !is_lower_sha256(&profile.embedding_space_id)
        || parse_embedding_space_id(&profile.embedding_space_id).is_err()
    {
        return Err("embedding_space_id must be a nonzero lowercase SHA-256-shaped ID".into());
    }

    let minimums = &profile.minimums;
    for (name, actual, floor) in [
        (
            "total_prompts",
            minimums.total_prompts,
            RELEASE_MIN_TOTAL_PROMPTS,
        ),
        (
            "retrieval_prompts",
            minimums.retrieval_prompts,
            RELEASE_MIN_RETRIEVAL_PROMPTS,
        ),
        (
            "no_gold_prompts",
            minimums.no_gold_prompts,
            RELEASE_MIN_NO_GOLD_PROMPTS,
        ),
        (
            "hard_negative_prompts",
            minimums.hard_negative_prompts,
            RELEASE_MIN_HARD_NEGATIVE_PROMPTS,
        ),
        (
            "confidence_label_prompts",
            minimums.confidence_label_prompts,
            RELEASE_MIN_CONFIDENCE_LABEL_PROMPTS,
        ),
        (
            "structured_passage_prompts",
            minimums.structured_passage_prompts,
            RELEASE_MIN_STRUCTURED_PASSAGE_PROMPTS,
        ),
        (
            "distinct_families",
            minimums.distinct_families,
            RELEASE_MIN_DISTINCT_FAMILIES,
        ),
        (
            "distinct_categories",
            minimums.distinct_categories,
            RELEASE_MIN_DISTINCT_CATEGORIES,
        ),
        (
            "distinct_target_books",
            minimums.distinct_target_books,
            RELEASE_MIN_DISTINCT_TARGET_BOOKS,
        ),
    ] {
        if actual < floor {
            return Err(format!(
                "minimums.{name}={actual} weakens the hard floor {floor}"
            ));
        }
    }
    if profile
        .required_categories
        .iter()
        .any(|(category, minimum)| category.trim().is_empty() || *minimum == 0)
    {
        return Err("required_categories must have nonempty names and positive counts".into());
    }
    for (category, floor) in RELEASE_CATEGORY_FLOORS {
        let actual = profile
            .required_categories
            .get(category)
            .copied()
            .unwrap_or(0);
        if actual < floor {
            return Err(format!(
                "required_categories.{category}={actual} weakens the hard floor {floor}"
            ));
        }
    }
    Ok(())
}

fn release_coverage(prompts: &[Prompt]) -> ReleaseCoverage {
    let mut coverage = ReleaseCoverage {
        total_prompts: prompts.len(),
        ..ReleaseCoverage::default()
    };
    let mut families = HashSet::new();
    let mut books = HashSet::new();
    for prompt in prompts {
        *coverage
            .categories
            .entry(prompt.category.clone())
            .or_default() += 1;
        if prompt.gold_refs.is_empty() {
            coverage.no_gold_prompts += 1;
        } else {
            coverage.retrieval_prompts += 1;
        }
        coverage.hard_negative_prompts += usize::from(!prompt.must_not.is_empty());
        coverage.confidence_label_prompts += usize::from(prompt.expected_confidence.is_some());
        coverage.structured_passage_prompts += usize::from(!prompt.expected_passages.is_empty());
        if let Some(family) = &prompt.family {
            families.insert(family.clone());
        }
        for reference in &prompt.gold_refs {
            books.extend(reference_books(reference));
        }
    }
    coverage.distinct_families = families.len();
    coverage.distinct_categories = coverage.categories.len();
    coverage.distinct_target_books = books.len();
    coverage
}

fn require_coverage(label: &str, actual: usize, minimum: usize) -> Result<(), String> {
    if actual < minimum {
        Err(format!(
            "semantic release coverage {label}={actual}; requires at least {minimum}"
        ))
    } else {
        Ok(())
    }
}

fn validate_release_coverage(
    profile: &SemanticReleaseProfile,
    prompts: &[Prompt],
) -> Result<ReleaseCoverage, String> {
    let coverage = release_coverage(prompts);
    let minimums = &profile.minimums;
    for (label, actual, minimum) in [
        (
            "total_prompts",
            coverage.total_prompts,
            minimums.total_prompts,
        ),
        (
            "retrieval_prompts",
            coverage.retrieval_prompts,
            minimums.retrieval_prompts,
        ),
        (
            "no_gold_prompts",
            coverage.no_gold_prompts,
            minimums.no_gold_prompts,
        ),
        (
            "hard_negative_prompts",
            coverage.hard_negative_prompts,
            minimums.hard_negative_prompts,
        ),
        (
            "confidence_label_prompts",
            coverage.confidence_label_prompts,
            minimums.confidence_label_prompts,
        ),
        (
            "structured_passage_prompts",
            coverage.structured_passage_prompts,
            minimums.structured_passage_prompts,
        ),
        (
            "distinct_families",
            coverage.distinct_families,
            minimums.distinct_families,
        ),
        (
            "distinct_categories",
            coverage.distinct_categories,
            minimums.distinct_categories,
        ),
        (
            "distinct_target_books",
            coverage.distinct_target_books,
            minimums.distinct_target_books,
        ),
    ] {
        require_coverage(label, actual, minimum)?;
    }
    for (category, minimum) in &profile.required_categories {
        require_coverage(
            &format!("category[{category}]"),
            coverage.categories.get(category).copied().unwrap_or(0),
            *minimum,
        )?;
    }
    Ok(coverage)
}

fn validate_semantic_release_inputs(
    profile: &SemanticReleaseProfile,
    prompts: &[Prompt],
    evidence: &SemanticRuntimeEvidence,
) -> Result<ReleaseCoverage, String> {
    validate_release_profile_strength(profile)?;
    if evidence.family_registry_count == 0 {
        return Err("semantic release gate requires at least one family registry".into());
    }
    if !evidence.holdout_only {
        return Err(
            "semantic release gate accepts only selected prompts whose split role is holdout; use --compatibility-gate for revealed suites"
                .into(),
        );
    }
    if evidence.base_pack_sha256 != profile.base_pack_sha256 {
        return Err("semantic release profile base-pack SHA-256 does not match runtime".into());
    }
    let mut expected_overlays = profile.overlay_sha256.clone();
    let mut actual_overlays = evidence.overlay_sha256.clone();
    expected_overlays.sort();
    actual_overlays.sort();
    if actual_overlays.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("runtime overlay set contains duplicate artifacts".into());
    }
    if actual_overlays != expected_overlays {
        return Err("semantic release profile overlay SHA-256 set does not match runtime".into());
    }
    let cli_space = evidence
        .cli_embedding_space_id
        .as_deref()
        .ok_or("semantic release gate requires --embedding-space-id")?;
    if cli_space != profile.embedding_space_id {
        return Err("CLI embedding-space ID does not match semantic release profile".into());
    }
    let dense_space = evidence
        .dense_embedding_space_id
        .as_deref()
        .ok_or("semantic release gate requires an LVEC v2/v3 dense pack")?;
    if dense_space != profile.embedding_space_id {
        return Err("dense-pack embedding-space ID does not match semantic release profile".into());
    }
    let dense_dimensions = evidence
        .dense_dimensions
        .ok_or("semantic release gate requires dense-pack dimensions")?;
    if evidence.query_embedding_count == 0 {
        return Err("semantic release gate requires prompt-bound query embeddings".into());
    }
    if evidence.query_embedding_count != prompts.len()
        || evidence.query_embedding_dimensions.len() != prompts.len()
    {
        return Err(format!(
            "semantic release gate requires exactly one query embedding per selected prompt ({} records for {} prompts)",
            evidence.query_embedding_count,
            prompts.len()
        ));
    }
    if evidence
        .query_embedding_dimensions
        .iter()
        .any(|dimensions| *dimensions != dense_dimensions)
    {
        return Err(format!(
            "query embedding dimensions do not match dense pack ({dense_dimensions})"
        ));
    }
    let prompt_ids: HashSet<String> = prompts.iter().map(|prompt| prompt.id.clone()).collect();
    if evidence.query_embedding_ids != prompt_ids {
        return Err("query embedding IDs do not exactly match selected prompt IDs".into());
    }
    validate_release_coverage(profile, prompts)
}

#[derive(Debug, Serialize)]
struct ArtifactIdentity {
    path: String,
    bytes: usize,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct RunMetadata {
    evaluator_version: &'static str,
    evaluator_binary: ArtifactIdentity,
    base_pack: ArtifactIdentity,
    overlays: Vec<ArtifactIdentity>,
    prompts: ArtifactIdentity,
    family_registries: Vec<ArtifactIdentity>,
    live_results: Option<ArtifactIdentity>,
    query_embeddings: Option<ArtifactIdentity>,
    release_profile: Option<ArtifactIdentity>,
    embedding_space_id: Option<String>,
    retrieval_depth: usize,
    split: Option<String>,
    baseline: bool,
    dense_diagnostic: bool,
    context_diagnostic: bool,
    release_gate: bool,
    compatibility_gate: bool,
}

#[derive(Serialize)]
struct JsonReport<'a> {
    metadata: &'a RunMetadata,
    systems: &'a [SystemScores],
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let mut pack_buffers = Vec::with_capacity(cli.overlay.len() + 1);
    pack_buffers.push(fs::read(&cli.pack)?);
    for overlay in &cli.overlay {
        pack_buffers.push(fs::read(overlay)?);
    }
    let pack_slices: Vec<&[u8]> = pack_buffers.iter().map(Vec::as_slice).collect();
    let eng = Engine::open(&pack_slices)?;
    let retrieval_depth = cli.top_k.clamp(10, MAX_TOP_K);
    let raw = fs::read_to_string(&cli.prompts)?;
    let registry_buffers: Vec<Vec<u8>> = cli
        .family_registry
        .iter()
        .map(fs::read)
        .collect::<Result<_, _>>()?;
    let live_results_buffer = cli.live_results.as_ref().map(fs::read).transpose()?;
    let query_embeddings_buffer = cli.query_embeddings.as_ref().map(fs::read).transpose()?;
    let release_profile_buffer = cli.release_profile.as_ref().map(fs::read).transpose()?;
    let release_profile = release_profile_buffer
        .as_deref()
        .map(parse_semantic_release_profile)
        .transpose()?;
    let executable_path = std::env::current_exe()?;
    let executable_bytes = fs::read(&executable_path)?;
    let embedding_space_id = cli
        .embedding_space_id
        .as_deref()
        .map(parse_embedding_space_id)
        .transpose()?;
    let metadata = RunMetadata {
        evaluator_version: env!("CARGO_PKG_VERSION"),
        evaluator_binary: artifact_identity(&executable_path, &executable_bytes),
        base_pack: artifact_identity(&cli.pack, &pack_buffers[0]),
        overlays: cli
            .overlay
            .iter()
            .zip(pack_buffers.iter().skip(1))
            .map(|(path, bytes)| artifact_identity(path, bytes))
            .collect(),
        prompts: artifact_identity(&cli.prompts, raw.as_bytes()),
        family_registries: cli
            .family_registry
            .iter()
            .zip(&registry_buffers)
            .map(|(path, bytes)| artifact_identity(path, bytes))
            .collect(),
        live_results: cli
            .live_results
            .as_ref()
            .zip(live_results_buffer.as_ref())
            .map(|(path, bytes)| artifact_identity(path, bytes)),
        query_embeddings: cli
            .query_embeddings
            .as_ref()
            .zip(query_embeddings_buffer.as_ref())
            .map(|(path, bytes)| artifact_identity(path, bytes)),
        release_profile: cli
            .release_profile
            .as_ref()
            .zip(release_profile_buffer.as_ref())
            .map(|(path, bytes)| artifact_identity(path, bytes)),
        embedding_space_id: embedding_space_id.map(hex_embedding_space_id),
        retrieval_depth,
        split: cli.split.clone(),
        baseline: cli.baseline,
        dense_diagnostic: cli.dense_diagnostic,
        context_diagnostic: cli.context_diagnostic,
        release_gate: cli.release_gate,
        compatibility_gate: cli.compatibility_gate,
    };
    let mut prompts = parse_prompts(&raw)?;
    let holdout_run = cli
        .split
        .as_deref()
        .is_some_and(|split| split_role(split) == "holdout")
        || (cli.split.is_none()
            && prompts.iter().any(|prompt| {
                prompt
                    .split
                    .as_deref()
                    .is_some_and(|split| split_role(split) == "holdout")
            }));
    if (cli.release_gate || holdout_run) && cli.family_registry.is_empty() {
        return Err(
            "semantic release-gate and holdout runs require at least one --family-registry corpus"
                .into(),
        );
    }
    if !cli.family_registry.is_empty() {
        let mut registry = prompts.clone();
        for registered_bytes in &registry_buffers {
            let registered_raw = std::str::from_utf8(registered_bytes)?;
            registry.extend(parse_prompts(registered_raw)?);
        }
        validate_prompt_registry(&registry)?;
    }
    if let Some(split) = &cli.split {
        prompts.retain(|prompt| prompt.split.as_deref() == Some(split.as_str()));
        if prompts.is_empty() {
            return Err(format!("no prompts found for split `{split}`").into());
        }
    }
    let query_embeddings = parse_query_embeddings(query_embeddings_buffer.as_deref())?;
    if !query_embeddings.is_empty() {
        for prompt in &prompts {
            if !query_embeddings.contains_key(&prompt.id) {
                return Err(format!("query embedding missing prompt id {}", prompt.id).into());
            }
        }
        validate_query_embedding_plans(&prompts, &query_embeddings)?;
    }
    if cli.release_gate {
        let profile = release_profile
            .as_ref()
            .ok_or("--release-gate requires --release-profile")?;
        let dense_metadata = eng.dense_metadata();
        let evidence = SemanticRuntimeEvidence {
            base_pack_sha256: metadata.base_pack.sha256.clone(),
            overlay_sha256: metadata
                .overlays
                .iter()
                .map(|artifact| artifact.sha256.clone())
                .collect(),
            cli_embedding_space_id: metadata.embedding_space_id.clone(),
            dense_embedding_space_id: dense_metadata
                .and_then(|dense| dense.embedding_space_id)
                .map(hex_embedding_space_id),
            dense_dimensions: dense_metadata.map(|dense| dense.dims),
            query_embedding_count: query_embeddings.len(),
            query_embedding_dimensions: query_embeddings
                .values()
                .map(|query| query.embedding.len())
                .collect(),
            query_embedding_ids: query_embeddings.keys().cloned().collect(),
            family_registry_count: cli.family_registry.len(),
            holdout_only: !prompts.is_empty()
                && prompts.iter().all(|prompt| {
                    prompt
                        .split
                        .as_deref()
                        .is_some_and(|split| split_role(split) == "holdout")
                }),
        };
        validate_semantic_release_inputs(profile, &prompts, &evidence)
            .map_err(|error| format!("semantic release preflight failed: {error}"))?;
    }

    println!(
        "pack: {} verses translation={}",
        eng.verse_count(),
        eng.translation_id()
    );
    println!("prompts: {}", prompts.len());
    println!("retrieval depth: {retrieval_depth}");
    println!("overlays: {}", cli.overlay.len());
    if let Some(split) = &cli.split {
        println!("split: {split}");
    }
    println!();

    let mut systems: Vec<SystemScores> = Vec::new();

    // --- verse-recall ---
    let vr = eval_system("verse-recall", &prompts, |p| {
        let opts = SearchOpts::default().with_top_k(retrieval_depth);
        let resp = if let Some(query_embedding) = query_embeddings.get(&p.id) {
            match (
                embedding_space_id.as_ref(),
                query_embedding.segments.is_empty(),
            ) {
                (Some(space_id), false) => {
                    let segments: Vec<QuerySegmentEmbedding<'_>> = query_embedding
                        .segments
                        .iter()
                        .map(|segment| QuerySegmentEmbedding {
                            text: &segment.text,
                            embedding: &segment.embedding,
                        })
                        .collect();
                    eng.search_with_segment_embeddings_in_space(
                        &p.prompt,
                        space_id,
                        &query_embedding.embedding,
                        &segments,
                        &opts,
                    )?
                }
                (Some(space_id), true) => eng.search_with_embedding_in_space(
                    &p.prompt,
                    space_id,
                    &query_embedding.embedding,
                    &opts,
                )?,
                (None, true) => {
                    eng.search_with_embedding(&p.prompt, &query_embedding.embedding, &opts)?
                }
                (None, false) => {
                    return Err("segment embeddings require an LVEC v2 embedding-space ID".into());
                }
            }
        } else {
            eng.search(&p.prompt, &opts)
        };
        let top_refs: Vec<String> = resp.results.iter().map(|r| r.reference.clone()).collect();
        // A concept result is an anchor plus a validated structured passage.
        // Retrieval scoring must credit that passage, while reports retain the
        // concise display anchors users actually see.
        let scoring_refs: Vec<String> = resp.results.iter().map(result_scoring_ref).collect();
        let safe = resp.results.iter().take(3).all(|result| {
            !matches!(result.confidence, Confidence::Recovered)
                || p.gold_refs
                    .iter()
                    .any(|gold| result_matches_ref(result, gold))
        });
        let mut score = score_prompt_at_depth(p, &scoring_refs, Some(safe), retrieval_depth);
        score.top_refs = top_refs;
        score.lane_used = relevant_lane_attribution(p, &resp.results);
        if let Some(expected) = p.expected_confidence {
            let relevant = relevant_results(p, &resp.results);
            score.confidence_label_ok = Some(
                score.first_relevant_rank.is_some()
                    && !relevant.is_empty()
                    && relevant
                        .iter()
                        .all(|result| confidence_matches(result.confidence, expected)),
            );
        }
        if !p.expected_passages.is_empty() {
            score.passage_output_ok = Some(p.expected_passages.iter().all(|expected| {
                resp.results.iter().take(3).any(|result| {
                    result.passages.iter().any(|passage| {
                        passage_range_covers(&passage.start_ref, &passage.end_ref, expected)
                    })
                })
            }));
        }
        Ok(score)
    })?;
    print_system(&vr);
    systems.push(vr);

    // --- source-authored sparse context recall (diagnostic only) ---
    if cli.context_diagnostic {
        if !eng.context_available() {
            return Err("--context-diagnostic requires CLEX/CPOST sections".into());
        }
        let context_only = eval_system("context-only-diagnostic", &prompts, |p| {
            let opts = SearchOpts::default().with_top_k(retrieval_depth);
            let resp = eng.search_context_only(&p.prompt, &opts);
            let top_refs: Vec<String> = resp
                .results
                .iter()
                .map(|result| result.reference.clone())
                .collect();
            let safe = resp
                .results
                .iter()
                .take(3)
                .all(|result| result.confidence != Confidence::Recovered);
            let mut score = score_prompt_at_depth(p, &top_refs, Some(safe), retrieval_depth);
            score.lane_used = Some("C-context".into());
            if let Some(expected) = p.expected_confidence {
                let relevant = relevant_results(p, &resp.results);
                score.confidence_label_ok = Some(
                    score.first_relevant_rank.is_some()
                        && !relevant.is_empty()
                        && relevant
                            .iter()
                            .all(|result| confidence_matches(result.confidence, expected)),
                );
            }
            if !p.expected_passages.is_empty() {
                score.passage_output_ok = Some(false);
            }
            Ok(score)
        })?;
        print_system(&context_only);
        systems.push(context_only);
    }

    // --- raw dense candidate recall (diagnostic only) ---
    // Fusion cannot recover a passage that the encoder never retrieves. Keep
    // this explicit, opt-in system separate from the user-facing engine so
    // representation bake-offs can compare R@50/R@100 without lexical help.
    if cli.dense_diagnostic {
        if query_embeddings.is_empty() {
            return Err("--dense-diagnostic requires --query-embeddings".into());
        }
        let dense = dense_lane_from_packs(&pack_buffers, eng.verse_count())?
            .ok_or("--dense-diagnostic requires an LVEC pack")?;
        let base = PackReader::open(&pack_buffers[0])?;
        let verse_refs: Vec<String> = base
            .text()?
            .verses
            .into_iter()
            .map(|verse| verse.reference)
            .collect();
        let dense_only = eval_system("dense-only-diagnostic", &prompts, |p| {
            let embedding = query_embeddings
                .get(&p.id)
                .ok_or_else(|| format!("query embedding missing prompt id {}", p.id))?;
            let hits = match embedding_space_id.as_ref() {
                Some(space_id) => {
                    dense.search_in_space(space_id, &embedding.embedding, retrieval_depth)?
                }
                None => dense.search(&embedding.embedding, retrieval_depth)?,
            };
            let top: Vec<String> = hits
                .into_iter()
                .map(|hit| verse_refs[hit.verse_index as usize].clone())
                .collect();
            Ok(score_prompt_at_depth(p, &top, Some(true), retrieval_depth))
        })?;
        print_system(&dense_only);
        systems.push(dense_only);
    }

    // --- offline keyword baseline ---
    if cli.baseline {
        let kw = KeywordBaseline::from_pack(&pack_buffers[0])?;
        let base = eval_system("offline-keyword", &prompts, |p| {
            let top = kw.search(&p.prompt, retrieval_depth);
            Ok(score_prompt_at_depth(p, &top, None, retrieval_depth))
        })?;
        print_system(&base);
        systems.push(base);
    }

    // --- live tools (optional file) ---
    if let Some(path) = &cli.live_results {
        let live_raw = std::str::from_utf8(
            live_results_buffer
                .as_deref()
                .ok_or_else(|| format!("missing live capture bytes for {}", path.display()))?,
        )?;
        let prompt_ids: HashSet<&str> = prompts.iter().map(|prompt| prompt.id.as_str()).collect();
        let mut by_tool: BTreeMap<String, HashMap<String, Vec<String>>> = BTreeMap::new();
        for line in live_raw.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let lr: LiveResult = serde_json::from_str(line)?;
            if !prompt_ids.contains(lr.id.as_str()) {
                return Err(format!(
                    "live capture for {} contains unknown prompt id `{}`",
                    lr.tool, lr.id
                )
                .into());
            }
            let tool = by_tool.entry(lr.tool.clone()).or_default();
            if tool.insert(lr.id.clone(), lr.top_refs).is_some() {
                return Err(format!(
                    "duplicate live capture for tool {} prompt {}",
                    lr.tool, lr.id
                )
                .into());
            }
        }
        for (tool, map) in by_tool {
            let missing: Vec<&str> = prompts
                .iter()
                .filter(|prompt| !map.contains_key(&prompt.id))
                .map(|prompt| prompt.id.as_str())
                .collect();
            println!(
                "live capture {tool}: {}/{} prompts",
                map.len(),
                prompts.len()
            );
            if !missing.is_empty() {
                return Err(format!(
                    "live capture {tool} is incomplete: missing {} prompt(s), including {}",
                    missing.len(),
                    missing
                        .iter()
                        .take(5)
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
                .into());
            }
            let unequal_depth: Vec<String> = prompts
                .iter()
                .filter_map(|prompt| {
                    let depth = map.get(&prompt.id)?.len();
                    (depth != retrieval_depth).then(|| format!("{}={depth}", prompt.id))
                })
                .collect();
            if !unequal_depth.is_empty() {
                return Err(format!(
                    "live capture {tool} must provide exactly {retrieval_depth} results per prompt; mismatches include {}",
                    unequal_depth.into_iter().take(5).collect::<Vec<_>>().join(", ")
                )
                .into());
            }
            let scored = eval_system(&tool, &prompts, |p| {
                let mut top = map.get(&p.id).cloned().unwrap_or_default();
                top.truncate(retrieval_depth);
                Ok(score_prompt_at_depth(p, &top, None, retrieval_depth))
            })?;
            print_system(&scored);
            systems.push(scored);
        }
    }

    if let Some(path) = &cli.report {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let md = render_report(&systems, &prompts, &metadata);
        fs::write(path, md)?;
        eprintln!("wrote report → {}", path.display());

        let json_path = path.with_extension("json");
        fs::write(
            &json_path,
            serde_json::to_string_pretty(&JsonReport {
                metadata: &metadata,
                systems: &systems,
            })?,
        )?;
        eprintln!("wrote json → {}", json_path.display());
    }

    if cli.compatibility_gate {
        validate_compatibility_gate_metrics(&systems[0].overall)
            .map_err(|error| format!("compatibility gate failed: {error}"))?;
        println!("PASS: revealed-suite compatibility floors (not a semantic release claim)");
    }
    if cli.release_gate {
        let profile = release_profile
            .as_ref()
            .ok_or("--release-gate requires --release-profile")?;
        validate_semantic_release_metrics(&systems[0].overall, profile)
            .map_err(|error| format!("semantic release gate failed: {error}"))?;
        println!("PASS: sealed semantic release profile, artifact binding, recall, and all safety/correction gates");
    }

    Ok(())
}

fn gate_metric_summary(metric: &MetricScore) -> String {
    format!(
        "R@1={:.1}% (>=60), R@3={:.1}% (>=85), R@10={:.1}% (>=95), confidence safety={:.1}% ({}/{}), no-gold safety={:.1}% ({}/{}), passage output={:.1}% ({}/{}), hard negatives={:.1}% ({}/{}), required labels={:.1}% ({}/{})",
        metric.recall_at_1 * 100.0,
        metric.recall_at_3 * 100.0,
        metric.recall_at_10 * 100.0,
        metric.confidence_safety * 100.0,
        metric.safe_confidence_queries,
        metric.safety_queries,
        metric.no_gold_safety * 100.0,
        metric.safe_no_gold,
        metric.no_gold_queries,
        metric.passage_accuracy * 100.0,
        metric.passage_passes,
        metric.passage_queries,
        metric.hard_negative_accuracy * 100.0,
        metric.hard_negative_passes,
        metric.hard_negative_queries,
        metric.confidence_label_accuracy * 100.0,
        metric.confidence_label_passes,
        metric.confidence_label_queries
    )
}

fn recall_metrics_pass(metric: &MetricScore) -> bool {
    metric.recall_at_1 >= RELEASE_RECALL_AT_1
        && metric.recall_at_3 >= RELEASE_RECALL_AT_3
        && metric.recall_at_10 >= RELEASE_RECALL_AT_10
}

fn validate_compatibility_gate_metrics(metric: &MetricScore) -> Result<(), String> {
    let safety_ok = metric.safety_queries == metric.queries && metric.confidence_safety == 1.0;
    let passages_ok = metric.passage_queries == 0 || metric.passage_accuracy == 1.0;
    let hard_negatives_ok =
        metric.hard_negative_queries == 0 || metric.hard_negative_accuracy == 1.0;
    let confidence_labels_ok =
        metric.confidence_label_queries == 0 || metric.confidence_label_accuracy == 1.0;
    if recall_metrics_pass(metric)
        && safety_ok
        && passages_ok
        && hard_negatives_ok
        && confidence_labels_ok
    {
        Ok(())
    } else {
        Err(gate_metric_summary(metric))
    }
}

fn validate_semantic_release_metrics(
    metric: &MetricScore,
    profile: &SemanticReleaseProfile,
) -> Result<(), String> {
    let minimums = &profile.minimums;
    let denominators_ok = metric.queries >= minimums.total_prompts
        && metric.retrieval_queries >= minimums.retrieval_prompts
        && metric.no_gold_queries >= minimums.no_gold_prompts
        && metric.passage_queries >= minimums.structured_passage_prompts
        && metric.hard_negative_queries >= minimums.hard_negative_prompts
        && metric.confidence_label_queries >= minimums.confidence_label_prompts;
    let safety_ok = metric.safety_queries == metric.queries
        && metric.safety_queries > 0
        && metric.confidence_safety == 1.0
        && metric.no_gold_queries > 0
        && metric.no_gold_safety == 1.0;
    let complete_slices_ok = metric.passage_queries > 0
        && metric.passage_accuracy == 1.0
        && metric.hard_negative_queries > 0
        && metric.hard_negative_accuracy == 1.0
        && metric.confidence_label_queries > 0
        && metric.confidence_label_accuracy == 1.0;
    if recall_metrics_pass(metric) && denominators_ok && safety_ok && complete_slices_ok {
        Ok(())
    } else {
        Err(gate_metric_summary(metric))
    }
}

fn dense_lane_from_packs(
    packs: &[Vec<u8>],
    expected_verse_count: usize,
) -> Result<Option<DenseLane>, Box<dyn std::error::Error>> {
    let mut dense = None;
    for bytes in packs {
        let reader = PackReader::open(bytes)?;
        if !reader.has(SectionTag::Lvec) {
            continue;
        }
        if dense.is_some() {
            return Err("multiple LVEC sections were supplied".into());
        }
        dense = Some(
            DenseLane::parse(
                reader.section_bytes(SectionTag::Lvec)?,
                expected_verse_count,
            )
            .map_err(|error| format!("invalid LVEC diagnostic pack: {error}"))?,
        );
    }
    Ok(dense)
}

fn parse_embedding_space_id(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let value = value.trim();
    if value.len() != 64 {
        return Err("embedding-space-id must contain exactly 64 hexadecimal characters".into());
    }
    let mut result = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let pair = std::str::from_utf8(pair)?;
        result[index] = u8::from_str_radix(pair, 16)
            .map_err(|_| "embedding-space-id contains non-hex characters")?;
    }
    if result.iter().all(|byte| *byte == 0) {
        return Err("embedding-space-id must not be all zero".into());
    }
    Ok(result)
}

fn hex_embedding_space_id(value: [u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn split_role(split: &str) -> String {
    split
        .split_once('-')
        .map(|(role, _)| role)
        .unwrap_or(split)
        .trim()
        .to_ascii_lowercase()
}

fn relevant_lane_attribution(prompt: &Prompt, results: &[SearchResult]) -> Option<String> {
    let relevant = relevant_results(prompt, results);
    let mut attributions = Vec::new();
    for result in relevant {
        let attribution = result_lane_attribution(result);
        if !attributions.contains(&attribution) {
            attributions.push(attribution);
        }
    }
    (!attributions.is_empty()).then(|| attributions.join("; "))
}

fn relevant_results<'a>(prompt: &Prompt, results: &'a [SearchResult]) -> Vec<&'a SearchResult> {
    if prompt.gold_refs.is_empty() {
        return Vec::new();
    }
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

fn artifact_identity(path: &std::path::Path, bytes: &[u8]) -> ArtifactIdentity {
    let display_path = fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string();
    ArtifactIdentity {
        path: display_path,
        bytes: bytes.len(),
        sha256: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}

fn result_lane_attribution(result: &SearchResult) -> String {
    let mut lanes = Vec::new();
    if result.lanes.a > 0.0 {
        lanes.push("lexical");
    }
    if result.lanes.b > 0.0 {
        if result
            .expanded_terms
            .iter()
            .any(|term| term.starts_with("translation:"))
        {
            lanes.push("alternate");
        } else {
            lanes.push("expand");
        }
    }
    if result.lanes.c > 0.0 {
        lanes.push("dense");
    }
    if result.lanes.d > 0.0 {
        if result.expanded_terms.iter().any(|term| {
            [
                "curated:",
                "event:",
                "event_shadow:",
                "topic:",
                "entity:",
                "person:",
                "place:",
            ]
            .iter()
            .any(|prefix| term.starts_with(prefix))
        }) {
            lanes.push("concept");
        } else {
            lanes.push("phrase");
        }
    }
    match lanes.as_slice() {
        [] => "none".into(),
        [lane] => (*lane).into(),
        _ => format!("fuse({})", lanes.join("+")),
    }
}

fn eval_system<F>(
    name: &str,
    prompts: &[Prompt],
    mut f: F,
) -> Result<SystemScores, Box<dyn std::error::Error>>
where
    F: FnMut(&Prompt) -> Result<PromptScore, Box<dyn std::error::Error>>,
{
    let mut per_prompt = Vec::new();

    for p in prompts {
        let ps = f(p)?;
        let status = if ps.passed_primary_check() {
            "PASS"
        } else {
            "MISS"
        };
        println!(
            "[{name}] [{status}] {} ({}) → rank={:?} top={:?} gold={:?}",
            p.id, p.category, ps.first_relevant_rank, ps.top_refs, p.gold_refs
        );
        if let Some(n) = &p.notes {
            println!("       notes: {n}");
        }
        per_prompt.push(ps);
    }
    Ok(summarize_system(name, per_prompt))
}

fn print_system(s: &SystemScores) {
    println!();
    println!("=== {} retrieval metrics ===", s.name);
    for (category, score) in &s.by_category {
        println!(
            "  {category:20} R@1 {:5.1}%  R@3 {:5.1}%  R@10 {:5.1}%{}  MRR {:.3}  (n={})",
            score.recall_at_1 * 100.0,
            score.recall_at_3 * 100.0,
            score.recall_at_10 * 100.0,
            deep_metric_suffix(score),
            score.mrr,
            score.retrieval_queries,
        );
        if score.no_gold_queries > 0 {
            println!(
                "  {category:20} no-gold safety {:5.1}% ({}/{})",
                score.no_gold_safety * 100.0,
                score.safe_no_gold,
                score.no_gold_queries
            );
        }
        if score.safety_queries > 0 {
            println!(
                "  {category:20} confidence safety {:5.1}% ({}/{})",
                score.confidence_safety * 100.0,
                score.safe_confidence_queries,
                score.safety_queries
            );
        }
        if score.hard_negative_queries > 0 {
            println!(
                "  {category:20} hard negatives {:5.1}% ({}/{})",
                score.hard_negative_accuracy * 100.0,
                score.hard_negative_passes,
                score.hard_negative_queries
            );
        }
        if score.confidence_label_queries > 0 {
            println!(
                "  {category:20} required labels {:5.1}% ({}/{})",
                score.confidence_label_accuracy * 100.0,
                score.confidence_label_passes,
                score.confidence_label_queries
            );
        }
    }
    println!(
        "  {:20} R@1 {:5.1}%  R@3 {:5.1}%  R@10 {:5.1}%{}  MRR {:.3}  (n={})",
        "ALL",
        s.overall.recall_at_1 * 100.0,
        s.overall.recall_at_3 * 100.0,
        s.overall.recall_at_10 * 100.0,
        deep_metric_suffix(&s.overall),
        s.overall.mrr,
        s.overall.retrieval_queries
    );
    if s.overall.no_gold_queries > 0 {
        println!(
            "  {:20} no-gold safety {:5.1}% ({}/{})",
            "ALL",
            s.overall.no_gold_safety * 100.0,
            s.overall.safe_no_gold,
            s.overall.no_gold_queries
        );
    }
    if s.overall.safety_queries > 0 {
        println!(
            "  {:20} confidence safety {:5.1}% ({}/{})",
            "ALL",
            s.overall.confidence_safety * 100.0,
            s.overall.safe_confidence_queries,
            s.overall.safety_queries
        );
    }
    if s.overall.passage_queries > 0 {
        println!(
            "  {:20} passage output {:5.1}% ({}/{})",
            "ALL",
            s.overall.passage_accuracy * 100.0,
            s.overall.passage_passes,
            s.overall.passage_queries
        );
    }
    if s.overall.hard_negative_queries > 0 {
        println!(
            "  {:20} hard negatives {:5.1}% ({}/{})",
            "ALL",
            s.overall.hard_negative_accuracy * 100.0,
            s.overall.hard_negative_passes,
            s.overall.hard_negative_queries
        );
    }
    if s.overall.confidence_label_queries > 0 {
        println!(
            "  {:20} required labels {:5.1}% ({}/{})",
            "ALL",
            s.overall.confidence_label_accuracy * 100.0,
            s.overall.confidence_label_passes,
            s.overall.confidence_label_queries
        );
    }
    println!();
}

fn deep_metric_suffix(score: &MetricScore) -> String {
    let mut suffix = String::new();
    if let Some(recall) = score.recall_at_50 {
        suffix.push_str(&format!("  R@50 {:5.1}%", recall * 100.0));
    }
    if let Some(recall) = score.recall_at_100 {
        suffix.push_str(&format!("  R@100 {:5.1}%", recall * 100.0));
    }
    suffix
}

fn markdown_recall(recall: Option<f64>) -> String {
    recall
        .map(|value| format!("{:.1}%", value * 100.0))
        .unwrap_or_else(|| "—".into())
}

fn markdown_hit(hit: Option<bool>) -> &'static str {
    match hit {
        Some(true) => "✓",
        Some(false) => "—",
        None => "n/a",
    }
}

fn render_report(systems: &[SystemScores], prompts: &[Prompt], metadata: &RunMetadata) -> String {
    let mut md = String::new();
    let report_at_50 = metadata.retrieval_depth >= 50;
    let report_at_100 = metadata.retrieval_depth >= 100;
    md.push_str("# Bible search red-team report\n\n");
    md.push_str("## Run identity\n\n");
    md.push_str(&format!(
        "- Evaluator: v{} (`{}`, {} bytes, SHA-256 `{}`)\n",
        metadata.evaluator_version,
        metadata.evaluator_binary.path,
        metadata.evaluator_binary.bytes,
        metadata.evaluator_binary.sha256
    ));
    md.push_str(&format!(
        "- Base pack: `{}` ({} bytes, SHA-256 `{}`)\n",
        metadata.base_pack.path, metadata.base_pack.bytes, metadata.base_pack.sha256
    ));
    for overlay in &metadata.overlays {
        md.push_str(&format!(
            "- Overlay: `{}` ({} bytes, SHA-256 `{}`)\n",
            overlay.path, overlay.bytes, overlay.sha256
        ));
    }
    md.push_str(&format!(
        "- Prompts: `{}` ({} bytes, SHA-256 `{}`)\n",
        metadata.prompts.path, metadata.prompts.bytes, metadata.prompts.sha256
    ));
    for registry in &metadata.family_registries {
        md.push_str(&format!(
            "- Family registry: `{}` ({} bytes, SHA-256 `{}`)\n",
            registry.path, registry.bytes, registry.sha256
        ));
    }
    if let Some(live) = &metadata.live_results {
        md.push_str(&format!(
            "- Live results: `{}` ({} bytes, SHA-256 `{}`)\n",
            live.path, live.bytes, live.sha256
        ));
    }
    if let Some(embeddings) = &metadata.query_embeddings {
        md.push_str(&format!(
            "- Query embeddings: `{}` ({} bytes, SHA-256 `{}`)\n",
            embeddings.path, embeddings.bytes, embeddings.sha256
        ));
    }
    if let Some(profile) = &metadata.release_profile {
        md.push_str(&format!(
            "- Semantic release profile: `{}` ({} bytes, SHA-256 `{}`)\n",
            profile.path, profile.bytes, profile.sha256
        ));
    }
    if let Some(space_id) = &metadata.embedding_space_id {
        md.push_str(&format!("- Embedding space ID: `{space_id}`\n"));
    }
    md.push_str(&format!(
        "- Parameters: depth={}, split={:?}, baseline={}, dense_diagnostic={}, context_diagnostic={}, release_gate={}, compatibility_gate={}, registry_count={}\n\n",
        metadata.retrieval_depth,
        metadata.split,
        metadata.baseline,
        metadata.dense_diagnostic,
        metadata.context_diagnostic,
        metadata.release_gate,
        metadata.compatibility_gate,
        metadata.family_registries.len()
    ));
    md.push_str(&format!(
        "Suite size: **{}** prompts. Empty-gold safety cases are excluded from retrieval denominators.\n\n",
        prompts.len()
    ));
    let deep_cutoffs = match (report_at_50, report_at_100) {
        (true, true) => "/50/100",
        (true, false) => "/50",
        _ => "",
    };
    md.push_str(&format!("Metrics: **Recall@1/3/10{deep_cutoffs}** and **MRR** over gold-bearing prompts; `gold_mode=all` requires every split/conflated gold; **confidence safety** requires every top-3 `Recovered` result to overlap gold (and forbids it for empty-gold prompts); named concepts may require all declared structured passages; hard negatives must rank below the completed gold; gold-bearing misquotes may require an explicit confidence label.\n\n"));

    md.push_str("## Overall\n\n");
    let mut overall_header = "| System | Retrieval n | R@1 | R@3 | R@10".to_string();
    let mut overall_rule = "|--------|------------:|----:|----:|-----:".to_string();
    if report_at_50 {
        overall_header.push_str(" | R@50");
        overall_rule.push_str("|-----:");
    }
    if report_at_100 {
        overall_header.push_str(" | R@100");
        overall_rule.push_str("|------:");
    }
    overall_header.push_str(
        " | MRR | Confidence safety | Passage output | Hard negatives | Required labels |\n",
    );
    overall_rule.push_str(
        "|----:|------------------:|---------------:|---------------:|----------------:|\n",
    );
    md.push_str(&overall_header);
    md.push_str(&overall_rule);
    for s in systems {
        let mut row = format!(
            "| {} | {} | {:.1}% | {:.1}% | {:.1}%",
            s.name,
            s.overall.retrieval_queries,
            s.overall.recall_at_1 * 100.0,
            s.overall.recall_at_3 * 100.0,
            s.overall.recall_at_10 * 100.0,
        );
        if report_at_50 {
            row.push_str(&format!(" | {}", markdown_recall(s.overall.recall_at_50)));
        }
        if report_at_100 {
            row.push_str(&format!(" | {}", markdown_recall(s.overall.recall_at_100)));
        }
        row.push_str(&format!(
            " | {:.3} | {:.1}% ({}/{}) | {:.1}% ({}/{}) | {:.1}% ({}/{}) | {:.1}% ({}/{}) |\n",
            s.overall.mrr,
            s.overall.confidence_safety * 100.0,
            s.overall.safe_confidence_queries,
            s.overall.safety_queries,
            s.overall.passage_accuracy * 100.0,
            s.overall.passage_passes,
            s.overall.passage_queries,
            s.overall.hard_negative_accuracy * 100.0,
            s.overall.hard_negative_passes,
            s.overall.hard_negative_queries,
            s.overall.confidence_label_accuracy * 100.0,
            s.overall.confidence_label_passes,
            s.overall.confidence_label_queries
        ));
        md.push_str(&row);
    }
    md.push('\n');

    // Category table
    let mut cats: HashSet<String> = HashSet::new();
    for s in systems {
        cats.extend(s.by_category.keys().cloned());
    }
    let mut cat_list: Vec<_> = cats.into_iter().collect();
    cat_list.sort();

    md.push_str("## By category\n\n");
    let mut category_header = "| Category | System | n | R@1 | R@3 | R@10".to_string();
    let mut category_rule = "|----------|--------|--:|----:|----:|-----:".to_string();
    if report_at_50 {
        category_header.push_str(" | R@50");
        category_rule.push_str("|-----:");
    }
    if report_at_100 {
        category_header.push_str(" | R@100");
        category_rule.push_str("|------:");
    }
    category_header.push_str(" | MRR |\n");
    category_rule.push_str("|----:|\n");
    md.push_str(&category_header);
    md.push_str(&category_rule);
    for category in &cat_list {
        for system in systems {
            if let Some(score) = system.by_category.get(category) {
                let mut row = format!(
                    "| {category} | {} | {} | {:.1}% | {:.1}% | {:.1}%",
                    system.name,
                    score.retrieval_queries,
                    score.recall_at_1 * 100.0,
                    score.recall_at_3 * 100.0,
                    score.recall_at_10 * 100.0,
                );
                if report_at_50 {
                    row.push_str(&format!(" | {}", markdown_recall(score.recall_at_50)));
                }
                if report_at_100 {
                    row.push_str(&format!(" | {}", markdown_recall(score.recall_at_100)));
                }
                row.push_str(&format!(" | {:.3} |\n", score.mrr));
                md.push_str(&row);
            }
        }
    }
    md.push('\n');

    md.push_str("## By family (verse-recall)\n\n");
    if let Some(vr) = systems.first() {
        let mut family_header = "| Family | n | R@1 | R@3 | R@10".to_string();
        let mut family_rule = "|--------|--:|----:|----:|-----:".to_string();
        if report_at_50 {
            family_header.push_str(" | R@50");
            family_rule.push_str("|-----:");
        }
        if report_at_100 {
            family_header.push_str(" | R@100");
            family_rule.push_str("|------:");
        }
        family_header.push_str(" | MRR |\n");
        family_rule.push_str("|----:|\n");
        md.push_str(&family_header);
        md.push_str(&family_rule);
        for (family, score) in &vr.by_family {
            let mut row = format!(
                "| {family} | {} | {:.1}% | {:.1}% | {:.1}%",
                score.retrieval_queries,
                score.recall_at_1 * 100.0,
                score.recall_at_3 * 100.0,
                score.recall_at_10 * 100.0,
            );
            if report_at_50 {
                row.push_str(&format!(" | {}", markdown_recall(score.recall_at_50)));
            }
            if report_at_100 {
                row.push_str(&format!(" | {}", markdown_recall(score.recall_at_100)));
            }
            row.push_str(&format!(" | {:.3} |\n", score.mrr));
            md.push_str(&row);
        }
    }
    md.push('\n');

    md.push_str("## Per-prompt (verse-recall)\n\n");
    if let Some(vr) = systems.first() {
        let mut prompt_header =
            "| id | category | family | gold mode | rank | R@1 | R@3 | R@10".to_string();
        let mut prompt_rule =
            "|----|----------|--------|-----------|-----:|-----|-----|------".to_string();
        if report_at_50 {
            prompt_header.push_str(" | R@50");
            prompt_rule.push_str("|-----");
        }
        if report_at_100 {
            prompt_header.push_str(" | R@100");
            prompt_rule.push_str("|------");
        }
        prompt_header.push_str(" | safe | passage | hard neg | label | lane used | top | gold |\n");
        prompt_rule.push_str("|------|---------|----------|-------|-----------|-----|------|\n");
        md.push_str(&prompt_header);
        md.push_str(&prompt_rule);
        for p in &vr.per_prompt {
            let mut row = format!(
                "| {} | {} | {} | {:?} | {} | {} | {} | {}",
                p.id,
                p.category,
                p.family.as_deref().unwrap_or("—"),
                p.gold_mode,
                p.first_relevant_rank
                    .map(|rank| rank.to_string())
                    .unwrap_or_else(|| "—".into()),
                if p.hit_at_1 { "✓" } else { "—" },
                if p.hit_at_3 { "✓" } else { "—" },
                if p.hit_at_10 { "✓" } else { "—" },
            );
            if report_at_50 {
                row.push_str(&format!(" | {}", markdown_hit(p.hit_at_50)));
            }
            if report_at_100 {
                row.push_str(&format!(" | {}", markdown_hit(p.hit_at_100)));
            }
            row.push_str(&format!(
                " | {} | {} | {} | {} | {} | {} | {} |\n",
                match p.confidence_safe {
                    Some(true) => "✓",
                    Some(false) => "FAIL",
                    None => "n/a",
                },
                match p.passage_output_ok {
                    Some(true) => "✓",
                    Some(false) => "FAIL",
                    None => "n/a",
                },
                match p.hard_negative_ok {
                    Some(true) => "✓",
                    Some(false) => "FAIL",
                    None => "n/a",
                },
                match p.confidence_label_ok {
                    Some(true) => "✓",
                    Some(false) => "FAIL",
                    None => "n/a",
                },
                p.lane_used.as_deref().unwrap_or("n/a"),
                p.top_refs.join("; "),
                p.gold_refs.join("; ")
            ));
            md.push_str(&row);
        }
    }
    md.push('\n');
    md.push_str("## Notes\n\n");
    md.push_str("- **offline-keyword**: same full BSB TEXT, naive multi-term token overlap without stemming, synonym expansion, or phrase memory (proxy for classic keyword Bible search).\n");
    md.push_str("- **Live tools** (Bible Gateway, Blue Letter Bible / BibleHub): scores loaded from captured query results when available; environmentally unreachable tools are recorded as such rather than fabricated.\n");
    md.push_str("- Family registries require explicit family/split metadata and reject both literal-family and overlapping-passage leakage across split roles.\n");
    md.push_str("- `--release-gate` is the sealed-holdout semantic gate: it requires the strict release profile, an exactly bound dense embedding space and query vectors, family registries, minimum family/category/book diversity, R@1 ≥ 60%, R@3 ≥ 85%, R@10 ≥ 95%, and nonempty 100% safety, passage, hard-negative, and required-label slices.\n");
    md.push_str("- `--compatibility-gate` retains the older absolute bars for revealed regression suites. Passing it is a compatibility floor, not evidence of generalized semantic quality.\n");
    md
}

// ---------------------------------------------------------------------------
// Offline keyword baseline (naive token overlap, no stem/expand/phrase)
// ---------------------------------------------------------------------------

struct KeywordBaseline {
    verses: Vec<(String, String, HashSet<String>)>, // ref, text, tokens
}

impl KeywordBaseline {
    fn from_pack(pack: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let reader = PackReader::open(pack)?;
        if !reader.has(SectionTag::Text) {
            return Err("pack missing TEXT".into());
        }
        let text = reader.text()?;
        let verses = text
            .verses
            .into_iter()
            .map(|v| {
                let tokens = naive_tokens(&v.text);
                (v.reference, v.text, tokens)
            })
            .collect();
        Ok(Self { verses })
    }

    fn search(&self, query: &str, top_k: usize) -> Vec<String> {
        let q = naive_tokens(query);
        if q.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(f32, &str)> = Vec::new();
        for (reference, _text, tokens) in &self.verses {
            let mut hits = 0usize;
            for t in &q {
                if tokens.contains(t) {
                    hits += 1;
                }
            }
            if hits == 0 {
                continue;
            }
            let mut score = hits as f32;
            // Prefer covering more of the query (classic keyword AND-ish boost).
            score *= 1.0 + (hits as f32 / q.len() as f32);
            if hits == q.len() {
                score *= 1.5;
            }
            scored.push((score, reference.as_str()));
        }
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored.into_iter().map(|(_, r)| r.to_string()).collect()
    }
}

fn naive_tokens(s: &str) -> HashSet<String> {
    // Deliberately weak: no stemming, minimal stops (classic keyword tool proxy).
    const STOP: &[&str] = &[
        "a", "an", "the", "and", "or", "of", "to", "in", "on", "for", "is", "are", "was", "be",
        "that", "this", "it", "he", "she", "they", "we", "you", "i", "his", "her", "with", "as",
        "by", "from", "at", "not", "my", "your",
    ];
    let mut set = HashSet::new();
    let mut cur = String::new();
    for ch in s.chars() {
        if ch.is_alphanumeric() {
            cur.extend(ch.to_lowercase());
        } else if !cur.is_empty() {
            if cur.len() > 1 && !STOP.contains(&cur.as_str()) {
                set.insert(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
    }
    if cur.len() > 1 && !STOP.contains(&cur.as_str()) {
        set.insert(cur);
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt_digest_hex(prompt: &str) -> String {
        prompt_utf8_sha256(prompt)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn test_identity() -> ArtifactIdentity {
        ArtifactIdentity {
            path: "/tmp/test".into(),
            bytes: 1,
            sha256: "00".repeat(32),
        }
    }

    fn test_metadata(retrieval_depth: usize) -> RunMetadata {
        RunMetadata {
            evaluator_version: "test",
            evaluator_binary: test_identity(),
            base_pack: test_identity(),
            overlays: Vec::new(),
            prompts: test_identity(),
            family_registries: Vec::new(),
            live_results: None,
            query_embeddings: None,
            release_profile: None,
            embedding_space_id: None,
            retrieval_depth,
            split: None,
            baseline: false,
            dense_diagnostic: false,
            context_diagnostic: false,
            release_gate: false,
            compatibility_gate: false,
        }
    }

    fn strong_release_profile() -> SemanticReleaseProfile {
        SemanticReleaseProfile {
            schema_version: 1,
            base_pack_sha256: "11".repeat(32),
            overlay_sha256: vec!["22".repeat(32)],
            embedding_space_id: "33".repeat(32),
            minimums: ReleaseCoverageMinimums {
                total_prompts: RELEASE_MIN_TOTAL_PROMPTS,
                retrieval_prompts: RELEASE_MIN_RETRIEVAL_PROMPTS,
                no_gold_prompts: RELEASE_MIN_NO_GOLD_PROMPTS,
                hard_negative_prompts: RELEASE_MIN_HARD_NEGATIVE_PROMPTS,
                confidence_label_prompts: RELEASE_MIN_CONFIDENCE_LABEL_PROMPTS,
                structured_passage_prompts: RELEASE_MIN_STRUCTURED_PASSAGE_PROMPTS,
                distinct_families: RELEASE_MIN_DISTINCT_FAMILIES,
                distinct_categories: RELEASE_MIN_DISTINCT_CATEGORIES,
                distinct_target_books: RELEASE_MIN_DISTINCT_TARGET_BOOKS,
            },
            required_categories: RELEASE_CATEGORY_FLOORS
                .into_iter()
                .map(|(category, minimum)| (category.to_string(), minimum))
                .collect(),
        }
    }

    fn diverse_release_prompts() -> Vec<Prompt> {
        const BOOKS: [&str; 40] = [
            "Genesis 1:1",
            "Exodus 1:1",
            "Leviticus 1:1",
            "Numbers 1:1",
            "Deuteronomy 1:1",
            "Joshua 1:1",
            "Judges 1:1",
            "Ruth 1:1",
            "1 Samuel 1:1",
            "2 Samuel 1:1",
            "1 Kings 1:1",
            "2 Kings 1:1",
            "1 Chronicles 1:1",
            "2 Chronicles 1:1",
            "Ezra 1:1",
            "Nehemiah 1:1",
            "Esther 1:1",
            "Job 1:1",
            "Psalm 1:1",
            "Proverbs 1:1",
            "Ecclesiastes 1:1",
            "Song of Solomon 1:1",
            "Isaiah 1:1",
            "Jeremiah 1:1",
            "Lamentations 1:1",
            "Ezekiel 1:1",
            "Daniel 1:1",
            "Hosea 1:1",
            "Joel 1:1",
            "Amos 1:1",
            "Obadiah 1:1",
            "Jonah 1:1",
            "Micah 1:1",
            "Nahum 1:1",
            "Habakkuk 1:1",
            "Zephaniah 1:1",
            "Haggai 1:1",
            "Zechariah 1:1",
            "Malachi 1:1",
            "Matthew 1:1",
        ];
        (0..80)
            .map(|index| {
                let category = match index {
                    0..=11 => "meaning-only",
                    12..=19 => "multi-intent",
                    20..=27 => "event-description",
                    28..=32 => "near-quote",
                    33..=37 => "translation-drift",
                    38..=42 => "entity-event",
                    43..=51 => "false-memory",
                    52..=60 => "misconception",
                    61..=69 => "place-description",
                    _ => "rare-metaphor",
                };
                Prompt {
                    id: format!("sealed-{index}"),
                    prompt: format!("unseen semantic memory {index}"),
                    category: category.into(),
                    gold_refs: if index < 72 {
                        vec![BOOKS[index % BOOKS.len()].to_string()]
                    } else {
                        Vec::new()
                    },
                    gold_mode: GoldMode::Any,
                    must_not: if index < 8 {
                        vec!["Revelation 22:21".into()]
                    } else {
                        Vec::new()
                    },
                    expected_confidence: (index < 4).then_some(ExpectedConfidence::Related),
                    expected_passages: if index < 8 {
                        vec![BOOKS[index % BOOKS.len()].to_string()]
                    } else {
                        Vec::new()
                    },
                    family: Some(format!("sealed-family-{index}")),
                    split: Some("holdout-synthetic-v1".into()),
                    tags: Vec::new(),
                    source: Some("synthetic unit fixture".into()),
                    notes: None,
                }
            })
            .collect()
    }

    fn valid_runtime_evidence(prompts: &[Prompt]) -> SemanticRuntimeEvidence {
        SemanticRuntimeEvidence {
            base_pack_sha256: "11".repeat(32),
            overlay_sha256: vec!["22".repeat(32)],
            cli_embedding_space_id: Some("33".repeat(32)),
            dense_embedding_space_id: Some("33".repeat(32)),
            dense_dimensions: Some(256),
            query_embedding_count: prompts.len(),
            query_embedding_dimensions: vec![256; prompts.len()],
            query_embedding_ids: prompts.iter().map(|prompt| prompt.id.clone()).collect(),
            family_registry_count: 1,
            holdout_only: true,
        }
    }

    #[test]
    fn semantic_release_cli_requires_profile_vectors_space_and_registry() {
        let missing_profile = Cli::try_parse_from([
            "recall-eval",
            "--pack",
            "base.pack",
            "--release-gate",
            "--query-embeddings",
            "queries.jsonl",
            "--embedding-space-id",
            &"33".repeat(32),
            "--family-registry",
            "registry.jsonl",
        ])
        .expect_err("profile must be mandatory");
        assert!(missing_profile.to_string().contains("release-profile"));

        let missing_vectors = Cli::try_parse_from([
            "recall-eval",
            "--pack",
            "base.pack",
            "--release-gate",
            "--release-profile",
            "profile.json",
            "--embedding-space-id",
            &"33".repeat(32),
            "--family-registry",
            "registry.jsonl",
        ])
        .expect_err("vectors must be mandatory");
        assert!(missing_vectors.to_string().contains("query-embeddings"));
    }

    #[test]
    fn semantic_release_profile_is_strict_and_cannot_weaken_floors() {
        let profile = strong_release_profile();
        validate_release_profile_strength(&profile).expect("hard floors");

        let mut weak = profile.clone();
        weak.minimums.distinct_families = RELEASE_MIN_DISTINCT_FAMILIES - 1;
        assert!(validate_release_profile_strength(&weak)
            .expect_err("weak family floor")
            .contains("weakens"));

        let mut weak_category = profile.clone();
        weak_category
            .required_categories
            .insert("meaning-only".into(), 11);
        assert!(validate_release_profile_strength(&weak_category)
            .expect_err("weak category floor")
            .contains("meaning-only"));

        let mut json = serde_json::to_value(profile).expect("serialize profile");
        json.as_object_mut()
            .expect("profile object")
            .insert("unexpected".into(), serde_json::json!(true));
        assert!(parse_semantic_release_profile(
            serde_json::to_vec(&json)
                .expect("encode profile")
                .as_slice()
        )
        .is_err());
    }

    #[test]
    fn checked_in_semantic_release_profile_meets_hard_floors() {
        let profile = parse_semantic_release_profile(include_bytes!(
            "../../../eval/semantic-release-profile-v1.json"
        ))
        .expect("checked-in semantic release profile");
        validate_release_profile_strength(&profile).expect("non-weakenable release floors");
    }

    #[test]
    fn semantic_release_inputs_require_dense_vectors_and_exact_artifact_binding() {
        let profile = strong_release_profile();
        let prompts = diverse_release_prompts();
        let evidence = valid_runtime_evidence(&prompts);
        validate_semantic_release_inputs(&profile, &prompts, &evidence)
            .expect("valid sealed release evidence");

        let mut missing_dense = evidence.clone();
        missing_dense.dense_embedding_space_id = None;
        missing_dense.dense_dimensions = None;
        assert!(
            validate_semantic_release_inputs(&profile, &prompts, &missing_dense)
                .expect_err("dense pack is mandatory")
                .contains("dense pack")
        );

        let mut missing_queries = evidence.clone();
        missing_queries.query_embedding_count = 0;
        missing_queries.query_embedding_dimensions.clear();
        missing_queries.query_embedding_ids.clear();
        assert!(
            validate_semantic_release_inputs(&profile, &prompts, &missing_queries)
                .expect_err("query vectors are mandatory")
                .contains("query embeddings")
        );

        let mut missing_registry = evidence.clone();
        missing_registry.family_registry_count = 0;
        assert!(
            validate_semantic_release_inputs(&profile, &prompts, &missing_registry)
                .expect_err("family registry is mandatory")
                .contains("family registry")
        );

        let mut revealed = evidence.clone();
        revealed.holdout_only = false;
        assert!(
            validate_semantic_release_inputs(&profile, &prompts, &revealed)
                .expect_err("revealed suites use compatibility gate")
                .contains("compatibility-gate")
        );

        for bad in [
            {
                let mut value = evidence.clone();
                value.base_pack_sha256 = "44".repeat(32);
                value
            },
            {
                let mut value = evidence.clone();
                value.overlay_sha256 = vec!["55".repeat(32)];
                value
            },
            {
                let mut value = evidence.clone();
                value.cli_embedding_space_id = Some("66".repeat(32));
                value
            },
            {
                let mut value = evidence.clone();
                value.dense_embedding_space_id = Some("77".repeat(32));
                value
            },
        ] {
            assert!(validate_semantic_release_inputs(&profile, &prompts, &bad).is_err());
        }
    }

    #[test]
    fn semantic_release_rejects_inadequate_family_category_and_book_diversity() {
        let profile = strong_release_profile();
        let prompts = diverse_release_prompts();

        let mut weak_families = prompts.clone();
        for prompt in &mut weak_families {
            prompt.family = Some("one-family".into());
        }
        assert!(validate_release_coverage(&profile, &weak_families)
            .expect_err("family diversity")
            .contains("distinct_families"));

        let mut weak_categories = prompts.clone();
        for prompt in &mut weak_categories {
            prompt.category = "meaning-only".into();
        }
        assert!(validate_release_coverage(&profile, &weak_categories)
            .expect_err("category diversity")
            .contains("distinct_categories"));

        let mut weak_books = prompts;
        for prompt in &mut weak_books {
            if !prompt.gold_refs.is_empty() {
                prompt.gold_refs = vec!["Genesis 1:1".into()];
            }
        }
        assert!(validate_release_coverage(&profile, &weak_books)
            .expect_err("book diversity")
            .contains("distinct_target_books"));
    }

    #[test]
    fn semantic_release_rejects_zero_safety_and_correction_denominators() {
        let profile = strong_release_profile();
        let metric = MetricScore {
            queries: 80,
            retrieval_queries: 72,
            no_gold_queries: 8,
            hits_at_1: 44,
            hits_at_3: 62,
            hits_at_10: 69,
            recall_at_1: 0.61,
            recall_at_3: 0.86,
            recall_at_10: 0.96,
            safety_queries: 80,
            safe_confidence_queries: 80,
            confidence_safety: 1.0,
            safe_no_gold: 8,
            no_gold_safety: 1.0,
            ..MetricScore::default()
        };
        let error = validate_semantic_release_metrics(&metric, &profile)
            .expect_err("empty passage/hard-negative/label slices must fail");
        assert!(error.contains("passage output=0.0% (0/0)"));

        let passing = MetricScore {
            passage_queries: 8,
            passage_passes: 8,
            passage_accuracy: 1.0,
            hard_negative_queries: 8,
            hard_negative_passes: 8,
            hard_negative_accuracy: 1.0,
            confidence_label_queries: 4,
            confidence_label_passes: 4,
            confidence_label_accuracy: 1.0,
            ..metric
        };
        validate_semantic_release_metrics(&passing, &profile).expect("complete strict slices");
    }

    #[test]
    fn artifact_identity_uses_sha256() {
        let identity = artifact_identity(std::path::Path::new("fixture"), b"abc");
        assert_eq!(
            identity.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn structured_ranges_are_compacted_into_matchable_eval_references() {
        let same_chapter = compact_passage_range("Ephesians 6:10", "Ephesians 6:18");
        assert_eq!(same_chapter, "Ephesians 6:10-18");
        assert!(refs_match(&same_chapter, "Ephesians 6:11-12"));

        let cross_chapter = compact_passage_range("Matthew 5:1", "Matthew 7:29");
        assert_eq!(cross_chapter, "Matthew 5:1-7:29");
        assert!(refs_match(&cross_chapter, "Matthew 6:9-13"));
    }

    #[test]
    fn evaluator_propagates_search_errors_instead_of_panicking() {
        let prompts = parse_prompts(
            r#"{"id":"error-path","prompt":"test query","category":"test","gold_refs":["Genesis 1:1"]}"#,
        )
        .expect("prompt fixture");
        let result = eval_system("fallible", &prompts, |_| {
            Err::<PromptScore, Box<dyn std::error::Error>>("incompatible embedding".into())
        });

        assert_eq!(
            result.expect_err("search error must propagate").to_string(),
            "incompatible embedding"
        );
    }

    #[test]
    fn segmented_query_embeddings_accept_the_supported_ordered_planner() {
        let prompt = "first remembered thought and second remembered thought";
        let raw = format!(
            r#"{{"id":"multi-intent","prompt_utf8_sha256":"{}","embedding":[0.1,0.2,0.3],"planner":"verse-recall-clause-v2","segments":[{{"text":"first remembered thought","embedding":[0.4,0.5,0.6]}},{{"text":"second remembered thought","embedding":[0.7,0.8,0.9]}}]}}"#,
            prompt_digest_hex(prompt)
        );

        let parsed =
            parse_query_embeddings(Some(raw.as_bytes())).expect("valid segmented embeddings");
        let prompts = parse_prompts(
            r#"{"id":"multi-intent","prompt":"first remembered thought and second remembered thought","category":"multi-intent","gold_refs":["Genesis 1:1"]}"#,
        )
        .expect("prompt fixture");
        validate_query_embedding_plans(&prompts, &parsed)
            .expect("canonical segments must be supplied in planner order");
        let query = parsed.get("multi-intent").expect("query record");
        assert_eq!(query.embedding, [0.1, 0.2, 0.3]);
        assert_eq!(query.segments.len(), 2);
        assert_eq!(query.segments[0].text, "first remembered thought");
        assert_eq!(query.segments[0].embedding, [0.4, 0.5, 0.6]);
        assert_eq!(query.segments[1].text, "second remembered thought");
        assert_eq!(query.segments[1].embedding, [0.7, 0.8, 0.9]);
    }

    #[test]
    fn evaluator_rejects_injected_edited_or_reordered_segment_texts_before_search() {
        let prompts = parse_prompts(
            r#"{"id":"multi-intent","prompt":"first remembered thought and second remembered image","category":"multi-intent","gold_refs":["Genesis 1:1"]}"#,
        )
        .expect("prompt fixture");

        for raw in [
            format!(
                r#"{{"id":"multi-intent","prompt_utf8_sha256":"{}","embedding":[0.1,0.2],"planner":"verse-recall-clause-v2","segments":[{{"text":"first remembered thought","embedding":[0.3,0.4]}},{{"text":"injected gold-directed clause","embedding":[0.5,0.6]}}]}}"#,
                prompt_digest_hex("first remembered thought and second remembered image")
            ),
            format!(
                r#"{{"id":"multi-intent","prompt_utf8_sha256":"{}","embedding":[0.1,0.2],"planner":"verse-recall-clause-v2","segments":[{{"text":"First remembered thought","embedding":[0.3,0.4]}},{{"text":"second remembered image","embedding":[0.5,0.6]}}]}}"#,
                prompt_digest_hex("first remembered thought and second remembered image")
            ),
            format!(
                r#"{{"id":"multi-intent","prompt_utf8_sha256":"{}","embedding":[0.1,0.2],"planner":"verse-recall-clause-v2","segments":[{{"text":"second remembered image","embedding":[0.5,0.6]}},{{"text":"first remembered thought","embedding":[0.3,0.4]}}]}}"#,
                prompt_digest_hex("first remembered thought and second remembered image")
            ),
        ] {
            let parsed =
                parse_query_embeddings(Some(raw.as_bytes())).expect("structurally valid vectors");
            let error = validate_query_embedding_plans(&prompts, &parsed)
                .expect_err("noncanonical clauses must fail at ingestion")
                .to_string();
            assert!(error.contains("canonical ordered query-only planner"));
        }
    }

    #[test]
    fn segmented_query_embeddings_reject_missing_or_unknown_planners() {
        for (description, raw) in [
            (
                "missing planner",
                br#"{"id":"missing","prompt_utf8_sha256":"0000000000000000000000000000000000000000000000000000000000000000","embedding":[0.1,0.2],"segments":[{"text":"a clause","embedding":[0.3,0.4]}]}"#.as_slice(),
            ),
            (
                "unknown planner",
                br#"{"id":"unknown","prompt_utf8_sha256":"0000000000000000000000000000000000000000000000000000000000000000","embedding":[0.1,0.2],"planner":"another-planner","segments":[{"text":"a clause","embedding":[0.3,0.4]}]}"#.as_slice(),
            ),
            (
                "stale planner",
                br#"{"id":"stale","prompt_utf8_sha256":"0000000000000000000000000000000000000000000000000000000000000000","embedding":[0.1,0.2],"planner":"verse-recall-clause-v1","segments":[{"text":"a clause","embedding":[0.3,0.4]}]}"#.as_slice(),
            ),
        ] {
            let error = parse_query_embeddings(Some(raw))
                .expect_err(description)
                .to_string();
            assert!(
                error.contains("segments without the supported planner"),
                "unexpected {description} error: {error}"
            );
        }
    }

    #[test]
    fn query_embeddings_reject_record_and_segment_dimension_mismatches() {
        let inconsistent_records = br#"
            {"id":"first","prompt_utf8_sha256":"0000000000000000000000000000000000000000000000000000000000000000","embedding":[0.1,0.2]}
            {"id":"second","prompt_utf8_sha256":"0000000000000000000000000000000000000000000000000000000000000000","embedding":[0.1,0.2,0.3]}
        "#;
        let error = parse_query_embeddings(Some(inconsistent_records))
            .expect_err("record dimensions must agree")
            .to_string();
        assert!(error.contains("line 3 has inconsistent dimensions"));

        let inconsistent_segment = br#"{"id":"segmented","prompt_utf8_sha256":"0000000000000000000000000000000000000000000000000000000000000000","embedding":[0.1,0.2,0.3],"planner":"verse-recall-clause-v2","segments":[{"text":"a clause","embedding":[0.4,0.5]}]}"#;
        let error = parse_query_embeddings(Some(inconsistent_segment))
            .expect_err("segment dimensions must match the whole query")
            .to_string();
        assert!(error.contains("line 1 has an invalid segment text or vector"));
    }

    #[test]
    fn prompt_bound_unsegmented_query_embedding_records_remain_supported() {
        let raw = br#"
            {"id":"legacy-one","prompt_utf8_sha256":"0000000000000000000000000000000000000000000000000000000000000000","embedding":[0.1,0.2]}

            {"id":"legacy-two","prompt_utf8_sha256":"1111111111111111111111111111111111111111111111111111111111111111","embedding":[0.3,0.4]}
        "#;

        let parsed = parse_query_embeddings(Some(raw)).expect("prompt-bound embedding records");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed["legacy-one"].embedding, [0.1, 0.2]);
        assert!(parsed["legacy-one"].segments.is_empty());
        assert_eq!(parsed["legacy-two"].embedding, [0.3, 0.4]);
        assert!(parsed["legacy-two"].segments.is_empty());
    }

    #[test]
    fn query_embeddings_require_and_verify_exact_prompt_utf8_sha256() {
        assert_eq!(
            prompt_digest_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let prompt_text = "  Lógos 🌿  ";
        let prompts = parse_prompts(&format!(
            r#"{{"id":"unicode","prompt":{},"category":"test","gold_refs":["John 1:1"]}}"#,
            serde_json::to_string(prompt_text).expect("serialize test prompt")
        ))
        .expect("prompt fixture");
        let valid = format!(
            r#"{{"id":"unicode","prompt_utf8_sha256":"{}","embedding":[0.1,0.2]}}"#,
            prompt_digest_hex(prompt_text)
        );
        let parsed = parse_query_embeddings(Some(valid.as_bytes())).expect("bound record");
        validate_query_embedding_plans(&prompts, &parsed).expect("exact UTF-8 binding");

        let stale = format!(
            r#"{{"id":"unicode","prompt_utf8_sha256":"{}","embedding":[0.1,0.2]}}"#,
            prompt_digest_hex("a different prompt")
        );
        let parsed = parse_query_embeddings(Some(stale.as_bytes())).expect("valid digest shape");
        let error = validate_query_embedding_plans(&prompts, &parsed)
            .expect_err("stale vectors must fail before evaluation")
            .to_string();
        assert!(error.contains("does not match the exact prompt UTF-8 bytes"));

        for raw in [
            r#"{"id":"missing","embedding":[0.1,0.2]}"#,
            r#"{"id":"uppercase","prompt_utf8_sha256":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","embedding":[0.1,0.2]}"#,
        ] {
            assert!(parse_query_embeddings(Some(raw.as_bytes())).is_err());
        }
    }

    #[test]
    fn markdown_reports_only_the_deep_cutoffs_that_were_measured() {
        let prompts = parse_prompts(
            r#"{"id":"deep","prompt":"query","category":"test","gold_refs":["John 3:16"],"family":"deep-family"}"#,
        )
        .expect("prompt fixture");
        let mut results = vec!["Genesis 1:1".into(); 100];
        results[41] = "John 3:16".into();

        let shallow_score = score_prompt_at_depth(&prompts[0], &results, None, 10);
        let shallow = summarize_system("test", vec![shallow_score]);
        let shallow_report = render_report(&[shallow], &prompts, &test_metadata(10));
        assert!(!shallow_report.contains("R@50"));
        assert!(!shallow_report.contains("R@100"));

        let fifty_score = score_prompt_at_depth(&prompts[0], &results, None, 50);
        let fifty = summarize_system("test", vec![fifty_score]);
        let fifty_report = render_report(&[fifty], &prompts, &test_metadata(50));
        assert!(fifty_report.contains("R@50"));
        assert!(!fifty_report.contains("R@100"));
        assert!(fifty_report.contains("100.0%"));

        let hundred_score = score_prompt_at_depth(&prompts[0], &results, None, 100);
        let hundred = summarize_system("test", vec![hundred_score]);
        let hundred_report = render_report(&[hundred], &prompts, &test_metadata(100));
        assert!(hundred_report.contains("R@50"));
        assert!(hundred_report.contains("R@100"));
        assert!(hundred_report.contains("| 100.0% | 100.0% |"));
    }
}

//! Export paired search responses for an offline, revealed-suite fusion bake-off.
//!
//! This is deliberately an evaluation helper rather than production ranking
//! code. It lets the sibling `evaluate_dense_pair` example compare two dense
//! representations while holding the base corpus, sparse overlays, encoder,
//! query planner, and retrieval depth constant.

use clap::Parser;
use recall_core::{
    plan_query_segments, Engine, QuerySegmentEmbedding, SearchOpts, QUERY_SEGMENT_PLANNER,
};
use recall_eval::{parse_prompts, Prompt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser)]
#[command(about = "Export paired dense-view results for revealed-suite research")]
struct Cli {
    #[arg(long)]
    pack: PathBuf,
    #[arg(long = "shared-overlay")]
    shared_overlays: Vec<PathBuf>,
    #[arg(long)]
    primary_dense: PathBuf,
    #[arg(long)]
    auxiliary_dense: PathBuf,
    #[arg(long)]
    embedding_space_id: String,
    /// Repeated `name,prompts.jsonl,query-embeddings.jsonl` specification.
    #[arg(long = "suite")]
    suites: Vec<String>,
    #[arg(long)]
    output: PathBuf,
    #[arg(long, default_value_t = 100)]
    depth: usize,
}

#[derive(Debug)]
struct SuiteSpec {
    name: String,
    prompts: PathBuf,
    embeddings: PathBuf,
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

#[derive(Serialize)]
struct Export<'a> {
    schema_version: u32,
    depth: usize,
    primary_dense: String,
    auxiliary_dense: String,
    rows: Vec<ExportRow<'a>>,
}

#[derive(Serialize)]
struct ExportRow<'a> {
    suite: &'a str,
    prompt: &'a Prompt,
    primary_micros: u128,
    auxiliary_micros: u128,
    primary: recall_core::SearchResponse,
    auxiliary: recall_core::SearchResponse,
}

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn Error>> {
    if cli.suites.is_empty() {
        return Err("at least one --suite is required".into());
    }
    let depth = cli.depth.clamp(1, 100);
    let space_id = parse_space_id(&cli.embedding_space_id)?;
    let specs: Vec<SuiteSpec> = cli
        .suites
        .iter()
        .map(|value| parse_suite(value))
        .collect::<Result<_, _>>()?;

    let base = fs::read(&cli.pack)?;
    let shared: Vec<Vec<u8>> = cli
        .shared_overlays
        .iter()
        .map(fs::read)
        .collect::<Result<_, _>>()?;
    let primary_dense = fs::read(&cli.primary_dense)?;
    let auxiliary_dense = fs::read(&cli.auxiliary_dense)?;
    let primary = open_engine(&base, &shared, &primary_dense)?;
    let auxiliary = open_engine(&base, &shared, &auxiliary_dense)?;

    let mut suite_data = Vec::with_capacity(specs.len());
    for spec in &specs {
        let prompts = parse_prompts(&fs::read_to_string(&spec.prompts)?)?;
        let embeddings = parse_embeddings(&spec.embeddings)?;
        validate_embeddings(&prompts, &embeddings)?;
        suite_data.push((spec, prompts, embeddings));
    }

    let opts = SearchOpts::default().with_top_k(depth);
    let mut rows = Vec::new();
    for (spec, prompts, embeddings) in &suite_data {
        for prompt in prompts {
            let embedding = embeddings
                .get(&prompt.id)
                .ok_or_else(|| format!("{} lacks embedding {}", spec.name, prompt.id))?;
            let started = Instant::now();
            let primary_response = search(&primary, prompt, embedding, &space_id, &opts)?;
            let primary_micros = started.elapsed().as_micros();
            let started = Instant::now();
            let auxiliary_response = search(&auxiliary, prompt, embedding, &space_id, &opts)?;
            let auxiliary_micros = started.elapsed().as_micros();
            rows.push(ExportRow {
                suite: &spec.name,
                prompt,
                primary_micros,
                auxiliary_micros,
                primary: primary_response,
                auxiliary: auxiliary_response,
            });
        }
    }
    let export = Export {
        schema_version: 1,
        depth,
        primary_dense: canonical_display(&cli.primary_dense),
        auxiliary_dense: canonical_display(&cli.auxiliary_dense),
        rows,
    };
    fs::write(&cli.output, serde_json::to_vec(&export)?)?;
    Ok(())
}

fn open_engine(base: &[u8], shared: &[Vec<u8>], dense: &[u8]) -> Result<Engine, Box<dyn Error>> {
    let mut packs = Vec::with_capacity(2 + shared.len());
    packs.push(base);
    packs.extend(shared.iter().map(Vec::as_slice));
    packs.push(dense);
    Ok(Engine::open(&packs)?)
}

fn search(
    engine: &Engine,
    prompt: &Prompt,
    query: &QueryEmbedding,
    space_id: &[u8; 32],
    opts: &SearchOpts,
) -> Result<recall_core::SearchResponse, Box<dyn Error>> {
    if query.segments.is_empty() {
        return Ok(engine.search_with_embedding_in_space(
            &prompt.prompt,
            space_id,
            &query.embedding,
            opts,
        )?);
    }
    let segments: Vec<QuerySegmentEmbedding<'_>> = query
        .segments
        .iter()
        .map(|segment| QuerySegmentEmbedding {
            text: &segment.text,
            embedding: &segment.embedding,
        })
        .collect();
    Ok(engine.search_with_segment_embeddings_in_space(
        &prompt.prompt,
        space_id,
        &query.embedding,
        &segments,
        opts,
    )?)
}

fn parse_suite(value: &str) -> Result<SuiteSpec, Box<dyn Error>> {
    let fields: Vec<&str> = value.split(',').collect();
    if fields.len() != 3 || fields.iter().any(|field| field.trim().is_empty()) {
        return Err(format!(
            "invalid --suite {value:?}; expected name,prompts.jsonl,embeddings.jsonl"
        )
        .into());
    }
    Ok(SuiteSpec {
        name: fields[0].to_string(),
        prompts: fields[1].into(),
        embeddings: fields[2].into(),
    })
}

fn parse_embeddings(path: &Path) -> Result<HashMap<String, QueryEmbedding>, Box<dyn Error>> {
    let raw = fs::read_to_string(path)?;
    let mut output = HashMap::new();
    let mut dimensions = None;
    for (line_index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: QueryEmbeddingRecord = serde_json::from_str(line)
            .map_err(|error| format!("{} line {}: {error}", path.display(), line_index + 1))?;
        let digest = parse_space_id(&record.prompt_utf8_sha256)?;
        if record.id.trim().is_empty()
            || record.embedding.is_empty()
            || record.embedding.iter().any(|value| !value.is_finite())
        {
            return Err(format!(
                "{} line {} has an invalid vector",
                path.display(),
                line_index + 1
            )
            .into());
        }
        if let Some(expected) = dimensions {
            if record.embedding.len() != expected {
                return Err(format!("{} has inconsistent dimensions", path.display()).into());
            }
        } else {
            dimensions = Some(record.embedding.len());
        }
        if !record.segments.is_empty() && record.planner.as_deref() != Some(QUERY_SEGMENT_PLANNER) {
            return Err(format!(
                "{} line {} has unsupported segments",
                path.display(),
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
                "{} line {} has an invalid segment",
                path.display(),
                line_index + 1
            )
            .into());
        }
        let id = record.id;
        if output
            .insert(
                id.clone(),
                QueryEmbedding {
                    prompt_utf8_sha256: digest,
                    embedding: record.embedding,
                    segments: record.segments,
                },
            )
            .is_some()
        {
            return Err(format!("{} duplicates id {id}", path.display()).into());
        }
    }
    Ok(output)
}

fn validate_embeddings(
    prompts: &[Prompt],
    embeddings: &HashMap<String, QueryEmbedding>,
) -> Result<(), Box<dyn Error>> {
    for prompt in prompts {
        let query = embeddings
            .get(&prompt.id)
            .ok_or_else(|| format!("query embedding missing prompt id {}", prompt.id))?;
        let actual: [u8; 32] = Sha256::digest(prompt.prompt.as_bytes()).into();
        if query.prompt_utf8_sha256 != actual {
            return Err(format!("query embedding {} is stale or swapped", prompt.id).into());
        }
        if query.segments.is_empty() {
            continue;
        }
        let expected: BTreeSet<String> = plan_query_segments(&prompt.prompt)
            .segments
            .into_iter()
            .collect();
        let supplied: BTreeSet<String> = query
            .segments
            .iter()
            .map(|segment| segment.text.clone())
            .collect();
        if expected.len() != query.segments.len() || expected != supplied {
            return Err(format!(
                "query embedding {} has a noncanonical segment plan",
                prompt.id
            )
            .into());
        }
    }
    Ok(())
}

fn parse_space_id(value: &str) -> Result<[u8; 32], Box<dyn Error>> {
    if value.len() != 64
        || value
            .as_bytes()
            .iter()
            .any(|byte| !matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err("identity must be 64 lowercase hexadecimal characters".into());
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair)?;
        output[index] = u8::from_str_radix(text, 16)?;
    }
    Ok(output)
}

fn canonical_display(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

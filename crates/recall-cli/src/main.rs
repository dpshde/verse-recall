//! Dev CLI: search a pack, inspect sections.

use clap::{Parser, Subcommand};
use recall_core::{Engine, QuerySegmentEmbedding, SearchOpts, QUERY_SEGMENT_PLANNER};
use recall_pack::{PackReader, SectionTag};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "recall-cli", about = "Search and inspect verse-recall packs")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a tip-of-the-tongue query against a pack
    Search {
        #[arg(long, short)]
        pack: PathBuf,
        /// Section-only recall overlay pack(s), in precedence-neutral order.
        #[arg(long = "overlay", value_name = "PACK")]
        overlays: Vec<PathBuf>,
        /// Query string
        query: String,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        #[arg(long)]
        book_hint: Option<String>,
        /// Print raw JSON
        #[arg(long)]
        json: bool,
        /// Diagnostic ablation over CLEX/CPOST Study Note context only.
        #[arg(long, conflicts_with = "query_embeddings")]
        context_only: bool,
        /// One JSON object containing `embedding` and optional canonical
        /// `planner`/`segments`, as produced by build_semantic_vectors.py.
        #[arg(long, requires = "embedding_space_id")]
        query_embeddings: Option<PathBuf>,
        /// 64-hex embedding-space identity required with query embeddings.
        #[arg(long, requires = "query_embeddings")]
        embedding_space_id: Option<String>,
    },
    /// List sections and META
    Inspect {
        #[arg(long, short)]
        pack: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.cmd {
        Cmd::Search {
            pack,
            overlays,
            query,
            top_k,
            book_hint,
            json,
            context_only,
            query_embeddings,
            embedding_space_id,
        } => {
            let mut buffers = Vec::with_capacity(1 + overlays.len());
            buffers.push(fs::read(&pack)?);
            for overlay in overlays {
                buffers.push(fs::read(overlay)?);
            }
            let pack_refs: Vec<&[u8]> = buffers.iter().map(Vec::as_slice).collect();
            let eng = Engine::open(&pack_refs)?;
            let mut opts = SearchOpts::default().with_top_k(top_k);
            if let Some(h) = book_hint {
                opts = opts.with_book_hint(h);
            }
            let response = if context_only {
                eng.search_context_only(&query, &opts)
            } else if let (Some(path), Some(space_id)) =
                (query_embeddings.as_ref(), embedding_space_id.as_deref())
            {
                let record = read_query_embedding_record(path)?;
                let space_id = parse_space_id(space_id)?;
                if record.segments.is_empty() {
                    eng.search_with_embedding_in_space(&query, &space_id, &record.embedding, &opts)?
                } else {
                    if record.planner.as_deref() != Some(QUERY_SEGMENT_PLANNER) {
                        return Err(format!(
                            "query embedding planner must be {QUERY_SEGMENT_PLANNER}"
                        )
                        .into());
                    }
                    let segments: Vec<QuerySegmentEmbedding<'_>> = record
                        .segments
                        .iter()
                        .map(|segment| QuerySegmentEmbedding {
                            text: &segment.text,
                            embedding: &segment.embedding,
                        })
                        .collect();
                    eng.search_with_segment_embeddings_in_space(
                        &query,
                        &space_id,
                        &record.embedding,
                        &segments,
                        &opts,
                    )?
                }
            } else {
                eng.search(&query, &opts)
            };
            if json {
                println!("{}", serde_json::to_string(&response)?);
            } else {
                let resp = response;
                println!("query: {}", resp.query);
                println!(
                    "translation: {} ({} verses)",
                    eng.translation_id(),
                    eng.verse_count()
                );
                for (i, r) in resp.results.iter().enumerate() {
                    println!(
                        "{}. {}  [{:.4}]  {}",
                        i + 1,
                        r.reference,
                        r.score,
                        r.confidence.as_str()
                    );
                    println!("   {}", truncate(&r.text, 120));
                    if !r.chips.is_empty() {
                        println!("   chips: {}", r.chips.join(", "));
                    }
                    if !r.passages.is_empty() {
                        let passages: Vec<String> = r
                            .passages
                            .iter()
                            .map(|passage| {
                                if passage.start_ref == passage.end_ref {
                                    passage.start_ref.clone()
                                } else {
                                    format!("{}–{}", passage.start_ref, passage.end_ref)
                                }
                            })
                            .collect();
                        println!("   passages: {}", passages.join("; "));
                    }
                }
            }
        }
        Cmd::Inspect { pack } => {
            let bytes = fs::read(&pack)?;
            let reader = PackReader::open(&bytes)?;
            let h = reader.header();
            println!("format_version: {}", h.format_version);
            println!("sections: {}", h.section_count);
            for s in reader.sections() {
                println!(
                    "  {:8} offset={:<10} len={}",
                    s.tag.as_str(),
                    s.offset,
                    s.len
                );
            }
            if reader.has(SectionTag::Meta) {
                let m = reader.meta()?;
                println!(
                    "META: translation={} verse_count={}",
                    m.translation_id, m.verse_count
                );
            }
        }
    }
    Ok(())
}

struct QueryEmbeddingRecord {
    embedding: Vec<f32>,
    planner: Option<String>,
    segments: Vec<OwnedQuerySegment>,
}

struct OwnedQuerySegment {
    text: String,
    embedding: Vec<f32>,
}

fn read_query_embedding_record(
    path: &PathBuf,
) -> Result<QueryEmbeddingRecord, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(path)?;
    let lines: Vec<&str> = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    if lines.len() != 1 {
        return Err("query embedding input must contain exactly one JSON object".into());
    }
    let value: serde_json::Value = serde_json::from_str(lines[0])?;
    let embedding = parse_f32_array(
        value
            .get("embedding")
            .ok_or("query embedding record is missing embedding")?,
    )?;
    let planner = value
        .get("planner")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let mut segments = Vec::new();
    if let Some(values) = value.get("segments") {
        let values = values
            .as_array()
            .ok_or("query embedding segments must be an array")?;
        for segment in values {
            let text = segment
                .get("text")
                .and_then(serde_json::Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .ok_or("query segment requires non-empty text")?
                .to_string();
            let embedding = parse_f32_array(
                segment
                    .get("embedding")
                    .ok_or("query segment is missing embedding")?,
            )?;
            segments.push(OwnedQuerySegment { text, embedding });
        }
    }
    Ok(QueryEmbeddingRecord {
        embedding,
        planner,
        segments,
    })
}

fn parse_f32_array(value: &serde_json::Value) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let values = value.as_array().ok_or("embedding must be an array")?;
    if values.is_empty() {
        return Err("embedding must not be empty".into());
    }
    values
        .iter()
        .map(|value| {
            let number = value.as_f64().ok_or("embedding values must be numbers")? as f32;
            if !number.is_finite() {
                return Err("embedding values must be finite".into());
            }
            Ok(number)
        })
        .collect()
}

fn parse_space_id(value: &str) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("embedding-space ID must contain exactly 64 hex characters".into());
    }
    let mut bytes = [0u8; 32];
    for (index, slot) in bytes.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(bytes)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}

//! Build-time packer: sources → `.pack`.
//!
//! Scaffold: accepts a simple JSON lines file of `{ "ref", "text" }` and writes
//! META+TEXT. Embeddings / lemmas / postings land in later iterations (fastembed
//! / ONNX live only in this crate).

use clap::{Parser, Subcommand};
use recall_core::{
    stem, tokenize_text, DenseLane, DenseTableRole, RecallOverlay, NORMALIZER_VERSION,
};
use recall_pack::{
    ExpandEntry, ExpandSection, ExpandTarget, LexSection, LexTerm, MetaSection, OverlayAlternate,
    OverlayConcept, OverlayRange, OverlaySection, PackReader, PackWriter, PostSection, PostTerm,
    Posting, PvecRow, PvecSection, PvecSourceKind, SectionTag, TextSection,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

const EVENT_SIGNATURE_RESERVED_PREFIX: &str = "event-signature-";

#[derive(Parser)]
#[command(name = "recall-packer", about = "Build verse-recall .pack files")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Pack verses from a JSONL file (each line: {"ref":"...","text":"..."}).
    Pack {
        /// Input JSONL path
        #[arg(long, short)]
        input: PathBuf,
        /// Optional verse-aligned JSONL of auxiliary explanatory documents.
        /// Every canonical reference must appear once, in canonical order.
        #[arg(long)]
        context: Option<PathBuf>,
        /// Output .pack path
        #[arg(long, short)]
        output: PathBuf,
        /// Translation id written into META
        #[arg(long, default_value = "BSB")]
        translation: String,
    },
    /// Wrap a deterministic recall-overlay JSONL as a META+PHRASE pack.
    Overlay {
        /// JSONL produced by scripts/build_recall_overlay.py
        #[arg(long, short)]
        input: PathBuf,
        /// Output section-only .pack path
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Compile deterministic expansion JSONL into a META+EXPAND pack.
    Expand {
        /// JSONL manifest followed by weighted source→target records.
        #[arg(long, short)]
        input: PathBuf,
        /// Output section-only .pack path.
        #[arg(long, short)]
        output: PathBuf,
    },
    /// Build an optional META+LVEC semantic pack from row-major f32 vectors.
    Semantic {
        /// Normalized or unnormalized verse vectors as little-endian f32 rows.
        #[arg(long)]
        verse_vectors: PathBuf,
        /// Optional ±1-window vectors with the same shape.
        #[arg(long, conflicts_with = "study_note_vectors")]
        window_vectors: Option<PathBuf>,
        /// Optional verse-aligned Study Note vectors with the same shape.
        /// Verses without notes should repeat their canonical representation.
        #[arg(long, conflicts_with = "window_vectors")]
        study_note_vectors: Option<PathBuf>,
        /// Embedding dimensions per row.
        #[arg(long)]
        dims: u16,
        /// Output section-only .pack path.
        #[arg(long, short)]
        output: PathBuf,
        /// Canonical translation id targeted by the vectors.
        #[arg(long, default_value = "BSB")]
        translation: String,
        /// 64 lowercase/uppercase hex characters from `build_semantic_vectors.py space-id`.
        #[arg(long)]
        embedding_space_id: String,
    },
    /// Build an optional META+PVEC passage-semantic pack.
    PassageSemantic {
        /// Row-major passage vectors as little-endian f32 rows.
        #[arg(long)]
        passage_vectors: PathBuf,
        /// JSONL rows with anchor_index, start_index, end_index, and source_kind.
        #[arg(long)]
        ranges: PathBuf,
        /// Number of verses in the targeted canonical TEXT corpus.
        #[arg(long)]
        canonical_verse_count: u32,
        /// Embedding dimensions per passage row.
        #[arg(long)]
        dims: u16,
        /// Output section-only .pack path.
        #[arg(long, short)]
        output: PathBuf,
        /// Canonical translation id targeted by the ranges.
        #[arg(long, default_value = "BSB")]
        translation: String,
        /// 64 hex characters identifying the same encoder contract as LVEC.
        #[arg(long)]
        embedding_space_id: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Pack {
            input,
            context,
            output,
            translation,
        } => {
            if let Err(e) = pack_jsonl(&input, context.as_ref(), &output, &translation) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Cmd::Overlay { input, output } => {
            if let Err(e) = pack_overlay_jsonl(&input, &output) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Cmd::Expand { input, output } => {
            if let Err(e) = pack_expand_jsonl(&input, &output) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Cmd::Semantic {
            verse_vectors,
            window_vectors,
            study_note_vectors,
            dims,
            output,
            translation,
            embedding_space_id,
        } => {
            let embedding_space_id = match parse_space_id(&embedding_space_id) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("error: {error}");
                    std::process::exit(1);
                }
            };
            if let Err(e) = pack_semantic(
                &verse_vectors,
                window_vectors.as_ref(),
                study_note_vectors.as_ref(),
                dims,
                &output,
                &translation,
                embedding_space_id,
            ) {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
        Cmd::PassageSemantic {
            passage_vectors,
            ranges,
            canonical_verse_count,
            dims,
            output,
            translation,
            embedding_space_id,
        } => {
            let embedding_space_id = match parse_space_id(&embedding_space_id) {
                Ok(value) => value,
                Err(error) => {
                    eprintln!("error: {error}");
                    std::process::exit(1);
                }
            };
            if let Err(error) = pack_passage_semantic(
                &passage_vectors,
                &ranges,
                canonical_verse_count,
                dims,
                &output,
                &translation,
                embedding_space_id,
            ) {
                eprintln!("error: {error}");
                std::process::exit(1);
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct PassageRangeInput {
    anchor_index: u32,
    start_index: u32,
    end_index: u32,
    source_kind: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExpansionManifest {
    schema_version: u32,
    kind: String,
    canonical_translation: String,
    canonical_verse_count: u32,
    provenance: ExpansionProvenance,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExpansionProvenance {
    builder: String,
    normalizer_version: u16,
    canonical_sha256: String,
    embeddings_sha256: String,
    vocabulary_sha256: String,
    embedding_rows: usize,
    embedding_dimensions: usize,
    source_count: usize,
    edge_count: usize,
    parameters: ExpansionParameters,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExpansionParameters {
    top_k: usize,
    similarity_floor: f64,
    batch_size: usize,
    weight_gamma: f64,
    weight_calibration: String,
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn expand_provenance_path(output: &std::path::Path) -> PathBuf {
    output.with_extension("provenance.json")
}

fn pack_expand_jsonl(input: &PathBuf, output: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(input)?;
    let mut canonical_translation: Option<String> = None;
    let mut canonical_verse_count: Option<u32> = None;
    let mut canonical_manifest: Option<Vec<u8>> = None;
    let mut declared_source_count: Option<usize> = None;
    let mut declared_edge_count: Option<usize> = None;
    let mut mappings: BTreeMap<String, BTreeMap<String, f32>> = BTreeMap::new();
    let mut expansion_record_count = 0usize;
    let mut expansion_edge_count = 0usize;

    for (line_no, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", line_no + 1))?;
        if value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        {
            return Err(
                format!("line {}: unsupported expansion schema version", line_no + 1).into(),
            );
        }
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("line {}: missing kind", line_no + 1))?;
        if kind != "manifest" && canonical_translation.is_none() {
            return Err(format!("line {}: manifest must be the first record", line_no + 1).into());
        }
        match kind {
            "manifest" => {
                if canonical_translation.is_some() {
                    return Err(format!("line {}: duplicate manifest", line_no + 1).into());
                }
                let manifest: ExpansionManifest =
                    serde_json::from_value(value).map_err(|error| {
                        format!("line {}: invalid expansion manifest: {error}", line_no + 1)
                    })?;
                let translation = manifest.canonical_translation.trim();
                if manifest.schema_version != 1
                    || manifest.kind != "manifest"
                    || translation.is_empty()
                    || translation != manifest.canonical_translation
                    || manifest.canonical_verse_count == 0
                    || manifest.provenance.builder.trim().is_empty()
                    || manifest.provenance.normalizer_version != NORMALIZER_VERSION
                    || !is_lower_sha256(&manifest.provenance.canonical_sha256)
                    || !is_lower_sha256(&manifest.provenance.embeddings_sha256)
                    || !is_lower_sha256(&manifest.provenance.vocabulary_sha256)
                    || manifest.provenance.embedding_rows == 0
                    || manifest.provenance.embedding_dimensions == 0
                    || manifest.provenance.source_count == 0
                    || manifest.provenance.edge_count == 0
                    || manifest.provenance.parameters.top_k == 0
                    || manifest.provenance.parameters.top_k > 64
                    || !manifest.provenance.parameters.similarity_floor.is_finite()
                    || !(0.0..=1.0).contains(&manifest.provenance.parameters.similarity_floor)
                    || manifest.provenance.parameters.batch_size == 0
                    || !manifest.provenance.parameters.weight_gamma.is_finite()
                    || manifest.provenance.parameters.weight_gamma <= 0.0
                    || manifest
                        .provenance
                        .parameters
                        .weight_calibration
                        .trim()
                        .is_empty()
                {
                    return Err(format!(
                        "line {}: expansion manifest contains invalid provenance",
                        line_no + 1
                    )
                    .into());
                }
                let verse_count = manifest.canonical_verse_count;
                declared_source_count = Some(manifest.provenance.source_count);
                declared_edge_count = Some(manifest.provenance.edge_count);
                let mut encoded = serde_json::to_vec(&manifest)?;
                encoded.push(b'\n');
                canonical_manifest = Some(encoded);
                canonical_translation = Some(translation.to_string());
                canonical_verse_count = Some(verse_count);
            }
            "expansion" => {
                let source_surface = value
                    .get("source")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| format!("line {}: expansion missing source", line_no + 1))?;
                let source = normalize_expand_surface(source_surface)
                    .map_err(|error| format!("line {}: source {error}", line_no + 1))?;
                let targets = value
                    .get("targets")
                    .and_then(serde_json::Value::as_array)
                    .filter(|targets| !targets.is_empty())
                    .ok_or_else(|| {
                        format!("line {}: targets must be a non-empty array", line_no + 1)
                    })?;
                for target in targets {
                    let target_surface = target
                        .get("stem")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| {
                            format!("line {}: expansion target missing stem", line_no + 1)
                        })?;
                    let target_stem = normalize_expand_surface(target_surface)
                        .map_err(|error| format!("line {}: target {error}", line_no + 1))?;
                    if target_stem == source {
                        return Err(format!(
                            "line {}: source and target normalize to the same stem",
                            line_no + 1
                        )
                        .into());
                    }
                    let weight = target
                        .get("weight")
                        .and_then(serde_json::Value::as_f64)
                        .filter(|weight| weight.is_finite() && *weight > 0.0 && *weight <= 1.0)
                        .ok_or_else(|| {
                            format!("line {}: target weight must be in (0, 1]", line_no + 1)
                        })? as f32;
                    mappings
                        .entry(source.clone())
                        .or_default()
                        .entry(target_stem)
                        .and_modify(|current| *current = current.max(weight))
                        .or_insert(weight);
                    expansion_edge_count += 1;
                }
                expansion_record_count += 1;
            }
            _ => {
                return Err(
                    format!("line {}: unsupported expansion kind {kind}", line_no + 1).into(),
                );
            }
        }
    }

    let canonical_translation = canonical_translation.ok_or("expansion input missing manifest")?;
    let canonical_verse_count =
        canonical_verse_count.ok_or("expansion input missing canonical verse count")?;
    if expansion_record_count == 0 || mappings.is_empty() {
        return Err("expansion input contains no expansion records".into());
    }
    if declared_source_count != Some(expansion_record_count)
        || declared_edge_count != Some(expansion_edge_count)
    {
        return Err("expansion manifest source/edge counts do not match records".into());
    }
    let canonical_manifest = canonical_manifest.ok_or("expansion input missing provenance")?;
    let provenance_sha256: [u8; 32] = Sha256::digest(&canonical_manifest).into();
    let entries = mappings
        .into_iter()
        .map(|(source, targets)| ExpandEntry {
            source,
            targets: targets
                .into_iter()
                .map(|(stem, weight)| ExpandTarget { stem, weight })
                .collect(),
        })
        .collect();
    let expand = ExpandSection::new_with_provenance(
        canonical_translation.clone(),
        canonical_verse_count,
        provenance_sha256,
        entries,
    )?;
    let meta = MetaSection::new(&canonical_translation, canonical_verse_count);
    let bytes = PackWriter::new()
        .add_meta(&meta)
        .add_expand(&expand)
        .finish();

    // Validate the emitted target identity before touching the destination.
    let reader = PackReader::open(&bytes)?;
    let emitted_meta = reader.meta()?;
    let emitted_expand = reader.expand()?;
    if emitted_meta.translation_id != canonical_translation
        || emitted_meta.verse_count != canonical_verse_count
        || emitted_expand.canonical_translation()? != canonical_translation
        || emitted_expand.canonical_verse_count() != canonical_verse_count
        || emitted_expand.provenance_sha256() != Some(provenance_sha256)
    {
        return Err("emitted EXPAND pack failed canonical target validation".into());
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let provenance_output = expand_provenance_path(output);
    fs::write(&provenance_output, &canonical_manifest)?;
    fs::write(output, &bytes)?;
    eprintln!(
        "wrote content-addressed EXPAND pack ({} source records, {} canonical entries, {} bytes) → {} (provenance {})",
        expansion_record_count,
        expand.entries().len(),
        bytes.len(),
        output.display(),
        provenance_output.display()
    );
    Ok(())
}

/// Build JSON may use vocabulary surfaces; the packed section stores exactly
/// the same one-token lowercase stems produced for indexed/query terms.
fn normalize_expand_surface(value: &str) -> Result<String, &'static str> {
    let tokens = tokenize_text(value.trim());
    if tokens.len() != 1 {
        return Err("must contain exactly one vocabulary token");
    }
    let normalized = stem(&tokens[0]);
    if normalized.len() <= 1 {
        return Err("normalizes to an empty or one-character stem");
    }
    Ok(normalized)
}

fn pack_semantic(
    verse_vectors: &PathBuf,
    window_vectors: Option<&PathBuf>,
    study_note_vectors: Option<&PathBuf>,
    dims: u16,
    output: &PathBuf,
    translation: &str,
    embedding_space_id: [u8; 32],
) -> Result<(), Box<dyn std::error::Error>> {
    if dims == 0 || translation.trim().is_empty() {
        return Err("semantic dims and translation must be non-empty".into());
    }
    let verse_bytes = fs::read(verse_vectors)?;
    let row_bytes = usize::from(dims)
        .checked_mul(4)
        .ok_or("semantic row size overflow")?;
    if verse_bytes.is_empty() || verse_bytes.len() % row_bytes != 0 {
        return Err("verse vector file is not an exact number of f32 rows".into());
    }
    let verse_count = verse_bytes.len() / row_bytes;
    let verse_count_u32 = u32::try_from(verse_count)?;
    if window_vectors.is_some() && study_note_vectors.is_some() {
        return Err("window and Study Note vectors are mutually exclusive".into());
    }
    let mut sources = vec![verse_bytes];
    let mut auxiliary_role = None;
    if let Some(path) = window_vectors {
        let bytes = fs::read(path)?;
        if bytes.len() != sources[0].len() {
            return Err("window vectors must have the same shape as verse vectors".into());
        }
        sources.push(bytes);
        auxiliary_role = Some(DenseTableRole::Window);
    }
    if let Some(path) = study_note_vectors {
        let bytes = fs::read(path)?;
        if bytes.len() != sources[0].len() {
            return Err("Study Note vectors must have the same shape as verse vectors".into());
        }
        sources.push(bytes);
        auxiliary_role = Some(DenseTableRole::StudyNote);
    }

    let mut tables = Vec::with_capacity(sources.len());
    for source in &sources {
        let mut table = Vec::with_capacity(verse_count * usize::from(dims));
        for row in source.chunks_exact(row_bytes) {
            let values: Vec<f32> = row
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes(bytes.try_into().expect("fixed slice")))
                .collect();
            if values.iter().any(|value| !value.is_finite()) {
                return Err("semantic vectors must contain only finite f32 values".into());
            }
            let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
            if norm <= f32::EPSILON {
                return Err("semantic vectors must have non-zero norm".into());
            }
            table.extend(
                values
                    .into_iter()
                    .map(|value| ((value / norm) * 127.0).round().clamp(-127.0, 127.0) as i8),
            );
        }
        tables.push(table);
    }
    let table_refs: Vec<&[i8]> = tables.iter().map(Vec::as_slice).collect();
    // Keep established verse-only and verse+window artifacts byte-compatible
    // with LVEC v2. Study Note artifacts require v3's explicit role byte so
    // the runtime cannot invent window provenance for table 2.
    let body = match auxiliary_role {
        Some(DenseTableRole::StudyNote) => DenseLane::encode_v3(
            usize::from(dims),
            verse_count,
            &table_refs,
            &[DenseTableRole::Verse, DenseTableRole::StudyNote],
            embedding_space_id,
        )?,
        _ => DenseLane::encode_v2(
            usize::from(dims),
            verse_count,
            &table_refs,
            embedding_space_id,
        )?,
    };
    let meta = MetaSection::new(translation.trim(), verse_count_u32);
    let bytes = PackWriter::new()
        .add_meta(&meta)
        .add_raw(SectionTag::Lvec, body)
        .finish();
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, &bytes)?;
    eprintln!(
        "wrote semantic pack ({} verses, {} dims, {} tables, {} bytes) → {}",
        verse_count,
        dims,
        sources.len(),
        bytes.len(),
        output.display()
    );
    Ok(())
}

fn pack_passage_semantic(
    passage_vectors: &PathBuf,
    ranges: &PathBuf,
    canonical_verse_count: u32,
    dims: u16,
    output: &PathBuf,
    translation: &str,
    embedding_space_id: [u8; 32],
) -> Result<(), Box<dyn std::error::Error>> {
    let translation = translation.trim();
    if dims == 0 || canonical_verse_count == 0 || translation.is_empty() {
        return Err(
            "passage semantic dims, canonical verse count, and translation must be non-empty"
                .into(),
        );
    }

    let raw_ranges = fs::read_to_string(ranges)?;
    let mut rows = Vec::new();
    for (line_number, line) in raw_ranges.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let input: PassageRangeInput = serde_json::from_str(line)
            .map_err(|error| format!("range line {}: {error}", line_number + 1))?;
        let source_kind = match input.source_kind.as_str() {
            "canonical-passage" => PvecSourceKind::CanonicalPassage,
            "study-bible-passage" => PvecSourceKind::StudyBiblePassage,
            other => {
                return Err(format!(
                    "range line {}: unsupported source_kind {other:?}",
                    line_number + 1
                )
                .into());
            }
        };
        rows.push(PvecRow {
            anchor_index: input.anchor_index,
            start_index: input.start_index,
            end_index: input.end_index,
            source_kind,
        });
    }
    if rows.is_empty() {
        return Err("passage range input is empty".into());
    }

    let bytes = fs::read(passage_vectors)?;
    let row_bytes = usize::from(dims)
        .checked_mul(4)
        .ok_or("passage semantic row size overflow")?;
    let expected_bytes = rows
        .len()
        .checked_mul(row_bytes)
        .ok_or("passage semantic vector size overflow")?;
    if bytes.len() != expected_bytes {
        return Err(format!(
            "passage vector file has {} bytes; {} range rows at {} dimensions require {}",
            bytes.len(),
            rows.len(),
            dims,
            expected_bytes
        )
        .into());
    }

    let mut vectors = Vec::with_capacity(rows.len() * usize::from(dims));
    for (row_index, raw_row) in bytes.chunks_exact(row_bytes).enumerate() {
        let values: Vec<f32> = raw_row
            .chunks_exact(4)
            .map(|value| f32::from_le_bytes(value.try_into().expect("fixed slice")))
            .collect();
        if values.iter().any(|value| !value.is_finite()) {
            return Err(
                format!("passage vector row {row_index} contains a non-finite value").into(),
            );
        }
        let norm = values.iter().map(|value| value * value).sum::<f32>().sqrt();
        if norm <= f32::EPSILON {
            return Err(format!("passage vector row {row_index} has zero norm").into());
        }
        vectors.extend(
            values
                .into_iter()
                .map(|value| ((value / norm) * 127.0).round().clamp(-127.0, 127.0) as i8),
        );
    }

    let pvec = PvecSection::new(
        dims,
        canonical_verse_count,
        embedding_space_id,
        rows,
        vectors,
    )?;
    let meta = MetaSection::new(translation, canonical_verse_count);
    let bytes = PackWriter::new().add_meta(&meta).add_pvec(&pvec).finish();
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, &bytes)?;
    eprintln!(
        "wrote passage semantic pack ({} passages, {} canonical verses, {} dims, {} bytes) → {}",
        pvec.rows.len(),
        canonical_verse_count,
        dims,
        bytes.len(),
        output.display()
    );
    Ok(())
}

fn parse_space_id(value: &str) -> Result<[u8; 32], String> {
    let value = value.trim();
    if value.len() != 64 {
        return Err("embedding-space-id must contain exactly 64 hex characters".into());
    }
    let mut output = [0u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(pair)
            .map_err(|_| "embedding-space-id contains non-ASCII characters".to_string())?;
        output[index] = u8::from_str_radix(text, 16)
            .map_err(|_| "embedding-space-id contains non-hex characters".to_string())?;
    }
    if output.iter().all(|byte| *byte == 0) {
        return Err("embedding-space-id must not be all zero".into());
    }
    Ok(output)
}

fn pack_overlay_jsonl(input: &PathBuf, output: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(input)?;
    let mut canonical_translation: Option<String> = None;
    let mut canonical_verse_count: Option<u32> = None;
    let mut manifest_value: Option<serde_json::Value> = None;
    let mut actual_concepts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut actual_event_signatures: BTreeMap<String, usize> = BTreeMap::new();
    let mut actual_alternates: BTreeMap<String, usize> = BTreeMap::new();
    let mut concepts = Vec::new();
    let mut alternates = Vec::new();
    let mut record_count = 0usize;

    for (line_no, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let value: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", line_no + 1))?;
        if value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(1)
        {
            return Err(format!("line {}: unsupported overlay schema version", line_no + 1).into());
        }
        let kind = value
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("line {}: missing kind", line_no + 1))?;
        if !matches!(kind, "manifest" | "concept" | "alternate_text") {
            return Err(format!("line {}: unsupported kind {kind}", line_no + 1).into());
        }
        if kind != "manifest" && canonical_translation.is_none() {
            return Err(format!("line {}: manifest must be the first record", line_no + 1).into());
        }
        if kind == "manifest" {
            if canonical_translation.is_some() {
                return Err(format!("line {}: duplicate manifest", line_no + 1).into());
            }
            let translation = value
                .get("canonical_translation")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    format!(
                        "line {}: manifest missing canonical_translation",
                        line_no + 1
                    )
                })?
                .trim();
            if translation.is_empty() {
                return Err(format!(
                    "line {}: canonical_translation must not be empty",
                    line_no + 1
                )
                .into());
            }
            canonical_translation = Some(translation.to_string());
            canonical_verse_count = Some(
                value
                    .get("canonical_verse_count")
                    .and_then(|v| v.as_u64())
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| {
                        format!(
                            "line {}: manifest missing canonical_verse_count",
                            line_no + 1
                        )
                    })?,
            );
            if canonical_verse_count == Some(0) {
                return Err(format!(
                    "line {}: canonical_verse_count must be positive",
                    line_no + 1
                )
                .into());
            }
            manifest_value = Some(value.clone());
        } else if kind == "concept" {
            let parsed: JsonConcept = serde_json::from_value(value.clone())
                .map_err(|error| format!("line {}: invalid concept: {error}", line_no + 1))?;
            let counts = actual_concepts
                .entry(parsed.concept_type.clone())
                .or_default();
            counts.0 += 1;
            counts.1 += parsed.aliases.len();
            *actual_event_signatures
                .entry(parsed.concept_type.clone())
                .or_default() += parsed
                .aliases
                .iter()
                .filter(|alias| {
                    alias
                        .to_ascii_lowercase()
                        .starts_with(EVENT_SIGNATURE_RESERVED_PREFIX)
                })
                .count();
            concepts.push(parsed.into_binary());
        } else if kind == "alternate_text" {
            let parsed: JsonAlternate = serde_json::from_value(value.clone()).map_err(|error| {
                format!("line {}: invalid alternate text: {error}", line_no + 1)
            })?;
            *actual_alternates
                .entry(parsed.source_translation.clone())
                .or_default() += 1;
            alternates.push(parsed.into_binary());
        }
        record_count += 1;
    }

    let canonical_translation = canonical_translation.ok_or("overlay missing manifest")?;
    let canonical_verse_count = canonical_verse_count.ok_or("overlay missing verse count")?;
    validate_overlay_manifest_counts(
        manifest_value.as_ref().ok_or("overlay missing manifest")?,
        &actual_concepts,
        &actual_event_signatures,
        &actual_alternates,
    )?;
    if record_count <= 1 {
        return Err("overlay contains no recall records".into());
    }
    let body = OverlaySection {
        canonical_translation: canonical_translation.clone(),
        canonical_verse_count,
        concepts,
        alternates,
    }
    .encode()?;
    // Run the runtime's typed overlay validation before wrapping the binary
    // body. This rejects malformed/unknown reserved markers, untrusted marker
    // sources, and cross-record signature collisions at compile time instead
    // of producing a pack that no host can open.
    RecallOverlay::parse(
        &[body.as_slice()],
        &canonical_translation,
        canonical_verse_count as usize,
    )?;

    let meta = MetaSection::new(
        format!("OVERLAY:{canonical_translation}"),
        canonical_verse_count,
    );
    let bytes = PackWriter::new()
        .add_meta(&meta)
        .add_raw(SectionTag::Phrase, body)
        .finish();
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, &bytes)?;
    eprintln!(
        "wrote recall overlay ({} records, {} bytes) → {}",
        record_count,
        bytes.len(),
        output.display()
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
struct JsonRange {
    start_ref: String,
    end_ref: String,
}

#[derive(Debug, Deserialize)]
struct JsonConcept {
    concept_type: String,
    key: String,
    label: String,
    #[serde(default)]
    aliases: Vec<String>,
    source: String,
    anchor_ref: String,
    target_ranges: Vec<JsonRange>,
}

impl JsonConcept {
    fn into_binary(self) -> OverlayConcept {
        OverlayConcept {
            concept_type: self.concept_type,
            key: self.key,
            label: self.label,
            aliases: self.aliases,
            source: self.source,
            anchor_ref: self.anchor_ref,
            target_ranges: self
                .target_ranges
                .into_iter()
                .map(|range| OverlayRange {
                    start_ref: range.start_ref,
                    end_ref: range.end_ref,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct JsonAlternate {
    source_translation: String,
    target_ref: String,
    text: String,
}

impl JsonAlternate {
    fn into_binary(self) -> OverlayAlternate {
        OverlayAlternate {
            source_translation: self.source_translation,
            target_ref: self.target_ref,
            text: self.text,
        }
    }
}

fn validate_overlay_manifest_counts(
    manifest: &serde_json::Value,
    actual_concepts: &BTreeMap<String, (usize, usize)>,
    actual_event_signatures: &BTreeMap<String, usize>,
    actual_alternates: &BTreeMap<String, usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let concept_sources = manifest
        .get("concept_sources")
        .and_then(serde_json::Value::as_array)
        .ok_or("manifest concept_sources must be an array")?;
    let mut expected_concepts = BTreeMap::new();
    let mut expected_event_signatures = BTreeMap::new();
    for source in concept_sources {
        let concept_type = source
            .get("concept_type")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("manifest concept source missing concept_type")?;
        let record_count = source
            .get("record_count")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("manifest concept source missing record_count")?;
        let alias_count = source
            .get("alias_count")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok());
        let generated_alias_count = source
            .get("generated_alias_count")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok());
        let aliases = match (alias_count, generated_alias_count) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "manifest concept source {concept_type} declares two alias counts"
                )
                .into());
            }
            (Some(count), None) | (None, Some(count)) => count,
            (None, None) => 0,
        };
        if expected_concepts
            .insert(concept_type.to_string(), (record_count, aliases))
            .is_some()
        {
            return Err(format!("manifest repeats concept type {concept_type}").into());
        }
        let generated_signature_count = source
            .get("generated_signature_count")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        if generated_signature_count > 0 && concept_type != "event" {
            return Err(format!(
                "manifest non-event concept source {concept_type} declares generated event signatures"
            )
            .into());
        }
        let signature_normalizer_version = source
            .get("signature_normalizer_version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| u16::try_from(value).ok());
        if source.get("signature_normalizer_version").is_some()
            && signature_normalizer_version != Some(NORMALIZER_VERSION)
            || generated_signature_count > 0
                && signature_normalizer_version != Some(NORMALIZER_VERSION)
        {
            return Err(
                "manifest event signatures use an incompatible or missing normalizer version"
                    .into(),
            );
        }
        expected_event_signatures.insert(concept_type.to_string(), generated_signature_count);
    }
    if &expected_concepts != actual_concepts {
        return Err(format!(
            "concept counts do not match manifest: expected {expected_concepts:?}, got {actual_concepts:?}"
        )
        .into());
    }
    expected_event_signatures.retain(|_, count| *count > 0);
    let mut actual_event_signatures = actual_event_signatures.clone();
    actual_event_signatures.retain(|_, count| *count > 0);
    if expected_event_signatures != actual_event_signatures {
        return Err(format!(
            "generated event signature counts do not match manifest: expected {expected_event_signatures:?}, got {actual_event_signatures:?}"
        )
        .into());
    }

    let alternate_sources = manifest
        .get("alternate_sources")
        .and_then(serde_json::Value::as_array)
        .ok_or("manifest alternate_sources must be an array")?;
    let mut expected_alternates = BTreeMap::new();
    for source in alternate_sources {
        let translation = source
            .get("translation")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("manifest alternate source missing translation")?;
        let matched_verse_count = source
            .get("matched_verse_count")
            .and_then(serde_json::Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("manifest alternate source missing matched_verse_count")?;
        if expected_alternates
            .insert(translation.to_string(), matched_verse_count)
            .is_some()
        {
            return Err(format!("manifest repeats alternate translation {translation}").into());
        }
    }
    if &expected_alternates != actual_alternates {
        return Err(format!(
            "alternate counts do not match manifest: expected {expected_alternates:?}, got {actual_alternates:?}"
        )
        .into());
    }
    Ok(())
}

fn pack_jsonl(
    input: &PathBuf,
    context: Option<&PathBuf>,
    output: &PathBuf,
    translation: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let translation = translation.trim();
    if translation.is_empty() {
        return Err("translation id must not be empty".into());
    }
    let raw = fs::read_to_string(input)?;
    let mut verses = Vec::new();
    let mut seen_refs = HashSet::new();
    for (line_no, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", line_no + 1))?;
        let text = v
            .get("text")
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("line {}: missing text", line_no + 1))?
            .trim();
        if text.is_empty() {
            return Err(format!("line {}: text must not be empty", line_no + 1).into());
        }
        // Prefer display refs from book+chapter+verseNum (Arweave / selah-tools BSB).
        // Fall back to "ref" (already display or OSIS).
        let reference = display_ref_from_row(&v)
            .ok_or_else(|| format!("line {}: missing ref/book+chapter+verseNum", line_no + 1))?;
        let normalized_ref = normalized_ref_key(&reference);
        if normalized_ref.is_empty() || !seen_refs.insert(normalized_ref) {
            return Err(format!(
                "line {}: duplicate or empty normalized ref {}",
                line_no + 1,
                reference
            )
            .into());
        }
        verses.push((reference, text.to_string()));
    }
    if verses.is_empty() {
        return Err("no verses in input".into());
    }

    let text = TextSection::from_verses(verses);
    let meta = MetaSection::new(translation, text.verses.len() as u32);
    let (lex, post) = build_lex_post(&text)?;
    let context_index = context
        .map(|path| load_aligned_context_jsonl(path, &text))
        .transpose()?
        .map(|context_text| build_lex_post(&context_text))
        .transpose()?;
    let mut writer = PackWriter::new();
    writer
        .add_meta(&meta)
        .add_text(&text)
        .add_lex(&lex)
        .add_post(&post);
    if let Some((context_lex, context_post)) = &context_index {
        writer
            .add_context_lex(context_lex)
            .add_context_post(context_post);
    }
    let bytes = writer.finish();
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, &bytes)?;
    eprintln!(
        "wrote {} ({} verses, context={}, {} bytes) → {}",
        translation,
        text.verses.len(),
        context_index.is_some(),
        bytes.len(),
        output.display()
    );
    Ok(())
}

/// Parse a complete auxiliary corpus and bind each row to its canonical
/// document id. The sparse sections do not carry references at runtime, so
/// the packer must reject missing, extra, reordered, or mismatched rows.
fn load_aligned_context_jsonl(
    input: &PathBuf,
    canonical: &TextSection,
) -> Result<TextSection, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(input)?;
    let mut documents = Vec::with_capacity(canonical.verses.len());
    for (line_no, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|error| format!("context line {}: {error}", line_no + 1))?;
        let row_index = documents.len();
        let expected = canonical.verses.get(row_index).ok_or_else(|| {
            format!(
                "context line {}: extra row; canonical corpus contains {} verses",
                line_no + 1,
                canonical.verses.len()
            )
        })?;
        let reference = display_ref_from_row(&value)
            .ok_or_else(|| format!("context line {}: missing ref", line_no + 1))?;
        if normalized_ref_key(&reference) != normalized_ref_key(&expected.reference) {
            return Err(format!(
                "context line {}: expected aligned ref {}, got {}",
                line_no + 1,
                expected.reference,
                reference
            )
            .into());
        }
        let supplied = value
            .get("study_context")
            .or_else(|| value.get("text"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| format!("context line {}: missing study_context/text", line_no + 1))?
            .trim();
        // The standard semantic-document builder prefixes every row with the
        // canonical verse. Exclude that exact prefix so context cannot cast a
        // second vote for wording already indexed by LEX/POST.
        let auxiliary = supplied
            .strip_prefix(expected.text.trim())
            .unwrap_or(supplied)
            .trim();
        // An aligned empty row is intentional: not every canonical verse has
        // source-authored study prose. POST retains the canonical document id
        // with length one while emitting no searchable postings for this row.
        documents.push((expected.reference.clone(), auxiliary.to_string()));
    }
    if documents.len() != canonical.verses.len() {
        return Err(format!(
            "context corpus has {} aligned rows; expected {}",
            documents.len(),
            canonical.verses.len()
        )
        .into());
    }
    Ok(TextSection::from_verses(documents))
}

fn build_lex_post(
    text: &TextSection,
) -> Result<(LexSection, PostSection), Box<dyn std::error::Error>> {
    let mut by_term: BTreeMap<String, Vec<Posting>> = BTreeMap::new();
    let mut document_lengths = Vec::with_capacity(text.verses.len());
    for verse in &text.verses {
        let tokens = tokenize_text(&verse.text);
        let document_length = u16::try_from(tokens.len().max(1))
            .map_err(|_| format!("{} exceeds the u16 token-length limit", verse.reference))?;
        document_lengths.push(document_length);
        let mut frequencies: HashMap<String, u16> = HashMap::new();
        for token in tokens {
            let term = stem(&token);
            if term.len() <= 1 {
                continue;
            }
            let frequency = frequencies.entry(term).or_default();
            *frequency = frequency.saturating_add(1);
        }
        for (term, frequency) in frequencies {
            by_term.entry(term).or_default().push(Posting {
                document_id: verse.index,
                term_frequency: u8::try_from(frequency.min(u16::from(u8::MAX)))
                    .expect("frequency was capped"),
            });
        }
    }
    if by_term.is_empty() {
        return Err("cannot build an empty lexical index".into());
    }
    let mut lex_terms = Vec::with_capacity(by_term.len());
    let mut post_terms = Vec::with_capacity(by_term.len());
    for (term, postings) in by_term {
        lex_terms.push(LexTerm {
            text: term,
            document_frequency: postings.len() as u32,
        });
        post_terms.push(PostTerm { postings });
    }
    Ok((
        LexSection::new(text.verses.len() as u32, lex_terms)?,
        PostSection::new(document_lengths, post_terms)?,
    ))
}

fn normalized_ref_key(reference: &str) -> String {
    let mut tokens = Vec::new();
    let mut token = String::new();
    for character in reference.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() {
            token.push(character);
        } else if !token.is_empty() {
            tokens.push(std::mem::take(&mut token));
        }
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    tokens.join(" ")
}

/// Build "Genesis 1:1" style refs from Arweave JSONL or pass through display refs.
fn display_ref_from_row(v: &serde_json::Value) -> Option<String> {
    let book = v.get("book").and_then(|x| x.as_str()).unwrap_or("").trim();
    let chapter = v
        .get("chapter")
        .and_then(|x| {
            x.as_str()
                .map(|s| s.to_string())
                .or_else(|| x.as_u64().map(|n| n.to_string()))
        })
        .unwrap_or_default();
    let verse = v
        .get("verseNum")
        .or_else(|| v.get("verse"))
        .and_then(|x| {
            x.as_str()
                .map(|s| s.to_string())
                .or_else(|| x.as_u64().map(|n| n.to_string()))
        })
        .unwrap_or_default();
    if !book.is_empty() && !chapter.is_empty() && !verse.is_empty() {
        let book = if book == "Psalms" { "Psalm" } else { book };
        return Some(format!("{book} {chapter}:{verse}"));
    }
    let r = v.get("ref").and_then(|x| x.as_str())?.trim();
    if r.is_empty() {
        None
    } else {
        Some(r.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        pack_expand_jsonl, pack_jsonl, pack_overlay_jsonl, pack_passage_semantic, pack_semantic,
        parse_space_id,
    };
    use recall_core::{stem, DenseLane, DenseTableRole};
    use recall_pack::{is_binary_overlay, OverlaySection, PackReader, SectionTag};
    use sha2::Digest;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_paths(label: &str) -> (PathBuf, PathBuf) {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "verse-recall-packer-{label}-{}-{nonce}",
            std::process::id()
        ));
        (root.with_extension("jsonl"), root.with_extension("pack"))
    }

    fn expansion_manifest(verse_count: u32, source_count: usize, edge_count: usize) -> String {
        format!(
            concat!(
                "{{\"schema_version\":1,\"kind\":\"manifest\",",
                "\"canonical_translation\":\"BSB\",\"canonical_verse_count\":{verse_count},",
                "\"provenance\":{{\"builder\":\"test-builder-v1\",",
                "\"normalizer_version\":{normalizer_version},",
                "\"canonical_sha256\":\"{canonical_hash}\",",
                "\"embeddings_sha256\":\"{embeddings_hash}\",",
                "\"vocabulary_sha256\":\"{vocabulary_hash}\",\"embedding_rows\":3,",
                "\"embedding_dimensions\":2,\"source_count\":{source_count},",
                "\"edge_count\":{edge_count},\"parameters\":{{\"top_k\":2,",
                "\"similarity_floor\":0.64,\"batch_size\":1,\"weight_gamma\":4.0,",
                "\"weight_calibration\":\"positive-cosine-power-v1\"}}}}}}\n"
            ),
            verse_count = verse_count,
            source_count = source_count,
            edge_count = edge_count,
            normalizer_version = super::NORMALIZER_VERSION,
            canonical_hash = "1".repeat(64),
            embeddings_hash = "2".repeat(64),
            vocabulary_hash = "3".repeat(64),
        )
    }

    fn f32_rows(rows: &[[f32; 2]]) -> Vec<u8> {
        rows.iter()
            .flat_map(|row| row.iter())
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn semantic_packer_uses_explicit_study_note_role_and_keeps_window_v2_compatible() {
        let (verse_vectors, study_pack) = temp_paths("semantic-study-role");
        let study_vectors = verse_vectors.with_extension("study.f32");
        let window_pack = verse_vectors.with_extension("window.pack");
        fs::write(&verse_vectors, f32_rows(&[[1.0, 0.0], [0.0, 1.0]])).unwrap();
        fs::write(&study_vectors, f32_rows(&[[0.0, 1.0], [1.0, 0.0]])).unwrap();
        let space_id = [0x42; 32];

        pack_semantic(
            &verse_vectors,
            None,
            Some(&study_vectors),
            2,
            &study_pack,
            "BSB",
            space_id,
        )
        .unwrap();
        let study_bytes = fs::read(&study_pack).unwrap();
        let study_reader = PackReader::open(&study_bytes).unwrap();
        let study_lane =
            DenseLane::parse(study_reader.section_bytes(SectionTag::Lvec).unwrap(), 2).unwrap();
        assert_eq!(study_lane.metadata().version, 3);
        assert_eq!(
            study_lane.metadata().table_roles[..2],
            [DenseTableRole::Verse, DenseTableRole::StudyNote]
        );

        pack_semantic(
            &verse_vectors,
            Some(&study_vectors),
            None,
            2,
            &window_pack,
            "BSB",
            space_id,
        )
        .unwrap();
        let window_bytes = fs::read(&window_pack).unwrap();
        let window_reader = PackReader::open(&window_bytes).unwrap();
        let window_lane =
            DenseLane::parse(window_reader.section_bytes(SectionTag::Lvec).unwrap(), 2).unwrap();
        assert_eq!(window_lane.metadata().version, 2);
        assert_eq!(
            window_lane.metadata().table_roles[1],
            DenseTableRole::Window
        );

        for path in [verse_vectors, study_vectors, study_pack, window_pack] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn passage_semantic_packer_round_trips_explicit_ranges_and_vectors() {
        let (ranges, output) = temp_paths("passage-semantic");
        let vectors = ranges.with_extension("f32");
        fs::write(
            &ranges,
            concat!(
                "{\"id\":17,\"anchor_ref\":\"Genesis 1:2\",",
                "\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:3\",",
                "\"anchor_index\":1,\"start_index\":0,\"end_index\":2,",
                "\"source_kind\":\"study-bible-passage\",",
                "\"section_type\":\"paragraph\",\"text\":\"auditable source text\"}\n",
                "{\"anchor_index\":4,\"start_index\":3,\"end_index\":5,",
                "\"source_kind\":\"canonical-passage\"}\n"
            ),
        )
        .unwrap();
        fs::write(&vectors, f32_rows(&[[3.0, 4.0], [0.0, 2.0]])).unwrap();

        pack_passage_semantic(&vectors, &ranges, 6, 2, &output, "BSB", [0x66; 32]).unwrap();
        let bytes = fs::read(&output).unwrap();
        let reader = PackReader::open(&bytes).unwrap();
        let meta = reader.meta().unwrap();
        let pvec = reader.pvec().unwrap();
        assert_eq!(meta.translation_id, "BSB");
        assert_eq!(meta.verse_count, 6);
        assert_eq!(pvec.dims, 2);
        assert_eq!(pvec.rows.len(), 2);
        assert_eq!(pvec.rows[0].start_index, 0);
        assert_eq!(pvec.rows[0].end_index, 2);
        assert_eq!(pvec.vectors, [76, 102, 0, 127]);

        for path in [ranges, vectors, output] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn passage_semantic_packer_rejects_shape_values_and_unsorted_ranges() {
        let (ranges, output) = temp_paths("passage-semantic-invalid");
        let vectors = ranges.with_extension("f32");
        fs::write(
            &ranges,
            concat!(
                "{\"anchor_index\":4,\"start_index\":3,\"end_index\":5,",
                "\"source_kind\":\"study-bible-passage\"}\n",
                "{\"anchor_index\":1,\"start_index\":0,\"end_index\":2,",
                "\"source_kind\":\"study-bible-passage\"}\n"
            ),
        )
        .unwrap();
        fs::write(&vectors, f32_rows(&[[1.0, 0.0], [0.0, 1.0]])).unwrap();
        let unsorted = pack_passage_semantic(&vectors, &ranges, 6, 2, &output, "BSB", [0x66; 32])
            .expect_err("unsorted ranges must fail closed");
        assert!(unsorted.to_string().contains("invalid PVEC"));

        fs::write(
            &ranges,
            "{\"anchor_index\":1,\"start_index\":0,\"end_index\":2,\"source_kind\":\"study-bible-passage\"}\n",
        )
        .unwrap();
        let shape = pack_passage_semantic(&vectors, &ranges, 6, 2, &output, "BSB", [0x66; 32])
            .expect_err("vector/range count mismatch must fail closed");
        assert!(shape.to_string().contains("range rows"));

        fs::write(&vectors, f32_rows(&[[0.0, 0.0]])).unwrap();
        let zero = pack_passage_semantic(&vectors, &ranges, 6, 2, &output, "BSB", [0x66; 32])
            .expect_err("zero vectors must fail closed");
        assert!(zero.to_string().contains("zero norm"));

        fs::write(&vectors, f32_rows(&[[f32::NAN, 1.0]])).unwrap();
        let non_finite = pack_passage_semantic(&vectors, &ranges, 6, 2, &output, "BSB", [0x66; 32])
            .expect_err("non-finite vectors must fail closed");
        assert!(non_finite.to_string().contains("non-finite"));

        for path in [ranges, vectors] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn base_packer_rejects_duplicate_refs_and_empty_translation() {
        let (input, output) = temp_paths("validation");
        fs::write(
            &input,
            "{\"ref\":\"Genesis 1:1\",\"text\":\"One\"}\n{\"ref\":\"Genesis 1.1\",\"text\":\"Duplicate\"}\n",
        )
        .expect("write fixture");

        let duplicate = pack_jsonl(&input, None, &output, "BSB").expect_err("duplicate must fail");
        assert!(duplicate
            .to_string()
            .contains("duplicate or empty normalized ref"));
        let empty_translation =
            pack_jsonl(&input, None, &output, "  ").expect_err("empty translation must fail");
        assert!(empty_translation
            .to_string()
            .contains("translation id must not be empty"));
        assert!(!output.exists());

        fs::remove_file(input).expect("remove fixture");
    }

    #[test]
    fn base_packer_adds_only_aligned_auxiliary_context() {
        let (input, output) = temp_paths("context-valid");
        let context = input.with_extension("context.jsonl");
        fs::write(
            &input,
            concat!(
                "{\"ref\":\"Genesis 1:1\",\"text\":\"Canonical alpha wording.\"}\n",
                "{\"ref\":\"Genesis 1:2\",\"text\":\"Canonical beta wording.\"}\n"
            ),
        )
        .unwrap();
        fs::write(
            &context,
            concat!(
                "{\"ref\":\"Genesis 1:1\",\"text\":\"Canonical alpha wording. Passage context: duplicate sparse wording.\",\"study_context\":\"Study explanation: Incarnation motif.\"}\n",
                "{\"ref\":\"Genesis 1:2\",\"text\":\"Canonical beta wording. Passage context: Cosmology frame.\",\"study_context\":\"\"}\n"
            ),
        )
        .unwrap();

        pack_jsonl(&input, Some(&context), &output, "BSB").unwrap();
        let bytes = fs::read(&output).unwrap();
        let reader = PackReader::open(&bytes).unwrap();
        assert!(reader.has(SectionTag::Clex));
        assert!(reader.has(SectionTag::Cpost));
        assert_eq!(
            reader.text().unwrap().verses[0].text,
            "Canonical alpha wording."
        );
        let context_lex = reader.context_lex().unwrap();
        assert!(context_lex.lookup(&stem("incarnation")).is_some());
        assert_eq!(context_lex.lookup(&stem("canonical")), None);

        for path in [input, context, output] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn base_packer_rejects_reordered_or_incomplete_context_refs() {
        let (input, output) = temp_paths("context-alignment");
        let context = input.with_extension("context.jsonl");
        fs::write(
            &input,
            concat!(
                "{\"ref\":\"Genesis 1:1\",\"text\":\"First canonical.\"}\n",
                "{\"ref\":\"Genesis 1:2\",\"text\":\"Second canonical.\"}\n"
            ),
        )
        .unwrap();
        fs::write(
            &context,
            "{\"ref\":\"Genesis 1:2\",\"text\":\"Unaligned context prose.\"}\n",
        )
        .unwrap();

        let error = pack_jsonl(&input, Some(&context), &output, "BSB")
            .expect_err("reordered context must fail closed");
        assert!(error
            .to_string()
            .contains("expected aligned ref Genesis 1:1"));
        assert!(!output.exists());

        for path in [input, context] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn overlay_packer_rejects_manifest_count_mismatch() {
        let (input, output) = temp_paths("overlay-counts");
        fs::write(
            &input,
            concat!(
                "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
                "\"concept_sources\":[{\"concept_type\":\"event\",\"record_count\":2}],\"alternate_sources\":[]}\n",
                "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"one\",",
                "\"label\":\"One\",\"aliases\":[],\"source\":\"test\",\"anchor_ref\":\"Genesis 1:1\",",
                "\"target_ranges\":[{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:2\"}]}\n"
            ),
        )
        .expect("write fixture");

        let error = pack_overlay_jsonl(&input, &output)
            .expect_err("truncated overlay declaration must fail");
        assert!(error
            .to_string()
            .contains("concept counts do not match manifest"));
        assert!(!output.exists());

        fs::remove_file(input).expect("remove fixture");
    }

    #[test]
    fn overlay_packer_rejects_generated_event_signature_count_mismatch() {
        let (input, output) = temp_paths("overlay-signature-counts");
        fs::write(
            &input,
            concat!(
                "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
                "\"concept_sources\":[{\"concept_type\":\"event\",\"record_count\":1,\"alias_count\":1,\"generated_signature_count\":2,\"signature_normalizer_version\":2}],\"alternate_sources\":[]}\n",
                "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"synthetic event\",",
                "\"label\":\"Synthetic Event\",\"aliases\":[\"event-signature-v1:full:cobalt lantern repair\"],",
                "\"source\":\"BSB.events\",\"anchor_ref\":\"Example 1:1\",",
                "\"target_ranges\":[{\"start_ref\":\"Example 1:1\",\"end_ref\":\"Example 1:1\"}]}\n"
            ),
        )
        .unwrap();

        let error =
            pack_overlay_jsonl(&input, &output).expect_err("missing generated signature must fail");
        assert!(error
            .to_string()
            .contains("generated event signature counts do not match manifest"));
        assert!(!output.exists());

        fs::remove_file(input).unwrap();
    }

    #[test]
    fn overlay_packer_rejects_malformed_or_unversioned_event_signatures() {
        let (input, output) = temp_paths("overlay-signature-validation");
        let write_fixture = |normalizer: &str, marker: &str| {
            fs::write(
                &input,
                format!(
                    concat!(
                        "{{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
                        "\"concept_sources\":[{{\"concept_type\":\"event\",\"record_count\":1,\"alias_count\":1,",
                        "\"generated_signature_count\":1{normalizer}}}],\"alternate_sources\":[]}}\n",
                        "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"synthetic event\",",
                        "\"label\":\"Synthetic Event\",\"aliases\":[\"{marker}\"],\"source\":\"BSB.events\",",
                        "\"anchor_ref\":\"Example 1:1\",\"target_ranges\":[{{\"start_ref\":\"Example 1:1\",",
                        "\"end_ref\":\"Example 1:1\"}}]}}\n"
                    ),
                    normalizer = normalizer,
                    marker = marker,
                ),
            )
            .unwrap();
        };

        write_fixture("", "event-signature-v1:full:cobalt lantern repair");
        let error = pack_overlay_jsonl(&input, &output)
            .expect_err("signature normalizer identity must be required");
        assert!(error
            .to_string()
            .contains("incompatible or missing normalizer version"));
        assert!(!output.exists());

        write_fixture(
            ",\"signature_normalizer_version\":2",
            "event-signature-v2:full:cobalt lantern repair",
        );
        let error = pack_overlay_jsonl(&input, &output)
            .expect_err("unknown reserved marker must fail during compilation");
        assert!(error
            .to_string()
            .contains("unknown reserved event signature marker"));
        assert!(!output.exists());

        fs::remove_file(input).unwrap();
    }

    #[test]
    fn overlay_packer_emits_typed_binary_by_default() {
        let (input, output) = temp_paths("overlay-binary");
        fs::write(
            &input,
            concat!(
                "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
                "\"concept_sources\":[{\"concept_type\":\"event\",\"record_count\":1,\"alias_count\":1}],",
                "\"alternate_sources\":[{\"translation\":\"KJV\",\"matched_verse_count\":1}]}\n",
                "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"the fall\",",
                "\"label\":\"The Fall\",\"aliases\":[\"fall of humanity\"],\"source\":\"test\",\"anchor_ref\":\"Genesis 1:1\",",
                "\"target_ranges\":[{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:2\"}]}\n",
                "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",",
                "\"target_ref\":\"Genesis 1:1\",\"text\":\"In the beginning God created the heaven and the earth.\"}\n"
            ),
        )
        .unwrap();

        pack_overlay_jsonl(&input, &output).unwrap();
        let pack = fs::read(&output).unwrap();
        let reader = PackReader::open(&pack).unwrap();
        let body = reader.section_bytes(SectionTag::Phrase).unwrap();
        assert!(is_binary_overlay(body));
        let overlay = OverlaySection::parse(body).unwrap();
        assert_eq!(overlay.canonical_translation, "BSB");
        assert_eq!(overlay.canonical_verse_count, 2);
        assert_eq!(overlay.concepts.len(), 1);
        assert_eq!(overlay.alternates.len(), 1);

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
    }

    #[test]
    fn overlay_packer_rejects_unreviewed_correction_provenance() {
        let (input, output) = temp_paths("overlay-unreviewed-correction");
        fs::write(
            &input,
            concat!(
                "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
                "\"concept_sources\":[{\"concept_type\":\"correction\",\"record_count\":1}],\"alternate_sources\":[]}\n",
                "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"correction\",\"key\":\"invented count\",",
                "\"label\":\"The source does not state a count\",\"aliases\":[],\"source\":\"candidate-only\",",
                "\"anchor_ref\":\"Genesis 1:1\",\"target_ranges\":[{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:2\"}]}\n"
            ),
        )
        .unwrap();

        let error = pack_overlay_jsonl(&input, &output)
            .expect_err("unreviewed correction must not become a binary overlay");
        assert!(error.to_string().contains("invalid binary overlay section"));
        assert!(!output.exists());
        fs::remove_file(input).unwrap();
    }

    #[test]
    fn expansion_packer_emits_valid_targeted_meta_and_expand_sections() {
        let (input, output) = temp_paths("expand-valid");
        fs::write(
            &input,
            format!(
                "{}{}",
                expansion_manifest(31_086, 1, 2),
                concat!(
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"Exhausted\",",
                "\"targets\":[{\"stem\":\"Rested\",\"weight\":0.8},{\"stem\":\"Strength\",\"weight\":0.6}]}\n"
                )
            ),
        )
        .unwrap();

        pack_expand_jsonl(&input, &output).unwrap();
        let bytes = fs::read(&output).unwrap();
        let reader = PackReader::open(&bytes).unwrap();
        assert_eq!(
            reader
                .sections()
                .iter()
                .map(|section| section.tag)
                .collect::<Vec<_>>(),
            [SectionTag::Meta, SectionTag::Expand]
        );
        let meta = reader.meta().unwrap();
        assert_eq!(meta.translation_id, "BSB");
        assert_eq!(meta.verse_count, 31_086);
        let expand = reader.expand().unwrap();
        assert_eq!(expand.canonical_translation().unwrap(), "BSB");
        assert_eq!(expand.canonical_verse_count(), 31_086);
        assert_eq!(expand.format_version(), 2);
        let provenance = super::expand_provenance_path(&output);
        assert_eq!(
            expand.provenance_sha256().unwrap().as_slice(),
            sha2::Sha256::digest(fs::read(&provenance).unwrap()).as_slice()
        );
        let neighbors: Vec<_> = expand.expansions("exhaust").unwrap().collect();
        assert_eq!(neighbors[0].stem, "rest");
        assert_eq!(neighbors[1].stem, "strength");

        fs::remove_file(input).unwrap();
        fs::remove_file(output).unwrap();
        fs::remove_file(provenance).unwrap();
    }

    #[test]
    fn expansion_packer_rejects_a_stale_normalizer_manifest() {
        let (input, output) = temp_paths("expand-stale-normalizer");
        let stale_manifest = expansion_manifest(2, 1, 1).replace(
            &format!("\"normalizer_version\":{}", super::NORMALIZER_VERSION),
            "\"normalizer_version\":1",
        );
        fs::write(
            &input,
            format!(
                "{stale_manifest}{}",
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"weary\",\"targets\":[{\"stem\":\"rest\",\"weight\":0.8}]}\n"
            ),
        )
        .unwrap();

        let error = pack_expand_jsonl(&input, &output)
            .expect_err("a graph mined under an old normalizer must fail closed");
        assert!(error
            .to_string()
            .contains("expansion manifest contains invalid provenance"));
        assert!(!output.exists());
        assert!(!super::expand_provenance_path(&output).exists());
        fs::remove_file(input).unwrap();
    }

    #[test]
    fn expansion_packer_binds_source_provenance_while_preserving_logical_graph() {
        let (first_input, first_output) = temp_paths("expand-determinism-a");
        let (second_input, second_output) = temp_paths("expand-determinism-b");
        fs::write(
            &first_input,
            format!(
                "{}{}",
                expansion_manifest(2, 2, 2),
                concat!(
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"Created\",\"targets\":[{\"stem\":\"Rested\",\"weight\":0.8}]}\n",
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"Weary\",\"targets\":[{\"stem\":\"Strength\",\"weight\":0.6}]}\n"
                )
            ),
        )
        .unwrap();
        fs::write(
            &second_input,
            format!(
                "{}{}",
                expansion_manifest(2, 3, 3),
                concat!(
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"WEARY\",\"targets\":[{\"stem\":\"strength\",\"weight\":0.6}]}\n",
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"creation\",\"targets\":[{\"stem\":\"rested\",\"weight\":0.4}]}\n",
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"created\",\"targets\":[{\"stem\":\"RESTED\",\"weight\":0.8}]}\n"
                )
            ),
        )
        .unwrap();

        pack_expand_jsonl(&first_input, &first_output).unwrap();
        pack_expand_jsonl(&second_input, &second_output).unwrap();
        assert_ne!(
            fs::read(&first_output).unwrap(),
            fs::read(&second_output).unwrap()
        );
        let first_bytes = fs::read(&first_output).unwrap();
        let second_bytes = fs::read(&second_output).unwrap();
        let first_reader = PackReader::open(&first_bytes).unwrap();
        let second_reader = PackReader::open(&second_bytes).unwrap();
        let first_expand = first_reader.expand().unwrap();
        let second_expand = second_reader.expand().unwrap();
        for source in ["create", "weary"] {
            let first: Vec<_> = first_expand
                .expansions(source)
                .unwrap()
                .map(|edge| (edge.stem.to_string(), edge.weight))
                .collect();
            let second: Vec<_> = second_expand
                .expansions(source)
                .unwrap()
                .map(|edge| (edge.stem.to_string(), edge.weight))
                .collect();
            assert_eq!(first, second);
        }

        for path in [
            super::expand_provenance_path(&first_output),
            super::expand_provenance_path(&second_output),
            first_input,
            first_output,
            second_input,
            second_output,
        ] {
            fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn expansion_packer_rejects_missing_manifest_bad_weights_and_multiword_nodes() {
        let (input, output) = temp_paths("expand-invalid");
        fs::write(
            &input,
            "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"weary\",\"targets\":[{\"stem\":\"rest\",\"weight\":0.8}]}\n",
        )
        .unwrap();
        assert!(pack_expand_jsonl(&input, &output)
            .unwrap_err()
            .to_string()
            .contains("manifest must be the first record"));

        fs::write(
            &input,
            format!(
                "{}{}",
                expansion_manifest(2, 1, 1),
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"weary\",\"targets\":[{\"stem\":\"rest\",\"weight\":0}]}\n"
            ),
        )
        .unwrap();
        assert!(pack_expand_jsonl(&input, &output)
            .unwrap_err()
            .to_string()
            .contains("target weight must be in (0, 1]"));

        fs::write(
            &input,
            format!(
                "{}{}",
                expansion_manifest(2, 1, 1),
                "{\"schema_version\":1,\"kind\":\"expansion\",\"source\":\"very weary\",\"targets\":[{\"stem\":\"rest\",\"weight\":0.8}]}\n"
            ),
        )
        .unwrap();
        assert!(pack_expand_jsonl(&input, &output)
            .unwrap_err()
            .to_string()
            .contains("exactly one vocabulary token"));
        assert!(!output.exists());

        fs::remove_file(input).unwrap();
    }

    #[test]
    fn semantic_space_id_is_strict_hex() {
        assert_eq!(parse_space_id(&"ab".repeat(32)).unwrap(), [0xab; 32]);
        assert!(parse_space_id("00").is_err());
        assert!(parse_space_id(&"00".repeat(32)).is_err());
        assert!(parse_space_id(&format!("{}zz", "ab".repeat(31))).is_err());
    }
}

use crate::lanes::traits::{Lane, LaneHit, LaneId, LaneSearchOutput};
use crate::lanes::PackedExpandLane;
use crate::normalize::{
    normalize_query, semantic_query_stems, NormalizedQuery, NORMALIZER_VERSION,
};
use crate::result::{Confidence, PassageRange};
use recall_pack::{is_binary_overlay, OverlaySection as BinaryOverlaySection, Verse};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

const OVERLAY_SCHEMA_VERSION: u64 = 1;
const MAX_DIRECT_PHRASE_MATCHES: usize = 3;
const MIN_FUZZY_ALTERNATE_WORDS: usize = 5;
const MIN_FUZZY_ALTERNATE_EXACT_WORDS: usize = 4;
const MAX_FUZZY_ALTERNATE_MISMATCHES: usize = 1;
const MAX_FUZZY_ALTERNATE_MISMATCH_PERCENT: usize = 20;
const MAX_FUZZY_ALTERNATE_MATCHES: usize = 3;
const MIN_DIRECT_ALTERNATE_COVERAGE_PERCENT: usize = 95;
const EVENT_SIGNATURE_ALIAS_PREFIX: &str = "event-signature-v1:";
const EVENT_SIGNATURE_RESERVED_PREFIX: &str = "event-signature-";
// Signature scheme v1 is explicitly bound to normalizer v2. A future
// normalizer change must mint a new marker scheme rather than silently making
// existing binary overlays mean something different.
const EVENT_SIGNATURE_NORMALIZER_VERSION: u16 = 2;
const _: () = assert!(EVENT_SIGNATURE_NORMALIZER_VERSION == NORMALIZER_VERSION);
const EVENT_SIGNATURE_SOURCE: &str = "BSB.events";
const MIN_EVENT_SIGNATURE_EXPANSION_WEIGHT: f32 = 0.35;

/// Parsed, ref-aligned recall data from one or more PHRASE overlay sections.
#[derive(Debug, Default)]
pub struct RecallOverlay {
    concepts: Vec<ConceptRecord>,
    alternates: Vec<AlternateTextRecord>,
}

#[derive(Debug, Deserialize)]
struct ManifestRecord {
    canonical_translation: String,
    canonical_verse_count: usize,
    concept_sources: Vec<ManifestConceptSource>,
    alternate_sources: Vec<ManifestAlternateSource>,
}

#[derive(Debug, Deserialize)]
struct ManifestConceptSource {
    concept_type: String,
    record_count: usize,
    #[serde(default)]
    alias_count: Option<usize>,
    #[serde(default)]
    generated_alias_count: Option<usize>,
    #[serde(default)]
    generated_signature_count: Option<usize>,
    #[serde(default)]
    signature_normalizer_version: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct ManifestAlternateSource {
    translation: String,
    matched_verse_count: usize,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct ConceptRecord {
    concept_type: String,
    key: String,
    label: String,
    #[serde(default)]
    aliases: Vec<String>,
    source: String,
    anchor_ref: String,
    #[serde(default)]
    target_ranges: Vec<TargetRange>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct TargetRange {
    start_ref: String,
    end_ref: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct AlternateTextRecord {
    source_translation: String,
    target_ref: String,
    text: String,
}

impl RecallOverlay {
    /// Parse the deterministic JSONL contract emitted by
    /// `scripts/build_recall_overlay.py`.
    pub fn parse<B: AsRef<[u8]>>(
        sections: &[B],
        canonical_translation: &str,
        canonical_verse_count: usize,
    ) -> Result<Self, String> {
        let mut out = Self::default();

        for (section_index, bytes) in sections.iter().enumerate() {
            let section_number = section_index + 1;
            let (section_concepts, section_alternates) = if is_binary_overlay(bytes.as_ref()) {
                parse_binary_section(
                    bytes.as_ref(),
                    canonical_translation,
                    canonical_verse_count,
                    section_number,
                )?
            } else {
                parse_legacy_json_section(
                    bytes.as_ref(),
                    canonical_translation,
                    canonical_verse_count,
                    section_number,
                )?
            };
            out.concepts.extend(section_concepts);
            out.alternates.extend(section_alternates);
        }

        validate_ranked_concept_surfaces(&out.concepts)?;
        let mut seen_concepts = HashSet::new();
        out.concepts
            .retain(|record| seen_concepts.insert(concept_fingerprint(record)));
        let mut seen_alternates = HashSet::new();
        out.alternates.retain(|record| {
            seen_alternates.insert((
                record.source_translation.clone(),
                normalize_ref_key(&record.target_ref),
                record.text.clone(),
            ))
        });

        Ok(out)
    }

    pub fn into_lanes_with_expansion(
        self,
        canonical: &[Verse],
        semantic_expander: Option<Arc<PackedExpandLane>>,
    ) -> Result<Vec<Box<dyn Lane>>, String> {
        let RecallOverlay {
            concepts,
            alternates,
        } = self;
        // Overlay authority is fixed by the source record type at build time.
        // In particular, Study Bible/Theographic people, places, and
        // `event_shadow` records stay score-zero explanation: opening an engine
        // must never promote them into ranked events or manufacture aliases.
        validate_ranked_concept_surfaces(&concepts)?;
        let (corrections, concepts): (Vec<_>, Vec<_>) = concepts
            .into_iter()
            .partition(|record| record.concept_type == "correction");
        let mut lanes: Vec<Box<dyn Lane>> = Vec::new();
        if !corrections.is_empty() {
            lanes.push(Box::new(CorrectionLane::new(
                canonical,
                &alternates,
                corrections,
            )?));
        }
        if !alternates.is_empty() {
            lanes.push(Box::new(AlternateTextLane::new(canonical, alternates)?));
        }
        let (shadow_concepts, ranked_concepts): (Vec<_>, Vec<_>) =
            concepts.into_iter().partition(|record| {
                matches!(
                    record.concept_type.as_str(),
                    "topic" | "entity" | "person" | "place" | "event_shadow"
                )
            });
        // Data-driven traditional-title routing: a 2–5 word topic surface may
        // become an ordinary event alias only when exactly one longer BSB event
        // name contains it as a contiguous word window, the topic and event
        // passages overlap in the same book, and the surface is not itself a
        // full event key. This is the OVERLAYS "ten commandments" rule, applied
        // uniformly at open time so release packs benefit without gold lists.
        let ranked_concepts =
            attach_topic_contained_event_aliases(ranked_concepts, &shadow_concepts, canonical);
        if !ranked_concepts.is_empty() {
            // Ranked BSB events may consult EXPAND only through validated
            // build-generated multi-cue signatures. Their direct tier remains
            // zero; curated records retain their existing exact-only path.
            lanes.push(Box::new(ConceptLane::new_with_expander(
                canonical,
                ranked_concepts,
                LaneId::D,
                false,
                semantic_expander.clone(),
            )?));
        }
        if !shadow_concepts.is_empty() {
            lanes.push(Box::new(ConceptLane::new_with_expander(
                canonical,
                shadow_concepts,
                // Named multi-word topics and independently constrained
                // multi-cue topic matches belong to semantic Lane C. Graph
                // entities/events remain score-zero explanatory evidence.
                LaneId::C,
                true,
                semantic_expander,
            )?));
        }
        Ok(lanes)
    }
}

fn attach_topic_contained_event_aliases(
    mut ranked: Vec<ConceptRecord>,
    shadows: &[ConceptRecord],
    canonical: &[Verse],
) -> Vec<ConceptRecord> {
    let ref_to_index: HashMap<String, u32> = canonical
        .iter()
        .map(|verse| (normalize_ref_key(&verse.reference), verse.index))
        .collect();
    let event_indexes: Vec<usize> = ranked
        .iter()
        .enumerate()
        .filter(|(_, record)| record.concept_type == "event")
        .map(|(index, _)| index)
        .collect();
    if event_indexes.is_empty() {
        return ranked;
    }

    let event_keys: Vec<String> = event_indexes
        .iter()
        .map(|&index| canonical_surface(&ranked[index].key))
        .collect();
    let full_event_key_set: HashSet<String> = event_keys.iter().cloned().collect();

    let mut topic_surfaces: Vec<(String, Vec<(u32, u32)>)> = Vec::new();
    for record in shadows.iter().filter(|record| record.concept_type == "topic") {
        let mut surfaces = vec![canonical_surface(&record.key)];
        surfaces.extend(record.aliases.iter().map(|alias| canonical_surface(alias)));
        surfaces.retain(|surface| {
            let words = surface.split_whitespace().count();
            (2..=5).contains(&words) && !full_event_key_set.contains(surface)
        });
        surfaces.sort();
        surfaces.dedup();
        if surfaces.is_empty() {
            continue;
        }
        let ranges = topic_index_ranges(record, &ref_to_index);
        if ranges.is_empty() {
            continue;
        }
        for surface in surfaces {
            topic_surfaces.push((surface, ranges.clone()));
        }
    }

    // surface -> list of (event_local_index) that contain it as a contiguous
    // word window and share a same-book overlapping range with the topic.
    let mut owners: HashMap<String, Vec<usize>> = HashMap::new();
    for (surface, topic_ranges) in &topic_surfaces {
        let surface_words: Vec<&str> = surface.split_whitespace().collect();
        for (local, &event_index) in event_indexes.iter().enumerate() {
            let event_words: Vec<&str> = event_keys[local].split_whitespace().collect();
            if !is_contiguous_word_subsequence(&surface_words, &event_words) {
                continue;
            }
            let event_ranges = concept_index_ranges(&ranked[event_index], &ref_to_index);
            if !ranges_overlap_same_book(topic_ranges, &event_ranges) {
                continue;
            }
            owners.entry(surface.clone()).or_default().push(local);
        }
    }

    for (surface, locals) in owners {
        let mut unique = locals;
        unique.sort_unstable();
        unique.dedup();
        if unique.len() != 1 {
            continue;
        }
        let event_index = event_indexes[unique[0]];
        if ranked[event_index].key == surface
            || ranked[event_index]
                .aliases
                .iter()
                .any(|alias| alias == &surface)
        {
            continue;
        }
        // Ordinary human-readable alias (not a generated signature marker).
        ranked[event_index].aliases.push(surface);
    }
    ranked
}

fn topic_index_ranges(
    record: &ConceptRecord,
    ref_to_index: &HashMap<String, u32>,
) -> Vec<(u32, u32)> {
    concept_index_ranges(record, ref_to_index)
}

fn concept_index_ranges(
    record: &ConceptRecord,
    ref_to_index: &HashMap<String, u32>,
) -> Vec<(u32, u32)> {
    record
        .target_ranges
        .iter()
        .filter_map(|range| {
            let start = *ref_to_index.get(&normalize_ref_key(&range.start_ref))?;
            let end = *ref_to_index.get(&normalize_ref_key(&range.end_ref))?;
            (start <= end).then_some((start, end))
        })
        .collect()
}

fn is_contiguous_word_subsequence(needle: &[&str], haystack: &[&str]) -> bool {
    if needle.is_empty() || needle.len() >= haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn ranges_overlap_same_book(left: &[(u32, u32)], right: &[(u32, u32)]) -> bool {
    left.iter()
        .any(|(ls, le)| right.iter().any(|(rs, re)| ls <= re && rs <= le))
}

fn parse_binary_section(
    bytes: &[u8],
    canonical_translation: &str,
    canonical_verse_count: usize,
    section_number: usize,
) -> Result<(Vec<ConceptRecord>, Vec<AlternateTextRecord>), String> {
    let section = BinaryOverlaySection::parse(bytes)
        .map_err(|error| format!("overlay section {section_number}: {error}"))?;
    if !section
        .canonical_translation
        .eq_ignore_ascii_case(canonical_translation)
    {
        return Err(format!(
            "overlay targets {} but base pack is {}",
            section.canonical_translation, canonical_translation
        ));
    }
    if section.canonical_verse_count as usize != canonical_verse_count {
        return Err(format!(
            "overlay verse count {} does not match base pack {}",
            section.canonical_verse_count, canonical_verse_count
        ));
    }
    let concepts = section
        .concepts
        .into_iter()
        .map(|record| ConceptRecord {
            concept_type: record.concept_type,
            key: record.key,
            label: record.label,
            aliases: record.aliases,
            source: record.source,
            anchor_ref: record.anchor_ref,
            target_ranges: record
                .target_ranges
                .into_iter()
                .map(|range| TargetRange {
                    start_ref: range.start_ref,
                    end_ref: range.end_ref,
                })
                .collect(),
        })
        .collect();
    let alternates = section
        .alternates
        .into_iter()
        .map(|record| AlternateTextRecord {
            source_translation: record.source_translation,
            target_ref: record.target_ref,
            text: record.text,
        })
        .collect();
    Ok((concepts, alternates))
}

fn parse_legacy_json_section(
    bytes: &[u8],
    canonical_translation: &str,
    canonical_verse_count: usize,
    section_number: usize,
) -> Result<(Vec<ConceptRecord>, Vec<AlternateTextRecord>), String> {
    let raw = std::str::from_utf8(bytes)
        .map_err(|_| format!("overlay section {section_number} is not UTF-8"))?;
    let mut manifest: Option<ManifestRecord> = None;
    let mut concepts = Vec::new();
    let mut alternates = Vec::new();
    for (line_index, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line_number = line_index + 1;
        let value: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            format!("overlay section {section_number} line {line_number}: {error}")
        })?;
        let schema_version = value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64);
        if schema_version != Some(OVERLAY_SCHEMA_VERSION) {
            return Err(format!(
                "overlay section {section_number} line {line_number}: unsupported schema version {schema_version:?}"
            ));
        }
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                format!("overlay section {section_number} line {line_number}: missing kind")
            })?;
        match kind {
            "manifest" => {
                if manifest.is_some() {
                    return Err(format!(
                        "overlay section {section_number} line {line_number}: duplicate manifest"
                    ));
                }
                let parsed: ManifestRecord = serde_json::from_value(value).map_err(|error| {
                    format!(
                        "overlay section {section_number} line {line_number}: invalid manifest: {error}"
                    )
                })?;
                if !parsed
                    .canonical_translation
                    .eq_ignore_ascii_case(canonical_translation)
                {
                    return Err(format!(
                        "overlay targets {} but base pack is {}",
                        parsed.canonical_translation, canonical_translation
                    ));
                }
                if parsed.canonical_verse_count != canonical_verse_count {
                    return Err(format!(
                        "overlay verse count {} does not match base pack {}",
                        parsed.canonical_verse_count, canonical_verse_count
                    ));
                }
                manifest = Some(parsed);
            }
            "concept" => {
                if manifest.is_none() {
                    return Err(format!(
                        "overlay section {section_number} line {line_number}: manifest must be the first record"
                    ));
                }
                concepts.push(serde_json::from_value(value).map_err(|error| {
                    format!(
                        "overlay section {section_number} line {line_number}: invalid concept: {error}"
                    )
                })?);
            }
            "alternate_text" => {
                if manifest.is_none() {
                    return Err(format!(
                        "overlay section {section_number} line {line_number}: manifest must be the first record"
                    ));
                }
                alternates.push(serde_json::from_value(value).map_err(|error| {
                    format!(
                        "overlay section {section_number} line {line_number}: invalid alternate text: {error}"
                    )
                })?);
            }
            other => {
                return Err(format!(
                    "overlay section {section_number} line {line_number}: unsupported kind {other}"
                ));
            }
        }
    }
    let manifest = manifest
        .ok_or_else(|| format!("overlay section {section_number} is missing its manifest"))?;
    validate_manifest_counts(&manifest, &concepts, &alternates, section_number)?;
    Ok((concepts, alternates))
}

fn validate_manifest_counts(
    manifest: &ManifestRecord,
    concepts: &[ConceptRecord],
    alternates: &[AlternateTextRecord],
    section_number: usize,
) -> Result<(), String> {
    let mut expected_concepts = BTreeMap::new();
    let mut expected_signatures = BTreeMap::new();
    for source in &manifest.concept_sources {
        let aliases = match (source.alias_count, source.generated_alias_count) {
            (Some(_), Some(_)) => {
                return Err(format!(
                    "overlay section {section_number} concept source {} declares two alias counts",
                    source.concept_type
                ));
            }
            (Some(count), None) | (None, Some(count)) => count,
            (None, None) => 0,
        };
        if expected_concepts
            .insert(source.concept_type.clone(), (source.record_count, aliases))
            .is_some()
        {
            return Err(format!(
                "overlay section {section_number} repeats manifest concept type {}",
                source.concept_type
            ));
        }
        let signature_count = source.generated_signature_count.unwrap_or(0);
        if signature_count > 0 && source.concept_type != "event" {
            return Err(format!(
                "overlay section {section_number} non-event concept source {} declares generated event signatures",
                source.concept_type
            ));
        }
        if source
            .signature_normalizer_version
            .is_some_and(|version| version != NORMALIZER_VERSION)
            || (signature_count > 0
                && source.signature_normalizer_version != Some(NORMALIZER_VERSION))
        {
            return Err(format!(
                "overlay section {section_number} generated event signatures use an incompatible or missing normalizer version"
            ));
        }
        expected_signatures.insert(source.concept_type.clone(), signature_count);
    }
    let mut actual_concepts: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut actual_signatures: BTreeMap<String, usize> = BTreeMap::new();
    for record in concepts {
        let counts = actual_concepts
            .entry(record.concept_type.clone())
            .or_default();
        counts.0 += 1;
        counts.1 += record.aliases.len();
        *actual_signatures
            .entry(record.concept_type.clone())
            .or_default() += record
            .aliases
            .iter()
            .filter(|alias| alias.starts_with(EVENT_SIGNATURE_ALIAS_PREFIX))
            .count();
    }
    if expected_concepts != actual_concepts {
        return Err(format!(
            "overlay section {section_number} concept counts do not match manifest: expected {expected_concepts:?}, got {actual_concepts:?}"
        ));
    }
    actual_signatures.retain(|_, count| *count > 0);
    expected_signatures.retain(|_, count| *count > 0);
    if expected_signatures != actual_signatures {
        return Err(format!(
            "overlay section {section_number} generated event signature counts do not match manifest: expected {expected_signatures:?}, got {actual_signatures:?}"
        ));
    }

    let mut expected_alternates = BTreeMap::new();
    for source in &manifest.alternate_sources {
        if expected_alternates
            .insert(source.translation.clone(), source.matched_verse_count)
            .is_some()
        {
            return Err(format!(
                "overlay section {section_number} repeats manifest alternate translation {}",
                source.translation
            ));
        }
    }
    let mut actual_alternates = BTreeMap::new();
    for record in alternates {
        *actual_alternates
            .entry(record.source_translation.clone())
            .or_default() += 1;
    }
    if expected_alternates != actual_alternates {
        return Err(format!(
            "overlay section {section_number} alternate counts do not match manifest: expected {expected_alternates:?}, got {actual_alternates:?}"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventSignatureKind {
    Single,
    Partial,
    Full,
}

impl EventSignatureKind {
    fn name(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Partial => "partial",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompiledEventSignature {
    kind: EventSignatureKind,
    stems: Vec<String>,
}

fn parse_event_signature_alias(value: &str) -> Result<Option<CompiledEventSignature>, String> {
    let Some(encoded) = value.strip_prefix(EVENT_SIGNATURE_ALIAS_PREFIX) else {
        if value
            .to_ascii_lowercase()
            .starts_with(EVENT_SIGNATURE_RESERVED_PREFIX)
        {
            return Err(format!("unknown reserved event signature marker {value:?}"));
        }
        return Ok(None);
    };
    let (raw_kind, payload) = encoded
        .split_once(':')
        .ok_or_else(|| format!("malformed generated event signature {value:?}"))?;
    let kind = match raw_kind {
        "single" => EventSignatureKind::Single,
        "partial" => EventSignatureKind::Partial,
        "full" => EventSignatureKind::Full,
        _ => {
            return Err(format!(
                "unknown generated event signature kind {raw_kind:?}"
            ))
        }
    };
    if payload.is_empty() || canonical_surface(payload) != payload {
        return Err(format!(
            "generated event signature payload is not canonical: {value:?}"
        ));
    }
    let payload_stems: Vec<String> = payload.split_whitespace().map(str::to_string).collect();
    let mut sorted_stems = payload_stems.clone();
    sorted_stems.sort();
    sorted_stems.dedup();
    if sorted_stems != payload_stems {
        return Err(format!(
            "generated event signature does not use sorted normalizer stems: {value:?}"
        ));
    }
    if payload_stems.iter().any(|payload_stem| {
        !payload_stem
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    }) {
        return Err(format!(
            "generated event signature payload is outside the ASCII normalizer contract: {value:?}"
        ));
    }
    let valid_length = match kind {
        EventSignatureKind::Single => {
            payload_stems.len() == 1
                && payload_stems[0].len() >= 7
                && payload_stems[0]
                    .bytes()
                    .all(|byte| byte.is_ascii_alphabetic())
        }
        EventSignatureKind::Partial => (2..=4).contains(&payload_stems.len()),
        EventSignatureKind::Full => (2..=8).contains(&payload_stems.len()),
    };
    if !valid_length {
        return Err(format!(
            "generated event signature has invalid stem count: {value:?}"
        ));
    }
    Ok(Some(CompiledEventSignature {
        kind,
        stems: payload_stems,
    }))
}

fn validate_ranked_concept_surfaces(records: &[ConceptRecord]) -> Result<(), String> {
    let mut mappings: BTreeMap<String, (String, String)> = BTreeMap::new();
    let mut signature_mappings: BTreeMap<String, (String, String)> = BTreeMap::new();
    for record in records {
        for alias in &record.aliases {
            if parse_event_signature_alias(alias)?.is_some()
                && (record.concept_type != "event" || record.source != EVENT_SIGNATURE_SOURCE)
            {
                return Err(format!(
                    "generated event signature on untrusted concept {:?}",
                    record.label
                ));
            }
        }
    }
    for record in records.iter().filter(|record| {
        matches!(
            record.concept_type.as_str(),
            "curated" | "event" | "correction"
        )
    }) {
        let signature = concept_target_signature(record);
        let searchable_labels = (record.concept_type != "correction")
            .then_some(&record.label)
            .into_iter();
        for surface in std::iter::once(&record.key)
            .chain(searchable_labels)
            .chain(record.aliases.iter())
            .filter(|value| !value.starts_with(EVENT_SIGNATURE_ALIAS_PREFIX))
            .map(|value| canonical_surface(value))
            .filter(|value| !value.is_empty())
        {
            if let Some((existing_signature, existing_label)) = mappings.get(&surface) {
                if existing_signature != &signature {
                    return Err(format!(
                        "ranked concept surface {surface:?} conflicts between {existing_label:?} and {:?}",
                        record.label
                    ));
                }
            } else {
                mappings.insert(surface, (signature.clone(), record.label.clone()));
            }
        }
        for alias in &record.aliases {
            let Some(event_signature) = parse_event_signature_alias(alias)? else {
                continue;
            };
            let surface = event_signature.stems.join(" ");
            if let Some((existing_signature, existing_label)) = signature_mappings.get(&surface) {
                if existing_signature != &signature {
                    return Err(format!(
                        "generated event signature {surface:?} conflicts between {existing_label:?} and {:?}",
                        record.label
                    ));
                }
            } else {
                signature_mappings.insert(surface, (signature.clone(), record.label.clone()));
            }
        }
    }
    for (surface, (signature, signature_label)) in &signature_mappings {
        if let Some((ordinary_signature, ordinary_label)) = mappings.get(surface) {
            if ordinary_signature != signature {
                return Err(format!(
                    "generated event signature {surface:?} for {signature_label:?} conflicts with ranked concept surface on {ordinary_label:?}"
                ));
            }
        }
    }
    Ok(())
}

fn concept_target_signature(record: &ConceptRecord) -> String {
    let mut signature = normalize_ref_key(&record.anchor_ref);
    for range in &record.target_ranges {
        signature.push('|');
        signature.push_str(&normalize_ref_key(&range.start_ref));
        signature.push('-');
        signature.push_str(&normalize_ref_key(&range.end_ref));
    }
    signature
}

fn concept_fingerprint(record: &ConceptRecord) -> String {
    let mut aliases: Vec<String> = record
        .aliases
        .iter()
        .map(|alias| canonical_surface(alias))
        .collect();
    aliases.sort();
    format!(
        "{}|{}|{}|{}|{}|{}",
        record.concept_type,
        canonical_surface(&record.key),
        canonical_surface(&record.label),
        aliases.join(","),
        record.source,
        concept_target_signature(record)
    )
}

const CORRECTION_SOURCE_PREFIX: &str = "correction-v1";
const CORRECTION_REVIEW_POLICY: &str = "source-backed-correction-v1";
const CORRECTION_LICENSE: &str = "CC-BY-SA-4.0";
const MAX_CORRECTION_WORDS: usize = 24;

struct CorrectionEntry {
    keys: Arc<[String]>,
    label: Arc<str>,
    source: Arc<str>,
    verse_index: u32,
    passages: Vec<PassageRange>,
}

/// Exact reviewed-claim provider for independently sourced misconceptions.
///
/// This lane deliberately has no fuzzy matching. A record can label a result
/// `OftenMisremembered` only when its complete normalized query surface was
/// reviewed and its compact runtime provenance proves that the build pipeline
/// accepted an Aquifer record under the correction review policy. Any surface
/// occurring verbatim in the canonical or supplied alternate translations
/// rejects the overlay at load time, so ordinary Scripture can never acquire a
/// correction label merely because an editorial record was malformed.
struct CorrectionLane {
    entries: Vec<CorrectionEntry>,
}

impl CorrectionLane {
    fn new(
        canonical: &[Verse],
        alternates: &[AlternateTextRecord],
        records: Vec<ConceptRecord>,
    ) -> Result<Self, String> {
        let ref_to_index = canonical_ref_map(canonical)?;
        let mut scripture_tokens: Vec<Vec<String>> = canonical
            .iter()
            .map(|verse| canonical_tokens(&verse.text))
            .collect();
        scripture_tokens.extend(
            alternates
                .iter()
                .map(|record| canonical_tokens(&record.text)),
        );
        let mut claimed_surfaces: HashMap<String, String> = HashMap::new();
        let mut entries = Vec::with_capacity(records.len());

        for record in records {
            validate_correction_source(&record.source).map_err(|error| {
                format!(
                    "correction {} has invalid provenance: {error}",
                    record.label
                )
            })?;
            let Some(&verse_index) = ref_to_index.get(&normalize_ref_key(&record.anchor_ref))
            else {
                return Err(format!(
                    "correction {} targets missing base ref {}",
                    record.label, record.anchor_ref
                ));
            };
            if record.target_ranges.is_empty() {
                return Err(format!("correction {} has no target ranges", record.label));
            }

            let mut anchor_is_in_primary = false;
            let mut passages = Vec::with_capacity(record.target_ranges.len());
            for (range_index, range) in record.target_ranges.iter().enumerate() {
                let start = ref_to_index
                    .get(&normalize_ref_key(&range.start_ref))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "correction {} range targets missing base ref {}",
                            record.label, range.start_ref
                        )
                    })?;
                let end = ref_to_index
                    .get(&normalize_ref_key(&range.end_ref))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "correction {} range targets missing base ref {}",
                            record.label, range.end_ref
                        )
                    })?;
                if reference_book_key(&range.start_ref) != reference_book_key(&range.end_ref) {
                    return Err(format!(
                        "correction {} has cross-book target range {}–{}",
                        record.label, range.start_ref, range.end_ref
                    ));
                }
                if start > end {
                    return Err(format!(
                        "correction {} has reversed target range {}–{}",
                        record.label, range.start_ref, range.end_ref
                    ));
                }
                if range_index == 0 {
                    anchor_is_in_primary = start <= verse_index && verse_index <= end;
                }
                passages.push(PassageRange {
                    start_ref: range.start_ref.clone(),
                    end_ref: range.end_ref.clone(),
                });
            }
            if !anchor_is_in_primary {
                return Err(format!(
                    "correction {} anchor {} is outside its primary target range",
                    record.label, record.anchor_ref
                ));
            }

            // The display label explains the correction; it is intentionally
            // not searchable. Only explicitly reviewed claim surfaces may
            // trigger the safety label.
            let mut keys = vec![canonical_surface(&record.key)];
            keys.extend(record.aliases.iter().map(|alias| canonical_surface(alias)));
            keys.sort();
            keys.dedup();
            if keys.iter().any(|key| {
                let count = key.split_whitespace().count();
                !(2..=MAX_CORRECTION_WORDS).contains(&count)
            }) {
                return Err(format!(
                    "correction {} claim surfaces must contain 2–{} words",
                    record.label, MAX_CORRECTION_WORDS
                ));
            }
            for key in &keys {
                let key_tokens = canonical_tokens(key);
                if scripture_tokens
                    .iter()
                    .any(|text| contains_token_window(text, &key_tokens))
                {
                    return Err(format!(
                        "correction {} claim surface {:?} occurs in canonical or alternate Scripture",
                        record.label, key
                    ));
                }
                if let Some(existing) = claimed_surfaces.insert(key.clone(), record.label.clone()) {
                    return Err(format!(
                        "correction claim surface {:?} is repeated by {:?} and {:?}",
                        key, existing, record.label
                    ));
                }
            }

            entries.push(CorrectionEntry {
                keys: keys.into(),
                label: Arc::from(record.label.as_str()),
                source: Arc::from(record.source.as_str()),
                verse_index,
                passages,
            });
        }

        Ok(Self { entries })
    }
}

impl Lane for CorrectionLane {
    fn id(&self) -> LaneId {
        LaneId::D
    }

    fn available(&self) -> bool {
        !self.entries.is_empty()
    }

    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit> {
        let key = canonical_surface(&query.raw);
        let key_words: Vec<String> = key.split_whitespace().map(str::to_string).collect();
        if key_words.len() < 2 {
            return Vec::new();
        }
        let mut hits: Vec<LaneHit> = self
            .entries
            .iter()
            .filter(|entry| entry.keys.iter().any(|candidate| candidate == &key))
            .map(|entry| LaneHit {
                verse_index: entry.verse_index,
                score: 104.0,
                matched_terms: key_words.clone(),
                expanded_terms: vec![
                    format!("correction:{}", entry.label),
                    format!("source:{}", entry.source),
                ],
                passages: entry.passages.clone(),
                confidence_hint: Confidence::OftenMisremembered,
                ranking_tier: 4,
                direct_tier: 0,
            })
            .collect();
        hits.sort_by_key(|hit| hit.verse_index);
        hits.truncate(top_k.max(1) * 3);
        hits
    }
}

fn validate_correction_source(source: &str) -> Result<(), &'static str> {
    let mut parts = source.split('|');
    if parts.next() != Some(CORRECTION_SOURCE_PREFIX) {
        return Err("missing correction-v1 prefix");
    }
    let mut fields = HashMap::new();
    for part in parts {
        let Some((name, value)) = part.split_once('=') else {
            return Err("malformed provenance field");
        };
        if name.is_empty()
            || value.is_empty()
            || value.contains(['|', '='])
            || fields.insert(name, value).is_some()
        {
            return Err("empty, unsafe, or duplicate provenance field");
        }
    }
    const REQUIRED: &[&str] = &[
        "id",
        "dataset",
        "record",
        "revision",
        "license",
        "content-sha256",
        "review-status",
        "review-policy",
        "reviewed-by",
        "reviewed-at",
    ];
    if fields.len() != REQUIRED.len() || REQUIRED.iter().any(|name| !fields.contains_key(name)) {
        return Err("missing or unknown provenance fields");
    }
    if fields.get("dataset") != Some(&"BibleAquifer") {
        return Err("unsupported correction dataset");
    }
    if fields.get("license") != Some(&CORRECTION_LICENSE) {
        return Err("unsupported correction source license");
    }
    if fields.get("review-status") != Some(&"approved")
        || fields.get("review-policy") != Some(&CORRECTION_REVIEW_POLICY)
    {
        return Err("record was not approved under the correction review policy");
    }
    let sha = fields["content-sha256"];
    if sha.len() != 64 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("content-sha256 is not 64 hexadecimal characters");
    }
    let date = fields["reviewed-at"].as_bytes();
    if date.len() != 10
        || date[4] != b'-'
        || date[7] != b'-'
        || !date
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return Err("reviewed-at is not YYYY-MM-DD");
    }
    Ok(())
}

struct AlternateCorpus {
    translation: String,
    target_indices: Vec<u32>,
    alternate_tokens: Vec<Vec<u32>>,
    direct_eligible: bool,
}

/// Lane B recall over hidden, ref-aligned translation text.
///
/// The alternate wording is never returned to the host. Hits are mapped back
/// to the canonical BSB verse index before fusion. Only contiguous phrases of
/// at least four words are admitted in this first version: an alternate corpus
/// is a precision recall overlay, not another broad lexical voter that can
/// crowd strong canonical matches out of the result set.
pub struct AlternateTextLane {
    corpora: Vec<AlternateCorpus>,
    token_ids: HashMap<String, u32>,
}

impl AlternateTextLane {
    fn new(canonical: &[Verse], records: Vec<AlternateTextRecord>) -> Result<Self, String> {
        let ref_to_index = canonical_ref_map(canonical)?;
        let mut grouped: HashMap<String, Vec<AlternateTextRecord>> = HashMap::new();
        for record in records {
            if record.source_translation.trim().is_empty() || record.text.trim().is_empty() {
                return Err("alternate text record has an empty translation or text".into());
            }
            grouped
                .entry(record.source_translation.clone())
                .or_default()
                .push(record);
        }

        let mut names: Vec<String> = grouped.keys().cloned().collect();
        names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then(a.cmp(b)));
        let mut corpora = Vec::with_capacity(names.len());
        let mut token_ids: HashMap<String, u32> = HashMap::new();

        for name in names {
            let records = grouped.remove(&name).unwrap_or_default();
            let direct_eligible = records.len().saturating_mul(100)
                >= canonical
                    .len()
                    .saturating_mul(MIN_DIRECT_ALTERNATE_COVERAGE_PERCENT);
            let mut seen_targets = HashSet::new();
            let mut target_indices = Vec::with_capacity(records.len());
            let mut alternate_tokens = Vec::with_capacity(records.len());

            for record in records {
                let target_key = normalize_ref_key(&record.target_ref);
                let Some(&target_index) = ref_to_index.get(&target_key) else {
                    return Err(format!(
                        "{} alternate text targets missing base ref {}",
                        name, record.target_ref
                    ));
                };
                if !seen_targets.insert(target_index) {
                    return Err(format!(
                        "{} alternate text repeats target {}",
                        name, record.target_ref
                    ));
                }
                target_indices.push(target_index);
                let mut encoded = Vec::new();
                for token in canonical_tokens(&record.text) {
                    let next_id = u32::try_from(token_ids.len())
                        .map_err(|_| "alternate token dictionary exceeds u32")?;
                    let token_id = *token_ids.entry(token).or_insert(next_id);
                    encoded.push(token_id);
                }
                alternate_tokens.push(encoded);
            }

            corpora.push(AlternateCorpus {
                translation: name,
                target_indices,
                alternate_tokens,
                direct_eligible,
            });
        }

        Ok(Self { corpora, token_ids })
    }
}

impl Lane for AlternateTextLane {
    fn id(&self) -> LaneId {
        LaneId::B
    }

    fn available(&self) -> bool {
        !self.corpora.is_empty()
    }

    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit> {
        self.search_with_risk(query, top_k).hits
    }

    fn search_with_risk(&self, query: &NormalizedQuery, top_k: usize) -> LaneSearchOutput {
        let query_words = canonical_tokens(&query.raw);
        let query_word_count = query_words.len();
        if query_word_count < 4 {
            return LaneSearchOutput {
                hits: Vec::new(),
                blocks_unique_recovery: false,
            };
        }
        let query_tokens: Vec<Option<u32>> = query_words
            .iter()
            .map(|token| self.token_ids.get(token).copied())
            .collect();
        let exact_query_tokens: Option<Vec<u32>> = query_tokens.iter().copied().collect();
        let fuzzy_enabled = query_word_count >= MIN_FUZZY_ALTERNATE_WORDS;
        let mut exact_candidates: Vec<(&AlternateCorpus, u32)> = Vec::new();
        let mut fuzzy_candidates: Vec<(&AlternateCorpus, u32)> = Vec::new();
        let mut ambiguous_complete_corpus = false;
        let mut saw_exact_surface = false;

        for corpus in &self.corpora {
            let mut exact_indices = Vec::new();
            let mut fuzzy_indices = Vec::new();
            for (index, tokens) in corpus.alternate_tokens.iter().enumerate() {
                if exact_query_tokens
                    .as_ref()
                    .is_some_and(|needle| contains_token_window(tokens, needle))
                {
                    if exact_indices.len() <= MAX_DIRECT_PHRASE_MATCHES {
                        exact_indices.push(index);
                    }
                } else if fuzzy_enabled
                    && contains_bounded_fuzzy_token_window(tokens, &query_tokens)
                    && fuzzy_indices.len() <= MAX_FUZZY_ALTERNATE_MATCHES
                {
                    fuzzy_indices.push(index);
                }
            }

            saw_exact_surface |= !exact_indices.is_empty();
            if exact_indices.len() > MAX_DIRECT_PHRASE_MATCHES {
                ambiguous_complete_corpus |= corpus.direct_eligible;
            } else {
                for local_index in exact_indices {
                    let Some(&target_index) = corpus.target_indices.get(local_index) else {
                        continue;
                    };
                    exact_candidates.push((corpus, target_index));
                }
            }

            // A one-token drift is useful only when it remains selective.
            // Suppress a repeated fuzzy surface rather than returning an
            // arbitrary prefix of the matching verses.
            if fuzzy_indices.len() <= MAX_FUZZY_ALTERNATE_MATCHES {
                for local_index in fuzzy_indices {
                    let Some(&target_index) = corpus.target_indices.get(local_index) else {
                        continue;
                    };
                    fuzzy_candidates.push((corpus, target_index));
                }
            }
        }

        // Selectivity is also global across translation corpora. Exact hits
        // remain available when fuzzy evidence is ambiguous; approximate
        // evidence must never demote or crowd out an exact phrase recovery.
        if saw_exact_surface || fuzzy_candidates.len() > MAX_FUZZY_ALTERNATE_MATCHES {
            fuzzy_candidates.clear();
        }

        // Selectivity is global across every adequately covered hidden
        // translation. If KJV and ESV uniquely point to different refs, neither
        // may claim direct recovery. Agreement on the same target remains
        // direct and is consolidated into one Lane-B vote later.
        let eligible_targets: HashSet<u32> = exact_candidates
            .iter()
            .filter(|(corpus, _)| corpus.direct_eligible)
            .map(|(_, target_index)| *target_index)
            .collect();
        let direct_target = if !ambiguous_complete_corpus && eligible_targets.len() == 1 {
            eligible_targets.iter().next().copied()
        } else {
            None
        };
        let matched_terms = if query.lemmas.is_empty() {
            query.terms.clone()
        } else {
            query.lemmas.clone()
        };
        let mut hits = Vec::with_capacity(exact_candidates.len() + fuzzy_candidates.len());
        for (corpus, target_index) in exact_candidates {
            let direct = corpus.direct_eligible && direct_target == Some(target_index);
            hits.push(LaneHit {
                verse_index: target_index,
                score: 100.0 + query_word_count as f32,
                matched_terms: matched_terms.clone(),
                expanded_terms: vec![format!("translation:{}", corpus.translation)],
                passages: Vec::new(),
                confidence_hint: if direct {
                    Confidence::Recovered
                } else {
                    Confidence::Related
                },
                ranking_tier: if direct { 2 } else { 0 },
                direct_tier: if direct { 2 } else { 0 },
            });
        }
        for (corpus, target_index) in fuzzy_candidates {
            hits.push(LaneHit {
                verse_index: target_index,
                score: 80.0 + (query_word_count - MAX_FUZZY_ALTERNATE_MISMATCHES) as f32,
                matched_terms: matched_terms.clone(),
                expanded_terms: vec![format!("translation-fuzzy:{}", corpus.translation)],
                passages: Vec::new(),
                confidence_hint: Confidence::Related,
                ranking_tier: 0,
                direct_tier: 0,
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
        LaneSearchOutput {
            hits,
            blocks_unique_recovery: ambiguous_complete_corpus,
        }
    }
}

struct ConceptEntry {
    keys: Arc<[String]>,
    event_signatures: Arc<[CompiledEventSignature]>,
    label: Arc<str>,
    source: Arc<str>,
    concept_type: Arc<str>,
    targets: ConceptTargets,
    ranking_tier: u8,
    direct_tier: u8,
}

enum ConceptTargets {
    Ranked {
        verse_index: u32,
        passage: Option<String>,
        passages: Vec<PassageRange>,
    },
    Shadow(Vec<ConceptMention>),
}

struct ConceptMention {
    verse_index: u32,
    passage: String,
    range: PassageRange,
}

/// Source-typed concept provider. Ranked curated/event records use Lane D;
/// BSB events may also use collision-audited morphology/expansion signatures.
/// Heuristic topic/entity records stay unadvertised. Exact multi-word topics
/// may rank as Related when independently corroborated (compositional atoms,
/// multi-source support, or a multi-verse validated range). Entity/event graph
/// records remain score-zero shadow evidence. Ranked BSB events also expose
/// corpus-unique contiguous subphrase surfaces so short traditional titles can
/// activate a longer source event name without prompt-specific aliases.
pub struct ConceptLane {
    entries: Vec<ConceptEntry>,
    lane_id: LaneId,
    evidence_only: bool,
    semantic_expander: Option<Arc<PackedExpandLane>>,
}

impl ConceptLane {
    #[cfg(test)]
    fn new(
        canonical: &[Verse],
        records: Vec<ConceptRecord>,
        lane_id: LaneId,
        evidence_only: bool,
    ) -> Result<Self, String> {
        Self::new_with_expander(canonical, records, lane_id, evidence_only, None)
    }

    fn new_with_expander(
        canonical: &[Verse],
        records: Vec<ConceptRecord>,
        lane_id: LaneId,
        evidence_only: bool,
        semantic_expander: Option<Arc<PackedExpandLane>>,
    ) -> Result<Self, String> {
        let ref_to_index = canonical_ref_map(canonical)?;
        let mut entries = Vec::with_capacity(records.len());

        for record in records {
            let anchor_key = normalize_ref_key(&record.anchor_ref);
            let Some(&verse_index) = ref_to_index.get(&anchor_key) else {
                return Err(format!(
                    "concept {} targets missing base ref {}",
                    record.label, record.anchor_ref
                ));
            };
            let mut keys = vec![
                canonical_surface(&record.key),
                canonical_surface(&record.label),
            ];
            let mut event_signatures = Vec::new();
            for alias in &record.aliases {
                match parse_event_signature_alias(alias)? {
                    Some(signature) => {
                        if record.concept_type != "event" || record.source != EVENT_SIGNATURE_SOURCE
                        {
                            return Err(format!(
                                "generated event signature on untrusted concept {:?}",
                                record.label
                            ));
                        }
                        event_signatures.push(signature);
                    }
                    None => keys.push(canonical_surface(alias)),
                }
            }
            keys.retain(|key| !key.is_empty());
            keys.sort();
            keys.dedup();
            event_signatures.sort_by(|left, right| {
                right
                    .kind
                    .cmp(&left.kind)
                    .then_with(|| right.stems.len().cmp(&left.stems.len()))
                    .then_with(|| left.stems.cmp(&right.stems))
            });
            event_signatures.dedup();
            if keys.is_empty() {
                return Err(format!(
                    "concept at {} has no searchable key",
                    record.anchor_ref
                ));
            }

            if record.target_ranges.is_empty() {
                return Err(format!("concept {} has no target ranges", record.label));
            }
            let mut anchor_is_in_primary = false;
            for (range_index, range) in record.target_ranges.iter().enumerate() {
                let start = ref_to_index
                    .get(&normalize_ref_key(&range.start_ref))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "concept {} range targets missing base ref {}",
                            record.label, range.start_ref
                        )
                    })?;
                let end = ref_to_index
                    .get(&normalize_ref_key(&range.end_ref))
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "concept {} range targets missing base ref {}",
                            record.label, range.end_ref
                        )
                    })?;
                if reference_book_key(&range.start_ref) != reference_book_key(&range.end_ref) {
                    return Err(format!(
                        "concept {} has cross-book target range {}–{}",
                        record.label, range.start_ref, range.end_ref
                    ));
                }
                if start > end {
                    return Err(format!(
                        "concept {} has reversed target range {}–{}",
                        record.label, range.start_ref, range.end_ref
                    ));
                }
                if range_index == 0 {
                    anchor_is_in_primary = start <= verse_index && verse_index <= end;
                }
            }
            if !anchor_is_in_primary {
                return Err(format!(
                    "concept {} anchor {} is outside its primary target range",
                    record.label, record.anchor_ref
                ));
            }

            let (ranking_tier, direct_tier) = match record.concept_type.as_str() {
                "curated" => (5, 5),
                // Raw BSB event annotations are useful exact-name routing
                // evidence, but they are not editorially curated answers.
                // Rank them decisively while keeping the claim Related.
                "event" => (4, 0),
                // Selah's browser topic artifact is a single heuristic
                // association per label, not an authoritative passage answer.
                // It may contribute Related evidence but cannot pin a result.
                "topic" => (0, 0),
                // Entity labels are useful evidence but too collision-prone to pin.
                "entity" | "person" | "place" | "event_shadow" => (0, 0),
                other => {
                    return Err(format!(
                        "concept {} has unsupported concept_type {}",
                        record.label, other
                    ));
                }
            };
            let keys: Arc<[String]> = keys.into();
            let event_signatures: Arc<[CompiledEventSignature]> = event_signatures.into();
            let label: Arc<str> = Arc::from(record.label.as_str());
            let source: Arc<str> = Arc::from(record.source.as_str());
            let concept_type: Arc<str> = Arc::from(record.concept_type.as_str());
            if evidence_only {
                // Keep one concept record and compact mention list. Expanding
                // to per-mention score-zero hits happens only for an exact
                // query, avoiding persistent duplication of labels and keys.
                let mut mentions = Vec::with_capacity(record.target_ranges.len());
                for range in &record.target_ranges {
                    let range_index = ref_to_index
                        .get(&normalize_ref_key(&range.start_ref))
                        .copied()
                        .expect("target range was validated above");
                    let passage = if normalize_ref_key(&range.start_ref)
                        == normalize_ref_key(&range.end_ref)
                    {
                        range.start_ref.clone()
                    } else {
                        format!("{}–{}", range.start_ref, range.end_ref)
                    };
                    mentions.push(ConceptMention {
                        verse_index: range_index,
                        passage,
                        range: PassageRange {
                            start_ref: range.start_ref.clone(),
                            end_ref: range.end_ref.clone(),
                        },
                    });
                }
                entries.push(ConceptEntry {
                    keys,
                    event_signatures,
                    label,
                    source,
                    concept_type,
                    targets: ConceptTargets::Shadow(mentions),
                    ranking_tier,
                    direct_tier,
                });
            } else {
                let passage = record.target_ranges.first().map(|range| {
                    if normalize_ref_key(&range.start_ref) == normalize_ref_key(&range.end_ref) {
                        range.start_ref.clone()
                    } else {
                        format!("{}–{}", range.start_ref, range.end_ref)
                    }
                });
                let passages = record
                    .target_ranges
                    .iter()
                    .map(|range| PassageRange {
                        start_ref: range.start_ref.clone(),
                        end_ref: range.end_ref.clone(),
                    })
                    .collect();
                entries.push(ConceptEntry {
                    keys,
                    event_signatures,
                    label,
                    source,
                    concept_type,
                    targets: ConceptTargets::Ranked {
                        verse_index,
                        passage,
                        passages,
                    },
                    ranking_tier,
                    direct_tier,
                });
            }
        }

        Ok(Self {
            entries,
            lane_id,
            evidence_only,
            semantic_expander,
        })
    }
}

#[derive(Debug, Clone)]
struct EventSignatureMatch {
    kind: EventSignatureKind,
    surface: String,
    exact_morphology: bool,
    stem_count: usize,
    score: f32,
}

impl EventSignatureMatch {
    fn compare_priority(&self, other: &Self) -> core::cmp::Ordering {
        self.exact_morphology
            .cmp(&other.exact_morphology)
            .then_with(|| self.stem_count.cmp(&other.stem_count))
            .then_with(|| self.kind.cmp(&other.kind))
            .then_with(|| {
                self.score
                    .partial_cmp(&other.score)
                    .unwrap_or(core::cmp::Ordering::Equal)
            })
    }
}

fn event_signature_expansion_score(
    signature: &CompiledEventSignature,
    weighted_stems: &[crate::lanes::WeightedStem],
) -> Option<f32> {
    if signature.kind == EventSignatureKind::Single {
        return None;
    }

    // Solve the distinct-query-group assignment as a bounded bitmask DP.
    // Signatures contain at most eight stems, so this is O(groups * 2^8 * 8)
    // even when an adversarially long query makes many groups point at the
    // same cue. The former recursive permutation search was exponential in
    // the number of matching groups.
    let mut candidates_by_group: BTreeMap<u16, BTreeMap<(usize, bool), f32>> = BTreeMap::new();
    for candidate in weighted_stems
        .iter()
        .filter(|candidate| candidate.weight >= MIN_EVENT_SIGNATURE_EXPANSION_WEIGHT)
    {
        let Some(group) = candidate.expansion_group else {
            continue;
        };
        let Ok(signature_index) = signature.stems.binary_search(&candidate.stem) else {
            continue;
        };
        candidates_by_group
            .entry(group)
            .or_default()
            .entry((signature_index, candidate.is_original))
            .and_modify(|weight| *weight = weight.max(candidate.weight))
            .or_insert(candidate.weight);
    }

    const ORIGINAL_FLAG: usize = 0b01;
    const INFERRED_FLAG: usize = 0b10;
    const FLAG_STATES: usize = 4;
    let mask_count = 1usize << signature.stems.len();
    let mut states = vec![None; mask_count * FLAG_STATES];
    states[0] = Some(0.0f32);
    for candidates in candidates_by_group.values() {
        let mut next = states.clone();
        for mask in 0..mask_count {
            for flags in 0..FLAG_STATES {
                let Some(score) = states[mask * FLAG_STATES + flags] else {
                    continue;
                };
                for (&(signature_index, is_original), &weight) in candidates {
                    let bit = 1usize << signature_index;
                    if mask & bit != 0 {
                        continue;
                    }
                    let next_mask = mask | bit;
                    let next_flags = flags
                        | if is_original {
                            ORIGINAL_FLAG
                        } else {
                            INFERRED_FLAG
                        };
                    let slot = &mut next[next_mask * FLAG_STATES + next_flags];
                    *slot =
                        Some(slot.map_or(score + weight, |current| current.max(score + weight)));
                }
            }
        }
        states = next;
    }

    states[(mask_count - 1) * FLAG_STATES + ORIGINAL_FLAG + INFERRED_FLAG]
        .map(|score| score / signature.stems.len() as f32)
}

fn best_event_signature_match(
    entry: &ConceptEntry,
    query_stems: &[String],
    weighted_stems: Option<&[crate::lanes::WeightedStem]>,
) -> Option<EventSignatureMatch> {
    let mut best: Option<EventSignatureMatch> = None;
    for signature in entry.event_signatures.iter() {
        let exact_morphology = signature
            .stems
            .iter()
            .all(|stem| query_stems.binary_search(stem).is_ok());
        let score = if exact_morphology {
            1.0
        } else {
            let Some(weighted_stems) = weighted_stems else {
                continue;
            };
            let Some(score) = event_signature_expansion_score(signature, weighted_stems) else {
                continue;
            };
            score
        };
        let candidate = EventSignatureMatch {
            kind: signature.kind,
            surface: signature.stems.join(" "),
            exact_morphology,
            stem_count: signature.stems.len(),
            score,
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate.compare_priority(current).is_gt())
        {
            best = Some(candidate);
        }
    }
    best
}

impl Lane for ConceptLane {
    fn id(&self) -> LaneId {
        self.lane_id
    }

    fn available(&self) -> bool {
        !self.entries.is_empty()
    }

    fn advertised(&self) -> bool {
        !self.evidence_only && self.available()
    }

    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit> {
        let key = canonical_surface(&query.raw);
        if key.is_empty() {
            return Vec::new();
        }
        let key_words: Vec<String> = key.split_whitespace().map(str::to_string).collect();
        let has_event_signatures = self
            .entries
            .iter()
            .any(|entry| !entry.event_signatures.is_empty());
        let needs_expansion = self.evidence_only || has_event_signatures;
        let semantic_stems = if needs_expansion {
            semantic_query_stems(query)
        } else {
            Vec::new()
        };
        let mut event_query_stems = if has_event_signatures {
            semantic_stems.clone()
        } else {
            Vec::new()
        };
        event_query_stems.sort();
        event_query_stems.dedup();
        let expanded_query = if needs_expansion {
            self.semantic_expander
                .as_ref()
                .map(|expander| expander.expand_stems(&semantic_stems))
        } else {
            None
        };
        let event_weighted_stems = expanded_query
            .as_ref()
            .filter(|expanded| expanded.has_expansions())
            .map(|expanded| expanded.weighted_stems.as_slice());
        let signature_matches = has_event_signatures.then(|| {
            self.entries
                .iter()
                .map(|entry| {
                    best_event_signature_match(entry, &event_query_stems, event_weighted_stems)
                })
                .collect::<Vec<_>>()
        });
        let best_signature = signature_matches
            .as_ref()
            .into_iter()
            .flatten()
            .filter_map(Option::as_ref)
            .max_by(|left, right| left.compare_priority(right));
        let signature_winner = best_signature.and_then(|best| {
            let mut winners =
                signature_matches
                    .as_ref()?
                    .iter()
                    .enumerate()
                    .filter(|(_, candidate)| {
                        candidate
                            .as_ref()
                            .is_some_and(|candidate| candidate.compare_priority(best).is_eq())
                    });
            let winner = winners.next().map(|(index, _)| index)?;
            winners.next().is_none().then_some(winner)
        });
        let exact_topic_support = self
            .entries
            .iter()
            .filter(|entry| {
                entry.concept_type.as_ref() == "topic"
                    && entry.keys.iter().any(|candidate| candidate == &key)
            })
            .count();
        let mut hits = Vec::new();

        for (entry_index, entry) in self.entries.iter().enumerate() {
            let exact_key_match = entry.keys.iter().any(|candidate| candidate == &key);
            let signature_match = signature_winner
                .filter(|winner| *winner == entry_index)
                .and_then(|_| signature_matches.as_ref()?[entry_index].as_ref());
            if !exact_key_match && signature_match.is_none() {
                continue;
            }
            let signature_only_match = (!exact_key_match).then_some(signature_match).flatten();
            // A generic single token should remain lexical. Named graph
            // entities may explain an independently scored hit, but this lane
            // is evidence-only and therefore cannot create or promote one.
            let permits_single_token_shadow =
                matches!(entry.concept_type.as_ref(), "entity" | "person" | "place");
            if key_words.len() < 2
                && entry.concept_type.as_ref() != "curated"
                && !permits_single_token_shadow
                && !signature_only_match
                    .is_some_and(|signature| signature.kind == EventSignatureKind::Single)
            {
                continue;
            }
            let direct_tier = if signature_only_match.is_some() {
                0
            } else {
                entry.direct_tier
            };
            let ranking_tier = signature_only_match.map_or(entry.ranking_tier, |signature| {
                if signature.exact_morphology {
                    3
                } else {
                    2
                }
            });
            let score =
                signature_only_match.map_or(100.0 + entry.ranking_tier as f32, |signature| {
                    if signature.exact_morphology {
                        102.0 + signature.score
                    } else {
                        101.0 + signature.score
                    }
                });
            let mut base_expanded = vec![
                format!("{}:{}", entry.concept_type, entry.label),
                format!("source:{}", entry.source),
            ];
            if let Some(signature) = signature_only_match {
                base_expanded.push(format!(
                    "event-signature:{}:{}",
                    signature.kind.name(),
                    signature.surface
                ));
                base_expanded.push(format!(
                    "event-signature-mode:{}",
                    if signature.exact_morphology {
                        "morphology"
                    } else {
                        "expansion"
                    }
                ));
            }
            let matched_terms =
                if signature_only_match.is_some_and(|signature| !signature.exact_morphology) {
                    Vec::new()
                } else {
                    key_words.clone()
                };
            match &entry.targets {
                ConceptTargets::Ranked {
                    verse_index,
                    passage,
                    passages,
                } => {
                    let mut expanded_terms = base_expanded;
                    if let Some(passage) = passage {
                        expanded_terms.push(format!("passage:{passage}"));
                    }
                    hits.push(LaneHit {
                        verse_index: *verse_index,
                        score,
                        matched_terms,
                        expanded_terms,
                        passages: passages.clone(),
                        confidence_hint: if direct_tier > 0 {
                            Confidence::Recovered
                        } else {
                            Confidence::Related
                        },
                        ranking_tier,
                        direct_tier,
                    });
                }
                ConceptTargets::Shadow(mentions) => {
                    // Exact multi-word topics may create Related candidates when
                    // the source range is itself multi-verse (a discourse unit),
                    // or when independent single-atom topics corroborate the
                    // same target. Single-verse topic noise stays score-zero.
                    let exact_named_topic = entry.concept_type.as_ref() == "topic"
                        && key_words.len() >= 2
                        && (exact_topic_support >= 2
                            || topic_has_compositional_target_support(entry, &key, &self.entries)
                            || topic_has_multi_verse_range(entry));
                    for mention in mentions {
                        let mut expanded_terms = base_expanded.clone();
                        expanded_terms.push(format!("passage:{}", mention.passage));
                        hits.push(LaneHit {
                            verse_index: mention.verse_index,
                            score: if exact_named_topic { 1.0 } else { 0.0 },
                            matched_terms: matched_terms.clone(),
                            expanded_terms,
                            passages: vec![mention.range.clone()],
                            confidence_hint: Confidence::Related,
                            ranking_tier: if exact_named_topic { 1 } else { ranking_tier },
                            direct_tier: 0,
                        });
                    }
                }
            }
        }

        if let Some(expanded) = expanded_query.as_ref() {
            if expanded.has_expansions() {
                let mut semantic_hits = Vec::new();
                for entry in self
                    .entries
                    .iter()
                    .filter(|entry| entry.concept_type.as_ref() == "topic")
                {
                    let Some(score) = entry
                        .keys
                        .iter()
                        .filter_map(|candidate| {
                            semantic_topic_score(candidate, &expanded.weighted_stems)
                        })
                        .max_by(|left, right| {
                            left.partial_cmp(right)
                                .unwrap_or(core::cmp::Ordering::Equal)
                        })
                    else {
                        continue;
                    };
                    let ConceptTargets::Shadow(mentions) = &entry.targets else {
                        continue;
                    };
                    let base_expanded = vec![
                        format!("semantic-topic:{}", entry.label),
                        format!("source:{}", entry.source),
                    ];
                    for mention in mentions {
                        let mut expanded_terms = base_expanded.clone();
                        expanded_terms.push(format!("passage:{}", mention.passage));
                        semantic_hits.push(LaneHit {
                            verse_index: mention.verse_index,
                            score,
                            matched_terms: Vec::new(),
                            expanded_terms,
                            passages: vec![mention.range.clone()],
                            confidence_hint: Confidence::Related,
                            ranking_tier: 0,
                            direct_tier: 0,
                        });
                    }
                }
                // Aliases and independently sourced duplicate labels must not
                // consume multiple rank positions for the same canonical
                // anchor. Keep the strongest score and merge explanations.
                let mut semantic_by_verse: HashMap<u32, LaneHit> = HashMap::new();
                for hit in semantic_hits {
                    match semantic_by_verse.entry(hit.verse_index) {
                        std::collections::hash_map::Entry::Occupied(mut occupied) => {
                            let existing = occupied.get_mut();
                            existing.score = existing.score.max(hit.score);
                            for term in hit.expanded_terms {
                                if !existing.expanded_terms.contains(&term) {
                                    existing.expanded_terms.push(term);
                                }
                            }
                            for passage in hit.passages {
                                if !existing.passages.contains(&passage) {
                                    existing.passages.push(passage);
                                }
                            }
                        }
                        std::collections::hash_map::Entry::Vacant(vacant) => {
                            vacant.insert(hit);
                        }
                    }
                }
                let mut semantic_hits: Vec<LaneHit> = semantic_by_verse.into_values().collect();
                semantic_hits.sort_by(|left, right| {
                    right
                        .score
                        .partial_cmp(&left.score)
                        .unwrap_or(core::cmp::Ordering::Equal)
                        .then_with(|| left.verse_index.cmp(&right.verse_index))
                });
                semantic_hits.truncate(top_k.max(1) * 3);
                hits.extend(semantic_hits);
            }
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
        // Shadow hits are a sparse exact-key lookup and must cover every
        // independently retrieved mention, including late-canon occurrences
        // of very common entities. They do not enter ranking, so output depth
        // is not a valid coverage cap. Ranked concept hits stay bounded.
        if !self.evidence_only {
            hits.truncate(top_k.max(1) * 3);
        }
        hits
    }
}

/// Minimum inclusive verse span for an exact multi-word topic to rank from
/// range evidence alone. Two-verse Selah associations (common for noisy
/// single-target pins expanded by one adjacent verse) stay score-zero;
/// discourse units such as parables and short teaching blocks are longer.
const MIN_TOPIC_DISCOURSE_SPAN: u32 = 3;

/// A multi-verse topic range is stronger than a one-verse heuristic pin when it
/// is long enough to look like a discourse unit rather than a top-1 association
/// with a neighboring verse. Exact multi-word queries against those units may
/// rank as Related without atom-level corroboration.
fn topic_has_multi_verse_range(entry: &ConceptEntry) -> bool {
    let ConceptTargets::Shadow(mentions) = &entry.targets else {
        return false;
    };
    mentions.iter().any(|mention| {
        topic_range_span_verses(&mention.range).is_some_and(|span| span >= MIN_TOPIC_DISCOURSE_SPAN)
    })
}

fn topic_range_span_verses(range: &PassageRange) -> Option<u32> {
    let start = parse_ref_chapter_verse(&range.start_ref)?;
    let end = parse_ref_chapter_verse(&range.end_ref)?;
    if start.0 != end.0 || start.1 != end.1 {
        // Cross-chapter ranges are always discourse-length for this gate.
        return Some(MIN_TOPIC_DISCOURSE_SPAN.max(3));
    }
    if end.2 < start.2 {
        return None;
    }
    Some(end.2 - start.2 + 1)
}

fn parse_ref_chapter_verse(reference: &str) -> Option<(&str, u32, u32)> {
    let (book, chapter_and_verse) = reference.rsplit_once(' ')?;
    let (chapter, verse) = chapter_and_verse.split_once(':')?;
    Some((book, chapter.parse().ok()?, verse.parse().ok()?))
}

/// A compound topic can outrank lexical noise only when every content atom is
/// also present as a genuine one-word topic mapped to exactly the same passage.
/// This admits independently corroborated labels such as “racism and slavery”
/// while keeping isolated or partially supported source noise score-zero.
fn topic_has_compositional_target_support(
    compound: &ConceptEntry,
    compound_key: &str,
    entries: &[ConceptEntry],
) -> bool {
    let ConceptTargets::Shadow(compound_mentions) = &compound.targets else {
        return false;
    };
    let mut stems = normalize_query(compound_key).lemmas;
    stems.sort();
    stems.dedup();
    if !(2..=5).contains(&stems.len()) {
        return false;
    }

    stems.iter().all(|stem| {
        entries.iter().any(|entry| {
            if entry.concept_type.as_ref() != "topic" {
                return false;
            }
            let ConceptTargets::Shadow(mentions) = &entry.targets else {
                return false;
            };
            shadow_mentions_match(compound_mentions, mentions)
                && entry.keys.iter().any(|candidate| {
                    canonical_surface(candidate).split_whitespace().count() == 1
                        && normalize_query(candidate).lemmas == [stem.clone()]
                })
        })
    })
}

fn shadow_mentions_match(left: &[ConceptMention], right: &[ConceptMention]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.verse_index == right.verse_index
                && normalize_ref_key(&left.range.start_ref)
                    == normalize_ref_key(&right.range.start_ref)
                && normalize_ref_key(&left.range.end_ref) == normalize_ref_key(&right.range.end_ref)
        })
}

const MIN_SEMANTIC_TOPIC_WEIGHT: f32 = 0.20;

/// A heuristic topic may rank only when every content word in a short label
/// is supported by a different typed query cue and at least one support edge
/// is inferred. A single-word topic requires two independent cues to converge
/// on that same word. This makes the broad topic inventory a bounded Lane-C
/// bridge without promoting one-cue associations.
fn semantic_topic_score(
    candidate: &str,
    weighted_stems: &[crate::lanes::WeightedStem],
) -> Option<f32> {
    let mut topic_stems = normalize_query(candidate).lemmas;
    topic_stems.sort();
    topic_stems.dedup();
    if topic_stems.len() > 5 || topic_stems.is_empty() {
        return None;
    }

    if topic_stems.len() == 1 {
        // Do not let a stop-word-heavy phrase (for example “being with God”)
        // collapse into a generic singleton. This exception is reserved for
        // labels that are genuinely one lexical word in the source artifact.
        if canonical_surface(candidate).split_whitespace().count() != 1 {
            return None;
        }
        let topic = &topic_stems[0];
        let mut supports: Vec<&crate::lanes::WeightedStem> = weighted_stems
            .iter()
            .filter(|candidate| {
                candidate.stem == *topic
                    && candidate.weight >= MIN_SEMANTIC_TOPIC_WEIGHT
                    && !candidate.is_original
                    && candidate.expansion_group.is_some()
            })
            .collect();
        supports.sort_by(|left, right| {
            right
                .weight
                .partial_cmp(&left.weight)
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        let strongest = supports.first()?;
        let second = supports
            .iter()
            .find(|candidate| candidate.expansion_group != strongest.expansion_group)?;
        // Two independently inferred cues are stronger than a coincidental
        // two-word label match; the bounded corroboration prior reflects that
        // without allowing the score to exceed the lane's normalized range.
        return Some((0.5 + (strongest.weight + second.weight) / 2.0).min(1.0));
    }

    fn assign(
        topic_stems: &[String],
        weighted_stems: &[crate::lanes::WeightedStem],
        topic_index: usize,
        used_groups: &mut Vec<u16>,
        score: f32,
        inferred: bool,
        best: &mut Option<f32>,
    ) {
        if topic_index == topic_stems.len() {
            if inferred {
                *best = Some(best.map_or(score, |current| current.max(score)));
            }
            return;
        }
        for candidate in weighted_stems.iter().filter(|candidate| {
            candidate.stem == topic_stems[topic_index]
                && candidate.weight >= MIN_SEMANTIC_TOPIC_WEIGHT
        }) {
            let Some(group) = candidate.expansion_group else {
                continue;
            };
            if used_groups.contains(&group) {
                continue;
            }
            used_groups.push(group);
            assign(
                topic_stems,
                weighted_stems,
                topic_index + 1,
                used_groups,
                score + candidate.weight,
                inferred || !candidate.is_original,
                best,
            );
            used_groups.pop();
        }
    }

    let mut best = None;
    assign(
        &topic_stems,
        weighted_stems,
        0,
        &mut Vec::with_capacity(topic_stems.len()),
        0.0,
        false,
        &mut best,
    );
    best.map(|score| score / topic_stems.len() as f32)
}

fn canonical_ref_map(verses: &[Verse]) -> Result<HashMap<String, u32>, String> {
    let mut by_ref = HashMap::with_capacity(verses.len());
    for verse in verses {
        let key = normalize_ref_key(&verse.reference);
        if by_ref.insert(key, verse.index).is_some() {
            return Err(format!(
                "base pack contains duplicate normalized ref {}",
                verse.reference
            ));
        }
    }
    Ok(by_ref)
}

fn normalize_ref_key(reference: &str) -> String {
    // Keep token boundaries: deleting punctuation would make Genesis 1:11
    // collide with Genesis 11:1 (both become `genesis111`). Overlay builders
    // emit canonical display refs, so normalized word/number tokens are a
    // stable and unambiguous join key.
    canonical_surface(reference)
}

fn reference_book_key(reference: &str) -> String {
    let surface = canonical_surface(reference);
    let mut parts: Vec<&str> = surface.split_whitespace().collect();
    if parts.len() >= 3 {
        parts.truncate(parts.len() - 2);
    }
    parts.join(" ")
}

fn canonical_tokens(text: &str) -> Vec<String> {
    canonical_surface(text)
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn contains_token_window<T: PartialEq>(haystack: &[T], needle: &[T]) -> bool {
    !needle.is_empty()
        && needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

fn contains_bounded_fuzzy_token_window(haystack: &[u32], needle: &[Option<u32>]) -> bool {
    if needle.len() < MIN_FUZZY_ALTERNATE_WORDS || needle.len() > haystack.len() {
        return false;
    }

    haystack.windows(needle.len()).any(|window| {
        let mut mismatches = 0usize;
        for (actual, expected) in window.iter().zip(needle) {
            if expected != &Some(*actual) {
                mismatches += 1;
                if mismatches > MAX_FUZZY_ALTERNATE_MISMATCHES {
                    return false;
                }
            }
        }
        let exact_words = needle.len() - mismatches;
        mismatches > 0
            && exact_words >= MIN_FUZZY_ALTERNATE_EXACT_WORDS
            && mismatches.saturating_mul(100)
                <= needle
                    .len()
                    .saturating_mul(MAX_FUZZY_ALTERNATE_MISMATCH_PERCENT)
    })
}

/// Exact alias normalization deliberately retains stop words so `the fall`
/// remains distinct from generic `fall`.
fn canonical_surface(text: &str) -> String {
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if matches!(ch, '\'' | '\u{2018}' | '\u{2019}') && !current.is_empty() {
            // Possessives and contractions should not invent a token boundary:
            // `Saul's conversion` must match source label `sauls conversion`.
            continue;
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lanes::WeightedStem;
    use crate::normalize::normalize_query;
    use recall_pack::{
        ExpandEntry, ExpandSection, ExpandTarget, LexSection, LexTerm, OverlayAlternate,
        OverlayConcept, OverlayRange, OverlaySection, PostSection, PostTerm, Posting,
    };

    #[test]
    fn semantic_topics_require_multi_cue_coverage_and_an_inferred_edge() {
        let weighted = vec![
            WeightedStem::original_in_group("depend", 0),
            WeightedStem::expanded_in_group("trust", 0.74, 0),
            WeightedStem::original_in_group("god", 1),
            WeightedStem::expanded_in_group("lord", 0.81, 1),
        ];
        assert!(semantic_topic_score("trust in the lord", &weighted).is_some());
        assert!(semantic_topic_score("trust", &weighted).is_none());

        let one_cue = vec![
            WeightedStem::expanded_in_group("trust", 0.74, 0),
            WeightedStem::expanded_in_group("lord", 0.81, 0),
        ];
        assert!(semantic_topic_score("trust in the lord", &one_cue).is_none());

        let exact_only = vec![
            WeightedStem::original_in_group("trust", 0),
            WeightedStem::original_in_group("lord", 1),
        ];
        assert!(semantic_topic_score("trust in the lord", &exact_only).is_none());

        let convergent_singleton = vec![
            WeightedStem::expanded_in_group("armor", 0.29, 0),
            WeightedStem::expanded_in_group("armor", 0.24, 1),
        ];
        assert!(semantic_topic_score("armor", &convergent_singleton).is_some());
        assert!(semantic_topic_score("the armor", &convergent_singleton).is_none());
        assert!(semantic_topic_score("armor", &convergent_singleton[..1]).is_none());
    }

    #[test]
    fn exact_compound_topic_requires_every_atom_on_the_same_target() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Galatians 3:28".into(),
                text: "There is neither Jew nor Greek, slave nor free.".into(),
            },
            Verse {
                index: 1,
                reference: "Romans 1:26".into(),
                text: "Canonical control one.".into(),
            },
            Verse {
                index: 2,
                reference: "1 Corinthians 6:19".into(),
                text: "Canonical control two.".into(),
            },
        ];
        let topic = |key: &str, reference: &str| ConceptRecord {
            concept_type: "topic".into(),
            key: key.into(),
            label: key.into(),
            aliases: Vec::new(),
            source: "Selah.test".into(),
            anchor_ref: reference.into(),
            target_ranges: vec![TargetRange {
                start_ref: reference.into(),
                end_ref: reference.into(),
            }],
        };
        let lane = ConceptLane::new(
            &canonical,
            vec![
                topic("racism and slavery", "Galatians 3:28"),
                topic("racism", "Galatians 3:28"),
                topic("slavery", "Galatians 3:28"),
                topic("animal molesters", "Romans 1:26"),
                topic("animal", "Romans 1:26"),
                topic("assisted suicide", "1 Corinthians 6:19"),
                topic("suicide", "1 Corinthians 6:19"),
            ],
            LaneId::C,
            true,
        )
        .unwrap();

        let supported = lane.search(&normalize_query("racism and slavery"), 10);
        assert_eq!(supported.len(), 1);
        assert_eq!(supported[0].verse_index, 0);
        assert_eq!(supported[0].score, 1.0);
        assert_eq!(supported[0].ranking_tier, 1);
        assert_eq!(supported[0].confidence_hint, Confidence::Related);

        for unsafe_partial in ["animal molesters", "assisted suicide"] {
            let hits = lane.search(&normalize_query(unsafe_partial), 10);
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0].score, 0.0, "{unsafe_partial}");
            assert_eq!(hits[0].ranking_tier, 0, "{unsafe_partial}");
        }
    }

    #[test]
    fn exact_multi_verse_topic_ranks_related_without_atom_support() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Matthew 5:3".into(),
                text: "Blessed are the poor in spirit.".into(),
            },
            Verse {
                index: 1,
                reference: "Matthew 5:4".into(),
                text: "Blessed are those who mourn.".into(),
            },
            Verse {
                index: 2,
                reference: "Matthew 5:12".into(),
                text: "Rejoice and be glad.".into(),
            },
            Verse {
                index: 3,
                reference: "Romans 1:26".into(),
                text: "Sensitive control start.".into(),
            },
            Verse {
                index: 4,
                reference: "Romans 1:27".into(),
                text: "Sensitive control end.".into(),
            },
        ];
        let lane = ConceptLane::new(
            &canonical,
            vec![
                ConceptRecord {
                    concept_type: "topic".into(),
                    key: "the beatitudes".into(),
                    label: "the beatitudes".into(),
                    aliases: Vec::new(),
                    source: "Selah.test".into(),
                    anchor_ref: "Matthew 5:3".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Matthew 5:3".into(),
                        end_ref: "Matthew 5:12".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "topic".into(),
                    key: "animal molesters".into(),
                    label: "animal molesters".into(),
                    aliases: Vec::new(),
                    source: "Selah.test".into(),
                    anchor_ref: "Romans 1:26".into(),
                    // Two-verse Selah pins must not rank from range evidence alone.
                    target_ranges: vec![TargetRange {
                        start_ref: "Romans 1:26".into(),
                        end_ref: "Romans 1:27".into(),
                    }],
                },
            ],
            LaneId::C,
            true,
        )
        .unwrap();

        let hits = lane.search(&normalize_query("the beatitudes"), 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].verse_index, 0);
        assert_eq!(hits[0].score, 1.0);
        assert_eq!(hits[0].ranking_tier, 1);
        assert_eq!(hits[0].passages[0].end_ref, "Matthew 5:12");

        let unsafe_hits = lane.search(&normalize_query("animal molesters"), 10);
        assert_eq!(unsafe_hits[0].score, 0.0);
        assert_eq!(unsafe_hits[0].ranking_tier, 0);
    }

    #[test]
    fn unique_topic_contained_in_event_activates_ranked_event() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Exodus 19:1".into(),
                text: "On the third month.".into(),
            },
            Verse {
                index: 1,
                reference: "Exodus 20:3".into(),
                text: "You shall have no other gods.".into(),
            },
            Verse {
                index: 2,
                reference: "Exodus 20:6".into(),
                text: "but showing loving devotion.".into(),
            },
            Verse {
                index: 3,
                reference: "Exodus 20:17".into(),
                text: "You shall not covet.".into(),
            },
            Verse {
                index: 4,
                reference: "Mark 7:1".into(),
                text: "The Pharisees gathered.".into(),
            },
            Verse {
                index: 5,
                reference: "Mark 7:15".into(),
                text: "Nothing outside a man.".into(),
            },
        ];
        let overlay = RecallOverlay {
            concepts: vec![
                ConceptRecord {
                    concept_type: "event".into(),
                    key: "ten commandments given".into(),
                    label: "Ten Commandments Given".into(),
                    aliases: Vec::new(),
                    source: "BSB.events".into(),
                    anchor_ref: "Exodus 20:6".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Exodus 19:1".into(),
                        end_ref: "Exodus 20:17".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "event".into(),
                    key: "commandments and tradition discourse".into(),
                    label: "Commandments and Tradition Discourse".into(),
                    aliases: Vec::new(),
                    source: "BSB.events".into(),
                    anchor_ref: "Mark 7:1".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Mark 7:1".into(),
                        end_ref: "Mark 7:15".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "topic".into(),
                    key: "ten commandments".into(),
                    label: "ten commandments".into(),
                    aliases: vec!["the ten commandments".into()],
                    source: "Selah.test".into(),
                    anchor_ref: "Exodus 20:3".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Exodus 20:3".into(),
                        end_ref: "Exodus 20:17".into(),
                    }],
                },
            ],
            alternates: Vec::new(),
        };
        let lanes = overlay
            .into_lanes_with_expansion(&canonical, None)
            .expect("valid overlay");
        let hits: Vec<_> = lanes
            .iter()
            .flat_map(|lane| lane.search(&normalize_query("ten commandments"), 5))
            .collect();
        let event_hit = hits
            .iter()
            .find(|hit| hit.score >= 100.0)
            .expect("topic⊂event alias should rank the BSB event");
        assert_eq!(event_hit.verse_index, 2);
        assert_eq!(event_hit.confidence_hint, Confidence::Related);
        assert_eq!(event_hit.passages[0].start_ref, "Exodus 19:1");
        assert_eq!(event_hit.passages[0].end_ref, "Exodus 20:17");
        assert!(event_hit
            .expanded_terms
            .iter()
            .any(|term| term == "event:Ten Commandments Given"));
    }

    #[test]
    fn engine_open_never_promotes_graph_shadows_or_synthesizes_aliases() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Exodus 2:2".into(),
                text: "She saw that the child was beautiful.".into(),
            },
            Verse {
                index: 1,
                reference: "Exodus 25:1".into(),
                text: "Then the LORD said to Moses about the tabernacle.".into(),
            },
            Verse {
                index: 2,
                reference: "2 Kings 25:1".into(),
                text: "The king marched against Jerusalem and began a siege.".into(),
            },
            Verse {
                index: 3,
                reference: "2 Kings 25:4".into(),
                text: "Then the city was breached.".into(),
            },
            Verse {
                index: 4,
                reference: "Joshua 3:1".into(),
                text: "Israel prepared to cross the Jordan.".into(),
            },
            Verse {
                index: 5,
                reference: "Joshua 3:17".into(),
                text: "All Israel crossed on dry ground.".into(),
            },
        ];
        let overlay = RecallOverlay {
            concepts: vec![
                ConceptRecord {
                    concept_type: "event_shadow".into(),
                    key: "lifetime of moses".into(),
                    label: "Lifetime of Moses".into(),
                    aliases: Vec::new(),
                    source: "Theographic.test".into(),
                    anchor_ref: "Exodus 2:2".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Exodus 2:2".into(),
                        end_ref: "Exodus 2:2".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "event_shadow".into(),
                    key: "tabernacle built".into(),
                    label: "Tabernacle Built".into(),
                    aliases: Vec::new(),
                    source: "Theographic.test".into(),
                    anchor_ref: "Exodus 25:1".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Exodus 25:1".into(),
                        end_ref: "Exodus 25:1".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "event_shadow".into(),
                    key: "siege of jerusalem".into(),
                    label: "Siege of Jerusalem".into(),
                    aliases: Vec::new(),
                    source: "Theographic.test".into(),
                    anchor_ref: "2 Kings 25:1".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "2 Kings 25:1".into(),
                        end_ref: "2 Kings 25:4".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "place".into(),
                    key: "jerusalem".into(),
                    label: "Jerusalem".into(),
                    aliases: Vec::new(),
                    source: "Theographic.test".into(),
                    anchor_ref: "2 Kings 25:1".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "2 Kings 25:1".into(),
                        end_ref: "2 Kings 25:1".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "event".into(),
                    key: "israel crosses the jordan".into(),
                    label: "Israel Crosses the Jordan".into(),
                    aliases: Vec::new(),
                    source: "BSB.events".into(),
                    anchor_ref: "Joshua 3:1".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Joshua 3:1".into(),
                        end_ref: "Joshua 3:17".into(),
                    }],
                },
                ConceptRecord {
                    concept_type: "topic".into(),
                    key: "crosses the jordan".into(),
                    label: "Crosses the Jordan".into(),
                    aliases: Vec::new(),
                    source: "Selah.test".into(),
                    anchor_ref: "Joshua 3:1".into(),
                    target_ranges: vec![TargetRange {
                        start_ref: "Joshua 3:1".into(),
                        end_ref: "Joshua 3:17".into(),
                    }],
                },
            ],
            alternates: Vec::new(),
        };

        let lanes = overlay
            .into_lanes_with_expansion(&canonical, None)
            .expect("valid source-typed overlay");
        assert_eq!(lanes.len(), 2, "one ranked lane and one shadow lane");

        for query in [
            "lifetime of moses",
            "tabernacle built",
            "siege of jerusalem",
            "fall of jerusalem",
        ] {
            let hits: Vec<_> = lanes
                .iter()
                .flat_map(|lane| lane.search(&normalize_query(query), 10))
                .collect();
            assert!(
                hits.iter().all(|hit| hit.score == 0.0),
                "graph-derived query unexpectedly ranked: {query}"
            );
            assert!(hits.iter().all(|hit| {
                !hit.expanded_terms
                    .iter()
                    .any(|term| term.starts_with("event:Fall of "))
            }));
        }

        // A multi-word topic that is a unique contiguous subphrase of one longer
        // BSB event with overlapping ranges is allowed to activate that event.
        let short: Vec<_> = lanes
            .iter()
            .flat_map(|lane| lane.search(&normalize_query("crosses the jordan"), 10))
            .collect();
        assert!(
            short.iter().any(|hit| {
                hit.score > 0.0
                    && hit
                        .expanded_terms
                        .iter()
                        .any(|term| term == "source:BSB.events")
            }),
            "topic⊂event should activate the ranked BSB event: {short:?}"
        );

        let ranked: Vec<_> = lanes
            .iter()
            .flat_map(|lane| lane.search(&normalize_query("israel crosses the jordan"), 10))
            .collect();
        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].score > 0.0);
        assert_eq!(ranked[0].ranking_tier, 4);
        assert!(ranked[0]
            .expanded_terms
            .iter()
            .any(|term| term == "source:BSB.events"));
    }

    #[test]
    fn packed_expansion_can_activate_a_multi_cue_topic_as_related_lane_c() {
        let canonical = vec![Verse {
            index: 0,
            reference: "Proverbs 3:5".into(),
            text: "Trust in the LORD with all your heart.".into(),
        }];
        let lex = LexSection::new(
            1,
            vec![
                LexTerm {
                    text: "lord".into(),
                    document_frequency: 1,
                },
                LexTerm {
                    text: "trust".into(),
                    document_frequency: 1,
                },
            ],
        )
        .unwrap();
        let post = PostSection::new(
            vec![8],
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
            ],
        )
        .unwrap();
        let lexical = Arc::new(
            crate::lanes::PackedLexicalLane::new_owned(lex.encode(), post.encode(), 1).unwrap(),
        );
        let expand = ExpandSection::new(
            "BSB",
            1,
            vec![
                ExpandEntry {
                    source: "depend".into(),
                    targets: vec![ExpandTarget {
                        stem: "trust".into(),
                        weight: 0.7,
                    }],
                },
                ExpandEntry {
                    source: "god".into(),
                    targets: vec![ExpandTarget {
                        stem: "lord".into(),
                        weight: 0.8,
                    }],
                },
            ],
        )
        .unwrap();
        let expander =
            Arc::new(PackedExpandLane::new_owned(expand.encode(), lexical, "BSB", 1).unwrap());
        let lane = ConceptLane::new_with_expander(
            &canonical,
            vec![ConceptRecord {
                concept_type: "topic".into(),
                key: "trust in the lord".into(),
                label: "trust in the lord".into(),
                aliases: vec![],
                source: "Selah.test".into(),
                anchor_ref: "Proverbs 3:5".into(),
                target_ranges: vec![TargetRange {
                    start_ref: "Proverbs 3:5".into(),
                    end_ref: "Proverbs 3:5".into(),
                }],
            }],
            LaneId::C,
            true,
            Some(expander),
        )
        .unwrap();
        let hits = lane.search(&normalize_query("depend on God"), 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].verse_index, 0);
        assert!(hits[0].score > 0.0);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
    }

    fn synthetic_event(
        key: &str,
        label: &str,
        reference: &str,
        signatures: &[&str],
    ) -> ConceptRecord {
        ConceptRecord {
            concept_type: "event".into(),
            key: key.into(),
            label: label.into(),
            aliases: signatures
                .iter()
                .map(|signature| format!("{EVENT_SIGNATURE_ALIAS_PREFIX}{signature}"))
                .collect(),
            source: EVENT_SIGNATURE_SOURCE.into(),
            anchor_ref: reference.into(),
            target_ranges: vec![TargetRange {
                start_ref: reference.into(),
                end_ref: reference.into(),
            }],
        }
    }

    #[test]
    fn generated_event_signatures_cover_morphology_full_partial_and_single_as_related() {
        let canonical: Vec<Verse> = (1..=3)
            .map(|verse| Verse {
                index: verse - 1,
                reference: format!("Example 1:{verse}"),
                text: format!("Synthetic source passage {verse}."),
            })
            .collect();
        let lane = ConceptLane::new(
            &canonical,
            vec![
                synthetic_event(
                    "synthetic alpha",
                    "Synthetic Alpha",
                    "Example 1:1",
                    &["full:artisan labyrinth silver walk"],
                ),
                synthetic_event(
                    "synthetic beta",
                    "Synthetic Beta",
                    "Example 1:2",
                    &["partial:cobalt lantern repair"],
                ),
                synthetic_event(
                    "synthetic gamma",
                    "Synthetic Gamma",
                    "Example 1:3",
                    &["single:bioluminescence"],
                ),
            ],
            LaneId::D,
            false,
        )
        .expect("valid generated signatures");

        let cases = [
            ("the artisan walked through a silver labyrinth", 0),
            ("repairing a cobalt lantern", 1),
            ("bioluminescence", 2),
        ];
        for (query, expected_verse) in cases {
            let hits = lane.search(&normalize_query(query), 10);
            assert_eq!(hits.len(), 1, "{query}");
            assert_eq!(hits[0].verse_index, expected_verse, "{query}");
            assert_eq!(hits[0].confidence_hint, Confidence::Related, "{query}");
            assert_eq!(hits[0].ranking_tier, 3, "{query}");
            assert_eq!(hits[0].direct_tier, 0, "{query}");
            assert!(hits[0]
                .expanded_terms
                .iter()
                .any(|term| term == "event-signature-mode:morphology"));
        }
        assert!(lane.search(&normalize_query("repairing"), 10).is_empty());
    }

    #[test]
    fn ambiguous_equal_event_signatures_are_suppressed() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Example 1:1".into(),
                text: "Synthetic one.".into(),
            },
            Verse {
                index: 1,
                reference: "Example 1:2".into(),
                text: "Synthetic two.".into(),
            },
        ];
        let lane = ConceptLane::new(
            &canonical,
            vec![
                synthetic_event(
                    "synthetic alpha",
                    "Synthetic Alpha",
                    "Example 1:1",
                    &["partial:cobalt lantern repair"],
                ),
                synthetic_event(
                    "synthetic beta",
                    "Synthetic Beta",
                    "Example 1:2",
                    &["partial:cobalt lantern restore"],
                ),
            ],
            LaneId::D,
            false,
        )
        .unwrap();

        assert!(lane
            .search(
                &normalize_query("repair and restore the cobalt lantern"),
                10,
            )
            .is_empty());
    }

    #[test]
    fn event_signature_expansion_requires_original_and_inferred_cues() {
        let canonical = vec![Verse {
            index: 0,
            reference: "Example 1:1".into(),
            text: "A synthetic source passage.".into(),
        }];
        let lex = LexSection::new(
            1,
            vec![LexTerm {
                text: "map".into(),
                document_frequency: 1,
            }],
        )
        .unwrap();
        let post = PostSection::new(
            vec![5],
            vec![PostTerm {
                postings: vec![Posting {
                    document_id: 0,
                    term_frequency: 1,
                }],
            }],
        )
        .unwrap();
        let lexical = Arc::new(
            crate::lanes::PackedLexicalLane::new_owned(lex.encode(), post.encode(), 1).unwrap(),
        );
        let expand = ExpandSection::new(
            "BSB",
            1,
            vec![
                ExpandEntry {
                    source: "chart".into(),
                    targets: vec![ExpandTarget {
                        stem: "map".into(),
                        weight: 0.8,
                    }],
                },
                ExpandEntry {
                    source: "navigator".into(),
                    targets: vec![ExpandTarget {
                        stem: "cartographer".into(),
                        weight: 0.8,
                    }],
                },
                ExpandEntry {
                    source: "conceal".into(),
                    targets: vec![ExpandTarget {
                        stem: "hidden".into(),
                        weight: 0.8,
                    }],
                },
                ExpandEntry {
                    source: "island".into(),
                    targets: vec![ExpandTarget {
                        stem: "archipelago".into(),
                        weight: 0.8,
                    }],
                },
            ],
        )
        .unwrap();
        let expander =
            Arc::new(PackedExpandLane::new_owned(expand.encode(), lexical, "BSB", 1).unwrap());
        let lane = ConceptLane::new_with_expander(
            &canonical,
            vec![synthetic_event(
                "synthetic voyage",
                "Synthetic Voyage",
                "Example 1:1",
                &["full:archipelago cartographer hidden map"],
            )],
            LaneId::D,
            false,
            Some(expander),
        )
        .unwrap();

        let hits = lane.search(
            &normalize_query("the cartographer charts a hidden archipelago"),
            10,
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
        assert_eq!(hits[0].ranking_tier, 2);
        assert_eq!(hits[0].direct_tier, 0);
        assert!(hits[0].matched_terms.is_empty());
        assert!(hits[0]
            .expanded_terms
            .iter()
            .any(|term| term == "event-signature-mode:expansion"));

        assert!(lane
            .search(&normalize_query("a navigator charts concealed islands"), 10,)
            .is_empty());
    }

    #[test]
    fn generated_event_signature_collisions_and_untrusted_markers_fail_closed() {
        let event = |label: &str, reference: &str, concept_type: &str| OverlayConcept {
            concept_type: concept_type.into(),
            key: label.to_lowercase(),
            label: label.into(),
            aliases: vec![format!(
                "{EVENT_SIGNATURE_ALIAS_PREFIX}partial:cobalt lantern repair"
            )],
            source: if concept_type == "event" {
                EVENT_SIGNATURE_SOURCE.into()
            } else {
                "Theographic.test".into()
            },
            anchor_ref: reference.into(),
            target_ranges: vec![OverlayRange {
                start_ref: reference.into(),
                end_ref: reference.into(),
            }],
        };
        let collision = OverlaySection {
            canonical_translation: "BSB".into(),
            canonical_verse_count: 2,
            concepts: vec![
                event("Synthetic Alpha", "Example 1:1", "event"),
                event("Synthetic Beta", "Example 1:2", "event"),
            ],
            alternates: Vec::new(),
        }
        .encode()
        .unwrap();
        let error = RecallOverlay::parse(&[collision], "BSB", 2).unwrap_err();
        assert!(error.contains("generated event signature \"cobalt lantern repair\" conflicts"));

        let untrusted = OverlaySection {
            canonical_translation: "BSB".into(),
            canonical_verse_count: 2,
            concepts: vec![event("Synthetic Shadow", "Example 1:1", "event_shadow")],
            alternates: Vec::new(),
        }
        .encode()
        .unwrap();
        let error = RecallOverlay::parse(&[untrusted], "BSB", 2).unwrap_err();
        assert!(error.contains("generated event signature on untrusted concept"));

        let unknown_marker = OverlaySection {
            canonical_translation: "BSB".into(),
            canonical_verse_count: 2,
            concepts: vec![OverlayConcept {
                concept_type: "event".into(),
                key: "synthetic marker".into(),
                label: "Synthetic Marker".into(),
                aliases: vec!["event-signature-v2:full:cobalt lantern repair".into()],
                source: EVENT_SIGNATURE_SOURCE.into(),
                anchor_ref: "Example 1:1".into(),
                target_ranges: vec![OverlayRange {
                    start_ref: "Example 1:1".into(),
                    end_ref: "Example 1:1".into(),
                }],
            }],
            alternates: Vec::new(),
        }
        .encode()
        .unwrap();
        let error = RecallOverlay::parse(&[unknown_marker], "BSB", 2).unwrap_err();
        assert!(error.contains("unknown reserved event signature marker"));
    }

    #[test]
    fn generated_event_signature_cannot_shadow_an_exact_ranked_surface() {
        let exact = OverlayConcept {
            concept_type: "event".into(),
            key: "first event".into(),
            label: "First Event".into(),
            aliases: vec!["cobalt lantern repair".into()],
            source: EVENT_SIGNATURE_SOURCE.into(),
            anchor_ref: "Example 1:1".into(),
            target_ranges: vec![OverlayRange {
                start_ref: "Example 1:1".into(),
                end_ref: "Example 1:1".into(),
            }],
        };
        let generated = OverlayConcept {
            concept_type: "event".into(),
            key: "second event".into(),
            label: "Second Event".into(),
            aliases: vec![format!(
                "{EVENT_SIGNATURE_ALIAS_PREFIX}partial:cobalt lantern repair"
            )],
            source: EVENT_SIGNATURE_SOURCE.into(),
            anchor_ref: "Example 1:2".into(),
            target_ranges: vec![OverlayRange {
                start_ref: "Example 1:2".into(),
                end_ref: "Example 1:2".into(),
            }],
        };
        let section = OverlaySection {
            canonical_translation: "BSB".into(),
            canonical_verse_count: 2,
            concepts: vec![exact, generated],
            alternates: Vec::new(),
        }
        .encode()
        .unwrap();
        let error = RecallOverlay::parse(&[section], "BSB", 2).unwrap_err();
        assert!(error.contains("conflicts with ranked concept surface"));
    }

    fn binary_fixture() -> Vec<u8> {
        OverlaySection {
            canonical_translation: "BSB".into(),
            canonical_verse_count: 2,
            concepts: vec![OverlayConcept {
                concept_type: "event".into(),
                key: "the fall".into(),
                label: "The Fall".into(),
                aliases: vec!["fall of humanity".into()],
                source: "test".into(),
                anchor_ref: "Genesis 1:1".into(),
                target_ranges: vec![OverlayRange {
                    start_ref: "Genesis 1:1".into(),
                    end_ref: "Genesis 1:2".into(),
                }],
            }],
            alternates: vec![OverlayAlternate {
                source_translation: "KJV".into(),
                target_ref: "Genesis 1:1".into(),
                text: "In the beginning God created the heaven and the earth.".into(),
            }],
        }
        .encode()
        .unwrap()
    }

    fn legacy_fixture() -> Vec<u8> {
        concat!(
            "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
            "\"concept_sources\":[{\"concept_type\":\"event\",\"record_count\":1,\"alias_count\":1}],",
            "\"alternate_sources\":[{\"translation\":\"KJV\",\"matched_verse_count\":1}]}\n",
            "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"the fall\",",
            "\"label\":\"The Fall\",\"aliases\":[\"fall of humanity\"],\"source\":\"test\",\"anchor_ref\":\"Genesis 1:1\",",
            "\"target_ranges\":[{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:2\"}]}\n",
            "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",",
            "\"target_ref\":\"Genesis 1:1\",\"text\":\"In the beginning God created the heaven and the earth.\"}\n"
        )
        .as_bytes()
        .to_vec()
    }

    #[test]
    fn binary_and_legacy_json_have_identical_runtime_records() {
        let binary = RecallOverlay::parse(&[binary_fixture()], "BSB", 2).unwrap();
        let legacy = RecallOverlay::parse(&[legacy_fixture()], "BSB", 2).unwrap();
        assert_eq!(binary.concepts, legacy.concepts);
        assert_eq!(binary.alternates, legacy.alternates);

        let mixed = RecallOverlay::parse(&[binary_fixture(), legacy_fixture()], "BSB", 2).unwrap();
        assert_eq!(mixed.concepts.len(), 1, "mixed formats still deduplicate");
        assert_eq!(mixed.alternates.len(), 1);
    }

    #[test]
    fn binary_overlay_validates_target_and_rejects_bad_framing() {
        let bytes = binary_fixture();
        let translation = RecallOverlay::parse(std::slice::from_ref(&bytes), "KJV", 2).unwrap_err();
        assert!(translation.contains("overlay targets BSB but base pack is KJV"));
        let count = RecallOverlay::parse(std::slice::from_ref(&bytes), "BSB", 3).unwrap_err();
        assert!(count.contains("overlay verse count 2 does not match base pack 3"));

        let truncated = bytes[..bytes.len() - 1].to_vec();
        assert!(RecallOverlay::parse(&[truncated], "BSB", 2)
            .unwrap_err()
            .contains("invalid binary overlay"));
        let mut trailing = bytes;
        trailing.push(0);
        assert!(RecallOverlay::parse(&[trailing], "BSB", 2)
            .unwrap_err()
            .contains("invalid binary overlay"));
    }

    #[test]
    fn exact_surface_keeps_articles() {
        assert_eq!(canonical_surface("The Fall"), "the fall");
        assert_ne!(canonical_surface("The Fall"), canonical_surface("fall"));
        assert_eq!(canonical_surface("Saul's conversion"), "sauls conversion");
        assert_eq!(canonical_surface("father’s house"), "fathers house");
    }

    #[test]
    fn reference_keys_preserve_chapter_verse_boundaries() {
        assert_ne!(
            normalize_ref_key("Genesis 1:11"),
            normalize_ref_key("Genesis 11:1")
        );
        assert_eq!(
            normalize_ref_key("1 Corinthians 13:4"),
            "1 corinthians 13 4"
        );
    }

    #[test]
    fn alternate_phrase_must_be_selective() {
        let canonical: Vec<Verse> = (1..=5)
            .map(|verse| Verse {
                index: verse - 1,
                reference: format!("Genesis 1:{verse}"),
                text: format!("Canonical verse {verse}"),
            })
            .collect();
        let mut records: Vec<AlternateTextRecord> = (1..=4)
            .map(|verse| AlternateTextRecord {
                source_translation: "KJV".into(),
                target_ref: format!("Genesis 1:{verse}"),
                text: "And it came to pass in those days.".into(),
            })
            .collect();
        records.push(AlternateTextRecord {
            source_translation: "KJV".into(),
            target_ref: "Genesis 1:5".into(),
            text: "Faith is the substance of things hoped for.".into(),
        });
        let lane = AlternateTextLane::new(&canonical, records).expect("alternate lane");

        assert!(lane
            .search(&normalize_query("and it came to pass"), 10)
            .is_empty());
        let unique = lane.search(
            &normalize_query("faith is the substance of things hoped for"),
            10,
        );
        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].verse_index, 4);
        assert_eq!(unique[0].confidence_hint, Confidence::Recovered);
        assert!(lane
            .search(&normalize_query("aith is the substance"), 10)
            .is_empty());
        assert!(lane
            .search(&normalize_query("faith is the sub"), 10)
            .is_empty());
    }

    #[test]
    fn alternate_phrase_allows_one_bounded_substitution_as_related_only() {
        let canonical: Vec<Verse> = (1..=4)
            .map(|verse| Verse {
                index: verse - 1,
                reference: format!("Genesis 1:{verse}"),
                text: format!("Canonical sample {verse}"),
            })
            .collect();
        let alternate_texts = [
            "Steady hope remains inside always",
            "Distant lanterns illuminate quiet roads at midnight",
            "Patient gardeners tend young orchards every morning",
            "Winter rivers travel beneath silent wooden bridges",
        ];
        let records: Vec<AlternateTextRecord> = alternate_texts
            .iter()
            .enumerate()
            .map(|(index, text)| AlternateTextRecord {
                source_translation: "TEST".into(),
                target_ref: format!("Genesis 1:{}", index + 1),
                text: (*text).into(),
            })
            .collect();
        let lane = AlternateTextLane::new(&canonical, records).expect("alternate lane");

        let exact = lane.search(&normalize_query("steady hope remains inside always"), 3);
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].confidence_hint, Confidence::Recovered);
        assert_eq!(exact[0].ranking_tier, 2);
        assert_eq!(exact[0].expanded_terms, vec!["translation:TEST"]);

        // `beyond` is absent from the alternate dictionary. An unknown query
        // token therefore consumes the one permitted substitution rather than
        // causing the whole query to be discarded.
        let fuzzy = lane.search(&normalize_query("steady hope remains beyond always"), 3);
        assert_eq!(fuzzy.len(), 1);
        assert_eq!(fuzzy[0].verse_index, 0);
        assert_eq!(fuzzy[0].confidence_hint, Confidence::Related);
        assert_eq!(fuzzy[0].ranking_tier, 0);
        assert_eq!(fuzzy[0].direct_tier, 0);
        assert_eq!(fuzzy[0].expanded_terms, vec!["translation-fuzzy:TEST"]);
        assert!(fuzzy[0].score < exact[0].score);

        assert!(lane
            .search(&normalize_query("steady courage remains beyond always"), 3,)
            .is_empty());
    }

    #[test]
    fn exact_alternate_surface_suppresses_nearby_fuzzy_candidates() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Genesis 1:1".into(),
                text: "Canonical sample one".into(),
            },
            Verse {
                index: 1,
                reference: "Genesis 1:2".into(),
                text: "Canonical sample two".into(),
            },
        ];
        let records = vec![
            AlternateTextRecord {
                source_translation: "TEST".into(),
                target_ref: "Genesis 1:1".into(),
                text: "Silver birds cross quiet valleys".into(),
            },
            AlternateTextRecord {
                source_translation: "TEST".into(),
                target_ref: "Genesis 1:2".into(),
                text: "Silver birds cross quiet rivers".into(),
            },
        ];
        let lane = AlternateTextLane::new(&canonical, records).expect("alternate lane");
        let hits = lane.search(&normalize_query("silver birds cross quiet valleys"), 3);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].verse_index, 0);
        assert_eq!(hits[0].confidence_hint, Confidence::Recovered);
        assert_eq!(hits[0].expanded_terms, vec!["translation:TEST"]);
    }

    #[test]
    fn repeated_fuzzy_alternate_surface_is_suppressed() {
        let canonical: Vec<Verse> = (1..=4)
            .map(|verse| Verse {
                index: verse - 1,
                reference: format!("Genesis 1:{verse}"),
                text: format!("Canonical sample {verse}"),
            })
            .collect();
        let records: Vec<AlternateTextRecord> = (1..=4)
            .map(|verse| AlternateTextRecord {
                source_translation: "TEST".into(),
                target_ref: format!("Genesis 1:{verse}"),
                text: "Steady hope remains inside always".into(),
            })
            .collect();
        let lane = AlternateTextLane::new(&canonical, records).expect("alternate lane");
        let output =
            lane.search_with_risk(&normalize_query("steady hope remains beyond always"), 10);

        assert!(output.hits.is_empty());
        assert!(
            !output.blocks_unique_recovery,
            "ambiguous approximate evidence must not demote independent exact evidence"
        );
    }

    #[test]
    fn repeated_alternate_phrase_is_rankable_but_not_recovered() {
        let canonical: Vec<Verse> = (1..=2)
            .map(|verse| Verse {
                index: verse - 1,
                reference: format!("Genesis 1:{verse}"),
                text: format!("Canonical verse {verse}"),
            })
            .collect();
        let records: Vec<AlternateTextRecord> = (1..=2)
            .map(|verse| AlternateTextRecord {
                source_translation: "KJV".into(),
                target_ref: format!("Genesis 1:{verse}"),
                text: "In my father's house there was room.".into(),
            })
            .collect();
        let lane = AlternateTextLane::new(&canonical, records).expect("alternate lane");
        let hits = lane.search(&normalize_query("in my father's house"), 10);

        assert_eq!(hits.len(), 2);
        assert!(hits
            .iter()
            .all(|hit| hit.confidence_hint == Confidence::Related));
    }

    #[test]
    fn sparse_alternate_corpus_cannot_claim_direct_recovery() {
        let canonical: Vec<Verse> = (1..=20)
            .map(|verse| Verse {
                index: verse - 1,
                reference: format!("Genesis 1:{verse}"),
                text: format!("Canonical verse {verse}"),
            })
            .collect();
        let records = vec![AlternateTextRecord {
            source_translation: "TEST".into(),
            target_ref: "Genesis 1:1".into(),
            text: "A supposedly unique four word phrase.".into(),
        }];
        let lane = AlternateTextLane::new(&canonical, records).expect("sparse research lane");
        let hits = lane.search(&normalize_query("supposedly unique four word phrase"), 3);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
        assert_eq!(hits[0].ranking_tier, 0);
        assert_eq!(hits[0].direct_tier, 0);
    }

    #[test]
    fn hidden_translations_must_agree_on_unique_target_for_recovery() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Genesis 1:1".into(),
                text: "Canonical one".into(),
            },
            Verse {
                index: 1,
                reference: "Genesis 1:2".into(),
                text: "Canonical two".into(),
            },
        ];
        let records = vec![
            AlternateTextRecord {
                source_translation: "KJV".into(),
                target_ref: "Genesis 1:1".into(),
                text: "Unique hidden phrase here now".into(),
            },
            AlternateTextRecord {
                source_translation: "KJV".into(),
                target_ref: "Genesis 1:2".into(),
                text: "KJV filler words only here".into(),
            },
            AlternateTextRecord {
                source_translation: "ESV".into(),
                target_ref: "Genesis 1:1".into(),
                text: "ESV filler words only here".into(),
            },
            AlternateTextRecord {
                source_translation: "ESV".into(),
                target_ref: "Genesis 1:2".into(),
                text: "Unique hidden phrase here now".into(),
            },
        ];
        let lane = AlternateTextLane::new(&canonical, records).expect("two complete alternates");
        let hits = lane.search(&normalize_query("unique hidden phrase here now"), 3);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|hit| {
            hit.confidence_hint == Confidence::Related
                && hit.ranking_tier == 0
                && hit.direct_tier == 0
        }));
    }

    #[test]
    fn ambiguous_complete_translation_blocks_unique_other_translation() {
        let canonical: Vec<Verse> = (1..=4)
            .map(|verse| Verse {
                index: verse - 1,
                reference: format!("Genesis 1:{verse}"),
                text: format!("Canonical {verse}"),
            })
            .collect();
        let mut records: Vec<AlternateTextRecord> = (1..=4)
            .map(|verse| AlternateTextRecord {
                source_translation: "KJV".into(),
                target_ref: format!("Genesis 1:{verse}"),
                text: "Repeated hidden phrase here now".into(),
            })
            .collect();
        records.extend((1..=4).map(|verse| AlternateTextRecord {
            source_translation: "ESV".into(),
            target_ref: format!("Genesis 1:{verse}"),
            text: if verse == 1 {
                "Repeated hidden phrase here now".into()
            } else {
                format!("Distinct ESV filler text number {verse}")
            },
        }));
        let lane = AlternateTextLane::new(&canonical, records).expect("complete alternates");
        let output = lane.search_with_risk(&normalize_query("repeated hidden phrase here now"), 3);
        assert!(output.blocks_unique_recovery);
        let hits = output.hits;

        assert_eq!(hits.len(), 1, "ambiguous KJV candidates are suppressed");
        assert_eq!(hits[0].verse_index, 0);
        assert_eq!(hits[0].confidence_hint, Confidence::Related);
        assert_eq!(hits[0].ranking_tier, 0);
    }

    #[test]
    fn manifest_counts_reject_truncated_overlay() {
        let truncated = concat!(
            "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
            "\"concept_sources\":[{\"concept_type\":\"event\",\"record_count\":2}],\"alternate_sources\":[]}\n",
            "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"one\",\"label\":\"One\",",
            "\"aliases\":[],\"source\":\"test\",\"anchor_ref\":\"Genesis 1:1\",",
            "\"target_ranges\":[{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:2\"}]}\n"
        )
        .as_bytes()
        .to_vec();

        let error = RecallOverlay::parse(&[truncated], "BSB", 2)
            .expect_err("manifest count mismatch must fail");
        assert!(error.contains("concept counts do not match manifest"));
    }

    #[test]
    fn manifest_counts_reject_missing_generated_event_signatures() {
        let dirty = concat!(
            "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
            "\"concept_sources\":[{\"concept_type\":\"event\",\"record_count\":1,\"alias_count\":1,\"generated_signature_count\":2,\"signature_normalizer_version\":2}],\"alternate_sources\":[]}\n",
            "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"synthetic event\",",
            "\"label\":\"Synthetic Event\",\"aliases\":[\"event-signature-v1:full:cobalt lantern repair\"],",
            "\"source\":\"BSB.events\",\"anchor_ref\":\"Example 1:1\",",
            "\"target_ranges\":[{\"start_ref\":\"Example 1:1\",\"end_ref\":\"Example 1:1\"}]}\n"
        )
        .as_bytes()
        .to_vec();

        let error = RecallOverlay::parse(&[dirty], "BSB", 2)
            .expect_err("generated signature count mismatch must fail");
        assert!(error.contains("generated event signature counts do not match manifest"));
    }

    #[test]
    fn manifest_rejects_missing_event_signature_normalizer_identity() {
        let dirty = concat!(
            "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
            "\"concept_sources\":[{\"concept_type\":\"event\",\"record_count\":1,\"alias_count\":1,\"generated_signature_count\":1}],\"alternate_sources\":[]}\n",
            "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"synthetic event\",",
            "\"label\":\"Synthetic Event\",\"aliases\":[\"event-signature-v1:full:cobalt lantern repair\"],",
            "\"source\":\"BSB.events\",\"anchor_ref\":\"Example 1:1\",",
            "\"target_ranges\":[{\"start_ref\":\"Example 1:1\",\"end_ref\":\"Example 1:1\"}]}\n"
        )
        .as_bytes()
        .to_vec();

        let error = RecallOverlay::parse(&[dirty], "BSB", 2)
            .expect_err("missing signature normalizer identity must fail");
        assert!(error.contains("incompatible or missing normalizer version"));
    }

    #[test]
    fn conflicting_ranked_surfaces_fail_but_identical_sections_dedupe() {
        let manifest = concat!(
            "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
            "\"concept_sources\":[{\"concept_type\":\"curated\",\"record_count\":1,\"alias_count\":1}],\"alternate_sources\":[]}\n"
        );
        let first = format!(
            "{manifest}{}",
            concat!(
                "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"curated\",\"key\":\"first\",\"label\":\"First\",",
                "\"aliases\":[\"shared story\"],\"source\":\"test\",\"anchor_ref\":\"Genesis 1:1\",",
                "\"target_ranges\":[{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:1\"}]}\n"
            )
        )
        .into_bytes();
        let second = format!(
            "{manifest}{}",
            concat!(
                "{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"curated\",\"key\":\"second\",\"label\":\"Second\",",
                "\"aliases\":[\"shared story\"],\"source\":\"test\",\"anchor_ref\":\"Genesis 1:2\",",
                "\"target_ranges\":[{\"start_ref\":\"Genesis 1:2\",\"end_ref\":\"Genesis 1:2\"}]}\n"
            )
        )
        .into_bytes();
        let error = RecallOverlay::parse(&[first.clone(), second], "BSB", 2)
            .expect_err("conflicting alias targets must fail");
        assert!(error.contains("ranked concept surface \"shared story\" conflicts"));

        let deduped = RecallOverlay::parse(&[first.clone(), first], "BSB", 2)
            .expect("identical repeated mapping is allowed");
        assert_eq!(deduped.concepts.len(), 1);
    }

    fn concept_record(anchor_ref: &str, start_ref: &str, end_ref: &str) -> ConceptRecord {
        ConceptRecord {
            concept_type: "event".into(),
            key: "test event".into(),
            label: "Test Event".into(),
            aliases: Vec::new(),
            source: "test".into(),
            anchor_ref: anchor_ref.into(),
            target_ranges: vec![TargetRange {
                start_ref: start_ref.into(),
                end_ref: end_ref.into(),
            }],
        }
    }

    #[test]
    fn concept_ranges_must_be_ordered_single_book_and_cover_anchor() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Genesis 1:1".into(),
                text: "One".into(),
            },
            Verse {
                index: 1,
                reference: "Genesis 1:2".into(),
                text: "Two".into(),
            },
            Verse {
                index: 2,
                reference: "Exodus 1:1".into(),
                text: "Three".into(),
            },
        ];

        let reversed = ConceptLane::new(
            &canonical,
            vec![concept_record("Genesis 1:1", "Genesis 1:2", "Genesis 1:1")],
            LaneId::D,
            false,
        )
        .err()
        .expect("reversed range must fail");
        assert!(reversed.contains("reversed target range"));

        let cross_book = ConceptLane::new(
            &canonical,
            vec![concept_record("Genesis 1:1", "Genesis 1:1", "Exodus 1:1")],
            LaneId::D,
            false,
        )
        .err()
        .expect("cross-book range must fail");
        assert!(cross_book.contains("cross-book target range"));

        let orphan = ConceptLane::new(
            &canonical,
            vec![concept_record("Genesis 1:1", "Genesis 1:2", "Genesis 1:2")],
            LaneId::D,
            false,
        )
        .err()
        .expect("orphan anchor must fail");
        assert!(orphan.contains("outside its primary target range"));
    }

    fn reviewed_correction_source() -> String {
        format!(
            "correction-v1|id=aquifer.test-1|dataset=BibleAquifer|record=42|revision=db-test|license=CC-BY-SA-4.0|content-sha256={}|review-status=approved|review-policy=source-backed-correction-v1|reviewed-by=editor-1|reviewed-at=2026-07-16",
            "ab".repeat(32)
        )
    }

    fn correction_record(key: &str) -> ConceptRecord {
        ConceptRecord {
            concept_type: "correction".into(),
            key: key.into(),
            label: "The source text does not state this detail".into(),
            aliases: vec!["assumed crowd size".into()],
            source: reviewed_correction_source(),
            anchor_ref: "Matthew 1:1".into(),
            target_ranges: vec![TargetRange {
                start_ref: "Matthew 1:1".into(),
                end_ref: "Matthew 1:2".into(),
            }],
        }
    }

    #[test]
    fn reviewed_correction_matches_only_complete_reviewed_claim_surfaces() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Matthew 1:1".into(),
                text: "A canonical genealogy begins here.".into(),
            },
            Verse {
                index: 1,
                reference: "Matthew 1:2".into(),
                text: "Abraham was the father of Isaac.".into(),
            },
        ];
        let lane = CorrectionLane::new(
            &canonical,
            &[],
            vec![correction_record("invented count detail")],
        )
        .expect("reviewed correction");

        let hits = lane.search(&normalize_query("Invented count detail!"), 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].confidence_hint, Confidence::OftenMisremembered);
        assert_eq!(hits[0].verse_index, 0);
        assert!(hits[0]
            .expanded_terms
            .iter()
            .any(|term| term.starts_with("source:correction-v1|")));
        assert!(lane
            .search(&normalize_query("invented count"), 10)
            .is_empty());
        assert!(lane
            .search(&normalize_query("a different assumed count"), 10)
            .is_empty());
    }

    #[test]
    fn correction_rejects_any_claim_surface_found_in_scripture_text() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Matthew 1:1".into(),
                text: "Love one another as I have loved you.".into(),
            },
            Verse {
                index: 1,
                reference: "Matthew 1:2".into(),
                text: "A canonical control verse.".into(),
            },
        ];
        let canonical_error =
            CorrectionLane::new(&canonical, &[], vec![correction_record("love one another")])
                .err()
                .expect("canonical phrase must be protected");
        assert!(canonical_error.contains("occurs in canonical or alternate Scripture"));

        let mut hidden = correction_record("hidden translation wording");
        hidden.aliases.clear();
        let alternate_error = CorrectionLane::new(
            &canonical,
            &[AlternateTextRecord {
                source_translation: "TEST".into(),
                target_ref: "Matthew 1:1".into(),
                text: "This contains the hidden translation wording exactly.".into(),
            }],
            vec![hidden],
        )
        .err()
        .expect("alternate phrase must be protected");
        assert!(alternate_error.contains("occurs in canonical or alternate Scripture"));
    }

    #[test]
    fn correction_rejects_unreviewed_or_unlicensed_provenance() {
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Matthew 1:1".into(),
                text: "A canonical genealogy begins here.".into(),
            },
            Verse {
                index: 1,
                reference: "Matthew 1:2".into(),
                text: "Abraham was the father of Isaac.".into(),
            },
        ];
        let mut unreviewed = correction_record("invented count detail");
        unreviewed.source = unreviewed
            .source
            .replace("review-status=approved", "review-status=candidate");
        let error = CorrectionLane::new(&canonical, &[], vec![unreviewed])
            .err()
            .expect("candidate record must not enter runtime");
        assert!(error.contains("not approved"));

        let mut unlicensed = correction_record("invented count detail");
        unlicensed.source = unlicensed.source.replace("CC-BY-SA-4.0", "unknown-license");
        let error = CorrectionLane::new(&canonical, &[], vec![unlicensed])
            .err()
            .expect("unlicensed record must not enter runtime");
        assert!(error.contains("unsupported correction source license"));
    }

    #[test]
    fn binary_overlay_preserves_reviewed_correction_runtime_behavior() {
        let section = OverlaySection {
            canonical_translation: "BSB".into(),
            canonical_verse_count: 2,
            concepts: vec![OverlayConcept {
                concept_type: "correction".into(),
                key: "invented count detail".into(),
                label: "The source text does not state this detail".into(),
                aliases: vec!["assumed crowd size".into()],
                source: reviewed_correction_source(),
                anchor_ref: "Matthew 1:1".into(),
                target_ranges: vec![OverlayRange {
                    start_ref: "Matthew 1:1".into(),
                    end_ref: "Matthew 1:2".into(),
                }],
            }],
            alternates: Vec::new(),
        };
        let bytes = section.encode().expect("binary correction overlay");
        let overlay = RecallOverlay::parse(&[bytes], "BSB", 2).expect("parse correction overlay");
        let canonical = vec![
            Verse {
                index: 0,
                reference: "Matthew 1:1".into(),
                text: "A canonical genealogy begins here.".into(),
            },
            Verse {
                index: 1,
                reference: "Matthew 1:2".into(),
                text: "Abraham was the father of Isaac.".into(),
            },
        ];
        let lanes = overlay
            .into_lanes_with_expansion(&canonical, None)
            .expect("instantiate correction lane");
        assert_eq!(lanes.len(), 1);
        let hits = lanes[0].search(&normalize_query("invented count detail"), 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].confidence_hint, Confidence::OftenMisremembered);
    }
}

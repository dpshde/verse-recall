use crate::error::{PackError, PackResult};
use crate::lex::LEX_NORMALIZER_VERSION;
use std::collections::BTreeMap;

const MAGIC: &[u8; 8] = b"VREXP001";
const LEGACY_EXPAND_FORMAT_VERSION: u16 = 1;
pub const EXPAND_FORMAT_VERSION: u16 = 2;
const LEGACY_HEADER_LEN: usize = 40;
const HEADER_LEN: usize = 72;
const DICTIONARY_RECORD_LEN: usize = 8;
const ENTRY_RECORD_LEN: usize = 16;
const EDGE_RECORD_LEN: usize = 8;

/// Defensive format limits. They are intentionally well above the expected
/// Bible-scale table while keeping malformed sections cheap to reject.
pub const MAX_EXPAND_SECTION_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_EXPAND_DICTIONARY_TERMS: usize = 500_000;
pub const MAX_EXPAND_ENTRIES: usize = 250_000;
pub const MAX_EXPAND_EDGES: usize = 8_000_000;
pub const MAX_EXPANSIONS_PER_SOURCE: usize = 64;
pub const MAX_EXPAND_STEM_BYTES: usize = 256;
const MAX_TRANSLATION_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct ExpandTarget {
    pub stem: String,
    /// Relative contribution to lexical scoring, in `(0, 1]`.
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExpandEntry {
    pub source: String,
    pub targets: Vec<ExpandTarget>,
}

/// Canonical build-time representation of an EXPAND section.
///
/// `new` sorts sources and targets, merges duplicate edges by their maximum
/// weight, and snaps weights to the u16 representation used on disk. Thus the
/// same logical mapping always encodes to the same bytes regardless of input
/// record order.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpandSection {
    canonical_translation: String,
    canonical_verse_count: u32,
    provenance_sha256: Option<[u8; 32]>,
    entries: Vec<ExpandEntry>,
}

impl ExpandSection {
    pub fn new(
        canonical_translation: impl Into<String>,
        canonical_verse_count: u32,
        entries: Vec<ExpandEntry>,
    ) -> PackResult<Self> {
        Self::build(canonical_translation, canonical_verse_count, None, entries)
    }

    /// Build the current content-addressed EXPAND format.
    ///
    /// The digest identifies a separately distributed canonical provenance
    /// manifest. Keeping it in the binary section prevents a release from
    /// silently swapping the mined graph's model, corpus, or parameters.
    pub fn new_with_provenance(
        canonical_translation: impl Into<String>,
        canonical_verse_count: u32,
        provenance_sha256: [u8; 32],
        entries: Vec<ExpandEntry>,
    ) -> PackResult<Self> {
        if !provenance_sha256.iter().any(|byte| *byte != 0) {
            return Err(PackError::InvalidExpand);
        }
        Self::build(
            canonical_translation,
            canonical_verse_count,
            Some(provenance_sha256),
            entries,
        )
    }

    fn build(
        canonical_translation: impl Into<String>,
        canonical_verse_count: u32,
        provenance_sha256: Option<[u8; 32]>,
        entries: Vec<ExpandEntry>,
    ) -> PackResult<Self> {
        let canonical_translation = canonical_translation.into();
        if canonical_translation.is_empty()
            || canonical_translation.trim() != canonical_translation
            || canonical_translation.len() > MAX_TRANSLATION_BYTES
            || canonical_translation.chars().any(char::is_control)
            || canonical_verse_count == 0
            || entries.is_empty()
        {
            return Err(PackError::InvalidExpand);
        }

        let mut mappings: BTreeMap<String, BTreeMap<String, u16>> = BTreeMap::new();
        for entry in entries {
            if !is_normalized_stem(&entry.source) || entry.targets.is_empty() {
                return Err(PackError::InvalidExpand);
            }
            let targets = mappings.entry(entry.source.clone()).or_default();
            for target in entry.targets {
                if !is_normalized_stem(&target.stem) || target.stem == entry.source {
                    return Err(PackError::InvalidExpand);
                }
                let quantized = quantize_weight(target.weight)?;
                targets
                    .entry(target.stem)
                    .and_modify(|current| *current = (*current).max(quantized))
                    .or_insert(quantized);
            }
        }

        if mappings.len() > MAX_EXPAND_ENTRIES {
            return Err(PackError::InvalidExpand);
        }
        let mut edge_count = 0usize;
        let mut canonical_entries = Vec::with_capacity(mappings.len());
        for (source, targets) in mappings {
            if targets.is_empty() || targets.len() > MAX_EXPANSIONS_PER_SOURCE {
                return Err(PackError::InvalidExpand);
            }
            edge_count = edge_count
                .checked_add(targets.len())
                .ok_or(PackError::Overflow)?;
            if edge_count > MAX_EXPAND_EDGES {
                return Err(PackError::InvalidExpand);
            }
            canonical_entries.push(ExpandEntry {
                source,
                targets: targets
                    .into_iter()
                    .map(|(stem, weight)| ExpandTarget {
                        stem,
                        weight: dequantize_weight(weight),
                    })
                    .collect(),
            });
        }

        let section = Self {
            canonical_translation,
            canonical_verse_count,
            provenance_sha256,
            entries: canonical_entries,
        };
        // Build the dictionary here as well so oversized input fails before
        // callers reach `encode`.
        section.dictionary()?;
        Ok(section)
    }

    pub fn canonical_translation(&self) -> &str {
        &self.canonical_translation
    }

    pub fn canonical_verse_count(&self) -> u32 {
        self.canonical_verse_count
    }

    pub fn entries(&self) -> &[ExpandEntry] {
        &self.entries
    }

    pub fn provenance_sha256(&self) -> Option<[u8; 32]> {
        self.provenance_sha256
    }

    pub fn encode(&self) -> Vec<u8> {
        let dictionary = self
            .dictionary()
            .expect("ExpandSection invariants were validated by new");
        let blob_len: usize = dictionary.iter().map(|term| term.len()).sum();
        let edge_count: usize = self.entries.iter().map(|entry| entry.targets.len()).sum();
        let header_len = if self.provenance_sha256.is_some() {
            HEADER_LEN
        } else {
            LEGACY_HEADER_LEN
        };
        let capacity = header_len
            + self.canonical_translation.len()
            + dictionary.len() * DICTIONARY_RECORD_LEN
            + self.entries.len() * ENTRY_RECORD_LEN
            + edge_count * EDGE_RECORD_LEN
            + blob_len;
        let mut out = Vec::with_capacity(capacity);
        out.extend_from_slice(MAGIC);
        let version = if self.provenance_sha256.is_some() {
            EXPAND_FORMAT_VERSION
        } else {
            LEGACY_EXPAND_FORMAT_VERSION
        };
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&LEX_NORMALIZER_VERSION.to_le_bytes());
        out.extend_from_slice(&self.canonical_verse_count.to_le_bytes());
        out.extend_from_slice(&(dictionary.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&(edge_count as u32).to_le_bytes());
        out.extend_from_slice(&(self.canonical_translation.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(blob_len as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        if let Some(provenance_sha256) = self.provenance_sha256 {
            out.extend_from_slice(&provenance_sha256);
        }
        out.extend_from_slice(self.canonical_translation.as_bytes());

        let mut blob_offset = 0u32;
        for term in &dictionary {
            out.extend_from_slice(&blob_offset.to_le_bytes());
            out.extend_from_slice(&(term.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            blob_offset += term.len() as u32;
        }

        let mut edge_start = 0u32;
        for entry in &self.entries {
            let source_term_id = dictionary
                .binary_search(&entry.source.as_str())
                .expect("source is in dictionary") as u32;
            out.extend_from_slice(&source_term_id.to_le_bytes());
            out.extend_from_slice(&edge_start.to_le_bytes());
            out.extend_from_slice(&(entry.targets.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            edge_start += entry.targets.len() as u32;
        }

        for entry in &self.entries {
            for target in &entry.targets {
                let target_term_id = dictionary
                    .binary_search(&target.stem.as_str())
                    .expect("target is in dictionary") as u32;
                out.extend_from_slice(&target_term_id.to_le_bytes());
                out.extend_from_slice(
                    &quantize_weight(target.weight)
                        .expect("weight was validated by new")
                        .to_le_bytes(),
                );
                out.extend_from_slice(&0u16.to_le_bytes());
            }
        }
        for term in dictionary {
            out.extend_from_slice(term.as_bytes());
        }
        debug_assert_eq!(out.len(), capacity);
        debug_assert!(out.len() <= MAX_EXPAND_SECTION_BYTES);
        out
    }

    fn dictionary(&self) -> PackResult<Vec<&str>> {
        let mut dictionary = Vec::new();
        for entry in &self.entries {
            dictionary.push(entry.source.as_str());
            dictionary.extend(entry.targets.iter().map(|target| target.stem.as_str()));
        }
        dictionary.sort_unstable();
        dictionary.dedup();
        if dictionary.len() > MAX_EXPAND_DICTIONARY_TERMS {
            return Err(PackError::InvalidExpand);
        }
        let blob_len = dictionary.iter().try_fold(0usize, |length, term| {
            length.checked_add(term.len()).ok_or(PackError::Overflow)
        })?;
        let edge_count: usize = self.entries.iter().map(|entry| entry.targets.len()).sum();
        let header_len = if self.provenance_sha256.is_some() {
            HEADER_LEN
        } else {
            LEGACY_HEADER_LEN
        };
        let encoded_len = header_len
            .checked_add(self.canonical_translation.len())
            .and_then(|length| {
                dictionary
                    .len()
                    .checked_mul(DICTIONARY_RECORD_LEN)
                    .and_then(|part| length.checked_add(part))
            })
            .and_then(|length| {
                self.entries
                    .len()
                    .checked_mul(ENTRY_RECORD_LEN)
                    .and_then(|part| length.checked_add(part))
            })
            .and_then(|length| {
                edge_count
                    .checked_mul(EDGE_RECORD_LEN)
                    .and_then(|part| length.checked_add(part))
            })
            .and_then(|length| length.checked_add(blob_len))
            .ok_or(PackError::Overflow)?;
        if blob_len > u32::MAX as usize || encoded_len > MAX_EXPAND_SECTION_BYTES {
            return Err(PackError::InvalidExpand);
        }
        Ok(dictionary)
    }
}

/// Owned immutable EXPAND bytes, validated once at construction.
#[derive(Debug, Clone)]
pub struct OwnedExpand {
    bytes: Vec<u8>,
}

impl OwnedExpand {
    pub fn new(bytes: Vec<u8>) -> PackResult<Self> {
        ExpandView::parse(&bytes)?;
        Ok(Self { bytes })
    }

    pub fn view(&self) -> ExpandView<'_> {
        ExpandView::from_validated(&self.bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

/// Allocation-free view over a validated EXPAND section.
#[derive(Debug, Clone, Copy)]
pub struct ExpandView<'a> {
    bytes: &'a [u8],
    version: u16,
    provenance_sha256: Option<[u8; 32]>,
    canonical_verse_count: u32,
    dictionary_count: u32,
    entry_count: u32,
    edge_count: u32,
    translation_start: usize,
    translation_len: usize,
    dictionary_start: usize,
    entries_start: usize,
    edges_start: usize,
    blob_start: usize,
}

impl<'a> ExpandView<'a> {
    pub fn parse(bytes: &'a [u8]) -> PackResult<Self> {
        if bytes.len() < LEGACY_HEADER_LEN || &bytes[..8] != MAGIC {
            return Err(PackError::InvalidExpand);
        }
        if bytes.len() > MAX_EXPAND_SECTION_BYTES {
            return Err(PackError::InvalidExpand);
        }
        let version = read_u16(bytes, 8);
        if version != LEGACY_EXPAND_FORMAT_VERSION && version != EXPAND_FORMAT_VERSION {
            return Err(PackError::UnsupportedExpandVersion(version));
        }
        let header_len = if version == EXPAND_FORMAT_VERSION {
            HEADER_LEN
        } else {
            LEGACY_HEADER_LEN
        };
        if bytes.len() < header_len {
            return Err(PackError::InvalidExpand);
        }
        let normalizer_version = read_u16(bytes, 10);
        if normalizer_version != LEX_NORMALIZER_VERSION {
            return Err(PackError::UnsupportedExpandNormalizerVersion(
                normalizer_version,
            ));
        }
        let canonical_verse_count = read_u32(bytes, 12);
        let dictionary_count = read_u32(bytes, 16);
        let entry_count = read_u32(bytes, 20);
        let edge_count = read_u32(bytes, 24);
        let translation_len = read_u16(bytes, 28) as usize;
        let blob_len = read_u32(bytes, 32) as usize;
        if canonical_verse_count == 0
            || dictionary_count < 2
            || dictionary_count as usize > MAX_EXPAND_DICTIONARY_TERMS
            || entry_count == 0
            || entry_count as usize > MAX_EXPAND_ENTRIES
            || edge_count == 0
            || edge_count as usize > MAX_EXPAND_EDGES
            || edge_count < entry_count
            || translation_len == 0
            || translation_len > MAX_TRANSLATION_BYTES
            || bytes[30..32] != [0u8; 2]
            || bytes[36..40] != [0u8; 4]
        {
            return Err(PackError::InvalidExpand);
        }

        let provenance_sha256 = if version == EXPAND_FORMAT_VERSION {
            let digest: [u8; 32] = bytes[LEGACY_HEADER_LEN..HEADER_LEN]
                .try_into()
                .map_err(|_| PackError::InvalidExpand)?;
            if !digest.iter().any(|byte| *byte != 0) {
                return Err(PackError::InvalidExpand);
            }
            Some(digest)
        } else {
            None
        };
        let translation_start = header_len;
        let dictionary_start = translation_start
            .checked_add(translation_len)
            .ok_or(PackError::Overflow)?;
        let entries_start = dictionary_start
            .checked_add(
                (dictionary_count as usize)
                    .checked_mul(DICTIONARY_RECORD_LEN)
                    .ok_or(PackError::Overflow)?,
            )
            .ok_or(PackError::Overflow)?;
        let edges_start = entries_start
            .checked_add(
                (entry_count as usize)
                    .checked_mul(ENTRY_RECORD_LEN)
                    .ok_or(PackError::Overflow)?,
            )
            .ok_or(PackError::Overflow)?;
        let blob_start = edges_start
            .checked_add(
                (edge_count as usize)
                    .checked_mul(EDGE_RECORD_LEN)
                    .ok_or(PackError::Overflow)?,
            )
            .ok_or(PackError::Overflow)?;
        let expected_len = blob_start
            .checked_add(blob_len)
            .ok_or(PackError::Overflow)?;
        if bytes.len() != expected_len {
            return Err(PackError::InvalidExpand);
        }

        let view = Self {
            bytes,
            version,
            provenance_sha256,
            canonical_verse_count,
            dictionary_count,
            entry_count,
            edge_count,
            translation_start,
            translation_len,
            dictionary_start,
            entries_start,
            edges_start,
            blob_start,
        };
        let translation = view.canonical_translation()?;
        if translation.trim() != translation || translation.chars().any(char::is_control) {
            return Err(PackError::InvalidExpand);
        }

        let mut expected_blob_offset = 0usize;
        let mut previous_term: Option<&str> = None;
        for term_id in 0..dictionary_count {
            let record = view.dictionary_record(term_id)?;
            if record.offset != expected_blob_offset
                || !is_normalized_stem(record.term)
                || previous_term.is_some_and(|term| term >= record.term)
            {
                return Err(PackError::InvalidExpand);
            }
            expected_blob_offset = expected_blob_offset
                .checked_add(record.len)
                .ok_or(PackError::Overflow)?;
            previous_term = Some(record.term);
        }
        if expected_blob_offset != blob_len {
            return Err(PackError::InvalidExpand);
        }

        let mut previous_source = None;
        let mut expected_edge_start = 0u32;
        for entry_index in 0..entry_count {
            let record = view.entry_record(entry_index)?;
            if record.source_term_id >= dictionary_count
                || previous_source.is_some_and(|source| source >= record.source_term_id)
                || record.edge_start != expected_edge_start
                || record.edge_count == 0
                || record.edge_count as usize > MAX_EXPANSIONS_PER_SOURCE
            {
                return Err(PackError::InvalidExpand);
            }
            let edge_end = record
                .edge_start
                .checked_add(u32::from(record.edge_count))
                .ok_or(PackError::Overflow)?;
            if edge_end > edge_count {
                return Err(PackError::InvalidExpand);
            }
            let mut previous_target = None;
            for edge_index in record.edge_start..edge_end {
                let edge = view.edge_record(edge_index)?;
                if edge.target_term_id >= dictionary_count
                    || edge.target_term_id == record.source_term_id
                    || edge.weight == 0
                    || previous_target.is_some_and(|target| target >= edge.target_term_id)
                {
                    return Err(PackError::InvalidExpand);
                }
                previous_target = Some(edge.target_term_id);
            }
            expected_edge_start = edge_end;
            previous_source = Some(record.source_term_id);
        }
        if expected_edge_start != edge_count {
            return Err(PackError::InvalidExpand);
        }
        Ok(view)
    }

    fn from_validated(bytes: &'a [u8]) -> Self {
        let version = read_u16(bytes, 8);
        let header_len = if version == EXPAND_FORMAT_VERSION {
            HEADER_LEN
        } else {
            LEGACY_HEADER_LEN
        };
        let provenance_sha256 = (version == EXPAND_FORMAT_VERSION).then(|| {
            bytes[LEGACY_HEADER_LEN..HEADER_LEN]
                .try_into()
                .expect("validated provenance digest")
        });
        let canonical_verse_count = read_u32(bytes, 12);
        let dictionary_count = read_u32(bytes, 16);
        let entry_count = read_u32(bytes, 20);
        let edge_count = read_u32(bytes, 24);
        let translation_len = read_u16(bytes, 28) as usize;
        let translation_start = header_len;
        let dictionary_start = translation_start + translation_len;
        let entries_start = dictionary_start + dictionary_count as usize * DICTIONARY_RECORD_LEN;
        let edges_start = entries_start + entry_count as usize * ENTRY_RECORD_LEN;
        let blob_start = edges_start + edge_count as usize * EDGE_RECORD_LEN;
        Self {
            bytes,
            version,
            provenance_sha256,
            canonical_verse_count,
            dictionary_count,
            entry_count,
            edge_count,
            translation_start,
            translation_len,
            dictionary_start,
            entries_start,
            edges_start,
            blob_start,
        }
    }

    pub fn canonical_translation(self) -> PackResult<&'a str> {
        std::str::from_utf8(
            &self.bytes[self.translation_start..self.translation_start + self.translation_len],
        )
        .map_err(|_| PackError::InvalidUtf8 {
            context: "EXPAND.canonical_translation",
        })
    }

    pub fn canonical_verse_count(self) -> u32 {
        self.canonical_verse_count
    }

    pub fn format_version(self) -> u16 {
        self.version
    }

    pub fn provenance_sha256(self) -> Option<[u8; 32]> {
        self.provenance_sha256
    }

    pub fn dictionary_count(self) -> u32 {
        self.dictionary_count
    }

    pub fn entry_count(self) -> u32 {
        self.entry_count
    }

    pub fn edge_count(self) -> u32 {
        self.edge_count
    }

    /// Find the weighted targets for an exact normalized source stem.
    pub fn expansions(self, source: &str) -> Option<Expansions<'a>> {
        let source_term_id = self.lookup_term(source)?;
        let mut left = 0u32;
        let mut right = self.entry_count;
        while left < right {
            let middle = left + (right - left) / 2;
            let entry = self.entry_record(middle).ok()?;
            match entry.source_term_id.cmp(&source_term_id) {
                core::cmp::Ordering::Less => left = middle + 1,
                core::cmp::Ordering::Greater => right = middle,
                core::cmp::Ordering::Equal => {
                    return Some(Expansions {
                        view: self,
                        next_edge: entry.edge_start,
                        remaining: entry.edge_count,
                    });
                }
            }
        }
        None
    }

    fn lookup_term(self, term: &str) -> Option<u32> {
        let mut left = 0u32;
        let mut right = self.dictionary_count;
        while left < right {
            let middle = left + (right - left) / 2;
            let candidate = self.dictionary_record(middle).ok()?.term;
            match candidate.cmp(term) {
                core::cmp::Ordering::Less => left = middle + 1,
                core::cmp::Ordering::Greater => right = middle,
                core::cmp::Ordering::Equal => return Some(middle),
            }
        }
        None
    }

    fn dictionary_record(self, term_id: u32) -> PackResult<DictionaryRecord<'a>> {
        if term_id >= self.dictionary_count {
            return Err(PackError::InvalidExpand);
        }
        let base = self.dictionary_start + term_id as usize * DICTIONARY_RECORD_LEN;
        let offset = read_u32(self.bytes, base) as usize;
        let len = read_u16(self.bytes, base + 4) as usize;
        if len == 0 || len > MAX_EXPAND_STEM_BYTES || self.bytes[base + 6..base + 8] != [0u8; 2] {
            return Err(PackError::InvalidExpand);
        }
        let end = offset.checked_add(len).ok_or(PackError::Overflow)?;
        let blob = &self.bytes[self.blob_start..];
        if end > blob.len() {
            return Err(PackError::InvalidExpand);
        }
        let term = std::str::from_utf8(&blob[offset..end]).map_err(|_| PackError::InvalidUtf8 {
            context: "EXPAND.term",
        })?;
        Ok(DictionaryRecord { offset, len, term })
    }

    fn entry_record(self, entry_index: u32) -> PackResult<EntryRecord> {
        if entry_index >= self.entry_count {
            return Err(PackError::InvalidExpand);
        }
        let base = self.entries_start + entry_index as usize * ENTRY_RECORD_LEN;
        if self.bytes[base + 10..base + 16] != [0u8; 6] {
            return Err(PackError::InvalidExpand);
        }
        Ok(EntryRecord {
            source_term_id: read_u32(self.bytes, base),
            edge_start: read_u32(self.bytes, base + 4),
            edge_count: read_u16(self.bytes, base + 8),
        })
    }

    fn edge_record(self, edge_index: u32) -> PackResult<EdgeRecord> {
        if edge_index >= self.edge_count {
            return Err(PackError::InvalidExpand);
        }
        let base = self.edges_start + edge_index as usize * EDGE_RECORD_LEN;
        if self.bytes[base + 6..base + 8] != [0u8; 2] {
            return Err(PackError::InvalidExpand);
        }
        Ok(EdgeRecord {
            target_term_id: read_u32(self.bytes, base),
            weight: read_u16(self.bytes, base + 4),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeightedExpansion<'a> {
    pub stem: &'a str,
    pub weight: f32,
}

pub struct Expansions<'a> {
    view: ExpandView<'a>,
    next_edge: u32,
    remaining: u16,
}

impl<'a> Iterator for Expansions<'a> {
    type Item = WeightedExpansion<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let edge = self.view.edge_record(self.next_edge).ok()?;
        let stem = self.view.dictionary_record(edge.target_term_id).ok()?.term;
        self.next_edge += 1;
        self.remaining -= 1;
        Some(WeightedExpansion {
            stem,
            weight: dequantize_weight(edge.weight),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::from(self.remaining);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for Expansions<'_> {}

struct DictionaryRecord<'a> {
    offset: usize,
    len: usize,
    term: &'a str,
}

struct EntryRecord {
    source_term_id: u32,
    edge_start: u32,
    edge_count: u16,
}

struct EdgeRecord {
    target_term_id: u32,
    weight: u16,
}

fn is_normalized_stem(value: &str) -> bool {
    value.len() >= 2
        && value.len() <= MAX_EXPAND_STEM_BYTES
        && value
            .chars()
            .all(|character| character.is_alphanumeric() && !character.is_uppercase())
}

fn quantize_weight(weight: f32) -> PackResult<u16> {
    if !weight.is_finite() || weight <= 0.0 || weight > 1.0 {
        return Err(PackError::InvalidExpand);
    }
    Ok((weight * f32::from(u16::MAX))
        .round()
        .clamp(1.0, f32::from(u16::MAX)) as u16)
}

fn dequantize_weight(weight: u16) -> f32 {
    f32::from(weight) / f32::from(u16::MAX)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated offset"),
    )
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated offset"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section() -> ExpandSection {
        ExpandSection::new(
            "BSB",
            31_086,
            vec![
                ExpandEntry {
                    source: "weary".into(),
                    targets: vec![
                        ExpandTarget {
                            stem: "rest".into(),
                            weight: 0.75,
                        },
                        ExpandTarget {
                            stem: "strength".into(),
                            weight: 0.5,
                        },
                    ],
                },
                ExpandEntry {
                    source: "exhausted".into(),
                    targets: vec![ExpandTarget {
                        stem: "rest".into(),
                        weight: 0.9,
                    }],
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn roundtrip_is_searchable_and_zero_copy() {
        let bytes = section().encode();
        let view = ExpandView::parse(&bytes).unwrap();
        assert_eq!(view.format_version(), LEGACY_EXPAND_FORMAT_VERSION);
        assert_eq!(view.provenance_sha256(), None);
        assert_eq!(view.canonical_translation().unwrap(), "BSB");
        assert_eq!(view.canonical_verse_count(), 31_086);
        assert_eq!(view.entry_count(), 2);
        assert_eq!(view.edge_count(), 3);
        let weary: Vec<_> = view.expansions("weary").unwrap().collect();
        assert_eq!(weary.len(), 2);
        assert_eq!(weary[0].stem, "rest");
        assert!((weary[0].weight - 0.75).abs() < 0.000_02);
        assert!(view.expansions("missing").is_none());
    }

    #[test]
    fn current_format_binds_nonzero_provenance_digest() {
        let digest = [0xa5; 32];
        let legacy = section();
        let current = ExpandSection::new_with_provenance(
            legacy.canonical_translation().to_string(),
            legacy.canonical_verse_count(),
            digest,
            legacy.entries().to_vec(),
        )
        .unwrap();
        let bytes = current.encode();
        let view = ExpandView::parse(&bytes).unwrap();
        assert_eq!(view.format_version(), EXPAND_FORMAT_VERSION);
        assert_eq!(view.provenance_sha256(), Some(digest));

        assert_eq!(
            ExpandSection::new_with_provenance("BSB", 1, [0; 32], current.entries().to_vec()),
            Err(PackError::InvalidExpand)
        );

        let mut zeroed = bytes;
        zeroed[LEGACY_HEADER_LEN..HEADER_LEN].fill(0);
        assert_eq!(
            ExpandView::parse(&zeroed).unwrap_err(),
            PackError::InvalidExpand
        );
    }

    #[test]
    fn logical_input_order_and_duplicates_encode_deterministically() {
        let first = section().encode();
        let second = ExpandSection::new(
            "BSB",
            31_086,
            vec![
                ExpandEntry {
                    source: "weary".into(),
                    targets: vec![
                        ExpandTarget {
                            stem: "strength".into(),
                            weight: 0.5,
                        },
                        ExpandTarget {
                            stem: "rest".into(),
                            weight: 0.6,
                        },
                    ],
                },
                ExpandEntry {
                    source: "exhausted".into(),
                    targets: vec![ExpandTarget {
                        stem: "rest".into(),
                        weight: 0.9,
                    }],
                },
                ExpandEntry {
                    source: "weary".into(),
                    targets: vec![ExpandTarget {
                        stem: "rest".into(),
                        weight: 0.75,
                    }],
                },
            ],
        )
        .unwrap()
        .encode();
        assert_eq!(first, second);
    }

    #[test]
    fn rejects_malformed_versions_reserved_fields_edges_and_trailing_bytes() {
        let bytes = section().encode();

        let mut bad_version = bytes.clone();
        bad_version[8..10].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(
            ExpandView::parse(&bad_version).unwrap_err(),
            PackError::UnsupportedExpandVersion(3)
        );

        let mut bad_normalizer = bytes.clone();
        bad_normalizer[10..12].copy_from_slice(&1u16.to_le_bytes());
        assert_eq!(
            ExpandView::parse(&bad_normalizer).unwrap_err(),
            PackError::UnsupportedExpandNormalizerVersion(1)
        );

        let mut dirty_reserved = bytes.clone();
        dirty_reserved[30] = 1;
        assert_eq!(
            ExpandView::parse(&dirty_reserved).unwrap_err(),
            PackError::InvalidExpand
        );

        let view = ExpandView::parse(&bytes).unwrap();
        let mut zero_weight = bytes.clone();
        zero_weight[view.edges_start + 4..view.edges_start + 6]
            .copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(
            ExpandView::parse(&zero_weight).unwrap_err(),
            PackError::InvalidExpand
        );

        let mut bad_edge_start = bytes.clone();
        bad_edge_start[view.entries_start + 4..view.entries_start + 8]
            .copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(
            ExpandView::parse(&bad_edge_start).unwrap_err(),
            PackError::InvalidExpand
        );

        let mut trailing = bytes;
        trailing.push(0);
        assert_eq!(
            ExpandView::parse(&trailing).unwrap_err(),
            PackError::InvalidExpand
        );
    }

    #[test]
    fn rejects_unnormalized_self_edges_bad_weights_and_fanout_over_limit() {
        let invalid = |source: &str, target: &str, weight: f32| {
            ExpandSection::new(
                "BSB",
                1,
                vec![ExpandEntry {
                    source: source.into(),
                    targets: vec![ExpandTarget {
                        stem: target.into(),
                        weight,
                    }],
                }],
            )
        };
        assert_eq!(invalid("Too", "small", 1.0), Err(PackError::InvalidExpand));
        assert_eq!(invalid("same", "same", 1.0), Err(PackError::InvalidExpand));
        assert_eq!(
            invalid("good", "target", 0.0),
            Err(PackError::InvalidExpand)
        );
        assert_eq!(
            invalid("good", "target", 1.1),
            Err(PackError::InvalidExpand)
        );

        let targets = (0..=MAX_EXPANSIONS_PER_SOURCE)
            .map(|index| ExpandTarget {
                stem: format!("target{index}"),
                weight: 0.5,
            })
            .collect();
        assert_eq!(
            ExpandSection::new(
                "BSB",
                1,
                vec![ExpandEntry {
                    source: "source".into(),
                    targets,
                }]
            ),
            Err(PackError::InvalidExpand)
        );
    }
}

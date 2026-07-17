use crate::{PackError, PackResult};
use std::collections::BTreeMap;

pub const BINARY_OVERLAY_MAGIC: &[u8; 8] = b"VROVL001";
const MAGIC_PREFIX: &[u8; 4] = b"VROV";
const FORMAT_VERSION: u16 = 1;
const HEADER_LEN: usize = 40;

const MAX_SECTION_BYTES: usize = 256 * 1024 * 1024;
const MAX_RECORDS: usize = 1_000_000;
const MAX_CHILD_RECORDS: usize = 4_000_000;
const MAX_STRING_BYTES: usize = 1_048_576;
const MAX_TRANSLATION_BYTES: usize = 256;

const CORRECTION_SOURCE_PREFIX: &str = "correction-v1";
const CORRECTION_REVIEW_POLICY: &str = "source-backed-correction-v1";
const CORRECTION_LICENSE: &str = "CC-BY-SA-4.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayRange {
    pub start_ref: String,
    pub end_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayConcept {
    pub concept_type: String,
    pub key: String,
    pub label: String,
    pub aliases: Vec<String>,
    pub source: String,
    pub anchor_ref: String,
    pub target_ranges: Vec<OverlayRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayAlternate {
    pub source_translation: String,
    pub target_ref: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlaySection {
    pub canonical_translation: String,
    pub canonical_verse_count: u32,
    pub concepts: Vec<OverlayConcept>,
    pub alternates: Vec<OverlayAlternate>,
}

impl OverlaySection {
    pub fn encode(&self) -> PackResult<Vec<u8>> {
        validate_section(self)?;
        let alias_count = self.concepts.iter().try_fold(0usize, |count, concept| {
            count
                .checked_add(concept.aliases.len())
                .ok_or(PackError::Overflow)
        })?;
        let range_count = self.concepts.iter().try_fold(0usize, |count, concept| {
            count
                .checked_add(concept.target_ranges.len())
                .ok_or(PackError::Overflow)
        })?;

        let mut payload = Vec::new();
        payload.extend_from_slice(self.canonical_translation.as_bytes());
        for concept in &self.concepts {
            push_string(&mut payload, &concept.concept_type)?;
            push_string(&mut payload, &concept.key)?;
            push_string(&mut payload, &concept.label)?;
            push_string(&mut payload, &concept.source)?;
            push_string(&mut payload, &concept.anchor_ref)?;
            payload.extend_from_slice(&(concept.aliases.len() as u32).to_le_bytes());
            payload.extend_from_slice(&(concept.target_ranges.len() as u32).to_le_bytes());
            for alias in &concept.aliases {
                push_string(&mut payload, alias)?;
            }
            for range in &concept.target_ranges {
                push_string(&mut payload, &range.start_ref)?;
                push_string(&mut payload, &range.end_ref)?;
            }
        }
        for alternate in &self.alternates {
            push_string(&mut payload, &alternate.source_translation)?;
            push_string(&mut payload, &alternate.target_ref)?;
            push_string(&mut payload, &alternate.text)?;
        }
        if payload.len() > MAX_SECTION_BYTES - HEADER_LEN || payload.len() > u32::MAX as usize {
            return Err(PackError::InvalidOverlay);
        }

        let mut output = Vec::with_capacity(HEADER_LEN + payload.len());
        output.extend_from_slice(BINARY_OVERLAY_MAGIC);
        output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(&self.canonical_verse_count.to_le_bytes());
        output.extend_from_slice(&(self.concepts.len() as u32).to_le_bytes());
        output.extend_from_slice(&(self.alternates.len() as u32).to_le_bytes());
        output.extend_from_slice(&(alias_count as u32).to_le_bytes());
        output.extend_from_slice(&(range_count as u32).to_le_bytes());
        output.extend_from_slice(&(self.canonical_translation.len() as u16).to_le_bytes());
        output.extend_from_slice(&0u16.to_le_bytes());
        output.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        debug_assert_eq!(output.len(), HEADER_LEN);
        output.extend_from_slice(&payload);
        Ok(output)
    }

    pub fn parse(bytes: &[u8]) -> PackResult<Self> {
        if bytes.len() < HEADER_LEN || bytes.len() > MAX_SECTION_BYTES {
            return Err(PackError::InvalidOverlay);
        }
        if &bytes[..8] != BINARY_OVERLAY_MAGIC {
            return Err(PackError::InvalidOverlay);
        }
        let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        if version != FORMAT_VERSION {
            return Err(PackError::UnsupportedOverlayVersion(version));
        }
        if bytes[10..12] != [0u8; 2] || bytes[34..36] != [0u8; 2] {
            return Err(PackError::InvalidOverlay);
        }
        let canonical_verse_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let concept_count = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let alternate_count = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
        let expected_alias_count = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
        let expected_range_count = u32::from_le_bytes(bytes[28..32].try_into().unwrap()) as usize;
        let translation_len = u16::from_le_bytes(bytes[32..34].try_into().unwrap()) as usize;
        let payload_len = u32::from_le_bytes(bytes[36..40].try_into().unwrap()) as usize;
        if canonical_verse_count == 0
            || concept_count > MAX_RECORDS
            || alternate_count > MAX_RECORDS
            || concept_count.saturating_add(alternate_count) == 0
            || concept_count.saturating_add(alternate_count) > MAX_RECORDS
            || expected_alias_count > MAX_CHILD_RECORDS
            || expected_range_count > MAX_CHILD_RECORDS
            || expected_alias_count.saturating_add(expected_range_count) > MAX_CHILD_RECORDS
            || translation_len == 0
            || translation_len > MAX_TRANSLATION_BYTES
            || payload_len != bytes.len() - HEADER_LEN
            || translation_len > payload_len
        {
            return Err(PackError::InvalidOverlay);
        }

        let mut cursor = HEADER_LEN;
        let translation_end = cursor
            .checked_add(translation_len)
            .ok_or(PackError::Overflow)?;
        let canonical_translation = read_utf8(bytes, cursor, translation_end)?.to_string();
        cursor = translation_end;
        if canonical_translation.trim() != canonical_translation {
            return Err(PackError::InvalidOverlay);
        }
        let minimum_record_bytes = concept_count
            .checked_mul(28)
            .and_then(|value| {
                alternate_count
                    .checked_mul(12)
                    .and_then(|alternate_bytes| value.checked_add(alternate_bytes))
            })
            .ok_or(PackError::Overflow)?;
        if minimum_record_bytes > bytes.len() - cursor {
            return Err(PackError::InvalidOverlay);
        }

        let mut concepts = Vec::with_capacity(concept_count);
        let mut alias_count = 0usize;
        let mut range_count = 0usize;
        for _ in 0..concept_count {
            let concept_type = read_string(bytes, &mut cursor)?;
            let key = read_string(bytes, &mut cursor)?;
            let label = read_string(bytes, &mut cursor)?;
            let source = read_string(bytes, &mut cursor)?;
            let anchor_ref = read_string(bytes, &mut cursor)?;
            let local_alias_count = read_count(bytes, &mut cursor, MAX_CHILD_RECORDS)?;
            let local_range_count = read_count(bytes, &mut cursor, MAX_CHILD_RECORDS)?;
            if local_range_count == 0 {
                return Err(PackError::InvalidOverlay);
            }
            let minimum_child_bytes = local_alias_count
                .checked_mul(4)
                .and_then(|value| {
                    local_range_count
                        .checked_mul(8)
                        .and_then(|range_bytes| value.checked_add(range_bytes))
                })
                .ok_or(PackError::Overflow)?;
            if minimum_child_bytes > bytes.len() - cursor {
                return Err(PackError::InvalidOverlay);
            }
            alias_count = alias_count
                .checked_add(local_alias_count)
                .ok_or(PackError::Overflow)?;
            range_count = range_count
                .checked_add(local_range_count)
                .ok_or(PackError::Overflow)?;
            if alias_count > expected_alias_count || range_count > expected_range_count {
                return Err(PackError::InvalidOverlay);
            }
            let mut aliases = Vec::with_capacity(local_alias_count);
            for _ in 0..local_alias_count {
                aliases.push(read_string(bytes, &mut cursor)?);
            }
            let mut target_ranges = Vec::with_capacity(local_range_count);
            for _ in 0..local_range_count {
                target_ranges.push(OverlayRange {
                    start_ref: read_string(bytes, &mut cursor)?,
                    end_ref: read_string(bytes, &mut cursor)?,
                });
            }
            concepts.push(OverlayConcept {
                concept_type,
                key,
                label,
                aliases,
                source,
                anchor_ref,
                target_ranges,
            });
        }

        let mut alternates = Vec::with_capacity(alternate_count);
        for _ in 0..alternate_count {
            alternates.push(OverlayAlternate {
                source_translation: read_string(bytes, &mut cursor)?,
                target_ref: read_string(bytes, &mut cursor)?,
                text: read_string(bytes, &mut cursor)?,
            });
        }
        if cursor != bytes.len()
            || alias_count != expected_alias_count
            || range_count != expected_range_count
        {
            return Err(PackError::InvalidOverlay);
        }
        let section = Self {
            canonical_translation,
            canonical_verse_count,
            concepts,
            alternates,
        };
        validate_section(&section)?;
        Ok(section)
    }
}

pub fn is_binary_overlay(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC_PREFIX)
}

fn validate_section(section: &OverlaySection) -> PackResult<()> {
    let translation = section.canonical_translation.as_bytes();
    if section.canonical_verse_count == 0
        || translation.is_empty()
        || translation.len() > MAX_TRANSLATION_BYTES
        || section.canonical_translation.trim() != section.canonical_translation
        || section
            .concepts
            .len()
            .saturating_add(section.alternates.len())
            == 0
        || section
            .concepts
            .len()
            .saturating_add(section.alternates.len())
            > MAX_RECORDS
    {
        return Err(PackError::InvalidOverlay);
    }
    let mut child_count = 0usize;
    for concept in &section.concepts {
        validate_string(&concept.concept_type)?;
        validate_string(&concept.key)?;
        validate_string(&concept.label)?;
        validate_string(&concept.source)?;
        validate_string(&concept.anchor_ref)?;
        if concept.concept_type == "correction" && !valid_correction_source(&concept.source) {
            return Err(PackError::InvalidOverlay);
        }
        if concept.target_ranges.is_empty() {
            return Err(PackError::InvalidOverlay);
        }
        child_count = child_count
            .checked_add(concept.aliases.len())
            .and_then(|value| value.checked_add(concept.target_ranges.len()))
            .ok_or(PackError::Overflow)?;
        if child_count > MAX_CHILD_RECORDS {
            return Err(PackError::InvalidOverlay);
        }
        for alias in &concept.aliases {
            validate_string(alias)?;
        }
        for range in &concept.target_ranges {
            validate_string(&range.start_ref)?;
            validate_string(&range.end_ref)?;
        }
    }
    for alternate in &section.alternates {
        validate_string(&alternate.source_translation)?;
        validate_string(&alternate.target_ref)?;
        validate_string(&alternate.text)?;
    }
    Ok(())
}

fn valid_correction_source(source: &str) -> bool {
    let mut parts = source.split('|');
    if parts.next() != Some(CORRECTION_SOURCE_PREFIX) {
        return false;
    }
    let mut fields = BTreeMap::new();
    for part in parts {
        let Some((name, value)) = part.split_once('=') else {
            return false;
        };
        if name.is_empty()
            || value.is_empty()
            || value.contains(['|', '='])
            || fields.insert(name, value).is_some()
        {
            return false;
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
        return false;
    }
    if fields.get("dataset") != Some(&"BibleAquifer")
        || fields.get("license") != Some(&CORRECTION_LICENSE)
        || fields.get("review-status") != Some(&"approved")
        || fields.get("review-policy") != Some(&CORRECTION_REVIEW_POLICY)
    {
        return false;
    }
    let sha = fields["content-sha256"];
    if sha.len() != 64 || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return false;
    }
    let date = fields["reviewed-at"].as_bytes();
    date.len() == 10
        && date[4] == b'-'
        && date[7] == b'-'
        && date
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn validate_string(value: &str) -> PackResult<()> {
    if value.trim().is_empty() || value.len() > MAX_STRING_BYTES {
        return Err(PackError::InvalidOverlay);
    }
    Ok(())
}

fn push_string(output: &mut Vec<u8>, value: &str) -> PackResult<()> {
    validate_string(value)?;
    let len = u32::try_from(value.len()).map_err(|_| PackError::Overflow)?;
    let next_len = output
        .len()
        .checked_add(4)
        .and_then(|value| value.checked_add(len as usize))
        .ok_or(PackError::Overflow)?;
    if next_len > MAX_SECTION_BYTES - HEADER_LEN {
        return Err(PackError::InvalidOverlay);
    }
    output.extend_from_slice(&len.to_le_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn read_string(bytes: &[u8], cursor: &mut usize) -> PackResult<String> {
    let len = read_u32(bytes, cursor)? as usize;
    if len == 0 || len > MAX_STRING_BYTES {
        return Err(PackError::InvalidOverlay);
    }
    let end = cursor.checked_add(len).ok_or(PackError::Overflow)?;
    let value = read_utf8(bytes, *cursor, end)?.to_string();
    *cursor = end;
    if value.trim().is_empty() {
        return Err(PackError::InvalidOverlay);
    }
    Ok(value)
}

fn read_count(bytes: &[u8], cursor: &mut usize, maximum: usize) -> PackResult<usize> {
    let value = read_u32(bytes, cursor)? as usize;
    if value > maximum {
        return Err(PackError::InvalidOverlay);
    }
    Ok(value)
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> PackResult<u32> {
    let end = cursor.checked_add(4).ok_or(PackError::Overflow)?;
    let slice = bytes.get(*cursor..end).ok_or(PackError::InvalidOverlay)?;
    *cursor = end;
    Ok(u32::from_le_bytes(slice.try_into().unwrap()))
}

fn read_utf8(bytes: &[u8], start: usize, end: usize) -> PackResult<&str> {
    let slice = bytes.get(start..end).ok_or(PackError::InvalidOverlay)?;
    std::str::from_utf8(slice).map_err(|_| PackError::InvalidUtf8 {
        context: "binary overlay string",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> OverlaySection {
        OverlaySection {
            canonical_translation: "BSB".into(),
            canonical_verse_count: 2,
            concepts: vec![OverlayConcept {
                concept_type: "event".into(),
                key: "the fall".into(),
                label: "The Fall".into(),
                aliases: vec!["fall of humanity".into()],
                source: "test".into(),
                anchor_ref: "Genesis 3:1".into(),
                target_ranges: vec![OverlayRange {
                    start_ref: "Genesis 3:1".into(),
                    end_ref: "Genesis 3:24".into(),
                }],
            }],
            alternates: vec![OverlayAlternate {
                source_translation: "KJV".into(),
                target_ref: "Genesis 1:1".into(),
                text: "In the beginning God created the heaven and the earth.".into(),
            }],
        }
    }

    fn reviewed_correction_source() -> String {
        format!(
            "correction-v1|id=aquifer.test-1|dataset=BibleAquifer|record=42|revision=db-test|license=CC-BY-SA-4.0|content-sha256={}|review-status=approved|review-policy=source-backed-correction-v1|reviewed-by=editor-1|reviewed-at=2026-07-16",
            "ab".repeat(32)
        )
    }

    #[test]
    fn roundtrip_preserves_typed_records() {
        let section = sample();
        let bytes = section.encode().unwrap();
        assert!(is_binary_overlay(&bytes));
        assert_eq!(OverlaySection::parse(&bytes).unwrap(), section);
    }

    #[test]
    fn correction_records_require_compact_approved_provenance() {
        let mut section = sample();
        section.concepts[0].concept_type = "correction".into();
        section.concepts[0].source = reviewed_correction_source();
        let bytes = section.encode().expect("approved correction source");
        assert_eq!(OverlaySection::parse(&bytes).unwrap(), section);

        section.concepts[0].source = section.concepts[0]
            .source
            .replace("review-status=approved", "review-status=candidate");
        assert_eq!(section.encode(), Err(PackError::InvalidOverlay));
    }

    #[test]
    fn rejects_truncation_trailing_data_and_reserved_bytes() {
        let bytes = sample().encode().unwrap();
        for removed in [1, 4, bytes.len() - HEADER_LEN + 1] {
            assert!(OverlaySection::parse(&bytes[..bytes.len() - removed]).is_err());
        }

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            OverlaySection::parse(&trailing),
            Err(PackError::InvalidOverlay)
        );

        let mut reserved = bytes;
        reserved[10] = 1;
        assert_eq!(
            OverlaySection::parse(&reserved),
            Err(PackError::InvalidOverlay)
        );
    }

    #[test]
    fn rejects_unsupported_versions_bad_counts_and_lengths() {
        let bytes = sample().encode().unwrap();

        let mut version = bytes.clone();
        version[8..10].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(
            OverlaySection::parse(&version),
            Err(PackError::UnsupportedOverlayVersion(2))
        );

        let mut aliases = bytes.clone();
        aliases[24..28].copy_from_slice(&2u32.to_le_bytes());
        assert_eq!(
            OverlaySection::parse(&aliases),
            Err(PackError::InvalidOverlay)
        );

        let mut record_count = bytes.clone();
        record_count[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            OverlaySection::parse(&record_count),
            Err(PackError::InvalidOverlay)
        );

        let mut translation = bytes;
        translation[32..34].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(
            OverlaySection::parse(&translation),
            Err(PackError::InvalidOverlay)
        );
    }

    #[test]
    fn encoder_rejects_empty_required_fields() {
        let mut section = sample();
        section.concepts[0].target_ranges.clear();
        assert_eq!(section.encode(), Err(PackError::InvalidOverlay));
        section.concepts[0].target_ranges.push(OverlayRange {
            start_ref: "Genesis 3:1".into(),
            end_ref: "Genesis 3:24".into(),
        });
        section.alternates[0].text.clear();
        assert_eq!(section.encode(), Err(PackError::InvalidOverlay));
    }
}

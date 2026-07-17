#!/usr/bin/env python3
"""Fail closed unless a release directory matches the complete release contract.

The JSON manifest is a content inventory, not a substitute for parsing the
artifacts.  This verifier therefore checks both the exact set of release roles
and the release-critical identities embedded in each binary pack.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
from typing import Any

try:
    from scripts.embedding_space_contract import validate_envelope
except ModuleNotFoundError:  # Direct execution from the scripts directory.
    from embedding_space_contract import validate_envelope


SCHEMA = "verse-recall-release-artifacts-v1"
CANONICAL_TRANSLATION = "BSB"
CANONICAL_VERSE_COUNT = 31_086
NORMALIZER_VERSION = 2
PACK_MAGIC = b"VRCALL01"
PACK_HEADER_SIZE = 64
SECTION_ENTRY_SIZE = 32
SECTION_ALIGNMENT = 64

BASE_FILENAME = "bsb-full.pack"
EXPAND_FILENAME = "bsb-expand.pack"
EXPAND_PROVENANCE_FILENAME = "bsb-expand.provenance.json"
OVERLAY_FILENAME = "recall-overlay.pack"
DENSE_FILENAME = "bsb-semantic.pack"
CONTRACT_FILENAME = "semantic-space-arctic-m-v1.5-m256.json"

BASE_ROLE = "canonical-index"
EXPAND_ROLE = "weighted-expansion"
EXPAND_PROVENANCE_ROLE = "expansion-provenance"
OVERLAY_ROLE = "knowledge-kjv-and-reviewed-corrections"
DENSE_ROLE = "dense-arctic-m-v1.5-matryoshka-256d-v2"
CONTRACT_ROLE = "embedding-space-contract"

_COMMON_ARTIFACT_KEYS = frozenset(
    {"filename", "role", "required", "bytes", "sha256"}
)
_DENSE_ARTIFACT_KEYS = _COMMON_ARTIFACT_KEYS | frozenset(
    {
        "lvec_version",
        "dimensions",
        "table_count",
        "embedding_space_id",
        "contract",
    }
)
_RELEASE_ARTIFACTS = {
    BASE_ROLE: (BASE_FILENAME, True, _COMMON_ARTIFACT_KEYS),
    EXPAND_ROLE: (EXPAND_FILENAME, False, _COMMON_ARTIFACT_KEYS),
    EXPAND_PROVENANCE_ROLE: (
        EXPAND_PROVENANCE_FILENAME,
        False,
        _COMMON_ARTIFACT_KEYS,
    ),
    OVERLAY_ROLE: (OVERLAY_FILENAME, False, _COMMON_ARTIFACT_KEYS),
    DENSE_ROLE: (DENSE_FILENAME, False, _DENSE_ARTIFACT_KEYS),
    CONTRACT_ROLE: (CONTRACT_FILENAME, False, _COMMON_ARTIFACT_KEYS),
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _is_sha256(value: object) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def _strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ValueError(f"duplicate JSON key: {key}")
        value[key] = item
    return value


def _reject_json_constant(value: str) -> None:
    raise ValueError(f"unsupported JSON numeric constant: {value}")


def _load_json(path: Path, description: str) -> Any:
    try:
        return json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=_strict_object,
            parse_constant=_reject_json_constant,
        )
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ValueError) as error:
        raise ValueError(f"invalid {description}: {error}") from error


def _align_up(value: int, alignment: int = SECTION_ALIGNMENT) -> int:
    return (value + alignment - 1) & ~(alignment - 1)


def _u16(payload: bytes, offset: int) -> int:
    return int.from_bytes(payload[offset : offset + 2], "little")


def _u32(payload: bytes, offset: int) -> int:
    return int.from_bytes(payload[offset : offset + 4], "little")


def _read_pack(path: Path, expected_tags: tuple[str, ...]) -> dict[str, bytes]:
    """Parse the deterministic pack envelope and return exact section bodies."""

    payload = path.read_bytes()
    if len(payload) < PACK_HEADER_SIZE or payload[:8] != PACK_MAGIC:
        raise ValueError(f"{path.name}: invalid pack header")
    if _u32(payload, 8) != 1:
        raise ValueError(f"{path.name}: unsupported pack version")
    if payload[12:16] != b"\0" * 4 or payload[20:64] != b"\0" * 44:
        raise ValueError(f"{path.name}: unsupported pack flags or reserved header fields")
    section_count = _u32(payload, 16)
    table_end = PACK_HEADER_SIZE + section_count * SECTION_ENTRY_SIZE
    if section_count != len(expected_tags) or table_end > len(payload):
        raise ValueError(f"{path.name}: invalid pack section table")

    data_start = _align_up(table_end)
    if data_start > len(payload) or any(payload[table_end:data_start]):
        raise ValueError(f"{path.name}: non-canonical pack table padding")

    sections: dict[str, bytes] = {}
    previous_end = data_start
    for index, expected_tag in enumerate(expected_tags):
        offset = PACK_HEADER_SIZE + index * SECTION_ENTRY_SIZE
        entry = payload[offset : offset + SECTION_ENTRY_SIZE]
        encoded_tag = expected_tag.encode("ascii").ljust(8, b"\0")
        start = int.from_bytes(entry[8:16], "little")
        length = int.from_bytes(entry[16:24], "little")
        end = start + length
        expected_start = _align_up(previous_end)
        if (
            entry[:8] != encoded_tag
            or entry[24:32] != b"\0" * 8
            or start != expected_start
            or start % SECTION_ALIGNMENT
            or length == 0
            or end > len(payload)
            or any(payload[previous_end:start])
        ):
            raise ValueError(f"{path.name}: invalid {expected_tag} section entry")
        sections[expected_tag] = payload[start:end]
        previous_end = end

    expected_length = _align_up(previous_end)
    if len(payload) != expected_length or any(payload[previous_end:expected_length]):
        raise ValueError(f"{path.name}: non-canonical pack tail")
    return sections


def _inspect_meta(payload: bytes, translation: str) -> int:
    if len(payload) < 10 or _u32(payload, 0) != 1:
        raise ValueError("invalid META section")
    verse_count = _u32(payload, 4)
    translation_length = _u16(payload, 8)
    after_translation = 10 + translation_length
    if after_translation > len(payload):
        raise ValueError("truncated META translation")
    try:
        actual_translation = payload[10:after_translation].decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("invalid META translation") from error
    if actual_translation != translation:
        raise ValueError("META translation does not match release role")

    padded = _align_up(after_translation, 4)
    if len(payload) == after_translation:
        return verse_count
    if len(payload) != padded + 16 or any(payload[after_translation:padded]):
        raise ValueError("invalid META weights or padding")
    for (weight,) in struct.iter_unpack("<f", payload[padded : padded + 16]):
        if weight != 0.0 and (not math.isfinite(weight) or not 0.0 < weight <= 10.0):
            raise ValueError("invalid META weight")
    return verse_count


def _inspect_text(payload: bytes, expected_count: int) -> int:
    if len(payload) < 8:
        raise ValueError("invalid TEXT section")
    verse_count = _u32(payload, 0)
    table_end = 4 + verse_count * 16
    if verse_count != expected_count or table_end + 4 > len(payload):
        raise ValueError("TEXT verse count is inconsistent")
    blob_length = _u32(payload, table_end)
    blob_start = table_end + 4
    if blob_start + blob_length != len(payload):
        raise ValueError("TEXT body length is inconsistent")
    blob = payload[blob_start:]
    seen_references: set[str] = set()
    for verse_index in range(verse_count):
        base = 4 + verse_index * 16
        reference_offset = _u32(payload, base)
        reference_length = _u32(payload, base + 4)
        text_offset = _u32(payload, base + 8)
        text_length = _u32(payload, base + 12)
        reference_end = reference_offset + reference_length
        text_end = text_offset + text_length
        if (
            reference_length == 0
            or text_length == 0
            or reference_end > blob_length
            or text_end > blob_length
        ):
            raise ValueError("TEXT record points outside its blob")
        try:
            reference = blob[reference_offset:reference_end].decode("utf-8")
            text = blob[text_offset:text_end].decode("utf-8")
        except UnicodeDecodeError as error:
            raise ValueError("TEXT contains invalid UTF-8") from error
        normalized_reference = " ".join(
            "".join(character.lower() if character.isalnum() else " " for character in reference).split()
        )
        if not normalized_reference or not text.strip() or normalized_reference in seen_references:
            raise ValueError("TEXT contains an empty or duplicate record")
        seen_references.add(normalized_reference)
    return verse_count


def _inspect_lex(payload: bytes, expected_documents: int) -> list[int]:
    if len(payload) < 24 or payload[:4] != b"LEX1":
        raise ValueError("invalid LEX section")
    if (
        _u16(payload, 4) != 1
        or _u16(payload, 6) != NORMALIZER_VERSION
        or payload[20:24] != b"\0" * 4
    ):
        raise ValueError("unsupported LEX version, normalizer, or reserved field")
    document_count = _u32(payload, 8)
    term_count = _u32(payload, 12)
    blob_length = _u32(payload, 16)
    blob_start = 24 + term_count * 12
    if (
        document_count != expected_documents
        or term_count == 0
        or blob_start + blob_length != len(payload)
    ):
        raise ValueError("LEX dimensions are inconsistent")
    blob = payload[blob_start:]
    expected_offset = 0
    previous_term: str | None = None
    frequencies: list[int] = []
    for term_id in range(term_count):
        base = 24 + term_id * 12
        offset = _u32(payload, base)
        length = _u16(payload, base + 4)
        frequency = _u32(payload, base + 8)
        end = offset + length
        if (
            offset != expected_offset
            or length == 0
            or payload[base + 6 : base + 8] != b"\0" * 2
            or not 0 < frequency <= document_count
            or end > blob_length
        ):
            raise ValueError("invalid LEX term record")
        try:
            term = blob[offset:end].decode("utf-8")
        except UnicodeDecodeError as error:
            raise ValueError("LEX contains invalid UTF-8") from error
        if previous_term is not None and previous_term >= term:
            raise ValueError("LEX dictionary is not strictly sorted")
        previous_term = term
        expected_offset = end
        frequencies.append(frequency)
    if expected_offset != blob_length:
        raise ValueError("LEX blob is not canonical")
    return frequencies


def _decode_varint(payload: bytes, cursor: int, limit: int) -> tuple[int, int]:
    value = 0
    for index in range(5):
        if cursor >= limit:
            raise ValueError("truncated POST varint")
        byte = payload[cursor]
        cursor += 1
        if index == 4 and byte & 0xF0:
            raise ValueError("POST varint overflows u32")
        value |= (byte & 0x7F) << (index * 7)
        if byte & 0x80 == 0:
            if index > 0 and byte == 0:
                raise ValueError("POST varint is not canonical")
            return value, cursor
    raise ValueError("POST varint is too long")


def _inspect_post(
    payload: bytes, expected_documents: int, lexical_frequencies: list[int]
) -> None:
    if len(payload) < 32 or payload[:4] != b"PST1":
        raise ValueError("invalid POST section")
    if _u16(payload, 4) != 1 or payload[6:8] != b"\0" * 2 or payload[24:32] != b"\0" * 8:
        raise ValueError("unsupported POST version or reserved field")
    document_count = _u32(payload, 8)
    term_count = _u32(payload, 12)
    posting_count = _u32(payload, 16)
    blob_length = _u32(payload, 20)
    lengths_end = 32 + document_count * 2
    records_start = _align_up(lengths_end, 4)
    blob_start = records_start + term_count * 16
    if (
        document_count != expected_documents
        or term_count != len(lexical_frequencies)
        or term_count == 0
        or posting_count == 0
        or blob_start + blob_length != len(payload)
        or any(payload[lengths_end:records_start])
    ):
        raise ValueError("POST dimensions are inconsistent")
    if any(_u16(payload, 32 + document_id * 2) == 0 for document_id in range(document_count)):
        raise ValueError("POST contains an empty document")

    expected_offset = 0
    decoded_postings = 0
    for term_id, lexical_frequency in enumerate(lexical_frequencies):
        base = records_start + term_id * 16
        offset = _u32(payload, base)
        length = _u32(payload, base + 4)
        frequency = _u32(payload, base + 8)
        if (
            offset != expected_offset
            or length == 0
            or frequency == 0
            or frequency != lexical_frequency
            or payload[base + 12 : base + 16] != b"\0" * 4
            or offset + length > blob_length
        ):
            raise ValueError("invalid POST term record")
        cursor = blob_start + offset
        limit = cursor + length
        previous_document = 0
        for posting_index in range(frequency):
            delta, cursor = _decode_varint(payload, cursor, limit)
            if cursor >= limit:
                raise ValueError("truncated POST term frequency")
            term_frequency = payload[cursor]
            cursor += 1
            document_id = delta if posting_index == 0 else previous_document + delta
            if (
                (posting_index > 0 and delta == 0)
                or document_id >= document_count
                or term_frequency == 0
            ):
                raise ValueError("invalid POST posting")
            previous_document = document_id
        if cursor != limit:
            raise ValueError("POST posting list has trailing data")
        expected_offset += length
        decoded_postings += frequency
    if expected_offset != blob_length or decoded_postings != posting_count:
        raise ValueError("POST totals are inconsistent")


def inspect_base_pack(path: Path) -> dict[str, object]:
    sections = _read_pack(path, ("META", "TEXT", "LEX", "POST"))
    verse_count = _inspect_meta(sections["META"], CANONICAL_TRANSLATION)
    if verse_count != CANONICAL_VERSE_COUNT:
        raise ValueError("base META verse count is inconsistent")
    _inspect_text(sections["TEXT"], verse_count)
    lexical_frequencies = _inspect_lex(sections["LEX"], verse_count)
    _inspect_post(sections["POST"], verse_count, lexical_frequencies)
    return {"translation": CANONICAL_TRANSLATION, "verse_count": verse_count}


def _inspect_expand_body(payload: bytes) -> dict[str, object]:
    if len(payload) < 72 or payload[:8] != b"VREXP001":
        raise ValueError("invalid EXPAND section")
    version = _u16(payload, 8)
    normalizer_version = _u16(payload, 10)
    verse_count = _u32(payload, 12)
    dictionary_count = _u32(payload, 16)
    entry_count = _u32(payload, 20)
    edge_count = _u32(payload, 24)
    translation_length = _u16(payload, 28)
    blob_length = _u32(payload, 32)
    provenance_sha256 = payload[40:72]
    if (
        version != 2
        or normalizer_version != NORMALIZER_VERSION
        or verse_count != CANONICAL_VERSE_COUNT
        or dictionary_count < 2
        or entry_count == 0
        or edge_count < entry_count
        or edge_count > 8_000_000
        or translation_length == 0
        or translation_length > 256
        or payload[30:32] != b"\0" * 2
        or payload[36:40] != b"\0" * 4
        or not any(provenance_sha256)
    ):
        raise ValueError("unsupported or inconsistent EXPAND header")
    translation_start = 72
    dictionary_start = translation_start + translation_length
    entries_start = dictionary_start + dictionary_count * 8
    edges_start = entries_start + entry_count * 16
    blob_start = edges_start + edge_count * 8
    if blob_start + blob_length != len(payload):
        raise ValueError("EXPAND body length is inconsistent")
    try:
        translation = payload[translation_start:dictionary_start].decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("EXPAND translation is invalid UTF-8") from error
    if translation != CANONICAL_TRANSLATION:
        raise ValueError("EXPAND translation is inconsistent")

    blob = payload[blob_start:]
    expected_offset = 0
    previous_term: str | None = None
    for term_id in range(dictionary_count):
        base = dictionary_start + term_id * 8
        offset = _u32(payload, base)
        length = _u16(payload, base + 4)
        end = offset + length
        if (
            offset != expected_offset
            or not 2 <= length <= 256
            or payload[base + 6 : base + 8] != b"\0" * 2
            or end > blob_length
        ):
            raise ValueError("invalid EXPAND dictionary record")
        try:
            term = blob[offset:end].decode("utf-8")
        except UnicodeDecodeError as error:
            raise ValueError("EXPAND dictionary is invalid UTF-8") from error
        if (
            previous_term is not None and previous_term >= term
        ) or any(character.isupper() or not character.isalnum() for character in term):
            raise ValueError("EXPAND dictionary term is not a normalized stem")
        previous_term = term
        expected_offset = end
    if expected_offset != blob_length:
        raise ValueError("EXPAND dictionary blob is not canonical")

    expected_edge_start = 0
    previous_source: int | None = None
    for entry_index in range(entry_count):
        base = entries_start + entry_index * 16
        source_term_id = _u32(payload, base)
        edge_start = _u32(payload, base + 4)
        local_edge_count = _u16(payload, base + 8)
        if (
            source_term_id >= dictionary_count
            or previous_source is not None and previous_source >= source_term_id
            or edge_start != expected_edge_start
            or not 1 <= local_edge_count <= 64
            or payload[base + 10 : base + 16] != b"\0" * 6
            or edge_start + local_edge_count > edge_count
        ):
            raise ValueError("invalid EXPAND entry record")
        previous_target: int | None = None
        for edge_index in range(edge_start, edge_start + local_edge_count):
            edge_base = edges_start + edge_index * 8
            target_term_id = _u32(payload, edge_base)
            weight = _u16(payload, edge_base + 4)
            if (
                target_term_id >= dictionary_count
                or target_term_id == source_term_id
                or previous_target is not None and previous_target >= target_term_id
                or weight == 0
                or payload[edge_base + 6 : edge_base + 8] != b"\0" * 2
            ):
                raise ValueError("invalid EXPAND edge record")
            previous_target = target_term_id
        expected_edge_start += local_edge_count
        previous_source = source_term_id
    if expected_edge_start != edge_count:
        raise ValueError("EXPAND edge total is inconsistent")
    return {
        "translation": translation,
        "verse_count": verse_count,
        "format_version": version,
        "provenance_sha256": provenance_sha256.hex(),
    }


def inspect_expand_pack(path: Path) -> dict[str, object]:
    sections = _read_pack(path, ("META", "EXPAND"))
    if _inspect_meta(sections["META"], CANONICAL_TRANSLATION) != CANONICAL_VERSE_COUNT:
        raise ValueError("expansion META verse count is inconsistent")
    return _inspect_expand_body(sections["EXPAND"])


def _read_overlay_string(payload: bytes, cursor: int) -> tuple[str, int]:
    if cursor + 4 > len(payload):
        raise ValueError("truncated PHRASE string length")
    length = _u32(payload, cursor)
    cursor += 4
    end = cursor + length
    if length == 0 or length > 1_048_576 or end > len(payload):
        raise ValueError("invalid PHRASE string length")
    try:
        value = payload[cursor:end].decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("PHRASE contains invalid UTF-8") from error
    if not value.strip():
        raise ValueError("PHRASE contains an empty string")
    return value, end


def _inspect_overlay_body(payload: bytes) -> dict[str, object]:
    if len(payload) < 40 or payload[:8] != b"VROVL001":
        raise ValueError("invalid typed PHRASE overlay magic")
    version = _u16(payload, 8)
    verse_count = _u32(payload, 12)
    concept_count = _u32(payload, 16)
    alternate_count = _u32(payload, 20)
    expected_alias_count = _u32(payload, 24)
    expected_range_count = _u32(payload, 28)
    translation_length = _u16(payload, 32)
    body_length = _u32(payload, 36)
    if (
        version != 1
        or payload[10:12] != b"\0" * 2
        or payload[34:36] != b"\0" * 2
        or verse_count != CANONICAL_VERSE_COUNT
        or concept_count + alternate_count == 0
        or concept_count + alternate_count > 1_000_000
        or expected_alias_count + expected_range_count > 4_000_000
        or not 1 <= translation_length <= 256
        or body_length != len(payload) - 40
    ):
        raise ValueError("unsupported or inconsistent PHRASE header")
    translation_end = 40 + translation_length
    try:
        translation = payload[40:translation_end].decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("PHRASE translation is invalid UTF-8") from error
    if translation != CANONICAL_TRANSLATION:
        raise ValueError("PHRASE translation is inconsistent")
    cursor = translation_end
    actual_alias_count = 0
    actual_range_count = 0
    for _ in range(concept_count):
        for _ in range(5):
            _, cursor = _read_overlay_string(payload, cursor)
        if cursor + 8 > len(payload):
            raise ValueError("truncated PHRASE concept counts")
        alias_count = _u32(payload, cursor)
        range_count = _u32(payload, cursor + 4)
        cursor += 8
        if range_count == 0:
            raise ValueError("PHRASE concept has no target range")
        actual_alias_count += alias_count
        actual_range_count += range_count
        if actual_alias_count > expected_alias_count or actual_range_count > expected_range_count:
            raise ValueError("PHRASE child counts exceed the header")
        for _ in range(alias_count):
            _, cursor = _read_overlay_string(payload, cursor)
        for _ in range(range_count * 2):
            _, cursor = _read_overlay_string(payload, cursor)
    for _ in range(alternate_count):
        for _ in range(3):
            _, cursor = _read_overlay_string(payload, cursor)
    if (
        cursor != len(payload)
        or actual_alias_count != expected_alias_count
        or actual_range_count != expected_range_count
    ):
        raise ValueError("PHRASE record totals are inconsistent")
    return {"translation": translation, "verse_count": verse_count, "format_version": version}


def inspect_overlay_pack(path: Path) -> dict[str, object]:
    sections = _read_pack(path, ("META", "PHRASE"))
    if _inspect_meta(sections["META"], "OVERLAY:BSB") != CANONICAL_VERSE_COUNT:
        raise ValueError("overlay META verse count is inconsistent")
    return _inspect_overlay_body(sections["PHRASE"])


def inspect_dense_pack(path: Path) -> dict[str, object]:
    """Parse the release-critical META/LVEC identity without native tooling."""

    sections = _read_pack(path, ("META", "LVEC"))
    verse_count = _inspect_meta(sections["META"], CANONICAL_TRANSLATION)
    lvec = sections["LVEC"]
    if len(lvec) < 48 or lvec[:4] != b"LVC1":
        raise ValueError("invalid LVEC header")
    version = _u16(lvec, 4)
    dimensions = _u16(lvec, 6)
    lvec_verse_count = _u32(lvec, 8)
    table_count = lvec[12]
    if version not in (2, 3):
        raise ValueError("release dense pack must use content-addressed LVEC v2/v3")
    if dimensions == 0 or table_count not in (1, 2):
        raise ValueError("invalid LVEC dimensions or table count")
    if version == 2 and lvec[13:16] != b"\0" * 3:
        raise ValueError("invalid LVEC v2 reserved fields")
    if version == 3 and lvec[15] != 0:
        raise ValueError("invalid LVEC v3 reserved fields")
    space_id = lvec[16:48]
    if not any(space_id):
        raise ValueError("LVEC embedding-space ID must not be zero")
    expected_length = 48 + dimensions * lvec_verse_count * table_count
    if (
        len(lvec) != expected_length
        or verse_count != CANONICAL_VERSE_COUNT
        or lvec_verse_count != verse_count
    ):
        raise ValueError("LVEC body length or verse count is inconsistent")
    return {
        "translation": CANONICAL_TRANSLATION,
        "verse_count": verse_count,
        "lvec_version": version,
        "dimensions": dimensions,
        "table_count": table_count,
        "embedding_space_id": space_id.hex(),
    }


def _inspect_expand_provenance(path: Path) -> dict[str, Any]:
    payload = _load_json(path, "expansion provenance")
    if not isinstance(payload, dict) or frozenset(payload) != {
        "schema_version",
        "kind",
        "canonical_translation",
        "canonical_verse_count",
        "provenance",
    }:
        raise ValueError("expansion provenance has unsupported fields")
    provenance = payload.get("provenance")
    if not isinstance(provenance, dict) or frozenset(provenance) != {
        "builder",
        "normalizer_version",
        "canonical_sha256",
        "embeddings_sha256",
        "vocabulary_sha256",
        "embedding_rows",
        "embedding_dimensions",
        "source_count",
        "edge_count",
        "parameters",
    }:
        raise ValueError("expansion provenance identity has unsupported fields")
    parameters = provenance.get("parameters")
    if not isinstance(parameters, dict) or frozenset(parameters) != {
        "top_k",
        "similarity_floor",
        "batch_size",
        "weight_gamma",
        "weight_calibration",
    }:
        raise ValueError("expansion provenance parameters have unsupported fields")
    positive_integer_fields = (
        "embedding_rows",
        "embedding_dimensions",
        "source_count",
        "edge_count",
    )
    if (
        payload["schema_version"] != 1
        or payload["kind"] != "manifest"
        or payload["canonical_translation"] != CANONICAL_TRANSLATION
        or payload["canonical_verse_count"] != CANONICAL_VERSE_COUNT
        or not isinstance(provenance["builder"], str)
        or not provenance["builder"].strip()
        or provenance["normalizer_version"] != NORMALIZER_VERSION
        or any(not _is_sha256(provenance[field]) for field in (
            "canonical_sha256", "embeddings_sha256", "vocabulary_sha256"
        ))
        or any(
            isinstance(provenance[field], bool)
            or not isinstance(provenance[field], int)
            or provenance[field] <= 0
            for field in positive_integer_fields
        )
        or isinstance(parameters["top_k"], bool)
        or not isinstance(parameters["top_k"], int)
        or not 1 <= parameters["top_k"] <= 64
        or isinstance(parameters["batch_size"], bool)
        or not isinstance(parameters["batch_size"], int)
        or parameters["batch_size"] <= 0
        or isinstance(parameters["similarity_floor"], bool)
        or not isinstance(parameters["similarity_floor"], (int, float))
        or not math.isfinite(parameters["similarity_floor"])
        or not 0.0 <= parameters["similarity_floor"] <= 1.0
        or isinstance(parameters["weight_gamma"], bool)
        or not isinstance(parameters["weight_gamma"], (int, float))
        or not math.isfinite(parameters["weight_gamma"])
        or parameters["weight_gamma"] <= 0.0
        or not isinstance(parameters["weight_calibration"], str)
        or not parameters["weight_calibration"].strip()
    ):
        raise ValueError("expansion provenance contains invalid values")
    return payload


def _validate_artifact_record(artifact: object, index: int) -> tuple[str, str]:
    if not isinstance(artifact, dict):
        raise ValueError(f"artifact {index} is not an object")
    role = artifact.get("role")
    if not isinstance(role, str) or role not in _RELEASE_ARTIFACTS:
        raise ValueError(f"artifact {index} has unknown or missing role")
    expected_filename, expected_required, expected_keys = _RELEASE_ARTIFACTS[role]
    if frozenset(artifact) != expected_keys:
        raise ValueError(f"artifact {index} has missing or unsupported fields")
    filename = artifact.get("filename")
    expected_bytes = artifact.get("bytes")
    expected_sha256 = artifact.get("sha256")
    if (
        filename != expected_filename
        or Path(filename).name != filename
        or artifact.get("required") is not expected_required
        or isinstance(expected_bytes, bool)
        or not isinstance(expected_bytes, int)
        or expected_bytes <= 0
        or not _is_sha256(expected_sha256)
    ):
        raise ValueError(f"artifact {index} has invalid release identity fields")
    return role, filename


def verify(manifest_path: Path, directory: Path) -> list[str]:
    manifest = _load_json(manifest_path, "release-artifact manifest")
    if not isinstance(manifest, dict) or frozenset(manifest) != {
        "schema",
        "canonical_translation",
        "canonical_verse_count",
        "artifacts",
    }:
        raise ValueError("release manifest has missing or unsupported fields")
    if manifest.get("schema") != SCHEMA:
        raise ValueError("unsupported or missing release-artifact schema")
    if manifest.get("canonical_translation") != CANONICAL_TRANSLATION:
        raise ValueError("release manifest must target canonical BSB")
    if manifest.get("canonical_verse_count") != CANONICAL_VERSE_COUNT:
        raise ValueError("release manifest must target 31,086 canonical verses")
    artifacts = manifest.get("artifacts")
    if not isinstance(artifacts, list) or len(artifacts) != len(_RELEASE_ARTIFACTS):
        raise ValueError("release manifest must contain the complete six-artifact set")

    records: dict[str, dict[str, Any]] = {}
    filenames: set[str] = set()
    for index, artifact in enumerate(artifacts, 1):
        role, filename = _validate_artifact_record(artifact, index)
        if role in records:
            raise ValueError(f"duplicate release role: {role}")
        if filename in filenames:
            raise ValueError(f"duplicate release filename: {filename}")
        records[role] = artifact
        filenames.add(filename)
    missing_roles = set(_RELEASE_ARTIFACTS) - set(records)
    if missing_roles:
        raise ValueError("release manifest is missing roles: " + ", ".join(sorted(missing_roles)))

    verified: list[str] = []
    for role in _RELEASE_ARTIFACTS:
        artifact = records[role]
        filename = artifact["filename"]
        path = directory / filename
        if not path.is_file():
            raise ValueError(f"missing release artifact: {path}")
        actual_bytes = path.stat().st_size
        if actual_bytes != artifact["bytes"]:
            raise ValueError(f"{filename}: expected {artifact['bytes']} bytes, got {actual_bytes}")
        actual_sha256 = sha256_file(path)
        if actual_sha256 != artifact["sha256"]:
            raise ValueError(
                f"{filename}: expected SHA-256 {artifact['sha256']}, got {actual_sha256}"
            )
        verified.append(f"{filename}: {actual_bytes} bytes sha256={actual_sha256}")

    inspect_base_pack(directory / BASE_FILENAME)
    expand = inspect_expand_pack(directory / EXPAND_FILENAME)
    _inspect_expand_provenance(directory / EXPAND_PROVENANCE_FILENAME)
    actual_provenance_sha256 = sha256_file(directory / EXPAND_PROVENANCE_FILENAME)
    if expand["provenance_sha256"] != actual_provenance_sha256:
        raise ValueError("EXPAND embedded provenance SHA-256 does not match companion artifact")
    inspect_overlay_pack(directory / OVERLAY_FILENAME)

    contract_record = records[CONTRACT_ROLE]
    contract_path = directory / CONTRACT_FILENAME
    contract_payload = _load_json(contract_path, "embedding-space contract")
    try:
        contract, verified_space_id = validate_envelope(contract_payload)
    except ValueError as error:
        raise ValueError(f"invalid embedding-space contract: {error}") from error

    dense_record = records[DENSE_ROLE]
    if (
        dense_record["contract"] != CONTRACT_FILENAME
        or dense_record["lvec_version"] != 2
        or dense_record["dimensions"] != 256
        or dense_record["table_count"] != 1
        or not _is_sha256(dense_record["embedding_space_id"])
        or dense_record["embedding_space_id"] != verified_space_id
        or contract.get("truncate_dims") != 256
    ):
        raise ValueError("dense release declaration does not match the fixed Arctic-M256 contract")
    dense = inspect_dense_pack(directory / DENSE_FILENAME)
    expected_dense = {
        "translation": CANONICAL_TRANSLATION,
        "verse_count": CANONICAL_VERSE_COUNT,
        "lvec_version": dense_record["lvec_version"],
        "dimensions": dense_record["dimensions"],
        "table_count": dense_record["table_count"],
        "embedding_space_id": verified_space_id,
    }
    if dense != expected_dense:
        raise ValueError("dense pack embedded META/LVEC identity does not match manifest")
    # Access the record so static/type checkers also capture that the companion
    # contract is a required, independently hashed member of this exact set.
    if contract_record["filename"] != dense_record["contract"]:
        raise ValueError("dense contract is not the hashed contract artifact")
    return verified


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path("data/release-artifacts-v2.json"),
    )
    parser.add_argument("--directory", type=Path, default=Path("data"))
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    for line in verify(args.manifest, args.directory):
        print(line)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Mine and compile source-backed Bible misconception records.

This module has two deliberately separate responsibilities:

* The CLI mines *candidates* from explicit correction language in the
  BibleAquifer rows bundled by studybible-mcp. Candidate output is an editorial
  queue and is never a runtime input.
* ``compile_reviewed_corrections`` validates a separately reviewed JSONL file
  against the exact Study Bible database, canonical text, and supplied hidden
  translations. Only approved records become generic ``correction`` concepts.

No benchmark prompts, gold references, or evaluation files are inputs to this
pipeline. The review contract is intentionally strict so a typo, stale source,
unlicensed row, or phrase that is actually Scripture fails closed.
"""

from __future__ import annotations

import argparse
import hashlib
import html
import json
import re
import sqlite3
import sys
import tempfile
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator, Mapping, Sequence


SCHEMA_VERSION = 1
REVIEW_POLICY = "source-backed-correction-v1"
SOURCE_DATASET = "BibleAquifer"
SOURCE_LICENSE = "CC BY-SA 4.0"
PACKED_LICENSE = "CC-BY-SA-4.0"
SOURCE_URL = "https://github.com/djayatillake/studybible-mcp"
MAX_CLAIM_WORDS = 24

# These patterns identify notes that explicitly distinguish the biblical text
# from a claim or tradition. They produce candidates only; matching one never
# grants runtime authority without editorial approval.
EXPLICIT_CORRECTION_PATTERNS: tuple[re.Pattern[str], ...] = tuple(
    re.compile(pattern, re.IGNORECASE)
    for pattern in (
        r"\b(?:the\s+)?(?:text|bible|scripture|passage|verse|account)\s+"
        r"(?:does\s+not|doesn't|never)\s+"
        r"(?:say|state|specify|mention|identify|indicate|record|tell\s+us)\b",
        r"\b(?:not|never)\s+(?:said|stated|specified|mentioned|identified|recorded)\s+"
        r"in\s+(?:the\s+)?(?:text|bible|scripture|passage|verse|account)\b",
        r"\b(?:common|popular|traditional)\s+"
        r"(?:idea|belief|assumption|misconception|misunderstanding)\b",
        r"\b(?:often|commonly|popularly)\s+"
        r"(?:assumed|believed|misunderstood|misquoted)\b",
        r"\b(?:contrary\s+to|despite)\s+(?:a\s+)?(?:popular|common|traditional)\b",
    )
)

REVIEWED_FIELDS = {
    "schema_version",
    "kind",
    "id",
    "claim_surfaces",
    "correction_label",
    "anchor_ref",
    "target_ranges",
    "source",
    "review",
}
SOURCE_FIELDS = {
    "dataset",
    "table",
    "row_id",
    "content_id",
    "resource_type",
    "source_ref",
    "source_url",
    "revision",
    "license",
    "content_sha256",
    "evidence_excerpt",
}
REVIEW_FIELDS = {
    "status",
    "policy",
    "reviewed_by",
    "reviewed_at",
    "rationale",
}
RANGE_FIELDS = {"start_ref", "end_ref"}


@dataclass(frozen=True)
class CorrectionBuild:
    records: tuple[dict, ...]
    input_record_count: int
    alias_count: int
    database_sha256: str

    def manifest(self) -> dict:
        return {
            "concept_type": "correction",
            "source": SOURCE_DATASET,
            "source_table": "aquifer_content",
            "source_url": SOURCE_URL,
            "source_license": SOURCE_LICENSE,
            "review_policy": REVIEW_POLICY,
            "study_bible_sha256": self.database_sha256,
            "input_record_count": self.input_record_count,
            "record_count": len(self.records),
            "alias_count": self.alias_count,
        }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def normalized_text(value: str) -> str:
    return " ".join(html.unescape(value).split())


def runtime_surface(value: str) -> str:
    words: list[str] = []
    current: list[str] = []
    for character in unicodedata.normalize("NFKC", value).casefold():
        if character.isalnum():
            current.append(character)
        elif character in {"'", "\u2018", "\u2019"} and current:
            continue
        elif current:
            words.append("".join(current))
            current = []
    if current:
        words.append("".join(current))
    return " ".join(words)


def contains_surface(text: str, surface: str) -> bool:
    haystack = runtime_surface(text).split()
    needle = runtime_surface(surface).split()
    return bool(needle) and any(
        haystack[index : index + len(needle)] == needle
        for index in range(len(haystack) - len(needle) + 1)
    )


def explicit_correction_match(text: str) -> re.Match[str] | None:
    text = normalized_text(text)
    matches = [pattern.search(text) for pattern in EXPLICIT_CORRECTION_PATTERNS]
    return min((match for match in matches if match), key=lambda match: match.start(), default=None)


def evidence_excerpt(text: str, match: re.Match[str], radius: int = 420) -> str:
    text = normalized_text(text)
    start = max(0, match.start() - radius)
    end = min(len(text), match.end() + radius)
    if start:
        boundary = text.find(" ", start)
        start = boundary + 1 if boundary >= 0 else start
    if end < len(text):
        boundary = text.rfind(" ", 0, end)
        end = boundary if boundary >= 0 else end
    return text[start:end]


def iter_jsonl(path: Path) -> Iterator[tuple[int, dict]]:
    with path.open(encoding="utf-8") as stream:
        for line_no, line in enumerate(stream, 1):
            line = line.strip()
            if not line:
                continue
            try:
                value = json.loads(line)
            except json.JSONDecodeError as exc:
                raise ValueError(f"{path}:{line_no}: {exc}") from exc
            if not isinstance(value, dict):
                raise ValueError(f"{path}:{line_no}: expected a JSON object")
            yield line_no, value


def require_exact_fields(
    value: Mapping[str, object], expected: set[str], location: str
) -> None:
    actual = set(value)
    if actual == expected:
        return
    missing = sorted(expected - actual)
    unknown = sorted(actual - expected)
    raise ValueError(
        f"{location}: schema mismatch; missing={missing}, unknown={unknown}"
    )


def strict_string(value: object, location: str) -> str:
    if not isinstance(value, str) or not value or value != value.strip():
        raise ValueError(f"{location}: expected a non-empty trimmed string")
    if "|" in value or "=" in value:
        raise ValueError(f"{location}: '|' and '=' are reserved")
    return value


def validate_date(value: str, location: str) -> None:
    if re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", value) is None:
        raise ValueError(f"{location}: expected YYYY-MM-DD")


def aquifer_columns(connection: sqlite3.Connection) -> set[str]:
    rows = connection.execute("PRAGMA table_info(aquifer_content)").fetchall()
    columns = {str(row[1]) for row in rows}
    required = {
        "id",
        "content_id",
        "resource_type",
        "title",
        "start_ref",
        "end_ref",
        "content_plain",
    }
    missing = required - columns
    if missing:
        raise ValueError(f"aquifer_content is missing columns {sorted(missing)}")
    return columns


def load_source_rows(
    database_path: Path,
) -> tuple[str, dict[int, sqlite3.Row]]:
    database_sha256 = sha256_file(database_path)
    connection = sqlite3.connect(f"file:{database_path}?mode=ro", uri=True)
    connection.row_factory = sqlite3.Row
    try:
        aquifer_columns(connection)
        rows = connection.execute(
            """
            SELECT id, content_id, resource_type, title, start_ref, end_ref,
                   content_plain
            FROM aquifer_content
            ORDER BY id
            """
        ).fetchall()
    finally:
        connection.close()
    return database_sha256, {int(row["id"]): row for row in rows}


def mine_candidates(database_path: Path) -> tuple[dict, tuple[dict, ...]]:
    database_sha256, rows = load_source_rows(database_path)
    candidates: list[dict] = []
    for row_id, row in rows.items():
        content = normalized_text(str(row["content_plain"]))
        match = explicit_correction_match(content)
        if match is None:
            continue
        excerpt = evidence_excerpt(content, match)
        candidates.append(
            {
                "schema_version": SCHEMA_VERSION,
                "kind": "correction_candidate",
                "id": f"aquifer.{row_id}",
                "claim_surfaces": [],
                "correction_label": "",
                "anchor_ref": "",
                "target_ranges": [],
                "source": {
                    "dataset": SOURCE_DATASET,
                    "table": "aquifer_content",
                    "row_id": row_id,
                    "content_id": int(row["content_id"]),
                    "resource_type": str(row["resource_type"]),
                    "source_ref": str(row["title"]),
                    "source_url": SOURCE_URL,
                    "revision": f"sha256:{database_sha256}",
                    "license": SOURCE_LICENSE,
                    "content_sha256": hashlib.sha256(content.encode("utf-8")).hexdigest(),
                    "evidence_excerpt": excerpt,
                    "start_ref": row["start_ref"],
                    "end_ref": row["end_ref"],
                },
                "review": {
                    "status": "candidate",
                    "policy": REVIEW_POLICY,
                    "reviewed_by": "",
                    "reviewed_at": "",
                    "rationale": "",
                },
            }
        )
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "kind": "correction_candidate_manifest",
        "source": SOURCE_DATASET,
        "source_table": "aquifer_content",
        "source_url": SOURCE_URL,
        "source_license": SOURCE_LICENSE,
        "study_bible_sha256": database_sha256,
        "input_row_count": len(rows),
        "candidate_count": len(candidates),
        "candidate_policy": "explicit-correction-language-v1",
        "runtime_eligible": False,
    }
    return manifest, tuple(candidates)


def write_candidates(database_path: Path, output_path: Path) -> dict:
    manifest, candidates = mine_candidates(database_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        "w",
        encoding="utf-8",
        dir=output_path.parent,
        prefix=f".{output_path.name}.",
        suffix=".tmp",
        delete=False,
    ) as stream:
        temporary_path = Path(stream.name)
        try:
            for value in (manifest, *candidates):
                stream.write(
                    json.dumps(
                        value,
                        ensure_ascii=False,
                        sort_keys=True,
                        separators=(",", ":"),
                    )
                    + "\n"
                )
        except Exception:
            temporary_path.unlink(missing_ok=True)
            raise
    temporary_path.replace(output_path)
    return manifest


def load_scripture_jsonl(path: Path) -> tuple[tuple[str, ...], tuple[str, ...]]:
    """Load a simple ``ref``/``text`` Scripture corpus for review validation."""

    refs: list[str] = []
    texts: list[str] = []
    for line_no, row in iter_jsonl(path):
        reference = row.get("ref")
        text = row.get("text")
        if not isinstance(reference, str) or not reference.strip():
            raise ValueError(f"{path}:{line_no}: expected a non-empty ref")
        if reference != reference.strip():
            raise ValueError(f"{path}:{line_no}: ref must be trimmed")
        if not isinstance(text, str) or not text.strip():
            raise ValueError(f"{path}:{line_no}: expected non-empty Scripture text")
        refs.append(reference)
        texts.append(text)
    if not refs:
        raise ValueError(f"{path}: no Scripture rows")
    return tuple(refs), tuple(texts)


def compile_reviewed_corrections(
    *,
    reviewed_path: Path,
    database_path: Path,
    canonical_refs: Sequence[str],
    canonical_texts: Sequence[str],
    alternate_texts: Iterable[str],
) -> CorrectionBuild:
    if len(canonical_refs) != len(canonical_texts) or not canonical_refs:
        raise ValueError("canonical refs/texts must be non-empty and aligned")
    ref_indexes: dict[str, int] = {}
    for index, reference in enumerate(canonical_refs):
        if reference in ref_indexes:
            raise ValueError(f"canonical display ref occurs twice: {reference!r}")
        ref_indexes[reference] = index

    database_sha256, source_rows = load_source_rows(database_path)
    scripture_texts = [*canonical_texts, *alternate_texts]
    records: list[dict] = []
    occupied_ids: set[str] = set()
    occupied_surfaces: dict[str, str] = {}
    alias_count = 0
    input_count = 0

    for line_no, raw in iter_jsonl(reviewed_path):
        input_count += 1
        location = f"{reviewed_path}:{line_no}"
        require_exact_fields(raw, REVIEWED_FIELDS, location)
        if raw["schema_version"] != SCHEMA_VERSION or raw["kind"] != "correction":
            raise ValueError(f"{location}: expected schema_version 1 correction")
        record_id = strict_string(raw["id"], f"{location} id")
        if record_id in occupied_ids:
            raise ValueError(f"{location}: duplicate id {record_id!r}")
        occupied_ids.add(record_id)

        source = raw["source"]
        if not isinstance(source, dict):
            raise ValueError(f"{location} source: expected an object")
        require_exact_fields(source, SOURCE_FIELDS, f"{location} source")
        if source["dataset"] != SOURCE_DATASET or source["table"] != "aquifer_content":
            raise ValueError(f"{location}: unsupported source dataset or table")
        if source["source_url"] != SOURCE_URL or source["license"] != SOURCE_LICENSE:
            raise ValueError(f"{location}: source URL/license does not match BibleAquifer")
        if source["revision"] != f"sha256:{database_sha256}":
            raise ValueError(f"{location}: stale Study Bible database revision")
        row_id = source["row_id"]
        if not isinstance(row_id, int) or row_id <= 0 or row_id not in source_rows:
            raise ValueError(f"{location}: source row_id is missing from aquifer_content")
        expected_record_id = f"aquifer.{row_id}"
        if record_id != expected_record_id:
            raise ValueError(
                f"{location}: id must identify source row_id as "
                f"{expected_record_id!r}"
            )
        row = source_rows[row_id]
        if source["content_id"] != row["content_id"]:
            raise ValueError(f"{location}: source content_id does not match database")
        if source["resource_type"] != row["resource_type"]:
            raise ValueError(f"{location}: source resource_type does not match database")
        if source["source_ref"] != row["title"]:
            raise ValueError(f"{location}: source_ref does not match database title")
        source_content = normalized_text(str(row["content_plain"]))
        source_hash = hashlib.sha256(source_content.encode("utf-8")).hexdigest()
        if source["content_sha256"] != source_hash:
            raise ValueError(f"{location}: source content hash does not match database")
        excerpt = source["evidence_excerpt"]
        if not isinstance(excerpt, str) or len(excerpt) < 20 or normalized_text(excerpt) not in source_content:
            raise ValueError(f"{location}: evidence_excerpt is not present in source content")
        if explicit_correction_match(source_content) is None:
            raise ValueError(f"{location}: source row lacks explicit correction language")

        review = raw["review"]
        if not isinstance(review, dict):
            raise ValueError(f"{location} review: expected an object")
        require_exact_fields(review, REVIEW_FIELDS, f"{location} review")
        if review["status"] != "approved" or review["policy"] != REVIEW_POLICY:
            raise ValueError(f"{location}: record is not approved under {REVIEW_POLICY}")
        reviewer = strict_string(review["reviewed_by"], f"{location} reviewed_by")
        reviewed_at = strict_string(review["reviewed_at"], f"{location} reviewed_at")
        validate_date(reviewed_at, f"{location} reviewed_at")
        rationale = strict_string(review["rationale"], f"{location} rationale")
        if len(rationale) < 20:
            raise ValueError(f"{location}: review rationale must contain at least 20 characters")

        claim_surfaces = raw["claim_surfaces"]
        if not isinstance(claim_surfaces, list) or not claim_surfaces:
            raise ValueError(f"{location}: claim_surfaces must be a non-empty array")
        normalized_claims: list[str] = []
        for claim_number, claim in enumerate(claim_surfaces, 1):
            claim = strict_string(claim, f"{location} claim {claim_number}")
            surface = runtime_surface(claim)
            word_count = len(surface.split())
            if not 2 <= word_count <= MAX_CLAIM_WORDS:
                raise ValueError(
                    f"{location}: claim surfaces must contain 2–{MAX_CLAIM_WORDS} words"
                )
            if surface != claim:
                raise ValueError(
                    f"{location}: claim {claim_number} must be normalized as {surface!r}"
                )
            if any(contains_surface(text, surface) for text in scripture_texts):
                raise ValueError(
                    f"{location}: claim surface {surface!r} occurs in canonical or alternate Scripture"
                )
            if surface in normalized_claims:
                raise ValueError(f"{location}: duplicate normalized claim surface {surface!r}")
            if surface in occupied_surfaces:
                raise ValueError(
                    f"{location}: claim surface {surface!r} repeats record {occupied_surfaces[surface]!r}"
                )
            occupied_surfaces[surface] = record_id
            normalized_claims.append(surface)
        normalized_claims.sort()

        correction_label = strict_string(
            raw["correction_label"], f"{location} correction_label"
        )
        anchor_ref = strict_string(raw["anchor_ref"], f"{location} anchor_ref")
        anchor_index = ref_indexes.get(anchor_ref)
        if anchor_index is None:
            raise ValueError(f"{location}: anchor_ref is not an exact canonical display ref")
        raw_ranges = raw["target_ranges"]
        if not isinstance(raw_ranges, list) or not raw_ranges:
            raise ValueError(f"{location}: target_ranges must be a non-empty array")
        target_ranges: list[dict] = []
        previous_end = -1
        primary: tuple[int, int] | None = None
        for range_number, raw_range in enumerate(raw_ranges, 1):
            range_location = f"{location} target range {range_number}"
            if not isinstance(raw_range, dict):
                raise ValueError(f"{range_location}: expected an object")
            require_exact_fields(raw_range, RANGE_FIELDS, range_location)
            start_ref = strict_string(raw_range["start_ref"], f"{range_location} start_ref")
            end_ref = strict_string(raw_range["end_ref"], f"{range_location} end_ref")
            start = ref_indexes.get(start_ref)
            end = ref_indexes.get(end_ref)
            if start is None or end is None:
                raise ValueError(f"{range_location}: endpoint is not an exact canonical ref")
            if start > end or start <= previous_end:
                raise ValueError(f"{range_location}: ranges must be ordered and non-overlapping")
            # Runtime separately validates same-book boundaries; this simple
            # display-ref check prevents the reviewed artifact from relying on
            # implicit numeric reference parsing.
            start_book = start_ref.rsplit(" ", 1)[0]
            end_book = end_ref.rsplit(" ", 1)[0]
            if start_book != end_book:
                raise ValueError(f"{range_location}: range crosses a canonical book")
            previous_end = end
            primary = primary or (start, end)
            target_ranges.append(
                {
                    "start_ref": start_ref,
                    "end_ref": end_ref,
                    "verse_count": end - start + 1,
                }
            )
        assert primary is not None
        if not primary[0] <= anchor_index <= primary[1]:
            raise ValueError(f"{location}: anchor_ref must be inside the primary range")

        compact_source = "|".join(
            (
                "correction-v1",
                f"id={record_id}",
                f"dataset={SOURCE_DATASET}",
                f"record={row_id}",
                f"revision={database_sha256}",
                f"license={PACKED_LICENSE}",
                f"content-sha256={source_hash}",
                "review-status=approved",
                f"review-policy={REVIEW_POLICY}",
                f"reviewed-by={reviewer}",
                f"reviewed-at={reviewed_at}",
            )
        )
        records.append(
            {
                "schema_version": SCHEMA_VERSION,
                "kind": "concept",
                "concept_type": "correction",
                "key": normalized_claims[0],
                "label": correction_label,
                "aliases": normalized_claims[1:],
                "source": compact_source,
                "anchor_ref": anchor_ref,
                "target_ranges": target_ranges,
            }
        )
        alias_count += len(normalized_claims) - 1

    if not records:
        raise ValueError(f"{reviewed_path}: no approved correction records")
    records.sort(key=lambda record: (record["key"], record["source"]))
    return CorrectionBuild(tuple(records), input_count, alias_count, database_sha256)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--study-bible-db", required=True, type=Path)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument(
        "--output",
        "-o",
        type=Path,
        help="mine a non-runtime candidate queue",
    )
    mode.add_argument(
        "--validate-reviewed",
        type=Path,
        metavar="JSONL",
        help="validate a reviewed corpus without writing an overlay",
    )
    parser.add_argument(
        "--canonical",
        type=Path,
        help="canonical ref/text JSONL required by --validate-reviewed",
    )
    parser.add_argument(
        "--alternate",
        action="append",
        default=[],
        type=Path,
        help="optional alternate ref/text JSONL checked for claim collisions",
    )
    args = parser.parse_args()
    try:
        if args.output is not None:
            if args.canonical is not None or args.alternate:
                parser.error("--canonical/--alternate require --validate-reviewed")
            manifest = write_candidates(args.study_bible_db, args.output)
            print(
                f"wrote {manifest['candidate_count']} non-runtime correction "
                f"candidates from {manifest['input_row_count']} Aquifer rows -> "
                f"{args.output}",
                file=sys.stderr,
            )
            return 0

        if args.canonical is None:
            parser.error("--validate-reviewed requires --canonical")
        canonical_refs, canonical_texts = load_scripture_jsonl(args.canonical)
        alternate_texts: list[str] = []
        for alternate_path in args.alternate:
            _, texts = load_scripture_jsonl(alternate_path)
            alternate_texts.extend(texts)
        build = compile_reviewed_corrections(
            reviewed_path=args.validate_reviewed,
            database_path=args.study_bible_db,
            canonical_refs=canonical_refs,
            canonical_texts=canonical_texts,
            alternate_texts=alternate_texts,
        )
        manifest = {
            **build.manifest(),
            "reviewed_sha256": sha256_file(args.validate_reviewed),
            "canonical_sha256": sha256_file(args.canonical),
            "alternate_sha256": [sha256_file(path) for path in args.alternate],
        }
        print(json.dumps(manifest, sort_keys=True, separators=(",", ":")))
        print(
            f"validated {manifest['record_count']} reviewed corrections and "
            f"{manifest['alias_count']} aliases",
            file=sys.stderr,
        )
        return 0
    except (OSError, ValueError, sqlite3.Error) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Build a deterministic, source-wide semantic passage corpus.

The resulting rows are query-independent.  Each row contains canonical BSB
text plus an explicit canonical start/end range from the Study Bible passage
table.  The raw database's opaque precomputed vectors are deliberately not
used: passage text is re-encoded with the same verified encoder contract as
queries and canonical verses.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import sqlite3
from typing import Any

try:
    from scripts.build_semantic_documents import (
        display_coordinate,
        jsonl,
    )
except ModuleNotFoundError:  # Direct `python scripts/...` execution.
    from build_semantic_documents import display_coordinate, jsonl


SCHEMA_VERSION = 1
SOURCE_URL = "https://github.com/djayatillake/studybible-mcp"
SOURCE_LICENSE = "CC BY-SA 4.0"
DEFAULT_MAX_VERSES = 30
STUDY_BOOK_CODES = (
    "Gen", "Exo", "Lev", "Num", "Deu", "Jos", "Jdg", "Rut", "1Sa", "2Sa",
    "1Ki", "2Ki", "1Ch", "2Ch", "Ezr", "Neh", "Est", "Job", "Psa", "Pro",
    "Ecc", "Sng", "Isa", "Jer", "Lam", "Ezk", "Dan", "Hos", "Jol", "Amo",
    "Oba", "Jon", "Mic", "Nam", "Hab", "Zep", "Hag", "Zec", "Mal", "Mat",
    "Mrk", "Luk", "Jhn", "Act", "Rom", "1Co", "2Co", "Gal", "Eph", "Php",
    "Col", "1Th", "2Th", "1Ti", "2Ti", "Tit", "Phm", "Heb", "Jas", "1Pe",
    "2Pe", "1Jn", "2Jn", "3Jn", "Jud", "Rev",
)
STUDY_REF = re.compile(
    r"^(?P<book>[1-3]?[A-Za-z]+)\.(?P<chapter>[1-9][0-9]*)\.(?P<verse>[1-9][0-9]*)"
    r"(?:\[[1-9][0-9]*\.[1-9][0-9]*\]|\([1-9][0-9]*\.[1-9][0-9]*\)|"
    r"\{[1-9][0-9]*\.[1-9][0-9]*\})*$"
)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _canonical_rows(
    path: Path,
) -> tuple[
    list[dict[str, Any]],
    dict[tuple[str, int, int], int],
    dict[str, str],
]:
    rows = jsonl(path)
    if not rows:
        raise ValueError("canonical corpus is empty")
    by_coordinate: dict[tuple[str, int, int], int] = {}
    seen_refs: set[str] = set()
    ordered_books: list[str] = []
    for index, row in enumerate(rows):
        reference, text = row.get("ref"), row.get("text")
        if not isinstance(reference, str) or not isinstance(text, str) or not text.strip():
            raise ValueError(f"canonical row {index + 1} requires non-empty ref and text")
        if reference in seen_refs:
            raise ValueError(f"duplicate canonical reference {reference}")
        seen_refs.add(reference)
        coordinate = display_coordinate(reference)
        if coordinate in by_coordinate:
            raise ValueError(f"duplicate canonical coordinate {coordinate}")
        by_coordinate[coordinate] = index
        if not ordered_books or ordered_books[-1] != coordinate[0]:
            if coordinate[0] in ordered_books:
                raise ValueError(f"canonical book {coordinate[0]} is not contiguous")
            ordered_books.append(coordinate[0])
    if len(ordered_books) > len(STUDY_BOOK_CODES):
        raise ValueError("canonical corpus has more books than the Study Bible schema")
    return rows, by_coordinate, dict(zip(STUDY_BOOK_CODES, ordered_books))


def _study_coordinate(raw: str, book_map: dict[str, str]) -> tuple[str, int, int] | None:
    match = STUDY_REF.fullmatch(raw.strip())
    if match is None or match.group("book") not in book_map:
        return None
    return (
        book_map[match.group("book")],
        int(match.group("chapter")),
        int(match.group("verse")),
    )


def build(
    canonical: Path,
    database: Path,
    *,
    max_verses: int = DEFAULT_MAX_VERSES,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    if not 1 <= max_verses <= 256:
        raise ValueError("max_verses must be between 1 and 256")
    canonical_rows, by_coordinate, book_map = _canonical_rows(canonical)
    exclusions: dict[str, int] = {
        "invalid_reference": 0,
        "noncanonical_endpoint": 0,
        "cross_book": 0,
        "invalid_span": 0,
        "over_max_verses": 0,
        "duplicate_range": 0,
    }
    candidates: list[tuple[int, int, int, dict[str, Any]]] = []
    seen_ranges: set[tuple[int, int]] = set()
    uri = f"file:{database.resolve()}?mode=ro"
    with sqlite3.connect(uri, uri=True) as connection:
        tables = {
            row[0]
            for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")
        }
        if "passages" not in tables:
            raise ValueError(f"{database}: missing passages table")
        raw_rows = list(
            connection.execute(
                """
                SELECT id, reference_start, reference_end, verse_count, section_type
                FROM passages
                ORDER BY id
                """
            )
        )

    for passage_id, raw_start, raw_end, raw_count, section_type in raw_rows:
        start = _study_coordinate(raw_start or "", book_map)
        end = _study_coordinate(raw_end or "", book_map)
        if start is None or end is None:
            exclusions["invalid_reference"] += 1
            continue
        if start[0] != end[0]:
            exclusions["cross_book"] += 1
            continue
        if start not in by_coordinate or end not in by_coordinate:
            exclusions["noncanonical_endpoint"] += 1
            continue
        start_index, end_index = by_coordinate[start], by_coordinate[end]
        span = end_index - start_index + 1
        if start_index > end_index or span <= 0 or span != int(raw_count):
            exclusions["invalid_span"] += 1
            continue
        if span > max_verses:
            exclusions["over_max_verses"] += 1
            continue
        range_key = (start_index, end_index)
        if range_key in seen_ranges:
            exclusions["duplicate_range"] += 1
            continue
        seen_ranges.add(range_key)
        start_ref = canonical_rows[start_index]["ref"]
        end_ref = canonical_rows[end_index]["ref"]
        text = " ".join(
            canonical_rows[index]["text"].strip()
            for index in range(start_index, end_index + 1)
        )
        candidates.append(
            (
                start_index,
                end_index,
                int(passage_id),
                {
                    "id": f"study-bible.passage.{int(passage_id)}",
                    "anchor_ref": start_ref,
                    "start_ref": start_ref,
                    "end_ref": end_ref,
                    "anchor_index": start_index,
                    "start_index": start_index,
                    "end_index": end_index,
                    "source_kind": "study-bible-passage",
                    "section_type": section_type or "unspecified",
                    "text": text,
                },
            )
        )

    candidates.sort(key=lambda item: (item[0], item[1], item[2]))
    documents = [item[3] for item in candidates]
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "kind": "semantic-passage-corpus",
        "builder": "scripts/build_passage_documents.py",
        "canonical": {
            "path": str(canonical),
            "verse_count": len(canonical_rows),
            "sha256": sha256_file(canonical),
        },
        "source": {
            "dataset": "studybible-mcp passages",
            "url": SOURCE_URL,
            "license": SOURCE_LICENSE,
            "database_sha256": sha256_file(database),
            "raw_passage_count": len(raw_rows),
        },
        "parameters": {"max_verses": max_verses},
        "included_passage_count": len(documents),
        "excluded_passage_count": sum(exclusions.values()),
        "exclusions": exclusions,
    }
    if len(documents) + manifest["excluded_passage_count"] != len(raw_rows):
        raise AssertionError("passage accounting is incomplete")
    if not documents:
        raise ValueError("no canonical passages survived validation")
    return documents, manifest


def write_outputs(documents: list[dict[str, Any]], manifest: dict[str, Any], output: Path) -> Path:
    output.parent.mkdir(parents=True, exist_ok=True)
    rendered = "".join(
        json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n"
        for row in documents
    )
    output.write_text(rendered, encoding="utf-8")
    manifest = dict(manifest)
    manifest["output"] = {
        "path": str(output),
        "bytes": output.stat().st_size,
        "sha256": sha256_file(output),
    }
    manifest_path = output.with_suffix(".manifest.json")
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest_path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--study-bible", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--max-verses", type=int, default=DEFAULT_MAX_VERSES)
    args = parser.parse_args()
    documents, manifest = build(
        args.canonical,
        args.study_bible,
        max_verses=args.max_verses,
    )
    manifest_path = write_outputs(documents, manifest, args.output)
    print(
        f"wrote {len(documents)} canonical semantic passages -> {args.output} "
        f"(manifest {manifest_path})"
    )


if __name__ == "__main__":
    main()

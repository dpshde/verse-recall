#!/usr/bin/env python3
"""Build a deterministic, verse-aligned dense view of Study Note prose.

The canonical verse embedding remains table 1 of LVEC. This tool prepares a
separate table-2 document for every canonical coordinate from the
``study_context`` field emitted by ``build_semantic_documents.py``. Verses
without an independently authored Study Note use their exact canonical text.
That fallback is intentionally conservative: it is non-empty, does not smear a
nearby note onto an unrelated coordinate, and becomes a duplicate of table 1
when both views are encoded under the same contract.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path


DEFAULT_MAX_CHARS = 1_800


def jsonl(path: Path) -> list[dict]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError(f"{path}:{line_number}: expected an object")
        rows.append(value)
    return rows


def normalized_text(value: object) -> str:
    if not isinstance(value, str):
        return ""
    return re.sub(r"\s+", " ", value).strip()


def bounded_text(value: str, max_chars: int) -> str:
    if len(value) <= max_chars:
        return value
    prefix = value[:max_chars]
    boundary = prefix.rfind(" ")
    return (prefix[:boundary] if boundary > 0 else prefix).rstrip()


def build(canonical: Path, documents: Path, max_chars: int = DEFAULT_MAX_CHARS) -> list[dict]:
    if max_chars <= 0:
        raise ValueError("max_chars must be positive")
    canonical_rows = jsonl(canonical)
    document_rows = jsonl(documents)
    if not canonical_rows:
        raise ValueError("canonical input is empty")
    if len(document_rows) != len(canonical_rows):
        raise ValueError(
            "semantic documents must have exactly one row per canonical verse "
            f"({len(document_rows)} != {len(canonical_rows)})"
        )

    output = []
    seen_refs = set()
    for index, (canonical_row, document_row) in enumerate(
        zip(canonical_rows, document_rows), 1
    ):
        reference = canonical_row.get("ref")
        canonical_text = normalized_text(canonical_row.get("text"))
        document_reference = document_row.get("ref")
        if not isinstance(reference, str) or not reference.strip() or not canonical_text:
            raise ValueError(f"canonical row {index} requires non-empty ref and text")
        if reference in seen_refs:
            raise ValueError(f"duplicate canonical reference {reference}")
        seen_refs.add(reference)
        if document_reference != reference:
            raise ValueError(
                f"semantic document row {index} is not canonical-aligned: "
                f"{document_reference!r} != {reference!r}"
            )

        study_context = normalized_text(document_row.get("study_context"))
        has_study_note = bool(study_context)
        text = bounded_text(study_context if has_study_note else canonical_text, max_chars)
        if not text:
            # Defensive invariant for downstream encoders: every aligned row
            # must produce a finite, non-zero embedding rather than a zero
            # sentinel that could acquire arbitrary cosine rank.
            raise ValueError(f"dense view row {index} normalized to empty text")
        output.append(
            {
                "ref": reference,
                "text": text,
                "view": "study-note" if has_study_note else "canonical-fallback",
            }
        )
    return output


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--documents", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--max-chars", type=int, default=DEFAULT_MAX_CHARS)
    args = parser.parse_args()

    rows = build(args.canonical, args.documents, args.max_chars)
    rendered = "".join(
        json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n"
        for row in rows
    )
    args.output.write_text(rendered, encoding="utf-8")
    covered = sum(row["view"] == "study-note" for row in rows)
    print(
        f"wrote {len(rows)} aligned study-note documents "
        f"({covered} notes, {len(rows) - covered} canonical fallbacks, "
        f"max {args.max_chars} chars) -> {args.output}"
    )


if __name__ == "__main__":
    main()

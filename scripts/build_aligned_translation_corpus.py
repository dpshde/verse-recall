#!/usr/bin/env python3
"""Build a canonical-coordinate semantic corpus from a hidden translation.

Every coordinate is represented exactly once.  Missing alternate rows fall
back to canonical text and are reported; source-only verses are rejected from
the aligned output rather than shifted by row position.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def read_jsonl(path: Path) -> list[dict]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError(f"{path}:{line_number}: expected an object")
        rows.append(value)
    return rows


def build(canonical_path: Path, overlay_path: Path, translation: str) -> tuple[list[dict], dict]:
    translation = translation.strip()
    if not translation:
        raise ValueError("translation must not be empty")
    canonical = read_jsonl(canonical_path)
    canonical_refs = []
    canonical_text = {}
    for row_number, row in enumerate(canonical, 1):
        reference, text = row.get("ref"), row.get("text")
        if not isinstance(reference, str) or not isinstance(text, str) or not text.strip():
            raise ValueError(f"{canonical_path}:{row_number}: invalid canonical row")
        if reference in canonical_text:
            raise ValueError(f"{canonical_path}:{row_number}: duplicate ref {reference!r}")
        canonical_refs.append(reference)
        canonical_text[reference] = text.strip()

    alternate = {}
    source_only = []
    for row in read_jsonl(overlay_path):
        if row.get("kind") != "alternate_text" or row.get("source_translation") != translation:
            continue
        target, text = row.get("target_ref"), row.get("text")
        if not isinstance(target, str) or not isinstance(text, str) or not text.strip():
            raise ValueError(f"{overlay_path}: malformed {translation} alternate row")
        if target not in canonical_text:
            source_only.append(target)
            continue
        if target in alternate:
            raise ValueError(f"{overlay_path}: duplicate {translation} target {target!r}")
        alternate[target] = text.strip()

    rows = [
        {
            "ref": reference,
            "text": alternate.get(reference, canonical_text[reference]),
            "source_translation": translation if reference in alternate else "canonical-fallback",
        }
        for reference in canonical_refs
    ]
    missing = [reference for reference in canonical_refs if reference not in alternate]
    manifest = {
        "schema": "verse-recall-aligned-translation-corpus-v1",
        "translation": translation,
        "canonical_rows": len(canonical_refs),
        "matched_rows": len(alternate),
        "missing_rows": len(missing),
        "missing_ref_sample": missing[:20],
        "source_only_rows": len(source_only),
        "source_only_ref_sample": sorted(source_only)[:20],
    }
    return rows, manifest


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--overlay-jsonl", type=Path, required=True)
    parser.add_argument("--translation", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--manifest", type=Path, required=True)
    args = parser.parse_args()

    rows, manifest = build(args.canonical, args.overlay_jsonl, args.translation)
    with args.output.open("w", encoding="utf-8") as output:
        for row in rows:
            output.write(json.dumps(row, separators=(",", ":")) + "\n")
    args.manifest.write_text(
        json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    print(
        f"wrote {len(rows)} aligned {args.translation} rows "
        f"({manifest['missing_rows']} canonical fallbacks) -> {args.output}"
    )


if __name__ == "__main__":
    main()

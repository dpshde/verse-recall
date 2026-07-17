#!/usr/bin/env python3
"""Normalize Arweave / selah-tools BSB JSONL into packer input.

Input lines may include ref/book/chapter/verseNum/text (and optional events/entities).
Output lines are {"ref": "Genesis 1:1", "text": "..."} with display refs for eval gold.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


def display_ref(row: dict) -> str:
    book = (row.get("book") or "").strip()
    chapter = str(row.get("chapter") or "").strip()
    verse = str(row.get("verseNum") or row.get("verse") or "").strip()
    if book == "Psalms":
        book = "Psalm"
    if book and chapter and verse:
        return f"{book} {chapter}:{verse}"
    # Fallback: already-display ref
    ref = row.get("ref") or ""
    if " " in ref and ":" in ref:
        return ref
    return ref


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--input", "-i", required=True, type=Path)
    ap.add_argument("--output", "-o", required=True, type=Path)
    args = ap.parse_args()

    books: set[str] = set()
    n = 0
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.input.open(encoding="utf-8") as fin, args.output.open(
        "w", encoding="utf-8"
    ) as fout:
        for line_no, line in enumerate(fin, 1):
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as e:
                print(f"line {line_no}: {e}", file=sys.stderr)
                return 1
            text = row.get("text")
            if not text:
                print(f"line {line_no}: missing text", file=sys.stderr)
                return 1
            ref = display_ref(row)
            fout.write(json.dumps({"ref": ref, "text": text}, ensure_ascii=False) + "\n")
            n += 1
            books.add(ref.split()[0] if ref else "")

    print(f"wrote {n} verses, ~{len(books)} book tokens → {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

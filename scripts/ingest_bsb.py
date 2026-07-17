#!/usr/bin/env python3
"""Normalize Arweave / selah-tools BSB JSONL into packer and overlay input.

Input lines may include ref/book/chapter/verseNum/text (and optional events/
entities). Output lines always carry display refs for eval gold:

  {"ref": "Genesis 1:1", "text": "..."}

When the source supplies a non-empty ``events`` string array, it is preserved
verbatim (empty strings dropped). The base packer still indexes only ref/text;
``build_recall_overlay.py`` reads the same file for ranked BSB event concepts.
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


def normalize_events(row: dict, line_no: int) -> list[str] | None:
    """Return a cleaned events list, or None when the field is absent/empty."""

    if "events" not in row:
        return None
    raw = row.get("events")
    if raw is None:
        return None
    if not isinstance(raw, list) or not all(isinstance(event, str) for event in raw):
        raise ValueError(f"line {line_no}: events must be a string array")
    events = [event.strip() for event in raw if event.strip()]
    return events or None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--input", "-i", required=True, type=Path)
    ap.add_argument("--output", "-o", required=True, type=Path)
    args = ap.parse_args()

    books: set[str] = set()
    n = 0
    with_events = 0
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
            try:
                events = normalize_events(row, line_no)
            except ValueError as error:
                print(error, file=sys.stderr)
                return 1
            record: dict = {"ref": ref, "text": text}
            if events is not None:
                record["events"] = events
                with_events += 1
            fout.write(json.dumps(record, ensure_ascii=False) + "\n")
            n += 1
            books.add(ref.split()[0] if ref else "")

    print(
        f"wrote {n} verses ({with_events} with events), "
        f"~{len(books)} book tokens → {args.output}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Build a deterministic, verse-aligned semantic document corpus.

The dense lane's first table represents individual canonical verses. This tool
builds the complementary second table from paragraph context, hidden alternate
wording, and trusted non-topical graph labels. It deliberately excludes Selah
topic labels: those are useful as separately calibrated graph evidence, but are
too noisy to become free-form dense-document truth.
"""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
from collections import defaultdict
from pathlib import Path


DISPLAY_REF = re.compile(r"^(?P<book>.+?) (?P<chapter>[1-9][0-9]*):(?P<verse>[1-9][0-9]*)$")
OSIS_REF = re.compile(r"^(?P<book>[1-3]?[A-Za-z]+)\.(?P<chapter>[1-9][0-9]*)\.(?P<verse>[1-9][0-9]*)$")
TRUSTED_CONCEPT_TYPES = {"event", "event_shadow", "person", "place"}
MAX_PASSAGE_VERSES = 12
MAX_LABELS_PER_VERSE = 8
MAX_STUDY_NOTE_VERSES = 64
MAX_DENSE_STUDY_NOTES_PER_VERSE = 2
MAX_STUDY_NOTE_CHARS = 900
AQUIFER_REFERENCE = re.compile(r"^(?P<book>[0-9]{2})(?P<chapter>[0-9]{3})(?P<verse>[0-9]{3})$")

OSIS_BOOK_KEYS = {
    "Gen": "genesis", "Exod": "exodus", "Lev": "leviticus", "Num": "numbers",
    "Deut": "deuteronomy", "Josh": "joshua", "Judg": "judges", "Ruth": "ruth",
    "1Sam": "1samuel", "2Sam": "2samuel", "1Kgs": "1kings", "2Kgs": "2kings",
    "1Chr": "1chronicles", "2Chr": "2chronicles", "Ezra": "ezra", "Neh": "nehemiah",
    "Esth": "esther", "Job": "job", "Ps": "psalm", "Prov": "proverbs",
    "Eccl": "ecclesiastes", "Song": "songofsolomon", "Isa": "isaiah", "Jer": "jeremiah",
    "Lam": "lamentations", "Ezek": "ezekiel", "Dan": "daniel", "Hos": "hosea",
    "Joel": "joel", "Amos": "amos", "Obad": "obadiah", "Jonah": "jonah",
    "Mic": "micah", "Nah": "nahum", "Hab": "habakkuk", "Zeph": "zephaniah",
    "Hag": "haggai", "Zech": "zechariah", "Mal": "malachi", "Matt": "matthew",
    "Mark": "mark", "Luke": "luke", "John": "john", "Acts": "acts", "Rom": "romans",
    "1Cor": "1corinthians", "2Cor": "2corinthians", "Gal": "galatians", "Eph": "ephesians",
    "Phil": "philippians", "Col": "colossians", "1Thess": "1thessalonians",
    "2Thess": "2thessalonians", "1Tim": "1timothy", "2Tim": "2timothy",
    "Titus": "titus", "Phlm": "philemon", "Heb": "hebrews", "Jas": "james",
    "1Pet": "1peter", "2Pet": "2peter", "1John": "1john", "2John": "2john",
    "3John": "3john", "Jude": "jude", "Rev": "revelation",
}


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


def book_key(value: str) -> str:
    value = value.lower().replace("psalms", "psalm")
    return "".join(character for character in value if character.isalnum())


def display_coordinate(reference: str) -> tuple[str, int, int]:
    match = DISPLAY_REF.fullmatch(reference.strip())
    if match is None:
        raise ValueError(f"invalid canonical display reference {reference!r}")
    return (
        book_key(match.group("book")),
        int(match.group("chapter")),
        int(match.group("verse")),
    )


def osis_coordinate(reference: str) -> tuple[str, int, int] | None:
    match = OSIS_REF.fullmatch(reference.strip())
    if match is None:
        return None
    key = OSIS_BOOK_KEYS.get(match.group("book"))
    if key is None:
        return None
    return key, int(match.group("chapter")), int(match.group("verse"))


def load_passage_contexts(
    database: Path,
    verses: list[dict],
    by_coordinate: dict[tuple[str, int, int], int],
) -> dict[int, str]:
    selected: dict[int, tuple[int, str]] = {}
    uri = f"file:{database.resolve()}?mode=ro"
    with sqlite3.connect(uri, uri=True) as connection:
        tables = {
            row[0]
            for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")
        }
        if "passages" not in tables:
            raise ValueError(f"{database}: missing passages table")
        rows = connection.execute(
            "SELECT reference_start, reference_end, verse_count FROM passages ORDER BY id"
        )
        for raw_start, raw_end, raw_count in rows:
            start = osis_coordinate(raw_start)
            end = osis_coordinate(raw_end)
            count = int(raw_count)
            if (
                start is None
                or end is None
                or start[0] != end[0]
                or not 1 <= count <= MAX_PASSAGE_VERSES
                or start not in by_coordinate
                or end not in by_coordinate
            ):
                continue
            start_index, end_index = by_coordinate[start], by_coordinate[end]
            if start_index > end_index or end_index - start_index + 1 != count:
                continue
            passage = " ".join(verses[index]["text"] for index in range(start_index, end_index + 1))
            for index in range(start_index, end_index + 1):
                previous = selected.get(index)
                if previous is None or count < previous[0]:
                    selected[index] = (count, passage)
    return {index: passage for index, (_, passage) in selected.items()}


def load_study_notes(
    database: Path,
    coordinates: list[tuple[str, int, int]],
    by_coordinate: dict[tuple[str, int, int], int],
) -> dict[int, list[str]]:
    """Map independently authored Aquifer study prose onto canonical rows.

    Numeric Aquifer references encode the canonical 66-book order. We derive
    that order from the canonical input instead of maintaining a passage list
    in code. Specific notes are preferred to broad introductions; ranges over
    64 verses are anchored only at their start to avoid smearing one article
    across a whole book.
    """

    ordered_books = []
    for book, _, _ in coordinates:
        if not ordered_books or ordered_books[-1] != book:
            ordered_books.append(book)

    def coordinate(raw: str) -> tuple[str, int, int] | None:
        match = AQUIFER_REFERENCE.fullmatch(raw or "")
        if match is None:
            return None
        book_number = int(match.group("book"))
        chapter = int(match.group("chapter"))
        verse = int(match.group("verse"))
        if not 1 <= book_number <= len(ordered_books) or chapter <= 0 or verse <= 0:
            return None
        return ordered_books[book_number - 1], chapter, verse

    selected: dict[int, list[tuple[int, int, str]]] = defaultdict(list)
    uri = f"file:{database.resolve()}?mode=ro"
    with sqlite3.connect(uri, uri=True) as connection:
        tables = {
            row[0]
            for row in connection.execute("SELECT name FROM sqlite_master WHERE type='table'")
        }
        if "aquifer_content" not in tables:
            raise ValueError(f"{database}: missing aquifer_content table")
        rows = connection.execute(
            """
            SELECT id, title, start_ref, end_ref, content_plain
            FROM aquifer_content
            WHERE resource_type = 'study_notes'
            ORDER BY id
            """
        )
        for note_id, title, raw_start, raw_end, content in rows:
            start = coordinate(raw_start)
            end = coordinate(raw_end or raw_start)
            if (
                start is None
                or end is None
                or start[0] != end[0]
                or start not in by_coordinate
                or end not in by_coordinate
            ):
                continue
            start_index, end_index = by_coordinate[start], by_coordinate[end]
            if start_index > end_index:
                continue
            span = end_index - start_index + 1
            clean_title = re.sub(r"\s+", " ", title or "").strip()
            clean_content = re.sub(r"\s+", " ", content or "").strip()
            if not clean_content:
                continue
            note = " — ".join(value for value in (clean_title, clean_content) if value)
            note = note[:MAX_STUDY_NOTE_CHARS].rstrip()
            targets = (
                range(start_index, end_index + 1)
                if span <= MAX_STUDY_NOTE_VERSES
                else (start_index,)
            )
            for index in targets:
                selected[index].append((span, int(note_id), note))

    output = {}
    for index, notes in selected.items():
        unique = []
        seen = set()
        for _, _, note in sorted(notes, key=lambda value: (value[0], value[1])):
            key = re.sub(r"\s+", " ", note).strip().casefold()
            if not key or key in seen:
                continue
            seen.add(key)
            unique.append(note)
        if unique:
            output[index] = unique
    return output


def fallback_context(index: int, verses: list[dict], coordinates: list[tuple[str, int, int]]) -> str:
    book, chapter, _ = coordinates[index]
    start = index
    while start > 0 and start > index - 2 and coordinates[start - 1][:2] == (book, chapter):
        start -= 1
    end = index
    while end + 1 < len(verses) and end < index + 2 and coordinates[end + 1][:2] == (book, chapter):
        end += 1
    return " ".join(verses[position]["text"] for position in range(start, end + 1))


def load_overlay_evidence(path: Path) -> tuple[dict[str, str], dict[str, list[str]]]:
    alternate = {}
    labels: dict[str, set[str]] = defaultdict(set)
    for row in jsonl(path):
        kind = row.get("kind")
        if kind == "alternate_text" and row.get("source_translation") == "KJV":
            target, text = row.get("target_ref"), row.get("text")
            if isinstance(target, str) and isinstance(text, str) and text.strip():
                alternate[target] = text.strip()
        elif kind == "concept" and row.get("concept_type") in TRUSTED_CONCEPT_TYPES:
            anchor = row.get("anchor_ref")
            if not isinstance(anchor, str):
                continue
            values = [row.get("label"), *(row.get("aliases") or [])]
            labels[anchor].update(
                value.strip()
                for value in values
                if isinstance(value, str) and value.strip()
            )
    return alternate, {
        reference: sorted(values, key=lambda value: (value.lower(), value))[:MAX_LABELS_PER_VERSE]
        for reference, values in labels.items()
    }


def build(
    canonical: Path,
    database: Path,
    overlay: Path,
    include_study_notes: bool = False,
    document_view: str = "enriched",
) -> list[dict]:
    if document_view not in {"enriched", "canonical-passage"}:
        raise ValueError("document_view must be enriched or canonical-passage")
    verses = jsonl(canonical)
    if not verses:
        raise ValueError("canonical input is empty")
    coordinates = []
    by_coordinate = {}
    seen_refs = set()
    for index, row in enumerate(verses, 1):
        reference, text = row.get("ref"), row.get("text")
        if not isinstance(reference, str) or not isinstance(text, str) or not text.strip():
            raise ValueError(f"canonical row {index} requires non-empty ref and text")
        if reference in seen_refs:
            raise ValueError(f"duplicate canonical reference {reference}")
        seen_refs.add(reference)
        coordinate = display_coordinate(reference)
        if coordinate in by_coordinate:
            raise ValueError(f"duplicate canonical coordinate {coordinate}")
        by_coordinate[coordinate] = index - 1
        coordinates.append(coordinate)

    paragraphs = load_passage_contexts(database, verses, by_coordinate)
    study_notes = (
        load_study_notes(database, coordinates, by_coordinate)
        if include_study_notes
        else {}
    )
    alternate, labels = load_overlay_evidence(overlay)
    documents = []
    for index, verse in enumerate(verses):
        reference = verse["ref"]
        parts = [verse["text"].strip()]
        all_study_notes = study_notes.get(index, [])
        if document_view == "enriched":
            if reference in alternate and alternate[reference] != parts[0]:
                parts.append(f"Alternate wording: {alternate[reference]}")
            if labels.get(reference):
                parts.append("Named associations: " + "; ".join(labels[reference]))
            if all_study_notes:
                parts.append(
                    "Study explanation: "
                    + " ".join(all_study_notes[:MAX_DENSE_STUDY_NOTES_PER_VERSE])
                )
        context = paragraphs.get(index) or fallback_context(index, verses, coordinates)
        if context != parts[0]:
            parts.append("Passage context: " + context)
        # Keep the full, independently authored Study Note view separate from
        # the dense document. CLEX/CPOST can index this field without copying
        # canonical, alternate-translation, graph-label, or passage wording
        # into a second sparse retrieval vote. Empty values preserve complete
        # one-row-per-canonical-verse alignment for uncovered coordinates.
        documents.append(
            {
                "ref": reference,
                "text": " ".join(parts),
                "study_context": " ".join(all_study_notes),
            }
        )
    return documents


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--study-bible", type=Path, required=True)
    parser.add_argument("--overlay-jsonl", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument(
        "--include-study-notes",
        action="store_true",
        help="include CC BY-SA 4.0 Aquifer Open Study Notes in semantic documents",
    )
    parser.add_argument(
        "--document-view",
        choices=("enriched", "canonical-passage"),
        default="enriched",
        help="select the deterministic corpus representation",
    )
    args = parser.parse_args()
    documents = build(
        args.canonical,
        args.study_bible,
        args.overlay_jsonl,
        include_study_notes=args.include_study_notes,
        document_view=args.document_view,
    )
    rendered = "".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) + "\n" for row in documents)
    args.output.write_text(rendered, encoding="utf-8")
    print(f"wrote {len(documents)} semantic documents -> {args.output}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build range-bound semantic documents from the complete Study Bible source.

This builder deliberately emits one typed document per source assertion and
canonical range.  It does not concatenate every note associated with a verse
into one opaque document, and it never reads evaluation data.  The resulting
JSONL can therefore be encoded as a separately calibrated multi-vector source
lane while retaining enough provenance for source-level ablations.

Only query-independent source fields are used:

* HLT Study Note titles and bounded content;
* plain HLT verse wording (never the annotated-markup field);
* theology theme labels/descriptions mapped through the theme and verse indexes;
* theology content titles/summaries mapped through the verse index;
* thematic-reference labels and notes;
* Theographic person/place/event descriptions and graph associations.

Every document is grounded to an explicit canonical BSB range.  Malformed,
unknown, cross-book, reversed, and overlong references fail closed and are
counted in the manifest.
"""

from __future__ import annotations

import argparse
from collections import defaultdict
from dataclasses import dataclass
import hashlib
import html
import json
from pathlib import Path
import re
import sqlite3
from typing import Any, Iterable

try:
    from scripts.build_passage_documents import STUDY_BOOK_CODES
    from scripts.build_semantic_documents import book_key, display_coordinate, jsonl
except ModuleNotFoundError:  # Direct ``python scripts/...`` execution.
    from build_passage_documents import STUDY_BOOK_CODES
    from build_semantic_documents import book_key, display_coordinate, jsonl


SCHEMA_VERSION = 1
SOURCE_URL = "https://github.com/djayatillake/studybible-mcp"
THEOGRAPHIC_URL = "https://github.com/robertrouse/theographic-bible-metadata"
DEFAULT_MAX_RANGE_VERSES = 30
DEFAULT_MAX_SOURCE_CHARS = 1_200
DEFAULT_MAX_CANONICAL_CHARS = 2_400
DEFAULT_MAX_ASSOCIATIONS = 16

# Theographic references use conventional OSIS abbreviations, while the HLT
# and theology tables use the compact Study Bible codes in STUDY_BOOK_CODES.
# Both sequences are mapped by canonical book order, never by verse row order.
THEOGRAPHIC_BOOK_CODES = (
    "Gen", "Exod", "Lev", "Num", "Deut", "Josh", "Judg", "Ruth",
    "1Sam", "2Sam", "1Kgs", "2Kgs", "1Chr", "2Chr", "Ezra", "Neh",
    "Esth", "Job", "Ps", "Prov", "Eccl", "Song", "Isa", "Jer", "Lam",
    "Ezek", "Dan", "Hos", "Joel", "Amos", "Obad", "Jonah", "Mic",
    "Nah", "Hab", "Zeph", "Hag", "Zech", "Mal", "Matt", "Mark",
    "Luke", "John", "Acts", "Rom", "1Cor", "2Cor", "Gal", "Eph",
    "Phil", "Col", "1Thess", "2Thess", "1Tim", "2Tim", "Titus",
    "Phlm", "Heb", "Jas", "1Pet", "2Pet", "1John", "2John", "3John",
    "Jude", "Rev",
)
if len(THEOGRAPHIC_BOOK_CODES) != len(STUDY_BOOK_CODES):
    raise AssertionError("canonical book-code registries must have equal length")
# Additional abbreviations observed in the theology tables. Values name the
# corresponding code in the 66-book Study registry; this is source-schema
# normalization, not query expansion.
ADDITIONAL_STUDY_BOOK_ALIASES = {"Eze": "Ezk", "Joe": "Jol"}

OSIS_RANGE = re.compile(
    r"^(?P<book>[1-3]?[A-Za-z]+)\.(?P<chapter>[1-9][0-9]*)\."
    r"(?P<verse>[1-9][0-9]*)"
    r"(?:-(?:(?P<end_book>[1-3]?[A-Za-z]+)\.)?"
    r"(?:(?P<end_chapter>[1-9][0-9]*)\.)?"
    r"(?P<end_verse>[1-9][0-9]*))?"
    r"(?P<versification>\([1-9][0-9]*\.[1-9][0-9]*\)|"
    r"\[[1-9][0-9]*\.[1-9][0-9]*\]|"
    r"\{[1-9][0-9]*\.[1-9][0-9]*\})?$"
)
DISPLAY_RANGE = re.compile(
    r"^(?P<book>.+?)\s+(?P<chapter>[1-9][0-9]*):(?P<verse>[1-9][0-9]*)"
    r"(?:-(?:(?P<end_chapter>[1-9][0-9]*):)?"
    r"(?P<end_verse>[1-9][0-9]*))?$"
)
MARKDOWN_LINK = re.compile(r"\[([^\]]+)\]\([^)]*\)")
HTML_TAG = re.compile(r"<[^>]+>")
WHITESPACE = re.compile(r"\s+")

REQUIRED_TABLES = {
    "graph_event_place_edges",
    "graph_events",
    "graph_people",
    "graph_person_event_edges",
    "graph_places",
    "graph_verse_mentions",
    "hlt_study_notes",
    "hlt_verses",
    "thematic_references",
    "theology_content",
    "theology_theme_index",
    "theology_themes",
    "theology_verse_index",
}


@dataclass(frozen=True)
class CanonicalCorpus:
    rows: tuple[dict[str, Any], ...]
    by_coordinate: dict[tuple[str, int, int], int]
    book_aliases: dict[str, str]


@dataclass(frozen=True)
class Binding:
    start: int
    end: int


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def clean_prose(value: Any) -> str:
    """Normalize source prose without inventing or expanding its meaning."""

    if not isinstance(value, str):
        return ""
    value = MARKDOWN_LINK.sub(lambda match: match.group(1), value)
    value = HTML_TAG.sub(" ", value)
    value = html.unescape(value)
    value = value.replace("**", "").replace("__", "").replace("`", "")
    return WHITESPACE.sub(" ", value).strip()


def bound_text(value: str, max_chars: int) -> tuple[str, bool]:
    value = clean_prose(value)
    if len(value) <= max_chars:
        return value, False
    boundary = value.rfind(" ", 0, max_chars - 1)
    if boundary < max_chars // 2:
        boundary = max_chars - 1
    return value[:boundary].rstrip(" ,;:-") + "…", True


def compose_text(parts: Iterable[str], max_chars: int) -> tuple[str, bool]:
    cleaned = [clean_prose(part).rstrip(". ") for part in parts]
    return bound_text(". ".join(part for part in cleaned if part), max_chars)


def load_canonical(path: Path) -> CanonicalCorpus:
    rows = jsonl(path)
    if not rows:
        raise ValueError("canonical corpus is empty")
    by_coordinate: dict[tuple[str, int, int], int] = {}
    ordered_books: list[str] = []
    seen_refs: set[str] = set()
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
        raise ValueError("canonical corpus has more books than the 66-book registry")

    aliases: dict[str, str] = {}
    for position, canonical_book in enumerate(ordered_books):
        source_aliases = {
            STUDY_BOOK_CODES[position],
            THEOGRAPHIC_BOOK_CODES[position],
            canonical_book,
        }
        for alias in source_aliases:
            key = alias.casefold()
            previous = aliases.get(key)
            if previous is not None and previous != canonical_book:
                raise ValueError(f"ambiguous canonical book alias {alias!r}")
            aliases[key] = canonical_book
    study_positions = {code: position for position, code in enumerate(STUDY_BOOK_CODES)}
    for alias, study_code in ADDITIONAL_STUDY_BOOK_ALIASES.items():
        position = study_positions[study_code]
        if position < len(ordered_books):
            aliases[alias.casefold()] = ordered_books[position]
    return CanonicalCorpus(tuple(rows), by_coordinate, aliases)


def _coordinate(
    corpus: CanonicalCorpus,
    raw_book: str,
    chapter: int,
    verse: int,
) -> tuple[str, int, int] | None:
    canonical_book = corpus.book_aliases.get(raw_book.casefold())
    if canonical_book is None:
        # Display book names normalize to the same key as the canonical input.
        normalized = book_key(raw_book)
        canonical_book = next(
            (value for value in corpus.book_aliases.values() if value == normalized),
            None,
        )
    if canonical_book is None:
        return None
    return canonical_book, chapter, verse


def resolve_reference(
    raw_reference: Any,
    corpus: CanonicalCorpus,
    max_range_verses: int,
) -> tuple[Binding | None, str | None]:
    if not isinstance(raw_reference, str) or not raw_reference.strip():
        return None, "malformed_reference"
    raw = raw_reference.strip()
    osis = OSIS_RANGE.fullmatch(raw)
    if osis is not None:
        start_book = osis.group("book")
        end_book = osis.group("end_book") or start_book
        start_chapter = int(osis.group("chapter"))
        end_chapter = int(osis.group("end_chapter") or start_chapter)
        start_verse = int(osis.group("verse"))
        end_verse = int(osis.group("end_verse") or start_verse)
    else:
        display = DISPLAY_RANGE.fullmatch(raw)
        if display is None:
            return None, "malformed_reference"
        start_book = end_book = display.group("book")
        start_chapter = int(display.group("chapter"))
        end_chapter = int(display.group("end_chapter") or start_chapter)
        start_verse = int(display.group("verse"))
        end_verse = int(display.group("end_verse") or start_verse)

    start_coordinate = _coordinate(corpus, start_book, start_chapter, start_verse)
    end_coordinate = _coordinate(corpus, end_book, end_chapter, end_verse)
    if start_coordinate is None or end_coordinate is None:
        return None, "unknown_book"
    if start_coordinate[0] != end_coordinate[0]:
        return None, "cross_book"
    if start_coordinate not in corpus.by_coordinate or end_coordinate not in corpus.by_coordinate:
        return None, "noncanonical_endpoint"
    start = corpus.by_coordinate[start_coordinate]
    end = corpus.by_coordinate[end_coordinate]
    if start > end:
        return None, "invalid_span"
    span = end - start + 1
    if span > max_range_verses:
        return None, "over_max_verses"
    return Binding(start, end), None


def _range_fields(
    corpus: CanonicalCorpus,
    binding: Binding,
    max_canonical_chars: int,
) -> dict[str, Any]:
    start_ref = corpus.rows[binding.start]["ref"]
    end_ref = corpus.rows[binding.end]["ref"]
    canonical, truncated = bound_text(
        " ".join(
            corpus.rows[index]["text"].strip()
            for index in range(binding.start, binding.end + 1)
        ),
        max_canonical_chars,
    )
    return {
        "anchor_ref": start_ref,
        "start_ref": start_ref,
        "end_ref": end_ref,
        "anchor_index": binding.start,
        "start_index": binding.start,
        "end_index": binding.end,
        "verse_count": binding.end - binding.start + 1,
        "canonical_text": canonical,
        "canonical_text_truncated": truncated,
    }


def _document(
    *,
    document_id: str,
    source_kind: str,
    source_tables: Iterable[str],
    source_record_ids: Iterable[str],
    text: str,
    text_truncated: bool,
    corpus: CanonicalCorpus,
    binding: Binding,
    max_canonical_chars: int,
    metadata: dict[str, Any] | None = None,
) -> dict[str, Any]:
    row = {
        "schema_version": SCHEMA_VERSION,
        "id": document_id,
        "source_kind": source_kind,
        "source_tables": sorted(set(source_tables)),
        "source_record_ids": sorted(set(source_record_ids)),
        "text": text,
        "text_truncated": text_truncated,
        **_range_fields(corpus, binding, max_canonical_chars),
    }
    if metadata:
        row["metadata"] = metadata
    return row


def _empty_exclusions() -> dict[str, int]:
    return {
        "malformed_reference": 0,
        "unknown_book": 0,
        "noncanonical_endpoint": 0,
        "cross_book": 0,
        "invalid_span": 0,
        "over_max_verses": 0,
        "empty_text": 0,
    }


def _read_only_connection(database: Path) -> sqlite3.Connection:
    connection = sqlite3.connect(f"{database.resolve().as_uri()}?mode=ro", uri=True)
    connection.execute("PRAGMA query_only=ON")
    # sqlite3 cannot parse sqlite-vec declarations when the extension is not
    # loaded. This pragma changes schema parsing only; mode=ro remains binding.
    connection.execute("PRAGMA writable_schema=ON")
    tables = {
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type IN ('table','view')"
        )
    }
    missing = sorted(REQUIRED_TABLES - tables)
    if missing:
        connection.close()
        raise ValueError(f"{database}: missing required tables: {', '.join(missing)}")
    return connection


def _build_hlt_documents(
    connection: sqlite3.Connection,
    corpus: CanonicalCorpus,
    *,
    max_range_verses: int,
    max_source_chars: int,
    max_canonical_chars: int,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    rows = list(
        connection.execute(
            "SELECT id, reference, note_type, title, content, priority "
            "FROM hlt_study_notes ORDER BY id"
        )
    )
    exclusions = _empty_exclusions()
    documents = []
    covered_refs: set[int] = set()
    truncated = 0
    for note_id, reference, note_type, title, content, priority in rows:
        binding, reason = resolve_reference(reference, corpus, max_range_verses)
        if binding is None:
            exclusions[reason or "malformed_reference"] += 1
            continue
        text, was_truncated = compose_text(
            (f"Study note: {title or ''}", content or ""), max_source_chars
        )
        if not text:
            exclusions["empty_text"] += 1
            continue
        truncated += int(was_truncated)
        covered_refs.update(range(binding.start, binding.end + 1))
        documents.append(
            _document(
                document_id=f"study-bible.hlt-study-note.{int(note_id)}",
                source_kind="hlt-study-note",
                source_tables=("hlt_study_notes",),
                source_record_ids=(f"hlt_study_notes:{int(note_id)}",),
                text=text,
                text_truncated=was_truncated,
                corpus=corpus,
                binding=binding,
                max_canonical_chars=max_canonical_chars,
                metadata={
                    "source_reference": clean_prose(reference),
                    "note_type": clean_prose(note_type) or "unspecified",
                    "priority": int(priority) if priority is not None else None,
                },
            )
        )
    excluded = sum(exclusions.values())
    if len(documents) + excluded != len(rows):
        raise AssertionError("HLT Study Note accounting is incomplete")
    return documents, {
        "input_row_count": len(rows),
        "document_count_before_dedup": len(documents),
        "covered_canonical_verse_count": len(covered_refs),
        "truncated_text_count": truncated,
        "exclusions": exclusions,
    }


def _build_hlt_verse_documents(
    connection: sqlite3.Connection,
    corpus: CanonicalCorpus,
    *,
    max_range_verses: int,
    max_source_chars: int,
    max_canonical_chars: int,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """Build a separately ablatable alternate-wording row for every HLT verse.

    ``hlt_text_annotated``, ``word_data_hlt``, and translation-note markup are
    intentionally not selected.  Status and confidence are retained as source
    metadata rather than treated as a ranking instruction.
    """

    rows = list(
        connection.execute(
            "SELECT id, reference, hlt_text, base_text_source, variant_used, "
            "confidence, status FROM hlt_verses ORDER BY id"
        )
    )
    exclusions = _empty_exclusions()
    candidates: dict[tuple[int, int], list[dict[str, Any]]] = defaultdict(list)
    statuses: dict[str, int] = defaultdict(int)
    confidences: dict[str, int] = defaultdict(int)
    base_sources: dict[str, int] = defaultdict(int)
    variant_used_count = 0
    for (
        verse_id,
        reference,
        hlt_text,
        base_text_source,
        variant_used,
        confidence,
        status,
    ) in rows:
        binding, reason = resolve_reference(reference, corpus, max_range_verses)
        if binding is None:
            exclusions[reason or "malformed_reference"] += 1
            continue
        text = clean_prose(hlt_text)
        if not text:
            exclusions["empty_text"] += 1
            continue
        clean_status = clean_prose(status) or "unspecified"
        clean_confidence = clean_prose(confidence) or "unspecified"
        clean_base = clean_prose(base_text_source) or "unspecified"
        statuses[clean_status] += 1
        confidences[clean_confidence] += 1
        base_sources[clean_base] += 1
        variant_used_count += int(variant_used is not None)
        match = OSIS_RANGE.fullmatch(reference.strip()) if isinstance(reference, str) else None
        candidates[(binding.start, binding.end)].append(
            {
                "verse_id": int(verse_id),
                "reference": clean_prose(reference),
                "versification": match.group("versification") if match else None,
                "text": text,
                "status": clean_status,
                "confidence": clean_confidence,
                "base_text_source": clean_base,
                "variant_used": variant_used,
            }
        )

    documents = []
    truncated = 0
    coalesced_fragment_count = 0
    for (start, end), fragments in sorted(candidates.items()):
        fragments.sort(key=lambda row: row["verse_id"])
        text, was_truncated = bound_text(
            " ".join(fragment["text"] for fragment in fragments),
            max_source_chars,
        )
        truncated += int(was_truncated)
        coalesced_fragment_count += len(fragments) - 1
        documents.append(
            _document(
                document_id=f"study-bible.hlt-verse.{start}.{end}",
                source_kind="hlt-verse",
                source_tables=("hlt_verses",),
                source_record_ids=(
                    f"hlt_verses:{fragment['verse_id']}" for fragment in fragments
                ),
                text=text,
                text_truncated=was_truncated,
                corpus=corpus,
                binding=Binding(start, end),
                max_canonical_chars=max_canonical_chars,
                metadata={
                    "source_references": [fragment["reference"] for fragment in fragments],
                    "versification_annotations": [
                        fragment["versification"]
                        for fragment in fragments
                        if fragment["versification"] is not None
                    ],
                    "statuses": sorted({fragment["status"] for fragment in fragments}),
                    "confidences": sorted(
                        {fragment["confidence"] for fragment in fragments}
                    ),
                    "base_text_sources": sorted(
                        {fragment["base_text_source"] for fragment in fragments}
                    ),
                    "variant_used": [
                        fragment["variant_used"]
                        for fragment in fragments
                        if fragment["variant_used"] is not None
                    ],
                    "source_fragment_count": len(fragments),
                    "annotated_markup_included": False,
                },
            )
        )
    excluded = sum(exclusions.values())
    accepted_rows = sum(len(fragments) for fragments in candidates.values())
    if accepted_rows + excluded != len(rows):
        raise AssertionError("HLT verse accounting is incomplete")
    return documents, {
        "input_row_count": len(rows),
        "accepted_source_row_count": accepted_rows,
        "document_count_before_dedup": len(documents),
        "covered_canonical_verse_count": len(candidates),
        "coalesced_fragment_count": coalesced_fragment_count,
        "truncated_text_count": truncated,
        "variant_used_count": variant_used_count,
        "status_counts": dict(sorted(statuses.items())),
        "confidence_counts": dict(sorted(confidences.items())),
        "base_text_source_counts": dict(sorted(base_sources.items())),
        "annotated_markup_included": False,
        "exclusions": exclusions,
    }


def _build_theology_documents(
    connection: sqlite3.Connection,
    corpus: CanonicalCorpus,
    *,
    max_range_verses: int,
    max_source_chars: int,
    max_canonical_chars: int,
) -> tuple[list[dict[str, Any]], dict[str, Any], dict[str, Any]]:
    themes = {
        key: (int(theme_id), label, description, parent)
        for theme_id, key, label, description, parent in connection.execute(
            "SELECT id, theme_key, theme_label, description, parent_theme "
            "FROM theology_themes ORDER BY theme_key"
        )
    }
    contents = {
        int(content_id): (
            source_work,
            source_author,
            source_type,
            chapter,
            title,
            summary,
        )
        for content_id, source_work, source_author, source_type, chapter, title, summary in connection.execute(
            "SELECT id, source_work, source_author, source_type, "
            "chapter_or_episode, title, content_summary "
            "FROM theology_content ORDER BY id"
        )
    }
    index_rows = list(
        connection.execute(
            "SELECT id, theme_key, content_id, reference "
            "FROM theology_theme_index ORDER BY id"
        )
    )
    verse_rows = list(
        connection.execute(
            "SELECT id, content_id, reference, relevance "
            "FROM theology_verse_index ORDER BY id"
        )
    )
    verses_by_content: dict[int, list[tuple[int, str, Any]]] = defaultdict(list)
    for verse_index_id, content_id, reference, relevance in verse_rows:
        verses_by_content[int(content_id)].append(
            (int(verse_index_id), reference, relevance)
        )

    theme_exclusions = _empty_exclusions()
    theme_exclusions.update({"unknown_theme": 0, "unknown_content": 0})
    theme_candidates: dict[tuple[str, int, int], dict[str, set[Any]]] = {}
    candidate_binding_count = 0
    invalid_candidate_binding_count = 0
    for index_id, theme_key, raw_content_id, direct_reference in index_rows:
        if theme_key not in themes:
            theme_exclusions["unknown_theme"] += 1
            continue
        if raw_content_id is None or int(raw_content_id) not in contents:
            theme_exclusions["unknown_content"] += 1
            continue
        content_id = int(raw_content_id)
        references: list[tuple[str, str, int | None]] = []
        if isinstance(direct_reference, str) and direct_reference.strip():
            references.append((direct_reference, "direct", None))
        references.extend(
            (reference, clean_prose(relevance) or "unspecified", verse_index_id)
            for verse_index_id, reference, relevance in verses_by_content.get(content_id, [])
        )
        for reference, relevance, verse_index_id in references:
            candidate_binding_count += 1
            binding, reason = resolve_reference(reference, corpus, max_range_verses)
            if binding is None:
                theme_exclusions[reason or "malformed_reference"] += 1
                invalid_candidate_binding_count += 1
                continue
            key = (theme_key, binding.start, binding.end)
            candidate = theme_candidates.setdefault(
                key,
                {"theme_index_ids": set(), "content_ids": set(), "verse_index_ids": set(), "relevances": set()},
            )
            candidate["theme_index_ids"].add(int(index_id))
            candidate["content_ids"].add(content_id)
            if verse_index_id is not None:
                candidate["verse_index_ids"].add(int(verse_index_id))
            candidate["relevances"].add(relevance)

    theme_documents = []
    theme_truncated = 0
    represented_themes: set[str] = set()
    for (theme_key, start, end), provenance in sorted(theme_candidates.items()):
        theme_id, label, description, parent = themes[theme_key]
        text, was_truncated = compose_text(
            (f"Theology theme: {label or theme_key}", description or ""),
            max_source_chars,
        )
        if not text:
            theme_exclusions["empty_text"] += 1
            continue
        theme_truncated += int(was_truncated)
        represented_themes.add(theme_key)
        source_ids = [f"theology_themes:{theme_id}"]
        source_ids.extend(
            f"theology_theme_index:{value}"
            for value in sorted(provenance["theme_index_ids"])
        )
        source_ids.extend(
            f"theology_verse_index:{value}"
            for value in sorted(provenance["verse_index_ids"])
        )
        theme_documents.append(
            _document(
                document_id=f"study-bible.theology-theme.{theme_key}.{start}.{end}",
                source_kind="theology-theme",
                source_tables=(
                    "theology_themes",
                    "theology_theme_index",
                    "theology_verse_index",
                ),
                source_record_ids=source_ids,
                text=text,
                text_truncated=was_truncated,
                corpus=corpus,
                binding=Binding(start, end),
                max_canonical_chars=max_canonical_chars,
                metadata={
                    "theme_key": theme_key,
                    "parent_theme": clean_prose(parent) or None,
                    "content_ids": sorted(provenance["content_ids"]),
                    "relevances": sorted(provenance["relevances"]),
                },
            )
        )

    content_exclusions = _empty_exclusions()
    content_exclusions["unknown_content"] = 0
    content_documents = []
    content_truncated = 0
    represented_content: set[int] = set()
    for verse_index_id, raw_content_id, reference, relevance in verse_rows:
        content_id = int(raw_content_id)
        content = contents.get(content_id)
        if content is None:
            content_exclusions["unknown_content"] += 1
            continue
        binding, reason = resolve_reference(reference, corpus, max_range_verses)
        if binding is None:
            content_exclusions[reason or "malformed_reference"] += 1
            continue
        source_work, source_author, source_type, chapter, title, summary = content
        text, was_truncated = compose_text(
            (f"Theology source: {title or chapter or source_work}", summary or ""),
            max_source_chars,
        )
        if not text:
            content_exclusions["empty_text"] += 1
            continue
        content_truncated += int(was_truncated)
        represented_content.add(content_id)
        content_documents.append(
            _document(
                document_id=f"study-bible.theology-content.{content_id}.{int(verse_index_id)}",
                source_kind="theology-content",
                source_tables=("theology_content", "theology_verse_index"),
                source_record_ids=(
                    f"theology_content:{content_id}",
                    f"theology_verse_index:{int(verse_index_id)}",
                ),
                text=text,
                text_truncated=was_truncated,
                corpus=corpus,
                binding=binding,
                max_canonical_chars=max_canonical_chars,
                metadata={
                    "source_reference": clean_prose(reference),
                    "source_work": clean_prose(source_work),
                    "source_author": clean_prose(source_author),
                    "source_type": clean_prose(source_type),
                    "chapter_or_episode": clean_prose(chapter) or None,
                    "relevance": clean_prose(relevance) or "unspecified",
                },
            )
        )
    if len(content_documents) + sum(content_exclusions.values()) != len(verse_rows):
        raise AssertionError("theology content accounting is incomplete")

    theme_stats = {
        "input_theme_count": len(themes),
        "input_theme_index_row_count": len(index_rows),
        "candidate_binding_count": candidate_binding_count,
        "unique_binding_count": len(theme_candidates),
        "duplicate_binding_count": candidate_binding_count
        - invalid_candidate_binding_count
        - len(theme_candidates),
        "document_count_before_dedup": len(theme_documents),
        "represented_theme_count": len(represented_themes),
        "theme_without_document_count": len(themes) - len(represented_themes),
        "truncated_text_count": theme_truncated,
        "exclusions": theme_exclusions,
    }
    content_stats = {
        "input_content_count": len(contents),
        "input_verse_index_row_count": len(verse_rows),
        "document_count_before_dedup": len(content_documents),
        "represented_content_count": len(represented_content),
        "content_without_document_count": len(contents) - len(represented_content),
        "truncated_text_count": content_truncated,
        "exclusions": content_exclusions,
    }
    return theme_documents + content_documents, theme_stats, content_stats


def _build_thematic_reference_documents(
    connection: sqlite3.Connection,
    corpus: CanonicalCorpus,
    *,
    max_range_verses: int,
    max_source_chars: int,
    max_canonical_chars: int,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    rows = list(
        connection.execute(
            "SELECT id, theme, reference, note FROM thematic_references ORDER BY id"
        )
    )
    exclusions = _empty_exclusions()
    documents = []
    truncated = 0
    for record_id, theme, reference, note in rows:
        binding, reason = resolve_reference(reference, corpus, max_range_verses)
        if binding is None:
            exclusions[reason or "malformed_reference"] += 1
            continue
        text, was_truncated = compose_text(
            (f"Thematic reference: {theme or ''}", note or ""),
            max_source_chars,
        )
        if not text:
            exclusions["empty_text"] += 1
            continue
        truncated += int(was_truncated)
        documents.append(
            _document(
                document_id=f"study-bible.thematic-reference.{int(record_id)}",
                source_kind="thematic-reference",
                source_tables=("thematic_references",),
                source_record_ids=(f"thematic_references:{int(record_id)}",),
                text=text,
                text_truncated=was_truncated,
                corpus=corpus,
                binding=binding,
                max_canonical_chars=max_canonical_chars,
                metadata={
                    "theme": clean_prose(theme),
                    "source_reference": clean_prose(reference),
                },
            )
        )
    if len(documents) + sum(exclusions.values()) != len(rows):
        raise AssertionError("thematic-reference accounting is incomplete")
    return documents, {
        "input_row_count": len(rows),
        "document_count_before_dedup": len(documents),
        "truncated_text_count": truncated,
        "exclusions": exclusions,
    }


def _split_ranges(
    indexes: Iterable[int],
    corpus: CanonicalCorpus,
    max_range_verses: int,
) -> list[Binding]:
    ordered = sorted(set(indexes))
    if not ordered:
        return []
    ranges: list[Binding] = []
    start = previous = ordered[0]
    for index in ordered[1:]:
        previous_coordinate = display_coordinate(corpus.rows[previous]["ref"])
        coordinate = display_coordinate(corpus.rows[index]["ref"])
        contiguous = index == previous + 1 and coordinate[0] == previous_coordinate[0]
        if not contiguous or index - start + 1 > max_range_verses:
            ranges.append(Binding(start, previous))
            start = index
        previous = index
    ranges.append(Binding(start, previous))
    return ranges


def _aliases(raw: Any, max_associations: int) -> tuple[list[str], int]:
    if not isinstance(raw, str):
        return [], 0
    values = sorted(
        {clean_prose(value) for value in raw.split(",") if clean_prose(value)},
        key=lambda value: (value.casefold(), value),
    )
    return values[:max_associations], max(0, len(values) - max_associations)


def _build_graph_documents(
    connection: sqlite3.Connection,
    corpus: CanonicalCorpus,
    *,
    max_range_verses: int,
    max_source_chars: int,
    max_canonical_chars: int,
    max_associations: int,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    entities: dict[str, dict[str, dict[str, Any]]] = {
        "person": {
            str(entity_id): {
                "name": name,
                "also_called": also_called,
                "description": description,
            }
            for entity_id, name, also_called, description in connection.execute(
                "SELECT id, name, also_called, description FROM graph_people ORDER BY id"
            )
        },
        "place": {
            str(entity_id): {"name": name, "feature_type": feature_type}
            for entity_id, name, feature_type in connection.execute(
                "SELECT id, name, feature_type FROM graph_places ORDER BY id"
            )
        },
        "event": {
            str(entity_id): {
                "title": title,
                "start_year": start_year,
                "duration": duration,
            }
            for entity_id, title, start_year, duration in connection.execute(
                "SELECT id, title, start_year, duration FROM graph_events ORDER BY id"
            )
        },
    }

    event_people: dict[str, set[str]] = defaultdict(set)
    event_places: dict[str, set[str]] = defaultdict(set)
    edge_stats = {
        "input_person_event_edge_count": 0,
        "input_event_place_edge_count": 0,
        "unknown_person_event_edge_count": 0,
        "unknown_event_place_edge_count": 0,
    }
    for person_id, event_id in connection.execute(
        "SELECT person_id, event_id FROM graph_person_event_edges ORDER BY event_id, person_id"
    ):
        edge_stats["input_person_event_edge_count"] += 1
        if str(event_id) not in entities["event"] or str(person_id) not in entities["person"]:
            edge_stats["unknown_person_event_edge_count"] += 1
            continue
        event_people[str(event_id)].add(clean_prose(entities["person"][str(person_id)]["name"]))
    for event_id, place_id in connection.execute(
        "SELECT event_id, place_id FROM graph_event_place_edges ORDER BY event_id, place_id"
    ):
        edge_stats["input_event_place_edge_count"] += 1
        if str(event_id) not in entities["event"] or str(place_id) not in entities["place"]:
            edge_stats["unknown_event_place_edge_count"] += 1
            continue
        event_places[str(event_id)].add(clean_prose(entities["place"][str(place_id)]["name"]))

    mention_exclusions = {
        "unknown_entity": 0,
        "duplicate_mention": 0,
        **{key: 0 for key in _empty_exclusions() if key != "empty_text"},
    }
    input_mentions = 0
    mention_indexes: dict[tuple[str, str], list[int]] = defaultdict(list)
    seen_mentions: set[tuple[str, str, int]] = set()
    for reference, entity_type, raw_entity_id in connection.execute(
        "SELECT verse_ref, entity_type, entity_id FROM graph_verse_mentions "
        "ORDER BY entity_type, entity_id, verse_ref"
    ):
        input_mentions += 1
        entity_id = str(raw_entity_id)
        if entity_type not in entities or entity_id not in entities[entity_type]:
            mention_exclusions["unknown_entity"] += 1
            continue
        binding, reason = resolve_reference(reference, corpus, 1)
        if binding is None:
            mention_exclusions[reason or "malformed_reference"] += 1
            continue
        key = (str(entity_type), entity_id, binding.start)
        if key in seen_mentions:
            mention_exclusions["duplicate_mention"] += 1
            continue
        seen_mentions.add(key)
        mention_indexes[(str(entity_type), entity_id)].append(binding.start)

    documents = []
    represented: dict[str, set[str]] = {key: set() for key in entities}
    truncated_text = 0
    missing_description = 0
    truncated_associations = 0
    for entity_type in ("person", "place", "event"):
        for entity_id, entity in sorted(entities[entity_type].items()):
            ranges = _split_ranges(
                mention_indexes.get((entity_type, entity_id), ()),
                corpus,
                max_range_verses,
            )
            if not ranges:
                continue
            if entity_type == "person":
                aliases, discarded = _aliases(entity.get("also_called"), max_associations)
                truncated_associations += discarded
                description = clean_prose(entity.get("description"))
                missing_description += int(not description)
                parts = [f"Person: {entity.get('name') or entity_id}"]
                if aliases:
                    parts.append("Also called: " + "; ".join(aliases))
                if description:
                    parts.append(description)
                metadata = {"aliases": aliases, "description_available": bool(description)}
            elif entity_type == "place":
                feature = clean_prose(entity.get("feature_type"))
                missing_description += int(not feature)
                parts = [f"Place: {entity.get('name') or entity_id}"]
                if feature:
                    parts.append(f"Place type: {feature}")
                metadata = {"feature_type": feature or None}
            else:
                people = sorted(event_people.get(entity_id, ()), key=lambda value: (value.casefold(), value))
                places = sorted(event_places.get(entity_id, ()), key=lambda value: (value.casefold(), value))
                truncated_associations += max(0, len(people) - max_associations)
                truncated_associations += max(0, len(places) - max_associations)
                people = people[:max_associations]
                places = places[:max_associations]
                parts = [f"Event: {entity.get('title') or entity_id}"]
                if people:
                    parts.append("People: " + "; ".join(people))
                if places:
                    parts.append("Places: " + "; ".join(places))
                metadata = {
                    "people": people,
                    "places": places,
                    "start_year": entity.get("start_year"),
                    "duration": clean_prose(entity.get("duration")) or None,
                }
            text, was_truncated = compose_text(parts, max_source_chars)
            if not text:
                # Names/titles are required by the upstream graph schema, so
                # this indicates corrupt source data rather than a soft skip.
                raise ValueError(f"graph {entity_type} {entity_id!r} has no semantic text")
            truncated_text += int(was_truncated)
            represented[entity_type].add(entity_id)
            for ordinal, binding in enumerate(ranges, 1):
                documents.append(
                    # Event text may be derived from two explicit graph-edge
                    # tables in addition to the event row itself. Keep those
                    # tables visible even when a particular event has no edge.
                    _document(
                        document_id=f"study-bible.graph-{entity_type}.{entity_id}.{ordinal}",
                        source_kind=f"graph-{entity_type}",
                        source_tables=(
                            f"graph_{'people' if entity_type == 'person' else 'places' if entity_type == 'place' else 'events'}",
                            "graph_verse_mentions",
                            *(
                                ("graph_person_event_edges", "graph_event_place_edges")
                                if entity_type == "event"
                                else ()
                            ),
                        ),
                        source_record_ids=(
                            f"graph_{'people' if entity_type == 'person' else 'places' if entity_type == 'place' else 'events'}:{entity_id}",
                        ),
                        text=text,
                        text_truncated=was_truncated,
                        corpus=corpus,
                        binding=binding,
                        max_canonical_chars=max_canonical_chars,
                        metadata=metadata,
                    )
                )

    valid_mentions = input_mentions - sum(mention_exclusions.values())
    if valid_mentions != len(seen_mentions):
        raise AssertionError("graph mention accounting is incomplete")
    by_type = {
        entity_type: {
            "input_entity_count": len(typed),
            "represented_entity_count": len(represented[entity_type]),
            "entity_without_document_count": len(typed) - len(represented[entity_type]),
            "document_count_before_dedup": sum(
                row["source_kind"] == f"graph-{entity_type}" for row in documents
            ),
        }
        for entity_type, typed in entities.items()
    }
    return documents, {
        "input_mention_count": input_mentions,
        "valid_unique_mention_count": valid_mentions,
        "mention_exclusions": mention_exclusions,
        "entity_types": by_type,
        "entity_description_or_type_missing_count": missing_description,
        "truncated_text_count": truncated_text,
        "truncated_association_count": truncated_associations,
        **edge_stats,
    }


def _deduplicate_documents(
    documents: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], dict[str, int]]:
    """Collapse only exact same-source-kind/range/text duplicate evidence."""

    documents.sort(
        key=lambda row: (
            row["start_index"],
            row["end_index"],
            row["source_kind"],
            row["id"],
        )
    )
    selected: dict[tuple[str, int, int, str], dict[str, Any]] = {}
    duplicate_counts: dict[str, int] = defaultdict(int)
    for row in documents:
        key = (
            row["source_kind"],
            row["start_index"],
            row["end_index"],
            clean_prose(row["text"]).casefold(),
        )
        previous = selected.get(key)
        if previous is None:
            selected[key] = row
            continue
        duplicate_counts[row["source_kind"]] += 1
        previous["source_tables"] = sorted(
            set(previous["source_tables"]) | set(row["source_tables"])
        )
        previous["source_record_ids"] = sorted(
            set(previous["source_record_ids"]) | set(row["source_record_ids"])
        )
    output = list(selected.values())
    output.sort(
        key=lambda row: (
            row["start_index"],
            row["end_index"],
            row["source_kind"],
            row["id"],
        )
    )
    seen_ids: set[str] = set()
    for row in output:
        if row["id"] in seen_ids:
            raise ValueError(f"duplicate semantic document id {row['id']!r}")
        seen_ids.add(row["id"])
    return output, dict(sorted(duplicate_counts.items()))


def build(
    canonical: Path,
    database: Path,
    *,
    max_range_verses: int = DEFAULT_MAX_RANGE_VERSES,
    max_source_chars: int = DEFAULT_MAX_SOURCE_CHARS,
    max_canonical_chars: int = DEFAULT_MAX_CANONICAL_CHARS,
    max_associations: int = DEFAULT_MAX_ASSOCIATIONS,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    if not 1 <= max_range_verses <= 256:
        raise ValueError("max_range_verses must be between 1 and 256")
    if not 128 <= max_source_chars <= 16_384:
        raise ValueError("max_source_chars must be between 128 and 16384")
    if not 128 <= max_canonical_chars <= 16_384:
        raise ValueError("max_canonical_chars must be between 128 and 16384")
    if not 1 <= max_associations <= 256:
        raise ValueError("max_associations must be between 1 and 256")
    corpus = load_canonical(canonical)
    connection = _read_only_connection(database)
    try:
        hlt_docs, hlt_stats = _build_hlt_documents(
            connection,
            corpus,
            max_range_verses=max_range_verses,
            max_source_chars=max_source_chars,
            max_canonical_chars=max_canonical_chars,
        )
        hlt_verse_docs, hlt_verse_stats = _build_hlt_verse_documents(
            connection,
            corpus,
            max_range_verses=max_range_verses,
            max_source_chars=max_source_chars,
            max_canonical_chars=max_canonical_chars,
        )
        theology_docs, theme_stats, content_stats = _build_theology_documents(
            connection,
            corpus,
            max_range_verses=max_range_verses,
            max_source_chars=max_source_chars,
            max_canonical_chars=max_canonical_chars,
        )
        thematic_docs, thematic_stats = _build_thematic_reference_documents(
            connection,
            corpus,
            max_range_verses=max_range_verses,
            max_source_chars=max_source_chars,
            max_canonical_chars=max_canonical_chars,
        )
        graph_docs, graph_stats = _build_graph_documents(
            connection,
            corpus,
            max_range_verses=max_range_verses,
            max_source_chars=max_source_chars,
            max_canonical_chars=max_canonical_chars,
            max_associations=max_associations,
        )
    finally:
        connection.close()

    raw_documents = hlt_verse_docs + hlt_docs + theology_docs + thematic_docs + graph_docs
    documents, duplicate_counts = _deduplicate_documents(raw_documents)
    document_counts: dict[str, int] = defaultdict(int)
    covered_verses: set[int] = set()
    for row in documents:
        document_counts[row["source_kind"]] += 1
        covered_verses.update(range(row["start_index"], row["end_index"] + 1))
    manifest = {
        "schema_version": SCHEMA_VERSION,
        "kind": "source-semantic-document-corpus",
        "builder": "scripts/build_source_semantic_documents.py",
        "canonical": {
            "path": str(canonical),
            "verse_count": len(corpus.rows),
            "sha256": sha256_file(canonical),
        },
        "source": {
            "dataset": "studybible-mcp study and graph tables",
            "url": SOURCE_URL,
            "database_sha256": sha256_file(database),
            "tables": sorted(REQUIRED_TABLES),
            "licensing": {
                "graph": {
                    "source": "Theographic Bible Metadata",
                    "url": THEOGRAPHIC_URL,
                    "license": "CC BY-SA 4.0",
                },
                "hlt_and_theology": {
                    "hlt_license": "CC BY-SA 4.0",
                    "theology_status": "upstream provenance; redistribution review required",
                },
            },
        },
        "parameters": {
            "max_range_verses": max_range_verses,
            "max_source_chars": max_source_chars,
            "max_canonical_chars": max_canonical_chars,
            "max_associations": max_associations,
        },
        "input_document_count_before_dedup": len(raw_documents),
        "document_count": len(documents),
        "document_counts": dict(sorted(document_counts.items())),
        "covered_canonical_verse_count": len(covered_verses),
        "duplicate_documents_removed": sum(duplicate_counts.values()),
        "duplicate_documents_removed_by_source_kind": duplicate_counts,
        "sources": {
            "hlt_study_notes": hlt_stats,
            "hlt_verses": hlt_verse_stats,
            "theology_themes": theme_stats,
            "theology_content": content_stats,
            "thematic_references": thematic_stats,
            "graph": graph_stats,
        },
    }
    if len(documents) + manifest["duplicate_documents_removed"] != len(raw_documents):
        raise AssertionError("semantic document dedup accounting is incomplete")
    if not documents:
        raise ValueError("no source semantic documents survived validation")
    return documents, manifest


def write_outputs(
    documents: list[dict[str, Any]],
    manifest: dict[str, Any],
    output: Path,
) -> Path:
    output.parent.mkdir(parents=True, exist_ok=True)
    rendered = "".join(
        json.dumps(row, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
        for row in documents
    )
    output.write_text(rendered, encoding="utf-8")
    bound_manifest = dict(manifest)
    bound_manifest["output"] = {
        "path": str(output),
        "bytes": output.stat().st_size,
        "sha256": sha256_file(output),
    }
    manifest_path = output.with_suffix(".manifest.json")
    manifest_path.write_text(
        json.dumps(bound_manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return manifest_path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--study-bible", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--max-range-verses", type=int, default=DEFAULT_MAX_RANGE_VERSES)
    parser.add_argument("--max-source-chars", type=int, default=DEFAULT_MAX_SOURCE_CHARS)
    parser.add_argument("--max-canonical-chars", type=int, default=DEFAULT_MAX_CANONICAL_CHARS)
    parser.add_argument("--max-associations", type=int, default=DEFAULT_MAX_ASSOCIATIONS)
    args = parser.parse_args()
    documents, manifest = build(
        args.canonical,
        args.study_bible,
        max_range_verses=args.max_range_verses,
        max_source_chars=args.max_source_chars,
        max_canonical_chars=args.max_canonical_chars,
        max_associations=args.max_associations,
    )
    manifest_path = write_outputs(documents, manifest, args.output)
    print(
        f"wrote {len(documents)} source semantic documents -> {args.output} "
        f"(manifest {manifest_path})"
    )


if __name__ == "__main__":
    main()

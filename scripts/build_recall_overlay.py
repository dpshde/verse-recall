#!/usr/bin/env python3
"""Build deterministic recall overlays aligned to a canonical Bible JSONL.

The output is an intermediate JSONL contract for build-time indexing. It does not
change the runtime pack format. Two record types are emitted:

* ``concept`` records load source-verified Aquifer corrections, group canonical
  BSB ``events`` into passage ranges, derive corpus-unique event signatures, and
  optionally align Selah topic labels to their ranked canonical passages.
* ``alternate_text`` records map another translation's text to a canonical ref.

Alternate inputs must use the same ``ref`` identifiers as the canonical source,
or include matching ``book``, ``chapter``, and ``verseNum``/``verse`` fields.
Only canonical intersections are emitted; coverage and unmatched refs are made
explicit in the manifest. This lets a licensed ESV input use the same pipeline as
the public-domain KJV without distributing either generated overlay by accident.
"""

from __future__ import annotations

import argparse
import json
import math
import re
import sqlite3
import sys
import tempfile
import unicodedata
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator

try:
    from build_correction_records import (
        CorrectionBuild,
        compile_reviewed_corrections,
    )
except ModuleNotFoundError:  # imported as a repository module in unit tests
    from scripts.build_correction_records import (
        CorrectionBuild,
        compile_reviewed_corrections,
    )


SCHEMA_VERSION = 1
SAMPLE_LIMIT = 20
EVENT_SIGNATURE_NORMALIZER_VERSION = 2
EVENT_SIGNATURE_ALIAS_PREFIX = "event-signature-v1:"
EVENT_SIGNATURE_RESERVED_PREFIX = "event-signature-"

# The topic browser artifact uses standard OSIS book codes while the canonical
# JSONL exposes full book names. Keep the conversion explicit: topic refs are
# aligned by (book, chapter, verse), never by row position.
OSIS_BY_BOOK_KEY = {
    "genesis": "GEN",
    "exodus": "EXO",
    "leviticus": "LEV",
    "numbers": "NUM",
    "deuteronomy": "DEU",
    "joshua": "JOS",
    "judges": "JDG",
    "ruth": "RUT",
    "1samuel": "1SA",
    "2samuel": "2SA",
    "1kings": "1KI",
    "2kings": "2KI",
    "1chronicles": "1CH",
    "2chronicles": "2CH",
    "ezra": "EZR",
    "nehemiah": "NEH",
    "esther": "EST",
    "job": "JOB",
    "psalm": "PSA",
    "proverbs": "PRO",
    "ecclesiastes": "ECC",
    "songofsolomon": "SNG",
    "isaiah": "ISA",
    "jeremiah": "JER",
    "lamentations": "LAM",
    "ezekiel": "EZK",
    "daniel": "DAN",
    "hosea": "HOS",
    "joel": "JOL",
    "amos": "AMO",
    "obadiah": "OBA",
    "jonah": "JON",
    "micah": "MIC",
    "nahum": "NAM",
    "habakkuk": "HAB",
    "zephaniah": "ZEP",
    "haggai": "HAG",
    "zechariah": "ZEC",
    "malachi": "MAL",
    "matthew": "MAT",
    "mark": "MRK",
    "luke": "LUK",
    "john": "JHN",
    "acts": "ACT",
    "romans": "ROM",
    "1corinthians": "1CO",
    "2corinthians": "2CO",
    "galatians": "GAL",
    "ephesians": "EPH",
    "philippians": "PHP",
    "colossians": "COL",
    "1thessalonians": "1TH",
    "2thessalonians": "2TH",
    "1timothy": "1TI",
    "2timothy": "2TI",
    "titus": "TIT",
    "philemon": "PHM",
    "hebrews": "HEB",
    "james": "JAS",
    "1peter": "1PE",
    "2peter": "2PE",
    "1john": "1JN",
    "2john": "2JN",
    "3john": "3JN",
    "jude": "JUD",
    "revelation": "REV",
}
SIMPLE_OSIS_REF = re.compile(
    r"^(?P<book>[1-3]?[A-Z]{2,3})\.(?P<chapter>[1-9][0-9]*)\."
    r"(?P<verse>[1-9][0-9]*)(?:-(?P<end_verse>[1-9][0-9]*))?$"
)
FULL_OSIS_RANGE = re.compile(
    r"^(?P<book>[1-3]?[A-Z]{2,3})\.(?P<chapter>[1-9][0-9]*)\."
    r"(?P<verse>[1-9][0-9]*)-(?P<end_book>[1-3]?[A-Z]{2,3})\."
    r"(?P<end_chapter>[1-9][0-9]*)\.(?P<end_verse>[1-9][0-9]*)$"
)
DISPLAY_BCV_REF = re.compile(
    r"^(?P<book>.+?)\s+(?P<chapter>[1-9][0-9]*):(?P<verse>[1-9][0-9]*)$"
)
BOOK_KEY_BY_OSIS = {osis: book for book, osis in OSIS_BY_BOOK_KEY.items()}

# Theographic uses conventional mixed-case OSIS abbreviations rather than the
# three-letter codes in Selah's browser artifacts. Normalize both to the
# canonical book key so graph rows are never aligned by row position.
THEOGRAPHIC_BOOK_KEYS = {
    "Gen": "genesis", "Exod": "exodus", "Lev": "leviticus",
    "Num": "numbers", "Deut": "deuteronomy", "Josh": "joshua",
    "Judg": "judges", "Ruth": "ruth", "1Sam": "1samuel",
    "2Sam": "2samuel", "1Kgs": "1kings", "2Kgs": "2kings",
    "1Chr": "1chronicles", "2Chr": "2chronicles", "Ezra": "ezra",
    "Neh": "nehemiah", "Esth": "esther", "Job": "job", "Ps": "psalm",
    "Prov": "proverbs", "Eccl": "ecclesiastes", "Song": "songofsolomon",
    "Isa": "isaiah", "Jer": "jeremiah", "Lam": "lamentations",
    "Ezek": "ezekiel", "Dan": "daniel", "Hos": "hosea", "Joel": "joel",
    "Amos": "amos", "Obad": "obadiah", "Jonah": "jonah", "Mic": "micah",
    "Nah": "nahum", "Hab": "habakkuk", "Zeph": "zephaniah",
    "Hag": "haggai", "Zech": "zechariah", "Mal": "malachi",
    "Matt": "matthew", "Mark": "mark", "Luke": "luke", "John": "john",
    "Acts": "acts", "Rom": "romans", "1Cor": "1corinthians",
    "2Cor": "2corinthians", "Gal": "galatians", "Eph": "ephesians",
    "Phil": "philippians", "Col": "colossians",
    "1Thess": "1thessalonians", "2Thess": "2thessalonians",
    "1Tim": "1timothy", "2Tim": "2timothy", "Titus": "titus",
    "Phlm": "philemon", "Heb": "hebrews", "Jas": "james",
    "1Pet": "1peter", "2Pet": "2peter", "1John": "1john",
    "2John": "2john", "3John": "3john", "Jude": "jude",
    "Rev": "revelation",
}
THEOGRAPHIC_REF = re.compile(
    r"^(?P<book>[1-3]?[A-Za-z]+)\.(?P<chapter>[1-9][0-9]*)\."
    r"(?P<verse>[1-9][0-9]*)$"
)


@dataclass(frozen=True)
class CanonicalVerse:
    source_ref: str
    display_ref: str
    text: str
    events: tuple[str, ...]
    bcv_key: tuple[str, str, str] | None


@dataclass(frozen=True)
class AlternateSource:
    translation: str
    path: Path


@dataclass(frozen=True)
class TopicSources:
    canonical_topics_path: Path
    verse_rankings_path: Path


@dataclass(frozen=True)
class TopicRanking:
    canonical_ref: str
    raw_score: float
    rank: int


@dataclass(frozen=True)
class TopicBuild:
    records: tuple[dict, ...]
    input_topic_count: int
    ranking_topic_count: int
    ranking_row_count: int
    generated_alias_count: int
    event_overlap_topic_ids: tuple[str, ...]
    missing_ranking_topic_ids: tuple[str, ...]
    unknown_ranking_topic_ids: tuple[str, ...]
    missing_canonical_targets: tuple[str, ...]
    topics_without_valid_target_ids: tuple[str, ...]


@dataclass(frozen=True)
class EventBuild:
    records: tuple[dict, ...]
    discovered_event_count: int
    quarantined_events: tuple[dict, ...]
    source_alias_count: int = 0
    generated_signature_count: int = 0
    full_signature_count: int = 0
    partial_signature_count: int = 0
    single_token_signature_count: int = 0
    event_with_signature_count: int = 0
    event_without_signature_count: int = 0
    suppressed_unsafe_event_count: int = 0
    suppressed_unsafe_candidate_count: int = 0
    suppressed_collision_candidate_count: int = 0
    suppressed_single_without_primary_evidence_count: int = 0
    suppressed_single_common_corpus_count: int = 0
    corpus_candidate_count: int = 0
    corpus_collision_signature_count: int = 0
    corpus_collision_candidate_count: int = 0
    collision_samples: tuple[dict, ...] = ()


@dataclass(frozen=True)
class StudyBibleBuild:
    records: tuple[dict, ...]
    manifests: tuple[dict, ...]


@dataclass(frozen=True)
class AlternateAlignment:
    translation: str
    input_verse_count: int
    records: tuple[dict, ...]
    unmatched_source_refs: tuple[str, ...]
    missing_canonical_refs: tuple[str, ...]


def iter_jsonl(path: Path) -> Iterator[tuple[int, dict]]:
    with path.open(encoding="utf-8") as stream:
        for line_no, line in enumerate(stream, 1):
            line = line.strip()
            if not line:
                continue
            try:
                row = json.loads(line)
            except json.JSONDecodeError as exc:
                raise ValueError(f"{path}:{line_no}: {exc}") from exc
            if not isinstance(row, dict):
                raise ValueError(f"{path}:{line_no}: expected a JSON object")
            yield line_no, row


def value_as_string(value: object) -> str:
    if isinstance(value, str):
        return value.strip()
    if isinstance(value, int):
        return str(value)
    return ""


def display_ref(row: dict) -> str:
    book = value_as_string(row.get("book"))
    chapter = value_as_string(row.get("chapter"))
    verse = value_as_string(row.get("verseNum") or row.get("verse"))
    if book == "Psalms":
        book = "Psalm"
    if book and chapter and verse:
        return f"{book} {chapter}:{verse}"
    return value_as_string(row.get("ref"))


def bcv_key(row: dict) -> tuple[str, str, str] | None:
    book = value_as_string(row.get("book"))
    chapter = value_as_string(row.get("chapter"))
    verse = value_as_string(row.get("verseNum") or row.get("verse"))
    if book and chapter and verse:
        book = re.sub(r"[^0-9a-z]+", "", book.casefold())
        if book == "psalms":
            book = "psalm"
        return book, chapter.lstrip("0") or "0", verse.lstrip("0") or "0"

    # Production base packs intentionally keep only ``ref`` and ``text``.
    # Recover BCV from either their display refs (``1 Samuel 17:1``) or an
    # OSIS source ref (``1SA.17.1``) so curated ranges, Selah topics, and
    # structured alternate translations remain reproducible from that form.
    source_ref = value_as_string(row.get("ref"))
    display_match = DISPLAY_BCV_REF.fullmatch(source_ref)
    if display_match is not None:
        book_key = re.sub(
            r"[^0-9a-z]+", "", display_match.group("book").casefold()
        )
        if book_key == "psalms":
            book_key = "psalm"
        if book_key not in OSIS_BY_BOOK_KEY:
            return None
        return (
            book_key,
            display_match.group("chapter"),
            display_match.group("verse"),
        )

    osis_match = SIMPLE_OSIS_REF.fullmatch(source_ref.upper())
    if osis_match is None or osis_match.group("end_verse") is not None:
        return None
    book_key = BOOK_KEY_BY_OSIS.get(osis_match.group("book"))
    if book_key is None:
        return None
    return book_key, osis_match.group("chapter"), osis_match.group("verse")


def normalized_concept_key(label: str) -> str:
    text = unicodedata.normalize("NFKC", label).casefold()
    text = text.replace("\u2018", "'").replace("\u2019", "'")
    text = re.sub(r"[^0-9a-z']+", " ", text)
    return " ".join(text.split())


def runtime_search_surface(text: str) -> str:
    """Mirror recall-core's exact concept surface normalization.

    Curated validation must reject collisions in the representation the runtime
    actually searches, not only in the more editorially useful concept key.
    In particular, apostrophes do not form a token boundary at runtime, so
    ``Paul's`` and ``Pauls`` are the same search surface.
    """

    words: list[str] = []
    current: list[str] = []
    for character in text.lower():
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


# Event signatures use the same stop filtering and morphology identity as the
# runtime query normalizer.  They are compiled as reserved aliases on direct-
# safe BSB event records; ordinary aliases remain exact human-readable labels.
# Keeping this policy build-time makes its corpus coverage and collisions
# measurable instead of inventing query-specific rewrites at runtime.
EVENT_SIGNATURE_TOKEN = re.compile(
    r"[0-9A-Za-z]+(?:[-'\u2019][0-9A-Za-z]+)*"
)
EVENT_SIGNATURE_TOKEN_PART = re.compile(r"[0-9A-Za-z]+")
EVENT_SIGNATURE_STOP_WORDS = {
    "a",
    "an",
    "the",
    "and",
    "or",
    "but",
    "if",
    "of",
    "to",
    "in",
    "on",
    "for",
    "is",
    "are",
    "was",
    "were",
    "be",
    "been",
    "being",
    "that",
    "this",
    "it",
    "he",
    "she",
    "they",
    "we",
    "you",
    "i",
    "his",
    "her",
    "their",
    "my",
    "your",
    "with",
    "as",
    "by",
    "from",
    "at",
    "not",
    "no",
    "so",
    "one",
    "only",
    "about",
    "verse",
    "verses",
    "passage",
    "passages",
    "scripture",
    "bible",
    "guy",
    "man",
    "people",
    "person",
    "someone",
    "where",
    "when",
    "who",
    "what",
    "how",
    "which",
    "there",
    "here",
    "into",
    "over",
    "under",
    "than",
    "then",
    "also",
    "just",
    "very",
    "much",
    "more",
    "some",
    "any",
    "all",
    "its",
    "our",
    "does",
    "did",
    "do",
    "has",
    "have",
    "had",
    "will",
    "would",
    "could",
    "should",
    "can",
    "may",
    "might",
    "shall",
    "unto",
    "ye",
    "thou",
    "thy",
    "thee",
}

# These are generic event/era roles, not distinctive one-token descriptions.
# They remain available inside globally unique multi-token signatures.
EVENT_SIGNATURE_GENERIC_SINGLE_STEMS = {
    "arrest",
    "begin",
    "birth",
    "call",
    "conversion",
    "death",
    "deliver",
    "discourse",
    "heal",
    "journey",
    "king",
    "kingdom",
    "life",
    "lifetime",
    "meet",
    "mission",
    "oppression",
    "parable",
    "preach",
    "prophecy",
    "reign",
    "return",
    "sermon",
    "speak",
    "subjugation",
    "tour",
    "trial",
    "visit",
    "work",
    "write",
    "year",
}
EVENT_SIGNATURE_UNSAFE_PREFIXES = (
    "lifetime of ",
    "reign of ",
    "judgeship of ",
)
EVENT_SIGNATURE_KIND_ORDER = {"full": 0, "partial": 1, "single": 2}
MAX_FULL_EVENT_SIGNATURE_STEMS = 8
MAX_PARTIAL_EVENT_SOURCE_STEMS = 5
MIN_DISTINCTIVE_SINGLE_STEM_LENGTH = 7
MAX_DISTINCTIVE_SINGLE_VERSE_FREQUENCY = 5


@dataclass(frozen=True)
class EventSignatureCandidate:
    kind: str
    stems: tuple[str, ...]

    @property
    def surface(self) -> str:
        return " ".join(self.stems)

    def alias(self) -> str:
        return f"{EVENT_SIGNATURE_ALIAS_PREFIX}{self.kind}:{self.surface}"


def event_signature_stem(token: str) -> str:
    """Mirror recall-core normalizer v2 for generated event signatures."""

    irregular = {
        "lost": "lose", "loses": "lose", "lose": "lose",
        "losing": "lose", "losts": "lose", "loved": "love",
        "loves": "love", "loving": "love", "love": "love",
        "gave": "give", "gives": "give", "giving": "give",
        "give": "give", "given": "give", "found": "find",
        "finds": "find", "finding": "find", "find": "find",
        "left": "leave", "leaves": "leave", "leaving": "leave",
        "leave": "leave", "blessed": "bless", "blesses": "bless",
        "blessing": "bless", "bless": "bless", "believes": "believe",
        "believed": "believe", "believing": "believe",
        "believe": "believe", "believers": "believe",
        "believer": "believe", "created": "create",
        "creates": "create", "creating": "create", "create": "create",
        "creation": "create", "made": "make", "make": "make",
        "makes": "make", "making": "make", "shepherds": "shepherd",
        "shepherding": "shepherd", "shepherded": "shepherd",
        "shepherd": "shepherd", "sheep": "sheep", "sheeps": "sheep",
        "sons": "son", "son": "son", "worlds": "world",
        "world": "world", "spirits": "spirit", "spirit": "spirit",
        "moneys": "money", "money": "money", "evils": "evil",
        "evil": "evil", "roots": "root", "root": "root",
        "poor": "poor", "begotten": "begot", "ninety": "ninety",
        "ninty": "ninety", "hundred": "hundred", "parable": "parable",
        "parables": "parable", "kingdom": "kingdom",
        "kingdoms": "kingdom", "heaven": "heaven",
        "heavens": "heaven", "eternal": "eternal",
        "eternally": "eternal", "perish": "perish",
        "perished": "perish", "perishes": "perish",
        "perishing": "perish", "walking": "walk", "walked": "walk",
        "walks": "walk", "walk": "walk", "scared": "scare",
        "scares": "scare", "scaring": "scare", "scare": "scare",
        "dangerous": "danger", "danger": "danger", "darkness": "dark",
        "dark": "dark", "qualities": "quality", "quality": "quality",
        "faithful": "faithful", "faithfully": "faithful",
        "faithfulness": "faithful", "angry": "anger", "anger": "anger",
        "produces": "produce", "produced": "produce",
        "producing": "produce", "produce": "produce",
        "beginning": "begin", "begin": "begin", "began": "begin",
        "begun": "begin", "starts": "start", "started": "start",
        "starting": "start", "start": "start", "armor": "armor",
        "armour": "armor", "psalm": "psalm", "psalms": "psalm",
    }
    if token in irregular:
        return irregular[token]
    if token in {"everything", "everyone", "everyone's"}:
        return token
    if token.endswith(
        ("thing", "ness", "ous", "tion", "sion", "ment", "ity", "ence", "ance")
    ):
        return token

    result = token
    if len(result) > 6 and result.endswith("ing"):
        base = result[:-3]
        if len(base) >= 4:
            result = base
            if len(result) >= 2 and result[-1] == result[-2]:
                result = result[:-1]
    elif len(result) > 5 and result.endswith("ies"):
        result = result[:-3] + "y"
    elif len(result) > 5 and result.endswith("es") and not result.endswith(
        ("ness", "ous")
    ):
        base = result[:-2]
        if base.endswith(("s", "x", "z", "ch", "sh")):
            result = base
    elif (
        len(result) > 4
        and result.endswith("s")
        and not result.endswith(("ss", "us"))
    ):
        result = result[:-1]
    elif len(result) > 5 and result.endswith("ed"):
        result = result[:-2]
        if result.endswith("i"):
            result = result[:-1] + "y"
    elif len(result) > 5 and result.endswith("ly"):
        result = result[:-2]
    return result


def event_signature_terms(text: str) -> tuple[tuple[str, ...], frozenset[str]]:
    """Return sorted unique runtime stems and stems written in lower case."""

    text = unicodedata.normalize("NFKC", text)
    stems: set[str] = set()
    lower_case_stems: set[str] = set()
    for raw_compound in EVENT_SIGNATURE_TOKEN.findall(text):
        compound_is_lower = raw_compound.islower()
        for raw_token in EVENT_SIGNATURE_TOKEN_PART.findall(raw_compound):
            token = raw_token.casefold()
            token = (
                token.replace("armour", "armor")
                .replace("favour", "favor")
                .replace("honour", "honor")
                .replace("saviour", "savior")
                .replace("neighbour", "neighbor")
            )
            if len(token) <= 1 or token in EVENT_SIGNATURE_STOP_WORDS:
                continue
            stem = event_signature_stem(token)
            if len(stem) <= 1:
                continue
            stems.add(stem)
            if compound_is_lower:
                lower_case_stems.add(stem)
    return tuple(sorted(stems)), frozenset(lower_case_stems)


def event_signature_candidates(label: str) -> tuple[EventSignatureCandidate, ...]:
    """Derive bounded candidates from one source label, without target data."""

    stems, lower_case_stems = event_signature_terms(label)
    candidates: set[EventSignatureCandidate] = set()
    if 2 <= len(stems) <= MAX_FULL_EVENT_SIGNATURE_STEMS:
        candidates.add(EventSignatureCandidate("full", stems))

    # Partial signatures omit exactly one content stem.  They are emitted only
    # for short labels and must retain a lower-case source word, which rules out
    # participant/place-only combinations such as two title-cased names.
    if 3 <= len(stems) <= MAX_PARTIAL_EVENT_SOURCE_STEMS:
        for omitted in stems:
            partial = tuple(stem for stem in stems if stem != omitted)
            if not lower_case_stems.intersection(partial):
                continue
            if all(stem in EVENT_SIGNATURE_GENERIC_SINGLE_STEMS for stem in partial):
                continue
            candidates.add(EventSignatureCandidate("partial", partial))

    # A singleton is eligible only when it is a long lower-case source word.
    # Corpus-wide uniqueness and primary-passage evidence are enforced later.
    if len(stems) >= 2:
        for stem in stems:
            if (
                stem in lower_case_stems
                and len(stem) >= MIN_DISTINCTIVE_SINGLE_STEM_LENGTH
                and stem.isalpha()
                and stem not in EVENT_SIGNATURE_GENERIC_SINGLE_STEMS
            ):
                candidates.add(EventSignatureCandidate("single", (stem,)))

    return tuple(
        sorted(
            candidates,
            key=lambda candidate: (
                EVENT_SIGNATURE_KIND_ORDER[candidate.kind],
                candidate.stems,
            ),
        )
    )


def attach_event_signature_aliases(
    records: tuple[dict, ...],
    labels_by_key: dict[str, tuple[str, ...]],
    primary_terms_by_key: dict[str, frozenset[str]],
    canonical_stem_document_frequency: dict[str, int],
) -> tuple[tuple[dict, ...], dict]:
    """Attach only globally unique signatures from the complete event corpus."""

    candidates_by_key = {
        key: event_signature_candidates(labels[0])
        for key, labels in sorted(labels_by_key.items())
    }

    # A signature is unique only when no other source event label contains all
    # of its cues.  Looking merely for another *eligible generated candidate*
    # is insufficient: title case can make a singleton ineligible, and label
    # length can make a partial ineligible, without making the user's query
    # unambiguous.  Include every spelling grouped under every event key in
    # this containment registry, including quarantined events.
    label_stem_sets_by_key = {
        key: tuple(
            frozenset(event_signature_terms(label)[0]) for label in labels
        )
        for key, labels in sorted(labels_by_key.items())
    }
    candidate_surfaces = {
        candidate.stems
        for candidates in candidates_by_key.values()
        for candidate in candidates
    }
    owners: dict[tuple[str, ...], set[str]] = {
        stems: {
            key
            for key, label_stem_sets in label_stem_sets_by_key.items()
            if any(set(stems).issubset(label_stems) for label_stems in label_stem_sets)
        }
        for stems in sorted(candidate_surfaces)
    }

    collisions = {
        stems: event_keys
        for stems, event_keys in owners.items()
        if len(event_keys) > 1
    }
    collision_samples = tuple(
        {
            "signature": " ".join(stems),
            "event_keys": sorted(event_keys),
        }
        for stems, event_keys in sorted(collisions.items())[:SAMPLE_LIMIT]
    )

    generated_by_kind = {"full": 0, "partial": 0, "single": 0}
    source_alias_count = 0
    event_with_signatures = 0
    event_without_signatures = 0
    suppressed_unsafe_events = 0
    suppressed_unsafe_candidates = 0
    suppressed_collisions = 0
    suppressed_single_evidence = 0
    suppressed_single_common = 0
    output_records: list[dict] = []
    for source_record in records:
        record = dict(source_record)
        source_aliases = list(record["aliases"])
        source_surfaces = [record["key"], record["label"], *source_aliases]
        if any(
            surface.casefold().startswith(EVENT_SIGNATURE_RESERVED_PREFIX)
            for surface in source_surfaces
        ):
            raise ValueError(
                f"BSB event surface uses reserved signature prefix: {record['label']!r}"
            )
        source_alias_count += len(source_aliases)
        candidates = candidates_by_key.get(record["key"], ())
        generated: list[EventSignatureCandidate] = []
        if record["key"].startswith(EVENT_SIGNATURE_UNSAFE_PREFIXES):
            suppressed_unsafe_events += 1
            suppressed_unsafe_candidates += len(candidates)
        else:
            primary_terms = primary_terms_by_key.get(record["key"], frozenset())
            for candidate in candidates:
                if len(owners[candidate.stems]) != 1:
                    suppressed_collisions += 1
                    continue
                if (
                    candidate.kind == "single"
                    and candidate.stems[0] not in primary_terms
                ):
                    suppressed_single_evidence += 1
                    continue
                if (
                    candidate.kind == "single"
                    and canonical_stem_document_frequency.get(
                        candidate.stems[0], 0
                    ) > MAX_DISTINCTIVE_SINGLE_VERSE_FREQUENCY
                ):
                    suppressed_single_common += 1
                    continue
                generated.append(candidate)

        if generated:
            event_with_signatures += 1
        else:
            event_without_signatures += 1
        for candidate in generated:
            generated_by_kind[candidate.kind] += 1
        record["aliases"] = source_aliases + [
            candidate.alias() for candidate in generated
        ]
        output_records.append(record)

    stats = {
        "source_alias_count": source_alias_count,
        "generated_signature_count": sum(generated_by_kind.values()),
        "full_signature_count": generated_by_kind["full"],
        "partial_signature_count": generated_by_kind["partial"],
        "single_token_signature_count": generated_by_kind["single"],
        "event_with_signature_count": event_with_signatures,
        "event_without_signature_count": event_without_signatures,
        "suppressed_unsafe_event_count": suppressed_unsafe_events,
        "suppressed_unsafe_candidate_count": suppressed_unsafe_candidates,
        "suppressed_collision_candidate_count": suppressed_collisions,
        "suppressed_single_without_primary_evidence_count": (
            suppressed_single_evidence
        ),
        "suppressed_single_common_corpus_count": suppressed_single_common,
        "corpus_candidate_count": sum(
            len(candidates) for candidates in candidates_by_key.values()
        ),
        "corpus_collision_signature_count": len(collisions),
        "corpus_collision_candidate_count": sum(
            len(event_keys) for event_keys in collisions.values()
        ),
        "collision_samples": collision_samples,
    }
    return tuple(output_records), stats


EVENT_LABEL_STOP_WORDS = {
    "a",
    "an",
    "and",
    "at",
    "by",
    "from",
    "in",
    "into",
    "of",
    "on",
    "the",
    "to",
}

# Event-kind words describe the relationship, not the participant that
# disambiguates one annotated passage from another.  In particular, a generic
# judges summary can contain "delivered" while the actual Othniel narrative is
# the only range that names Othniel.
EVENT_GENERIC_STEMS = {
    "deliver",
}


def event_word_stem(token: str) -> str:
    """Small deterministic stemmer for event-label/range salience only."""

    if len(token) > 6 and token.endswith("ing"):
        token = token[:-3]
    elif len(token) > 5 and token.endswith("ed"):
        token = token[:-2]
    elif len(token) > 7 and token.endswith("ance"):
        token = token[:-4]
    if len(token) > 4 and token.endswith("s"):
        token = token[:-1]
    return token


def event_content_terms(text: str) -> set[str]:
    return {
        event_word_stem(token)
        for token in normalized_concept_key(text).split()
        if token not in EVENT_LABEL_STOP_WORDS
        and event_word_stem(token) not in EVENT_GENERIC_STEMS
    }


def event_type_cue(key: str, text: str) -> int:
    normalized = f" {normalized_concept_key(text)} "
    if key.startswith("death of "):
        cues = (
            " died ",
            " dead ",
            " death ",
            " breathed his last ",
            " breathed her last ",
            " rested with his fathers ",
            " rested with her fathers ",
            " was killed ",
            " were killed ",
            " was slain ",
            " were slain ",
            " fell asleep ",
        )
        return int(any(cue in normalized for cue in cues))
    if key.startswith("birth of "):
        cues = (
            " born ",
            " birth ",
            " gave birth ",
            " conceived ",
            " came out ",
            " begot ",
            " fathered ",
        )
        return int(any(cue in normalized for cue in cues))
    return 0


def event_range_semantics(
    verses: tuple[CanonicalVerse, ...],
    start: int,
    end: int,
    key: str,
    label_terms: set[str],
) -> tuple[int, int]:
    text = " ".join(verse.text for verse in verses[start : end + 1])
    overlap = len(label_terms & event_content_terms(text))
    return event_type_cue(key, text), overlap


def event_anchor_index(
    verses: tuple[CanonicalVerse, ...],
    start: int,
    end: int,
    key: str,
    label_terms: set[str],
) -> int:
    return max(
        range(start, end + 1),
        key=lambda index: (
            *event_range_semantics(verses, index, index, key, label_terms),
            -index,
        ),
    )


def canonically_adjacent(
    previous: CanonicalVerse, current: CanonicalVerse
) -> bool:
    """Reject apparent runs created by sparse or malformed canonical input."""

    if previous.bcv_key is None or current.bcv_key is None:
        return False
    previous_book, previous_chapter, previous_verse = previous.bcv_key
    current_book, current_chapter, current_verse = current.bcv_key
    if previous_book != current_book:
        return False
    previous_chapter_number = int(previous_chapter)
    current_chapter_number = int(current_chapter)
    previous_verse_number = int(previous_verse)
    current_verse_number = int(current_verse)
    return (
        current_chapter_number == previous_chapter_number
        and current_verse_number == previous_verse_number + 1
    ) or (
        current_chapter_number == previous_chapter_number + 1
        and current_verse_number == 1
    )


def load_canonical(path: Path) -> tuple[CanonicalVerse, ...]:
    verses: list[CanonicalVerse] = []
    source_refs: set[str] = set()
    bcv_keys: set[tuple[str, str, str]] = set()
    for line_no, row in iter_jsonl(path):
        source_ref = value_as_string(row.get("ref"))
        text = value_as_string(row.get("text"))
        shown_ref = display_ref(row)
        if not source_ref:
            raise ValueError(f"{path}:{line_no}: missing ref")
        if not shown_ref:
            raise ValueError(f"{path}:{line_no}: cannot derive display ref")
        if not text:
            raise ValueError(f"{path}:{line_no}: missing text")
        if source_ref in source_refs:
            raise ValueError(f"{path}:{line_no}: duplicate ref {source_ref!r}")
        source_refs.add(source_ref)

        key = bcv_key(row)
        if key is not None:
            if key in bcv_keys:
                raise ValueError(
                    f"{path}:{line_no}: duplicate book/chapter/verse {key}"
                )
            bcv_keys.add(key)

        raw_events = row.get("events") or []
        if not isinstance(raw_events, list) or not all(
            isinstance(event, str) for event in raw_events
        ):
            raise ValueError(f"{path}:{line_no}: events must be a string array")
        events = tuple(event.strip() for event in raw_events if event.strip())
        verses.append(CanonicalVerse(source_ref, shown_ref, text, events, key))
    if not verses:
        raise ValueError(f"{path}: no verses")
    return tuple(verses)


def build_event_records_with_diagnostics(
    verses: tuple[CanonicalVerse, ...],
) -> EventBuild:
    """Build direct event concepts, quarantining structurally unsafe labels.

    ``events`` is annotation data, not a trusted routing table. A useful direct
    concept needs a bounded, coherent primary passage. The longest contiguous
    range is primary (canonical order breaks ties); fragmented or singleton
    annotations stay out of the direct lane.
    """

    grouped: dict[str, dict[str, set]] = {}
    for index, verse in enumerate(verses):
        for label in verse.events:
            key = normalized_concept_key(label)
            if not key:
                continue
            group = grouped.setdefault(key, {"labels": set(), "indexes": set()})
            group["labels"].add(label)
            group["indexes"].add(index)
    records: list[dict] = []
    quarantined: list[dict] = []
    primary_terms_by_key: dict[str, frozenset[str]] = {}
    for key in sorted(grouped):
        labels = sorted(
            grouped[key]["labels"], key=lambda value: (value.casefold(), value)
        )
        indexes = sorted(grouped[key]["indexes"])
        index_ranges: list[tuple[int, int]] = []
        start = previous = indexes[0]
        for index in indexes[1:]:
            if index != previous + 1 or not canonically_adjacent(
                verses[previous], verses[index]
            ):
                index_ranges.append((start, previous))
                start = index
            previous = index
        index_ranges.append((start, previous))

        label_terms = event_content_terms(key)
        semantic_scores = {
            value: event_range_semantics(
                verses, value[0], value[1], key, label_terms
            )
            for value in index_ranges
        }
        primary_start, primary_end = max(
            index_ranges,
            key=lambda value: (
                *semantic_scores[value],
                value[1] - value[0] + 1,
                -value[0],
            ),
        )
        primary_count = primary_end - primary_start + 1
        total_count = len(indexes)
        reasons: list[str] = []
        # A lifetime is not a bounded narrative event. In the source it often
        # tags only a person's birth plus an isolated death/age verse, so the
        # longest contiguous range can be a confidently wrong birth anchor.
        # Keep these records available for later curated age/entity modeling,
        # but never promote the raw annotation as a direct event resolver.
        if key.startswith("lifetime of "):
            reasons.append("unsupported_lifetime_summary")
        if primary_count < 2:
            reasons.append("primary_range_too_short")
        if primary_count > 200:
            reasons.append("primary_range_too_long")
        other_semantics = [
            score
            for value, score in semantic_scores.items()
            if value != (primary_start, primary_end)
        ]
        has_unique_semantic_lead = bool(other_semantics) and semantic_scores[
            (primary_start, primary_end)
        ] > max(other_semantics)
        if (
            len(index_ranges) > 1
            and primary_count * 3 < total_count * 2
            and not has_unique_semantic_lead
        ):
            reasons.append("fragmented_annotations")
        primary_text = " ".join(
            verse.text for verse in verses[primary_start : primary_end + 1]
        )
        if key.startswith("death of ") and not event_type_cue(key, primary_text):
            reasons.append("missing_death_cue")
        if key.startswith("birth of ") and not event_type_cue(key, primary_text):
            reasons.append("missing_birth_cue")

        primary_range = range_record(verses, primary_start, primary_end)
        if reasons:
            quarantined.append(
                {
                    "key": key,
                    "label": labels[0],
                    "reasons": reasons,
                    "range_count": len(index_ranges),
                    "tagged_verse_count": total_count,
                    "primary_range": primary_range,
                }
            )
            continue

        # Raw secondary annotations are diagnostics, not public passage
        # evidence. A correct primary can coexist with a blatantly wrong tag
        # (for example Death of Moses on Genesis 34:1). Preserve only the
        # semantically selected primary; reviewed parallel passages belong in a
        # separately curated record.
        ranges = [range_record(verses, primary_start, primary_end)]
        anchor_index = event_anchor_index(
            verses, primary_start, primary_end, key, label_terms
        )
        records.append(
            {
                "schema_version": SCHEMA_VERSION,
                "kind": "concept",
                "concept_type": "event",
                "key": key,
                "label": labels[0],
                "aliases": labels[1:],
                "source": "BSB.events",
                "anchor_ref": verses[anchor_index].display_ref,
                "target_ranges": ranges,
            }
        )
        primary_terms_by_key[key] = frozenset(
            event_signature_terms(primary_text)[0]
        )

    labels_by_key = {
        key: tuple(
            sorted(
                grouped[key]["labels"],
                key=lambda value: (value.casefold(), value),
            )
        )
        for key in sorted(grouped)
    }
    canonical_stem_document_frequency: dict[str, int] = {}
    for verse in verses:
        for stem in event_signature_terms(verse.text)[0]:
            canonical_stem_document_frequency[stem] = (
                canonical_stem_document_frequency.get(stem, 0) + 1
            )
    signed_records, signature_stats = attach_event_signature_aliases(
        tuple(records),
        labels_by_key,
        primary_terms_by_key,
        canonical_stem_document_frequency,
    )
    return EventBuild(
        records=signed_records,
        discovered_event_count=len(grouped),
        quarantined_events=tuple(quarantined),
        **signature_stats,
    )


def build_event_records(verses: tuple[CanonicalVerse, ...]) -> tuple[dict, ...]:
    """Compatibility wrapper returning only direct-safe event records."""

    return build_event_records_with_diagnostics(verses).records


def range_record(
    verses: tuple[CanonicalVerse, ...], start_index: int, end_index: int
) -> dict:
    return {
        "start_ref": verses[start_index].display_ref,
        "end_ref": verses[end_index].display_ref,
        "verse_count": end_index - start_index + 1,
    }


def load_json_array(path: Path, description: str) -> list:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        raise ValueError(f"{path}: {exc}") from exc
    if not isinstance(value, list):
        raise ValueError(f"{path}: expected {description} to be a JSON array")
    return value


def load_topic_labels(path: Path) -> dict[str, str]:
    """Read browser tuples shaped exactly as ``[topic_id, label]``."""

    labels: dict[str, str] = {}
    normalized_labels: dict[str, str] = {}
    for row_number, row in enumerate(
        load_json_array(path, "canonical topics"), 1
    ):
        if not isinstance(row, list) or len(row) != 2:
            raise ValueError(
                f"{path}: row {row_number}: expected [topic_id, label]"
            )
        topic_id, label = row
        if not isinstance(topic_id, str) or not topic_id.strip():
            raise ValueError(f"{path}: row {row_number}: invalid topic id")
        if not isinstance(label, str) or not label.strip():
            raise ValueError(f"{path}: row {row_number}: invalid topic label")
        topic_id = topic_id.strip()
        label = label.strip()
        if topic_id in labels:
            raise ValueError(f"{path}: row {row_number}: duplicate topic {topic_id!r}")
        key = normalized_concept_key(label)
        if not key:
            raise ValueError(f"{path}: row {row_number}: empty normalized label")
        if key in normalized_labels:
            raise ValueError(
                f"{path}: row {row_number}: label {label!r} normalizes to the "
                f"same key as topic {normalized_labels[key]!r}"
            )
        labels[topic_id] = label
        normalized_labels[key] = topic_id
    return labels


def load_topic_rankings(path: Path) -> dict[str, tuple[TopicRanking, ...]]:
    """Read browser tuples shaped as ``[topic_id, [[ref, score, rank], ...]]``.

    The score and rank fields are validated as metadata but never interpreted as
    refs. Likewise, topic ids and labels are not guessed to be verse refs.
    """

    rankings: dict[str, tuple[TopicRanking, ...]] = {}
    for row_number, row in enumerate(
        load_json_array(path, "topic verse rankings"), 1
    ):
        if not isinstance(row, list) or len(row) != 2:
            raise ValueError(
                f"{path}: row {row_number}: expected [topic_id, ranking_rows]"
            )
        topic_id, raw_rows = row
        if not isinstance(topic_id, str) or not topic_id.strip():
            raise ValueError(f"{path}: row {row_number}: invalid topic id")
        topic_id = topic_id.strip()
        if topic_id in rankings:
            raise ValueError(f"{path}: row {row_number}: duplicate topic {topic_id!r}")
        if not isinstance(raw_rows, list):
            raise ValueError(
                f"{path}: row {row_number}: ranking rows must be an array"
            )

        parsed: list[TopicRanking] = []
        seen_refs: set[str] = set()
        for ranking_number, raw_ranking in enumerate(raw_rows, 1):
            if not isinstance(raw_ranking, list) or len(raw_ranking) != 3:
                raise ValueError(
                    f"{path}: row {row_number} ranking {ranking_number}: "
                    "expected [canonical_ref, raw_score, rank]"
                )
            canonical_ref, raw_score, rank = raw_ranking
            if not isinstance(canonical_ref, str) or not canonical_ref.strip():
                raise ValueError(
                    f"{path}: row {row_number} ranking {ranking_number}: invalid ref"
                )
            canonical_ref = canonical_ref.strip()
            if canonical_ref in seen_refs:
                raise ValueError(
                    f"{path}: row {row_number}: duplicate ranked ref "
                    f"{canonical_ref!r}"
                )
            seen_refs.add(canonical_ref)
            if (
                isinstance(raw_score, bool)
                or not isinstance(raw_score, (int, float))
                or not math.isfinite(raw_score)
            ):
                raise ValueError(
                    f"{path}: row {row_number} ranking {ranking_number}: "
                    "raw score must be finite"
                )
            if isinstance(rank, bool) or not isinstance(rank, int) or rank < 1:
                raise ValueError(
                    f"{path}: row {row_number} ranking {ranking_number}: "
                    "rank must be a positive integer"
                )
            parsed.append(TopicRanking(canonical_ref, float(raw_score), rank))

        rankings[topic_id] = tuple(
            sorted(
                parsed,
                key=lambda item: (item.rank, -item.raw_score, item.canonical_ref),
            )
        )
    return rankings


def canonical_osis_index(
    verses: tuple[CanonicalVerse, ...],
) -> dict[tuple[str, int, int], int]:
    index: dict[tuple[str, int, int], int] = {}
    for verse_index, verse in enumerate(verses):
        if verse.bcv_key is None:
            raise ValueError(
                f"canonical verse {verse.source_ref!r} has no book/chapter/verse "
                "fields required for topic alignment"
            )
        book, raw_chapter, raw_verse = verse.bcv_key
        osis_book = OSIS_BY_BOOK_KEY.get(book)
        if osis_book is None:
            raise ValueError(
                f"canonical verse {verse.source_ref!r} has unsupported book {book!r}"
            )
        try:
            chapter = int(raw_chapter)
            verse_number = int(raw_verse)
        except ValueError as exc:
            raise ValueError(
                f"canonical verse {verse.source_ref!r} has non-numeric coordinates"
            ) from exc
        key = (osis_book, chapter, verse_number)
        if key in index:
            raise ValueError(f"canonical topic coordinate occurs twice: {key}")
        index[key] = verse_index
    return index


def parse_osis_range(
    value: str,
) -> tuple[tuple[str, int, int], tuple[str, int, int]]:
    full = FULL_OSIS_RANGE.fullmatch(value)
    if full:
        start = (
            full.group("book"),
            int(full.group("chapter")),
            int(full.group("verse")),
        )
        end = (
            full.group("end_book"),
            int(full.group("end_chapter")),
            int(full.group("end_verse")),
        )
    else:
        simple = SIMPLE_OSIS_REF.fullmatch(value)
        if not simple:
            raise ValueError(f"invalid topic canonical ref {value!r}")
        start = (
            simple.group("book"),
            int(simple.group("chapter")),
            int(simple.group("verse")),
        )
        end = (
            simple.group("book"),
            int(simple.group("chapter")),
            int(simple.group("end_verse") or simple.group("verse")),
        )

    known_codes = set(OSIS_BY_BOOK_KEY.values())
    if start[0] not in known_codes or end[0] not in known_codes:
        raise ValueError(f"unsupported OSIS book in topic canonical ref {value!r}")
    if start[0] != end[0]:
        raise ValueError(f"cross-book topic canonical ref is unsupported: {value!r}")
    if (start[1], start[2]) > (end[1], end[2]):
        raise ValueError(f"reversed topic canonical ref {value!r}")
    return start, end


def topic_range_record(
    verses: tuple[CanonicalVerse, ...],
    osis_index: dict[tuple[str, int, int], int],
    canonical_ref: str,
) -> dict | None:
    start, end = parse_osis_range(canonical_ref)
    start_index = osis_index.get(start)
    end_index = osis_index.get(end)
    if start_index is None or end_index is None:
        return None
    if start_index > end_index:
        raise ValueError(
            f"topic canonical ref is out of canonical order: {canonical_ref!r}"
        )
    return range_record(verses, start_index, end_index)


def optional_leading_the_alias(
    label: str,
    occupied_keys: set[str],
) -> str | None:
    key = normalized_concept_key(label)
    words = key.split()
    if len(words) < 2 or words[0] in {"a", "an", "the"}:
        return None
    alias = f"the {label}"
    if normalized_concept_key(alias) in occupied_keys:
        return None
    return alias


def build_topic_records(
    verses: tuple[CanonicalVerse, ...],
    event_records: tuple[dict, ...],
    sources: TopicSources,
) -> TopicBuild:
    labels = load_topic_labels(sources.canonical_topics_path)
    rankings = load_topic_rankings(sources.verse_rankings_path)
    osis_index = canonical_osis_index(verses)

    topic_keys = {normalized_concept_key(label) for label in labels.values()}
    event_keys = {record["key"] for record in event_records}
    occupied_alias_keys = topic_keys | event_keys
    event_overlaps: list[str] = []
    missing_rankings: list[str] = []
    missing_targets: list[str] = []
    without_valid_target: list[str] = []
    records: list[dict] = []
    generated_alias_count = 0

    for topic_id, label in sorted(
        labels.items(),
        key=lambda item: (normalized_concept_key(item[1]), item[0]),
    ):
        key = normalized_concept_key(label)
        if key in event_keys:
            event_overlaps.append(topic_id)
        ranked_passages = rankings.get(topic_id)
        if ranked_passages is None:
            missing_rankings.append(topic_id)
            continue

        ranges: list[dict] = []
        for ranking in ranked_passages:
            target = topic_range_record(
                verses, osis_index, ranking.canonical_ref
            )
            if target is None:
                missing_targets.append(f"{topic_id}:{ranking.canonical_ref}")
                continue
            ranges.append(target)
        if not ranges:
            without_valid_target.append(topic_id)
            continue

        alias = optional_leading_the_alias(label, occupied_alias_keys)
        aliases = [alias] if alias is not None else []
        generated_alias_count += len(aliases)
        records.append(
            {
                "schema_version": SCHEMA_VERSION,
                "kind": "concept",
                "concept_type": "topic",
                "key": key,
                "label": label,
                "aliases": aliases,
                "source": "Selah.topic-verse-rankings.browser",
                "anchor_ref": ranges[0]["start_ref"],
                "target_ranges": ranges,
            }
        )

    unknown_rankings = sorted(set(rankings) - set(labels))
    return TopicBuild(
        records=tuple(records),
        input_topic_count=len(labels),
        ranking_topic_count=len(rankings),
        ranking_row_count=sum(len(rows) for rows in rankings.values()),
        generated_alias_count=generated_alias_count,
        event_overlap_topic_ids=tuple(sorted(event_overlaps)),
        missing_ranking_topic_ids=tuple(sorted(missing_rankings)),
        unknown_ranking_topic_ids=tuple(unknown_rankings),
        missing_canonical_targets=tuple(sorted(missing_targets)),
        topics_without_valid_target_ids=tuple(sorted(without_valid_target)),
    )


def topic_manifest(build: TopicBuild) -> dict:
    skipped_topic_count = build.input_topic_count - len(build.records)
    return {
        "concept_type": "topic",
        "source": "Selah.canonical-topics.browser+topic-verse-rankings.browser",
        "input_topic_count": build.input_topic_count,
        "ranking_topic_count": build.ranking_topic_count,
        "ranking_row_count": build.ranking_row_count,
        "record_count": len(build.records),
        "skipped_topic_count": skipped_topic_count,
        "generated_alias_count": build.generated_alias_count,
        "event_overlap_topic_count": len(build.event_overlap_topic_ids),
        "event_overlap_topic_id_sample": list(
            build.event_overlap_topic_ids[:SAMPLE_LIMIT]
        ),
        "missing_ranking_topic_count": len(build.missing_ranking_topic_ids),
        "missing_ranking_topic_id_sample": list(
            build.missing_ranking_topic_ids[:SAMPLE_LIMIT]
        ),
        "unknown_ranking_topic_count": len(build.unknown_ranking_topic_ids),
        "unknown_ranking_topic_id_sample": list(
            build.unknown_ranking_topic_ids[:SAMPLE_LIMIT]
        ),
        "missing_canonical_target_count": len(build.missing_canonical_targets),
        "missing_canonical_target_sample": list(
            build.missing_canonical_targets[:SAMPLE_LIMIT]
        ),
        "topic_without_valid_target_count": len(
            build.topics_without_valid_target_ids
        ),
        "topic_without_valid_target_id_sample": list(
            build.topics_without_valid_target_ids[:SAMPLE_LIMIT]
        ),
    }


def align_alternate(
    source: AlternateSource, canonical: tuple[CanonicalVerse, ...]
) -> AlternateAlignment:
    by_source_ref = {verse.source_ref: verse for verse in canonical}
    by_bcv = {verse.bcv_key: verse for verse in canonical if verse.bcv_key is not None}
    seen_source_refs: set[str] = set()
    matched_canonical_refs: set[str] = set()
    unmatched: list[str] = []
    records_by_canonical_ref: dict[str, dict] = {}
    input_count = 0

    for line_no, row in iter_jsonl(source.path):
        input_count += 1
        source_ref = value_as_string(row.get("ref")) or display_ref(row)
        text = value_as_string(row.get("text"))
        if not source_ref:
            raise ValueError(f"{source.path}:{line_no}: missing ref")
        if source_ref in seen_source_refs:
            raise ValueError(f"{source.path}:{line_no}: duplicate ref {source_ref!r}")
        seen_source_refs.add(source_ref)
        if not text:
            raise ValueError(f"{source.path}:{line_no}: missing text")

        row_bcv = bcv_key(row)
        verse = by_source_ref.get(source_ref)
        if (
            verse is not None
            and row_bcv is not None
            and verse.bcv_key is not None
            and row_bcv != verse.bcv_key
        ):
            raise ValueError(
                f"{source.path}:{line_no}: book/chapter/verse {row_bcv!r} "
                f"conflicts with ref {source_ref!r}"
            )
        if verse is None:
            verse = by_bcv.get(row_bcv) if row_bcv is not None else None
        if verse is None:
            unmatched.append(source_ref)
            continue
        if verse.source_ref in matched_canonical_refs:
            raise ValueError(
                f"{source.path}:{line_no}: multiple rows map to {verse.source_ref!r}"
            )
        matched_canonical_refs.add(verse.source_ref)
        records_by_canonical_ref[verse.source_ref] = {
            "schema_version": SCHEMA_VERSION,
            "kind": "alternate_text",
            "source_translation": source.translation,
            "source_ref": source_ref,
            "target_ref": verse.display_ref,
            "text": text,
        }

    missing = [
        verse.source_ref
        for verse in canonical
        if verse.source_ref not in matched_canonical_refs
    ]
    records = tuple(
        records_by_canonical_ref[verse.source_ref]
        for verse in canonical
        if verse.source_ref in records_by_canonical_ref
    )
    return AlternateAlignment(
        translation=source.translation,
        input_verse_count=input_count,
        records=records,
        unmatched_source_refs=tuple(sorted(unmatched)),
        missing_canonical_refs=tuple(missing),
    )


def alignment_manifest(alignment: AlternateAlignment, canonical_count: int) -> dict:
    matched = len(alignment.records)
    return {
        "translation": alignment.translation,
        "input_verse_count": alignment.input_verse_count,
        "matched_verse_count": matched,
        "coverage_ppm": matched * 1_000_000 // canonical_count,
        "unmatched_source_verse_count": len(alignment.unmatched_source_refs),
        "unmatched_source_ref_sample": list(
            alignment.unmatched_source_refs[:SAMPLE_LIMIT]
        ),
        "missing_canonical_verse_count": len(alignment.missing_canonical_refs),
        "missing_canonical_ref_sample": list(
            alignment.missing_canonical_refs[:SAMPLE_LIMIT]
        ),
    }


def _study_bible_connection(path: Path) -> sqlite3.Connection:
    """Open the large source DB read-only without requiring sqlite-vec."""

    connection = sqlite3.connect(f"{path.resolve().as_uri()}?mode=ro", uri=True)
    connection.execute("PRAGMA query_only=ON")
    # sqlite3 cannot parse sqlite-vec virtual-table declarations when the
    # extension is absent. This read-only compatibility pragma permits normal
    # graph tables to be queried; it does not alter the database.
    connection.execute("PRAGMA writable_schema=ON")
    required = {
        "graph_people",
        "graph_places",
        "graph_events",
        "graph_verse_mentions",
    }
    tables = {
        row[0]
        for row in connection.execute(
            "SELECT name FROM sqlite_master WHERE type IN ('table','view')"
        )
    }
    missing = sorted(required - tables)
    if missing:
        connection.close()
        raise ValueError(
            f"{path}: missing study-bible graph tables: {', '.join(missing)}"
        )
    return connection


def _theographic_index(
    raw_ref: str,
    canonical_by_bcv: dict[tuple[str, str, str], int],
) -> int | None:
    match = THEOGRAPHIC_REF.fullmatch(raw_ref.strip())
    if match is None:
        return None
    book = THEOGRAPHIC_BOOK_KEYS.get(match.group("book"))
    if book is None:
        return None
    return canonical_by_bcv.get((book, match.group("chapter"), match.group("verse")))


def _index_ranges(
    verses: tuple[CanonicalVerse, ...], indexes: Iterable[int]
) -> list[tuple[int, int]]:
    ordered = sorted(set(indexes))
    if not ordered:
        return []
    ranges: list[tuple[int, int]] = []
    start = previous = ordered[0]
    for index in ordered[1:]:
        if index != previous + 1 or not canonically_adjacent(
            verses[previous], verses[index]
        ):
            ranges.append((start, previous))
            start = index
        previous = index
    ranges.append((start, previous))
    return ranges


def _study_aliases(raw: str | None, label: str) -> list[str]:
    if not raw:
        return []
    aliases = {
        value.strip()
        for value in raw.split(",")
        if value.strip() and normalized_concept_key(value) != normalized_concept_key(label)
    }
    return sorted(aliases, key=lambda value: (value.casefold(), value))


def _study_entities(
    rows: Iterable[tuple], entity_type: str
) -> dict[str, tuple[str, str | None]]:
    entities: dict[str, tuple[str, str | None]] = {}
    for row_number, row in enumerate(rows, 1):
        entity_id, label, raw_aliases = row
        if not isinstance(entity_id, str) or not entity_id.strip():
            raise ValueError(
                f"study-bible {entity_type} row {row_number} has invalid id"
            )
        if not isinstance(label, str) or not label.strip():
            raise ValueError(
                f"study-bible {entity_type} {entity_id!r} has invalid label"
            )
        if raw_aliases is not None and not isinstance(raw_aliases, str):
            raise ValueError(
                f"study-bible {entity_type} {entity_id!r} has invalid aliases"
            )
        if entity_id in entities:
            raise ValueError(
                f"study-bible {entity_type} repeats id {entity_id!r}"
            )
        entities[entity_id] = (label.strip(), raw_aliases)
    return entities


def build_study_bible_records(
    path: Path,
    verses: tuple[CanonicalVerse, ...],
) -> StudyBibleBuild:
    """Import comprehensive Theographic graph evidence from study_bible.db.

    People, places, and events remain score-zero shadow evidence. This first
    integration deliberately imports the entire event graph without treating
    an upstream timeline label as a reviewed direct answer. Every missing or
    malformed verse reference is counted in the manifest so coverage
    regressions cannot hide.
    """

    canonical_by_bcv = {
        verse.bcv_key: index
        for index, verse in enumerate(verses)
        if verse.bcv_key is not None
    }
    if not canonical_by_bcv:
        raise ValueError("canonical source has no book/chapter/verse coordinates")
    connection = _study_bible_connection(path)
    try:
        entities: dict[str, dict[str, tuple[str, str | None]]] = {
            "person": _study_entities(
                connection.execute(
                    "SELECT id, name, also_called FROM graph_people ORDER BY id"
                ),
                "person",
            ),
            "place": _study_entities(
                (
                    (entity_id, name, None)
                    for entity_id, name in connection.execute(
                        "SELECT id, name FROM graph_places ORDER BY id"
                    )
                ),
                "place",
            ),
            "event": _study_entities(
                (
                    (entity_id, title, None)
                    for entity_id, title in connection.execute(
                        "SELECT id, title FROM graph_events ORDER BY id"
                    )
                ),
                "event",
            ),
        }
        mentions: dict[tuple[str, str], list[int]] = {}
        invalid_ref_count = 0
        unknown_entity_count = 0
        invalid_ref_samples: list[str] = []
        for raw_ref, entity_type, entity_id in connection.execute(
            "SELECT verse_ref, entity_type, entity_id "
            "FROM graph_verse_mentions ORDER BY entity_type, entity_id, verse_ref"
        ):
            if entity_type not in entities or entity_id not in entities[entity_type]:
                unknown_entity_count += 1
                continue
            if not isinstance(raw_ref, str):
                raise ValueError(
                    "study-bible graph mention for "
                    f"{entity_type}:{entity_id} has invalid verse_ref"
                )
            index = _theographic_index(raw_ref, canonical_by_bcv)
            if index is None:
                invalid_ref_count += 1
                if len(invalid_ref_samples) < SAMPLE_LIMIT:
                    invalid_ref_samples.append(raw_ref)
                continue
            mentions.setdefault((entity_type, entity_id), []).append(index)
    finally:
        connection.close()

    records_by_type: dict[str, list[dict]] = {
        "person": [],
        "place": [],
        "event": [],
        "event_shadow": [],
    }
    entities_without_targets: dict[str, int] = {key: 0 for key in entities}
    source = "Theographic Bible Metadata via studybible-mcp"
    for entity_type in ("person", "place", "event"):
        for entity_id, (label, raw_aliases) in sorted(entities[entity_type].items()):
            indexes = mentions.get((entity_type, entity_id), [])
            ranges = _index_ranges(verses, indexes)
            if not ranges:
                entities_without_targets[entity_type] += 1
                continue
            key = normalized_concept_key(label)
            if not key:
                entities_without_targets[entity_type] += 1
                continue

            concept_type = "event_shadow" if entity_type == "event" else entity_type
            selected_ranges = ranges

            target_ranges = [
                range_record(verses, start, end) for start, end in selected_ranges
            ]
            anchor_index = selected_ranges[0][0]
            records_by_type[concept_type].append(
                {
                    "schema_version": SCHEMA_VERSION,
                    "kind": "concept",
                    "concept_type": concept_type,
                    "key": key,
                    "label": label,
                    "aliases": _study_aliases(raw_aliases, label),
                    "source": source,
                    "source_id": entity_id,
                    "anchor_ref": verses[anchor_index].display_ref,
                    "target_ranges": target_ranges,
                }
            )

    records: list[dict] = []
    manifests: list[dict] = []
    for concept_type in ("person", "place", "event", "event_shadow"):
        typed = sorted(
            records_by_type[concept_type],
            key=lambda row: (row["key"], row["source_id"]),
        )
        if not typed:
            continue
        records.extend(typed)
        input_type = "event" if concept_type == "event_shadow" else concept_type
        manifest = {
            "concept_type": concept_type,
            "record_count": len(typed),
            "alias_count": sum(len(row["aliases"]) for row in typed),
            "source": source,
            "source_license": "CC BY-SA 4.0",
            "source_url": "https://github.com/robertrouse/theographic-bible-metadata",
        }
        if concept_type in {"person", "place"}:
            manifest["input_entity_count"] = len(entities[input_type])
            manifest["entity_without_canonical_target_count"] = (
                entities_without_targets[input_type]
            )
        if concept_type == "event_shadow":
            manifest["input_event_count"] = len(entities["event"])
            manifest["invalid_or_missing_canonical_ref_count"] = invalid_ref_count
            manifest["invalid_or_missing_canonical_ref_sample"] = invalid_ref_samples
            manifest["unknown_entity_mention_count"] = unknown_entity_count
            manifest["event_without_canonical_target_count"] = (
                entities_without_targets["event"]
            )
        manifests.append(manifest)
    return StudyBibleBuild(tuple(records), tuple(manifests))


def json_line(record: dict) -> str:
    return json.dumps(
        record, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ) + "\n"


def write_overlay(
    *,
    canonical_path: Path,
    output_path: Path,
    alternates: Iterable[AlternateSource],
    canonical_translation: str = "BSB",
    include_events: bool = True,
    min_coverage: float = 0.99,
    topic_sources: TopicSources | None = None,
    curated_concepts_path: Path | None = None,
    study_bible_db_path: Path | None = None,
    reviewed_corrections_path: Path | None = None,
) -> dict:
    if not 0.0 <= min_coverage <= 1.0:
        raise ValueError("min_coverage must be between 0 and 1")
    if curated_concepts_path is not None:
        raise ValueError(
            "--curated-concepts was retired: provenance-free direct mappings "
            "cannot be compiled; use comprehensive source datasets or the "
            "reviewed correction pipeline"
        )
    sources = sorted(
        alternates,
        key=lambda item: (item.translation.casefold(), item.translation),
    )
    output_identity = output_path.resolve()
    input_paths = [canonical_path, *(source.path for source in sources)]
    if topic_sources is not None:
        input_paths.extend(
            [
                topic_sources.canonical_topics_path,
                topic_sources.verse_rankings_path,
            ]
        )
    if study_bible_db_path is not None:
        input_paths.append(study_bible_db_path)
    if reviewed_corrections_path is not None:
        input_paths.append(reviewed_corrections_path)
    if any(path.resolve() == output_identity for path in input_paths):
        raise ValueError("output path must not replace an input source")

    canonical = load_canonical(canonical_path)
    event_build = (
        build_event_records_with_diagnostics(canonical)
        if include_events
        else EventBuild((), 0, ())
    )
    event_records = event_build.records
    topic_build = (
        build_topic_records(canonical, event_records, topic_sources)
        if topic_sources is not None
        else None
    )
    study_bible_build = (
        build_study_bible_records(study_bible_db_path, canonical)
        if study_bible_db_path is not None
        else None
    )

    folded_names: set[str] = set()
    alignments: list[AlternateAlignment] = []
    for source in sources:
        if not source.translation.strip():
            raise ValueError("alternate translation name must not be empty")
        folded = source.translation.casefold()
        if folded in folded_names:
            raise ValueError(f"duplicate alternate translation {source.translation!r}")
        folded_names.add(folded)
        alignment = align_alternate(source, canonical)
        coverage = len(alignment.records) / len(canonical)
        if coverage < min_coverage:
            raise ValueError(
                f"{source.translation} coverage {coverage:.4%} is below "
                f"required {min_coverage:.4%}"
            )
        alignments.append(alignment)

    correction_build: CorrectionBuild | None = None
    if reviewed_corrections_path is not None:
        if study_bible_db_path is None:
            raise ValueError(
                "--reviewed-corrections requires --study-bible-db so every "
                "approved record can be verified against its source row"
            )
        correction_build = compile_reviewed_corrections(
            reviewed_path=reviewed_corrections_path,
            database_path=study_bible_db_path,
            canonical_refs=[verse.display_ref for verse in canonical],
            canonical_texts=[verse.text for verse in canonical],
            alternate_texts=(
                record["text"]
                for alignment in alignments
                for record in alignment.records
            ),
        )

    manifest = {
        "schema_version": SCHEMA_VERSION,
        "kind": "manifest",
        "canonical_translation": canonical_translation,
        "canonical_verse_count": len(canonical),
        "concept_sources": (
            [
                {
                    "concept_type": "event",
                    "input_event_count": event_build.discovered_event_count,
                    "record_count": len(event_records),
                    "alias_count": sum(
                        len(record["aliases"]) for record in event_records
                    ),
                    "source_alias_count": event_build.source_alias_count,
                    "generated_signature_count": (
                        event_build.generated_signature_count
                    ),
                    "signature_normalizer_version": (
                        EVENT_SIGNATURE_NORMALIZER_VERSION
                    ),
                    "full_signature_count": event_build.full_signature_count,
                    "partial_signature_count": (
                        event_build.partial_signature_count
                    ),
                    "single_token_signature_count": (
                        event_build.single_token_signature_count
                    ),
                    "event_with_signature_count": (
                        event_build.event_with_signature_count
                    ),
                    "event_without_signature_count": (
                        event_build.event_without_signature_count
                    ),
                    "suppressed_unsafe_event_count": (
                        event_build.suppressed_unsafe_event_count
                    ),
                    "suppressed_unsafe_candidate_count": (
                        event_build.suppressed_unsafe_candidate_count
                    ),
                    "suppressed_collision_candidate_count": (
                        event_build.suppressed_collision_candidate_count
                    ),
                    "suppressed_single_without_primary_evidence_count": (
                        event_build.suppressed_single_without_primary_evidence_count
                    ),
                    "suppressed_single_common_corpus_count": (
                        event_build.suppressed_single_common_corpus_count
                    ),
                    "max_distinctive_single_verse_frequency": (
                        MAX_DISTINCTIVE_SINGLE_VERSE_FREQUENCY
                    ),
                    "corpus_candidate_count": event_build.corpus_candidate_count,
                    "corpus_collision_signature_count": (
                        event_build.corpus_collision_signature_count
                    ),
                    "corpus_collision_candidate_count": (
                        event_build.corpus_collision_candidate_count
                    ),
                    "collision_sample": list(event_build.collision_samples),
                    "quarantined_event_count": len(
                        event_build.quarantined_events
                    ),
                    "quarantined_event_sample": list(
                        event_build.quarantined_events[:SAMPLE_LIMIT]
                    ),
                    "source": "BSB.events",
                }
            ]
            if include_events
            else []
        )
        + ([topic_manifest(topic_build)] if topic_build is not None else [])
        + (
            list(study_bible_build.manifests)
            if study_bible_build is not None
            else []
        )
        + ([correction_build.manifest()] if correction_build is not None else []),
        "alternate_sources": [
            alignment_manifest(alignment, len(canonical))
            for alignment in alignments
        ],
    }

    output_path.parent.mkdir(parents=True, exist_ok=True)
    # Validate everything before replacing an existing artifact.
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
            stream.write(json_line(manifest))
            for record in event_records:
                stream.write(json_line(record))
            if topic_build is not None:
                for record in topic_build.records:
                    stream.write(json_line(record))
            if study_bible_build is not None:
                for record in study_bible_build.records:
                    stream.write(json_line(record))
            if correction_build is not None:
                for record in correction_build.records:
                    stream.write(json_line(record))
            for alignment in alignments:
                for record in alignment.records:
                    stream.write(json_line(record))
        except Exception:
            temporary_path.unlink(missing_ok=True)
            raise
    temporary_path.replace(output_path)
    return manifest


def parse_alternate(value: str) -> AlternateSource:
    translation, separator, raw_path = value.partition("=")
    translation = translation.strip()
    raw_path = raw_path.strip()
    if not separator or not translation or not raw_path:
        raise argparse.ArgumentTypeError("expected TRANSLATION=/path/to/source.jsonl")
    return AlternateSource(translation, Path(raw_path))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--canonical", required=True, type=Path)
    parser.add_argument("--output", "-o", required=True, type=Path)
    parser.add_argument(
        "--alternate",
        action="append",
        default=[],
        type=parse_alternate,
        metavar="TRANSLATION=PATH",
        help="repeatable alternate translation source",
    )
    parser.add_argument("--canonical-translation", default="BSB")
    parser.add_argument("--no-events", action="store_true")
    parser.add_argument(
        "--curated-concepts",
        type=Path,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--canonical-topics",
        type=Path,
        help="Selah canonical-topics.browser.json tuple artifact",
    )
    parser.add_argument(
        "--topic-verse-rankings",
        type=Path,
        help="Selah topic-verse-rankings.browser.json tuple artifact",
    )
    parser.add_argument(
        "--study-bible-db",
        type=Path,
        help=(
            "optional study_bible.db; imports Theographic people, places, "
            "and events as provenance-marked shadow evidence"
        ),
    )
    parser.add_argument(
        "--reviewed-corrections",
        type=Path,
        help=(
            "approved source-backed correction JSONL; requires "
            "--study-bible-db and rejects claims present in canonical/alternate text"
        ),
    )
    parser.add_argument(
        "--min-coverage",
        type=float,
        default=0.99,
        help="minimum canonical-ref coverage required per alternate (default: 0.99)",
    )
    args = parser.parse_args()

    if (args.canonical_topics is None) != (args.topic_verse_rankings is None):
        parser.error(
            "--canonical-topics and --topic-verse-rankings must be provided together"
        )
    topic_sources = (
        TopicSources(args.canonical_topics, args.topic_verse_rankings)
        if args.canonical_topics is not None
        else None
    )

    try:
        manifest = write_overlay(
            canonical_path=args.canonical,
            output_path=args.output,
            alternates=args.alternate,
            canonical_translation=args.canonical_translation,
            include_events=not args.no_events,
            min_coverage=args.min_coverage,
            topic_sources=topic_sources,
            curated_concepts_path=args.curated_concepts,
            study_bible_db_path=args.study_bible_db,
            reviewed_corrections_path=args.reviewed_corrections,
        )
    except (OSError, ValueError, sqlite3.Error) as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    alternate_count = sum(
        source["matched_verse_count"] for source in manifest["alternate_sources"]
    )
    concept_count = sum(
        source["record_count"] for source in manifest["concept_sources"]
    )
    print(
        f"wrote {concept_count} concepts + {alternate_count} alternate texts "
        f"→ {args.output}",
        file=sys.stderr,
    )
    for source in manifest["alternate_sources"]:
        print(
            f"{source['translation']}: {source['matched_verse_count']}/"
            f"{manifest['canonical_verse_count']} canonical refs; "
            f"{source['unmatched_source_verse_count']} source-only",
            file=sys.stderr,
        )
    for source in manifest["concept_sources"]:
        if source["concept_type"] == "event":
            print(
                f"events: {source['record_count']}/{source['input_event_count']} "
                f"records; {source['generated_signature_count']} signatures; "
                f"{source['corpus_collision_signature_count']} corpus collisions; "
                f"{source['quarantined_event_count']} quarantined",
                file=sys.stderr,
            )
        elif source["concept_type"] == "topic":
            print(
                f"topics: {source['record_count']}/{source['input_topic_count']} "
                f"records; {source['missing_canonical_target_count']} missing "
                "canonical targets; "
                f"{source['event_overlap_topic_count']} event overlaps",
                file=sys.stderr,
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

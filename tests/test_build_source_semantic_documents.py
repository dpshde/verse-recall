import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from scripts.build_source_semantic_documents import build, sha256_file, write_outputs


class SourceSemanticDocumentBuilderTests(unittest.TestCase):
    def fixture(self, root: Path) -> tuple[Path, Path]:
        canonical = root / "canonical.jsonl"
        rows = [
            {
                "ref": f"Genesis 1:{verse}",
                "text": f"Canonical verse {verse} " + ("word " * 20).strip(),
            }
            for verse in range(1, 5)
        ]
        rows.append({"ref": "Exodus 1:1", "text": "Other canonical book."})
        canonical.write_text(
            "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8"
        )

        database = root / "study.db"
        with sqlite3.connect(database) as connection:
            connection.executescript(
                """
                CREATE TABLE hlt_study_notes (
                    id INTEGER PRIMARY KEY,
                    reference TEXT,
                    note_type TEXT,
                    title TEXT,
                    content TEXT,
                    priority INTEGER
                );
                CREATE TABLE hlt_verses (
                    id INTEGER PRIMARY KEY,
                    reference TEXT,
                    hlt_text TEXT,
                    hlt_text_annotated TEXT,
                    base_text_source TEXT,
                    variant_used INTEGER,
                    confidence TEXT,
                    status TEXT
                );
                CREATE TABLE theology_themes (
                    id INTEGER PRIMARY KEY,
                    theme_key TEXT,
                    theme_label TEXT,
                    description TEXT,
                    parent_theme TEXT
                );
                CREATE TABLE theology_content (
                    id INTEGER PRIMARY KEY,
                    source_work TEXT,
                    source_author TEXT,
                    source_type TEXT,
                    chapter_or_episode TEXT,
                    title TEXT,
                    content_summary TEXT
                );
                CREATE TABLE theology_theme_index (
                    id INTEGER PRIMARY KEY,
                    theme_key TEXT,
                    content_id INTEGER,
                    reference TEXT
                );
                CREATE TABLE theology_verse_index (
                    id INTEGER PRIMARY KEY,
                    content_id INTEGER,
                    reference TEXT,
                    relevance TEXT
                );
                CREATE TABLE thematic_references (
                    id INTEGER PRIMARY KEY,
                    theme TEXT,
                    reference TEXT,
                    note TEXT
                );
                CREATE TABLE graph_people (
                    id TEXT PRIMARY KEY,
                    name TEXT,
                    also_called TEXT,
                    description TEXT
                );
                CREATE TABLE graph_places (
                    id TEXT PRIMARY KEY,
                    name TEXT,
                    feature_type TEXT
                );
                CREATE TABLE graph_events (
                    id TEXT PRIMARY KEY,
                    title TEXT,
                    start_year INTEGER,
                    duration TEXT
                );
                CREATE TABLE graph_verse_mentions (
                    verse_ref TEXT,
                    entity_type TEXT,
                    entity_id TEXT
                );
                CREATE TABLE graph_person_event_edges (
                    person_id TEXT,
                    event_id TEXT
                );
                CREATE TABLE graph_event_place_edges (
                    event_id TEXT,
                    place_id TEXT
                );
                """
            )
            connection.executemany(
                "INSERT INTO hlt_study_notes VALUES (?, ?, ?, ?, ?, ?)",
                [
                    (
                        1,
                        "Gen.1.2",
                        "study",
                        "A bounded **title**",
                        "Authored explanation " + "semantic " * 30,
                        3,
                    ),
                    (2, "Unknown.1.1", "study", "Invalid", "Excluded", 3),
                ],
            )
            connection.executemany(
                "INSERT INTO hlt_verses VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                [
                    (
                        1,
                        "Gen.1.1",
                        "Distinct plain HLT wording.",
                        "ANNOTATED_MARKUP_MUST_NOT_APPEAR",
                        "MT",
                        None,
                        "standard",
                        "draft",
                    ),
                    (
                        2,
                        "Unknown.1.1",
                        "Excluded alternate text.",
                        "excluded",
                        "MT",
                        None,
                        "standard",
                        "draft",
                    ),
                    (
                        3,
                        "Gen.1.2(1.3)",
                        "Alternate wording with a versification annotation.",
                        "ANNOTATED_SUFFIX_ROW",
                        "MT",
                        None,
                        "standard",
                        "draft",
                    ),
                    (
                        4,
                        "Gen.1.2",
                        "Second source fragment for the same canonical coordinate.",
                        "ANNOTATED_PLAIN_ROW",
                        "MT",
                        None,
                        "standard",
                        "draft",
                    ),
                ],
            )
            connection.execute(
                "INSERT INTO theology_themes VALUES (?, ?, ?, ?, ?)",
                (1, "theme_one", "Theme One", "A source-wide theme description.", None),
            )
            connection.execute(
                "INSERT INTO theology_content VALUES (?, ?, ?, ?, ?, ?, ?)",
                (
                    1,
                    "Source Work",
                    "Source Author",
                    "book",
                    "Chapter One",
                    "Content Title",
                    "An independently authored content summary.",
                ),
            )
            # Duplicate join evidence must become one theme/range document.
            connection.executemany(
                "INSERT INTO theology_theme_index VALUES (?, ?, ?, ?)",
                [(1, "theme_one", 1, None), (2, "theme_one", 1, None)],
            )
            connection.execute(
                "INSERT INTO theology_verse_index VALUES (?, ?, ?, ?)",
                (1, 1, "Gen.1.3", "primary"),
            )
            connection.execute(
                "INSERT INTO thematic_references VALUES (?, ?, ?, ?)",
                (1, "creation_theme", "Genesis 1:1-2", "A declared range note."),
            )
            connection.executemany(
                "INSERT INTO graph_people VALUES (?, ?, ?, ?)",
                [
                    (
                        "person_1",
                        "Example Person",
                        "Alias Two, Alias One",
                        "A [source description](/link) for this person.",
                    ),
                    ("person_2", "Unmentioned Person", None, None),
                ],
            )
            connection.execute(
                "INSERT INTO graph_places VALUES (?, ?, ?)",
                ("place_1", "Example Place", "river"),
            )
            connection.execute(
                "INSERT INTO graph_events VALUES (?, ?, ?, ?)",
                ("event_1", "Example event", -1, "one day"),
            )
            connection.execute(
                "INSERT INTO graph_person_event_edges VALUES (?, ?)",
                ("person_1", "event_1"),
            )
            connection.execute(
                "INSERT INTO graph_event_place_edges VALUES (?, ?)",
                ("event_1", "place_1"),
            )
            mentions = [
                (f"Gen.1.{verse}", "person", "person_1")
                for verse in range(1, 5)
            ]
            mentions.extend(
                [
                    ("Gen.1.3", "event", "event_1"),
                    ("Gen.1.3", "place", "place_1"),
                    ("Gen.1.1", "person", "person_1"),  # exact duplicate
                    ("Gen.1.4", "person", "missing_person"),
                    ("Unknown.1.1", "event", "event_1"),
                ]
            )
            connection.executemany(
                "INSERT INTO graph_verse_mentions VALUES (?, ?, ?)", mentions
            )
        return canonical, database

    def test_builds_typed_range_bound_source_documents_with_accounting(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            canonical, database = self.fixture(Path(directory))
            documents, manifest = build(
                canonical,
                database,
                max_source_chars=128,
                max_canonical_chars=128,
            )
            by_kind = {}
            for document in documents:
                by_kind.setdefault(document["source_kind"], []).append(document)

            self.assertEqual(
                set(by_kind),
                {
                    "hlt-verse",
                    "hlt-study-note",
                    "theology-theme",
                    "theology-content",
                    "thematic-reference",
                    "graph-person",
                    "graph-place",
                    "graph-event",
                },
            )
            self.assertEqual(len(by_kind["theology-theme"]), 1)
            self.assertEqual(len(by_kind["graph-person"]), 1)
            self.assertEqual(
                (by_kind["graph-person"][0]["start_ref"], by_kind["graph-person"][0]["end_ref"]),
                ("Genesis 1:1", "Genesis 1:4"),
            )
            self.assertIn("Example Person", by_kind["graph-event"][0]["text"])
            self.assertIn("Example Place", by_kind["graph-event"][0]["text"])
            self.assertNotIn("/link", by_kind["graph-person"][0]["text"])
            self.assertNotIn(
                "ANNOTATED_MARKUP_MUST_NOT_APPEAR", by_kind["hlt-verse"][0]["text"]
            )
            self.assertFalse(
                by_kind["hlt-verse"][0]["metadata"]["annotated_markup_included"]
            )
            suffix_row = next(
                row
                for row in by_kind["hlt-verse"]
                if row["metadata"]["versification_annotations"]
            )
            self.assertEqual(suffix_row["start_ref"], "Genesis 1:2")
            self.assertEqual(suffix_row["metadata"]["versification_annotations"], ["(1.3)"])
            self.assertEqual(suffix_row["metadata"]["source_fragment_count"], 2)
            self.assertEqual(len(suffix_row["source_record_ids"]), 2)
            self.assertEqual(
                manifest["sources"]["hlt_verses"]["coalesced_fragment_count"], 1
            )
            self.assertTrue(by_kind["hlt-study-note"][0]["text_truncated"])
            self.assertTrue(by_kind["graph-person"][0]["canonical_text_truncated"])
            for document in documents:
                self.assertLessEqual(len(document["text"]), 128)
                self.assertLessEqual(len(document["canonical_text"]), 128)
                self.assertLessEqual(document["start_index"], document["end_index"])
                self.assertTrue(document["source_record_ids"])

            self.assertEqual(
                manifest["sources"]["hlt_verses"]["exclusions"]["unknown_book"], 1
            )
            self.assertEqual(
                manifest["sources"]["hlt_study_notes"]["exclusions"]["unknown_book"],
                1,
            )
            self.assertEqual(
                manifest["sources"]["theology_themes"]["duplicate_binding_count"],
                1,
            )
            graph = manifest["sources"]["graph"]
            self.assertEqual(graph["mention_exclusions"]["duplicate_mention"], 1)
            self.assertEqual(graph["mention_exclusions"]["unknown_entity"], 1)
            self.assertEqual(graph["mention_exclusions"]["unknown_book"], 1)

            repeated_documents, repeated_manifest = build(
                canonical,
                database,
                max_source_chars=128,
                max_canonical_chars=128,
            )
            self.assertEqual(documents, repeated_documents)
            self.assertEqual(manifest, repeated_manifest)

    def test_graph_ranges_are_split_at_a_uniform_maximum(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            canonical, database = self.fixture(Path(directory))
            documents, _ = build(
                canonical,
                database,
                max_range_verses=2,
                max_source_chars=128,
                max_canonical_chars=128,
            )
            person_ranges = [
                (row["start_ref"], row["end_ref"])
                for row in documents
                if row["source_kind"] == "graph-person"
            ]
            self.assertEqual(
                person_ranges,
                [
                    ("Genesis 1:1", "Genesis 1:2"),
                    ("Genesis 1:3", "Genesis 1:4"),
                ],
            )

    def test_output_manifest_binds_exact_deterministic_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical, database = self.fixture(root)
            documents, manifest = build(canonical, database)
            output = root / "source-documents.jsonl"
            manifest_path = write_outputs(documents, manifest, output)
            bound = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(bound["output"]["sha256"], sha256_file(output))
            self.assertEqual(bound["output"]["bytes"], output.stat().st_size)
            parsed = [json.loads(line) for line in output.read_text().splitlines()]
            self.assertEqual(parsed, documents)

    def test_missing_tables_and_invalid_limits_fail_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            canonical.write_text(
                json.dumps({"ref": "Genesis 1:1", "text": "One."}) + "\n",
                encoding="utf-8",
            )
            database = root / "empty.db"
            with sqlite3.connect(database):
                pass
            with self.assertRaisesRegex(ValueError, "missing required tables"):
                build(canonical, database)
            with self.assertRaisesRegex(ValueError, "max_range_verses"):
                build(canonical, database, max_range_verses=0)
            with self.assertRaisesRegex(ValueError, "max_source_chars"):
                build(canonical, database, max_source_chars=64)


if __name__ == "__main__":
    unittest.main()

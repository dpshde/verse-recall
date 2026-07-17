import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

from scripts.build_semantic_documents import build


class BuildSemanticDocumentsTests(unittest.TestCase):
    def test_enriches_with_paragraph_alternate_and_trusted_labels(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            canonical.write_text(
                "\n".join(
                    json.dumps({"ref": f"Matthew 1:{verse}", "text": f"Verse {verse}"})
                    for verse in range(1, 4)
                )
                + "\n",
                encoding="utf-8",
            )
            overlay = root / "overlay.jsonl"
            rows = [
                {"kind": "alternate_text", "source_translation": "KJV", "target_ref": "Matthew 1:2", "text": "KJV two"},
                {"kind": "concept", "concept_type": "event", "anchor_ref": "Matthew 1:2", "label": "Trusted event", "aliases": ["Event alias"]},
                {"kind": "concept", "concept_type": "topic", "anchor_ref": "Matthew 1:2", "label": "Noisy topic", "aliases": []},
            ]
            overlay.write_text("\n".join(map(json.dumps, rows)) + "\n", encoding="utf-8")
            database = root / "study.db"
            with sqlite3.connect(database) as connection:
                connection.execute("CREATE TABLE passages (id INTEGER PRIMARY KEY, reference_start TEXT, reference_end TEXT, verse_count INTEGER)")
                connection.execute("INSERT INTO passages VALUES (1, 'Matt.1.1', 'Matt.1.3', 3)")

            documents = build(canonical, database, overlay)
            enriched = documents[1]["text"]
            self.assertIn("Alternate wording: KJV two", enriched)
            self.assertIn("Named associations: Event alias; Trusted event", enriched)
            self.assertIn("Passage context: Verse 1 Verse 2 Verse 3", enriched)
            self.assertNotIn("Noisy topic", enriched)

            passage_only = build(
                canonical, database, overlay, document_view="canonical-passage"
            )[1]["text"]
            self.assertEqual(
                passage_only, "Verse 2 Passage context: Verse 1 Verse 2 Verse 3"
            )
            self.assertNotIn("KJV two", passage_only)
            self.assertNotIn("Trusted event", passage_only)

    def test_long_passage_falls_back_to_local_same_chapter_context(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            canonical.write_text(
                "\n".join(
                    json.dumps({"ref": f"Genesis 1:{verse}", "text": f"Verse {verse}"})
                    for verse in range(1, 15)
                )
                + "\n",
                encoding="utf-8",
            )
            overlay = root / "overlay.jsonl"
            overlay.write_text("", encoding="utf-8")
            database = root / "study.db"
            with sqlite3.connect(database) as connection:
                connection.execute("CREATE TABLE passages (id INTEGER PRIMARY KEY, reference_start TEXT, reference_end TEXT, verse_count INTEGER)")
                connection.execute("INSERT INTO passages VALUES (1, 'Gen.1.1', 'Gen.1.14', 14)")

            documents = build(canonical, database, overlay)
            self.assertIn("Passage context: Verse 5 Verse 6 Verse 7 Verse 8 Verse 9", documents[6]["text"])
            self.assertNotIn("Verse 1 Verse 2 Verse 3 Verse 4", documents[6]["text"])

    def test_study_notes_are_source_driven_specific_and_opt_in(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            canonical.write_text(
                "\n".join(
                    json.dumps(
                        {"ref": f"Genesis 1:{verse}", "text": f"Verse {verse}"}
                    )
                    for verse in range(1, 4)
                )
                + "\n",
                encoding="utf-8",
            )
            overlay = root / "overlay.jsonl"
            overlay.write_text("", encoding="utf-8")
            database = root / "study.db"
            with sqlite3.connect(database) as connection:
                connection.execute(
                    "CREATE TABLE passages (id INTEGER PRIMARY KEY, reference_start TEXT, reference_end TEXT, verse_count INTEGER)"
                )
                connection.execute(
                    "INSERT INTO passages VALUES (1, 'Gen.1.1', 'Gen.1.3', 3)"
                )
                connection.execute(
                    "CREATE TABLE aquifer_content (id INTEGER PRIMARY KEY, resource_type TEXT, title TEXT, start_ref TEXT, end_ref TEXT, content_plain TEXT)"
                )
                connection.execute(
                    "INSERT INTO aquifer_content VALUES (1, 'study_notes', 'Range', '01001001', '01001003', 'broad explanation')"
                )
                connection.execute(
                    "INSERT INTO aquifer_content VALUES (2, 'study_notes', 'Specific', '01001002', '01001002', 'specific explanation')"
                )
                connection.execute(
                    "INSERT INTO aquifer_content VALUES (3, 'translation_notes_uw', 'Wrong source', '01001002', '01001002', 'not included')"
                )
                connection.execute(
                    "INSERT INTO aquifer_content VALUES (4, 'study_notes', 'Range', '01001001', '01001003', 'broad explanation')"
                )
                connection.execute(
                    "INSERT INTO aquifer_content VALUES (5, 'study_notes', 'Additional', '01001001', '01001003', 'third unique explanation')"
                )

            plain = build(canonical, database, overlay)
            self.assertNotIn("Study explanation:", plain[1]["text"])
            self.assertEqual(plain[1]["study_context"], "")
            enriched = build(canonical, database, overlay, include_study_notes=True)
            self.assertIn("Specific — specific explanation", enriched[1]["text"])
            self.assertIn("Range — broad explanation", enriched[1]["text"])
            self.assertNotIn("Wrong source", enriched[1]["text"])
            self.assertIn("Specific — specific explanation", enriched[1]["study_context"])
            self.assertIn("Range — broad explanation", enriched[1]["study_context"])
            self.assertIn(
                "Additional — third unique explanation",
                enriched[1]["study_context"],
            )
            self.assertNotIn("Additional — third unique explanation", enriched[1]["text"])
            self.assertEqual(
                enriched[1]["study_context"].count("Range — broad explanation"),
                1,
            )
            self.assertNotIn("Passage context:", enriched[1]["study_context"])


if __name__ == "__main__":
    unittest.main()

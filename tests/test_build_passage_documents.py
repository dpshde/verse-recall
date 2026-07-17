import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from scripts.build_passage_documents import build, sha256_file, write_outputs


class PassageDocumentBuilderTests(unittest.TestCase):
    def fixture(self, root: Path) -> tuple[Path, Path]:
        canonical = root / "canonical.jsonl"
        rows = [
            {"ref": "Genesis 1:1", "text": "First verse."},
            {"ref": "Genesis 1:2", "text": "Second verse."},
            {"ref": "Genesis 1:3", "text": "Third verse."},
            {"ref": "Exodus 1:1", "text": "Other book."},
        ]
        canonical.write_text(
            "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8"
        )
        database = root / "study.db"
        with sqlite3.connect(database) as connection:
            connection.execute(
                """
                CREATE TABLE passages (
                    id INTEGER PRIMARY KEY,
                    reference_start TEXT NOT NULL,
                    reference_end TEXT NOT NULL,
                    verse_count INTEGER NOT NULL,
                    section_type TEXT
                )
                """
            )
            connection.executemany(
                "INSERT INTO passages VALUES (?, ?, ?, ?, ?)",
                [
                    (1, "Gen.1.1", "Gen.1.2", 2, "paragraph"),
                    (2, "Gen.1.2", "Gen.1.3", 2, "paragraph"),
                    (3, "Gen.1.1", "Gen.1.3", 3, "paragraph"),
                    (4, "Gen.1.1", "Exo.1.1", 4, "paragraph"),
                    (5, "Unknown.1.1", "Unknown.1.1", 1, None),
                    (6, "Gen.1.1", "Gen.1.2", 2, "setumah"),
                    (7, "Gen.1.1", "Gen.1.3", 2, "paragraph"),
                ],
            )
        return canonical, database

    def test_build_is_range_bound_deterministic_and_fully_accounted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical, database = self.fixture(root)
            documents, manifest = build(canonical, database, max_verses=2)
            self.assertEqual(
                [(row["start_ref"], row["end_ref"]) for row in documents],
                [("Genesis 1:1", "Genesis 1:2"), ("Genesis 1:2", "Genesis 1:3")],
            )
            self.assertEqual(documents[0]["text"], "First verse. Second verse.")
            self.assertEqual(manifest["included_passage_count"], 2)
            self.assertEqual(manifest["excluded_passage_count"], 5)
            self.assertEqual(manifest["exclusions"]["cross_book"], 1)
            self.assertEqual(manifest["exclusions"]["invalid_reference"], 1)
            self.assertEqual(manifest["exclusions"]["duplicate_range"], 1)
            self.assertEqual(manifest["exclusions"]["invalid_span"], 1)
            self.assertEqual(manifest["exclusions"]["over_max_verses"], 1)

            repeated, repeated_manifest = build(canonical, database, max_verses=2)
            self.assertEqual(documents, repeated)
            self.assertEqual(manifest, repeated_manifest)

    def test_output_manifest_binds_exact_corpus_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical, database = self.fixture(root)
            documents, manifest = build(canonical, database, max_verses=2)
            output = root / "passages.jsonl"
            manifest_path = write_outputs(documents, manifest, output)
            bound = json.loads(manifest_path.read_text(encoding="utf-8"))
            self.assertEqual(bound["output"]["sha256"], sha256_file(output))
            self.assertEqual(bound["output"]["bytes"], output.stat().st_size)

    def test_max_verses_is_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical, database = self.fixture(root)
            with self.assertRaisesRegex(ValueError, "max_verses"):
                build(canonical, database, max_verses=0)


if __name__ == "__main__":
    unittest.main()

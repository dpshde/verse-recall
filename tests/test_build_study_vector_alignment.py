import json
import sqlite3
import tempfile
import unittest
from pathlib import Path

import numpy as np

from scripts.build_study_vector_alignment import (
    effective_source_coordinate,
    extract,
    fit_rectangular_procrustes,
    read_canonical,
    stable_fold,
    transformed_queries,
)


class BuildStudyVectorAlignmentTests(unittest.TestCase):
    def write_canonical(self, path: Path) -> None:
        rows = [
            {"ref": "Genesis 1:1", "text": "first"},
            {"ref": "Genesis 1:2", "text": "second"},
            {"ref": "Exodus 1:1", "text": "third"},
        ]
        path.write_text(
            "".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8"
        )

    def build_shadow_database(self, path: Path) -> None:
        connection = sqlite3.connect(path)
        connection.executescript(
            """
            CREATE TABLE verses (
                id INTEGER PRIMARY KEY,
                reference TEXT NOT NULL,
                book TEXT,
                chapter INTEGER,
                verse INTEGER
            );
            CREATE TABLE verse_vectors_info (key TEXT PRIMARY KEY, value ANY);
            CREATE TABLE verse_vectors_chunks (
                chunk_id INTEGER PRIMARY KEY,
                size INTEGER NOT NULL,
                validity BLOB NOT NULL,
                rowids BLOB NOT NULL
            );
            CREATE TABLE verse_vectors_vector_chunks00 (
                rowid INTEGER PRIMARY KEY,
                vectors BLOB NOT NULL
            );
            CREATE TABLE verse_vectors_rowids (
                rowid INTEGER PRIMARY KEY,
                id,
                chunk_id INTEGER,
                chunk_offset INTEGER
            );
            """
        )
        source_rows = [
            (8, "Gen.1.1", "Gen", 1, 1),
            # Brackets, unlike parentheses, identify the English coordinate.
            (10, "Gen.1.1[1.2]", "Gen", 1, 1),
            (12, "Gen.1.2", "Gen", 1, 2),
            (14, "Exo.1.1(1.1)", "Exo", 1, 1),
            (16, "metadata", "metadata", 0, 0),
        ]
        connection.executemany(
            "INSERT INTO verses(id,reference,book,chapter,verse) VALUES(?,?,?,?,?)",
            source_rows,
        )
        connection.execute(
            "INSERT INTO verse_vectors_info(key,value) VALUES('CREATE_VERSION','test')"
        )
        size, dims = 8, 3
        validity = bytes([0b00011111])
        rowids = np.zeros(size, dtype="<i8")
        rowids[:5] = [8, 10, 12, 14, 16]
        vectors = np.zeros((size, dims), dtype="<f4")
        vectors[:5] = [
            [1.0, 0.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 3.0],
            [4.0, 4.0, 0.0],
            [1.0, 1.0, 1.0],
        ]
        connection.execute(
            "INSERT INTO verse_vectors_chunks VALUES(?,?,?,?)",
            (1, size, validity, rowids.tobytes()),
        )
        connection.execute(
            "INSERT INTO verse_vectors_vector_chunks00 VALUES(?,?)",
            (1, vectors.tobytes()),
        )
        connection.executemany(
            "INSERT INTO verse_vectors_rowids(rowid,id,chunk_id,chunk_offset) "
            "VALUES(?,NULL,1,?)",
            [(int(source_id), offset) for offset, source_id in enumerate(rowids[:5])],
        )
        connection.commit()
        connection.close()

    def test_effective_coordinate_uses_english_split_but_not_parentheses(self) -> None:
        self.assertEqual(
            effective_source_coordinate("2Co.13.13[13.14]", "2Co", 13, 13),
            ("2Co", 13, 14),
        )
        self.assertEqual(
            effective_source_coordinate("Gen.31.55(32.1)", "Gen", 31, 55),
            ("Gen", 31, 55),
        )

    def test_extracts_shadow_vectors_and_aggregates_split_fragments(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            database = root / "study.db"
            output = root / "study.f32"
            manifest = root / "manifest.json"
            self.write_canonical(canonical)
            self.build_shadow_database(database)

            result = extract(database, canonical, output, manifest, dims=3)
            values = np.fromfile(output, dtype="<f4").reshape(3, 3)
            np.testing.assert_allclose(values[0], [1.0, 0.0, 0.0], atol=1e-6)
            np.testing.assert_allclose(
                values[1], [0.0, 2**-0.5, 2**-0.5], atol=1e-6
            )
            np.testing.assert_allclose(
                values[2], [2**-0.5, 2**-0.5, 0.0], atol=1e-6
            )
            self.assertEqual(result["source_active_vectors"], 5)
            self.assertEqual(result["source_mapped_vectors"], 4)
            self.assertEqual(result["aggregated_canonical_coordinates"], 1)
            self.assertEqual(result["reference_audit"]["coverage"], 1.0)
            self.assertEqual(result["reference_audit"]["missing_canonical_references"], [])
            self.assertIn("metadata", result["reference_audit"]["excluded_source_references"])

            second = root / "study-second.f32"
            second_manifest = root / "manifest-second.json"
            again = extract(database, canonical, second, second_manifest, dims=3)
            self.assertEqual(result["output_sha256"], again["output_sha256"])

    def test_expected_database_hash_is_enforced(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            database = root / "study.db"
            self.write_canonical(canonical)
            self.build_shadow_database(database)
            with self.assertRaisesRegex(ValueError, "expected"):
                extract(
                    database,
                    canonical,
                    root / "study.f32",
                    root / "manifest.json",
                    dims=3,
                    expected_database_sha256="00" * 32,
                )

    def test_book_fold_and_rectangular_procrustes_are_deterministic(self) -> None:
        self.assertEqual(stable_fold("Genesis", 4), stable_fold("Genesis", 4))
        source = np.asarray(
            [[1, 0], [0, 1], [-1, 0], [0, -1], [1, 1], [-1, 1]],
            dtype=np.float32,
        )
        rotation = np.asarray(
            [[1, 0, 0], [0, 1, 0]],
            dtype=np.float32,
        )
        target = source @ rotation
        indices = np.arange(len(source), dtype=np.int64)
        fitted, _ = fit_rectangular_procrustes(source, target, indices)
        transformed = transformed_queries(source, fitted)
        expected = target / np.linalg.norm(target, axis=1, keepdims=True)
        np.testing.assert_allclose(transformed, expected, atol=1e-6)
        np.testing.assert_allclose(fitted @ fitted.T, np.eye(2), atol=1e-6)

    def test_canonical_reader_rejects_duplicate_references(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "canonical.jsonl"
            path.write_text(
                '{"ref":"Genesis 1:1"}\n{"ref":"Genesis 1:1"}\n',
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "duplicate"):
                read_canonical(path)


if __name__ == "__main__":
    unittest.main()

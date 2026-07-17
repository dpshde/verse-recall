from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

import numpy as np


SCRIPT = Path(__file__).parents[1] / "scripts" / "fit_semantic_query_adapter.py"
SPEC = importlib.util.spec_from_file_location("fit_semantic_query_adapter", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
adapter = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = adapter
SPEC.loader.exec_module(adapter)


class FitSemanticQueryAdapterTest(unittest.TestCase):
    def test_procrustes_recovers_a_query_to_document_rotation(self) -> None:
        source = np.asarray(
            [[1.0, 0.0], [0.0, 1.0], [-1.0, 0.0], [0.0, -1.0]],
            dtype=np.float32,
        )
        rotation = np.asarray([[0.0, 1.0], [-1.0, 0.0]], dtype=np.float32)
        target = source @ rotation
        fitted = adapter.fit_procrustes(source, target)
        np.testing.assert_allclose(fitted, rotation, atol=1e-6)

        metrics = adapter.exact_retrieval_metrics(
            source,
            target,
            np.arange(4, dtype=np.int64),
            fitted,
            batch_size=2,
        )
        self.assertEqual(metrics["hit_at_1"], 1.0)
        self.assertEqual(metrics["hit_at_10"], 1.0)

    def test_source_identical_note_ranges_are_multi_positive(self) -> None:
        rows = [
            {"ref": "Book A 1:1", "view": "study-note", "text": "range note"},
            {"ref": "Book A 1:2", "view": "study-note", "text": "range note"},
            {"ref": "Book A 1:3", "view": "study-note", "text": "other note"},
        ]
        groups = adapter.equivalent_gold_indices(rows, np.ones(3, dtype=bool))
        np.testing.assert_array_equal(groups[0], np.asarray([0, 1]))
        np.testing.assert_array_equal(groups[1], np.asarray([0, 1]))
        np.testing.assert_array_equal(groups[2], np.asarray([2]))

        # The first query aligns best to the second verse in the same exact
        # source-note range, which must still count as rank one.
        source = np.asarray([[1.0, 0.0]], dtype=np.float32)
        corpus = np.asarray(
            [[0.8, 0.2], [1.0, 0.0], [0.0, 1.0]], dtype=np.float32
        )
        metrics = adapter.exact_retrieval_metrics(
            source,
            corpus,
            np.asarray([0]),
            np.eye(2, dtype=np.float32),
            batch_size=1,
            gold_groups=groups,
        )
        self.assertEqual(metrics["hit_at_1"], 1.0)

    def test_book_holdout_and_sampling_are_content_independent(self) -> None:
        rows = [
            {"ref": f"Book {letter} 1:1", "view": "study-note"}
            for letter in "ABCDEFGHI"
        ]
        # The helper treats every portion before the final coordinate as the
        # book, so synthetic multi-word names exercise numbered/compound books.
        heldout = adapter.heldout_books(rows, stride=3)
        self.assertEqual(heldout, ["Book A", "Book D", "Book G"])
        eligible = np.ones(len(rows), dtype=bool)
        first = adapter.stable_validation_indices(
            rows, eligible, set(heldout), limit=2
        )
        second = adapter.stable_validation_indices(
            rows, eligible, set(heldout), limit=2
        )
        np.testing.assert_array_equal(first, second)
        self.assertTrue(
            all(adapter.reference_book(rows[int(i)]["ref"]) in heldout for i in first)
        )

    def test_ridge_rejects_invalid_regularization(self) -> None:
        matrix = np.eye(3, dtype=np.float32)
        with self.assertRaisesRegex(ValueError, "must be positive"):
            adapter.fit_ridge(matrix, matrix, 0.0)

    def test_evaluation_tree_is_forbidden_as_training_input(self) -> None:
        with self.assertRaisesRegex(ValueError, "evaluation artifacts are forbidden"):
            adapter.reject_evaluation_input(
                Path(__file__).parents[1] / "eval" / "semantic-quality-v2.jsonl"
            )


if __name__ == "__main__":
    unittest.main()

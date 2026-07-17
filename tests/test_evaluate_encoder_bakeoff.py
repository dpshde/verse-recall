import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from scripts.evaluate_encoder_bakeoff import (
    QUERY_SEGMENT_PLANNER,
    REVEALED_SUITES,
    RevealedSuite,
    aggregate_system,
    interpreter_path,
    validate_corpus_vectors,
    validate_revealed_suite,
)


def write_suite(path: Path, split: str = "regression-test") -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(
            {
                "id": "one",
                "prompt": "remembered wording",
                "gold_refs": ["Genesis 1:1"],
                "family": "genesis-1-1",
                "category": "meaning-only",
                "split": split,
            }
        )
        + "\n",
        encoding="utf-8",
    )
    return hashlib.sha256(path.read_bytes()).hexdigest()


def system(name: str, hit_3: int, hit_10: int) -> dict:
    overall = {
        "queries": 2,
        "retrieval_queries": 1,
        "no_gold_queries": 1,
        "hits_at_1": 0,
        "hits_at_3": hit_3,
        "hits_at_10": hit_10,
        "hits_at_50": 1,
        "hits_at_100": 1,
        "recall_at_1": 0.0,
        "recall_at_3": float(hit_3),
        "recall_at_10": float(hit_10),
        "recall_at_50": 1.0,
        "recall_at_100": 1.0,
        "mrr": 0.5,
        "safe_no_gold": 1,
        "no_gold_safety": 1.0,
        "safety_queries": 2,
        "safe_confidence_queries": 2,
        "confidence_safety": 1.0,
        "passage_queries": 0,
        "passage_passes": 0,
        "passage_accuracy": 0.0,
        "hard_negative_queries": 1,
        "hard_negative_passes": 1,
        "hard_negative_accuracy": 1.0,
        "confidence_label_queries": 1,
        "confidence_label_passes": 1,
        "confidence_label_accuracy": 1.0,
    }
    slice_metrics = dict(overall, queries=1, no_gold_queries=0)
    return {
        "name": name,
        "overall": overall,
        "by_family": {"family": slice_metrics},
        "by_category": {"meaning-only": slice_metrics},
    }


class EvaluateEncoderBakeoffTests(unittest.TestCase):
    def test_bakeoff_uses_ordered_v2_query_planner_contract(self) -> None:
        self.assertEqual(QUERY_SEGMENT_PLANNER, "verse-recall-clause-v2")

    def test_allowlist_names_no_unrevealed_holdouts(self) -> None:
        self.assertEqual(len(REVEALED_SUITES), 8)
        for suite in REVEALED_SUITES:
            self.assertNotIn("holdout-v3", suite.relative_path)
            self.assertNotIn("holdout-v4", suite.relative_path)
            self.assertNotIn("holdout-v5", suite.relative_path)
            self.assertNotIn("holdout-v6", suite.relative_path)

    def test_suite_requires_pinned_hash_and_regression_split(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "eval/revealed.jsonl"
            digest = write_suite(path)
            suite = RevealedSuite("safe", "Safe", "eval/revealed.jsonl", digest)
            self.assertEqual(validate_revealed_suite(root, suite), (path, 1))

            holdout_digest = write_suite(path, "holdout-test")
            holdout_suite = RevealedSuite(
                "unsafe", "Unsafe", "eval/revealed.jsonl", holdout_digest
            )
            with self.assertRaisesRegex(ValueError, "only regression"):
                validate_revealed_suite(root, holdout_suite)

            mutated_suite = RevealedSuite(
                "changed", "Changed", "eval/revealed.jsonl", "0" * 64
            )
            with self.assertRaisesRegex(ValueError, "identity changed"):
                validate_revealed_suite(root, mutated_suite)

    def test_corpus_vector_shape_must_be_exact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "vectors.f32"
            path.write_bytes(bytes(3 * 2 * 4))
            validate_corpus_vectors(path, 2, 3)
            path.write_bytes(path.read_bytes() + b"\0")
            with self.assertRaisesRegex(ValueError, "shape mismatch"):
                validate_corpus_vectors(path, 2, 3)

    def test_interpreter_launcher_symlink_is_not_dereferenced(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "base-python"
            target.write_bytes(b"")
            environment = root / "environment"
            environment.mkdir()
            launcher = environment / "python"
            launcher.symlink_to(target)
            self.assertEqual(interpreter_path(launcher, root), launcher)

    def test_aggregate_reports_pooled_and_macro_metrics(self) -> None:
        first = RevealedSuite("one", "One", "one", "a" * 64)
        second = RevealedSuite("two", "Two", "two", "b" * 64)
        reports = [
            (first, {"systems": [system("verse-recall", 1, 1)]}),
            (second, {"systems": [system("verse-recall", 0, 1)]}),
        ]
        aggregate = aggregate_system(reports, "verse-recall")
        self.assertEqual(aggregate["queries"], 4)
        self.assertEqual(aggregate["retrieval_queries"], 2)
        self.assertEqual(aggregate["recall_at_3"], 0.5)
        self.assertEqual(aggregate["suite_macro_recall"][3], 0.5)
        self.assertEqual(aggregate["family_macro_recall_at_3"], 0.5)
        self.assertEqual(aggregate["confidence_safety"], 1.0)


if __name__ == "__main__":
    unittest.main()

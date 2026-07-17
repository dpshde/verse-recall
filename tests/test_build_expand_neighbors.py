from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np


SCRIPT = Path(__file__).parents[1] / "scripts" / "build_expand_neighbors.py"
SPEC = importlib.util.spec_from_file_location("build_expand_neighbors", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
expand = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = expand
SPEC.loader.exec_module(expand)


class BuildExpandNeighborsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.canonical = self.root / "canonical.jsonl"
        self.canonical.write_text(
            "\n".join(
                [
                    json.dumps({"ref": "Test 1:1", "text": "Trust and trusted armor."}),
                    json.dumps({"ref": "Test 1:2", "text": "A shield protects."}),
                ]
            )
            + "\n",
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def test_default_vocabulary_is_sorted_ascii_corpus_vocabulary(self) -> None:
        count, vocabulary = expand.read_canonical_corpus(self.canonical)
        self.assertEqual(count, 2)
        self.assertEqual(
            vocabulary, ["a", "and", "armor", "protects", "shield", "trust", "trusted"]
        )

    def test_light_stem_matches_runtime_for_nonsemantic_normalization(self) -> None:
        fixture_path = SCRIPT.parents[1] / "tests" / "normalizer-v2-fixtures.json"
        fixtures = json.loads(fixture_path.read_text(encoding="utf-8"))
        self.assertEqual(expand.NORMALIZER_VERSION, 2)
        for surface, expected in fixtures:
            with self.subTest(surface=surface):
                self.assertEqual(expand.light_stem(surface), expected)

    def test_build_is_deterministic_deduped_filtered_and_packer_shaped(self) -> None:
        vocabulary = ["trust", "trusted", "rely", "armor", "shield", "the"]
        vocab_path = self.root / "vocabulary.json"
        vocab_path.write_text(json.dumps(vocabulary), encoding="utf-8")
        # trust/trusted share one runtime stem; "the" is deliberately close
        # to everything but must never become a source or target.
        vectors = np.asarray(
            [
                [1.00, 0.00, 0.00],
                [0.99, 0.01, 0.00],
                [0.95, 0.05, 0.00],
                [0.00, 1.00, 0.00],
                [0.05, 0.95, 0.00],
                [0.70, 0.70, 0.00],
            ],
            dtype=np.float32,
        )
        matrix = self.root / "vectors.npy"
        np.save(matrix, vectors)
        first = self.root / "first.jsonl"
        second = self.root / "second.jsonl"
        kwargs = dict(
            canonical=self.canonical,
            embeddings=matrix,
            canonical_translation="BSB",
            vocabulary_path=vocab_path,
            top_k=2,
            similarity_floor=0.60,
            batch_size=1,
        )
        expand.build_expand_jsonl(output=first, **kwargs)
        expand.build_expand_jsonl(output=second, **kwargs)
        self.assertEqual(first.read_bytes(), second.read_bytes())

        records = [json.loads(line) for line in first.read_text().splitlines()]
        manifest = records[0]
        self.assertEqual(manifest["schema_version"], 1)
        self.assertEqual(manifest["kind"], "manifest")
        self.assertEqual(manifest["canonical_translation"], "BSB")
        self.assertEqual(manifest["canonical_verse_count"], 2)
        self.assertEqual(len(manifest["provenance"]["embeddings_sha256"]), 64)
        self.assertEqual(manifest["provenance"]["normalizer_version"], 2)

        mappings = {record["source"]: record["targets"] for record in records[1:]}
        self.assertNotIn("the", mappings)
        self.assertIn("rely", mappings)
        rely_targets = mappings["rely"]
        self.assertEqual([target["stem"] for target in rely_targets], ["trust"])
        for source, targets in mappings.items():
            self.assertLessEqual(len(targets), 2)
            self.assertNotIn("the", {target["stem"] for target in targets})
            self.assertNotIn(source, {target["stem"] for target in targets})
            self.assertEqual(
                len(targets), len({target["stem"] for target in targets})
            )
            self.assertTrue(all(0.0 < target["weight"] <= 1.0 for target in targets))

    def test_rejects_row_mismatch_zero_vectors_and_oversized_target_count(self) -> None:
        with self.assertRaisesRegex(ValueError, "zero-norm"):
            expand.normalize_matrix(np.asarray([[0.0, 0.0]], dtype=np.float32))

        matrix = self.root / "short.npy"
        np.save(matrix, np.ones((2, 3), dtype=np.float32))
        with self.assertRaisesRegex(ValueError, "rows do not match"):
            expand.build_expand_jsonl(
                canonical=self.canonical,
                embeddings=matrix,
                output=self.root / "unused.jsonl",
            )

        with self.assertRaisesRegex(ValueError, "top_k"):
            expand.mine_expansions(
                ["trust", "armor"],
                ["trust", "armor"],
                np.eye(2, dtype=np.float32),
                top_k=65,
            )


if __name__ == "__main__":
    unittest.main()

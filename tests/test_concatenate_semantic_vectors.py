import json
from pathlib import Path
import tempfile
import unittest

import numpy as np

from scripts.concatenate_semantic_vectors import (
    build_contract,
    canonical_space_id,
    concatenate_matrices,
    queries,
    read_contract,
    write_contract,
)


SPACE_A = "1" * 64
SPACE_B = "2" * 64


class Args:
    pass


class ConcatenateSemanticVectorsTests(unittest.TestCase):
    def contract(self) -> dict:
        return build_contract(
            [f"small={SPACE_A}=2=1", f"large={SPACE_B}=3=1"]
        )

    def test_equal_weight_dot_is_average_component_cosine(self) -> None:
        contract = self.contract()
        query = concatenate_matrices(
            [np.asarray([[1.0, 0.0]]), np.asarray([[1.0, 0.0, 0.0]])],
            contract,
        )[0]
        document = concatenate_matrices(
            [np.asarray([[0.0, 1.0]]), np.asarray([[0.6, 0.8, 0.0]])],
            contract,
        )[0]
        self.assertAlmostEqual(float(query @ document), (0.0 + 0.6) / 2, places=6)
        self.assertAlmostEqual(float(np.linalg.norm(query)), 1.0, places=6)

    def test_contract_canonicalizes_proportional_weights(self) -> None:
        one = self.contract()
        two = build_contract(
            [f"small={SPACE_A}=2=2", f"large={SPACE_B}=3=2"]
        )
        self.assertEqual(one, two)
        self.assertEqual(canonical_space_id(one), canonical_space_id(two))

    def test_query_metadata_and_segments_must_match(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            contract_path = root / "contract.json"
            write_contract(contract_path, self.contract())
            self.assertEqual(read_contract(contract_path)[0], self.contract())
            digest = "a" * 64
            first = {
                "id": "q",
                "prompt_utf8_sha256": digest,
                "embedding": [1, 0],
                "planner": "verse-recall-clause-v2",
                "segments": [{"text": "first thought", "embedding": [0, 1]}],
            }
            second = {
                "id": "q",
                "prompt_utf8_sha256": digest,
                "embedding": [1, 0, 0],
                "planner": "verse-recall-clause-v2",
                "segments": [
                    {"text": "different thought", "embedding": [0, 1, 0]}
                ],
            }
            paths = []
            for name, row in (("small", first), ("large", second)):
                path = root / f"{name}.jsonl"
                path.write_text(json.dumps(row) + "\n", encoding="utf-8")
                paths.append(f"{name}={path}")
            args = Args()
            args.contract = contract_path
            args.input = paths
            args.output = root / "output.jsonl"
            with self.assertRaisesRegex(ValueError, "text differs"):
                queries(args)

    def test_query_segments_preserve_v2_planner_order(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            contract_path = root / "contract.json"
            write_contract(contract_path, self.contract())
            digest = "a" * 64
            component_rows = (
                (
                    "small",
                    {
                        "id": "q",
                        "prompt_utf8_sha256": digest,
                        "embedding": [1, 0],
                        "planner": "verse-recall-clause-v2",
                        "segments": [
                            {"text": "first thought", "embedding": [0, 1]},
                            {"text": "second thought", "embedding": [1, 1]},
                        ],
                    },
                ),
                (
                    "large",
                    {
                        "id": "q",
                        "prompt_utf8_sha256": digest,
                        "embedding": [1, 0, 0],
                        "planner": "verse-recall-clause-v2",
                        "segments": [
                            {"text": "first thought", "embedding": [0, 1, 0]},
                            {"text": "second thought", "embedding": [1, 1, 0]},
                        ],
                    },
                ),
            )
            paths = []
            for name, row in component_rows:
                path = root / f"{name}.jsonl"
                path.write_text(json.dumps(row) + "\n", encoding="utf-8")
                paths.append(f"{name}={path}")
            args = Args()
            args.contract = contract_path
            args.input = paths
            args.output = root / "output.jsonl"

            queries(args)

            output = json.loads(args.output.read_text(encoding="utf-8"))
            self.assertEqual(output["planner"], "verse-recall-clause-v2")
            self.assertEqual(
                [segment["text"] for segment in output["segments"]],
                ["first thought", "second thought"],
            )

    def test_query_segments_reject_stale_planner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            contract_path = root / "contract.json"
            write_contract(contract_path, self.contract())
            digest = "a" * 64
            paths = []
            for name, embedding, segment_embedding in (
                ("small", [1, 0], [0, 1]),
                ("large", [1, 0, 0], [0, 1, 0]),
            ):
                path = root / f"{name}.jsonl"
                row = {
                    "id": "q",
                    "prompt_utf8_sha256": digest,
                    "embedding": embedding,
                    "planner": "verse-recall-clause-v1",
                    "segments": [
                        {"text": "first thought", "embedding": segment_embedding}
                    ],
                }
                path.write_text(json.dumps(row) + "\n", encoding="utf-8")
                paths.append(f"{name}={path}")
            args = Args()
            args.contract = contract_path
            args.input = paths
            args.output = root / "output.jsonl"

            with self.assertRaisesRegex(ValueError, "unsupported segment planner"):
                queries(args)


if __name__ == "__main__":
    unittest.main()

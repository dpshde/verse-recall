import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).parents[1] / "scripts" / "evaluate_cross_encoder_reranker.py"
SPEC = importlib.util.spec_from_file_location("evaluate_cross_encoder_reranker", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class CrossEncoderRerankerHarnessTests(unittest.TestCase):
    def test_rrf60_is_deterministic_and_balances_both_orders(self):
        import numpy as np

        cross_order = np.asarray([2, 1, 0, 3])
        first = MODULE.rrf60_order(cross_order, 4).tolist()
        second = MODULE.rrf60_order(cross_order, 4).tolist()
        self.assertEqual(first, second)
        self.assertEqual(sorted(first), [0, 1, 2, 3])
        self.assertEqual(first[0], 0)
        self.assertEqual(first[1], 2)

    def test_candidate_export_rejects_non_allowlisted_suite(self):
        allowed = {"semantic": {"one": {"prompt": "hello", "split": "regression-x"}}}
        value = {
            "schema_version": 1,
            "depth": 100,
            "rows": [
                {
                    "suite": "holdout-v6",
                    "prompt": {"id": "one", "prompt": "hello", "split": "holdout"},
                    "primary": {"results": [{"ref": f"X {index}:1"} for index in range(100)]},
                }
            ],
        }
        with self.assertRaisesRegex(ValueError, "disallowed suite"):
            MODULE.validate_candidate_export(value, allowed)

    def test_canonical_loader_requires_complete_unique_bsb(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "tiny.jsonl"
            path.write_text(json.dumps({"ref": "Genesis 1:1", "text": "In the beginning"}) + "\n")
            with self.assertRaisesRegex(ValueError, "31,086"):
                MODULE.load_canonical_text(path)


if __name__ == "__main__":
    unittest.main()

"""Tests for method-tradeoffs-v2 majority scoring (real module entry points)."""

from __future__ import annotations

import importlib.util
import json
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "evaluate_method_tradeoffs_v2.py"


def load_module():
    spec = importlib.util.spec_from_file_location("evaluate_method_tradeoffs_v2", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class MethodTradeoffsV2Test(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.mod = load_module()

    def test_majority_summary_counts_strict_wins(self) -> None:
        retained = {
            "key": "arctic_full",
            "metrics": {
                "suite_macro_r1": 0.70,
                "suite_macro_r3": 0.84,
                "suite_macro_r10": 0.91,
                "family_cell_r3": 0.80,
                "family_cell_r10": 0.89,
                "meaning_only_r3": 0.71,
                "multi_intent_r3": 0.40,
                "hard_negative_rate": 0.90,
                "confidence_safety": 1.0,
                "engine_data_bytes": 27_496_192,
            },
        }
        weak = {
            "key": "keyword",
            "metrics": {
                "suite_macro_r1": 0.40,
                "suite_macro_r3": 0.50,
                "suite_macro_r10": 0.60,
                "family_cell_r3": 0.45,
                "family_cell_r10": 0.55,
                "meaning_only_r3": 0.15,
                "multi_intent_r3": 0.10,
                "hard_negative_rate": 0.80,
                "confidence_safety": 0.0,
                "engine_data_bytes": 6_433_344,
            },
        }
        summary = self.mod.majority_summary(retained, [retained, weak])
        self.assertTrue(summary["majority"])
        self.assertGreater(summary["win_rate"], 0.5)
        self.assertEqual(summary["losses"], 0)
        # All quality metrics should be comparable and won by retained.
        quality_cells = [
            cell
            for cell in summary["cells"]
            if cell["metric"] != "engine_data_size_near_quality"
        ]
        self.assertTrue(all(cell["winner"] == "retained" for cell in quality_cells))

    def test_size_cell_only_when_r3_within_one_point(self) -> None:
        retained = {
            "key": "a",
            "metrics": {
                "suite_macro_r1": 0.7,
                "suite_macro_r3": 0.84,
                "suite_macro_r10": 0.9,
                "family_cell_r3": None,
                "family_cell_r10": None,
                "meaning_only_r3": None,
                "multi_intent_r3": None,
                "hard_negative_rate": None,
                "confidence_safety": 1.0,
                "engine_data_bytes": 20_000_000,
            },
        }
        near = {
            "key": "b",
            "metrics": {
                "suite_macro_r1": 0.69,
                "suite_macro_r3": 0.835,  # within 1 point
                "suite_macro_r10": 0.89,
                "family_cell_r3": None,
                "family_cell_r10": None,
                "meaning_only_r3": None,
                "multi_intent_r3": None,
                "hard_negative_rate": None,
                "confidence_safety": 1.0,
                "engine_data_bytes": 10_000_000,
            },
        }
        cells = self.mod.compare_cells(retained, near)
        size_cells = [c for c in cells if c["metric"] == "engine_data_size_near_quality"]
        self.assertEqual(len(size_cells), 1)
        self.assertEqual(size_cells[0]["winner"], "competitor")

        far = dict(near)
        far["metrics"] = dict(near["metrics"])
        far["metrics"]["suite_macro_r3"] = 0.70  # far from retained
        cells_far = self.mod.compare_cells(retained, far)
        self.assertFalse(
            any(c["metric"] == "engine_data_size_near_quality" for c in cells_far)
        )

    def test_suite_macro_averages_suites_equally(self) -> None:
        systems = [
            {"overall": {"recall_at_3": 1.0}},
            {"overall": {"recall_at_3": 0.0}},
        ]
        self.assertAlmostEqual(self.mod.suite_macro_from_systems(systems, 3), 0.5)

    def test_report_schema_when_present(self) -> None:
        path = ROOT / "eval" / "method-tradeoffs-v2.json"
        if not path.is_file():
            self.skipTest("method-tradeoffs-v2.json not generated yet")
        study = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(study["schema"], "verse-recall-method-tradeoffs-v2")
        self.assertIn("majority", study)
        self.assertTrue(study["majority"]["majority"])
        self.assertGreater(study["majority"]["win_rate"], 0.5)
        retained = next(m for m in study["methods"] if m["retained"])
        self.assertEqual(retained["key"], study["majority"]["retained_key"])
        self.assertIn(retained["key"], {"arctic_passage", "arctic_full"})
        # Safety floor where measured
        safety = retained["metrics"]["confidence_safety"]
        if safety is not None:
            self.assertAlmostEqual(safety, 1.0)
        # Suite-macro R@3 within 2 points of documented Arctic baseline 84.20%
        r3 = retained["metrics"]["suite_macro_r3"]
        self.assertGreaterEqual(r3, 0.8420 - 0.02)
        # Majority is strict wins over comparable cells only
        self.assertGreater(study["majority"]["comparable_cells"], 0)
        self.assertEqual(
            study["majority"]["wins"] + study["majority"]["losses"],
            study["majority"]["comparable_cells"],
        )


if __name__ == "__main__":
    unittest.main()

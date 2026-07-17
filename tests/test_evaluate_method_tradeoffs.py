import unittest

from scripts.evaluate_method_tradeoffs import (
    QUERY_GROUPS,
    diversity_metrics,
    get_system,
    render_markdown,
)


def row(category: str, hit_at_3: bool, hit_at_10: bool) -> dict:
    return {
        "id": category,
        "category": category,
        "has_gold": True,
        "hit_at_1": False,
        "hit_at_3": hit_at_3,
        "hit_at_10": hit_at_10,
        "reciprocal_rank": 0.1 if hit_at_10 else 0,
        "no_gold_safe": None,
        "confidence_safe": True,
        "passage_output_ok": None,
        "hard_negative_ok": None,
        "confidence_label_ok": None,
    }


class EvaluateMethodTradeoffsTests(unittest.TestCase):
    def test_diversity_metrics_equal_weight_categories(self) -> None:
        rows = [
            row(next(iter(categories)), index % 2 == 0, index < 5)
            for index, (_key, _label, categories) in enumerate(QUERY_GROUPS)
        ]
        result = diversity_metrics(rows)
        self.assertAlmostEqual(result["positive_category_macro_recall_at_3"], 0.5)
        self.assertAlmostEqual(
            result["positive_category_macro_recall_at_10"], 5 / 6
        )
        self.assertEqual(result["weakest_positive_category_recall_at_3"], 0)
        self.assertEqual(result["categories_at_or_above_80pct_recall_at_3"], 3)

    def test_get_system_fails_closed(self) -> None:
        report = {"systems": [{"name": "one"}]}
        self.assertEqual(get_system(report, "one")["name"], "one")
        with self.assertRaisesRegex(ValueError, "does not contain"):
            get_system(report, "missing")

    def test_keyword_confidence_is_rendered_as_not_applicable(self) -> None:
        rows = [
            row(next(iter(categories)), True, True)
            for _key, _label, categories in QUERY_GROUPS
        ]
        overall = {
            "recall_at_1": 0.25,
            "recall_at_3": 0.5,
            "recall_at_10": 0.75,
            "safety_queries": 0,
            "confidence_safety": 0,
            "hard_negative_accuracy": 1,
        }
        method = {
            "key": "full_fusion",
            "label": "Synthetic",
            "artifact_bytes": 1,
            "query_embeddings": False,
            "strengths": "strength",
            "weaknesses": "weakness",
            "overall": overall,
            "diversity": diversity_metrics(rows),
            "by_suite": {"synthetic": overall},
        }
        no_dense = dict(method, key="hybrid_no_dense")
        study = {
            "methods": [method, no_dense],
            "overall_queries": 6,
            "overall_retrieval_queries": 6,
            "suites": [
                {
                    "key": "synthetic",
                    "label": "Synthetic suite",
                    "prompts": {"sha256": "a" * 64},
                    "query_embeddings": {"sha256": "b" * 64},
                }
            ],
            "embedding_space_id": "c" * 64,
        }
        self.assertIn("| n/a |", render_markdown(study))


if __name__ == "__main__":
    unittest.main()

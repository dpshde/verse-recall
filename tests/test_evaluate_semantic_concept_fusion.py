import unittest

from scripts.evaluate_semantic_concept_fusion import rrf_fuse


class SemanticConceptFusionTests(unittest.TestCase):
    def test_rrf_is_deterministic_and_rewards_cross_view_agreement(self) -> None:
        fused = rrf_fuse(["A", "B", "C"], ["B", "D", "A"], 1.0)
        self.assertEqual("B", fused[0])
        self.assertEqual("A", fused[1])
        self.assertEqual(fused, rrf_fuse(["A", "B", "C"], ["B", "D", "A"], 1.0))

    def test_zero_secondary_influence_is_rejected_by_cli_not_math(self) -> None:
        self.assertEqual(["A", "B", "C"], rrf_fuse(["A", "B"], ["C"], 0.0))

    def test_intersection_never_injects_a_source_only_candidate(self) -> None:
        fused = rrf_fuse(
            ["A", "B", "C"],
            ["D", "C", "A"],
            1.0,
            intersection_only=True,
        )
        self.assertEqual({"A", "B", "C"}, set(fused))
        self.assertNotIn("D", fused)


if __name__ == "__main__":
    unittest.main()

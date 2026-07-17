import unittest

import numpy as np

from scripts.evaluate_grouped_passage_fusion import grouped_ranking, paragraph_groups


class GroupedPassageFusionTests(unittest.TestCase):
    def test_paragraphs_cannot_flood_the_ranking(self) -> None:
        rows = [
            {"text": "first Passage context: shared"},
            {"text": "second Passage context: shared"},
            {"text": "independent"},
        ]
        groups = paragraph_groups(rows)
        ranking = grouped_ranking(
            np.asarray([0.9, 0.8, 0.7], dtype=np.float32),
            groups,
            ["A", "B", "C"],
            3,
        )
        self.assertEqual(["A", "C"], ranking)


if __name__ == "__main__":
    unittest.main()

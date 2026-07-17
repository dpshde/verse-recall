import json
import tempfile
import unittest
from pathlib import Path

from scripts.build_semantic_concept_corpus import build_rows


class SemanticConceptCorpusTests(unittest.TestCase):
    def test_preserves_every_source_target_without_eval_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            topics = root / "topics.json"
            rankings = root / "rankings.json"
            topics.write_text(
                json.dumps([["b", "Second concept"], ["a", "First concept"]]),
                encoding="utf-8",
            )
            rankings.write_text(
                json.dumps(
                    [
                        ["a", [["GEN.1.1", 7, 2], ["GEN.1.2", 9, 1]]],
                        ["b", [["MAT.1.1-2", 3, 1]]],
                    ]
                ),
                encoding="utf-8",
            )
            rows = build_rows(topics, rankings)
            self.assertEqual(["a", "b"], [row["id"] for row in rows])
            self.assertEqual("GEN.1.2", rows[0]["ref"])
            self.assertEqual(2, len(rows[0]["targets"]))
            self.assertEqual("Selah.canonical-topics+topic-verse-rankings", rows[0]["source"])

    def test_rejects_partial_source_sets(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            topics = root / "topics.json"
            rankings = root / "rankings.json"
            topics.write_text(json.dumps([["a", "First"]]), encoding="utf-8")
            rankings.write_text(json.dumps([]), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                build_rows(topics, rankings)


if __name__ == "__main__":
    unittest.main()

import json
import tempfile
import unittest
from pathlib import Path

from scripts.build_aligned_translation_corpus import build


class AlignedTranslationCorpusTests(unittest.TestCase):
    def test_aligns_by_reference_and_falls_back_without_shifting(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            overlay = root / "overlay.jsonl"
            canonical.write_text(
                '{"ref":"Genesis 1:1","text":"canonical one"}\n'
                '{"ref":"Genesis 1:2","text":"canonical two"}\n',
                encoding="utf-8",
            )
            overlay.write_text(
                '{"kind":"alternate_text","source_translation":"KJV",'
                '"target_ref":"Genesis 1:2","text":"alternate two"}\n',
                encoding="utf-8",
            )
            rows, manifest = build(canonical, overlay, "KJV")
            self.assertEqual(["canonical one", "alternate two"], [row["text"] for row in rows])
            self.assertEqual(1, manifest["matched_rows"])
            self.assertEqual(["Genesis 1:1"], manifest["missing_ref_sample"])


if __name__ == "__main__":
    unittest.main()

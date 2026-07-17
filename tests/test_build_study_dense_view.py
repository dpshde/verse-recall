import json
import tempfile
import unittest
from pathlib import Path

from scripts.build_study_dense_view import build


class BuildStudyDenseViewTests(unittest.TestCase):
    def write_jsonl(self, path: Path, rows: list[dict]) -> None:
        path.write_text(
            "".join(json.dumps(row) + "\n" for row in rows),
            encoding="utf-8",
        )

    def test_uses_note_view_and_exact_canonical_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            documents = root / "documents.jsonl"
            self.write_jsonl(
                canonical,
                [
                    {"ref": "Genesis 1:1", "text": "Canonical first."},
                    {"ref": "Genesis 1:2", "text": "Canonical   second."},
                ],
            )
            self.write_jsonl(
                documents,
                [
                    {
                        "ref": "Genesis 1:1",
                        "text": "ignored enriched text",
                        "study_context": "Specific note.\n Broad note.",
                    },
                    {
                        "ref": "Genesis 1:2",
                        "text": "ignored enriched text",
                        "study_context": "",
                    },
                ],
            )

            rows = build(canonical, documents)
            self.assertEqual(
                rows,
                [
                    {
                        "ref": "Genesis 1:1",
                        "text": "Specific note. Broad note.",
                        "view": "study-note",
                    },
                    {
                        "ref": "Genesis 1:2",
                        "text": "Canonical second.",
                        "view": "canonical-fallback",
                    },
                ],
            )

    def test_bounds_note_at_a_word_boundary_and_never_emits_empty_text(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            documents = root / "documents.jsonl"
            self.write_jsonl(
                canonical,
                [{"ref": "Genesis 1:1", "text": "Non-empty canonical text."}],
            )
            self.write_jsonl(
                documents,
                [
                    {
                        "ref": "Genesis 1:1",
                        "study_context": "alpha beta gamma delta",
                    }
                ],
            )

            rows = build(canonical, documents, max_chars=12)
            self.assertEqual(rows[0]["text"], "alpha beta")
            self.assertTrue(all(row["text"] for row in rows))

    def test_rejects_missing_reordered_or_duplicate_canonical_rows(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            canonical = root / "canonical.jsonl"
            documents = root / "documents.jsonl"
            self.write_jsonl(
                canonical,
                [
                    {"ref": "Genesis 1:1", "text": "one"},
                    {"ref": "Genesis 1:1", "text": "duplicate"},
                ],
            )
            self.write_jsonl(
                documents,
                [
                    {"ref": "Genesis 1:1", "study_context": "note"},
                    {"ref": "Genesis 1:2", "study_context": "note"},
                ],
            )

            with self.assertRaisesRegex(ValueError, "duplicate canonical reference"):
                build(canonical, documents)

            self.write_jsonl(
                canonical,
                [
                    {"ref": "Genesis 1:1", "text": "one"},
                    {"ref": "Genesis 1:2", "text": "two"},
                ],
            )
            self.write_jsonl(
                documents,
                [
                    {"ref": "Genesis 1:2", "study_context": "note"},
                    {"ref": "Genesis 1:1", "study_context": "note"},
                ],
            )
            with self.assertRaisesRegex(ValueError, "not canonical-aligned"):
                build(canonical, documents)


if __name__ == "__main__":
    unittest.main()

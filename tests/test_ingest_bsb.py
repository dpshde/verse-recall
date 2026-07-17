#!/usr/bin/env python3
"""Tests for scripts/ingest_bsb.py event-preserving ingest."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
INGEST = ROOT / "scripts" / "ingest_bsb.py"


class IngestBsbTests(unittest.TestCase):
    def _run(self, rows: list[dict]) -> list[dict]:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "in.jsonl"
            output = root / "out.jsonl"
            source.write_text(
                "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows),
                encoding="utf-8",
            )
            completed = subprocess.run(
                [sys.executable, str(INGEST), "--input", str(source), "--output", str(output)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            return [
                json.loads(line)
                for line in output.read_text(encoding="utf-8").splitlines()
                if line.strip()
            ]

    def test_preserves_events_and_display_refs(self) -> None:
        rows = self._run(
            [
                {
                    "ref": "Gen.3.1",
                    "book": "Genesis",
                    "chapter": "3",
                    "verseNum": "1",
                    "text": "Now the serpent was more crafty.",
                    "events": ["The Fall", ""],
                    "entities": ["serpent"],
                },
                {
                    "ref": "Gen.1.1",
                    "book": "Genesis",
                    "chapter": "1",
                    "verseNum": "1",
                    "text": "In the beginning God created the heavens and the earth.",
                },
            ]
        )
        self.assertEqual(
            rows[0],
            {
                "ref": "Genesis 3:1",
                "text": "Now the serpent was more crafty.",
                "events": ["The Fall"],
            },
        )
        self.assertEqual(
            rows[1],
            {
                "ref": "Genesis 1:1",
                "text": "In the beginning God created the heavens and the earth.",
            },
        )

    def test_rejects_malformed_events(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / "in.jsonl"
            output = root / "out.jsonl"
            source.write_text(
                json.dumps(
                    {
                        "book": "Genesis",
                        "chapter": "1",
                        "verseNum": "1",
                        "text": "In the beginning.",
                        "events": "not-a-list",
                    }
                )
                + "\n",
                encoding="utf-8",
            )
            completed = subprocess.run(
                [sys.executable, str(INGEST), "--input", str(source), "--output", str(output)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(completed.returncode, 0)
            self.assertIn("events must be a string array", completed.stderr)


if __name__ == "__main__":
    unittest.main()

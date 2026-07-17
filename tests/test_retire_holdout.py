from __future__ import annotations

from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from scripts.retire_holdout import (
    RetirementError,
    main,
    partial_path_for,
    retire_holdout,
)


class RetireHoldoutTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.source = self.root / "sealed.jsonl"
        self.destination = self.root / "regression.jsonl"
        self.report = self.root / "first-shot.md"
        self.report.write_text("# First shot\n", encoding="utf-8")
        self.report.with_suffix(".json").write_text("{}\n", encoding="utf-8")

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def write_rows(self, rows: list[dict]) -> None:
        self.source.write_text(
            "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows),
            encoding="utf-8",
        )

    def test_retires_consistent_split_without_changing_decoded_content(self) -> None:
        rows = [
            {
                "id": "synthetic-a",
                "prompt": "Peace — be still 🙏",
                "category": "synthetic",
                "gold_refs": ["Example 1:1"],
                "family": "synthetic-family-a",
                "split": "holdout-synthetic-v1",
            },
            {
                "id": "synthetic-b",
                "prompt": "A literal \\u2014 sequence",
                "category": "synthetic",
                "gold_refs": [],
                "family": "synthetic-family-b",
                "split": "holdout-synthetic-v1",
            },
        ]
        self.write_rows(rows)
        source_bytes = self.source.read_bytes()

        result = retire_holdout(self.source, self.destination, self.report)

        self.assertEqual(result.records, 2)
        self.assertEqual(result.source_split, "holdout-synthetic-v1")
        self.assertEqual(result.destination_split, "regression-synthetic-v1")
        self.assertEqual(self.source.read_bytes(), source_bytes)
        retired = [
            json.loads(line)
            for line in self.destination.read_text(encoding="utf-8").splitlines()
        ]
        for before, after in zip(rows, retired):
            expected = dict(before)
            expected["split"] = "regression-synthetic-v1"
            self.assertEqual(after, expected)
            self.assertEqual(after["prompt"], before["prompt"])
        self.assertFalse(partial_path_for(self.destination).exists())

    def test_requires_both_first_shot_reports_before_reading_source(self) -> None:
        self.write_rows(
            [{"id": "a", "prompt": "synthetic", "split": "holdout-v1"}]
        )
        self.report.with_suffix(".json").unlink()

        with mock.patch(
            "scripts.retire_holdout._transform_rows"
        ) as transform, self.assertRaisesRegex(RetirementError, "paired first-shot"):
            retire_holdout(self.source, self.destination, self.report)

        transform.assert_not_called()
        self.assertFalse(self.destination.exists())

    def test_rejects_inconsistent_missing_and_non_holdout_splits(self) -> None:
        bad_sources = (
            [
                {"id": "a", "prompt": "one", "split": "holdout-v1"},
                {"id": "b", "prompt": "two", "split": "holdout-v2"},
            ],
            [{"id": "a", "prompt": "one"}],
            [{"id": "a", "prompt": "one", "split": "regression-v1"}],
            [{"id": "a", "prompt": "one", "split": "holdout-"}],
        )
        for rows in bad_sources:
            with self.subTest(rows=len(rows), split=rows[0].get("split")):
                self.write_rows(rows)
                with self.assertRaises(RetirementError):
                    retire_holdout(self.source, self.destination, self.report)
                self.assertFalse(self.destination.exists())
                self.assertFalse(partial_path_for(self.destination).exists())

    def test_rejects_invalid_json_non_objects_and_duplicate_keys_without_leaks(
        self,
    ) -> None:
        unsafe_sources = (
            '["SECRET_PROMPT", "holdout-v1"]\n',
            '{"prompt":"SECRET_PROMPT","split":"holdout-v1",}\n',
            '{"prompt":"SECRET_PROMPT","split":"holdout-v1","split":"holdout-v1"}\n',
            '{"prompt":"SECRET_PROMPT","score":NaN,"split":"holdout-v1"}\n',
        )
        for raw in unsafe_sources:
            with self.subTest(raw=raw[:1]):
                self.source.write_text(raw, encoding="utf-8")
                with self.assertRaises(RetirementError) as raised:
                    retire_holdout(self.source, self.destination, self.report)
                self.assertNotIn("SECRET_PROMPT", str(raised.exception))
                self.assertFalse(self.destination.exists())

    def test_refuses_existing_destination_and_preexisting_partial(self) -> None:
        self.write_rows(
            [{"id": "a", "prompt": "synthetic", "split": "holdout-v1"}]
        )
        self.destination.write_text("do not replace\n", encoding="utf-8")
        with self.assertRaisesRegex(RetirementError, "destination already exists"):
            retire_holdout(self.source, self.destination, self.report)
        self.assertEqual(
            self.destination.read_text(encoding="utf-8"), "do not replace\n"
        )

        self.destination.unlink()
        partial = partial_path_for(self.destination)
        partial.write_text("interrupted\n", encoding="utf-8")
        with self.assertRaisesRegex(RetirementError, "partial retirement"):
            retire_holdout(self.source, self.destination, self.report)
        self.assertEqual(partial.read_text(encoding="utf-8"), "interrupted\n")
        self.assertFalse(self.destination.exists())

    def test_refuses_source_as_destination(self) -> None:
        self.write_rows(
            [{"id": "a", "prompt": "synthetic", "split": "holdout-v1"}]
        )
        with self.assertRaisesRegex(RetirementError, "separate from"):
            retire_holdout(self.source, self.source, self.report)

    def test_fsyncs_temporary_file_before_atomic_replace(self) -> None:
        self.write_rows(
            [{"id": "a", "prompt": "synthetic", "split": "holdout-v1"}]
        )
        events: list[str] = []
        real_fsync = os.fsync
        real_replace = os.replace

        def recording_fsync(descriptor: int) -> None:
            events.append("fsync")
            real_fsync(descriptor)

        def recording_replace(source: Path, destination: Path) -> None:
            events.append("replace")
            real_replace(source, destination)

        with mock.patch(
            "scripts.retire_holdout.os.fsync", side_effect=recording_fsync
        ), mock.patch(
            "scripts.retire_holdout.os.replace", side_effect=recording_replace
        ):
            retire_holdout(self.source, self.destination, self.report)

        self.assertGreaterEqual(len(events), 3)
        self.assertEqual(events[:2], ["fsync", "replace"])

    def test_failed_commit_removes_its_own_partial(self) -> None:
        self.write_rows(
            [{"id": "a", "prompt": "synthetic", "split": "holdout-v1"}]
        )
        with mock.patch(
            "scripts.retire_holdout.os.replace", side_effect=OSError("synthetic")
        ), self.assertRaisesRegex(RetirementError, "committed atomically"):
            retire_holdout(self.source, self.destination, self.report)
        self.assertFalse(self.destination.exists())
        self.assertFalse(partial_path_for(self.destination).exists())

    def test_cli_output_never_contains_prompt_or_gold_content(self) -> None:
        self.write_rows(
            [
                {
                    "id": "a",
                    "prompt": "SECRET_PROMPT",
                    "gold_refs": ["SECRET_GOLD"],
                    "split": "holdout-v1",
                }
            ]
        )
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = main(
                [
                    "--source",
                    str(self.source),
                    "--destination",
                    str(self.destination),
                    "--first-shot-report",
                    str(self.report),
                ]
            )
        self.assertEqual(status, 0)
        displayed = stdout.getvalue() + stderr.getvalue()
        self.assertNotIn("SECRET_PROMPT", displayed)
        self.assertNotIn("SECRET_GOLD", displayed)


if __name__ == "__main__":
    unittest.main()

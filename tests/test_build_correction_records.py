from __future__ import annotations

import importlib.util
import json
import sqlite3
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "scripts" / "build_correction_records.py"
SPEC = importlib.util.spec_from_file_location("build_correction_records", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
corrections = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = corrections
SPEC.loader.exec_module(corrections)

OVERLAY_SCRIPT = Path(__file__).parents[1] / "scripts" / "build_recall_overlay.py"
OVERLAY_SPEC = importlib.util.spec_from_file_location(
    "build_recall_overlay_with_corrections", OVERLAY_SCRIPT
)
assert OVERLAY_SPEC is not None and OVERLAY_SPEC.loader is not None
overlay = importlib.util.module_from_spec(OVERLAY_SPEC)
sys.modules[OVERLAY_SPEC.name] = overlay
OVERLAY_SPEC.loader.exec_module(overlay)


def write_jsonl(path: Path, rows: list[dict]) -> None:
    path.write_text(
        "".join(
            json.dumps(row, ensure_ascii=False, sort_keys=True) + "\n"
            for row in rows
        ),
        encoding="utf-8",
    )


class BuildCorrectionRecordsTest(unittest.TestCase):
    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.database = self.root / "study_bible.db"
        connection = sqlite3.connect(self.database)
        connection.executescript(
            """
            CREATE TABLE aquifer_content (
                id INTEGER PRIMARY KEY,
                content_id INTEGER NOT NULL,
                resource_type TEXT NOT NULL,
                title TEXT NOT NULL,
                start_ref TEXT,
                end_ref TEXT,
                content_plain TEXT NOT NULL
            );
            CREATE TABLE graph_people (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, also_called TEXT
            );
            CREATE TABLE graph_places (id TEXT PRIMARY KEY, name TEXT NOT NULL);
            CREATE TABLE graph_events (id TEXT PRIMARY KEY, title TEXT NOT NULL);
            CREATE TABLE graph_verse_mentions (
                verse_ref TEXT NOT NULL, entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL
            );
            INSERT INTO aquifer_content VALUES (
                7, 101, 'study_notes', 'Matthew 1:1', '40001001', '40001001',
                'A popular assumption adds a precise traveler count. The text does not state how many travelers arrived.'
            );
            INSERT INTO aquifer_content VALUES (
                8, 102, 'study_notes', 'Matthew 1:2', '40001002', '40001002',
                'This row contains ordinary background material without a correction claim.'
            );
            """
        )
        connection.commit()
        connection.close()

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def candidate(self) -> dict:
        _, candidates = corrections.mine_candidates(self.database)
        self.assertEqual(1, len(candidates))
        candidate = candidates[0]
        source = {
            key: value
            for key, value in candidate["source"].items()
            if key in corrections.SOURCE_FIELDS
        }
        return {
            "schema_version": 1,
            "kind": "correction",
            "id": candidate["id"],
            "claim_surfaces": [
                "invented traveler count",
                "exactly four travelers",
            ],
            "correction_label": "The source note says the text does not specify a count",
            "anchor_ref": "Matthew 1:1",
            "target_ranges": [
                {"start_ref": "Matthew 1:1", "end_ref": "Matthew 1:2"}
            ],
            "source": source,
            "review": {
                "status": "approved",
                "policy": corrections.REVIEW_POLICY,
                "reviewed_by": "editor-1",
                "reviewed_at": "2026-07-16",
                "rationale": "The note explicitly rejects the claimed numerical detail.",
            },
        }

    def compile(self, row: dict):
        reviewed = self.root / "reviewed.jsonl"
        write_jsonl(reviewed, [row])
        return corrections.compile_reviewed_corrections(
            reviewed_path=reviewed,
            database_path=self.database,
            canonical_refs=["Matthew 1:1", "Matthew 1:2"],
            canonical_texts=[
                "A canonical genealogy begins here.",
                "Abraham was the father of Isaac.",
            ],
            alternate_texts=["An aligned hidden translation sentence."],
        )

    def test_candidate_queue_is_deterministic_and_never_runtime_eligible(self) -> None:
        first = self.root / "first.jsonl"
        second = self.root / "second.jsonl"
        manifest = corrections.write_candidates(self.database, first)
        corrections.write_candidates(self.database, second)
        self.assertEqual(first.read_bytes(), second.read_bytes())
        self.assertEqual(1, manifest["candidate_count"])
        self.assertFalse(manifest["runtime_eligible"])
        rows = [json.loads(line) for line in first.read_text().splitlines()]
        self.assertEqual("correction_candidate_manifest", rows[0]["kind"])
        self.assertEqual("candidate", rows[1]["review"]["status"])
        self.assertEqual([], rows[1]["claim_surfaces"])

    def test_approved_record_compiles_to_generic_provenance_marked_concept(self) -> None:
        build = self.compile(self.candidate())
        self.assertEqual(1, len(build.records))
        record = build.records[0]
        self.assertEqual("correction", record["concept_type"])
        self.assertEqual("exactly four travelers", record["key"])
        self.assertEqual(["invented traveler count"], record["aliases"])
        self.assertTrue(record["source"].startswith("correction-v1|"))
        self.assertIn("review-status=approved", record["source"])
        self.assertEqual("CC BY-SA 4.0", build.manifest()["source_license"])

    def test_candidate_stale_source_and_scripture_phrases_fail_closed(self) -> None:
        candidate = self.candidate()
        candidate["review"]["status"] = "candidate"
        with self.assertRaisesRegex(ValueError, "not approved"):
            self.compile(candidate)

        stale = self.candidate()
        stale["source"]["content_sha256"] = "00" * 32
        with self.assertRaisesRegex(ValueError, "content hash"):
            self.compile(stale)

        canonical = self.candidate()
        canonical["claim_surfaces"] = ["canonical genealogy begins"]
        with self.assertRaisesRegex(ValueError, "occurs in canonical or alternate"):
            self.compile(canonical)

        alternate = self.candidate()
        alternate["claim_surfaces"] = ["hidden translation sentence"]
        with self.assertRaisesRegex(ValueError, "occurs in canonical or alternate"):
            self.compile(alternate)

    def test_record_identity_and_normalized_claim_collisions_fail_closed(self) -> None:
        wrong_identity = self.candidate()
        wrong_identity["id"] = "aquifer.8"
        with self.assertRaisesRegex(ValueError, "id must identify source row_id"):
            self.compile(wrong_identity)

        duplicate_claim = self.candidate()
        duplicate_claim["claim_surfaces"] = [
            "invented traveler count",
            "invented traveler count",
        ]
        with self.assertRaisesRegex(ValueError, "duplicate normalized claim surface"):
            self.compile(duplicate_claim)

    def test_reviewed_records_join_the_generic_overlay_pipeline(self) -> None:
        canonical = self.root / "canonical.jsonl"
        write_jsonl(
            canonical,
            [
                {
                    "ref": "Matt.1.1",
                    "book": "Matthew",
                    "chapter": "1",
                    "verseNum": "1",
                    "text": "A canonical genealogy begins here.",
                    "events": [],
                },
                {
                    "ref": "Matt.1.2",
                    "book": "Matthew",
                    "chapter": "1",
                    "verseNum": "2",
                    "text": "Abraham was the father of Isaac.",
                    "events": [],
                },
            ],
        )
        reviewed = self.root / "reviewed.jsonl"
        write_jsonl(reviewed, [self.candidate()])
        output = self.root / "overlay.jsonl"
        manifest = overlay.write_overlay(
            canonical_path=canonical,
            output_path=output,
            alternates=[],
            include_events=False,
            study_bible_db_path=self.database,
            reviewed_corrections_path=reviewed,
        )

        correction_source = next(
            source
            for source in manifest["concept_sources"]
            if source["concept_type"] == "correction"
        )
        self.assertEqual(1, correction_source["record_count"])
        self.assertEqual("CC BY-SA 4.0", correction_source["source_license"])
        rows = [json.loads(line) for line in output.read_text().splitlines()]
        correction = next(
            row for row in rows if row.get("concept_type") == "correction"
        )
        self.assertTrue(correction["source"].startswith("correction-v1|"))


if __name__ == "__main__":
    unittest.main()

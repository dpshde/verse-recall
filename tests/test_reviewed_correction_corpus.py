from __future__ import annotations

import hashlib
import importlib.util
import json
import sys
import unittest
from collections import Counter
from pathlib import Path


ROOT = Path(__file__).parents[1]
SCRIPT = ROOT / "scripts" / "build_correction_records.py"
SPEC = importlib.util.spec_from_file_location("reviewed_correction_corpus", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
corrections = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = corrections
SPEC.loader.exec_module(corrections)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class ReviewedCorrectionCorpusTest(unittest.TestCase):
    def test_checked_in_corpus_matches_its_review_manifest(self) -> None:
        schema_path = ROOT / "data" / "correction-record.schema.json"
        for version in ("v1", "v2"):
            with self.subTest(version=version):
                data_root = (
                    ROOT / "data" / "archive" if version == "v1" else ROOT / "data"
                )
                corpus_path = data_root / f"reviewed-corrections-{version}.jsonl"
                manifest = json.loads(
                    (
                        data_root / f"reviewed-corrections-{version}.manifest.json"
                    ).read_text(encoding="utf-8")
                )
                records = [
                    record for _, record in corrections.iter_jsonl(corpus_path)
                ]

                self.assertEqual(manifest["corpus"]["sha256"], sha256(corpus_path))
                self.assertEqual(manifest["schema"]["sha256"], sha256(schema_path))
                self.assertEqual(
                    manifest["corpus"]["byte_count"], corpus_path.stat().st_size
                )
                self.assertEqual(manifest["corpus"]["record_count"], len(records))
                self.assertEqual(
                    manifest["approved_record_ids"],
                    [record["id"] for record in records],
                )
                claim_count = sum(
                    len(record["claim_surfaces"]) for record in records
                )
                self.assertEqual(
                    manifest["review_counts"]["approved_claim_surface_count"],
                    claim_count,
                )
                self.assertEqual(
                    manifest["review_counts"]["approved_alias_count"],
                    claim_count - len(records),
                )

                resource_counts = Counter(
                    record["source"]["resource_type"] for record in records
                )
                expected_resource_counts = {
                    row["resource_type"]: row["approved_count"]
                    for row in manifest["review_counts"]["by_resource_type"]
                }
                self.assertEqual(expected_resource_counts, dict(resource_counts))

                database_hash = manifest["source"]["database_sha256"]
                occupied_surfaces: set[str] = set()
                for record in records:
                    corrections.require_exact_fields(
                        record, corrections.REVIEWED_FIELDS, record["id"]
                    )
                    corrections.require_exact_fields(
                        record["source"], corrections.SOURCE_FIELDS, record["id"]
                    )
                    corrections.require_exact_fields(
                        record["review"], corrections.REVIEW_FIELDS, record["id"]
                    )
                    self.assertEqual(
                        record["id"], f"aquifer.{record['source']['row_id']}"
                    )
                    self.assertEqual(
                        record["source"]["revision"], f"sha256:{database_hash}"
                    )
                    self.assertEqual("approved", record["review"]["status"])
                    self.assertEqual(
                        corrections.REVIEW_POLICY, record["review"]["policy"]
                    )
                    self.assertEqual(
                        manifest["reviewed_by"], record["review"]["reviewed_by"]
                    )
                    self.assertEqual(
                        sorted(record["claim_surfaces"]), record["claim_surfaces"]
                    )
                    for claim in record["claim_surfaces"]:
                        self.assertEqual(claim, corrections.runtime_surface(claim))
                        self.assertNotIn(claim, occupied_surfaces)
                        occupied_surfaces.add(claim)

    def test_v2_retains_v1_and_records_the_full_source_review(self) -> None:
        v1_path = ROOT / "data" / "archive" / "reviewed-corrections-v1.jsonl"
        v2_path = ROOT / "data" / "reviewed-corrections-v2.jsonl"
        manifest = json.loads(
            (ROOT / "data" / "reviewed-corrections-v2.manifest.json").read_text(
                encoding="utf-8"
            )
        )
        v1_records = {
            record["id"]: record for _, record in corrections.iter_jsonl(v1_path)
        }
        v2_records = {
            record["id"]: record for _, record in corrections.iter_jsonl(v2_path)
        }

        self.assertEqual("codex-source-review-v2", manifest["reviewed_by"])
        self.assertEqual(442, manifest["candidate_artifact"]["candidate_count"])
        self.assertEqual(20, manifest["review_counts"]["retained_v1_record_count"])
        self.assertEqual(26, manifest["review_counts"]["new_v2_record_count"])
        self.assertEqual(46, manifest["review_counts"]["approved_record_count"])
        self.assertEqual(396, manifest["review_counts"]["rejected_candidate_count"])
        self.assertEqual(
            94,
            manifest["collision_validation"]["checked_claim_surface_count"],
        )
        self.assertEqual(0, manifest["collision_validation"]["collision_count"])
        self.assertEqual(set(v1_records), set(v1_records) & set(v2_records))

        for record_id, v1_record in v1_records.items():
            v2_record = v2_records[record_id]
            self.assertEqual(
                {key: value for key, value in v1_record.items() if key != "review"},
                {key: value for key, value in v2_record.items() if key != "review"},
            )
            self.assertIn(
                "V2 source re-review retained", v2_record["review"]["rationale"]
            )


if __name__ == "__main__":
    unittest.main()

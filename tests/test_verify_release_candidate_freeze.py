from __future__ import annotations

import copy
import hashlib
import json
import tempfile
import unittest
from unittest import mock
from pathlib import Path

from scripts.verify_release_candidate_freeze import (
    EXPECTED_HOLDOUT_IDENTITIES,
    RELEASE_ARTIFACT_FILES,
    REQUIRED_FILE_PATHS,
    REQUIRED_SOURCE_TREES,
    sha256_file,
    source_tree_sha256,
    verify,
)


class ReleaseCandidateFreezeTests(unittest.TestCase):
    def write(self, root: Path, relative: str, content: bytes) -> Path:
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
        return path

    def file_record(self, root: Path, role: str, relative: str) -> dict:
        path = root / relative
        return {
            "role": role,
            "path": relative,
            "bytes": path.stat().st_size,
            "sha256": sha256_file(path),
        }

    def build_valid_fixture(self, root: Path) -> tuple[Path, dict]:
        source_file_paths = {
            "Cargo.toml",
            "Cargo.lock",
            "bindings/js/package.json",
            "bindings/swift/Package.swift",
            "bindings/kotlin/build.gradle.kts",
            "bindings/kotlin/settings.gradle.kts",
            "bindings/kotlin/gradle.properties",
            *REQUIRED_SOURCE_TREES["builders"],
        }
        for paths in REQUIRED_SOURCE_TREES.values():
            for relative in paths:
                if relative in source_file_paths:
                    self.write(root, relative, f"source:{relative}".encode())
                else:
                    self.write(root, f"{relative}/fixture.txt", f"source:{relative}".encode())

        flexible_paths = {
            "encoder-model": "external/model_quantized.onnx",
            "encoder-tokenizer": "external/tokenizer.json",
            "revealed-regression-summary": "eval/release-candidate-v4-revealed-regressions.json",
            "overlay-intermediate": "external/recall-overlay.jsonl",
            "study-bible-source": "external/study_bible.db",
            "enriched-bsb-source": "external/enriched-bsb.jsonl",
            "selah-topics-source": "external/canonical-topics.json",
            "selah-topic-rankings-source": "external/topic-rankings.json",
            "kjv-source": "external/kjv.jsonl",
        }
        paths = {
            role: required or flexible_paths[role]
            for role, required in REQUIRED_FILE_PATHS.items()
        }

        structured_roles = {
            "release-inventory",
            "release-profile",
            "embedding-contract",
            "revealed-regression-summary",
            "reviewed-corrections-manifest",
        }
        for role, relative in paths.items():
            if role not in structured_roles:
                self.write(root, relative, f"artifact:{role}".encode())

        embedding_space_id = "a" * 64
        contract = {
            "embedding_space_id": embedding_space_id,
            "contract": {
                "tokenizer_sha256": sha256_file(root / paths["encoder-tokenizer"]),
                "encoder_assets": [
                    {
                        "name": Path(paths["encoder-model"]).name,
                        "sha256": sha256_file(root / paths["encoder-model"]),
                    }
                ],
            },
        }
        self.write(
            root,
            paths["embedding-contract"],
            (json.dumps(contract, separators=(",", ":")) + "\n").encode(),
        )

        artifact_records = {}
        for role, filename in RELEASE_ARTIFACT_FILES.items():
            path = root / paths[role]
            artifact_records[filename] = {
                "filename": filename,
                "bytes": path.stat().st_size,
                "sha256": sha256_file(path),
            }
        inventory = {"artifacts": list(artifact_records.values())}
        self.write(
            root,
            paths["release-inventory"],
            (json.dumps(inventory, separators=(",", ":")) + "\n").encode(),
        )

        profile = {
            "embedding_space_id": embedding_space_id,
            "base_pack_sha256": artifact_records["bsb-full.pack"]["sha256"],
            "overlay_sha256": [
                artifact_records["bsb-expand.pack"]["sha256"],
                artifact_records["recall-overlay.pack"]["sha256"],
                artifact_records["bsb-semantic.pack"]["sha256"],
            ],
        }
        self.write(
            root,
            paths["release-profile"],
            (json.dumps(profile, separators=(",", ":")) + "\n").encode(),
        )
        candidate = "release-candidate-v4-frozen-pre-holdout"
        pooled = {
            "total_prompts": 283,
            "retrieval_prompts": 264,
            "recall_at_1": 0.67,
            "recall_at_3": 0.81,
            "recall_at_10": 0.91,
        }
        summary = {
            "schema": "verse-recall-revealed-regression-summary-v1",
            "date": "2026-07-16",
            "candidate": candidate,
            "embedding_space_id": embedding_space_id,
            "query_adapter": None,
            "artifacts_manifest_sha256": sha256_file(root / paths["release-inventory"]),
            "evaluator_sha256": sha256_file(root / paths["release-evaluator"]),
            "pooled": pooled,
            "suite_macro": {},
            "suites": [],
        }
        self.write(
            root,
            paths["revealed-regression-summary"],
            (json.dumps(summary, separators=(",", ":")) + "\n").encode(),
        )

        correction_manifest = {
            "corpus": {
                "path": paths["reviewed-corrections"],
                "byte_count": (root / paths["reviewed-corrections"]).stat().st_size,
                "sha256": sha256_file(root / paths["reviewed-corrections"]),
            },
            "source": {
                "database_sha256": sha256_file(root / paths["study-bible-source"]),
            },
            "collision_validation": {
                "canonical_source_sha256": sha256_file(
                    root / paths["enriched-bsb-source"]
                ),
                "alternate_sources": [
                    {
                        "name": "KJV",
                        "source_sha256": sha256_file(root / paths["kjv-source"]),
                    }
                ],
            },
        }
        self.write(
            root,
            paths["reviewed-corrections-manifest"],
            (json.dumps(correction_manifest, separators=(",", ":")) + "\n").encode(),
        )

        files = [
            self.file_record(root, role, paths[role])
            for role in REQUIRED_FILE_PATHS
        ]
        trees = []
        for name, declared in REQUIRED_SOURCE_TREES.items():
            count, digest = source_tree_sha256(root, declared)
            trees.append(
                {"name": name, "paths": declared, "file_count": count, "sha256": digest}
            )

        holdout_path = self.write(root, "sealed/holdout-v6.jsonl", b"opaque\n")
        summary_record = next(
            record for record in files if record["role"] == "revealed-regression-summary"
        )
        manifest = {
            "schema": "verse-recall-release-candidate-freeze-v1",
            "candidate": candidate,
            "frozen_date": "2026-07-16",
            "status": "frozen-before-first-shot-reveal",
            "query_adapter": None,
            "embedding_space_id": embedding_space_id,
            "files": files,
            "source_trees": trees,
            "sealed_holdout": {
                "path": str(holdout_path.resolve()),
                "bytes": holdout_path.stat().st_size,
                "sha256": sha256_file(holdout_path),
                "split": "holdout-v6",
                "content_inspected_by_runtime_team": False,
            },
            "revealed_regression_summary": {
                "path": summary_record["path"],
                "sha256": summary_record["sha256"],
                **pooled,
            },
            "integrity_rules": {
                "runtime_frozen_before_holdout_binding": True,
                "holdout_content_uninspected_process_assertion": True,
                "one_shot_reveal_only": True,
                "retire_immediately_after_report": True,
                "no_failed_holdout_patching": True,
            },
        }
        manifest_path = root / "freeze.json"
        manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
        return manifest_path, manifest

    def approved_holdout(self, manifest: dict) -> dict[str, str | int]:
        sealed = manifest["sealed_holdout"]
        return {
            "path": sealed["path"],
            "bytes": sealed["bytes"],
            "sha256": sealed["sha256"],
            "split": sealed["split"],
        }

    def test_tree_digest_binds_relative_paths_and_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "tree").mkdir()
            (root / "tree" / "b.txt").write_bytes(b"second")
            (root / "tree" / "a.txt").write_bytes(b"first")
            count, digest = source_tree_sha256(root, ["tree"])

            expected = hashlib.sha256()
            for name, content in (("tree/a.txt", b"first"), ("tree/b.txt", b"second")):
                expected.update(name.encode("utf-8"))
                expected.update(b"\0")
                expected.update(content)
                expected.update(b"\0")

            self.assertEqual(count, 2)
            self.assertEqual(digest, expected.hexdigest())

    def test_tree_digest_changes_when_only_a_path_changes(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "one").mkdir()
            (root / "two").mkdir()
            (root / "one" / "value").write_bytes(b"same")
            (root / "two" / "value").write_bytes(b"same")
            self.assertNotEqual(
                source_tree_sha256(root, ["one"])[1],
                source_tree_sha256(root, ["two"])[1],
            )

    def test_full_verifier_accepts_only_the_complete_cross_bound_fixture(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path, manifest = self.build_valid_fixture(root)
            with (
                mock.patch.dict(
                    EXPECTED_HOLDOUT_IDENTITIES,
                    {manifest["candidate"]: self.approved_holdout(manifest)},
                ),
                mock.patch(
                    "scripts.verify_release_candidate_freeze.verify_release_artifacts",
                    return_value=[],
                ) as strict_verify,
            ):
                candidate, file_count, tree_count = verify(manifest_path, root)
            self.assertEqual(candidate, "release-candidate-v4-frozen-pre-holdout")
            self.assertEqual(file_count, len(REQUIRED_FILE_PATHS))
            self.assertEqual(tree_count, len(REQUIRED_SOURCE_TREES))
            strict_verify.assert_called_once_with(
                root / "data/release-artifacts-v2.json", root / "data"
            )

    def test_full_verifier_rejects_self_selected_files_and_trees(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path, manifest = self.build_valid_fixture(root)
            tampered = copy.deepcopy(manifest)
            tampered["files"] = tampered["files"][:1]
            tampered["source_trees"] = []
            manifest_path.write_text(json.dumps(tampered), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "missing required frozen file roles"):
                verify(manifest_path, root)

    def test_full_verifier_rejects_cross_binding_and_process_claim_tampering(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path, manifest = self.build_valid_fixture(root)
            tampered = copy.deepcopy(manifest)
            tampered["embedding_space_id"] = "b" * 64
            manifest_path.write_text(json.dumps(tampered), encoding="utf-8")
            with mock.patch(
                "scripts.verify_release_candidate_freeze.verify_release_artifacts",
                return_value=[],
            ):
                with self.assertRaisesRegex(ValueError, "release profile embedding-space"):
                    verify(manifest_path, root)

            tampered = copy.deepcopy(manifest)
            tampered["sealed_holdout"]["content_inspected_by_runtime_team"] = True
            manifest_path.write_text(json.dumps(tampered), encoding="utf-8")
            with mock.patch(
                "scripts.verify_release_candidate_freeze.verify_release_artifacts",
                return_value=[],
            ):
                with self.assertRaisesRegex(ValueError, "did not inspect"):
                    verify(manifest_path, root)

    def test_full_verifier_rejects_contaminated_holdout_identity(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path, manifest = self.build_valid_fixture(root)
            contaminated = copy.deepcopy(manifest)
            contaminated["sealed_holdout"]["split"] = "holdout-v3-contaminated"
            manifest_path.write_text(json.dumps(contaminated), encoding="utf-8")
            with mock.patch(
                "scripts.verify_release_candidate_freeze.verify_release_artifacts",
                return_value=[],
            ):
                with self.assertRaisesRegex(ValueError, "requires sealed split holdout-v6"):
                    verify(manifest_path, root)

    def test_full_verifier_rejects_lookalike_holdout_v6_path(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path, manifest = self.build_valid_fixture(root)
            approved = self.approved_holdout(manifest)
            lookalike_path = self.write(
                root,
                "sealed/contaminated/holdout-v6.jsonl",
                (root / "sealed/holdout-v6.jsonl").read_bytes(),
            )
            tampered = copy.deepcopy(manifest)
            tampered["sealed_holdout"].update(
                {
                    "path": str(lookalike_path.resolve()),
                    "bytes": lookalike_path.stat().st_size,
                    "sha256": sha256_file(lookalike_path),
                }
            )
            manifest_path.write_text(json.dumps(tampered), encoding="utf-8")
            with (
                mock.patch.dict(
                    EXPECTED_HOLDOUT_IDENTITIES,
                    {manifest["candidate"]: approved},
                ),
                mock.patch(
                    "scripts.verify_release_candidate_freeze.verify_release_artifacts",
                    return_value=[],
                ),
            ):
                with self.assertRaisesRegex(ValueError, "independently approved identity"):
                    verify(manifest_path, root)


if __name__ == "__main__":
    unittest.main()

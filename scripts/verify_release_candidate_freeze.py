#!/usr/bin/env python3
"""Verify the exact pre-reveal release candidate without parsing its holdout.

The release inventory, profile, packs, encoder, evaluator, build inputs,
regression summary, repository policy, and fixed source trees are mandatory.
The sealed prompt file is hashed as opaque bytes; its human-uninspected status
is an explicit process assertion, not something software can prove.
"""

from __future__ import annotations

import argparse
from datetime import date
import hashlib
import json
from pathlib import Path
import re
from typing import Any

try:
    from scripts.verify_release_artifacts import verify as verify_release_artifacts
except ModuleNotFoundError:  # Direct `python scripts/...` execution.
    from verify_release_artifacts import verify as verify_release_artifacts


SCHEMA = "verse-recall-release-candidate-freeze-v1"
HOLDOUT_SPLIT_RE = re.compile(r"holdout-[A-Za-z0-9][A-Za-z0-9._-]*\Z")
CANDIDATE_RE = re.compile(r"release-candidate-v[0-9]+-[a-z0-9-]+\Z")
SHA256_RE = re.compile(r"[0-9a-f]{64}\Z")
EXPECTED_HOLDOUT_IDENTITIES: dict[str, dict[str, str | int]] = {
    "release-candidate-v4-frozen-pre-holdout": {
        "path": "/private/tmp/verse-recall-holdout-v6-sealed-20260716.jsonl",
        "bytes": 33150,
        "sha256": "2a647a058e639db46b198e082543cdf47d9ad14c2820c1f4c5f41076316d3946",
        "split": "holdout-v6",
    },
}

REQUIRED_FILE_PATHS: dict[str, str | None] = {
    "release-inventory": "data/release-artifacts-v2.json",
    "release-profile": "eval/semantic-release-profile-v1.json",
    "canonical-pack": "data/bsb-full.pack",
    "expansion-pack": "data/bsb-expand.pack",
    "expansion-provenance": "data/bsb-expand.provenance.json",
    "recall-overlay": "data/recall-overlay.pack",
    "dense-pack": "data/bsb-semantic.pack",
    "embedding-contract": "data/semantic-space-arctic-m-v1.5-m256.json",
    "encoder-model": None,
    "encoder-tokenizer": None,
    "release-evaluator": "target/release/recall-eval",
    "release-cli": "target/release/recall-cli",
    "release-packer": "target/release/recall-packer",
    "revealed-regression-summary": None,
    "reviewed-corrections": "data/reviewed-corrections-v2.jsonl",
    "reviewed-corrections-manifest": "data/reviewed-corrections-v2.manifest.json",
    "overlay-intermediate": None,
    "canonical-corpus": "data/bsb.jsonl",
    "study-bible-source": None,
    "enriched-bsb-source": None,
    "selah-topics-source": None,
    "selah-topic-rankings-source": None,
    "kjv-source": None,
    "repository-policy": "AGENTS.md",
    "freeze-verifier": "scripts/verify_release_candidate_freeze.py",
}

RELEASE_ARTIFACT_FILES = {
    "canonical-pack": "bsb-full.pack",
    "expansion-pack": "bsb-expand.pack",
    "expansion-provenance": "bsb-expand.provenance.json",
    "recall-overlay": "recall-overlay.pack",
    "dense-pack": "bsb-semantic.pack",
    "embedding-contract": "semantic-space-arctic-m-v1.5-m256.json",
}

REQUIRED_SOURCE_TREES: dict[str, list[str]] = {
    "runtime": [
        "Cargo.toml",
        "Cargo.lock",
        "crates/recall-core",
        "crates/recall-pack",
    ],
    "evaluator": ["crates/recall-eval"],
    "packer": ["crates/recall-packer"],
    "builders": [
        "scripts/build_recall_overlay.py",
        "scripts/build_semantic_vectors.py",
        "scripts/build_correction_records.py",
        "scripts/ingest_bsb.py",
        "scripts/verify_release_artifacts.py",
        "scripts/verify_reviewed_correction_runtime.py",
    ],
    "portability": [
        "crates/recall-ffi",
        "crates/recall-wasm",
        "bindings/js/package.json",
        "bindings/js/src",
        "bindings/js/test",
        "bindings/js/scripts",
        "bindings/swift/Package.swift",
        "bindings/swift/Sources",
        "bindings/swift/Tests",
        "bindings/swift/scripts",
        "bindings/kotlin/build.gradle.kts",
        "bindings/kotlin/settings.gradle.kts",
        "bindings/kotlin/gradle.properties",
        "bindings/kotlin/scripts",
        "bindings/kotlin/verse-recall/src/main",
        "bindings/kotlin/verse-recall/src/test",
    ],
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def collect_tree_files(root: Path, declared: list[str]) -> list[Path]:
    if not declared:
        raise ValueError("source-tree paths cannot be empty")
    files: set[Path] = set()
    for value in declared:
        if not isinstance(value, str) or not value or Path(value).is_absolute():
            raise ValueError(f"invalid source-tree path: {value!r}")
        path = root / value
        if path.is_file():
            files.add(path)
        elif path.is_dir():
            files.update(candidate for candidate in path.rglob("*") if candidate.is_file())
        else:
            raise ValueError(f"missing source-tree path: {value}")
    if not files:
        raise ValueError("source tree contains no files")
    return sorted(files, key=lambda path: path.relative_to(root).as_posix())


def source_tree_sha256(root: Path, declared: list[str]) -> tuple[int, str]:
    files = collect_tree_files(root, declared)
    digest = hashlib.sha256()
    for path in files:
        relative = path.relative_to(root).as_posix().encode("utf-8")
        digest.update(relative)
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return len(files), digest.hexdigest()


def require_exact_keys(value: dict[str, Any], expected: set[str], context: str) -> None:
    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        raise ValueError(f"{context} keys differ: missing={missing}, extra={extra}")


def _reject_duplicates(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def load_json_object(path: Path, context: str) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_reject_duplicates)
    if not isinstance(value, dict):
        raise ValueError(f"{context} must be a JSON object")
    return value


def resolve_frozen_path(root: Path, raw_path: str) -> Path:
    path = Path(raw_path)
    return path if path.is_absolute() else root / path


def require_sha256(value: Any, context: str) -> str:
    if not isinstance(value, str) or SHA256_RE.fullmatch(value) is None:
        raise ValueError(f"{context} must be a lower-case SHA-256")
    return value


def require_positive_int(value: Any, context: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value <= 0:
        raise ValueError(f"{context} must be a positive integer")
    return value


def require_unit_float(value: Any, context: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{context} must be numeric")
    number = float(value)
    if not 0.0 <= number <= 1.0:
        raise ValueError(f"{context} must be between zero and one")
    return number


def verify_frozen_files(
    root: Path, raw_files: Any
) -> dict[str, tuple[dict[str, Any], Path]]:
    if not isinstance(raw_files, list):
        raise ValueError("files must be an array")
    records: dict[str, tuple[dict[str, Any], Path]] = {}
    seen_paths: set[str] = set()
    for index, record in enumerate(raw_files):
        if not isinstance(record, dict):
            raise ValueError(f"file record {index} must be an object")
        require_exact_keys(record, {"role", "path", "bytes", "sha256"}, f"file record {index}")
        role = record["role"]
        raw_path = record["path"]
        if not isinstance(role, str) or role not in REQUIRED_FILE_PATHS:
            raise ValueError(f"unknown frozen file role: {role!r}")
        if role in records:
            raise ValueError(f"duplicate frozen file role: {role}")
        if not isinstance(raw_path, str) or not raw_path:
            raise ValueError(f"file record {index} has an invalid path")
        if raw_path in seen_paths:
            raise ValueError(f"duplicate frozen path: {raw_path}")
        seen_paths.add(raw_path)
        required_path = REQUIRED_FILE_PATHS[role]
        if required_path is not None and raw_path != required_path:
            raise ValueError(f"role {role} must bind {required_path}")
        expected_bytes = require_positive_int(record["bytes"], f"{role} bytes")
        expected_sha = require_sha256(record["sha256"], f"{role} sha256")
        path = resolve_frozen_path(root, raw_path)
        if not path.is_file():
            raise ValueError(f"missing frozen file for role {role}")
        if path.stat().st_size != expected_bytes:
            raise ValueError(f"byte-size mismatch for role {role}")
        if sha256_file(path) != expected_sha:
            raise ValueError(f"SHA-256 mismatch for role {role}")
        records[role] = (record, path)
    missing = sorted(set(REQUIRED_FILE_PATHS) - set(records))
    if missing:
        raise ValueError(f"missing required frozen file roles: {missing}")
    return records


def verify_source_trees(root: Path, raw_trees: Any) -> int:
    if not isinstance(raw_trees, list):
        raise ValueError("source_trees must be an array")
    records: dict[str, dict[str, Any]] = {}
    for index, record in enumerate(raw_trees):
        if not isinstance(record, dict):
            raise ValueError(f"source tree {index} must be an object")
        require_exact_keys(
            record, {"name", "paths", "file_count", "sha256"}, f"source tree {index}"
        )
        name = record["name"]
        if not isinstance(name, str) or name not in REQUIRED_SOURCE_TREES:
            raise ValueError(f"unknown source-tree name: {name!r}")
        if name in records:
            raise ValueError(f"duplicate source-tree name: {name}")
        if record["paths"] != REQUIRED_SOURCE_TREES[name]:
            raise ValueError(f"source tree {name} does not bind the required paths")
        expected_count = require_positive_int(record["file_count"], f"{name} file_count")
        expected_sha = require_sha256(record["sha256"], f"{name} sha256")
        count, digest = source_tree_sha256(root, REQUIRED_SOURCE_TREES[name])
        if count != expected_count or digest != expected_sha:
            raise ValueError(f"source-tree identity mismatch for {name}")
        records[name] = record
    missing = sorted(set(REQUIRED_SOURCE_TREES) - set(records))
    if missing:
        raise ValueError(f"missing required source trees: {missing}")
    return len(records)


def verify_release_cross_bindings(
    manifest: dict[str, Any],
    records: dict[str, tuple[dict[str, Any], Path]],
) -> None:
    embedding_space_id = require_sha256(
        manifest["embedding_space_id"], "embedding_space_id"
    )

    inventory = load_json_object(records["release-inventory"][1], "release inventory")
    artifacts = inventory.get("artifacts")
    if not isinstance(artifacts, list):
        raise ValueError("release inventory artifacts must be an array")
    by_filename: dict[str, dict[str, Any]] = {}
    for artifact in artifacts:
        if not isinstance(artifact, dict) or not isinstance(artifact.get("filename"), str):
            raise ValueError("release inventory contains a malformed artifact")
        filename = artifact["filename"]
        if filename in by_filename:
            raise ValueError(f"duplicate release inventory filename: {filename}")
        by_filename[filename] = artifact
    if set(by_filename) != set(RELEASE_ARTIFACT_FILES.values()):
        raise ValueError("release inventory does not contain the exact six artifacts")
    for role, filename in RELEASE_ARTIFACT_FILES.items():
        frozen = records[role][0]
        artifact = by_filename[filename]
        if artifact.get("bytes") != frozen["bytes"] or artifact.get("sha256") != frozen["sha256"]:
            raise ValueError(f"release inventory is not bound to role {role}")

    artifact_directory = records["canonical-pack"][1].parent
    if any(records[role][1].parent != artifact_directory for role in RELEASE_ARTIFACT_FILES):
        raise ValueError("all six release artifacts must share one verified directory")
    verify_release_artifacts(records["release-inventory"][1], artifact_directory)

    profile = load_json_object(records["release-profile"][1], "release profile")
    if profile.get("embedding_space_id") != embedding_space_id:
        raise ValueError("release profile embedding-space ID mismatch")
    if profile.get("base_pack_sha256") != records["canonical-pack"][0]["sha256"]:
        raise ValueError("release profile base-pack mismatch")
    expected_overlays = [
        records[role][0]["sha256"]
        for role in ("expansion-pack", "recall-overlay", "dense-pack")
    ]
    if profile.get("overlay_sha256") != expected_overlays:
        raise ValueError("release profile overlay set or order mismatch")

    contract = load_json_object(records["embedding-contract"][1], "embedding contract")
    if contract.get("embedding_space_id") != embedding_space_id:
        raise ValueError("embedding contract ID mismatch")
    payload = contract.get("contract")
    if not isinstance(payload, dict):
        raise ValueError("embedding contract payload must be an object")
    if payload.get("tokenizer_sha256") != records["encoder-tokenizer"][0]["sha256"]:
        raise ValueError("encoder tokenizer is not bound to the embedding contract")
    assets = payload.get("encoder_assets")
    model_name = records["encoder-model"][1].name
    if not isinstance(assets, list) or not any(
        isinstance(asset, dict)
        and asset.get("name") == model_name
        and asset.get("sha256") == records["encoder-model"][0]["sha256"]
        for asset in assets
    ):
        raise ValueError("encoder model is not bound to the embedding contract")

    summary_binding = manifest["revealed_regression_summary"]
    if not isinstance(summary_binding, dict):
        raise ValueError("revealed_regression_summary must be an object")
    summary_keys = {
        "path",
        "sha256",
        "total_prompts",
        "retrieval_prompts",
        "recall_at_1",
        "recall_at_3",
        "recall_at_10",
    }
    require_exact_keys(summary_binding, summary_keys, "revealed_regression_summary")
    frozen_summary = records["revealed-regression-summary"][0]
    if (
        summary_binding["path"] != frozen_summary["path"]
        or summary_binding["sha256"] != frozen_summary["sha256"]
    ):
        raise ValueError("revealed regression summary binding mismatch")
    summary = load_json_object(records["revealed-regression-summary"][1], "regression summary")
    require_exact_keys(
        summary,
        {
            "schema",
            "date",
            "candidate",
            "embedding_space_id",
            "query_adapter",
            "artifacts_manifest_sha256",
            "evaluator_sha256",
            "pooled",
            "suite_macro",
            "suites",
        },
        "regression summary",
    )
    if summary.get("schema") != "verse-recall-revealed-regression-summary-v1":
        raise ValueError("regression summary schema mismatch")
    if summary.get("date") != manifest["frozen_date"]:
        raise ValueError("regression summary date differs from frozen date")
    if summary.get("candidate") != manifest["candidate"]:
        raise ValueError("candidate name differs from revealed regression summary")
    if summary.get("embedding_space_id") != embedding_space_id:
        raise ValueError("regression summary embedding-space ID mismatch")
    if summary.get("query_adapter") is not manifest["query_adapter"]:
        raise ValueError("regression summary query-adapter choice mismatch")
    if summary.get("artifacts_manifest_sha256") != records["release-inventory"][0]["sha256"]:
        raise ValueError("regression summary release-inventory mismatch")
    if summary.get("evaluator_sha256") != records["release-evaluator"][0]["sha256"]:
        raise ValueError("regression summary evaluator mismatch")
    pooled = summary.get("pooled")
    if not isinstance(pooled, dict):
        raise ValueError("regression summary pooled metrics are missing")
    for key in ("total_prompts", "retrieval_prompts"):
        expected = require_positive_int(summary_binding[key], f"summary {key}")
        if pooled.get(key) != expected:
            raise ValueError(f"regression summary {key} mismatch")
    for key in ("recall_at_1", "recall_at_3", "recall_at_10"):
        expected = require_unit_float(summary_binding[key], f"summary {key}")
        if pooled.get(key) != expected:
            raise ValueError(f"regression summary {key} mismatch")

    correction_manifest = load_json_object(
        records["reviewed-corrections-manifest"][1], "reviewed correction manifest"
    )
    corpus = correction_manifest.get("corpus")
    source = correction_manifest.get("source")
    collision = correction_manifest.get("collision_validation")
    if not isinstance(corpus, dict) or not isinstance(source, dict) or not isinstance(collision, dict):
        raise ValueError("reviewed correction manifest provenance is incomplete")
    corrections = records["reviewed-corrections"][0]
    if (
        corpus.get("path") != corrections["path"]
        or corpus.get("byte_count") != corrections["bytes"]
        or corpus.get("sha256") != corrections["sha256"]
    ):
        raise ValueError("reviewed correction corpus is not bound to its manifest")
    if source.get("database_sha256") != records["study-bible-source"][0]["sha256"]:
        raise ValueError("Study Bible source is not bound to correction provenance")
    if collision.get("canonical_source_sha256") != records["enriched-bsb-source"][0]["sha256"]:
        raise ValueError("enriched BSB source is not bound to correction collision review")
    alternates = collision.get("alternate_sources")
    if not isinstance(alternates, list) or not any(
        isinstance(alternate, dict)
        and alternate.get("name") == "KJV"
        and alternate.get("source_sha256") == records["kjv-source"][0]["sha256"]
        for alternate in alternates
    ):
        raise ValueError("KJV source is not bound to correction collision review")


def verify_sealed_holdout(root: Path, candidate: str, raw_sealed: Any) -> None:
    if not isinstance(raw_sealed, dict):
        raise ValueError("sealed_holdout must be an object")
    require_exact_keys(
        raw_sealed,
        {"path", "bytes", "sha256", "split", "content_inspected_by_runtime_team"},
        "sealed_holdout",
    )
    if raw_sealed["content_inspected_by_runtime_team"] is not False:
        raise ValueError("manifest must assert that the runtime team did not inspect the holdout")
    split = raw_sealed["split"]
    if not isinstance(split, str) or HOLDOUT_SPLIT_RE.fullmatch(split) is None:
        raise ValueError("sealed holdout must declare a valid holdout-* split")
    expected_identity = EXPECTED_HOLDOUT_IDENTITIES.get(candidate)
    if expected_identity is None:
        raise ValueError(f"candidate {candidate} has no independently approved holdout identity")
    expected_split = expected_identity["split"]
    if split != expected_split:
        raise ValueError(f"candidate {candidate} requires sealed split {expected_split}")
    raw_path = raw_sealed["path"]
    if not isinstance(raw_path, str) or not raw_path:
        raise ValueError("sealed holdout path must be a nonempty string")
    if (
        not Path(raw_path).is_absolute()
        or raw_path != str(Path(raw_path).resolve(strict=False))
        or raw_path != expected_identity["path"]
    ):
        raise ValueError("sealed holdout path does not match the independently approved identity")
    if "contaminated" in Path(raw_path).as_posix().lower():
        raise ValueError("a contaminated path cannot be used as a sealed holdout")
    expected_bytes = require_positive_int(raw_sealed["bytes"], "sealed holdout bytes")
    expected_sha = require_sha256(raw_sealed["sha256"], "sealed holdout sha256")
    if expected_bytes != expected_identity["bytes"] or expected_sha != expected_identity["sha256"]:
        raise ValueError("sealed holdout size or SHA-256 differs from the approved identity")
    holdout_path = resolve_frozen_path(root, raw_path)
    if not holdout_path.is_file():
        raise ValueError("sealed holdout file is missing")
    if holdout_path.is_symlink():
        raise ValueError("sealed holdout path must identify a regular file, not a symlink")
    if holdout_path.stat().st_size != expected_bytes:
        raise ValueError("sealed holdout byte-size mismatch")
    if sha256_file(holdout_path) != expected_sha:
        raise ValueError("sealed holdout SHA-256 mismatch")


def verify_integrity_rules(raw_rules: Any) -> None:
    if not isinstance(raw_rules, dict):
        raise ValueError("integrity_rules must be an object")
    expected = {
        "runtime_frozen_before_holdout_binding",
        "holdout_content_uninspected_process_assertion",
        "one_shot_reveal_only",
        "retire_immediately_after_report",
        "no_failed_holdout_patching",
    }
    require_exact_keys(raw_rules, expected, "integrity_rules")
    if any(raw_rules[key] is not True for key in expected):
        raise ValueError("every integrity rule must be asserted true")


def verify(manifest_path: Path, root: Path) -> tuple[str, int, int]:
    manifest = load_json_object(manifest_path, "freeze manifest")
    require_exact_keys(
        manifest,
        {
            "schema",
            "candidate",
            "frozen_date",
            "status",
            "query_adapter",
            "embedding_space_id",
            "files",
            "source_trees",
            "sealed_holdout",
            "revealed_regression_summary",
            "integrity_rules",
        },
        "manifest",
    )
    if manifest["schema"] != SCHEMA:
        raise ValueError(f"unsupported freeze schema: {manifest['schema']!r}")
    candidate = manifest["candidate"]
    if not isinstance(candidate, str) or CANDIDATE_RE.fullmatch(candidate) is None:
        raise ValueError("candidate must be a versioned release-candidate name")
    frozen_date = manifest["frozen_date"]
    if not isinstance(frozen_date, str):
        raise ValueError("frozen_date must be an ISO date")
    try:
        date.fromisoformat(frozen_date)
    except ValueError as error:
        raise ValueError("frozen_date must be an ISO date") from error
    if manifest["status"] != "frozen-before-first-shot-reveal":
        raise ValueError("freeze status must be frozen-before-first-shot-reveal")
    if manifest["query_adapter"] is not None:
        raise ValueError("the retained release candidate must not declare a query adapter")
    require_sha256(manifest["embedding_space_id"], "embedding_space_id")

    records = verify_frozen_files(root, manifest["files"])
    tree_count = verify_source_trees(root, manifest["source_trees"])
    verify_release_cross_bindings(manifest, records)
    verify_sealed_holdout(root, candidate, manifest["sealed_holdout"])
    verify_integrity_rules(manifest["integrity_rules"])
    return candidate, len(records), tree_count


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    candidate, file_count, tree_count = verify(args.manifest, root)
    print(
        f"PASS: frozen candidate {candidate}; {file_count} required files and "
        f"{tree_count} fixed source trees match; holdout bytes were hash-bound "
        "without parsing"
    )


if __name__ == "__main__":
    main()

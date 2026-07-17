#!/usr/bin/env python3
"""Build the portable PCA variant of the corpus-only Study vector alignment.

Selection is performed on the same deterministic, whole-book holdout as the
full 256→1536 rectangular alignment.  Study vectors are normalized, centered,
and reduced to 256 dimensions using *training books only*.  A 256×256
orthogonal Procrustes adapter then maps Arctic query vectors into that reduced
Study space.  No evaluation prompt or gold reference is read by this tool.
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import numpy as np

try:
    from scripts.build_semantic_vectors import save_projection, save_query_adapter
    from scripts.build_study_vector_alignment import (
        ALIGNMENT_SEED,
        VIABILITY_THRESHOLDS,
        benchmark_transform,
        fit_rectangular_procrustes,
        load_matrix,
        normalized_matrix,
        read_canonical,
        sha256_file,
        stable_fold,
        validate_transform,
        write_json,
    )
except ModuleNotFoundError:  # Direct ``python scripts/...`` invocation.
    from build_semantic_vectors import save_projection, save_query_adapter
    from build_study_vector_alignment import (
        ALIGNMENT_SEED,
        VIABILITY_THRESHOLDS,
        benchmark_transform,
        fit_rectangular_procrustes,
        load_matrix,
        normalized_matrix,
        read_canonical,
        sha256_file,
        stable_fold,
        validate_transform,
        write_json,
    )


def fit_pca_components(
    vectors: np.ndarray,
    indices: np.ndarray,
    output_dims: int,
    batch_size: int = 1024,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Fit deterministic centered PCA without materializing the training set."""

    if not 0 < output_dims <= vectors.shape[1]:
        raise ValueError("PCA output dimensions must be within the input dimensions")
    if len(indices) < 2:
        raise ValueError("PCA requires at least two training rows")
    total = np.zeros(vectors.shape[1], dtype=np.float64)
    for start in range(0, len(indices), batch_size):
        block = normalized_matrix(vectors[indices[start : start + batch_size]])
        total += block.astype(np.float64).sum(axis=0)
    mean = total / len(indices)

    covariance = np.zeros((vectors.shape[1], vectors.shape[1]), dtype=np.float64)
    for start in range(0, len(indices), batch_size):
        block = normalized_matrix(vectors[indices[start : start + batch_size]])
        centered = block.astype(np.float64) - mean
        covariance += centered.T @ centered
    eigenvalues, eigenvectors = np.linalg.eigh(covariance)
    order = np.argsort(-eigenvalues, kind="stable")[:output_dims]
    components = eigenvectors[:, order]
    for column in range(components.shape[1]):
        pivot = int(np.argmax(np.abs(components[:, column])))
        if components[pivot, column] < 0:
            components[:, column] *= -1
    selected_values = eigenvalues[order]
    return (
        mean.astype(np.float32),
        components.astype(np.float32),
        selected_values,
    )


def project_in_batches(
    vectors: np.ndarray,
    mean: np.ndarray,
    components: np.ndarray,
    batch_size: int = 1024,
) -> np.ndarray:
    output = np.empty((vectors.shape[0], components.shape[1]), dtype=np.float32)
    for start in range(0, vectors.shape[0], batch_size):
        stop = min(start + batch_size, vectors.shape[0])
        block = normalized_matrix(vectors[start:stop]).astype(np.float64)
        projected = (block - mean.astype(np.float64)) @ components.astype(np.float64)
        output[start:stop] = normalized_matrix(projected)
    return output


def bound_inputs(
    canonical: Path,
    arctic_vectors: Path,
    study_vectors: Path,
    arctic_space_contract: Path,
    study_manifest: Path,
) -> tuple[list[dict[str, str]], dict[str, object]]:
    rows = read_canonical(canonical)
    upstream = json.loads(arctic_space_contract.read_text(encoding="utf-8"))
    manifest = json.loads(study_manifest.read_text(encoding="utf-8"))
    study_hash = sha256_file(study_vectors)
    canonical_hash = sha256_file(canonical)
    if manifest.get("output_sha256") != study_hash:
        raise ValueError("Study vector file does not match its extraction manifest")
    if manifest.get("canonical_sha256") != canonical_hash:
        raise ValueError("Study extraction manifest names a different canonical corpus")
    identity = {
        "pipeline_sha256": sha256_file(Path(__file__)),
        "canonical_sha256": canonical_hash,
        "arctic_vectors_sha256": sha256_file(arctic_vectors),
        "study_vectors_sha256": study_hash,
        "arctic_space_contract_sha256": sha256_file(arctic_space_contract),
        "upstream_arctic_embedding_space_id": upstream.get("embedding_space_id"),
        "study_extract_manifest_sha256": sha256_file(study_manifest),
        "study_database_sha256": manifest.get("database_sha256"),
    }
    return rows, identity


def fit_variant(
    source: np.ndarray,
    study: np.ndarray,
    fit_indices: np.ndarray,
    output_dims: int,
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    mean, components, eigenvalues = fit_pca_components(
        study, fit_indices, output_dims
    )
    projected = project_in_batches(study, mean, components)
    adapter, singular_values = fit_rectangular_procrustes(
        source, projected, fit_indices
    )
    return mean, components, projected, adapter, np.concatenate(
        [eigenvalues, singular_values]
    )


def validate(args: argparse.Namespace) -> dict[str, object]:
    rows, identity = bound_inputs(
        args.canonical,
        args.arctic_vectors,
        args.study_vectors,
        args.arctic_space_contract,
        args.study_manifest,
    )
    source = load_matrix(args.arctic_vectors, len(rows), args.source_dims)
    study = load_matrix(args.study_vectors, len(rows), args.study_dims)
    books = [row["book"] for row in rows]
    holdout = np.asarray(
        [
            index
            for index, book in enumerate(books)
            if stable_fold(book, args.fold_count) == args.holdout_fold
        ],
        dtype=np.int64,
    )
    train = np.asarray(
        [
            index
            for index, book in enumerate(books)
            if stable_fold(book, args.fold_count) != args.holdout_fold
        ],
        dtype=np.int64,
    )
    started = time.perf_counter()
    mean, components, projected, adapter, spectra = fit_variant(
        source, study, train, args.output_dims
    )
    fit_seconds = time.perf_counter() - started
    metrics = validate_transform(
        source,
        projected,
        adapter,
        rows,
        holdout,
        args.queries_per_book,
        args.global_candidates,
    )
    report: dict[str, object] = {
        "format": "verse-recall.study-pca-book-holdout-validation.v1",
        "supervision": "canonical corpus coordinate pairs only; no evaluation prompts or gold",
        **identity,
        "source_dimensions": args.source_dims,
        "study_dimensions": args.study_dims,
        "output_dimensions": args.output_dims,
        "canonical_rows": len(rows),
        "train_rows": int(len(train)),
        "holdout_rows": int(len(holdout)),
        "fold_count": args.fold_count,
        "holdout_fold": args.holdout_fold,
        "book_split": {
            "algorithm": "uint64_be(sha256(seed + ':' + book)[:8]) modulo fold_count",
            "seed": ALIGNMENT_SEED,
        },
        "algorithm": "training-book centered Study PCA then orthogonal Procrustes",
        "fit_seconds": fit_seconds,
        "metrics": metrics,
        "pre_registered_thresholds": VIABILITY_THRESHOLDS,
        "adapter_orthogonality_max_abs_error": float(
            np.max(np.abs(adapter @ adapter.T - np.eye(args.source_dims)))
        ),
        "runtime_query_transform_latency": benchmark_transform(adapter),
        "spectrum_summary": {
            "largest": float(spectra.max()),
            "median": float(np.median(spectra)),
            "smallest": float(spectra.min()),
        },
    }
    if metrics["viable"] or args.save_failed_artifacts:
        save_projection(args.projection_output, mean, components)
        save_query_adapter(args.adapter_output, adapter)
        report["projection_sha256"] = sha256_file(args.projection_output)
        report["projection_bytes"] = args.projection_output.stat().st_size
        report["adapter_sha256"] = sha256_file(args.adapter_output)
        report["adapter_bytes"] = args.adapter_output.stat().st_size
    write_json(args.report_output, report)
    return report


def release(args: argparse.Namespace) -> dict[str, object]:
    validation = json.loads(args.validation_report.read_text(encoding="utf-8"))
    if not validation.get("metrics", {}).get("viable"):
        raise ValueError("held-out-book PCA validation did not pass")
    rows, identity = bound_inputs(
        args.canonical,
        args.arctic_vectors,
        args.study_vectors,
        args.arctic_space_contract,
        args.study_manifest,
    )
    mismatches = [
        key for key, digest in identity.items() if validation.get(key) != digest
    ]
    if mismatches:
        raise ValueError(
            "validation report does not bind the requested release inputs: "
            + ", ".join(mismatches)
        )
    source = load_matrix(args.arctic_vectors, len(rows), args.source_dims)
    study = load_matrix(args.study_vectors, len(rows), args.study_dims)
    all_indices = np.arange(len(rows), dtype=np.int64)
    mean, components, projected, adapter, spectra = fit_variant(
        source, study, all_indices, args.output_dims
    )
    save_projection(args.projection_output, mean, components)
    save_query_adapter(args.adapter_output, adapter)
    projected.astype("<f4").tofile(args.projected_output)
    contract = {
        "format": "verse-recall.study-pca-alignment.v1",
        **identity,
        "algorithm": "all-corpus centered Study PCA then orthogonal Procrustes",
        "source_dimensions": args.source_dims,
        "study_dimensions": args.study_dims,
        "output_dimensions": args.output_dims,
        "canonical_rows": len(rows),
        "projection_sha256": sha256_file(args.projection_output),
        "projection_bytes": args.projection_output.stat().st_size,
        "query_adapter_sha256": sha256_file(args.adapter_output),
        "query_adapter_bytes": args.adapter_output.stat().st_size,
        "projected_study_sha256": sha256_file(args.projected_output),
        "projected_study_bytes": args.projected_output.stat().st_size,
        "validation_report_sha256": sha256_file(args.validation_report),
        "spectrum_summary": {
            "largest": float(spectra.max()),
            "median": float(np.median(spectra)),
            "smallest": float(spectra.min()),
        },
    }
    write_json(args.contract_output, contract)
    return contract


def common(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--arctic-vectors", type=Path, required=True)
    parser.add_argument("--study-vectors", type=Path, required=True)
    parser.add_argument("--arctic-space-contract", type=Path, required=True)
    parser.add_argument("--study-manifest", type=Path, required=True)
    parser.add_argument("--source-dims", type=int, default=256)
    parser.add_argument("--study-dims", type=int, default=1536)
    parser.add_argument("--output-dims", type=int, default=256)
    parser.add_argument("--projection-output", type=Path, required=True)
    parser.add_argument("--adapter-output", type=Path, required=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)
    validate_parser = commands.add_parser("validate")
    common(validate_parser)
    validate_parser.add_argument("--fold-count", type=int, default=4)
    validate_parser.add_argument("--holdout-fold", type=int, default=0)
    validate_parser.add_argument("--queries-per-book", type=int, default=32)
    validate_parser.add_argument("--global-candidates", type=int, default=4096)
    validate_parser.add_argument("--report-output", type=Path, required=True)
    validate_parser.add_argument("--save-failed-artifacts", action="store_true")
    validate_parser.set_defaults(run=validate)

    release_parser = commands.add_parser("fit-release")
    common(release_parser)
    release_parser.add_argument("--validation-report", type=Path, required=True)
    release_parser.add_argument("--projected-output", type=Path, required=True)
    release_parser.add_argument("--contract-output", type=Path, required=True)
    release_parser.set_defaults(run=release)

    args = parser.parse_args()
    result = args.run(args)
    if args.command == "validate":
        print(f"held-out-book PCA alignment viable={result['metrics']['viable']}")
    else:
        print(f"wrote projected Study vectors -> {args.projected_output}")


if __name__ == "__main__":
    main()

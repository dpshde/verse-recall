#!/usr/bin/env python3
"""Fit a source-supervised query adapter without evaluation-prompt access.

Study Bible notes are naturally aligned explanatory prose: each note describes
one canonical verse or range independently of this project's red-team sets.
This tool uses those alignments to learn a small query-only linear transform
from encoder query space into canonical verse document space. Hyperparameters
are selected on whole held-out Bible books, never on evaluation prompts.

The output is ``VRQADP01`` as consumed by ``build_semantic_vectors.py``. The
adapter is content-addressed by the embedding-space contract and is never
applied to corpus vectors.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
from typing import Iterable

import numpy as np


SCRIPT = Path(__file__).with_name("build_semantic_vectors.py")
SPEC = importlib.util.spec_from_file_location("verse_recall_semantic_vectors", SCRIPT)
if SPEC is None or SPEC.loader is None:  # pragma: no cover - import invariant
    raise RuntimeError(f"cannot import {SCRIPT}")
semantic = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = semantic
SPEC.loader.exec_module(semantic)
PROJECT_ROOT = Path(__file__).resolve().parents[1]
EVAL_ROOT = PROJECT_ROOT / "eval"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def reject_evaluation_input(path: Path) -> None:
    """Fail closed if a fitting input is sourced from the evaluation tree."""

    resolved = path.resolve()
    if resolved == EVAL_ROOT or EVAL_ROOT in resolved.parents:
        raise ValueError(
            f"evaluation artifacts are forbidden as adapter-fitting inputs: {path}"
        )


def load_metadata(path: Path) -> list[dict]:
    rows = semantic.read_jsonl(path)
    for line_number, row in enumerate(rows, 1):
        if not isinstance(row.get("ref"), str) or not row["ref"].strip():
            raise ValueError(f"metadata row {line_number} has no canonical ref")
        if row.get("view") not in {"study-note", "canonical-fallback"}:
            raise ValueError(f"metadata row {line_number} has an invalid view")
    return rows


def reference_book(reference: str) -> str:
    try:
        book, coordinate = reference.rsplit(" ", 1)
        chapter, verse = coordinate.split(":", 1)
        if not book or int(chapter) <= 0 or int(verse) <= 0:
            raise ValueError
    except (ValueError, TypeError) as error:
        raise ValueError(f"invalid canonical reference {reference!r}") from error
    return book


def heldout_books(rows: list[dict], stride: int) -> list[str]:
    if stride < 3:
        raise ValueError("book holdout stride must be at least 3")
    books = sorted({reference_book(row["ref"]) for row in rows})
    if len(books) < stride * 2:
        raise ValueError("source corpus has too few books for a book-held-out split")
    # Every Nth sorted book is transparent, deterministic, and independent of
    # content, prompts, model scores, or canonical targets.
    return books[::stride]


def stable_validation_indices(
    rows: list[dict], eligible: np.ndarray, holdout: set[str], limit: int
) -> np.ndarray:
    candidates = [
        index
        for index in np.flatnonzero(eligible)
        if reference_book(rows[int(index)]["ref"]) in holdout
    ]
    if not candidates:
        raise ValueError("book-held-out split contains no Study Note rows")
    ranked = sorted(
        candidates,
        key=lambda index: (
            hashlib.sha256(rows[int(index)]["ref"].encode("utf-8")).digest(),
            index,
        ),
    )
    return np.asarray(ranked[: min(limit, len(ranked))], dtype=np.int64)


def fit_procrustes(source: np.ndarray, target: np.ndarray) -> np.ndarray:
    if source.shape != target.shape or source.ndim != 2:
        raise ValueError("Procrustes requires equally shaped 2D matrices")
    left, _singular, right = np.linalg.svd(
        source.astype(np.float64).T @ target.astype(np.float64),
        full_matrices=False,
    )
    return (left @ right).astype(np.float32)


def fit_ridge(source: np.ndarray, target: np.ndarray, multiplier: float) -> np.ndarray:
    if source.shape != target.shape or source.ndim != 2:
        raise ValueError("ridge fitting requires equally shaped 2D matrices")
    if multiplier <= 0:
        raise ValueError("ridge multiplier must be positive")
    source64 = source.astype(np.float64)
    target64 = target.astype(np.float64)
    gram = source64.T @ source64
    scale = float(np.trace(gram) / gram.shape[0])
    regularization = multiplier * max(scale, np.finfo(np.float64).eps)
    matrix = np.linalg.solve(
        gram + np.eye(gram.shape[0], dtype=np.float64) * regularization,
        source64.T @ target64,
    )
    return matrix.astype(np.float32)


def candidate_adapters(source: np.ndarray, target: np.ndarray) -> Iterable[tuple[str, np.ndarray]]:
    if source.shape[1] != target.shape[1]:
        raise ValueError("automatic adapters require equal source and target dimensions")
    yield "identity", np.eye(source.shape[1], dtype=np.float32)
    yield "orthogonal-procrustes", fit_procrustes(source, target)
    for multiplier in (0.001, 0.01, 0.1, 1.0):
        yield f"ridge-{multiplier:g}x", fit_ridge(source, target, multiplier)


def equivalent_gold_indices(rows: list[dict], eligible: np.ndarray) -> dict[int, np.ndarray]:
    """Group verses backed by the exact same source-authored note document.

    Aquifer notes can cover a passage rather than one verse. Penalizing a
    retriever for returning another verse carrying that identical note would
    turn source validation into false precision. Exact text equality is a
    deliberately strict, source-only equivalence relation.
    """

    by_text: dict[str, list[int]] = {}
    for index in np.flatnonzero(eligible):
        text = rows[int(index)].get("text")
        if not isinstance(text, str) or not text.strip():
            raise ValueError(f"eligible metadata row {int(index) + 1} has no note text")
        by_text.setdefault(text, []).append(int(index))
    return {
        index: np.asarray(indices, dtype=np.int64)
        for indices in by_text.values()
        for index in indices
    }


def exact_retrieval_metrics(
    source: np.ndarray,
    target_corpus: np.ndarray,
    gold_indices: np.ndarray,
    adapter: np.ndarray,
    batch_size: int,
    gold_groups: dict[int, np.ndarray] | None = None,
) -> dict:
    if len(source) != len(gold_indices):
        raise ValueError("validation vectors and gold indices are not aligned")
    adapted = semantic.apply_query_adapter(source, adapter)
    target_corpus = semantic.normalize_vectors(target_corpus)
    ranks: list[int] = []
    positive_scores: list[float] = []
    all_indices = np.arange(len(target_corpus), dtype=np.int64)
    for start in range(0, len(adapted), batch_size):
        query = adapted[start : start + batch_size]
        gold = gold_indices[start : start + batch_size]
        scores = query.astype(np.float32) @ target_corpus.T.astype(np.float32)
        positive = np.empty(len(query), dtype=np.float32)
        positive_index = np.empty(len(query), dtype=np.int64)
        for row_offset, gold_index in enumerate(gold):
            acceptable = (
                gold_groups.get(int(gold_index), np.asarray([gold_index]))
                if gold_groups is not None
                else np.asarray([gold_index])
            )
            acceptable_scores = scores[row_offset, acceptable]
            best_score = acceptable_scores.max()
            # Runtime tie-breaking favors the lower canonical verse index.
            best_indices = acceptable[acceptable_scores == best_score]
            positive[row_offset] = best_score
            positive_index[row_offset] = int(best_indices.min())
        # Exact deterministic rank: larger cosine first, then lower canonical
        # verse index for ties, matching the runtime's stable ordering.
        greater = (scores > positive[:, None]).sum(axis=1)
        tied_before = (
            (scores == positive[:, None])
            & (all_indices[None, :] < positive_index[:, None])
        ).sum(axis=1)
        ranks.extend((1 + greater + tied_before).astype(int).tolist())
        positive_scores.extend(positive.astype(float).tolist())
    ranks_array = np.asarray(ranks, dtype=np.int64)
    return {
        "queries": int(len(ranks_array)),
        "hit_at_1": float(np.mean(ranks_array <= 1)),
        "hit_at_3": float(np.mean(ranks_array <= 3)),
        "hit_at_10": float(np.mean(ranks_array <= 10)),
        "hit_at_50": float(np.mean(ranks_array <= 50)),
        "hit_at_100": float(np.mean(ranks_array <= 100)),
        "mrr": float(np.mean(1.0 / ranks_array)),
        "mean_positive_cosine": float(np.mean(positive_scores)),
        "median_rank": float(np.median(ranks_array)),
    }


def selection_key(item: tuple[str, np.ndarray, dict]) -> tuple:
    name, _matrix, metrics = item
    # R@3 is the primary product gate, followed by breadth at R@10. The name
    # provides a final stable tie-break and cannot depend on prompt outcomes.
    return (
        metrics["hit_at_3"],
        metrics["hit_at_10"],
        metrics["hit_at_1"],
        metrics["mrr"],
        metrics["mean_positive_cosine"],
        name,
    )


def run(args: argparse.Namespace) -> dict:
    for input_path in (args.metadata, args.source_vectors, args.target_vectors):
        reject_evaluation_input(input_path)
    rows = load_metadata(args.metadata)
    source = semantic.normalize_vectors(
        semantic.read_f32_matrix(args.source_vectors, args.dims)
    )
    target = semantic.normalize_vectors(
        semantic.read_f32_matrix(args.target_vectors, args.dims)
    )
    if len(rows) != len(source) or len(rows) != len(target):
        raise ValueError(
            "metadata, source vectors, and target vectors must have identical aligned rows"
        )
    eligible = np.asarray([row["view"] == "study-note" for row in rows], dtype=bool)
    holdout = set(heldout_books(rows, args.book_holdout_stride))
    train_indices = np.asarray(
        [
            index
            for index in np.flatnonzero(eligible)
            if reference_book(rows[int(index)]["ref"]) not in holdout
        ],
        dtype=np.int64,
    )
    validation_indices = stable_validation_indices(
        rows, eligible, holdout, args.validation_limit
    )
    gold_groups = equivalent_gold_indices(rows, eligible)
    if len(train_indices) < args.dims * 2:
        raise ValueError("too few source-supervised rows to fit a stable adapter")

    candidates = []
    for name, matrix in candidate_adapters(source[train_indices], target[train_indices]):
        metrics = exact_retrieval_metrics(
            source[validation_indices],
            target,
            validation_indices,
            matrix,
            args.batch_size,
            gold_groups,
        )
        candidates.append((name, matrix, metrics))
    selected_name, _validation_matrix, selected_metrics = max(
        candidates, key=selection_key
    )

    # Hyperparameters are now frozen. Refit the selected family on all
    # source-authored notes for the distributable adapter.
    all_indices = np.flatnonzero(eligible)
    if selected_name == "identity":
        final_matrix = np.eye(args.dims, dtype=np.float32)
    elif selected_name == "orthogonal-procrustes":
        final_matrix = fit_procrustes(source[all_indices], target[all_indices])
    else:
        multiplier = float(selected_name.removeprefix("ridge-").removesuffix("x"))
        final_matrix = fit_ridge(source[all_indices], target[all_indices], multiplier)
    semantic.save_query_adapter(args.output, final_matrix)

    report = {
        "schema": "verse-recall-source-supervised-query-adapter-v1",
        "selection_policy": "whole-book-held-out; exact full-corpus retrieval; no eval prompts",
        "pipeline_sha256": sha256_file(Path(__file__)),
        "metadata": {
            "path": str(args.metadata),
            "sha256": sha256_file(args.metadata),
            "rows": len(rows),
            "study_note_rows": int(eligible.sum()),
            "canonical_fallback_rows": int((~eligible).sum()),
        },
        "source_vectors": {
            "path": str(args.source_vectors),
            "sha256": sha256_file(args.source_vectors),
        },
        "target_vectors": {
            "path": str(args.target_vectors),
            "sha256": sha256_file(args.target_vectors),
        },
        "dimensions": args.dims,
        "training_rows": int(len(train_indices)),
        "validation_rows": int(len(validation_indices)),
        "validation_unique_note_documents": len(
            {rows[int(index)]["text"] for index in validation_indices}
        ),
        "source_note_equivalence_groups": len(
            {rows[int(index)]["text"] for index in np.flatnonzero(eligible)}
        ),
        "book_holdout_stride": args.book_holdout_stride,
        "heldout_books": sorted(holdout),
        "candidates": [
            {"name": name, **metrics} for name, _matrix, metrics in candidates
        ],
        "selected": {"name": selected_name, **selected_metrics},
        "output": {
            "path": str(args.output),
            "sha256": sha256_file(args.output),
            "bytes": args.output.stat().st_size,
            "application": "post-encoder-query-only-v1",
        },
    }
    args.report.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return report


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-vectors", type=Path, required=True)
    parser.add_argument("--target-vectors", type=Path, required=True)
    parser.add_argument("--metadata", type=Path, required=True)
    parser.add_argument("--dims", type=int, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--book-holdout-stride", type=int, default=5)
    parser.add_argument("--validation-limit", type=int, default=512)
    parser.add_argument("--batch-size", type=int, default=64)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.dims <= 0 or args.validation_limit <= 0 or args.batch_size <= 0:
        raise ValueError("dimensions, validation limit, and batch size must be positive")
    report = run(args)
    print(
        "selected source-supervised query adapter "
        f"{report['selected']['name']} "
        f"(held-out R@3={report['selected']['hit_at_3']:.3f}, "
        f"R@10={report['selected']['hit_at_10']:.3f})"
    )


if __name__ == "__main__":
    main()

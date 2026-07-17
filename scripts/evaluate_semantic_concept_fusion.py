#!/usr/bin/env python3
"""Evaluate source-derived concept vectors beside an existing ranking.

This is deliberately an offline experiment driver.  It consumes a complete
concept corpus and its aligned vectors, then emits ordinary live-capture JSONL
for the strict Rust evaluator.  It does not inspect prompt gold references.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np

try:
    from build_recall_overlay import canonical_osis_index, load_canonical, parse_osis_range
except ModuleNotFoundError:
    from scripts.build_recall_overlay import (
        canonical_osis_index,
        load_canonical,
        parse_osis_range,
    )


RRF_K = 60.0


def read_jsonl(path: Path) -> list[dict]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError(f"{path}:{line_number}: expected an object")
        rows.append(value)
    return rows


def baseline_rankings(path: Path) -> dict[str, list[str]]:
    report = json.loads(path.read_text(encoding="utf-8"))
    systems = report.get("systems")
    if not isinstance(systems, list):
        raise ValueError(f"{path}: missing systems")
    system = next(
        (candidate for candidate in systems if candidate.get("name") == "verse-recall"),
        None,
    )
    if system is None:
        raise ValueError(f"{path}: missing verse-recall system")
    rankings = {}
    for row in system.get("per_prompt", []):
        prompt_id, refs = row.get("id"), row.get("top_refs")
        if not isinstance(prompt_id, str) or not isinstance(refs, list):
            raise ValueError(f"{path}: malformed per-prompt result")
        if prompt_id in rankings:
            raise ValueError(f"{path}: duplicate prompt id {prompt_id!r}")
        rankings[prompt_id] = refs
    return rankings


def rrf_fuse(
    primary: list[str],
    secondary: list[str],
    secondary_weight: float,
    *,
    intersection_only: bool = False,
) -> list[str]:
    if intersection_only:
        primary_set = set(primary)
        secondary = [reference for reference in secondary if reference in primary_set]
    scores: dict[str, float] = {}
    first_seen: dict[str, tuple[int, int]] = {}
    for source, (ranking, weight) in enumerate(
        ((primary, 1.0), (secondary, secondary_weight))
    ):
        for rank, reference in enumerate(ranking):
            if reference not in first_seen:
                first_seen[reference] = (source, rank)
            scores[reference] = scores.get(reference, 0.0) + weight / (RRF_K + rank + 1)
    return sorted(
        scores,
        key=lambda reference: (-scores[reference], first_seen[reference], reference),
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--concept-corpus", type=Path, required=True)
    parser.add_argument("--concept-vectors", type=Path, required=True)
    parser.add_argument("--canonical", type=Path, required=True)
    parser.add_argument("--dimensions", type=int, required=True)
    parser.add_argument("--query-embeddings", type=Path, required=True)
    parser.add_argument("--baseline-report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--depth", type=int, default=100)
    parser.add_argument("--secondary-weight", type=float, action="append")
    parser.add_argument(
        "--intersection-only",
        action="store_true",
        help="allow concepts to reorder only candidates already retrieved by the primary view",
    )
    args = parser.parse_args()

    concepts = read_jsonl(args.concept_corpus)
    canonical = load_canonical(args.canonical)
    canonical_by_osis = canonical_osis_index(canonical)
    concept_anchors = []
    for concept in concepts:
        start, _ = parse_osis_range(concept["ref"])
        index = canonical_by_osis.get(start)
        if index is None:
            raise ValueError(f"concept target {concept['ref']!r} is not canonical")
        concept_anchors.append(canonical[index].display_ref)
    vectors = np.fromfile(args.concept_vectors, dtype=np.float32)
    expected = len(concepts) * args.dimensions
    if vectors.size != expected:
        raise ValueError(
            f"concept vector count mismatch: {vectors.size} values, expected {expected}"
        )
    vectors = vectors.reshape(len(concepts), args.dimensions)
    if not np.isfinite(vectors).all():
        raise ValueError("concept vectors contain nonfinite values")
    queries = read_jsonl(args.query_embeddings)
    baseline = baseline_rankings(args.baseline_report)
    query_ids = {row.get("id") for row in queries}
    if query_ids != set(baseline):
        raise ValueError("query embeddings and baseline report ids differ")
    weights = args.secondary_weight or [0.25, 0.5, 0.75, 1.0]
    if any(not np.isfinite(weight) or weight <= 0 for weight in weights):
        raise ValueError("secondary weights must be positive and finite")

    captures = []
    for query in queries:
        prompt_id = query["id"]
        embedding = np.asarray(query.get("embedding"), dtype=np.float32)
        if embedding.shape != (args.dimensions,) or not np.isfinite(embedding).all():
            raise ValueError(f"query {prompt_id!r} has an invalid embedding")
        scores = vectors @ embedding
        order = np.lexsort((np.arange(len(concepts)), -scores))
        concept_refs = []
        seen = set()
        for index in order:
            reference = concept_anchors[int(index)]
            if reference not in seen:
                seen.add(reference)
                concept_refs.append(reference)
            if len(concept_refs) >= args.depth:
                break
        for weight in weights:
            fused = rrf_fuse(
                baseline[prompt_id],
                concept_refs,
                weight,
                intersection_only=args.intersection_only,
            )[: args.depth]
            if len(fused) != args.depth:
                raise ValueError(f"query {prompt_id!r} produced only {len(fused)} results")
            captures.append(
                {
                    "id": prompt_id,
                    "tool": (
                        "verse+selah-concept-intersection-rrf-"
                        if args.intersection_only
                        else "verse+selah-concept-rrf-"
                    )
                    + f"{weight:g}",
                    "top_refs": fused,
                }
            )

    with args.output.open("w", encoding="utf-8") as output:
        for capture in captures:
            output.write(json.dumps(capture, separators=(",", ":")) + "\n")
    print(
        f"wrote {len(captures)} captures for {len(queries)} queries and "
        f"{len(weights)} generic RRF weights -> {args.output}"
    )


if __name__ == "__main__":
    main()

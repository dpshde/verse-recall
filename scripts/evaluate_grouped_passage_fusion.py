#!/usr/bin/env python3
"""Evaluate paragraph-grouped dense retrieval without evaluation gold access.

The aligned research table contains one focus-verse vector per canonical row.
Several rows can represent the same source paragraph.  This driver keeps only
the best focus verse from each paragraph before rank fusion, preventing a long
paragraph from consuming many result slots.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path

import numpy as np

try:
    from evaluate_semantic_concept_fusion import baseline_rankings, read_jsonl, rrf_fuse
except ModuleNotFoundError:
    from scripts.evaluate_semantic_concept_fusion import (
        baseline_rankings,
        read_jsonl,
        rrf_fuse,
    )


PASSAGE_MARKER = " Passage context: "


def paragraph_groups(rows: list[dict]) -> np.ndarray:
    keys: dict[str, int] = {}
    groups = []
    for row_number, row in enumerate(rows, 1):
        text = row.get("text")
        if not isinstance(text, str) or not text.strip():
            raise ValueError(f"semantic document row {row_number} has no text")
        _, marker, context = text.partition(PASSAGE_MARKER)
        # Rows without a source passage remain independent canonical verses.
        key = context if marker and context.strip() else f"verse:{row_number}"
        if key not in keys:
            keys[key] = len(keys)
        groups.append(keys[key])
    return np.asarray(groups, dtype=np.int32)


def grouped_ranking(
    scores: np.ndarray,
    groups: np.ndarray,
    references: list[str],
    depth: int,
) -> list[str]:
    order = np.lexsort((np.arange(scores.size), -scores))
    seen_groups = set()
    seen_refs = set()
    ranking = []
    for index in order:
        group = int(groups[int(index)])
        reference = references[int(index)]
        if group in seen_groups or reference in seen_refs:
            continue
        seen_groups.add(group)
        seen_refs.add(reference)
        ranking.append(reference)
        if len(ranking) >= depth:
            break
    return ranking


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--documents", type=Path, required=True)
    parser.add_argument("--vectors", type=Path, required=True)
    parser.add_argument("--dimensions", type=int, required=True)
    parser.add_argument("--query-embeddings", type=Path, required=True)
    parser.add_argument("--baseline-report", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--depth", type=int, default=100)
    parser.add_argument("--secondary-weight", type=float, action="append")
    parser.add_argument("--intersection-only", action="store_true")
    args = parser.parse_args()

    documents = read_jsonl(args.documents)
    references = [row.get("ref") for row in documents]
    if not all(isinstance(reference, str) and reference for reference in references):
        raise ValueError("every semantic document requires a canonical ref")
    groups = paragraph_groups(documents)
    vectors = np.fromfile(args.vectors, dtype=np.float32)
    expected = len(documents) * args.dimensions
    if vectors.size != expected:
        raise ValueError(f"vector count mismatch: {vectors.size} values, expected {expected}")
    vectors = vectors.reshape(len(documents), args.dimensions)
    if not np.isfinite(vectors).all():
        raise ValueError("passage vectors contain nonfinite values")

    queries = read_jsonl(args.query_embeddings)
    baseline = baseline_rankings(args.baseline_report)
    if {row.get("id") for row in queries} != set(baseline):
        raise ValueError("query embeddings and baseline report ids differ")
    weights = args.secondary_weight or [0.25, 0.5, 0.75, 1.0]

    captures = []
    for query in queries:
        prompt_id = query["id"]
        embedding = np.asarray(query.get("embedding"), dtype=np.float32)
        if embedding.shape != (args.dimensions,) or not np.isfinite(embedding).all():
            raise ValueError(f"query {prompt_id!r} has an invalid embedding")
        passage = grouped_ranking(vectors @ embedding, groups, references, args.depth)
        for weight in weights:
            fused = rrf_fuse(
                baseline[prompt_id],
                passage,
                weight,
                intersection_only=args.intersection_only,
            )[: args.depth]
            if len(fused) != args.depth:
                raise ValueError(f"query {prompt_id!r} produced only {len(fused)} results")
            captures.append(
                {
                    "id": prompt_id,
                    "tool": (
                        "verse+grouped-passage-intersection-rrf-"
                        if args.intersection_only
                        else "verse+grouped-passage-rrf-"
                    )
                    + f"{weight:g}",
                    "top_refs": fused,
                }
            )

    with args.output.open("w", encoding="utf-8") as output:
        for capture in captures:
            output.write(json.dumps(capture, separators=(",", ":")) + "\n")
    print(
        f"wrote {len(captures)} captures from {len(set(groups.tolist()))} paragraph groups "
        f"-> {args.output}"
    )


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build a deterministic multi-vector concept corpus for retrieval research.

Unlike an enriched verse document, this keeps each independently sourced
concept label as its own semantic vector and preserves the complete ranked
target list.  It never reads evaluation prompts or gold references.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def load_array(path: Path, label: str) -> list:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, list):
        raise ValueError(f"{path}: {label} must be a JSON array")
    return value


def build_rows(topics_path: Path, rankings_path: Path) -> list[dict]:
    labels: dict[str, str] = {}
    for row_number, row in enumerate(load_array(topics_path, "topics"), 1):
        if (
            not isinstance(row, list)
            or len(row) != 2
            or not isinstance(row[0], str)
            or not isinstance(row[1], str)
            or not row[0].strip()
            or not row[1].strip()
        ):
            raise ValueError(f"{topics_path}:{row_number}: invalid topic row")
        topic_id, label = row[0].strip(), row[1].strip()
        if topic_id in labels:
            raise ValueError(f"{topics_path}:{row_number}: duplicate topic id {topic_id!r}")
        labels[topic_id] = label

    rankings: dict[str, list[dict]] = {}
    for row_number, row in enumerate(load_array(rankings_path, "rankings"), 1):
        if (
            not isinstance(row, list)
            or len(row) != 2
            or not isinstance(row[0], str)
            or not isinstance(row[1], list)
        ):
            raise ValueError(f"{rankings_path}:{row_number}: invalid ranking row")
        topic_id = row[0].strip()
        if topic_id in rankings:
            raise ValueError(f"{rankings_path}:{row_number}: duplicate topic id {topic_id!r}")
        targets = []
        for target_number, target in enumerate(row[1], 1):
            if (
                not isinstance(target, list)
                or len(target) != 3
                or not isinstance(target[0], str)
                or not target[0].strip()
                or not isinstance(target[1], (int, float))
                or not isinstance(target[2], int)
                or target[2] <= 0
            ):
                raise ValueError(
                    f"{rankings_path}:{row_number}: invalid target {target_number}"
                )
            targets.append(
                {
                    "ref": target[0].strip(),
                    "source_score": float(target[1]),
                    "source_rank": target[2],
                }
            )
        rankings[topic_id] = sorted(
            targets,
            key=lambda value: (
                value["source_rank"],
                -value["source_score"],
                value["ref"],
            ),
        )

    unknown = sorted(set(rankings) - set(labels))
    missing = sorted(set(labels) - set(rankings))
    if unknown or missing:
        raise ValueError(
            f"topic/ranking identity mismatch: {len(missing)} missing, {len(unknown)} unknown"
        )

    return [
        {
            "id": topic_id,
            "ref": targets[0]["ref"],
            "text": labels[topic_id],
            "source": "Selah.canonical-topics+topic-verse-rankings",
            "targets": targets,
        }
        for topic_id in sorted(labels)
        if (targets := rankings[topic_id])
    ]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--canonical-topics", type=Path, required=True)
    parser.add_argument("--topic-verse-rankings", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    rows = build_rows(args.canonical_topics, args.topic_verse_rankings)
    with args.output.open("w", encoding="utf-8") as output:
        for row in rows:
            output.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
    print(f"wrote {len(rows)} source-derived semantic concept rows -> {args.output}")


if __name__ == "__main__":
    main()

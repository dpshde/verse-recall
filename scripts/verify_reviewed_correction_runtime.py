#!/usr/bin/env python3
"""End-to-end contract check for a source-reviewed correction overlay.

This is not a retrieval benchmark: every query surface and expected anchor is
declared by the reviewed source artifact itself. The verifier proves that the
compiled pack preserves those declarations through the production CLI.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def sha256_file(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read_jsonl(path: Path) -> list[dict]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def verify(args: argparse.Namespace) -> dict:
    records = read_jsonl(args.reviewed)
    failures = []
    checked = 0
    for record in records:
        for surface in record["claim_surfaces"]:
            command = [
                str(args.cli),
                "search",
                "--pack",
                str(args.pack),
            ]
            for overlay in args.overlay:
                command.extend(("--overlay", str(overlay)))
            command.extend(("--top-k", "3", "--json", surface))
            completed = subprocess.run(
                command,
                check=True,
                capture_output=True,
                text=True,
                timeout=args.timeout,
            )
            payload = json.loads(completed.stdout)
            lead = payload.get("results", [{}])[0]
            required_evidence = f"id={record['id']}|"
            actual_passages = {
                (passage.get("start_ref"), passage.get("end_ref"))
                for passage in lead.get("passages", [])
            }
            expected_passages = {
                (target["start_ref"], target["end_ref"])
                for target in record["target_ranges"]
            }
            checks = {
                "anchor": lead.get("ref") == record["anchor_ref"],
                "confidence": lead.get("confidence") == "often-misremembered",
                "correction_label": (
                    "correction:" + record["correction_label"]
                    in lead.get("expanded_terms", [])
                ),
                "provenance": any(
                    required_evidence in value
                    for value in lead.get("expanded_terms", [])
                ),
                "structured_passages": expected_passages.issubset(actual_passages),
            }
            checked += 1
            if not all(checks.values()):
                failures.append(
                    {
                        "record_id": record["id"],
                        "surface": surface,
                        "checks": checks,
                        "lead_ref": lead.get("ref"),
                        "lead_confidence": lead.get("confidence"),
                    }
                )
    report = {
        "schema": "verse-recall-reviewed-correction-runtime-verification-v1",
        "reviewed_sha256": sha256_file(args.reviewed),
        "pack_sha256": sha256_file(args.pack),
        "overlay_sha256": [sha256_file(path) for path in args.overlay],
        "record_count": len(records),
        "claim_surface_count": checked,
        "passes": checked - len(failures),
        "failures": failures,
        "success": not failures,
    }
    if args.report:
        args.report.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    if failures:
        raise SystemExit(
            f"reviewed correction runtime verification failed: {len(failures)}/{checked}"
        )
    return report


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--reviewed", type=Path, required=True)
    parser.add_argument("--cli", type=Path, required=True)
    parser.add_argument("--pack", type=Path, required=True)
    parser.add_argument("--overlay", type=Path, action="append", default=[])
    parser.add_argument("--report", type=Path)
    parser.add_argument("--timeout", type=float, default=10.0)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    for path in (args.reviewed, args.cli, args.pack, *args.overlay):
        if not path.is_file():
            raise SystemExit(f"missing verifier input: {path}")
    report = verify(args)
    print(
        f"reviewed correction runtime: {report['passes']}/"
        f"{report['claim_surface_count']} claim surfaces passed"
    )


if __name__ == "__main__":
    main()

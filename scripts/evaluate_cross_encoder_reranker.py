#!/usr/bin/env python3
"""Build a reproducible, revealed-suite-only cross-encoder reranker export.

This is an evaluation harness, not production ranking code.  It consumes a
depth-100 product-result export, scores the *same* candidates using canonical
BSB text, and emits:

* a full-metadata pair export for ``evaluate_dense_pair`` (product order vs.
  cross-encoder order), and
* reference-only JSONL for ``recall-eval --live-results`` (product,
  cross-encoder, and a predeclared rank-only RRF-60 comparator).

The eight accepted prompt files are pinned below.  The harness never discovers
prompt files, never accepts a holdout path, and never reads gold fields while
scoring.  Gold and auxiliary result metadata are copied through only so the
canonical evaluator can measure recall, confidence safety, passage output,
hard negatives, and required confidence labels after reordering.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import statistics
import time
from typing import Any, Iterable


@dataclass(frozen=True)
class RevealedSuite:
    key: str
    relative_path: str
    sha256: str


# Keep this synchronized with scripts/evaluate_encoder_bakeoff.py.  Do not
# replace it with a glob: a glob could silently ingest a future sealed holdout.
REVEALED_SUITES = (
    RevealedSuite(
        "semantic",
        "eval/semantic-quality-v2.jsonl",
        "f47e0d037c423a8c199f0d7733a5d5afec2f57fa0e62cfe879ba7c95b010915d",
    ),
    RevealedSuite(
        "tot",
        "eval/tot-prompts.jsonl",
        "1f1c626cd8650e21e3c85f559f84040b2818d77dd67bced707db3f890a208918",
    ),
    RevealedSuite(
        "diagnostic",
        "eval/overfit-diagnostic-v1.jsonl",
        "acd45841777e012cc8f7e58e042a216f4e9e9e02a8a39aecabf8c2288442f4fa",
    ),
    RevealedSuite(
        "confirmation",
        "eval/overfit-confirmation-v1.jsonl",
        "0c2e1e3f506d05e39864509dea1acb8ea75b60aadb616d8dbe7fbff2523e9271",
    ),
    RevealedSuite(
        "revealed-v2",
        "eval/overfit-holdout-v2.jsonl",
        "fe069a1633fbee94ed5f02cf9d75bc11c158dabedf9818c72d555b1dfa5e9361",
    ),
    RevealedSuite(
        "family",
        "eval/family-regression-v1.jsonl",
        "52206614108b18724162aa0a02c5b88681903dba4e5180fb046a0bb398e0238a",
    ),
    RevealedSuite(
        "redteam",
        "eval/redteam-prompts.jsonl",
        "fc5f88604c0b9502d28d042d9029bad9ddf251d1de07917d696145d1e08b1db5",
    ),
    RevealedSuite(
        "overlay",
        "eval/overlay-adversarial-v1.jsonl",
        "882b669bce9ac5ce379f48e4d5a197028d7bf03e57c96a4a47d85f6e17345b74",
    ),
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def artifact(path: Path) -> dict[str, Any]:
    return {
        "path": str(path.resolve()),
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        value = json.loads(line)
        if not isinstance(value, dict):
            raise ValueError(f"{path}:{line_number}: expected a JSON object")
        rows.append(value)
    return rows


def load_allowed_prompts(repo_root: Path) -> dict[str, dict[str, dict[str, Any]]]:
    allowed: dict[str, dict[str, dict[str, Any]]] = {}
    for suite in REVEALED_SUITES:
        path = repo_root / suite.relative_path
        if not path.is_file() or sha256(path) != suite.sha256:
            raise ValueError(f"revealed suite identity mismatch: {path}")
        by_id: dict[str, dict[str, Any]] = {}
        for row in read_jsonl(path):
            prompt_id = row.get("id")
            split = row.get("split")
            if not isinstance(prompt_id, str) or not prompt_id:
                raise ValueError(f"{path}: prompt is missing a string id")
            if prompt_id in by_id:
                raise ValueError(f"{path}: duplicate prompt id {prompt_id!r}")
            if not isinstance(split, str) or not split.startswith("regression-"):
                raise ValueError(f"{path}: only regression-* rows are allowed")
            by_id[prompt_id] = row
        allowed[suite.key] = by_id
    return allowed


def validate_candidate_export(
    value: dict[str, Any], allowed: dict[str, dict[str, dict[str, Any]]]
) -> list[dict[str, Any]]:
    if value.get("schema_version") != 1 or value.get("depth") != 100:
        raise ValueError("candidate export must be nonempty schema v1 at depth 100")
    rows = value.get("rows")
    if not isinstance(rows, list) or not rows:
        raise ValueError("candidate export rows must be a nonempty array")
    seen: set[tuple[str, str]] = set()
    for row in rows:
        suite = row.get("suite")
        prompt = row.get("prompt")
        primary = row.get("primary")
        if suite not in allowed or not isinstance(prompt, dict):
            raise ValueError(f"candidate export contains disallowed suite {suite!r}")
        prompt_id = prompt.get("id")
        key = (suite, prompt_id)
        if key in seen:
            raise ValueError(f"duplicate candidate row {suite}/{prompt_id}")
        seen.add(key)
        expected = allowed[suite].get(prompt_id)
        if expected is None:
            raise ValueError(f"candidate export contains unknown id {suite}/{prompt_id}")
        # These are the only prompt properties visible to scoring.  Gold,
        # must-not, family, category, and expected-output fields are never read
        # below; they pass through inside `prompt` for the canonical evaluator.
        if prompt.get("prompt") != expected.get("prompt") or prompt.get("split") != expected.get("split"):
            raise ValueError(f"prompt bytes/split mismatch for {suite}/{prompt_id}")
        results = primary.get("results") if isinstance(primary, dict) else None
        if not isinstance(results, list) or len(results) != 100:
            raise ValueError(f"{suite}/{prompt_id}: expected exactly 100 candidates")
        references = [result.get("ref") for result in results if isinstance(result, dict)]
        if (
            len(references) != 100
            or any(not isinstance(reference, str) or not reference for reference in references)
            or len(set(references)) != 100
        ):
            raise ValueError(f"{suite}/{prompt_id}: candidate refs must be 100 unique strings")
    expected_keys = {
        (suite, prompt_id) for suite, prompts in allowed.items() for prompt_id in prompts
    }
    if seen != expected_keys:
        missing = sorted(expected_keys - seen)
        extra = sorted(seen - expected_keys)
        raise ValueError(f"candidate export coverage mismatch; missing={missing[:3]} extra={extra[:3]}")
    return rows


def load_canonical_text(path: Path) -> dict[str, str]:
    output: dict[str, str] = {}
    for row in read_jsonl(path):
        reference = row.get("ref")
        text = row.get("text")
        if not isinstance(reference, str) or not isinstance(text, str):
            raise ValueError(f"{path}: canonical row lacks ref/text strings")
        if reference in output:
            raise ValueError(f"{path}: duplicate canonical ref {reference}")
        output[reference] = text
    if len(output) != 31_086:
        raise ValueError(f"{path}: expected 31,086 canonical BSB rows, got {len(output)}")
    return output


def stable_descending(scores: Any) -> Any:
    # NumPy's stable sort makes equal-score ordering inherit product order.
    import numpy as np

    return np.argsort(-scores, kind="stable")


def rrf60_order(cross_order: Any, count: int) -> Any:
    """Predeclared equal-weight rank fusion, independent of prompt/gold data."""

    import numpy as np

    product_rank = np.arange(count, dtype=np.float64)
    cross_rank = np.empty(count, dtype=np.float64)
    cross_rank[cross_order] = np.arange(count, dtype=np.float64)
    scores = 1.0 / (60.0 + product_rank + 1.0) + 1.0 / (
        60.0 + cross_rank + 1.0
    )
    return stable_descending(scores)


def percentile(values: Iterable[float], percentile_value: float) -> float:
    import numpy as np

    return float(np.percentile(list(values), percentile_value))


class OnnxCrossEncoder:
    def __init__(self, model_dir: Path, threads: int, max_length: int):
        import onnxruntime as ort
        from tokenizers import Tokenizer

        options = ort.SessionOptions()
        options.intra_op_num_threads = threads
        options.inter_op_num_threads = 1
        options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
        self.session = ort.InferenceSession(
            str(model_dir / "model.onnx"),
            sess_options=options,
            providers=["CPUExecutionProvider"],
        )
        self.tokenizer = Tokenizer.from_file(str(model_dir / "tokenizer.json"))
        self.tokenizer.enable_truncation(max_length=max_length)
        self.tokenizer.enable_padding()

    def score(self, query: str, passages: list[str]) -> Any:
        import numpy as np

        encodings = self.tokenizer.encode_batch([(query, passage) for passage in passages])
        inputs = {
            "input_ids": np.asarray([encoding.ids for encoding in encodings], dtype=np.int64),
            "attention_mask": np.asarray(
                [encoding.attention_mask for encoding in encodings], dtype=np.int64
            ),
            "token_type_ids": np.asarray(
                [encoding.type_ids for encoding in encodings], dtype=np.int64
            ),
        }
        return self.session.run(["logits"], inputs)[0].reshape(-1)


def parse_model(value: str) -> tuple[str, Path]:
    name, separator, raw_path = value.partition("=")
    if not separator or not name or not raw_path or any(ch.isspace() for ch in name):
        raise argparse.ArgumentTypeError("--model must be NAME=/path/to/model-directory")
    return name, Path(raw_path)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--candidate-export", type=Path, required=True)
    parser.add_argument("--canonical", type=Path, default=Path("data/bsb.jsonl"))
    parser.add_argument("--model", action="append", type=parse_model, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--max-length", type=int, default=128)
    args = parser.parse_args()

    if args.threads <= 0 or args.max_length <= 0:
        raise SystemExit("threads and max-length must be positive")
    repo_root = args.repo_root.resolve()
    candidate_path = args.candidate_export.resolve()
    canonical_path = (
        args.canonical.resolve()
        if args.canonical.is_absolute()
        else (repo_root / args.canonical).resolve()
    )
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    try:
        allowed = load_allowed_prompts(repo_root)
        candidate_value = json.loads(candidate_path.read_text(encoding="utf-8"))
        rows = validate_candidate_export(candidate_value, allowed)
        canonical = load_canonical_text(canonical_path)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        raise SystemExit(str(error)) from error

    model_artifacts = []
    for model_name, unresolved_model_dir in args.model:
        model_dir = unresolved_model_dir.resolve()
        model_path = model_dir / "model.onnx"
        tokenizer_path = model_dir / "tokenizer.json"
        config_path = model_dir / "config.json"
        if not all(path.is_file() for path in (model_path, tokenizer_path, config_path)):
            raise SystemExit(f"{model_dir}: expected model.onnx, tokenizer.json, config.json")

        reranker = OnnxCrossEncoder(model_dir, args.threads, args.max_length)
        first = rows[0]
        first_result = first["primary"]["results"][0]
        reranker.score(first["prompt"]["prompt"], [canonical[first_result["ref"]]])

        pair_rows = []
        live_rows = []
        live_by_suite: dict[str, list[dict[str, Any]]] = {
            suite.key: [] for suite in REVEALED_SUITES
        }
        elapsed_ms: list[float] = []
        by_suite: dict[str, list[float]] = {suite.key: [] for suite in REVEALED_SUITES}
        for row in rows:
            prompt_text = row["prompt"]["prompt"]
            product_results = row["primary"]["results"]
            try:
                passages = [canonical[result["ref"]] for result in product_results]
            except KeyError as error:
                raise SystemExit(f"candidate ref is absent from canonical BSB: {error}") from error

            started = time.perf_counter()
            scores = reranker.score(prompt_text, passages)
            duration_ms = (time.perf_counter() - started) * 1000.0
            elapsed_ms.append(duration_ms)
            by_suite[row["suite"]].append(duration_ms)
            cross_order = stable_descending(scores)
            fused_order = rrf60_order(cross_order, len(product_results))
            cross_results = [product_results[int(index)] for index in cross_order]
            fused_results = [product_results[int(index)] for index in fused_order]

            pair_rows.append(
                {
                    "suite": row["suite"],
                    "prompt": row["prompt"],
                    "primary": row["primary"],
                    "auxiliary": {"query": row["primary"]["query"], "results": cross_results},
                }
            )
            for tool, results in (
                ("product-top100", product_results),
                (f"{model_name}-cross-top100", cross_results),
                (f"{model_name}-rrf60-top100", fused_results),
            ):
                live_row = {
                    "id": row["prompt"]["id"],
                    "tool": tool,
                    "top_refs": [result["ref"] for result in results],
                }
                live_rows.append({"suite": row["suite"], **live_row})
                live_by_suite[row["suite"]].append(live_row)

        pair_path = output_dir / f"{model_name}-pair-export.json"
        pair_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "depth": 100,
                    "primary_dense": candidate_value.get("primary_dense", "product-top100"),
                    "auxiliary_dense": model_name,
                    "rows": pair_rows,
                },
                separators=(",", ":"),
            ),
            encoding="utf-8",
        )
        live_path = output_dir / f"{model_name}-live-results.jsonl"
        live_path.write_text(
            "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in live_rows),
            encoding="utf-8",
        )
        suite_live_artifacts = []
        for suite, suite_rows in live_by_suite.items():
            suite_live_path = output_dir / f"{model_name}-{suite}-live-results.jsonl"
            suite_live_path.write_text(
                "".join(
                    json.dumps(row, separators=(",", ":")) + "\n"
                    for row in suite_rows
                ),
                encoding="utf-8",
            )
            suite_live_artifacts.append({"suite": suite, **artifact(suite_live_path)})
        timing_path = output_dir / f"{model_name}-timings.json"
        timing_path.write_text(
            json.dumps(
                {
                    "queries": len(elapsed_ms),
                    "candidates_per_query": 100,
                    "threads": args.threads,
                    "max_length": args.max_length,
                    "milliseconds": {
                        "mean": statistics.fmean(elapsed_ms),
                        "median": statistics.median(elapsed_ms),
                        "p95": percentile(elapsed_ms, 95),
                        "minimum": min(elapsed_ms),
                        "maximum": max(elapsed_ms),
                    },
                    "by_suite_mean": {
                        suite: statistics.fmean(values) for suite, values in by_suite.items()
                    },
                },
                indent=2,
            )
            + "\n",
            encoding="utf-8",
        )
        model_artifacts.append(
            {
                "name": model_name,
                "model": artifact(model_path),
                "tokenizer": artifact(tokenizer_path),
                "config": artifact(config_path),
                "pair_export": artifact(pair_path),
                "live_results": artifact(live_path),
                "suite_live_results": suite_live_artifacts,
                "timings": artifact(timing_path),
            }
        )

    manifest_path = output_dir / "manifest.json"
    manifest_path.write_text(
        json.dumps(
            {
                "schema": "verse-recall-cross-encoder-reranker-bakeoff-v1",
                "protocol": {
                    "candidate_depth": 100,
                    "candidate_text": "canonical BSB verse text",
                    "max_length": args.max_length,
                    "threads": args.threads,
                    "fusion_comparator": "equal-weight rank-only RRF, k=60",
                    "gold_visible_to_scoring": False,
                },
                "candidate_export": artifact(candidate_path),
                "canonical": artifact(canonical_path),
                "revealed_suites": [
                    {
                        "key": suite.key,
                        "path": suite.relative_path,
                        "sha256": suite.sha256,
                        "prompts": len(allowed[suite.key]),
                    }
                    for suite in REVEALED_SUITES
                ],
                "models": model_artifacts,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()

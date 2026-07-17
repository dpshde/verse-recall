#!/usr/bin/env python3
"""Evaluate one verse-only encoder on the frozen revealed regression suites.

This is deliberately not a sealed-holdout runner.  The prompt inputs are an
allowlist of eight already-revealed compatibility suites with pinned SHA-256
identities.  The script never discovers prompt files with a glob and refuses
any row whose split is not ``regression-*``.

The output includes the fused product system and the raw dense diagnostic at
R@1/3/10/50/100.  It is candidate-selection evidence only; a candidate chosen
with these results still requires a fresh family-disjoint sealed holdout.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import subprocess
from typing import Any, Iterable

try:
    from scripts.embedding_space_contract import validate_envelope
except ModuleNotFoundError:  # Direct ``python scripts/...`` execution.
    from embedding_space_contract import validate_envelope


QUERY_SEGMENT_PLANNER = "verse-recall-clause-v2"


@dataclass(frozen=True)
class RevealedSuite:
    key: str
    label: str
    relative_path: str
    sha256: str


# Compatibility corpora are pinned rather than discovered.  In particular,
# never replace this with ``eval/*.jsonl``: that could silently ingest a future
# sealed holdout before candidate selection is frozen.
REVEALED_SUITES = (
    RevealedSuite(
        "semantic",
        "Semantic quality v2",
        "eval/semantic-quality-v2.jsonl",
        "f47e0d037c423a8c199f0d7733a5d5afec2f57fa0e62cfe879ba7c95b010915d",
    ),
    RevealedSuite(
        "tot",
        "Tip of the tongue",
        "eval/tot-prompts.jsonl",
        "1f1c626cd8650e21e3c85f559f84040b2818d77dd67bced707db3f890a208918",
    ),
    RevealedSuite(
        "diagnostic",
        "Overfit diagnostic",
        "eval/overfit-diagnostic-v1.jsonl",
        "acd45841777e012cc8f7e58e042a216f4e9e9e02a8a39aecabf8c2288442f4fa",
    ),
    RevealedSuite(
        "confirmation",
        "Overfit confirmation",
        "eval/overfit-confirmation-v1.jsonl",
        "0c2e1e3f506d05e39864509dea1acb8ea75b60aadb616d8dbe7fbff2523e9271",
    ),
    RevealedSuite(
        "overfit-v2",
        "Retired overfit v2",
        "eval/overfit-holdout-v2.jsonl",
        "fe069a1633fbee94ed5f02cf9d75bc11c158dabedf9818c72d555b1dfa5e9361",
    ),
    RevealedSuite(
        "family",
        "Family regression",
        "eval/family-regression-v1.jsonl",
        "52206614108b18724162aa0a02c5b88681903dba4e5180fb046a0bb398e0238a",
    ),
    RevealedSuite(
        "redteam",
        "Red-team regression",
        "eval/redteam-prompts.jsonl",
        "fc5f88604c0b9502d28d042d9029bad9ddf251d1de07917d696145d1e08b1db5",
    ),
    RevealedSuite(
        "overlay",
        "Overlay adversarial",
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


def artifact_identity(path: Path) -> dict[str, Any]:
    return {
        "path": str(path.resolve()),
        "bytes": path.stat().st_size,
        "sha256": sha256(path),
    }


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            row = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"{path}:{line_number}: invalid JSON: {error}") from error
        if not isinstance(row, dict):
            raise ValueError(f"{path}:{line_number}: expected a JSON object")
        rows.append(row)
    if not rows:
        raise ValueError(f"{path}: contains no prompt rows")
    return rows


def validate_revealed_suite(repo_root: Path, suite: RevealedSuite) -> tuple[Path, int]:
    path = repo_root / suite.relative_path
    if not path.is_file():
        raise ValueError(f"missing revealed suite: {path}")
    actual_sha = sha256(path)
    if actual_sha != suite.sha256:
        raise ValueError(
            f"revealed suite identity changed for {path}: expected {suite.sha256}, "
            f"got {actual_sha}"
        )
    rows = read_jsonl(path)
    ids: set[str] = set()
    for row_number, row in enumerate(rows, 1):
        prompt_id = row.get("id")
        prompt = row.get("prompt")
        split = row.get("split")
        if not isinstance(prompt_id, str) or not prompt_id:
            raise ValueError(f"{path}:{row_number}: missing string id")
        if prompt_id in ids:
            raise ValueError(f"{path}:{row_number}: duplicate id {prompt_id!r}")
        ids.add(prompt_id)
        if not isinstance(prompt, str) or not prompt:
            raise ValueError(f"{path}:{row_number}: missing string prompt")
        if not isinstance(split, str) or not split.startswith("regression-"):
            raise ValueError(
                f"{path}:{row_number}: encoder bake-off accepts only regression-* splits"
            )
    return path, len(rows)


def validate_corpus_vectors(path: Path, dims: int, expected_rows: int) -> None:
    if not path.is_file():
        raise ValueError(f"missing corpus vectors: {path}")
    expected_bytes = expected_rows * dims * 4
    actual_bytes = path.stat().st_size
    if actual_bytes != expected_bytes:
        raise ValueError(
            f"corpus vector shape mismatch: expected {expected_rows}x{dims} f32 "
            f"({expected_bytes} bytes), got {actual_bytes} bytes"
        )


def run(command: list[str]) -> None:
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL)


def get_system(report: dict[str, Any], name: str) -> dict[str, Any]:
    systems = report.get("systems")
    if not isinstance(systems, list):
        raise ValueError("evaluator report has no systems array")
    matches = [system for system in systems if system.get("name") == name]
    if len(matches) != 1:
        raise ValueError(f"evaluator report must contain exactly one {name!r} system")
    return matches[0]


def weighted_average(values: Iterable[tuple[float, int]]) -> float:
    values = list(values)
    denominator = sum(weight for _value, weight in values)
    return (
        sum(value * weight for value, weight in values) / denominator
        if denominator
        else 0.0
    )


def aggregate_system(
    suite_reports: list[tuple[RevealedSuite, dict[str, Any]]], system_name: str
) -> dict[str, Any]:
    systems = [(suite, get_system(report, system_name)) for suite, report in suite_reports]
    overalls = [system["overall"] for _suite, system in systems]

    count_fields = (
        "queries",
        "retrieval_queries",
        "no_gold_queries",
        "hits_at_1",
        "hits_at_3",
        "hits_at_10",
        "hits_at_50",
        "hits_at_100",
        "safe_no_gold",
        "safety_queries",
        "safe_confidence_queries",
        "passage_queries",
        "passage_passes",
        "hard_negative_queries",
        "hard_negative_passes",
        "confidence_label_queries",
        "confidence_label_passes",
    )
    totals = {
        field: sum(int(overall.get(field, 0)) for overall in overalls)
        for field in count_fields
    }
    retrieval = totals["retrieval_queries"]

    def retrieval_rate(hit_field: str) -> float:
        return totals[hit_field] / retrieval if retrieval else 0.0

    def check_rate(pass_field: str, query_field: str) -> float | None:
        denominator = totals[query_field]
        return totals[pass_field] / denominator if denominator else None

    suite_macro = {
        cutoff: sum(float(overall[f"recall_at_{cutoff}"]) for overall in overalls)
        / len(overalls)
        for cutoff in (1, 3, 10, 50, 100)
    }
    positive_families = []
    positive_categories = []
    for suite, system in systems:
        for family, metrics in system["by_family"].items():
            if metrics["retrieval_queries"]:
                positive_families.append((suite.key, family, metrics))
        for category, metrics in system["by_category"].items():
            if metrics["retrieval_queries"]:
                positive_categories.append((suite.key, category, metrics))

    return {
        **totals,
        "recall_at_1": retrieval_rate("hits_at_1"),
        "recall_at_3": retrieval_rate("hits_at_3"),
        "recall_at_10": retrieval_rate("hits_at_10"),
        "recall_at_50": retrieval_rate("hits_at_50"),
        "recall_at_100": retrieval_rate("hits_at_100"),
        "mrr": weighted_average(
            (float(overall["mrr"]), int(overall["retrieval_queries"]))
            for overall in overalls
        ),
        "no_gold_safety": check_rate("safe_no_gold", "no_gold_queries"),
        "confidence_safety": check_rate(
            "safe_confidence_queries", "safety_queries"
        ),
        "passage_accuracy": check_rate("passage_passes", "passage_queries"),
        "hard_negative_accuracy": check_rate(
            "hard_negative_passes", "hard_negative_queries"
        ),
        "confidence_label_accuracy": check_rate(
            "confidence_label_passes", "confidence_label_queries"
        ),
        "suite_macro_recall": suite_macro,
        "positive_family_slices": len(positive_families),
        "family_macro_recall_at_3": sum(
            float(metrics["recall_at_3"]) for _suite, _family, metrics in positive_families
        )
        / len(positive_families),
        "family_macro_recall_at_10": sum(
            float(metrics["recall_at_10"])
            for _suite, _family, metrics in positive_families
        )
        / len(positive_families),
        "positive_category_slices": len(positive_categories),
        "category_macro_recall_at_3": sum(
            float(metrics["recall_at_3"])
            for _suite, _category, metrics in positive_categories
        )
        / len(positive_categories),
        "category_macro_recall_at_10": sum(
            float(metrics["recall_at_10"])
            for _suite, _category, metrics in positive_categories
        )
        / len(positive_categories),
    }


def percent(value: float | None) -> str:
    return "n/a" if value is None else f"{value * 100:.1f}%"


def render_markdown(study: dict[str, Any]) -> str:
    lines = [
        f"# {study['candidate']['name']} — revealed encoder bake-off",
        "",
        "This compares one frozen verse-only representation on eight already-revealed "
        "regression suites. It is compatibility and candidate-selection evidence, "
        "**not a sealed generalization result**. No holdout is read or evaluated by "
        "this harness.",
        "",
        "## Aggregate",
        "",
        "| System | R@1 | R@3 | R@10 | R@50 | R@100 | MRR | Suite-macro R@3 | Family-macro R@3 | Safety | Hard negatives | Labels |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for system in study["systems"]:
        metrics = system["aggregate"]
        lines.append(
            f"| {system['label']} | {percent(metrics['recall_at_1'])} | "
            f"{percent(metrics['recall_at_3'])} | {percent(metrics['recall_at_10'])} | "
            f"{percent(metrics['recall_at_50'])} | {percent(metrics['recall_at_100'])} | "
            f"{metrics['mrr']:.3f} | "
            f"{percent(metrics['suite_macro_recall'][3])} | "
            f"{percent(metrics['family_macro_recall_at_3'])} | "
            f"{percent(metrics['confidence_safety'])} | "
            f"{percent(metrics['hard_negative_accuracy'])} | "
            f"{percent(metrics['confidence_label_accuracy'])} |"
        )

    lines.extend(
        [
            "",
            "The raw dense row isolates encoder coverage; the fused row measures the "
            "unchanged product engine with lexical, expansion, translation/knowledge, "
            "and candidate dense lanes. R@50/R@100 are diagnostics, not release bars.",
            "",
            "## Per suite",
            "",
            "| Suite | Queries | Fused R@1 | Fused R@3 | Fused R@10 | Dense R@3 | Dense R@10 | Dense R@100 |",
            "|---|---:|---:|---:|---:|---:|---:|---:|",
        ]
    )
    fused = next(system for system in study["systems"] if system["key"] == "fused")
    dense = next(system for system in study["systems"] if system["key"] == "dense")
    for suite in study["suites"]:
        fused_metrics = fused["by_suite"][suite["key"]]
        dense_metrics = dense["by_suite"][suite["key"]]
        lines.append(
            f"| {suite['label']} | {fused_metrics['queries']} | "
            f"{percent(fused_metrics['recall_at_1'])} | "
            f"{percent(fused_metrics['recall_at_3'])} | "
            f"{percent(fused_metrics['recall_at_10'])} | "
            f"{percent(dense_metrics['recall_at_3'])} | "
            f"{percent(dense_metrics['recall_at_10'])} | "
            f"{percent(dense_metrics['recall_at_100'])} |"
        )

    candidate = study["candidate"]
    lines.extend(
        [
            "",
            "## Reproduction identity",
            "",
            f"- Declared upstream model: `{candidate['model_repository']}` at "
            f"`{candidate['model_revision']}`.",
            f"- Embedding-space ID: `{candidate['embedding_space_id']}`.",
            f"- Model SHA-256: `{candidate['model']['sha256']}`.",
            f"- Tokenizer SHA-256: `{candidate['tokenizer']['sha256']}`.",
            f"- Corpus vectors SHA-256: `{candidate['corpus_vectors']['sha256']}` "
            f"({candidate['corpus_rows']} x {candidate['dimensions']} Float32).",
            f"- Candidate pack SHA-256: `{candidate['dense_pack']['sha256']}`.",
            "- Query vectors are bound to the exact prompt UTF-8 bytes and include "
            f"query-only ordered clause segments from `{QUERY_SEGMENT_PLANNER}`.",
            "",
            "## Interpretation guardrail",
            "",
            "Select or reject the encoder on broad suite/family behavior, safety, "
            "artifact cost, and latency—not individual misses. If selected, freeze "
            "the candidate and evaluate it exactly once on a fresh family-disjoint "
            "holdout. Do not patch revealed misses into runtime rules or data.",
            "",
        ]
    )
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path.cwd())
    parser.add_argument("--python", dest="python_executable", type=Path, required=True)
    parser.add_argument("--model-dir", type=Path, required=True)
    parser.add_argument("--corpus-vectors", type=Path, required=True)
    parser.add_argument("--dimensions", type=int, required=True)
    parser.add_argument("--pooling", choices=("cls", "mean"), required=True)
    parser.add_argument("--max-length", type=int, required=True)
    parser.add_argument("--batch-size", type=int, default=64)
    parser.add_argument("--query-prefix", default="")
    parser.add_argument("--corpus-prefix", default="")
    parser.add_argument("--model-repository", required=True)
    parser.add_argument("--model-revision", required=True)
    parser.add_argument("--candidate-name", required=True)
    parser.add_argument("--expected-model-sha256", required=True)
    parser.add_argument("--expected-tokenizer-sha256", required=True)
    parser.add_argument("--base", type=Path, default=Path("data/bsb-full.pack"))
    parser.add_argument("--expand", type=Path, default=Path("data/bsb-expand.pack"))
    parser.add_argument(
        "--knowledge", type=Path, default=Path("data/recall-overlay.pack")
    )
    parser.add_argument(
        "--packer", type=Path, default=Path("target/release/recall-packer")
    )
    parser.add_argument(
        "--evaluator", type=Path, default=Path("target/release/recall-eval")
    )
    parser.add_argument("--top-k", type=int, default=100, choices=(100,))
    parser.add_argument("--output-dir", type=Path, required=True)
    return parser.parse_args()


def resolved_under(root: Path, path: Path) -> Path:
    return path if path.is_absolute() else root / path


def interpreter_path(path: Path, working_directory: Path) -> Path:
    """Return an absolute launcher path without dereferencing environment symlinks.

    Python virtual environments and cached uv archives commonly expose an
    interpreter symlink whose location selects the dependency environment.
    ``Path.resolve()`` would jump to the base interpreter and lose those
    dependencies even though the user supplied the correct launcher.
    """

    expanded = path.expanduser()
    return expanded if expanded.is_absolute() else working_directory / expanded


def main() -> None:
    args = parse_args()
    repo_root = args.repo_root.resolve()
    if args.dimensions <= 0 or args.max_length <= 0 or args.batch_size <= 0:
        raise SystemExit("dimensions, max-length, and batch-size must be positive")

    python_executable = interpreter_path(args.python_executable, Path.cwd())
    model_dir = args.model_dir.resolve()
    corpus_vectors = args.corpus_vectors.resolve()
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    model = model_dir / "model_quantized.onnx"
    tokenizer = model_dir / "tokenizer.json"
    config = model_dir / "config.json"
    base = resolved_under(repo_root, args.base).resolve()
    expand = resolved_under(repo_root, args.expand).resolve()
    knowledge = resolved_under(repo_root, args.knowledge).resolve()
    packer = resolved_under(repo_root, args.packer).resolve()
    evaluator = resolved_under(repo_root, args.evaluator).resolve()
    semantic_builder = repo_root / "scripts/build_semantic_vectors.py"
    canonical = repo_root / "data/bsb.jsonl"

    required = (
        python_executable,
        model,
        tokenizer,
        config,
        corpus_vectors,
        base,
        expand,
        knowledge,
        packer,
        evaluator,
        semantic_builder,
        canonical,
    )
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        raise SystemExit("missing required files: " + ", ".join(missing))
    if sha256(model) != args.expected_model_sha256:
        raise SystemExit("model SHA-256 does not match --expected-model-sha256")
    if sha256(tokenizer) != args.expected_tokenizer_sha256:
        raise SystemExit("tokenizer SHA-256 does not match --expected-tokenizer-sha256")

    suites = []
    for suite in REVEALED_SUITES:
        try:
            path, row_count = validate_revealed_suite(repo_root, suite)
        except ValueError as error:
            raise SystemExit(str(error)) from error
        suites.append((suite, path, row_count))

    canonical_rows = sum(1 for line in canonical.read_text(encoding="utf-8").splitlines() if line.strip())
    try:
        validate_corpus_vectors(corpus_vectors, args.dimensions, canonical_rows)
    except ValueError as error:
        raise SystemExit(str(error)) from error

    space_path = output_dir / "embedding-space.json"
    space_command = [
        str(python_executable),
        str(semantic_builder),
        "space-id",
        "--model-dir",
        str(model_dir),
        "--pooling",
        args.pooling,
        "--max-length",
        str(args.max_length),
        "--query-prefix",
        args.query_prefix,
        "--corpus-prefix",
        args.corpus_prefix,
        "--output",
        str(space_path),
    ]
    run(space_command)
    contract, embedding_space_id = validate_envelope(
        json.loads(space_path.read_text(encoding="utf-8"))
    )

    dense_pack = output_dir / "candidate-semantic.pack"
    pack_command = [
        str(packer),
        "semantic",
        "--verse-vectors",
        str(corpus_vectors),
        "--dims",
        str(args.dimensions),
        "--output",
        str(dense_pack),
        "--translation",
        "BSB",
        "--embedding-space-id",
        embedding_space_id,
    ]
    run(pack_command)

    suite_reports: list[tuple[RevealedSuite, dict[str, Any]]] = []
    suite_output = []
    reproduction_commands = [space_command, pack_command]
    for suite, prompts, prompt_count in suites:
        query_path = output_dir / f"{suite.key}-query.jsonl"
        query_command = [
            str(python_executable),
            str(semantic_builder),
            "queries",
            "--input",
            str(prompts.resolve()),
            "--output",
            str(query_path),
            "--model-dir",
            str(model_dir),
            "--pooling",
            args.pooling,
            "--max-length",
            str(args.max_length),
            "--batch-size",
            str(args.batch_size),
            "--query-prefix",
            args.query_prefix,
            "--segment-embeddings",
        ]
        run(query_command)
        report_path = output_dir / f"{suite.key}-report.md"
        evaluation_command = [
            str(evaluator),
            "--pack",
            str(base),
            "--overlay",
            str(expand),
            "--overlay",
            str(knowledge),
            "--overlay",
            str(dense_pack),
            "--prompts",
            str(prompts.resolve()),
            "--query-embeddings",
            str(query_path),
            "--embedding-space-id",
            embedding_space_id,
            "--dense-diagnostic",
            "--baseline",
            "false",
            "--top-k",
            str(args.top_k),
            "--report",
            str(report_path),
        ]
        run(evaluation_command)
        report_json = report_path.with_suffix(".json")
        report = json.loads(report_json.read_text(encoding="utf-8"))
        # Fail closed if evaluator or query builder silently omitted a prompt.
        if get_system(report, "verse-recall")["overall"]["queries"] != prompt_count:
            raise SystemExit(f"evaluator prompt count mismatch for suite {suite.key}")
        get_system(report, "dense-only-diagnostic")
        suite_reports.append((suite, report))
        suite_output.append(
            {
                "key": suite.key,
                "label": suite.label,
                "prompts": artifact_identity(prompts),
                "query_embeddings": artifact_identity(query_path),
                "evaluator_report": artifact_identity(report_json),
            }
        )
        reproduction_commands.extend((query_command, evaluation_command))

    systems = []
    for key, label, evaluator_name in (
        ("fused", "Product fusion", "verse-recall"),
        ("dense", "Raw verse-only dense", "dense-only-diagnostic"),
    ):
        systems.append(
            {
                "key": key,
                "label": label,
                "evaluator_system": evaluator_name,
                "aggregate": aggregate_system(suite_reports, evaluator_name),
                "by_suite": {
                    suite.key: get_system(report, evaluator_name)["overall"]
                    for suite, report in suite_reports
                },
            }
        )

    study = {
        "schema": "verse-recall-revealed-encoder-bakeoff-v1",
        "evidence_class": "revealed-regression-candidate-selection-only",
        "sealed_generalization_claim": False,
        "candidate": {
            "name": args.candidate_name,
            "model_repository": args.model_repository,
            "model_revision": args.model_revision,
            "model": artifact_identity(model),
            "tokenizer": artifact_identity(tokenizer),
            "config": artifact_identity(config),
            "embedding_contract": contract,
            "embedding_space_id": embedding_space_id,
            "pooling": args.pooling,
            "max_length": args.max_length,
            "query_prefix": args.query_prefix,
            "corpus_prefix": args.corpus_prefix,
            "dimensions": args.dimensions,
            "corpus_rows": canonical_rows,
            "canonical_corpus": artifact_identity(canonical),
            "corpus_vectors": artifact_identity(corpus_vectors),
            "dense_pack": artifact_identity(dense_pack),
        },
        "runtime_artifacts": {
            "evaluator": artifact_identity(evaluator),
            "packer": artifact_identity(packer),
            "base": artifact_identity(base),
            "expand": artifact_identity(expand),
            "knowledge": artifact_identity(knowledge),
        },
        "retrieval_depth": args.top_k,
        "query_segment_planner": QUERY_SEGMENT_PLANNER,
        "suites": suite_output,
        "systems": systems,
        "reproduction_commands": reproduction_commands,
    }
    json_output = output_dir / "encoder-bakeoff.json"
    markdown_output = output_dir / "encoder-bakeoff.md"
    json_output.write_text(json.dumps(study, indent=2) + "\n", encoding="utf-8")
    markdown_output.write_text(render_markdown(study), encoding="utf-8")
    print(f"wrote {markdown_output}")
    print(f"wrote {json_output}")


if __name__ == "__main__":
    main()

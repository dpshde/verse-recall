#!/usr/bin/env python3
"""Run a controlled retrieval-method ablation and render its trade-off matrix.

Every method is evaluated with the same prompt corpus and retrieval depth. The
only independent variable is the set of runtime packs/query vectors supplied to
the evaluator. The resulting JSON retains per-prompt evidence so the aggregate
table can be audited rather than taken on trust.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


QUERY_GROUPS = (
    ("remembered_wording", "Remembered wording", {"near-quote", "partial-phrase", "canonical-phrase"}),
    (
        "translation_drift",
        "Translation drift",
        {"translation-drift", "wrong-translation", "kjv-drift", "translation-hybrid"},
    ),
    ("meaning_situation", "Meaning / situation", {"meaning-only", "situation"}),
    (
        "concept_event",
        "Concept / entity / event",
        {
            "named-concept",
            "event-alias",
            "event-name",
            "topic-positive-control",
            "collision-control",
            "concept-data-error",
        },
    ),
    ("misquote", "Misquote", {"misquote", "false-memory"}),
    ("conflation", "Conflation", {"conflation", "conflated"}),
)


@dataclass(frozen=True)
class Method:
    key: str
    label: str
    role: str
    overlays: tuple[str, ...]
    query_embeddings: bool
    source_system: str = "verse-recall"
    strengths: str = ""
    weaknesses: str = ""


METHODS = (
    Method(
        "keyword",
        "Classic keyword overlap",
        "traditional baseline",
        (),
        False,
        "offline-keyword",
        "Tiny logic surface; transparent; strong when the remembered words are exact.",
        "No phrase model, morphology, translation bridge, graph, or meaning-only recall.",
    ),
    Method(
        "lexical",
        "Indexed lexical core",
        "traditional production baseline",
        (),
        False,
        strengths="Fast compact BM25-style postings; excellent near-quotes; deterministic offline behavior.",
        weaknesses="Weak on zero-overlap paraphrases, named concepts, and translation-specific wording.",
    ),
    Method(
        "expanded",
        "Lexical + expansion",
        "traditional IR extension",
        ("expand",),
        False,
        strengths="Portable weighted vocabulary expansion improves wrong-word and partial semantic recall.",
        weaknesses="Synonym neighborhoods add noise; cannot reliably ground events or deep paraphrases.",
    ),
    Method(
        "knowledge",
        "Lexical + translation/knowledge",
        "traditional rules + graph",
        ("knowledge",),
        False,
        strengths="Best direct handling of KJV drift, source-backed entities/events, corrections, and passage windows.",
        weaknesses="Sparse graph evidence does not solve general meaning similarity; source/licensing burden.",
    ),
    Method(
        "hybrid_no_dense",
        "Lexical + expansion + knowledge",
        "hybrid without vectors",
        ("expand", "knowledge"),
        False,
        strengths="Broad offline recall without an encoder; strong quotes, translation drift, and named concepts.",
        weaknesses="Semantic tail remains shallow and one hard-negative ordering failure remains in this suite.",
    ),
    Method(
        "dense_guarded",
        "Dense + lexical guard",
        "vector alternative",
        ("dense",),
        True,
        strengths="Generic zero/low-overlap meaning retrieval without passage-specific rewrite rules.",
        weaknesses="Needs an exact query encoder contract; alone it misses translation and graph advantages.",
    ),
    Method(
        "full_fusion",
        "Mode-gated full fusion",
        "release candidate",
        ("expand", "knowledge", "dense"),
        True,
        strengths="Only configuration here meeting every aggregate release bar, including semantic tail and safety.",
        weaknesses="Largest payload and memory budget; dense fusion lowers R@1 versus the no-vector hybrid; conflations remain weak.",
    ),
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def percent(value: float | None) -> str:
    return "n/a" if value is None else f"{value * 100:.1f}%"


def get_system(report: dict[str, Any], name: str) -> dict[str, Any]:
    for system in report["systems"]:
        if system["name"] == name:
            return system
    raise ValueError(f"report does not contain system {name!r}")


def summarize_rows(rows: list[dict[str, Any]]) -> dict[str, Any]:
    retrieval = [row for row in rows if row["has_gold"]]

    def nullable_rate(field: str) -> tuple[int, int, float]:
        values = [row[field] for row in rows if row[field] is not None]
        passes = sum(bool(value) for value in values)
        return len(values), passes, passes / len(values) if values else 0.0

    no_gold_n, no_gold_passes, no_gold_rate = nullable_rate("no_gold_safe")
    safety_n, safety_passes, safety_rate = nullable_rate("confidence_safe")
    passage_n, passage_passes, passage_rate = nullable_rate("passage_output_ok")
    hard_n, hard_passes, hard_rate = nullable_rate("hard_negative_ok")
    label_n, label_passes, label_rate = nullable_rate("confidence_label_ok")
    retrieval_n = len(retrieval)

    def retrieval_rate(field: str) -> float:
        return sum(bool(row[field]) for row in retrieval) / retrieval_n if retrieval else 0.0

    return {
        "queries": len(rows),
        "retrieval_queries": retrieval_n,
        "no_gold_queries": no_gold_n,
        "hits_at_1": sum(row["hit_at_1"] for row in retrieval),
        "hits_at_3": sum(row["hit_at_3"] for row in retrieval),
        "hits_at_10": sum(row["hit_at_10"] for row in retrieval),
        "recall_at_1": retrieval_rate("hit_at_1"),
        "recall_at_3": retrieval_rate("hit_at_3"),
        "recall_at_10": retrieval_rate("hit_at_10"),
        "mrr": (
            sum(row["reciprocal_rank"] for row in retrieval) / retrieval_n
            if retrieval
            else 0.0
        ),
        "safe_no_gold": no_gold_passes,
        "no_gold_safety": no_gold_rate,
        "safety_queries": safety_n,
        "safe_confidence_queries": safety_passes,
        "confidence_safety": safety_rate,
        "passage_queries": passage_n,
        "passage_passes": passage_passes,
        "passage_accuracy": passage_rate,
        "hard_negative_queries": hard_n,
        "hard_negative_passes": hard_passes,
        "hard_negative_accuracy": hard_rate,
        "confidence_label_queries": label_n,
        "confidence_label_passes": label_passes,
        "confidence_label_accuracy": label_rate,
    }


def grouped_metrics(rows: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    grouped = {}
    for key, label, categories in QUERY_GROUPS:
        selected = [
            row for row in rows if row["has_gold"] and row["category"] in categories
        ]
        if not selected:
            raise ValueError(f"query group {key} has no gold-bearing prompts")
        grouped[key] = {
            "label": label,
            "queries": len(selected),
            "recall_at_1": sum(row["hit_at_1"] for row in selected) / len(selected),
            "recall_at_3": sum(row["hit_at_3"] for row in selected) / len(selected),
            "recall_at_10": sum(row["hit_at_10"] for row in selected) / len(selected),
            "mrr": sum(row["reciprocal_rank"] for row in selected) / len(selected),
        }
    return grouped


def diversity_metrics(rows: list[dict[str, Any]]) -> dict[str, Any]:
    groups = grouped_metrics(rows)
    recalls = [group["recall_at_3"] for group in groups.values()]
    tail_recalls = [group["recall_at_10"] for group in groups.values()]
    return {
        "groups": groups,
        "positive_category_macro_recall_at_3": sum(recalls) / len(recalls),
        "positive_category_macro_recall_at_10": sum(tail_recalls) / len(tail_recalls),
        "weakest_positive_category_recall_at_3": min(recalls),
        "weakest_positive_category_recall_at_10": min(tail_recalls),
        "categories_at_or_above_80pct_recall_at_3": sum(value >= 0.8 for value in recalls),
        "positive_category_count": len(recalls),
    }


def render_markdown(study: dict[str, Any]) -> str:
    lines = [
        "# Retrieval method trade-offs — diverse-query ablation",
        "",
        f"This is a controlled ablation over {study['overall_queries']} prompts "
        f"({study['overall_retrieval_queries']} gold-bearing) pooled from "
        f"{len(study['suites'])} already-revealed frozen regression suites. It is "
        "product-regression evidence, **not** a sealed generalization claim. Every "
        "row uses the same BSB corpus and top-10 depth; only enabled retrieval "
        "lanes differ.",
        "",
        "## Overall matrix",
        "",
        "| Method | Runtime MiB | R@1 | R@3 | R@10 | Macro family R@3 | Macro family R@10 | Weakest family R@3 | Safety | Hard negatives | Encoder |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|",
    ]
    for method in study["methods"]:
        overall = method["overall"]
        diversity = method["diversity"]
        safety = (
            overall.get("confidence_safety")
            if overall.get("safety_queries", 0) > 0
            else None
        )
        lines.append(
            f"| {method['label']} | {method['artifact_bytes'] / 1048576:.2f} | "
            f"{percent(overall['recall_at_1'])} | {percent(overall['recall_at_3'])} | "
            f"{percent(overall['recall_at_10'])} | "
            f"{percent(diversity['positive_category_macro_recall_at_3'])} | "
            f"{percent(diversity['positive_category_macro_recall_at_10'])} | "
            f"{percent(diversity['weakest_positive_category_recall_at_3'])} | "
            f"{percent(safety)} | "
            f"{percent(overall.get('hard_negative_accuracy'))} | "
            f"{'BGE 384D' if method['query_embeddings'] else 'None'} |"
        )

    lines.extend(
        [
            "",
            "Macro family recall weights six product query groups equally, so "
            "a large quote or meaning slice cannot hide failures in conflations "
            "or named events. The weakest-family floor makes the "
            "remaining diversity gap explicit.",
            "",
            "## Product query-group R@3",
            "",
            "| Method | Remembered wording | Translation drift | Meaning / situation | Concept / entity / event | Misquote | Conflation |",
            "|---|---:|---:|---:|---:|---:|---:|",
        ]
    )
    for method in study["methods"]:
        values = [
            percent(method["diversity"]["groups"][key]["recall_at_3"])
            for key, _label, _categories in QUERY_GROUPS
        ]
        lines.append(f"| {method['label']} | " + " | ".join(values) + " |")

    lines.extend(
        [
            "",
            "## Per-suite R@3 / R@10",
            "",
            "| Method | "
            + " | ".join(suite["label"] for suite in study["suites"])
            + " |",
            "|---|" + "---:|" * len(study["suites"]),
        ]
    )
    for method in study["methods"]:
        cells = [
            f"{percent(method['by_suite'][suite['key']]['recall_at_3'])} / "
            f"{percent(method['by_suite'][suite['key']]['recall_at_10'])}"
            for suite in study["suites"]
        ]
        lines.append(f"| {method['label']} | " + " | ".join(cells) + " |")

    lines.extend(
        [
            "",
            "## Strengths and weaknesses",
            "",
            "| Method | Strengths | Weaknesses |",
            "|---|---|---|",
        ]
    )
    for method in study["methods"]:
        lines.append(
            f"| {method['label']} | {method['strengths']} | {method['weaknesses']} |"
        )

    winner = next(method for method in study["methods"] if method["key"] == "full_fusion")
    no_dense = next(
        method for method in study["methods"] if method["key"] == "hybrid_no_dense"
    )
    lines.extend(
        [
            "",
            "## What the evidence proves — and does not",
            "",
            f"The full fusion is the only tested configuration to reach the "
            f"aggregate release bars: R@3 {percent(winner['overall']['recall_at_3'])}, "
            f"R@10 {percent(winner['overall']['recall_at_10'])}, confidence safety "
            f"{percent(winner['overall']['confidence_safety'])}, and hard-negative "
            f"ordering {percent(winner['overall']['hard_negative_accuracy'])}. "
            f"The strongest encoder-free hybrid actually leads at R@3 with "
            f"{percent(no_dense['overall']['recall_at_3'])}, but stops at R@10 "
            f"{percent(no_dense['overall']['recall_at_10'])} and hard-negative "
            f"ordering {percent(no_dense['overall']['hard_negative_accuracy'])}. "
            "Dense retrieval is therefore buying semantic-tail coverage and safer "
            "ordering here—not a universal head-rank win.",
            "",
            f"The trade is real: the encoder-free hybrid has R@1 "
            f"{percent(no_dense['overall']['recall_at_1'])}, versus "
            f"{percent(winner['overall']['recall_at_1'])} for full fusion, while "
            "the full payload is larger. Conflation R@3 remains only "
            f"{percent(winner['diversity']['groups']['conflation']['recall_at_3'])}; this is "
            "the current weakest-family floor and the clearest next red-team target.",
            "",
            "Recommended deployment: ship the modular encoder-free recall tier where "
            "download/encoder budgets dominate, and the full mode-gated tier where "
            "diverse-query recall is the priority. Do not replace the lexical and "
            "knowledge lanes with vectors: the dense-only ablation shows why the "
            "lanes are complementary.",
            "",
            "## Reproduction identity",
            "",
            f"- Embedding-space ID: `{study['embedding_space_id']}`",
            "- Evaluator: release build of `recall-eval`; top-10 retrieval; offline; no network calls.",
            "- Full per-prompt evidence and pack identities are retained in `method-tradeoffs-v1.json`.",
            "",
        ]
    )
    for suite in study["suites"]:
        lines.insert(
            -3,
            f"- {suite['label']}: prompts `{suite['prompts']['sha256']}`, query vectors `{suite['query_embeddings']['sha256']}`",
        )
    return "\n".join(lines)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evaluator", type=Path, default=Path("target/release/recall-eval"))
    parser.add_argument("--base", type=Path, default=Path("data/bsb-full.pack"))
    parser.add_argument("--expand", type=Path, default=Path("data/bsb-expand.pack"))
    parser.add_argument("--knowledge", type=Path, default=Path("data/recall-overlay.pack"))
    parser.add_argument("--dense", type=Path, default=Path("data/bsb-semantic.pack"))
    parser.add_argument("--prompts", type=Path, default=Path("eval/semantic-quality-v2.jsonl"))
    parser.add_argument(
        "--query-embeddings",
        type=Path,
        default=Path("/private/tmp/bge-semantic-v2-query.jsonl"),
    )
    parser.add_argument(
        "--suite",
        action="append",
        default=[],
        metavar="KEY=LABEL=PROMPTS=QUERY_EMBEDDINGS",
        help="repeat to pool multiple frozen suites; paths must not contain '='",
    )
    parser.add_argument(
        "--embedding-space-id",
        default="8744273cd632ff40dc59a2c17bae871a5eebf8ed51a8e96b815c5c65c9672225",
    )
    parser.add_argument("--output", type=Path, default=Path("eval/method-tradeoffs-v1.md"))
    parser.add_argument(
        "--json-output", type=Path, default=Path("eval/method-tradeoffs-v1.json")
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    packs = {
        "base": args.base,
        "expand": args.expand,
        "knowledge": args.knowledge,
        "dense": args.dense,
    }
    suites = []
    if args.suite:
        for specification in args.suite:
            parts = specification.split("=", 3)
            if len(parts) != 4:
                raise SystemExit(
                    "--suite must be KEY=LABEL=PROMPTS=QUERY_EMBEDDINGS"
                )
            key, label, prompts, query_embeddings = parts
            if (
                not key
                or not label
                or any(character not in "abcdefghijklmnopqrstuvwxyz0123456789-_" for character in key)
            ):
                raise SystemExit(f"invalid suite key or label: {specification}")
            suites.append(
                {
                    "key": key,
                    "label": label,
                    "prompts": Path(prompts),
                    "query_embeddings": Path(query_embeddings),
                }
            )
    else:
        suites.append(
            {
                "key": "semantic-quality-v2",
                "label": "Semantic quality v2",
                "prompts": args.prompts,
                "query_embeddings": args.query_embeddings,
            }
        )
    if len({suite["key"] for suite in suites}) != len(suites):
        raise SystemExit("suite keys must be unique")

    required = [args.evaluator, *packs.values()]
    required.extend(suite["prompts"] for suite in suites)
    required.extend(suite["query_embeddings"] for suite in suites)
    missing = [str(path) for path in required if not path.is_file()]
    if missing:
        raise SystemExit("missing required files: " + ", ".join(missing))

    artifact_identity = {
        name: {"path": str(path), "bytes": path.stat().st_size, "sha256": sha256(path)}
        for name, path in packs.items()
    }
    reports: dict[tuple[str, str], dict[str, Any]] = {}
    with tempfile.TemporaryDirectory(prefix="verse-recall-ablation-") as temporary:
        temp = Path(temporary)
        for suite in suites:
            for method in METHODS:
                # The keyword row is emitted by the same base-only invocation as
                # the indexed lexical row, guaranteeing identical input/depth.
                if method.key == "keyword":
                    continue
                report_path = temp / f"{suite['key']}-{method.key}.md"
                command = [
                    str(args.evaluator),
                    "--pack",
                    str(args.base),
                    "--prompts",
                    str(suite["prompts"]),
                    "--baseline",
                    "true" if method.key == "lexical" else "false",
                    "--report",
                    str(report_path),
                ]
                for overlay in method.overlays:
                    command.extend(("--overlay", str(packs[overlay])))
                if method.query_embeddings:
                    command.extend(
                        (
                            "--query-embeddings",
                            str(suite["query_embeddings"]),
                            "--embedding-space-id",
                            args.embedding_space_id,
                        )
                    )
                subprocess.run(command, check=True, stdout=subprocess.DEVNULL)
                report = json.loads(
                    report_path.with_suffix(".json").read_text(encoding="utf-8")
                )
                reports[(suite["key"], method.key)] = report
                if method.key == "lexical":
                    reports[(suite["key"], "keyword")] = report

    rendered_methods: list[dict[str, Any]] = []
    for method in METHODS:
        systems = {
            suite["key"]: get_system(
                reports[(suite["key"], method.key)], method.source_system
            )
            for suite in suites
        }
        rows = []
        for suite in suites:
            for original in systems[suite["key"]]["per_prompt"]:
                row = dict(original)
                row["suite"] = suite["key"]
                rows.append(row)
        by_category = {
            category: summarize_rows(
                [row for row in rows if row["category"] == category]
            )
            for category in sorted({row["category"] for row in rows})
        }
        included = ("base", *method.overlays)
        rendered_methods.append(
            {
                "key": method.key,
                "label": method.label,
                "role": method.role,
                "included_artifacts": list(included),
                "artifact_bytes": sum(artifact_identity[name]["bytes"] for name in included),
                "query_embeddings": method.query_embeddings,
                "strengths": method.strengths,
                "weaknesses": method.weaknesses,
                "overall": summarize_rows(rows),
                "diversity": diversity_metrics(rows),
                "by_suite": {
                    suite["key"]: systems[suite["key"]]["overall"]
                    for suite in suites
                },
                "by_category": by_category,
                "per_prompt": rows,
            }
        )

    suite_identity = [
        {
            "key": suite["key"],
            "label": suite["label"],
            "prompts": {
                "path": str(suite["prompts"]),
                "bytes": suite["prompts"].stat().st_size,
                "sha256": sha256(suite["prompts"]),
            },
            "query_embeddings": {
                "path": str(suite["query_embeddings"]),
                "bytes": suite["query_embeddings"].stat().st_size,
                "sha256": sha256(suite["query_embeddings"]),
            },
        }
        for suite in suites
    ]
    study = {
        "schema": "verse-recall-method-tradeoffs-v1",
        "suites": suite_identity,
        "overall_queries": rendered_methods[-1]["overall"]["queries"],
        "overall_retrieval_queries": rendered_methods[-1]["overall"]["retrieval_queries"],
        "embedding_space_id": args.embedding_space_id,
        "artifacts": artifact_identity,
        "query_groups": [
            {"key": key, "label": label, "categories": sorted(categories)}
            for key, label, categories in QUERY_GROUPS
        ],
        "methods": rendered_methods,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.json_output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(render_markdown(study), encoding="utf-8")
    args.json_output.write_text(json.dumps(study, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {args.output}")
    print(f"wrote {args.json_output}")


if __name__ == "__main__":
    main()

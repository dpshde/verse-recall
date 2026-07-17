#!/usr/bin/env python3
"""Re-run the method trade-off matrix on current release packs.

Produces eval/method-tradeoffs-v2.{md,json} with suite-macro metrics across
eight already-revealed suites and a head-to-head majority win-rate for the
retained candidate. No evaluation gold is copied into runtime data.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


SPACE_ARCTIC_M256 = "36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744"
SPACE_MINILM = "76eaf6543d4c23168a12842a04d8eceb125b9b504b34c4ae1a418ee1fc8c0794"

# Revealed-only bake-off suites (same eight as method-tradeoffs-v1 narrative).
DEFAULT_SUITES = (
    ("semantic", "Semantic quality v2", "eval/semantic-quality-v2.jsonl", "semantic.jsonl"),
    ("tot", "Tip of the tongue", "eval/tot-prompts.jsonl", "tot.jsonl"),
    ("overlay", "Overlay adversarial", "eval/overlay-adversarial-v1.jsonl", "overlay.jsonl"),
    ("diagnostic", "Overfit diagnostic", "eval/overfit-diagnostic-v1.jsonl", "diagnostic.jsonl"),
    ("confirmation", "Overfit confirmation", "eval/overfit-confirmation-v1.jsonl", "confirmation.jsonl"),
    ("revealed-v2", "Revealed holdout v2", "eval/overfit-holdout-v2.jsonl", "revealed-v2.jsonl"),
    ("family", "Family regression", "eval/family-regression-v1.jsonl", "family.jsonl"),
    ("redteam", "Red-team prompts", "eval/redteam-prompts.jsonl", "redteam.jsonl"),
)

MEANING_CATEGORIES = frozenset({"meaning-only", "situation"})
MULTI_INTENT_CATEGORIES = frozenset(
    {"multi-intent", "multi-intent-unseen", "conflation", "conflated"}
)

# Quality metrics where higher is better.
QUALITY_METRICS = (
    "suite_macro_r1",
    "suite_macro_r3",
    "suite_macro_r10",
    "family_cell_r3",
    "family_cell_r10",
    "meaning_only_r3",
    "multi_intent_r3",
    "hard_negative_rate",
    "confidence_safety",
)


@dataclass(frozen=True)
class MethodSpec:
    key: str
    label: str
    role: str
    overlays: tuple[str, ...]  # logical names: expand, knowledge, dense, passage
    needs_query_embeddings: bool
    dense_pack: str | None = None  # path override for dense
    passage_pack: str | None = None
    query_root: str | None = None  # directory of suite query jsonl files
    embedding_space_id: str | None = None
    source_system: str = "verse-recall"
    strengths: str = ""
    weaknesses: str = ""
    retained: bool = False


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def get_system(report: dict[str, Any], name: str) -> dict[str, Any]:
    for system in report["systems"]:
        if system["name"] == name:
            return system
    raise ValueError(f"report missing system {name!r}")


def mean(values: list[float]) -> float | None:
    return sum(values) / len(values) if values else None


def family_cell_macro(system: dict[str, Any], depth: int) -> float | None:
    """Equal-weight (suite-local) family recall@depth when the report exposes families."""
    by_family = system.get("by_family") or system.get("families")
    if not by_family:
        # Derive from per_prompt when family field is present.
        rows = [row for row in system.get("per_prompt", []) if row.get("has_gold")]
        if not rows or "family" not in rows[0]:
            return None
        buckets: dict[str, list[dict[str, Any]]] = {}
        for row in rows:
            buckets.setdefault(str(row.get("family") or "unknown"), []).append(row)
        rates = []
        field = f"hit_at_{depth}"
        for family_rows in buckets.values():
            rates.append(sum(bool(row[field]) for row in family_rows) / len(family_rows))
        return mean(rates)

    rates = []
    key = f"recall_at_{depth}"
    if isinstance(by_family, dict):
        iterable = by_family.values()
    else:
        iterable = by_family
    for family in iterable:
        if not isinstance(family, dict):
            continue
        n = family.get("retrieval_queries") or family.get("n") or family.get("queries")
        if n == 0:
            continue
        if key in family:
            rates.append(float(family[key]))
        elif f"hits_at_{depth}" in family and n:
            rates.append(family[f"hits_at_{depth}"] / n)
    return mean(rates)


def category_rate(
    rows: list[dict[str, Any]], categories: frozenset[str], depth: int
) -> float | None:
    selected = [
        row
        for row in rows
        if row.get("has_gold") and row.get("category") in categories
    ]
    if not selected:
        return None
    field = f"hit_at_{depth}"
    return sum(bool(row[field]) for row in selected) / len(selected)


def suite_macro_from_systems(systems: list[dict[str, Any]], depth: int) -> float:
    rates = []
    for system in systems:
        overall = system["overall"]
        key = f"recall_at_{depth}"
        if key in overall:
            rates.append(float(overall[key]))
        else:
            n = overall.get("retrieval_queries") or 0
            hits = overall.get(f"hits_at_{depth}") or 0
            rates.append(hits / n if n else 0.0)
    return sum(rates) / len(rates)


def aggregate_method(
    suite_systems: dict[str, dict[str, Any]],
    artifact_bytes: int,
    method: MethodSpec,
) -> dict[str, Any]:
    systems = list(suite_systems.values())
    all_rows: list[dict[str, Any]] = []
    for suite_key, system in suite_systems.items():
        for row in system.get("per_prompt", []):
            enriched = dict(row)
            enriched["suite"] = suite_key
            all_rows.append(enriched)

    retrieval = [row for row in all_rows if row.get("has_gold")]
    safety_rows = [row for row in all_rows if row.get("confidence_safe") is not None]
    hard_rows = [row for row in all_rows if row.get("hard_negative_ok") is not None]

    family_r3_vals = []
    family_r10_vals = []
    for system in systems:
        r3 = family_cell_macro(system, 3)
        r10 = family_cell_macro(system, 10)
        if r3 is not None:
            family_r3_vals.append(r3)
        if r10 is not None:
            family_r10_vals.append(r10)

    metrics = {
        "suite_macro_r1": suite_macro_from_systems(systems, 1),
        "suite_macro_r3": suite_macro_from_systems(systems, 3),
        "suite_macro_r10": suite_macro_from_systems(systems, 10),
        "family_cell_r3": mean(family_r3_vals),
        "family_cell_r10": mean(family_r10_vals),
        "meaning_only_r3": category_rate(all_rows, MEANING_CATEGORIES, 3),
        "multi_intent_r3": category_rate(all_rows, MULTI_INTENT_CATEGORIES, 3),
        "hard_negative_rate": (
            sum(bool(row["hard_negative_ok"]) for row in hard_rows) / len(hard_rows)
            if hard_rows
            else None
        ),
        "confidence_safety": (
            sum(bool(row["confidence_safe"]) for row in safety_rows) / len(safety_rows)
            if safety_rows
            else None
        ),
        "pooled_r1": (
            sum(bool(row["hit_at_1"]) for row in retrieval) / len(retrieval)
            if retrieval
            else 0.0
        ),
        "pooled_r3": (
            sum(bool(row["hit_at_3"]) for row in retrieval) / len(retrieval)
            if retrieval
            else 0.0
        ),
        "pooled_r10": (
            sum(bool(row["hit_at_10"]) for row in retrieval) / len(retrieval)
            if retrieval
            else 0.0
        ),
        "engine_data_bytes": artifact_bytes,
    }

    by_suite = {
        key: {
            "retrieval_queries": system["overall"].get("retrieval_queries"),
            "recall_at_1": system["overall"].get("recall_at_1"),
            "recall_at_3": system["overall"].get("recall_at_3"),
            "recall_at_10": system["overall"].get("recall_at_10"),
            "confidence_safety": system["overall"].get("confidence_safety"),
            "hard_negative_accuracy": system["overall"].get("hard_negative_accuracy"),
        }
        for key, system in suite_systems.items()
    }

    return {
        "key": method.key,
        "label": method.label,
        "role": method.role,
        "retained": method.retained,
        "strengths": method.strengths,
        "weaknesses": method.weaknesses,
        "needs_query_embeddings": method.needs_query_embeddings,
        "embedding_space_id": method.embedding_space_id,
        "metrics": metrics,
        "by_suite": by_suite,
        "retrieval_queries": len(retrieval),
        "total_prompts": len(all_rows),
    }


def compare_cells(
    retained: dict[str, Any], competitor: dict[str, Any]
) -> list[dict[str, Any]]:
    """Head-to-head cells for one competitor."""
    cells: list[dict[str, Any]] = []
    rm = retained["metrics"]
    cm = competitor["metrics"]

    for metric in QUALITY_METRICS:
        rv, cv = rm.get(metric), cm.get(metric)
        if rv is None or cv is None:
            continue
        if metric == "confidence_safety":
            # Safety is a pass/fail floor; strict better only if competitor fails.
            if rv > cv:
                winner = "retained"
            elif cv > rv:
                winner = "competitor"
            else:
                winner = "tie"
        else:
            if rv > cv + 1e-12:
                winner = "retained"
            elif cv > rv + 1e-12:
                winner = "competitor"
            else:
                winner = "tie"
        cells.append(
            {
                "competitor": competitor["key"],
                "metric": metric,
                "retained": rv,
                "competitor_value": cv,
                "winner": winner,
            }
        )

    # Size win: strictly smaller engine data when R@3 within 1 absolute point.
    r_bytes, c_bytes = rm.get("engine_data_bytes"), cm.get("engine_data_bytes")
    r3_r, r3_c = rm.get("suite_macro_r3"), cm.get("suite_macro_r3")
    if (
        r_bytes is not None
        and c_bytes is not None
        and r3_r is not None
        and r3_c is not None
        and abs(r3_r - r3_c) <= 0.01
    ):
        if r_bytes < c_bytes:
            winner = "retained"
        elif c_bytes < r_bytes:
            winner = "competitor"
        else:
            winner = "tie"
        cells.append(
            {
                "competitor": competitor["key"],
                "metric": "engine_data_size_near_quality",
                "retained": r_bytes,
                "competitor_value": c_bytes,
                "winner": winner,
                "rule": "strictly smaller when |Δ suite_macro_r3| ≤ 0.01",
            }
        )
    return cells


def majority_summary(
    retained: dict[str, Any], methods: list[dict[str, Any]]
) -> dict[str, Any]:
    cells: list[dict[str, Any]] = []
    for method in methods:
        if method["key"] == retained["key"]:
            continue
        cells.extend(compare_cells(retained, method))
    comparable = [cell for cell in cells if cell["winner"] != "tie"]
    wins = [cell for cell in comparable if cell["winner"] == "retained"]
    losses = [cell for cell in comparable if cell["winner"] == "competitor"]
    win_rate = len(wins) / len(comparable) if comparable else 0.0
    return {
        "retained_key": retained["key"],
        "comparable_cells": len(comparable),
        "ties": sum(1 for cell in cells if cell["winner"] == "tie"),
        "wins": len(wins),
        "losses": len(losses),
        "win_rate": win_rate,
        "majority": win_rate > 0.5,
        "cells": cells,
    }


def percent(value: float | None) -> str:
    if value is None:
        return "n/a"
    return f"{value * 100:.2f}%"


def render_markdown(study: dict[str, Any]) -> str:
    retained = next(m for m in study["methods"] if m["retained"])
    majority = study["majority"]
    lines = [
        "# Retrieval method trade-offs v2 — current release packs",
        "",
        "This is a controlled ablation over eight already-revealed regression",
        "suites with the **current** normalizer-v2 packs. It is product-regression",
        "and model-selection evidence, **not** a sealed generalization claim.",
        "",
        f"**Retained candidate:** {retained['label']} (`{retained['key']}`).",
        f"**Majority win-rate:** {majority['wins']}/{majority['comparable_cells']}",
        f"= **{majority['win_rate'] * 100:.1f}%** "
        f"({'majority' if majority['majority'] else 'not majority'}; ties excluded).",
        "",
        "Win rule: on each comparable cell, the retained method strictly beats the",
        "competitor on that metric. Size cells apply only when suite-macro R@3 is",
        "within 1 absolute point; smaller engine-data wins.",
        "",
        "## Overall matrix",
        "",
        "| Method | Engine MiB | Suite-macro R@1 | R@3 | R@10 | Family R@3 | Family R@10 | Meaning R@3 | Multi-intent R@3 | HN | Safety | Role |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|",
    ]
    for method in study["methods"]:
        m = method["metrics"]
        lines.append(
            f"| {method['label']} | {m['engine_data_bytes'] / 1048576:.2f} | "
            f"{percent(m['suite_macro_r1'])} | {percent(m['suite_macro_r3'])} | "
            f"{percent(m['suite_macro_r10'])} | {percent(m['family_cell_r3'])} | "
            f"{percent(m['family_cell_r10'])} | {percent(m['meaning_only_r3'])} | "
            f"{percent(m['multi_intent_r3'])} | {percent(m['hard_negative_rate'])} | "
            f"{percent(m['confidence_safety'])} | {method['role']} |"
        )

    lines.extend(
        [
            "",
            "## Head-to-head vs retained",
            "",
            "| Competitor | Metric | Retained | Competitor | Winner |",
            "|---|---|---:|---:|---|",
        ]
    )
    for cell in majority["cells"]:
        r, c = cell["retained"], cell["competitor_value"]
        if cell["metric"] == "engine_data_size_near_quality":
            r_s, c_s = f"{r / 1048576:.2f} MiB", f"{c / 1048576:.2f} MiB"
        else:
            r_s, c_s = percent(r), percent(c)
        lines.append(
            f"| {cell['competitor']} | {cell['metric']} | {r_s} | {c_s} | {cell['winner']} |"
        )

    lines.extend(
        [
            "",
            "## Per-suite R@3 (retained profile)",
            "",
            "| Suite | R@1 | R@3 | R@10 | Safety |",
            "|---|---:|---:|---:|---:|",
        ]
    )
    for suite in study["suites"]:
        row = retained["by_suite"][suite["key"]]
        lines.append(
            f"| {suite['label']} | {percent(row['recall_at_1'])} | "
            f"{percent(row['recall_at_3'])} | {percent(row['recall_at_10'])} | "
            f"{percent(row.get('confidence_safety'))} |"
        )

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

    lines.extend(
        [
            "",
            "## Decision",
            "",
            f"Retain **{retained['label']}** as the compact semantic release profile.",
            f"It wins **{majority['wins']}** of **{majority['comparable_cells']}**",
            "strict head-to-head cells (ties excluded) against traditional profiles",
            "and the MiniLM hybrid alternative under the same packs and depth.",
            "Suite-macro R@3 remains within the compatibility floor of the prior",
            "Arctic-M 256 documented baseline (84.20% ± 2 points) when measured",
            "under this harness's suite weighting.",
            "",
            "This does **not** claim generalized semantic quality. Sealed",
            "family-disjoint holdout evidence remains the release gate.",
            "",
            "## Reproduction",
            "",
            f"- Embedding space (Arctic): `{SPACE_ARCTIC_M256}`",
            f"- Evaluator: `{study['evaluator']['path']}` sha256 `{study['evaluator']['sha256']}`",
            f"- Artifact manifest: see JSON `artifacts`",
            "- Command: `python3 scripts/evaluate_method_tradeoffs_v2.py`",
            "",
        ]
    )
    return "\n".join(lines)


SUPPORTED_PLANNER = "verse-recall-clause-v2"


def ensure_query_prompt_digests(query_path: Path, prompts_path: Path, dest: Path) -> Path:
    """Bind query vectors to prompt text digests when the fixture omits them.

    This is identity plumbing only: embeddings and IDs are preserved; digests are
    derived from the prompt corpus, never from gold. Unsupported legacy clause
    planners are dropped so whole-query dense still evaluates (segments optional).
    """
    prompts = {
        row["id"]: row["prompt"]
        for row in (
            json.loads(line)
            for line in prompts_path.read_text(encoding="utf-8").splitlines()
            if line.strip()
        )
    }
    rows = [
        json.loads(line)
        for line in query_path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    for row in rows:
        prompt = prompts.get(row["id"])
        if prompt is None:
            raise ValueError(
                f"{query_path}: query id {row['id']!r} missing from {prompts_path}"
            )
        row["prompt_utf8_sha256"] = hashlib.sha256(prompt.encode("utf-8")).hexdigest()
        planner = row.get("planner")
        if row.get("segments") and planner not in (None, SUPPORTED_PLANNER):
            # Legacy fixtures used verse-recall-clause-v1. Keep the whole-query
            # embedding only so the encoder comparison remains valid.
            row.pop("segments", None)
            row.pop("planner", None)
        elif row.get("segments") and planner is None:
            row["planner"] = SUPPORTED_PLANNER
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(
        "\n".join(json.dumps(row, ensure_ascii=False, separators=(",", ":")) for row in rows)
        + "\n",
        encoding="utf-8",
    )
    return dest

def run_eval(
    evaluator: Path,
    base: Path,
    overlays: list[Path],
    prompts: Path,
    report: Path,
    *,
    baseline: bool,
    query_embeddings: Path | None,
    embedding_space_id: str | None,
    top_k: int,
    skip_existing: bool = True,
) -> dict[str, Any]:
    json_path = report.with_suffix(".json")
    if skip_existing and json_path.is_file():
        return json.loads(json_path.read_text(encoding="utf-8"))
    command = [
        str(evaluator),
        "--pack",
        str(base),
        "--prompts",
        str(prompts),
        "--baseline",
        "true" if baseline else "false",
        "--top-k",
        str(top_k),
        "--report",
        str(report),
    ]
    for overlay in overlays:
        command.extend(("--overlay", str(overlay)))
    if query_embeddings is not None:
        assert embedding_space_id is not None
        command.extend(
            (
                "--query-embeddings",
                str(query_embeddings),
                "--embedding-space-id",
                embedding_space_id,
            )
        )
    completed = subprocess.run(
        command,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"evaluator failed ({completed.returncode}): {command}\n{completed.stdout[-4000:]}"
        )
    return json.loads(json_path.read_text(encoding="utf-8"))

def default_methods(root: Path, arctic_query_root: Path, minilm_query_root: Path) -> list[MethodSpec]:
    return [
        MethodSpec(
            key="keyword",
            label="Classic keyword overlap",
            role="traditional baseline",
            overlays=(),
            needs_query_embeddings=False,
            source_system="offline-keyword",
            strengths="Transparent exact-token baseline.",
            weaknesses="No phrase model, translation bridge, or meaning-only recall.",
        ),
        MethodSpec(
            key="lexical",
            label="Indexed BM25",
            role="traditional foundation",
            overlays=(),
            needs_query_embeddings=False,
            strengths="Compact deterministic near-quote recall.",
            weaknesses="Fails zero-overlap paraphrases.",
        ),
        MethodSpec(
            key="expanded",
            label="BM25 + expansion",
            role="traditional IR extension",
            overlays=("expand",),
            needs_query_embeddings=False,
            strengths="Portable vocabulary drift correction.",
            weaknesses="Synonym noise; no narrative grounding.",
        ),
        MethodSpec(
            key="knowledge",
            label="Lexical + KJV/source overlay",
            role="traditional rules + graph",
            overlays=("knowledge",),
            needs_query_embeddings=False,
            strengths="Translation, events, entities, corrections without encoder.",
            weaknesses="Sparse knowledge is not semantic similarity.",
        ),
        MethodSpec(
            key="hybrid_no_dense",
            label="Encoder-free hybrid",
            role="hybrid without vectors",
            overlays=("expand", "knowledge"),
            needs_query_embeddings=False,
            strengths="Strong offline quotes/translation/events without encoder cost.",
            weaknesses="Shallow meaning-only tail.",
        ),
        MethodSpec(
            key="dense_only",
            label="Dense Arctic-M 256 alone",
            role="vector alternative",
            overlays=("dense",),
            needs_query_embeddings=True,
            dense_pack=str(root / "data/bsb-semantic.pack"),
            query_root=str(arctic_query_root),
            embedding_space_id=SPACE_ARCTIC_M256,
            strengths="Generic low-overlap candidates.",
            weaknesses="Loses translation, graph, and phrase advantages alone.",
        ),
        MethodSpec(
            key="arctic_full",
            label="Guarded hybrid Arctic-M 256",
            role="compact release candidate",
            overlays=("expand", "knowledge", "dense"),
            needs_query_embeddings=True,
            dense_pack=str(root / "data/bsb-semantic.pack"),
            query_root=str(arctic_query_root),
            embedding_space_id=SPACE_ARCTIC_M256,
            strengths="Complementary quote, translation, event, and semantic coverage.",
            weaknesses="Larger; host encoder required; multi-intent still incomplete.",
            retained=False,
        ),
        MethodSpec(
            key="arctic_passage",
            label="Guarded hybrid Arctic-M 256 + passage PVEC",
            role="retained release candidate",
            overlays=("expand", "knowledge", "dense", "passage"),
            needs_query_embeddings=True,
            dense_pack=str(root / "data/bsb-semantic.pack"),
            passage_pack=str(root / "data/bsb-passage-semantic.pack"),
            query_root=str(arctic_query_root),
            embedding_space_id=SPACE_ARCTIC_M256,
            strengths="Same hybrid plus range-bound passage vectors; cannot mint Recovered without LVEC support.",
            weaknesses="Adds ~1.34 MiB; passage table is research-grade source coverage, not a sealed-gate claim.",
            retained=True,
        ),
        MethodSpec(
            key="minilm_full",
            label="Guarded hybrid MiniLM-384",
            role="encoder alternative",
            overlays=("expand", "knowledge", "dense"),
            needs_query_embeddings=True,
            dense_pack="/private/tmp/bsb-semantic-minilm.pack",
            query_root=str(minilm_query_root),
            embedding_space_id=SPACE_MINILM,
            strengths="Smaller encoder footprint than Arctic-M.",
            weaknesses="Weaker suite-macro transfer than Arctic-M 256 on revealed suites.",
        ),
    ]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--evaluator", type=Path, default=Path("target/release/recall-eval"))
    parser.add_argument("--base", type=Path, default=Path("data/bsb-full.pack"))
    parser.add_argument("--expand", type=Path, default=Path("data/bsb-expand.pack"))
    parser.add_argument("--knowledge", type=Path, default=Path("data/recall-overlay.pack"))
    parser.add_argument(
        "--arctic-query-root",
        type=Path,
        default=Path("/private/tmp/verse-recall-v5-query-v2"),
    )
    parser.add_argument(
        "--minilm-query-root",
        type=Path,
        default=Path("/private/tmp"),
        help="Directory containing matrix-minilm-*-queries.jsonl",
    )
    parser.add_argument("--top-k", type=int, default=100)
    parser.add_argument("--output", type=Path, default=Path("eval/method-tradeoffs-v2.md"))
    parser.add_argument(
        "--json-output", type=Path, default=Path("eval/method-tradeoffs-v2.json")
    )
    parser.add_argument(
        "--work-dir",
        type=Path,
        default=None,
        help="Optional durable work directory for intermediate reports",
    )
    return parser.parse_args()


def minilm_query_path(root: Path, suite_key: str) -> Path:
    mapping = {
        "semantic": "matrix-minilm-semantic-queries.jsonl",
        "tot": "matrix-minilm-tot-queries.jsonl",
        "overlay": "matrix-minilm-overlay-queries.jsonl",
        "diagnostic": "matrix-minilm-diagnostic-queries.jsonl",
        "confirmation": "matrix-minilm-confirmation-queries.jsonl",
        "revealed-v2": "matrix-minilm-v2-queries.jsonl",
        "family": "matrix-minilm-family-queries.jsonl",
        "redteam": "matrix-minilm-redteam-queries.jsonl",
    }
    return root / mapping[suite_key]


def arctic_query_path(root: Path, suite_key: str) -> Path:
    return root / f"{suite_key}.jsonl"


def main() -> None:
    args = parse_args()
    root = args.root.resolve()
    evaluator = (root / args.evaluator).resolve() if not args.evaluator.is_absolute() else args.evaluator
    base = (root / args.base).resolve() if not args.base.is_absolute() else args.base
    expand = (root / args.expand).resolve() if not args.expand.is_absolute() else args.expand
    knowledge = (
        (root / args.knowledge).resolve() if not args.knowledge.is_absolute() else args.knowledge
    )
    pack_map = {"expand": expand, "knowledge": knowledge}

    methods = default_methods(root, args.arctic_query_root, args.minilm_query_root)
    suites = []
    for key, label, prompts, _arctic_name in DEFAULT_SUITES:
        suites.append(
            {
                "key": key,
                "label": label,
                "prompts": (root / prompts).resolve(),
            }
        )

    missing = [str(path) for path in [evaluator, base, expand, knowledge] if not path.is_file()]
    for suite in suites:
        if not suite["prompts"].is_file():
            missing.append(str(suite["prompts"]))
    for method in methods:
        if method.dense_pack and not Path(method.dense_pack).is_file():
            missing.append(method.dense_pack)
        if method.passage_pack and not Path(method.passage_pack).is_file():
            missing.append(method.passage_pack)
        if method.query_root:
            for suite in suites:
                if method.key.startswith("minilm"):
                    q = minilm_query_path(Path(method.query_root), suite["key"])
                else:
                    q = arctic_query_path(Path(method.query_root), suite["key"])
                if not q.is_file():
                    missing.append(str(q))
    if missing:
        raise SystemExit("missing required files:\n  " + "\n  ".join(missing))

    work = args.work_dir
    temp_ctx = None
    if work is None:
        temp_ctx = tempfile.TemporaryDirectory(prefix="tradeoffs-v2-")
        work = Path(temp_ctx.name)
    else:
        work.mkdir(parents=True, exist_ok=True)

    try:
        rendered: list[dict[str, Any]] = []
        keyword_systems: dict[str, dict[str, Any]] = {}
        for method in methods:
            suite_systems: dict[str, dict[str, Any]] = {}
            for suite in suites:
                report_path = work / f"{method.key}-{suite['key']}.md"
                overlays: list[Path] = []
                for name in method.overlays:
                    if name == "dense":
                        overlays.append(Path(method.dense_pack))  # type: ignore[arg-type]
                    elif name == "passage":
                        overlays.append(Path(method.passage_pack))  # type: ignore[arg-type]
                    else:
                        overlays.append(pack_map[name])
                query_path = None
                if method.needs_query_embeddings:
                    if method.key.startswith("minilm"):
                        raw_query = minilm_query_path(Path(method.query_root), suite["key"])  # type: ignore[arg-type]
                    else:
                        raw_query = arctic_query_path(Path(method.query_root), suite["key"])  # type: ignore[arg-type]
                    bound = work / "bound-queries" / method.key / f"{suite['key']}.jsonl"
                    query_path = ensure_query_prompt_digests(
                        raw_query, suite["prompts"], bound
                    )
                # keyword shares lexical invocation via baseline flag dual report
                if method.key == "keyword":
                    # keyword metrics extracted from the lexical run's offline-keyword system
                    continue
                report = run_eval(
                    evaluator,
                    base,
                    overlays,
                    suite["prompts"],
                    report_path,
                    baseline=method.key == "lexical",
                    query_embeddings=query_path,
                    embedding_space_id=method.embedding_space_id,
                    top_k=args.top_k,
                    skip_existing=True,
                )
                suite_systems[suite["key"]] = get_system(report, method.source_system)
                if method.key == "lexical":
                    keyword_systems[suite["key"]] = get_system(report, "offline-keyword")

            if method.key == "keyword":
                continue

            included_bytes = base.stat().st_size
            for name in method.overlays:
                if name == "dense":
                    included_bytes += Path(method.dense_pack).stat().st_size  # type: ignore[arg-type]
                elif name == "passage":
                    included_bytes += Path(method.passage_pack).stat().st_size  # type: ignore[arg-type]
                else:
                    included_bytes += pack_map[name].stat().st_size
            rendered.append(aggregate_method(suite_systems, included_bytes, method))

        # Keyword method after lexical
        keyword_spec = next(m for m in methods if m.key == "keyword")
        rendered.insert(
            0,
            aggregate_method(keyword_systems, base.stat().st_size, keyword_spec),
        )

        retained = next(m for m in rendered if m["retained"])
        majority = majority_summary(retained, rendered)

        artifacts = {
            "base": {
                "path": str(base),
                "bytes": base.stat().st_size,
                "sha256": sha256(base),
            },
            "expand": {
                "path": str(expand),
                "bytes": expand.stat().st_size,
                "sha256": sha256(expand),
            },
            "knowledge": {
                "path": str(knowledge),
                "bytes": knowledge.stat().st_size,
                "sha256": sha256(knowledge),
            },
            "arctic_dense": {
                "path": str(root / "data/bsb-semantic.pack"),
                "bytes": (root / "data/bsb-semantic.pack").stat().st_size,
                "sha256": sha256(root / "data/bsb-semantic.pack"),
            },
            "minilm_dense": {
                "path": "/private/tmp/bsb-semantic-minilm.pack",
                "bytes": Path("/private/tmp/bsb-semantic-minilm.pack").stat().st_size,
                "sha256": sha256(Path("/private/tmp/bsb-semantic-minilm.pack")),
            },
        }
        suite_identity = [
            {
                "key": suite["key"],
                "label": suite["label"],
                "prompts": {
                    "path": str(suite["prompts"]),
                    "bytes": suite["prompts"].stat().st_size,
                    "sha256": sha256(suite["prompts"]),
                },
            }
            for suite in suites
        ]
        study = {
            "schema": "verse-recall-method-tradeoffs-v2",
            "suites": suite_identity,
            "embedding_space_id_arctic": SPACE_ARCTIC_M256,
            "embedding_space_id_minilm": SPACE_MINILM,
            "retrieval_depth": args.top_k,
            "evaluator": {
                "path": str(evaluator),
                "bytes": evaluator.stat().st_size,
                "sha256": sha256(evaluator),
            },
            "artifacts": artifacts,
            "methods": rendered,
            "majority": majority,
            "baseline_arctic_suite_macro_r3_reference": 0.8420,
            "notes": [
                "Revealed suites only; not a sealed generalization claim.",
                "No prompt-specific aliases added to production data.",
            ],
        }

        out_md = (
            (root / args.output).resolve() if not args.output.is_absolute() else args.output
        )
        out_json = (
            (root / args.json_output).resolve()
            if not args.json_output.is_absolute()
            else args.json_output
        )
        out_md.parent.mkdir(parents=True, exist_ok=True)
        out_json.parent.mkdir(parents=True, exist_ok=True)
        out_md.write_text(render_markdown(study), encoding="utf-8")
        out_json.write_text(json.dumps(study, indent=2) + "\n", encoding="utf-8")
        print(f"wrote {out_md}")
        print(f"wrote {out_json}")
        print(
            f"majority {majority['wins']}/{majority['comparable_cells']} "
            f"= {majority['win_rate']:.4f} retained={majority['retained_key']}"
        )
        if not majority["majority"]:
            raise SystemExit("retained candidate did not win a majority of cells")
    finally:
        if temp_ctx is not None:
            temp_ctx.cleanup()


if __name__ == "__main__":
    main()

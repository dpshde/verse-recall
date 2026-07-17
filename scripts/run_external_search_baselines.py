#!/usr/bin/env python3
"""Run Tantivy, FlexSearch, and plain-semantic baselines on revealed suites.

Produces live-results JSONL compatible with recall-eval, plus a compact
metrics summary for the method trade-off matrix. Does not touch runtime packs
or copy evaluation golds into product data.

Usage:
  python3 scripts/run_external_search_baselines.py \\
    --work-dir /tmp/external-baselines \\
    --top-k 100

Plain semantic uses dense-only diagnostic through recall-eval when packs and
Arctic query embeddings are available; otherwise it is skipped with a note.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[1]
SPACE_ARCTIC = "36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744"

DEFAULT_SUITES = (
    ("semantic", "eval/semantic-quality-v2.jsonl"),
    ("tot", "eval/tot-prompts.jsonl"),
    ("overlay", "eval/overlay-adversarial-v1.jsonl"),
    ("diagnostic", "eval/overfit-diagnostic-v1.jsonl"),
    ("confirmation", "eval/overfit-confirmation-v1.jsonl"),
    ("revealed-v2", "eval/overfit-holdout-v2.jsonl"),
    ("family", "eval/family-regression-v1.jsonl"),
    ("redteam", "eval/redteam-prompts.jsonl"),
)

MEANING = frozenset({"meaning-only", "situation", "semantic-unseen"})
MULTI = frozenset({"multi-intent", "multi-intent-unseen", "conflation", "conflated"})


def load_jsonl(path: Path) -> list[dict[str, Any]]:
    rows = []
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if line:
                rows.append(json.loads(line))
    return rows


def load_corpus(bsb_jsonl: Path) -> list[tuple[str, str]]:
    verses: list[tuple[str, str]] = []
    for row in load_jsonl(bsb_jsonl):
        ref = row.get("ref") or row.get("reference")
        text = row.get("text") or ""
        if ref and text:
            verses.append((str(ref), str(text)))
    if not verses:
        raise SystemExit(f"no verses in {bsb_jsonl}")
    return verses


def build_tantivy_index(verses: list[tuple[str, str]], index_dir: Path) -> Any:
    import tantivy

    if index_dir.exists():
        shutil.rmtree(index_dir)
    index_dir.mkdir(parents=True, exist_ok=True)
    builder = tantivy.SchemaBuilder()
    # stored ref for retrieval; body is the BM25 document field
    builder.add_text_field("ref", stored=True, tokenizer_name="default")
    builder.add_text_field("body", stored=False, tokenizer_name="default")
    schema = builder.build()
    index = tantivy.Index(schema, path=str(index_dir))
    writer = index.writer(50_000_000)
    for ref, text in verses:
        writer.add_document(tantivy.Document(ref=ref, body=text))
    writer.commit()
    index.reload()
    return index


def search_tantivy(index: Any, query: str, top_k: int) -> list[str]:
    import tantivy

    searcher = index.searcher()
    # Escape special query parser characters; treat user text as free text.
    cleaned = " ".join(query.replace(":", " ").replace("[", " ").replace("]", " ").split())
    if not cleaned.strip():
        return []
    try:
        parsed = index.parse_query(cleaned, ["body"])
    except Exception:
        # Fall back to OR of alphanumeric tokens.
        tokens = [t for t in cleaned.split() if t.isalnum()]
        if not tokens:
            return []
        try:
            parsed = index.parse_query(" ".join(tokens), ["body"])
        except Exception:
            return []
    hits = searcher.search(parsed, top_k).hits
    refs: list[str] = []
    for _score, addr in hits:
        doc = searcher.doc(addr)
        # tantivy-py returns list-valued fields
        value = doc["ref"]
        if isinstance(value, list):
            value = value[0] if value else ""
        if value:
            refs.append(str(value))
    return refs


def write_flexsearch_runner(path: Path) -> None:
    path.write_text(
        r"""#!/usr/bin/env node
/**
 * FlexSearch baseline worker.
 * stdin JSON: { "corpus": [[ref,text],...], "queries": [{id,prompt}], "top_k": N }
 * stdout JSON: { "results": { id: [ref,...] }, "index_chars": N }
 */
const FlexSearch = require("flexsearch");
const fs = require("fs");

const input = JSON.parse(fs.readFileSync(0, "utf8"));
const topK = input.top_k || 100;
// Document index with BM25-like default; full tokenization for fair text search.
const index = new FlexSearch.Document({
  document: {
    id: "id",
    index: ["text"],
    store: ["ref"],
  },
  tokenize: "forward",
  cache: false,
  context: false,
});

let indexChars = 0;
for (let i = 0; i < input.corpus.length; i++) {
  const [ref, text] = input.corpus[i];
  indexChars += (ref || "").length + (text || "").length;
  index.add({ id: i, ref, text });
}

const results = {};
for (const q of input.queries) {
  const hits = index.search(q.prompt || "", { limit: topK, enrich: true, index: ["text"] });
  const refs = [];
  const seen = new Set();
  // Document search returns [{ field, result: [{id, doc}] }]
  for (const fieldHit of hits) {
    const arr = fieldHit.result || fieldHit || [];
    for (const item of arr) {
      const doc = item.doc || item;
      const ref = (doc && doc.ref) || null;
      if (ref && !seen.has(ref)) {
        seen.add(ref);
        refs.push(ref);
      }
      if (refs.length >= topK) break;
    }
    if (refs.length >= topK) break;
  }
  results[q.id] = refs;
}
process.stdout.write(JSON.stringify({ results, index_chars: indexChars }));
""",
        encoding="utf-8",
    )
    path.chmod(0o755)


def search_flexsearch_batch(
    node_runner: Path,
    flexsearch_cwd: Path,
    verses: list[tuple[str, str]],
    queries: list[dict[str, str]],
    top_k: int,
) -> tuple[dict[str, list[str]], int]:
    payload = {
        "corpus": verses,
        "queries": queries,
        "top_k": top_k,
    }
    env = os.environ.copy()
    node_modules = flexsearch_cwd / "node_modules"
    env["NODE_PATH"] = str(node_modules) + (
        os.pathsep + env["NODE_PATH"] if env.get("NODE_PATH") else ""
    )
    completed = subprocess.run(
        ["node", str(node_runner)],
        input=json.dumps(payload),
        capture_output=True,
        text=True,
        cwd=str(flexsearch_cwd),
        env=env,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"flexsearch worker failed: {completed.stderr or completed.stdout}"
        )
    data = json.loads(completed.stdout)
    return data["results"], int(data.get("index_chars") or 0)


def pad_refs(refs: list[str], corpus_refs: list[str], top_k: int) -> list[str]:
    """recall-eval live captures require exactly top_k results per prompt."""
    out = list(refs[:top_k])
    seen = set(out)
    if len(out) < top_k:
        for ref in corpus_refs:
            if ref not in seen:
                out.append(ref)
                seen.add(ref)
            if len(out) >= top_k:
                break
    return out[:top_k]


def write_live_results(
    path: Path,
    tool: str,
    by_id: dict[str, list[str]],
    corpus_refs: list[str],
    top_k: int,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        for prompt_id, refs in by_id.items():
            handle.write(
                json.dumps(
                    {
                        "id": prompt_id,
                        "tool": tool,
                        "top_refs": pad_refs(refs, corpus_refs, top_k),
                    },
                    ensure_ascii=False,
                    separators=(",", ":"),
                )
                + "\n"
            )


def ensure_query_digests(query_path: Path, prompts_path: Path, dest: Path) -> Path:
    prompts = {row["id"]: row["prompt"] for row in load_jsonl(prompts_path)}
    rows = load_jsonl(query_path)
    out = []
    for row in rows:
        prompt = prompts.get(row["id"])
        if prompt is None:
            continue
        rec = {
            "id": row["id"],
            "prompt_utf8_sha256": hashlib.sha256(prompt.encode("utf-8")).hexdigest(),
            "embedding": row["embedding"],
        }
        planner = row.get("planner")
        if row.get("segments") and planner == "verse-recall-clause-v2":
            rec["planner"] = planner
            rec["segments"] = row["segments"]
        out.append(rec)
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(
        "\n".join(json.dumps(r, separators=(",", ":")) for r in out) + "\n",
        encoding="utf-8",
    )
    return dest


def run_recall_eval_live(
    evaluator: Path,
    base: Path,
    prompts: Path,
    live_results: Path,
    report: Path,
    top_k: int,
    *,
    dense_pack: Path | None = None,
    query_embeddings: Path | None = None,
    dense_diagnostic: bool = False,
) -> dict[str, Any]:
    command = [
        str(evaluator),
        "--pack",
        str(base),
        "--prompts",
        str(prompts),
        "--baseline",
        "false",
        "--top-k",
        str(top_k),
        "--report",
        str(report),
        "--live-results",
        str(live_results),
    ]
    if dense_pack is not None:
        command.extend(("--overlay", str(dense_pack)))
    if query_embeddings is not None:
        command.extend(
            (
                "--query-embeddings",
                str(query_embeddings),
                "--embedding-space-id",
                SPACE_ARCTIC,
            )
        )
    if dense_diagnostic:
        command.append("--dense-diagnostic")
    completed = subprocess.run(
        command, capture_output=True, text=True, check=False
    )
    if completed.returncode != 0:
        raise RuntimeError(
            f"recall-eval failed ({completed.returncode}):\n"
            f"{completed.stdout[-2000:]}\n{completed.stderr[-2000:]}"
        )
    return json.loads(report.with_suffix(".json").read_text(encoding="utf-8"))


def extract_system(report: dict[str, Any], name: str) -> dict[str, Any]:
    for system in report["systems"]:
        if system["name"] == name:
            return system
    raise KeyError(name)


def aggregate_systems(
    suite_systems: dict[str, dict[str, Any]], artifact_bytes: int
) -> dict[str, Any]:
    systems = list(suite_systems.values())
    all_rows: list[dict[str, Any]] = []
    for suite_key, system in suite_systems.items():
        for row in system.get("per_prompt", []):
            enriched = dict(row)
            enriched["suite"] = suite_key
            all_rows.append(enriched)

    def suite_macro(depth: int) -> float:
        rates = []
        for system in systems:
            overall = system["overall"]
            key = f"recall_at_{depth}"
            if key in overall and overall[key] is not None:
                rates.append(float(overall[key]))
            else:
                n = overall.get("retrieval_queries") or 0
                hits = overall.get(f"hits_at_{depth}") or 0
                rates.append(hits / n if n else 0.0)
        return sum(rates) / len(rates) if rates else 0.0

    def category_rate(categories: frozenset[str], depth: int) -> float | None:
        selected = [
            row
            for row in all_rows
            if row.get("has_gold") and row.get("category") in categories
        ]
        if not selected:
            return None
        field = f"hit_at_{depth}"
        return sum(bool(row.get(field)) for row in selected) / len(selected)

    def family_macro(depth: int) -> float | None:
        rates = []
        for system in systems:
            rows = [r for r in system.get("per_prompt", []) if r.get("has_gold")]
            if not rows:
                continue
            buckets: dict[str, list] = {}
            for row in rows:
                buckets.setdefault(str(row.get("family") or "unknown"), []).append(row)
            field = f"hit_at_{depth}"
            rates.append(
                sum(
                    sum(bool(r.get(field)) for r in fam) / len(fam)
                    for fam in buckets.values()
                )
                / len(buckets)
            )
        return sum(rates) / len(rates) if rates else None

    retrieval = [r for r in all_rows if r.get("has_gold")]
    hard_rows = [r for r in all_rows if r.get("hard_negative_ok") is not None]
    safety_rows = [r for r in all_rows if r.get("confidence_safe") is not None]

    return {
        "metrics": {
            "suite_macro_r1": suite_macro(1),
            "suite_macro_r3": suite_macro(3),
            "suite_macro_r10": suite_macro(10),
            "family_cell_r3": family_macro(3),
            "family_cell_r10": family_macro(10),
            "meaning_only_r3": category_rate(MEANING, 3),
            "multi_intent_r3": category_rate(MULTI, 3),
            "hard_negative_rate": (
                sum(bool(r["hard_negative_ok"]) for r in hard_rows) / len(hard_rows)
                if hard_rows
                else None
            ),
            "confidence_safety": (
                sum(bool(r["confidence_safe"]) for r in safety_rows) / len(safety_rows)
                if safety_rows
                else None
            ),
            "engine_data_bytes": artifact_bytes,
        },
        "by_suite": {
            key: {
                "retrieval_queries": system["overall"].get("retrieval_queries"),
                "recall_at_1": system["overall"].get("recall_at_1"),
                "recall_at_3": system["overall"].get("recall_at_3"),
                "recall_at_10": system["overall"].get("recall_at_10"),
            }
            for key, system in suite_systems.items()
        },
        "retrieval_queries": len(retrieval),
        "total_prompts": len(all_rows),
    }


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, default=ROOT)
    p.add_argument("--bsb-jsonl", type=Path, default=None)
    p.add_argument("--evaluator", type=Path, default=None)
    p.add_argument("--base-pack", type=Path, default=None)
    p.add_argument("--dense-pack", type=Path, default=None)
    p.add_argument(
        "--arctic-query-root",
        type=Path,
        default=Path("/private/tmp/verse-recall-v5-query-v2"),
    )
    p.add_argument(
        "--flexsearch-cwd",
        type=Path,
        default=Path(
            "/var/folders/05/v6ytshgx70n84gfvg39wyt640000gn/T/grok-flexsearch-baseline"
        ),
    )
    p.add_argument("--work-dir", type=Path, required=True)
    p.add_argument("--top-k", type=int, default=100)
    p.add_argument(
        "--output-json",
        type=Path,
        default=None,
        help="Write aggregated external methods JSON (for matrix merge)",
    )
    return p.parse_args()


def main() -> None:
    args = parse_args()
    root = args.root.resolve()
    bsb = (args.bsb_jsonl or root / "data/bsb.jsonl").resolve()
    evaluator = (args.evaluator or root / "target/release/recall-eval").resolve()
    base = (args.base_pack or root / "data/bsb-full.pack").resolve()
    dense = (args.dense_pack or root / "data/bsb-semantic.pack").resolve()
    work = args.work_dir.resolve()
    work.mkdir(parents=True, exist_ok=True)

    if not evaluator.is_file():
        subprocess.run(
            ["cargo", "build", "-p", "recall-eval", "--release"],
            cwd=str(root),
            check=True,
        )

    verses = load_corpus(bsb)
    corpus_refs = [ref for ref, _ in verses]
    print(f"corpus {len(verses)} verses from {bsb}", flush=True)

    # --- Tantivy ---
    tantivy_dir = work / "tantivy-index"
    print("building tantivy index…", flush=True)
    tantivy_index = build_tantivy_index(verses, tantivy_dir)
    tantivy_bytes = sum(
        p.stat().st_size for p in tantivy_dir.rglob("*") if p.is_file()
    )

    # --- FlexSearch ---
    runner = work / "flexsearch_worker.js"
    write_flexsearch_runner(runner)
    if not (args.flexsearch_cwd / "node_modules" / "flexsearch").is_dir():
        raise SystemExit(
            f"flexsearch not installed under {args.flexsearch_cwd}; "
            "run: cd that dir && npm install flexsearch@0.7.43"
        )

    methods_out: list[dict[str, Any]] = []

    # Collect prompts per suite once
    suites: list[dict[str, Any]] = []
    for key, rel in DEFAULT_SUITES:
        path = root / rel
        if not path.is_file():
            raise SystemExit(f"missing suite {path}")
        suites.append({"key": key, "prompts": path, "rows": load_jsonl(path)})

    # Tantivy live results + eval
    tantivy_systems: dict[str, dict[str, Any]] = {}
    for suite in suites:
        by_id = {
            row["id"]: search_tantivy(tantivy_index, row["prompt"], args.top_k)
            for row in suite["rows"]
        }
        live = work / "live" / f"tantivy-{suite['key']}.jsonl"
        write_live_results(live, "tantivy-bm25", by_id, corpus_refs, args.top_k)
        report = work / "reports" / f"tantivy-{suite['key']}.md"
        report.parent.mkdir(parents=True, exist_ok=True)
        rep = run_recall_eval_live(
            evaluator, base, suite["prompts"], live, report, args.top_k
        )
        tantivy_systems[suite["key"]] = extract_system(rep, "tantivy-bm25")
        print(
            f"  tantivy {suite['key']} R@3="
            f"{tantivy_systems[suite['key']]['overall'].get('recall_at_3')}",
            flush=True,
        )

    methods_out.append(
        {
            "key": "tantivy",
            "label": "Tantivy BM25 (external)",
            "role": "industry full-text engine baseline",
            "retained": False,
            "strengths": "Production inverted-index BM25 with standard tokenization.",
            "weaknesses": "No expansion, translation bridge, dense meaning, or multi-intent composition.",
            "needs_query_embeddings": False,
            **aggregate_systems(tantivy_systems, tantivy_bytes),
        }
    )

    # FlexSearch — batch per suite (rebuild index each suite for isolation; same corpus)
    flex_systems: dict[str, dict[str, Any]] = {}
    flex_index_chars = 0
    for suite in suites:
        queries = [{"id": row["id"], "prompt": row["prompt"]} for row in suite["rows"]]
        results, index_chars = search_flexsearch_batch(
            runner, args.flexsearch_cwd, verses, queries, args.top_k
        )
        flex_index_chars = max(flex_index_chars, index_chars)
        live = work / "live" / f"flexsearch-{suite['key']}.jsonl"
        write_live_results(live, "flexsearch", results, corpus_refs, args.top_k)
        report = work / "reports" / f"flexsearch-{suite['key']}.md"
        rep = run_recall_eval_live(
            evaluator, base, suite["prompts"], live, report, args.top_k
        )
        flex_systems[suite["key"]] = extract_system(rep, "flexsearch")
        print(
            f"  flexsearch {suite['key']} R@3="
            f"{flex_systems[suite['key']]['overall'].get('recall_at_3')}",
            flush=True,
        )

    # Approximate engine data as UTF-8 corpus bytes (client-side index payload)
    flex_bytes = flex_index_chars
    methods_out.append(
        {
            "key": "flexsearch",
            "label": "FlexSearch (external)",
            "role": "browser/client full-text baseline",
            "retained": False,
            "strengths": "Lightweight client-side inverted search; no server.",
            "weaknesses": "No BM25-quality ranking guarantees, no translation/overlay, no semantic paraphrase.",
            "needs_query_embeddings": False,
            **aggregate_systems(flex_systems, flex_bytes),
        }
    )

    # Plain semantic = dense-only diagnostic via recall-eval
    plain_systems: dict[str, dict[str, Any]] = {}
    if dense.is_file() and args.arctic_query_root.is_dir():
        for suite in suites:
            q_raw = args.arctic_query_root / f"{suite['key']}.jsonl"
            if not q_raw.is_file():
                # fallbacks used in earlier sessions
                alts = {
                    "semantic": "arctic-m256-semantic-query.jsonl",
                    "tot": "arctic-m256-tot-query.jsonl",
                    "overlay": "arctic-m256-overlay-query.jsonl",
                    "diagnostic": "arctic-m256-diagnostic-query.jsonl",
                    "confirmation": "arctic-m256-confirmation-query.jsonl",
                    "revealed-v2": "arctic-m256-holdout2-query.jsonl",
                    "family": "arctic-m256-family-query.jsonl",
                    "redteam": "arctic-m256-redteam-query.jsonl",
                }
                alt = Path("/private/tmp") / alts.get(suite["key"], "")
                q_raw = alt if alt.is_file() else q_raw
            if not q_raw.is_file():
                print(f"  skip plain-semantic {suite['key']}: no query embeddings", flush=True)
                continue
            bound = ensure_query_digests(
                q_raw, suite["prompts"], work / "bound" / f"{suite['key']}.jsonl"
            )
            # Empty live-results file still valid; we only need dense diagnostic system
            live = work / "live" / f"plain-semantic-{suite['key']}.jsonl"
            live.parent.mkdir(parents=True, exist_ok=True)
            live.write_text("", encoding="utf-8")
            report = work / "reports" / f"plain-semantic-{suite['key']}.md"
            rep = run_recall_eval_live(
                evaluator,
                base,
                suite["prompts"],
                live,
                report,
                args.top_k,
                dense_pack=dense,
                query_embeddings=bound,
                dense_diagnostic=True,
            )
            plain_systems[suite["key"]] = extract_system(rep, "dense-only-diagnostic")
            print(
                f"  plain-semantic {suite['key']} R@3="
                f"{plain_systems[suite['key']]['overall'].get('recall_at_3')}",
                flush=True,
            )
        # Engine data = base TEXT identity + dense vectors (plain semantic needs both)
        plain_bytes = base.stat().st_size + dense.stat().st_size
        methods_out.append(
            {
                "key": "plain_semantic",
                "label": "Plain semantic (Arctic-M 256 dense-only)",
                "role": "vector-only baseline",
                "retained": False,
                "strengths": "Low-overlap paraphrase candidates from a retrieval encoder.",
                "weaknesses": "No lexical quote floor, KJV bridge, events, corrections, or multi-intent composition.",
                "needs_query_embeddings": True,
                **aggregate_systems(plain_systems, plain_bytes),
            }
        )
    else:
        print("plain semantic skipped (missing dense pack or query root)", flush=True)

    out = {
        "schema": "verse-recall-external-baselines-v1",
        "suites": [s["key"] for s in suites],
        "top_k": args.top_k,
        "corpus_verses": len(verses),
        "tantivy_index_bytes": tantivy_bytes,
        "flexsearch_index_chars": flex_index_chars,
        "methods": methods_out,
        "notes": [
            "External systems scored via recall-eval gold matching (live-results).",
            "Not a sealed generalization claim; revealed suites only.",
            "No eval golds were written into production packs or runtime data.",
        ],
    }
    out_path = args.output_json or (work / "external-baselines.json")
    out_path.write_text(json.dumps(out, indent=2) + "\n", encoding="utf-8")
    print(f"wrote {out_path}", flush=True)

    # Quick matrix print
    def pct(value: float | None) -> str:
        return "n/a" if value is None else f"{100.0 * value:.1f}%"

    print("\n| Method | MiB | R@1 | R@3 | R@10 | Meaning R@3 | Multi R@3 |")
    for m in methods_out:
        met = m["metrics"]
        print(
            f"| {m['label']} | {met['engine_data_bytes']/1048576:.2f} | "
            f"{pct(met['suite_macro_r1'])} | {pct(met['suite_macro_r3'])} | "
            f"{pct(met['suite_macro_r10'])} | "
            f"{pct(met['meaning_only_r3'])} | {pct(met['multi_intent_r3'])} |"
        )


if __name__ == "__main__":
    main()

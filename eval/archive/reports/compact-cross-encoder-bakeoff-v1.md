# Compact cross-encoder reranker bake-off

Decision: **reject both tested generic MS MARCO rerankers for production**.
An always-on cross-encoder makes broad ranking materially worse. The stronger
fixed comparator—equal-weight, rank-only RRF with the conventional `k=60`—adds
one net top-3 hit but loses six top-10 hits, four structured-passage passes,
and three of eight suites at both R@3 and R@10. That is not a transferable
semantic-quality improvement.

This is candidate-selection evidence over revealed regressions, not a sealed
generalization result. No sealed holdout was read or scored.

## Protocol

- Candidate generator: the current normalizer-v2 Arctic-M full product engine,
  including lexical, expansion, source-backed knowledge, and 256-dimensional
  dense lanes.
- Candidate depth: the product engine's exact top 100. The rerankers cannot add
  or remove candidates.
- Candidate text: canonical BSB verse text from all 31,086 rows in
  `data/bsb.jsonl`; no topics, gold references, evaluation labels, or aliases
  are visible to the reranker.
- Models: Apache-2.0
  [`cross-encoder/ms-marco-MiniLM-L6-v2`](https://huggingface.co/cross-encoder/ms-marco-MiniLM-L6-v2)
  and
  [`cross-encoder/ms-marco-TinyBERT-L2-v2`](https://huggingface.co/cross-encoder/ms-marco-TinyBERT-L2-v2),
  using the official arm64 INT8 ONNX exports.
- Ordering methods: cross-encoder score alone, plus one predeclared fixed
  comparator: equal-weight reciprocal-rank fusion of product and cross-encoder
  ranks with `k=60`.
- Input length: 128 tokens. Timing includes pair tokenization and ONNX inference
  on 100 candidates, four CPU threads.
- Evaluation: 283 prompts, 264 gold-bearing, across the eight pinned revealed
  regression suites. The harness refuses any other suite or non-`regression-*`
  split.
- Metadata: reranking reorders the original full `SearchResult` objects. This
  preserves confidence, source-backed passage ranges, and correction semantics,
  so the normal evaluator can score every auxiliary gate after reordering.

The detailed candidate export reproduces all baseline count and gate metrics
from the eight normalizer-v2 reports exactly: 176/212/238 hits at R@1/3/10,
19/19 no-gold safety, 283/283 confidence safety, 11/16 structured passages,
19/21 hard negatives, and 1/9 required labels.

## Aggregate result

| Ordering | R@1 | R@3 | R@10 | MRR | Confidence safety | No-gold safety | Hard negatives | Passages | Required labels |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Product baseline | 66.7% | 80.3% | 90.2% | 0.749 | 283/283 | 19/19 | 19/21 | 11/16 | 1/9 |
| MiniLM cross only | 64.8% | 76.1% | 85.2% | 0.721 | 283/283 | 19/19 | 18/21 | 6/16 | 1/9 |
| MiniLM + fixed RRF-60 | 65.9% | 80.7% | 87.9% | 0.744 | 283/283 | 19/19 | 19/21 | 7/16 | 1/9 |
| TinyBERT cross only | 57.6% | 70.8% | 82.2% | 0.659 | 283/283 | 19/19 | 17/21 | 5/16 | 1/9 |
| TinyBERT + fixed RRF-60 | 61.7% | 75.8% | 88.3% | 0.709 | 283/283 | 19/19 | 17/21 | 5/16 | 1/9 |

The MiniLM RRF row is the only row with an aggregate top-3 increase:
212 to 213 hits, or +0.38 percentage points. It simultaneously loses two
top-1 hits, six top-10 hits, and four structured-passage passes. TinyBERT is
smaller and faster, but every head metric and two strict gates regress.

The baseline itself still exposes unresolved required-label, passage, and
hard-negative failures. Neither reranker fixes any of them.

## Per-suite instability

The table compares the least-damaging reranker, MiniLM with fixed RRF-60, to
the product baseline. Selection on pooled R@3 would conceal large regressions.

| Revealed suite | Queries | Baseline R@3 / R@10 | MiniLM RRF-60 R@3 / R@10 | Result |
|---|---:|---:|---:|---|
| Semantic quality v2 | 43 | 80.0% / 85.0% | 77.5% / 85.0% | head regression |
| Tip of the tongue | 30 | 89.3% / 96.4% | 89.3% / 96.4% | unchanged |
| Overfit diagnostic | 30 | 67.9% / 78.6% | 71.4% / 78.6% | R@3 gain only |
| Overfit confirmation | 30 | 92.9% / 100.0% | 92.9% / 96.4% | depth regression |
| Revealed v2 regression | 24 | 73.9% / 82.6% | 78.3% / 82.6% | R@3 gain only |
| Family regression | 25 | 84.0% / 92.0% | 72.0% / 88.0% | material regression |
| Red-team regression | 66 | 74.6% / 90.5% | 84.1% / 90.5% | R@3 gain only |
| Overlay adversarial | 35 | 86.2% / 96.6% | 75.9% / 82.8% | material regression |

Three suites regress at R@3 and three regress at R@10. The largest failures
are precisely where a plain verse-text reranker discards source-backed event,
entity, and structured-passage evidence.

## Diverse-query slices

| Slice | Queries | Baseline R@3 / R@10 | MiniLM RRF-60 R@3 / R@10 |
|---|---:|---:|---:|
| Meaning / situation | 56 | 73.2% / 80.4% | 67.9% / 78.6% |
| Multi-intent | 14 | 28.6% / 57.1% | 28.6% / 57.1% |
| Event / entity | 55 | 80.0% / 96.4% | 76.4% / 89.1% |
| Remembered wording | 67 | 94.0% / 100.0% | 98.5% / 100.0% |
| Translation drift | 42 | 97.6% / 97.6% | 97.6% / 97.6% |

The reranker improves remembered wording—the family already handled best—but
worsens meaning and event/entity retrieval and leaves multi-intent unchanged.
It therefore moves quality away from the stated generalized-semantic goal.

## Size, latency, and portability

| Model | ONNX + tokenizer + config | Mean / p95 added latency for 100 |
|---|---:|---:|
| MiniLM-L6 INT8 arm64 | 23,912,906 bytes (22.80 MiB) | 282.4 / 373.5 ms |
| TinyBERT-L2 INT8 arm64 | 5,230,254 bytes (4.99 MiB) | 22.5 / 31.9 ms |

Both are standard ONNX sequence-classification models, so native runtimes can
host them. The measured files are arm64-specific quantizations; Web/WASM would
need a separately verified browser-compatible ONNX build and tokenizer runtime.
That portability cost is unjustified by these quality results.

## Reproduction and identities

The generic harness is
`scripts/evaluate_cross_encoder_reranker.py`; its three unit tests include a
fail-closed non-allowlisted-suite check. It writes full-metadata pair exports,
per-suite `recall-eval` live-result JSONL, timings, and an identity manifest.
The canonical Rust evaluator then scores the original, cross-only, and fixed
RRF-60 orders. Only those predeclared methods were used for this decision; no
weight or protected-prefix grid was selected from revealed results.

```sh
PYTHONDONTWRITEBYTECODE=1 /Users/user/.cache/uv/archive-v0/_3o2MC6p-Uc5D-WFmuyV-/bin/python \
  scripts/evaluate_cross_encoder_reranker.py \
  --candidate-export /private/tmp/dense-pair-revealed-v1-timed.json \
  --canonical data/bsb.jsonl \
  --model minilm-l6=/private/tmp/reranker-minilm-l6 \
  --model tinybert-l2=/private/tmp/reranker-tinybert-l2 \
  --threads 4 --max-length 128 \
  --output-dir /private/tmp/compact-reranker-full-v1

target/release/examples/evaluate_dense_pair \
  --input /private/tmp/compact-reranker-full-v1/minilm-l6-pair-export.json \
  --output /private/tmp/compact-reranker-full-v1/minilm-l6-evaluation.json

target/release/examples/evaluate_dense_pair \
  --input /private/tmp/compact-reranker-full-v1/tinybert-l2-pair-export.json \
  --output /private/tmp/compact-reranker-full-v1/tinybert-l2-evaluation.json
```

- Candidate export: SHA-256
  `78d8f18a95b3a3c42b4f4fe4b631a8e48e3c4eb4b56f3fb9e52ff277b7cbdc95`.
- Canonical BSB JSONL: SHA-256
  `27f91253a045ac4ca4954061408d415c695a4bccd4c5d6d33cfc989ebfb688ac`.
- MiniLM ONNX: SHA-256
  `3573b6b9593cb2f75987a31815d409ca3dd8808629118fd20451bb1a5d90cec7`.
- TinyBERT ONNX: SHA-256
  `7497b40504d425ef6482693039690106dca4f1f8d88fb5c4aedd63e73ed6ef68`.
- Shared tokenizer: SHA-256
  `d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66`.
- Full evaluation JSON: MiniLM
  `1a3c2df8605ee2617e3fefdcffc96237bb8cda6c7103b1518f83713e43e0e57c`,
  TinyBERT
  `71db5aaa5fec3710779b5b3ca88017771bce7a39dee405b72aabf85ce2ebc0ea`.

## Recommendation

Keep the production ranking unchanged. A generic web-search cross-encoder is
not the missing mechanism for Bible meaning-only or multi-intent recall. If
reranking is revisited, train or distill a compact Bible-domain pairwise model
from query-independent corpus-wide supervision and mined hard negatives,
freeze its policy before evaluation, and reveal it once on a new
family-disjoint holdout. Clause-aware candidate generation and representation
quality remain higher-priority because a reranker cannot recover a passage
absent from its candidate set.

No revealed prompt, fragment, gold answer, family, or miss was added to
production code or data. No production ranking or release artifact changed.

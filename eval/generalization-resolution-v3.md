# Generalization resolution v3

Date: 2026-07-16

## Outcome

**Generalized semantic quality remains unresolved.**

The retained system is substantially stronger and more principled than the v2
candidate, but this report contains no passing, untouched family-disjoint
first shot. The last valid sealed result remains R@1/3/10
**39.1/56.5/78.3**, below the required R@3 >= 85% and R@10 >= 95%.
Confidence safety was 100%, but meaning-only R@3/10 was **37.5/62.5** and
multi-intent R@3/10 was **0/50**.

The work below resolves concrete architecture, source-coverage, correction,
and portability shortcomings without hiding the remaining retrieval gap.
Already-revealed scores are compatibility floors and candidate-selection
evidence only.

## Retained general mechanisms

### 1. Better compact semantic representation

The retired BGE 384D verse/window candidate was replaced by
Snowflake Arctic-embed-m-v1.5 with one canonical BSB vector per verse,
Matryoshka-truncated to 256 dimensions. The choice was made across eight
revealed suites and 214 family cells, not by tuning to one frontier.

The embedding-space contract binds model and tokenizer hashes, CLS pooling,
maximum length 128, the query prefix, 256D truncation, and L2 normalization.
The Rust engine rejects a missing or different identity even when dimensions
match.

Why this should transfer: the change is corpus-wide and query-independent;
every canonical verse and every future query is encoded under the same
retrieval-trained contract. There is no prompt lookup or gold-derived rule.

### 2. Query-only clause planning

The whole query is retained, and as many as four independently meaningful
clauses can be embedded and searched. Generic composition reserves a distinct
primary candidate for each clause and a bounded round-robin of secondary
candidates. It is deterministic, preserves negation, and has no access to
packs, references, or evaluation data.

Why this should transfer: it addresses the structural failure mode in which a
single embedding collapses two memories. It applies uniformly to future
multi-clause text. It improves coverage monotonically in generic tests, but
the revealed multi-intent slice is still too weak to call resolved.

### 3. Comprehensive source ingestion with bounded authority

The overlay is not a few hardcoded samples. The current build ingests:

- all 6,687 supplied Selah topics, with 0 missing or skipped targets;
- 3,067 people, 1,264 places, and 450 events from the
  Study Bible/Theographic graph;
- 31,085 of 31,086 canonical KJV coordinates; and
- 247 bounded BSB events from 402 annotations, with 155 uncertain events
  quarantined. The previous eight provenance-free direct mappings and their
  17 aliases are not release inputs.

Graph records are score-zero explanation. Topic promotion requires independent
multi-cue and dense agreement. Hidden KJV phrases must be selective, and the
one-substitution fuzzy path is bounded, ambiguity-suppressed, and always
`Related`. ESV uses the same adapter boundary only when a host has an
appropriate license; no ESV text is bundled.

Why this should transfer: coverage comes from complete identified datasets and
manifested exclusions, not search failures. Authority is determined by source
type and corroboration rather than by whether an example appeared in an eval.

### 4. Source-backed correction corpus

The BibleAquifer candidate miner examined all 102,673 source rows and found
442 rows with explicit correction language. The identified
`codex-source-review-v1` source review used no evaluation data, approved 20,
and rejected 422. It was not an independent human editorial review. The
approved records contain 42 globally unique exact claim surfaces across 15
canonical books.

Every runtime correction carries database revision, row identity, content
hash, source excerpt, license, reviewer, date, and rationale. Build and runtime
checks reject unapproved/stale records and claims that collide with canonical
or supplied translation text. Matching is exact over the complete claim;
there is no fuzzy, partial, semantic, or gold-derived correction rule.

All 42 reviewed surfaces pass the source/runtime verifier: the cited anchor is
rank one with `OftenMisremembered`, the correction label, provenance, and all
declared structured passages. A same-binary A/B over the eight revealed suites
found byte-equivalent search systems before and after adding the correction
records, so the new lane did
not perturb unrelated ranking. This proves the reviewed source corpus was
implemented; it does not claim comprehensive coverage of every misconception.

### 5. Conservative fusion and confidence

Strong exact evidence protects the head. Dense evidence cannot turn a weak
match into `Recovered`; graph/context evidence cannot create a vote; and only
the lead result can make a high-confidence recovery claim. Independent
all-lane RRF, dense-rank summation, and ordinary Study-context votes were
reverted when they harmed breadth or hard-negative behavior.

Why this should transfer: the policy uses evidence type, selectivity, and
agreement rather than named passages or prompt families.

## Revealed compatibility snapshot

These suites were available during development and model selection. They are
not holdouts. The table uses the retained Arctic 256D verse-only profile plus
the generic clause/secondary composition and current source overlay.

| Revealed suite | R@1 | R@3 | R@10 | Interpretation |
|---|---:|---:|---:|---|
| Semantic quality v2 | 72.5 | 82.5 | 87.5 | below semantic release thresholds |
| Tip-of-the-tongue | 75.0 | 89.3 | 96.4 | compatibility pass at depth |
| Overlay adversarial | 72.4 | 89.7 | 100 | source-only rebuild loses former direct mappings |
| Overfit diagnostic | 57.1 | 67.9 | 78.6 | hard frontier remains visible |
| Overfit confirmation | 82.1 | 92.9 | 100 | strong, already revealed |
| Revealed v2 | 65.2 | 73.9 | 82.6 | original breadth failure remains visible |
| Family regression | 64.0 | 84.0 | 92.0 | named-event regressions exposed after alias retirement |
| Red-team prompts | 58.7 | 74.6 | 90.5 | diverse tail remains below target |

The frozen encoder bake-off, before the later generic secondary-coverage
adjustment, recorded suite-macro R@1/3/10 **70.44/84.20/91.40** and
family-cell R@3/10 **80.22/89.25**. Query-type slices were:

| Revealed slice | R@1 | R@3 | R@10 |
|---|---:|---:|---:|
| Meaning-only | 51.8 | 71.4 | 78.6 |
| Multi-intent | 0 | 40 | 60 |
| Entity/event | 74.5 | 90.9 | 100 |
| Translation | 92.9 | 97.6 | 97.6 |
| Phrase | 90.0 | 96.7 | 100 |

Confidence safety is 100% in the revealed candidate reports. Hard-negative
ordering is not yet a universal 100%: the semantic suite is 87.5%, and the
eight-suite encoder comparison is 90.5% where applicable. Required labels in
old prompt suites are not repaired from their gold answers; the new correction
lane is instead proven against its independently sourced 42-surface corpus.
The source-only overlay recheck also exposes structured-passage accuracy of
66.7% on overlay adversarial and 50% on family regression. Those regressions
are left visible rather than restoring the retired query-to-passage table.
The next sealed evaluation must reach 100% for every applicable safety,
hard-negative, label, and passage requirement.

## Rejected alternatives

Every rejection below is evidence against selecting a plausible-looking
single-suite gain.

| Candidate | Why it was plausible | Why it was rejected |
|---|---|---|
| BGE-small 384D | prior regression suite met 85/95 | weak sealed v2 transfer; inferior family-weighted bake-off |
| Arctic-S, MiniLM embeddings | smaller host assets | weaker equal-weight breadth than Arctic-M 256D |
| Nomic v1.5 256D | retrieval-trained Matryoshka model | weaker eight-suite macro and larger encoder assets |
| Arctic-M 128D | halves dense pack | materially worse recall while encoder remains the same size |
| Verse + window | improved revealed macro R@3 and multi-intent | doubles dense bytes and worsens hard negatives/slices |
| Enriched BSB/KJV/labels/passages document | comprehensive query-independent context | regresses macro and meaning-only recall |
| Sparse Study Note context | 91.4% aligned, interpretable evidence | no stable cutoff gain; risks a duplicate semantic vote |
| Dense Study-note table | source-authored explanatory representation | regresses multiple revealed suites |
| Full Study/OpenAI 1536D alignment | excellent source-coordinate holdout | broad query-transfer regression; adds 41,362,960 bytes (39.45 MiB) and a provenance boundary |
| Study PCA 256D | portable projection of source vectors | failed preregistered source gate before prompt scoring |
| Linear source query adapter | source R@3/10 improved | broad semantic/diagnostic/red-team transfer regression |
| Compact nonlinear adapter | only 66,720 bytes | source gain was small and selection reused its source-heldout partition; no untouched prompt proof |
| MiniLM/TinyBERT cross-encoders | pairwise relevance can improve order | no stable R@3 gain, hard-negative regression, added model and latency |
| Study cross-reference promotion | 404,501 source-derived related-passage edges | generic insertion and dense-top-N filtering showed no consistent broad gain and displaced stronger heads |
| Unrestricted RRF / dense-rank sum | conventional hybrid fusion | regressed revealed breadth or suppressed valid single-view candidates |

Detailed numbers, strengths, size, and latency are in
[`method-tradeoffs-v2.md`](./method-tradeoffs-v2.md)
(historical: [`archive/reports/method-tradeoffs-v1.md`](./archive/reports/method-tradeoffs-v1.md)).

## Artifact and portability evidence

| Runtime artifact | Bytes | SHA-256 prefix |
|---|---:|---|
| canonical BSB | 6,433,152 | `137ceb9c...` |
| expansion | 5,628,672 | `bf916e3f...` |
| corrected source/KJV overlay (unsourced mappings removed) | 7,440,320 | `3ec28f7d...` |
| Arctic-M 256D dense | 7,958,272 | `9551d428...` |

All engine data is 27,460,416 bytes (26.19 MiB). Fully offline query encoding
also requires the 110,145,162-byte quantized ONNX model and 711,649-byte
tokenizer, for about 131.91 MiB including packs. A server/host embedding path
needs only engine data.

Rust, C, Swift, Kotlin/JNI, and JavaScript/WASM consume the same packs and
result schema. The query planner is mirrored across host surfaces. Optional
linear query-adapter support uses content-addressed `VRQADP01`; no adapter is
required by the retained release.

On the development Mac, 220 warmed queries measured encoder median/p95
**3.347/6.137 ms** and persistent native all-lane search
**139.366/341.633 ms**. These are local observations, not target-device or
browser guarantees.

## Holdout discipline

The candidate-selection suites named above are revealed regressions. Three
later holdout files were contaminated and are permanently excluded; see
[`holdout-contamination-ledger.md`](./holdout-contamination-ledger.md). Their
prompt contents and any scores are not evidence in this report.

A legitimate next claim requires this exact sequence:

1. freeze the engine, source packs, encoder, adapter choice, fusion, and
   artifact hashes;
2. verify a new family-disjoint corpus against every development/regression
   family and gold passage without exposing its prompt text;
3. reveal exactly once with all family registries supplied;
4. save immutable first-shot JSON and Markdown reports;
5. immediately retire that batch to regression; and
6. if it fails, make no prompt- or gold-derived patch and freeze another
   disjoint holdout before a subsequent candidate.

The release gate is R@3 >= 85%, R@10 >= 95%, 100% confidence safety, 100%
applicable hard-negative ordering, 100% required labels and structured
passages, and no material exact-quote, translation, event, size, latency, or
portability regression.

## Integrity statement

No revealed prompt, prompt fragment, failed query, family, or gold reference
was added to runtime code or production data. The retained changes are
query-independent encoding, generic clause composition, complete identified
source ingestion, and separately reviewed source evidence. Until a new sealed
first shot clears every threshold, the accurate conclusion is:

> The engine has strong lexical, translation, event/entity/topic, correction,
> safety, integrity, and portability mechanisms. Its generalized semantic and
> multi-intent quality remains unproven and below the governing release gate.

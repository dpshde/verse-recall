# Guarded dense fusion bake-off v1

Status: **research candidate only; do not ship and do not describe semantic
quality as resolved.**

## Integrity boundary

This bake-off used 283 prompts (264 with gold passages) from eight already
revealed regression suites: semantic quality v2, tip-of-the-tongue,
overfit-diagnostic v1, overfit-confirmation v1, revealed v2, family regression
v1, red-team prompts, and overlay-adversarial v1. No contaminated v3-v5 file
and no sealed v6 file was opened or supplied to either evaluation helper.

The auxiliary representation is corpus-wide and query-independent: 31,086 BSB
focus verses plus deterministic paragraph context drawn from Study Bible
passage boundaries where available and a chapter-bounded fallback elsewhere.
It contains no evaluation prompt, family, or gold-derived alias.

The bake-off evaluated 242 fixed, generic policies: the two source rankings and
a grid of weighted reciprocal-rank fusion policies varying only protected head
length, `k`, auxiliary weight, and whether auxiliary-only candidates may enter.
Selecting a policy from this many revealed alternatives creates selection
optimism. Any candidate must be frozen before exactly one sealed holdout reveal.

## Main trade-off matrix

All safety and structured-output columns below were rescored from complete
result objects, not inferred from reference-only rankings. “Category macro”
and “family macro” give each represented category/family equal weight.

| Method | R@1 | R@3 | R@10 | R@100 | Category macro R@3 / R@10 | Family macro R@3 / R@10 | Confidence safety | Hard negatives | Passage output | Required labels |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Production verse view | 66.67% | 80.30% | 90.15% | 96.97% | 68.91% / 84.66% | 77.30% / 88.17% | 283/283 | 19/21 | 11/16 | 1/9 |
| Verse + canonical-passage best-view rank | 68.94% | 81.44% | 90.15% | 97.73% | 73.21% / 87.74% | 78.81% / 88.49% | 283/283 | 18/21 | 11/16 | 1/9 |
| Consensus RRF, primary candidates only (`p=0`, `k=60`, auxiliary `0.5`) | 66.67% | **84.09%** | **90.91%** | 96.97% | **77.82% / 86.40%** | **81.59% / 89.13%** | 283/283 | 19/21 | 11/16 | 1/9 |
| Protected-top-3 tail RRF (`p=3`, `k=60`, auxiliary `1.0`) | 66.67% | 80.30% | **91.29%** | 96.97% | 68.91% / **88.01%** | 77.30% / **89.60%** | 283/283 | 19/21 | 11/16 | 1/9 |

The unrestricted consensus policy gained 11 and lost 1 R@3 hits, gained 2 and
lost 0 R@10 hits, preserved R@1 exactly, and caused no aggregate R@3/R@10
regression in any of the eight suites. It reranked only candidates already
retrieved by the production view, so it did not obtain its gains by importing
auxiliary-only candidates.

The protected-top-3 policy is a stronger integrity guard: it preserved the
entire production head and gained 3/lost 0 R@10 hits. Its weakness is decisive:
it makes no progress at R@3.

## Diversity slices

| Slice | Production R@3 / R@10 | Consensus RRF R@3 / R@10 | Change |
|---|---:|---:|---:|
| Meaning / situation | 73.21% / 80.36% | 75.00% / 82.14% | +1.79 / +1.79 pp |
| Multi-intent | 28.57% / 57.14% | 28.57% / 57.14% | no change |
| Event / entity | 80.00% / 96.36% | 87.27% / 96.36% | +7.27 / 0 pp |
| Remembered wording | 94.03% / 100.00% | 97.01% / 100.00% | +2.98 / 0 pp |
| Translation drift | 97.62% / 97.62% | 97.62% / 97.62% | no change |

The apparent aggregate improvement is therefore not a solution to the hardest
problem. Multi-intent recall is unchanged, and meaning-only depth remains far
below the release bar. One `semantic-unseen` case also moved out of the top
three even though the suite aggregate improved.

## Suite matrix for the strongest revealed consensus policy

| Suite | Production R@3 / R@10 | Consensus RRF R@3 / R@10 |
|---|---:|---:|
| Confirmation | 92.86% / 100.00% | 92.86% / 100.00% |
| Diagnostic | 67.86% / 78.57% | 71.43% / 82.14% |
| Family regression | 84.00% / 92.00% | 88.00% / 92.00% |
| Overlay adversarial | 86.21% / 96.55% | 89.66% / 96.55% |
| Red team | 74.60% / 90.48% | 82.54% / 90.48% |
| Revealed v2 | 73.91% / 82.61% | 78.26% / 82.61% |
| Semantic quality v2 | 80.00% / 85.00% | 82.50% / 87.50% |
| Tip-of-the-tongue | 89.29% / 96.43% | 89.29% / 96.43% |

## Size and latency

| Artifact / execution | Bytes | Change |
|---|---:|---:|
| Production 256D verse dense pack | 7,958,272 | baseline |
| Verse + canonical-passage dense pack | 15,916,288 | +7,958,016 bytes; 2.00x dense payload |

Across the same 283 release-mode searches at depth 100, the production engine
measured 193.10 ms mean / 155.37 ms median and the two-table auxiliary engine
measured 199.33 ms mean / 160.44 ms median. The auxiliary engine alone was
3.2% slower by mean. The experimental final-output consensus currently needs
both rankings, so naïve dual-engine execution is 2.03x the production mean.
A shared-sparse-lanes implementation could approach the smaller two-table
overhead, but that architecture has not been implemented or measured.

The current auxiliary LVEC is also encoded under the legacy second-table
`Window` role. A natural paragraph representation must receive an explicit
`Passage` role before any runtime experiment; otherwise the engine's fixed
±1-verse window metadata would assert false provenance.

## Decision

Do not modify `recall-core` or the release artifact for this result. The
standalone auxiliary view regresses two suites and one hard negative. Guarded
tail fusion offers only three extra top-10 hits for a doubled dense payload.
The stronger consensus RRF is a legitimate, query-independent research
candidate, but it was selected from 242 revealed policies, leaves R@3 at
84.09%, R@10 at 90.91%, hard negatives at 90.48%, passage output at 68.75%,
required labels at 11.11%, and multi-intent R@3 at 28.57%.

If the project chooses to spend the next sealed reveal on this direction, the
only defensible candidate is the primary-candidate consensus policy
(`p=0`, `k=60`, auxiliary weight `0.5`) after freezing an explicit `Passage`
role and a shared-view implementation. A holdout pass would still not erase
the unresolved label, passage, hard-negative, and multi-intent compatibility
failures above. The safer recommendation is to retain production verse-only
ranking and invest in a general clause-aware representation/reranker before
using a sealed reveal.

## Reproduction evidence

- Export helper: `crates/recall-eval/examples/export_dense_pair.rs`
- Fusion evaluator: `crates/recall-eval/examples/evaluate_dense_pair.rs`
- Timed paired export (temporary, 48,631,109 bytes):
  `/private/tmp/dense-pair-revealed-v1-timed.json`, SHA-256
  `78d8f18a95b3a3c42b4f4fe4b631a8e48e3c4eb4b56f3fb9e52ff277b7cbdc95`
- Complete 242-policy grid (temporary, 12,719,773 bytes):
  `/private/tmp/guarded-dense-fusion-grid-v1.json`, SHA-256
  `dfe7b26e395b9106a4d4e7271958adad29e94a7e54795c4386f0b3292d651c2b`
- Production dense SHA-256:
  `9551d428dedde6168326e2dcc5c7818abcbd86de8334b9310b6c90a2cc0afd8d`
- Auxiliary dense SHA-256:
  `18cf672e8d5a66d43a70fd7a956809173a5c91f3a8047850a834854fe740dda9`

No revealed prompt, fragment, family, or gold reference was added to production
code or runtime data.

# GTE ModernBERT and fixed Arctic+GTE ensemble bake-off

Status: **both candidates rejected**. This is revealed-regression candidate
selection evidence, not a sealed generalization result.

## Protocol

The bake-off used the 283 prompts (264 gold-bearing) in the eight pinned,
already-revealed suites enforced by `scripts/evaluate_encoder_bakeoff.py`:
semantic quality v2, tip-of-the-tongue, overfit diagnostic, overfit
confirmation, retired overfit v2, family regression, red-team regression, and
overlay adversarial. The harness checks each complete prompt-file SHA-256 and
accepts only `regression-*` rows. It does not discover evaluation files or
accept an arbitrary prompt path.

No v3, v4, v5, or v6 holdout was opened or evaluated. No prompt, family,
failed result, or gold reference informed either representation.

All runs used the same current normalizer-v2 base and expansion packs, the same
source-only recall overlay, retrieval depth 100, generic query-only clause
segments, and the unchanged runtime fusion. The raw dense diagnostic isolates
the representation; product fusion measures the candidate with the existing
lexical, expansion, translation, entity, and event lanes.

GTE used direct 768D CLS embeddings from
`Alibaba-NLP/gte-modernbert-base` revision
`e7f32e3c00f91d699e8c43b53106206bcc72bb22`, with no query or corpus prefix.
Its embedding-space ID is
`12ad4a5b5f4440d7c91a0ab885fa96d46ed18fa7ffeadc87dfbb7f86b657c0c0`.

The ensemble policy was frozen before evaluation: independently L2-normalize
Arctic-256 and GTE-768, multiply each by `sqrt(0.5)`, concatenate, and L2
normalize the result once. Therefore joint cosine is the equal-weight mean of
the two component cosines apart from Float32 rounding. There is no learned or
query-dependent weight. Its embedding-space ID is
`b412e92eb55dff52c3d0917e8d47b410ea9026c4ca70538944cd5fe0a23338cb`.

## Product-fusion results

| Candidate | R@1 | R@3 | R@10 | R@50 | R@100 | MRR | Family-macro R@3 | Family-macro R@10 | Category-macro R@3 | Safety | Hard negatives | Passages | Required labels |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Arctic-256 incumbent | 66.7% | 80.3% | 90.2% | 95.5% | 97.0% | 0.749 | 77.7% | 88.4% | 70.7% | 283/283 | 19/21 | 11/16 | 1/9 |
| GTE-768 | 61.0% | 75.8% | 88.6% | 95.8% | 97.3% | 0.707 | 73.0% | 87.2% | 68.4% | 283/283 | 19/21 | 11/16 | 1/9 |
| Arctic-256 + GTE-768 equal ensemble | 64.4% | 82.2% | 90.9% | 97.0% | 99.2% | 0.746 | 80.5% | 89.8% | 73.3% | 283/283 | **18/21** | 11/16 | 1/9 |

GTE alone is not a broad improvement. Against Arctic it gains 0.4 points at
R@50 and 0.4 at R@100 but loses 5.7 points at R@1, 4.5 at R@3, 1.5 at R@10,
and 4.8 family-macro points at R@3.

The fixed ensemble improves pooled R@3 by 1.9 points, pooled R@10 by 0.8,
family-macro R@3 by 2.8, family-macro R@10 by 1.4, and R@100 by 2.3 versus
Arctic. That gain is not acceptable because hard-negative ordering regresses
from 19/21 to 18/21. The two component encoders fail different meaning-only
hard negatives; equal concatenation retains both component-specific failures
instead of resolving either. The ensemble also remains below the release bars
and does not improve structured passages or required correction labels.

## Per-suite product R@3 / R@10

| Suite | Arctic-256 | GTE-768 | Equal ensemble |
|---|---:|---:|---:|
| Semantic quality v2 | 80.0% / 85.0% | 80.0% / 87.5% | 80.0% / 90.0% |
| Tip of the tongue | 89.3% / 96.4% | 92.9% / 96.4% | 92.9% / 96.4% |
| Overfit diagnostic | 67.9% / 78.6% | 60.7% / 75.0% | 64.3% / 78.6% |
| Overfit confirmation | 92.9% / 100% | 78.6% / 96.4% | 96.4% / 100% |
| Retired overfit v2 | 73.9% / 82.6% | 73.9% / 91.3% | 82.6% / 91.3% |
| Family regression | 84.0% / 92.0% | 84.0% / 84.0% | 84.0% / 88.0% |
| Red-team regression | 74.6% / 90.5% | 61.9% / 85.7% | 74.6% / 88.9% |
| Overlay adversarial | 86.2% / 96.6% | 89.7% / 96.6% | 93.1% / 96.6% |

The suite matrix explains the rejection: GTE's gains on translation/overlay
and retired-v2 cases do not transfer to the diagnostic and red-team slices.
The ensemble improves several heads but gives back depth on family and
red-team regressions.

## Raw dense results

| Representation | R@1 | R@3 | R@10 | R@50 | R@100 | MRR | Family-macro R@3 | Family-macro R@10 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Arctic-256 | 62.5% | 76.1% | 85.6% | 92.8% | 94.7% | 0.704 | 73.8% | 84.0% |
| GTE-768 | 54.2% | 71.2% | 79.9% | 91.3% | 93.6% | 0.637 | 69.3% | 78.9% |
| Equal ensemble | 60.2% | 79.2% | 87.5% | 93.6% | 96.6% | 0.706 | 77.6% | 86.6% |

The ensemble does increase dense candidate coverage, confirming that the two
encoders carry complementary evidence. The unsafe ordering trade and the
unchanged correction/passage failures show why candidate coverage alone is not
a release criterion.

## Artifact and runtime cost

| Representation | Dimensions | Dense pack | Query encoder assets | Full-scan dot dimensions per query | Relative encoder calls |
|---|---:|---:|---:|---:|---:|
| Arctic-256 | 256 | 7,958,272 bytes (7.59 MiB) | 110,856,811 bytes (105.72 MiB) | 7,958,016 | 1 |
| GTE-768 | 768 | 23,874,304 bytes (22.77 MiB) | 153,801,244 bytes (146.68 MiB) | 23,874,048 | 1 |
| Equal ensemble | 1,024 | 31,832,320 bytes (30.36 MiB) | 264,658,055 bytes (252.40 MiB) | 31,832,064 | 2 |

The equal ensemble makes the dense payload 4.0x the incumbent, performs 4.0x
as many full-scan dot dimensions, and requires both query encoders. Those costs
would need a decisive, safety-preserving recall gain; this experiment does not
provide one.

## Reproduction identities

- Arctic dense pack SHA-256:
  `9551d428dedde6168326e2dcc5c7818abcbd86de8334b9310b6c90a2cc0afd8d`
- GTE dense pack SHA-256:
  `f70bcf680aa7b0c40b12cab2562e8097c548006c78d59424e874b359d7f2afe2`
- Ensemble dense pack SHA-256:
  `d45c6c11ec597d3a2b87216eef53e147ee61ac712a0f91fc932a7b090d88643f`
- Ensemble Float32 corpus SHA-256:
  `69b6a7948395fb625fe5e3ba34f31f04ae347702a51ace3ec8d9dbf55df87bac`
- Ensemble contract file SHA-256:
  `8a58d575c4609eef5c86cc8a7f63c1bada48ed8654a3c194aa2c9ca5d2114e6f`
- GTE model SHA-256:
  `bae96b276d342bf86eeee07c1bdbc0c75bb82bf4033941aab7fabc1e33ee3b44`
- GTE tokenizer SHA-256:
  `6c8aaa9a542084f2457eab775d4eeb51f92a70c0fd9de28d5edb0ddec3c08d30`

Individual immutable evaluator JSON/Markdown reports and prompt-bound query
vectors are under `/private/tmp/gte-{suite}-*` and
`/private/tmp/arctic-gte-equal-ensemble/{suite}-*`. The generic transformation
is implemented and tested in `scripts/concatenate_semantic_vectors.py`.

## Decision

Retain Arctic-256 verse-only as the compatibility floor for now. Reject GTE as
a replacement and quarantine the fixed ensemble as a research result. Neither
candidate should be presented to a sealed holdout, because revealed evidence
already fails the stop rules. The next representation candidate should address
hard-negative ordering and correction/passage output through generic training
or independently sourced evidence, then pass this compatibility matrix before
a new holdout is frozen.

No revealed prompt or gold answer was added to production code or data.

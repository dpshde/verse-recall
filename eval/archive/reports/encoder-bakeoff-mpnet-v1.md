# Licensed MPNet encoder bake-off

Date: 2026-07-16

## Decision

Reject `sentence-transformers/all-mpnet-base-v2` as the release encoder. This
is an independently motivated, Apache-2.0, 109.5M-parameter sentence/paragraph
encoder trained on more than one billion paired sentences. It was evaluated
over the full 31,086-verse BSB corpus and all eight explicitly enumerated,
already-revealed regression suites. No sealed or contaminated holdout was read.

The candidate is weaker than the retained Arctic-M 256D representation at
every pooled release cutoff while using three times as many vector dimensions:

| Product representation | R@1 | R@3 | R@10 | R@100 | Hard negatives | Passage output | LVEC bytes |
|---|---:|---:|---:|---:|---:|---:|---:|
| Arctic-M 256D, bounded semantic-tail schedule | 66.7% | 80.3% | 90.9% | 97.0% | 19/21 | 11/16 | 7,958,272 |
| all-MPNet-base-v2 768D | 61.4% | 75.0% | 88.6% | 97.0% | 19/21 | 12/16 | 23,874,304 |

MPNet's suite-macro R@3 was 76.6% and family-cell-macro R@3 was 72.8%.
Raw dense retrieval reached only R@1/3/10 51.9/65.2/75.8. Although passage
output gained one case, the candidate lost 14 pooled rank-three hits and 4
rank-ten hits. The isolated passage gain does not justify a broad head-recall
regression, 15.9 MiB larger dense pack, or a slightly larger host encoder.

## Reproduction identity

- Repository: <https://huggingface.co/sentence-transformers/all-mpnet-base-v2>
- Revision: `e8c3b32edf5434bc2275fc9bab85f82640a19130`
- License declared by the upstream model card: Apache-2.0
- Quantized ARM64 ONNX SHA-256:
  `c392a9c545c7d4438a16fed8287a76a576b27eaf029c1c23bbf78a7a666d197f`
- Tokenizer SHA-256:
  `b8be2c30ba5dd723a6d5ee26d013da103d5408d92ddcb23747622f9e48f1d842`
- Mean pooling, L2 normalization, maximum length 128, 768 dimensions
- Corpus-vector SHA-256:
  `6a50f70ea99649026c8aeeb0cacfd43d78281325e7c4eebdcde39bc023f58bd4`
- Embedding-space ID:
  `26ca3054ceb265d6fb68cd4c0d9962466adbf1dac57115a69f4a3fa6717b40c2`
- Dense-pack SHA-256:
  `97cd6c47cdef464d24e019b02bd4976322097d8ab16d2637ed861353fb55b00d`
- Full generated report:
  `/private/tmp/all-mpnet-revealed-bakeoff-v1/encoder-bakeoff.{md,json}`

The unlicensed-or-undeclared `multi-qa-mpnet-base-dot-v1` variant was not
selected for a distributable release experiment because its upstream model
metadata does not declare a license. A plausible model is not a valid portable
artifact until redistribution rights are explicit.

## Interpretation

This result closes a meaningful alternative rather than tuning another model
against individual misses. It reinforces the current diagnosis: the next
quality gains must come from generic event/source representations and better
candidate composition, not merely a larger conventional sentence encoder.
The result is regression-selection evidence only and makes no generalized
semantic-quality claim.

# Revealed encoder bake-off protocol

`scripts/evaluate_encoder_bakeoff.py` evaluates one frozen verse-only encoder
against exactly eight already-revealed regression suites. The suite paths and
SHA-256 identities are pinned in the script. It does not glob `eval/`, accepts
only `regression-*` split rows, and has no option for adding a prompt corpus.
Consequently, this harness is not a sealed-holdout runner.

The candidate must be encoded symmetrically: same model/tokenizer bytes,
pooling, maximum length, prefix policy, dimensionality, and normalization for
the canonical BSB corpus and all queries. Query records include
`prompt_utf8_sha256` and generic query-only clause segments. The evaluator
checks the prompt digest, segment plan, dimensions, and exact embedding-space
ID before search.

For the pinned `Alibaba-NLP/gte-modernbert-base` experiment, first build the
release tools and encode the corpus:

```sh
cargo build --release -p recall-packer -p recall-eval
PYTHONDONTWRITEBYTECODE=1 /Users/user/.cache/uv/archive-v0/_3o2MC6p-Uc5D-WFmuyV-/bin/python scripts/build_semantic_vectors.py corpus \
  --input data/bsb.jsonl \
  --verse-output /private/tmp/gte-modernbert-bsb-verse.f32 \
  --model-dir /private/tmp/gte-modernbert-base \
  --pooling cls \
  --max-length 128 \
  --batch-size 64
```

Then run the bake-off:

```sh
PYTHONDONTWRITEBYTECODE=1 /Users/user/.cache/uv/archive-v0/_3o2MC6p-Uc5D-WFmuyV-/bin/python scripts/evaluate_encoder_bakeoff.py \
  --python /Users/user/.cache/uv/archive-v0/_3o2MC6p-Uc5D-WFmuyV-/bin/python \
  --model-dir /private/tmp/gte-modernbert-base \
  --corpus-vectors /private/tmp/gte-modernbert-bsb-verse.f32 \
  --dimensions 768 \
  --pooling cls \
  --max-length 128 \
  --batch-size 64 \
  --candidate-name gte-modernbert-base-verse-only \
  --model-repository Alibaba-NLP/gte-modernbert-base \
  --model-revision e7f32e3c00f91d699e8c43b53106206bcc72bb22 \
  --expected-model-sha256 bae96b276d342bf86eeee07c1bdbc0c75bb82bf4033941aab7fabc1e33ee3b44 \
  --expected-tokenizer-sha256 6c8aaa9a542084f2457eab775d4eeb51f92a70c0fd9de28d5edb0ddec3c08d30 \
  --base /private/tmp/bsb-full-normalizer-v2.pack \
  --expand /private/tmp/bsb-expand-normalizer-v2.pack \
  --output-dir /private/tmp/gte-modernbert-revealed-bakeoff-v1
```

The final reports are:

- `/private/tmp/gte-modernbert-revealed-bakeoff-v1/encoder-bakeoff.md`
- `/private/tmp/gte-modernbert-revealed-bakeoff-v1/encoder-bakeoff.json`

The JSON retains exact artifact, prompt, query-vector, evaluator-report, model,
tokenizer, corpus-vector, and embedding-contract identities plus reproduction
argument arrays. The Markdown compares product fusion and raw dense retrieval
through R@100, including suite-weighted and family-weighted views.

Use these results only to select or reject a representation. If a candidate is
selected, freeze its source, artifact hashes, contract, and evaluator before a
single evaluation on a fresh family-disjoint holdout. A revealed-suite win is
not a generalized semantic-quality claim, and individual misses must not be
converted into runtime aliases, boosts, rules, or gold-derived data.

## Fixed concatenated ensembles

`scripts/concatenate_semantic_vectors.py` provides a generic, source-only
weighted concatenation. A contract names two or more independent embedding
spaces, dimensions, and positive weights. The builder canonicalizes weights to
fractions, independently normalizes each component, scales it by the square
root of its fraction, concatenates the components, and normalizes once more.
It applies the identical operation to corpus, whole-query, and clause-segment
vectors while requiring exact prompt digests, row IDs, planners, and segment
texts across components.

The fixed equal Arctic-256 + GTE-768 experiment was prepared with:

```sh
PYTHONDONTWRITEBYTECODE=1 /Users/user/.cache/uv/archive-v0/_3o2MC6p-Uc5D-WFmuyV-/bin/python scripts/concatenate_semantic_vectors.py contract \
  --component arctic=36db5d816aeac4c6f6631619d587d7caea5f829fcddfff51450d60823e58d744=256=1 \
  --component gte=12ad4a5b5f4440d7c91a0ab885fa96d46ed18fa7ffeadc87dfbb7f86b657c0c0=768=1 \
  --output /private/tmp/arctic-gte-equal-ensemble/embedding-space.json

PYTHONDONTWRITEBYTECODE=1 /Users/user/.cache/uv/archive-v0/_3o2MC6p-Uc5D-WFmuyV-/bin/python scripts/concatenate_semantic_vectors.py corpus \
  --contract /private/tmp/arctic-gte-equal-ensemble/embedding-space.json \
  --input arctic=/private/tmp/arctic-m256-bsb-verse.f32 \
  --input gte=/private/tmp/gte-modernbert-bsb-verse.f32 \
  --output /private/tmp/arctic-gte-equal-ensemble/bsb-verse.f32
```

For each revealed suite, run the `queries` subcommand with the corresponding
Arctic and GTE prompt-bound query files, then pack the 1,024D corpus with the
contract's embedding-space ID. The complete result and rejection rationale are
in `eval/encoder-bakeoff-gte-ensemble-v1.md`.

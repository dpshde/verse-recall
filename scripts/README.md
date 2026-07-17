# Build and evaluation scripts

Scripts fall into three tiers. Prefer the **release path** unless you are
running a documented experiment.

## Release path

| Script | Purpose |
|---|---|
| `ingest_bsb.py` | Canonical BSB display-ref JSONL |
| `build_expand_neighbors.py` | Weighted expansion graph intermediate |
| `build_recall_overlay.py` | KJV + events + topics + graph + corrections → overlay JSONL |
| `build_correction_records.py` | Source-reviewed correction corpus builder |
| `build_semantic_vectors.py` | Arctic (or other) corpus/query vectors + space contract |
| `verify_release_artifacts.py` | Six-artifact pack + contract integrity |
| `verify_reviewed_correction_runtime.py` | End-to-end correction surface checks |
| `verify_release_candidate_freeze.py` | Freeze identity before a sealed reveal |
| `retire_holdout.py` | Mechanical holdout → regression retirement |

Pack compilation is `cargo run -p recall-packer --release -- …`
(see root `README.md` and `data/README.md`).

## Evaluation helpers

| Script | Purpose |
|---|---|
| `evaluate_method_tradeoffs.py` | Historical method matrix (v1) |
| `evaluate_method_tradeoffs_v2.py` | Current-pack method matrix + majority win-rate |
| `evaluate_encoder_bakeoff.py` | Encoder/representation comparison |
| `evaluate_grouped_passage_fusion.py` | Passage-group fusion ablations |
| `evaluate_semantic_concept_fusion.py` | Concept-fusion experiments |
| `evaluate_cross_encoder_reranker.py` | Late-interaction / cross-encoder probes |
| `serve_search_ui.py` | Local manual search UI |

## Research / rejected-candidate tooling

These remain for reproducibility. **None is required to ship the four-pack
baseline.** Do not promote their outputs into release packs without a sealed
holdout and the `AGENTS.md` discipline.

| Script | Notes |
|---|---|
| `build_semantic_documents.py` | Enriched opaque verse documents (rejected for release) |
| `build_passage_documents.py` | Study passage ranges for optional `PVEC` |
| `build_source_semantic_documents.py` | Typed Study/graph multi-document corpus |
| `build_semantic_concept_corpus.py` | Concept corpus experiments |
| `build_study_dense_view.py` | Study-note dense tables |
| `build_study_vector_alignment.py` | Align Study/OpenAI vectors |
| `build_study_pca_alignment.py` | PCA alignment experiments |
| `build_aligned_translation_corpus.py` | Translation-alignment helpers |
| `concatenate_semantic_vectors.py` | Multi-table vector concatenation |
| `fit_semantic_query_adapter.py` | Linear query adapter fit |
| `train_source_query_adapter.py` | Source-domain adapter training |
| `source_query_adapter.py` | Adapter application helpers |
| `embedding_space_contract.py` | Shared space-contract library |

## Policy

Scripts must not read evaluation gold answers to invent runtime aliases.
Source datasets only, with provenance and measurable coverage.

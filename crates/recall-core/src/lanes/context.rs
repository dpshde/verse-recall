use crate::lanes::{Lane, LaneHit, LaneId, PackedLexicalLane};
use crate::normalize::NormalizedQuery;
use crate::result::Confidence;
use recall_pack::PackError;

/// Stable provenance marker attached to every auxiliary-context candidate.
pub const STUDY_CONTEXT_PROVENANCE: &str = "context:study";

/// Sparse search over verse-aligned explanatory documents.
///
/// Context prose is useful candidate evidence, but it is neither canonical
/// wording nor a reviewed query-to-passage mapping. This wrapper therefore
/// reuses the compact BM25 implementation while permanently removing its
/// canonical direct-recovery semantics.
pub struct ContextLexicalLane {
    lexical: PackedLexicalLane,
}

impl ContextLexicalLane {
    pub fn new_owned(
        lex_bytes: Vec<u8>,
        post_bytes: Vec<u8>,
        expected_document_count: usize,
    ) -> Result<Self, PackError> {
        Ok(Self {
            lexical: PackedLexicalLane::new_owned(lex_bytes, post_bytes, expected_document_count)?,
        })
    }
}

impl Lane for ContextLexicalLane {
    fn id(&self) -> LaneId {
        // Auxiliary prose is semantic evidence. Sharing conceptual Lane C
        // also ensures dense and sparse semantic providers consolidate to one
        // vote instead of manufacturing cross-lane agreement.
        LaneId::C
    }

    fn available(&self) -> bool {
        self.lexical.document_count() > 0
    }

    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit> {
        let mut hits = self.lexical.search(query, top_k);
        for hit in &mut hits {
            hit.confidence_hint = Confidence::Related;
            hit.ranking_tier = 0;
            hit.direct_tier = 0;
            if !hit
                .expanded_terms
                .iter()
                .any(|term| term == STUDY_CONTEXT_PROVENANCE)
            {
                hit.expanded_terms
                    .push(STUDY_CONTEXT_PROVENANCE.to_string());
            }
        }
        hits
    }
}

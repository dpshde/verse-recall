use crate::error::{CoreError, CoreResult};
use crate::fuse::{fuse_lanes, stitch_passage_results, FuseWeights};
use crate::lanes::{
    ContextLexicalLane, DenseLane, DenseMetadata, Lane, LaneId, PackedExpandLane,
    PackedLexicalLane, PassageDenseLane, PassageDenseMetadata, RecallOverlay, TextScanLane,
};
use crate::normalize::{normalize_query, semantic_query_stems};
use crate::planner::{plan_query_composition, BoundaryRelation, MAX_QUERY_SEGMENTS};
use crate::result::SearchResponse;
use crate::types::{SearchOpts, MAX_TOP_K};
use recall_pack::{PackReader, SectionTag, Verse};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Candidate generation is deliberately independent from the caller's output
/// depth. Otherwise asking for 1 result can produce a different rank 1 than
/// asking for 10 because each lane exposes a different candidate pool to RRF.
const LANE_CANDIDATE_DEPTH: usize = 128;

fn bounded_search_opts(opts: &SearchOpts) -> SearchOpts {
    opts.clone().with_top_k(opts.top_k)
}

/// One independently meaningful part of a compound query, paired with the
/// embedding produced for that exact text by the same host encoder as the
/// whole-query embedding.
///
/// `recall-core` provides a query-only clause planner; inference deliberately
/// stays host-side so native, web, and mobile callers can share deterministic
/// composition without coupling the runtime to a tokenizer or model runtime.
#[derive(Clone, Copy, Debug)]
pub struct QuerySegmentEmbedding<'a> {
    pub text: &'a str,
    pub embedding: &'a [f32],
}

/// Private search metadata needed by query-view composition. Authority is not
/// inferred from the public confidence label: an exact but recurrent phrase
/// may be deliberately presented as `Related` while still deserving head
/// protection, and an uncorroborated dense leader never does.
struct InternalSearchResponse {
    response: SearchResponse,
    protect_head: bool,
    /// Whether the returned head is uniquely ranked within at least one
    /// authoritative query-local view. This is an ordinal tie decision only;
    /// composition never thresholds or compares magnitudes across queries.
    head_is_decisive: bool,
}

/// In-memory engine over one or more pack buffers.
///
/// Owns copies of pack bytes so the host may free its buffers after open.
pub struct Engine {
    verses: Arc<[Verse]>,
    translation_id: String,
    weights: FuseWeights,
    lanes: Vec<Box<dyn Lane>>,
    context_lane: Option<ContextLexicalLane>,
    dense_lane: Option<DenseLane>,
    passage_dense_lane: Option<PassageDenseLane>,
}

impl Engine {
    /// Open engine from one or more pack byte buffers (same translation).
    pub fn open(packs: &[&[u8]]) -> CoreResult<Self> {
        if packs.is_empty() {
            return Err(CoreError::EmptyPacks);
        }

        let mut verses: Option<Vec<Verse>> = None;
        let mut translation_id = String::from("unknown");
        let mut weights = FuseWeights::default();
        let mut overlay_sections: Vec<(&[u8], String, usize)> = Vec::new();
        let mut canonical_section_overlays: Vec<(String, usize)> = Vec::new();
        let mut dense_section: Option<(&[u8], String, usize)> = None;
        let mut passage_dense_section: Option<(&[u8], String, usize)> = None;
        let mut lex_section: Option<(Vec<u8>, String, usize)> = None;
        let mut post_section: Option<(Vec<u8>, String, usize)> = None;
        let mut context_lex_section: Option<(Vec<u8>, String, usize)> = None;
        let mut context_post_section: Option<(Vec<u8>, String, usize)> = None;
        let mut expand_section: Option<(Vec<u8>, String, usize)> = None;

        for buf in packs {
            let reader = PackReader::open(buf)?;
            let meta = if reader.has(SectionTag::Meta) {
                Some(reader.meta()?)
            } else {
                None
            };
            if reader.has(SectionTag::Text) {
                if verses.is_some() {
                    return Err(CoreError::MultipleTextSections);
                }
                let text = reader.text()?;
                if let Some(meta) = &meta {
                    if meta.verse_count as usize != text.verses.len() {
                        return Err(CoreError::Pack(recall_pack::PackError::InvalidMeta));
                    }
                }
                verses = Some(text.verses);
                if let Some(meta) = &meta {
                    translation_id = meta.translation_id.clone();
                    if meta.weight_a > 0.0 {
                        weights.a = meta.weight_a;
                    }
                    if meta.weight_b > 0.0 {
                        weights.b = meta.weight_b;
                    }
                    if meta.weight_c > 0.0 {
                        weights.c = meta.weight_c;
                    }
                    if meta.weight_d > 0.0 {
                        weights.d = meta.weight_d;
                    }
                }
            }
            if reader.has(SectionTag::Phrase) {
                let recall_overlay_target = meta
                    .as_ref()
                    .and_then(|meta| meta.translation_id.strip_prefix("OVERLAY:"));
                if let Some(target) = recall_overlay_target {
                    if reader.has(SectionTag::Text) {
                        return Err(CoreError::InvalidOverlay(
                            "recall overlay pack must not contain TEXT".into(),
                        ));
                    }
                    let overlay_meta = meta.as_ref().expect("overlay target came from META");
                    overlay_sections.push((
                        reader.section_bytes(SectionTag::Phrase)?,
                        target.to_string(),
                        overlay_meta.verse_count as usize,
                    ));
                } else if !reader.has(SectionTag::Text) {
                    let Some(meta) = &meta else {
                        return Err(CoreError::InvalidOverlay(
                            "section-only canonical pack with PHRASE requires META".into(),
                        ));
                    };
                    canonical_section_overlays
                        .push((meta.translation_id.clone(), meta.verse_count as usize));
                }
            }
            if reader.has(SectionTag::Lvec) {
                if dense_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "multiple LVEC sections were supplied".into(),
                    ));
                }
                let Some(meta) = &meta else {
                    return Err(CoreError::InvalidOverlay(
                        "LVEC section requires META".into(),
                    ));
                };
                dense_section = Some((
                    reader.section_bytes(SectionTag::Lvec)?,
                    meta.translation_id.clone(),
                    meta.verse_count as usize,
                ));
            }
            if reader.has(SectionTag::Pvec) {
                if passage_dense_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "multiple PVEC sections were supplied".into(),
                    ));
                }
                let Some(meta) = &meta else {
                    return Err(CoreError::InvalidOverlay(
                        "PVEC section requires META".into(),
                    ));
                };
                passage_dense_section = Some((
                    reader.section_bytes(SectionTag::Pvec)?,
                    meta.translation_id.clone(),
                    meta.verse_count as usize,
                ));
            }
            if reader.has(SectionTag::Lex) {
                if lex_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "multiple LEX sections were supplied".into(),
                    ));
                }
                let Some(meta) = &meta else {
                    return Err(CoreError::InvalidOverlay(
                        "LEX section requires META".into(),
                    ));
                };
                lex_section = Some((
                    reader.section_bytes(SectionTag::Lex)?.to_vec(),
                    meta.translation_id.clone(),
                    meta.verse_count as usize,
                ));
            }
            if reader.has(SectionTag::Post) {
                if post_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "multiple POST sections were supplied".into(),
                    ));
                }
                let Some(meta) = &meta else {
                    return Err(CoreError::InvalidOverlay(
                        "POST section requires META".into(),
                    ));
                };
                post_section = Some((
                    reader.section_bytes(SectionTag::Post)?.to_vec(),
                    meta.translation_id.clone(),
                    meta.verse_count as usize,
                ));
            }
            if reader.has(SectionTag::Clex) {
                if context_lex_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "multiple CLEX sections were supplied".into(),
                    ));
                }
                let Some(meta) = &meta else {
                    return Err(CoreError::InvalidOverlay(
                        "CLEX section requires META".into(),
                    ));
                };
                context_lex_section = Some((
                    reader.section_bytes(SectionTag::Clex)?.to_vec(),
                    meta.translation_id.clone(),
                    meta.verse_count as usize,
                ));
            }
            if reader.has(SectionTag::Cpost) {
                if context_post_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "multiple CPOST sections were supplied".into(),
                    ));
                }
                let Some(meta) = &meta else {
                    return Err(CoreError::InvalidOverlay(
                        "CPOST section requires META".into(),
                    ));
                };
                context_post_section = Some((
                    reader.section_bytes(SectionTag::Cpost)?.to_vec(),
                    meta.translation_id.clone(),
                    meta.verse_count as usize,
                ));
            }
            if reader.has(SectionTag::Expand) {
                if expand_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "multiple EXPAND sections were supplied".into(),
                    ));
                }
                let Some(meta) = &meta else {
                    return Err(CoreError::InvalidOverlay(
                        "EXPAND section requires META".into(),
                    ));
                };
                expand_section = Some((
                    reader.section_bytes(SectionTag::Expand)?.to_vec(),
                    meta.translation_id.clone(),
                    meta.verse_count as usize,
                ));
            }
        }

        let verses: Arc<[Verse]> = verses.ok_or(CoreError::NoTextSection)?.into();
        for (_, overlay_translation, overlay_verse_count) in &overlay_sections {
            if overlay_translation.trim().is_empty()
                || !overlay_translation.eq_ignore_ascii_case(&translation_id)
            {
                return Err(CoreError::InvalidOverlay(format!(
                    "recall overlay META targets {overlay_translation:?} but base pack is {translation_id}"
                )));
            }
            if *overlay_verse_count != verses.len() {
                return Err(CoreError::InvalidOverlay(format!(
                    "recall overlay META verse count {overlay_verse_count} does not match base pack {}",
                    verses.len()
                )));
            }
        }
        for (overlay_translation, overlay_verse_count) in canonical_section_overlays {
            if !overlay_translation.eq_ignore_ascii_case(&translation_id) {
                return Err(CoreError::InvalidOverlay(format!(
                    "canonical section pack targets {overlay_translation} but base pack is {translation_id}"
                )));
            }
            if overlay_verse_count != verses.len() {
                return Err(CoreError::InvalidOverlay(format!(
                    "canonical section pack verse count {overlay_verse_count} does not match base pack {}",
                    verses.len()
                )));
            }
        }
        let dense_lane = if let Some((body, target_translation, target_verse_count)) = dense_section
        {
            if !target_translation.eq_ignore_ascii_case(&translation_id) {
                return Err(CoreError::InvalidOverlay(format!(
                    "LVEC pack targets {target_translation} but base pack is {translation_id}"
                )));
            }
            if target_verse_count != verses.len() {
                return Err(CoreError::InvalidOverlay(format!(
                    "LVEC META verse count {target_verse_count} does not match base pack {}",
                    verses.len()
                )));
            }
            Some(DenseLane::parse(body, verses.len()).map_err(CoreError::InvalidOverlay)?)
        } else {
            None
        };
        let passage_dense_lane =
            if let Some((body, target_translation, target_verse_count)) = passage_dense_section {
                if !target_translation.eq_ignore_ascii_case(&translation_id) {
                    return Err(CoreError::InvalidOverlay(format!(
                        "PVEC pack targets {target_translation} but base pack is {translation_id}"
                    )));
                }
                if target_verse_count != verses.len() {
                    return Err(CoreError::InvalidOverlay(format!(
                        "PVEC META verse count {target_verse_count} does not match base pack {}",
                        verses.len()
                    )));
                }
                Some(PassageDenseLane::parse(body, &verses).map_err(CoreError::InvalidOverlay)?)
            } else {
                None
            };
        if let Some(passage) = &passage_dense_lane {
            let dense = dense_lane.as_ref().ok_or_else(|| {
                CoreError::InvalidOverlay(
                    "PVEC requires a canonical LVEC section for independent support".into(),
                )
            })?;
            let dense_meta = dense.metadata();
            let passage_meta = passage.metadata();
            if dense_meta.dims != passage_meta.dims
                || dense_meta.embedding_space_id != Some(passage_meta.embedding_space_id)
            {
                return Err(CoreError::InvalidOverlay(
                    "PVEC dimensions and embedding-space ID must exactly match LVEC".into(),
                ));
            }
        }

        let mut lanes: Vec<Box<dyn Lane>> = Vec::new();
        let mut semantic_expander: Option<Arc<PackedExpandLane>> = None;
        match (lex_section, post_section) {
            (
                Some((lex, lex_translation, lex_count)),
                Some((post, post_translation, post_count)),
            ) => {
                if !lex_translation.eq_ignore_ascii_case(&translation_id)
                    || !post_translation.eq_ignore_ascii_case(&translation_id)
                    || lex_count != verses.len()
                    || post_count != verses.len()
                {
                    return Err(CoreError::InvalidOverlay(
                        "LEX/POST metadata does not match canonical TEXT".into(),
                    ));
                }
                let packed = Arc::new(PackedLexicalLane::new_owned_with_verses(
                    lex,
                    post,
                    Arc::clone(&verses),
                )?);
                lanes.push(Box::new(SharedPackedLexicalLane(Arc::clone(&packed))));
                if let Some((expand, expand_translation, expand_count)) = expand_section {
                    if !expand_translation.eq_ignore_ascii_case(&translation_id)
                        || expand_count != verses.len()
                    {
                        return Err(CoreError::InvalidOverlay(
                            "EXPAND metadata does not match canonical TEXT".into(),
                        ));
                    }
                    let lane = Arc::new(PackedExpandLane::new_owned(
                        expand,
                        packed,
                        &translation_id,
                        verses.len(),
                    )?);
                    lanes.push(Box::new(ContrastAwarePackedExpandLane(Arc::clone(&lane))));
                    semantic_expander = Some(lane);
                }
            }
            (None, None) => {
                if expand_section.is_some() {
                    return Err(CoreError::InvalidOverlay(
                        "EXPAND requires canonical LEX and POST sections".into(),
                    ));
                }
                // Legacy packs remain readable, at a higher open-time memory cost.
                lanes.push(Box::new(TextScanLane::new(Arc::clone(&verses))));
            }
            _ => {
                return Err(CoreError::InvalidOverlay(
                    "LEX and POST must be supplied together".into(),
                ));
            }
        }
        let context_lane = match (context_lex_section, context_post_section) {
            (
                Some((lex, lex_translation, lex_count)),
                Some((post, post_translation, post_count)),
            ) => {
                if !lex_translation.eq_ignore_ascii_case(&translation_id)
                    || !post_translation.eq_ignore_ascii_case(&translation_id)
                    || lex_count != verses.len()
                    || post_count != verses.len()
                {
                    return Err(CoreError::InvalidOverlay(
                        "CLEX/CPOST metadata does not match canonical TEXT".into(),
                    ));
                }
                Some(ContextLexicalLane::new_owned(lex, post, verses.len())?)
            }
            (None, None) => None,
            _ => {
                return Err(CoreError::InvalidOverlay(
                    "CLEX and CPOST must be supplied together".into(),
                ));
            }
        };
        if !overlay_sections.is_empty() {
            let overlay_bodies: Vec<&[u8]> = overlay_sections
                .into_iter()
                .map(|(body, _, _)| body)
                .collect();
            let overlay = RecallOverlay::parse(&overlay_bodies, &translation_id, verses.len())
                .map_err(CoreError::InvalidOverlay)?;
            lanes.extend(
                overlay
                    .into_lanes_with_expansion(&verses, semantic_expander)
                    .map_err(CoreError::InvalidOverlay)?,
            );
        }

        Ok(Self {
            verses,
            translation_id,
            weights,
            lanes,
            context_lane,
            dense_lane,
            passage_dense_lane,
        })
    }

    pub fn translation_id(&self) -> &str {
        &self.translation_id
    }

    pub fn verse_count(&self) -> usize {
        self.verses.len()
    }

    pub fn available_lanes(&self) -> Vec<LaneId> {
        [LaneId::A, LaneId::B, LaneId::C, LaneId::D]
            .into_iter()
            .filter(|id| {
                (*id == LaneId::C && self.dense_lane.is_some())
                    || (*id == LaneId::C && self.context_lane.is_some())
                    || self
                        .lanes
                        .iter()
                        .any(|lane| lane.advertised() && lane.id() == *id)
            })
            .collect()
    }

    pub fn dense_metadata(&self) -> Option<DenseMetadata> {
        self.dense_lane.as_ref().map(DenseLane::metadata)
    }

    pub fn passage_dense_metadata(&self) -> Option<PassageDenseMetadata> {
        self.passage_dense_lane
            .as_ref()
            .map(PassageDenseLane::metadata)
    }

    pub fn context_available(&self) -> bool {
        self.context_lane.is_some()
    }

    /// Diagnostic ablation over source-authored context only.
    ///
    /// This deliberately excludes canonical lexical, expansion, overlay, and
    /// dense providers. Returned text still comes from canonical TEXT, and
    /// context evidence remains `Related` with no direct resolver tier.
    pub fn search_context_only(&self, query: &str, opts: &SearchOpts) -> SearchResponse {
        let opts = bounded_search_opts(opts);
        let nq = normalize_query(query);
        let lane_lists = self
            .context_lane
            .as_ref()
            .map(|lane| vec![(LaneId::C, lane.search(&nq, LANE_CANDIDATE_DEPTH))])
            .unwrap_or_default();
        SearchResponse {
            query: nq.raw,
            results: fuse_lanes(&lane_lists, &self.verses, &opts, self.weights),
        }
    }

    /// Run the full query pipeline.
    pub fn search(&self, query: &str, opts: &SearchOpts) -> SearchResponse {
        self.search_internal(query, None, opts, true)
            .expect("search without a query embedding cannot fail")
            .response
    }

    /// Search with a host-computed embedding for the optional LVEC Lane C.
    /// The host may use native ONNX, Transformers.js, or another offline model;
    /// the core remains deterministic and model-runtime agnostic.
    pub fn search_with_embedding(
        &self,
        query: &str,
        embedding: &[f32],
        opts: &SearchOpts,
    ) -> CoreResult<SearchResponse> {
        self.search_internal(query, Some((embedding, None)), opts, true)
            .map(|internal| internal.response)
    }

    pub fn search_with_embedding_in_space(
        &self,
        query: &str,
        embedding_space_id: &[u8; 32],
        embedding: &[f32],
        opts: &SearchOpts,
    ) -> CoreResult<SearchResponse> {
        self.search_internal(
            query,
            Some((embedding, Some(embedding_space_id))),
            opts,
            true,
        )
        .map(|internal| internal.response)
    }

    /// Search a compound memory using one whole-query embedding and up to four
    /// host-planned clause embeddings from the same validated embedding space.
    ///
    /// The ordinary whole-query pipeline is run unchanged. Each segment then
    /// runs through the same lexical, expansion, overlay, and dense retrieval
    /// pipeline independently. A deterministic rank-only diversification pass
    /// gives every intent an opportunity to contribute without comparing raw
    /// scores from different queries. Strong direct whole-query recoveries keep
    /// their head position; segment-derived candidates never acquire a
    /// `Recovered` confidence claim through composition.
    pub fn search_with_segment_embeddings_in_space(
        &self,
        query: &str,
        embedding_space_id: &[u8; 32],
        whole_embedding: &[f32],
        segments: &[QuerySegmentEmbedding<'_>],
        opts: &SearchOpts,
    ) -> CoreResult<SearchResponse> {
        let opts = bounded_search_opts(opts);
        let dense = self.dense_lane.as_ref().ok_or_else(|| {
            CoreError::InvalidQueryEmbedding(
                "search requested segment embeddings but no LVEC pack is loaded".into(),
            )
        })?;
        dense
            .validate_query_space(embedding_space_id)
            .map_err(CoreError::InvalidQueryEmbedding)?;
        dense
            .validate_embedding(whole_embedding)
            .map_err(CoreError::InvalidQueryEmbedding)?;

        if segments.is_empty() {
            return Err(CoreError::InvalidQueryEmbedding(
                "segment embedding search requires at least one segment".into(),
            ));
        }
        if segments.len() > MAX_QUERY_SEGMENTS {
            return Err(CoreError::InvalidQueryEmbedding(format!(
                "segment embedding search accepts at most {MAX_QUERY_SEGMENTS} segments"
            )));
        }

        let composition_plan = plan_query_composition(query);
        if composition_plan.segments.len() != segments.len()
            || composition_plan
                .segments
                .iter()
                .map(String::as_str)
                .ne(segments.iter().map(|segment| segment.text))
        {
            return Err(CoreError::InvalidQueryEmbedding(
                "query segment texts must exactly match the canonical ordered query-only planner output"
                    .into(),
            ));
        }

        // Retain the planner's source order. Besides making every host enforce
        // one contract, this prevents alphabetical clause text from becoming
        // an accidental ranking tie-break.
        let mut canonical_segments = Vec::with_capacity(segments.len());
        for segment in segments {
            dense
                .validate_embedding(segment.embedding)
                .map_err(CoreError::InvalidQueryEmbedding)?;
            canonical_segments.push((segment.text, segment.embedding));
        }

        let candidate_opts = opts.clone().with_top_k(LANE_CANDIDATE_DEPTH);
        let whole = self.search_internal(
            query,
            Some((whole_embedding, Some(embedding_space_id))),
            &candidate_opts,
            false,
        )?;
        let intent_groups =
            build_query_intent_groups(composition_plan.segments.len(), &composition_plan.relations);
        // Alternative, subordinate, and contrastive views do not by
        // themselves make a query multi-intent. Preserve the ordinary
        // whole-query order unless at least two hard Strong/Additive groups
        // exist; segment vectors are still validated above so hosts share one
        // fail-closed contract.
        if intent_groups.len() < 2 {
            let mut results = whole.response.results;
            results.truncate(opts.top_k);
            return Ok(SearchResponse {
                query: normalize_query(query).raw,
                results,
            });
        }
        let mut segment_results = Vec::with_capacity(canonical_segments.len());
        let mut segment_decisive = Vec::with_capacity(canonical_segments.len());
        for (text, embedding) in canonical_segments {
            let response = self.search_internal(
                text,
                Some((embedding, Some(embedding_space_id))),
                &candidate_opts,
                false,
            )?;
            segment_decisive.push(response.head_is_decisive);
            segment_results.push((text.to_string(), response.response.results));
        }

        // Hard groups introduce when they have ranked candidates with either an
        // authoritative/decisive head or a unique ordinal head. Tied/flat
        // views stay inert; non-decisive meaning-only clauses with a unique
        // nearest neighbor still get a composition slot.
        let active_group_count = intent_groups
            .iter()
            .filter(|group| {
                group.introducers.iter().any(|index| {
                    let Some((_, results)) = segment_results.get(*index) else {
                        return false;
                    };
                    segment_introduces_intent(
                        results,
                        segment_decisive.get(*index).copied().unwrap_or(false),
                    )
                })
            })
            .count();
        if active_group_count < 2 {
            let mut results = whole.response.results;
            results.truncate(opts.top_k);
            return Ok(SearchResponse {
                query: normalize_query(query).raw,
                results,
            });
        }

        // Active-group policy is reapplied inside diversify from the same
        // introduce rule; diversify uses query-local ranks only.
        let diversified = diversify_segment_results(
            &whole.response.results,
            &segment_results,
            &segment_decisive,
            &composition_plan.relations,
            whole.protect_head,
            LANE_CANDIDATE_DEPTH,
        );
        let mut results = stitch_passage_results(diversified, &self.verses);
        results.truncate(opts.top_k.max(1));
        Ok(SearchResponse {
            query: normalize_query(query).raw,
            results,
        })
    }

    fn search_internal(
        &self,
        query: &str,
        embedding: Option<(&[f32], Option<&[u8; 32]>)>,
        opts: &SearchOpts,
        allow_lexical_multi_intent: bool,
    ) -> CoreResult<InternalSearchResponse> {
        let bounded_opts = bounded_search_opts(opts);
        let opts = &bounded_opts;
        let nq = normalize_query(query);
        let mut lane_lists = Vec::new();
        let mut blocks_unique_recovery = false;

        for lane in &self.lanes {
            if !lane.available() {
                continue;
            }
            let output = lane.search_with_risk(&nq, LANE_CANDIDATE_DEPTH);
            blocks_unique_recovery |= output.blocks_unique_recovery;
            lane_lists.push((lane.id(), output.hits));
        }
        let context_hits = self
            .context_lane
            .as_ref()
            .map(|lane| lane.search(&nq, LANE_CANDIDATE_DEPTH))
            .unwrap_or_default();
        // Source-authored Study Notes are an auxiliary representation, not a
        // second independently authoritative retrieval vote.  Keep them out
        // of ordinary lane fusion: otherwise merely installing the optional
        // context pack can reorder exact quotes, events, and hidden
        // translations.  They may corroborate an LVEC candidate below, and
        // remain directly measurable through `search_context_only`.
        let has_multi_intent_resolution = allow_lexical_multi_intent
            && self.multi_intent_hits(&nq, opts).is_some_and(|hits| {
                lane_lists.push((LaneId::D, hits));
                true
            });
        let direct_head_candidates: HashSet<u32> = lane_lists
            .iter()
            .filter(|(lane_id, _)| *lane_id != LaneId::C)
            .flat_map(|(_, hits)| hits)
            .filter(|hit| hit.score > 0.0 && hit.ranking_tier >= 2)
            .map(|hit| hit.verse_index)
            .collect();
        let has_ranked_direct_evidence = lane_lists.iter().any(|(_, hits)| {
            hits.iter()
                .any(|hit| hit.score > 0.0 && hit.ranking_tier >= 2)
        });
        // Lane C includes both dense retrieval and heuristic topic bridges.
        // Neither is independent lexical evidence, and a broad topic edge
        // carrying a passage must not switch the whole query into
        // lexical-head preservation. Only the literal/translation/resolver
        // lanes participate in this probe.
        let lexical_direct_lists: Vec<_> = lane_lists
            .iter()
            .filter(|(lane_id, _)| *lane_id != LaneId::C)
            .cloned()
            .collect();
        let lexical_probe_opts = opts.clone().with_top_k(1);
        let has_corroborated_lexical_head = fuse_lanes(
            &lexical_direct_lists,
            &self.verses,
            &lexical_probe_opts,
            self.weights,
        )
        .first()
        .is_some_and(|result| {
            matches!(
                result.confidence,
                crate::result::Confidence::Recovered
                    | crate::result::Confidence::OftenMisremembered
            ) && (result.lanes.agreeing_lanes() >= 2
                || result.lanes.d > 0.0
                || !result.passages.is_empty())
        });
        let has_direct_resolution = has_ranked_direct_evidence || has_corroborated_lexical_head;
        // Preserve the non-dense providers as their own retrieval view. This
        // includes independently supported exact topic names, while inferred
        // `semantic-topic:*` hits are demoted to shadow evidence by fusion
        // unless a true dense provider confirms them. Appending LVEC hits to
        // this list would let one representation vote twice.
        let lexical_lane_lists = lane_lists.clone();
        let mut dense_lane_lists = Vec::new();
        if let Some((query_embedding, embedding_space_id)) = embedding {
            let dense = self.dense_lane.as_ref().ok_or_else(|| {
                CoreError::InvalidQueryEmbedding(
                    "search requested an embedding but no LVEC pack is loaded".into(),
                )
            })?;
            dense
                .validate_embedding(query_embedding)
                .map_err(CoreError::InvalidQueryEmbedding)?;
            match (dense.embedding_space_id(), embedding_space_id) {
                (Some(_), Some(space_id)) => dense
                    .validate_query_space(space_id)
                    .map_err(CoreError::InvalidQueryEmbedding)?,
                (Some(_), None) => {
                    return Err(CoreError::InvalidQueryEmbedding(
                        "LVEC v2 requires an embedding-space ID".into(),
                    ));
                }
                (None, Some(_)) => {
                    return Err(CoreError::InvalidQueryEmbedding(
                        "LVEC v1 cannot validate an embedding-space ID".into(),
                    ));
                }
                (None, None) => {}
            }
            // Even a strong lexical recovery keeps a bounded dense tail. The
            // merge below protects its lexical head while allowing a genuine
            // semantic alternative to enter the lower top-ten positions.
            let mut hits = match embedding_space_id {
                Some(space_id) => {
                    dense.search_in_space(space_id, query_embedding, LANE_CANDIDATE_DEPTH)
                }
                None => dense.search(query_embedding, LANE_CANDIDATE_DEPTH),
            }
            .map_err(CoreError::InvalidQueryEmbedding)?;
            if let Some(passage_dense) = &self.passage_dense_lane {
                let space_id = embedding_space_id.ok_or_else(|| {
                    CoreError::InvalidQueryEmbedding(
                        "PVEC requires a validated embedding-space ID".into(),
                    )
                })?;
                let passage_hits = passage_dense
                    .search_in_space(space_id, query_embedding, LANE_CANDIDATE_DEPTH)
                    .map_err(CoreError::InvalidQueryEmbedding)?;
                hits = merge_supported_passage_dense(hits, passage_hits, &self.verses);
            }
            attach_dense_window_passages(&mut hits, &self.verses);
            promote_dense_context_agreement(&mut hits, &context_hits, &nq);
            // Keep the actual LVEC candidates separate from other providers
            // that share the conceptual Lane-C identifier (notably the Selah
            // topic graph). Semantic-mode ordering must be encoder output,
            // not a second fusion of encoder and heuristic topic ranks.
            dense_lane_lists.push((LaneId::C, hits));
        }
        let preserve_lexical_head = has_direct_resolution || has_multi_intent_resolution;
        let effective_weights = match (embedding.is_some(), preserve_lexical_head) {
            (true, false) => FuseWeights {
                // A compatible query embedding owns semantic-mode ordering.
                // Surface overlap remains available through the bounded
                // lexical reserve below instead of being allowed to drown out
                // meaning-only candidates in a single blended score.
                a: 0.0,
                b: 0.0,
                c: 10.0,
                d: 0.0,
                agreement_bonus: 0.0,
                ..self.weights
            },
            // Dense remains available as weak corroboration but cannot reorder
            // a strong lexical/phrase recovery.
            (true, true) => FuseWeights {
                c: 0.0,
                ..self.weights
            },
            (false, _) => self.weights,
        };
        // Resolver tiers and confidence hints from lexical lanes are useful in
        // the lexical ordering, but they must not leak into the supposedly
        // dense ordering merely because their numeric weights are zero. Build
        // the semantic list from Lane C alone, then interleave/merge lexical
        // evidence explicitly below.
        let (mut results, decisive_heads) = if embedding.is_some() && !preserve_lexical_head {
            // A single vector is good at intent but can collapse a multi-clause
            // memory into one facet. Keep a bounded semantic head, then expose
            // independent lexical/translation candidates before filling the
            // tail. This is a query-mode policy, not a prompt-specific rule.
            let candidate_opts = opts.clone().with_top_k(LANE_CANDIDATE_DEPTH);
            let semantic = fuse_lanes(
                &dense_lane_lists,
                &self.verses,
                &candidate_opts,
                effective_weights,
            );
            let lexical = fuse_lanes(
                &lexical_lane_lists,
                &self.verses,
                &candidate_opts,
                self.weights,
            );
            let decisive = [
                unique_ordinal_head(&semantic),
                unique_ordinal_head(&lexical),
            ];
            let merged = merge_semantic_and_lexical(semantic, lexical, LANE_CANDIDATE_DEPTH);
            let mut stitched = stitch_passage_results(merged, &self.verses);
            stitched.truncate(opts.top_k.max(1));
            (stitched, decisive)
        } else if embedding.is_some() {
            let candidate_opts = opts.clone().with_top_k(LANE_CANDIDATE_DEPTH);
            let lexical = fuse_lanes(
                &lexical_lane_lists,
                &self.verses,
                &candidate_opts,
                self.weights,
            );
            let semantic = fuse_lanes(
                &dense_lane_lists,
                &self.verses,
                &candidate_opts,
                FuseWeights {
                    a: 0.0,
                    b: 0.0,
                    c: 10.0,
                    d: 0.0,
                    agreement_bonus: 0.0,
                    ..self.weights
                },
            );
            let decisive = [
                unique_ordinal_head(&lexical),
                unique_ordinal_head(&semantic),
            ];
            let merged =
                merge_lexical_head_and_semantic_tail(lexical, semantic, LANE_CANDIDATE_DEPTH);
            let mut stitched = stitch_passage_results(merged, &self.verses);
            stitched.truncate(opts.top_k.max(1));
            (stitched, decisive)
        } else {
            let results = fuse_lanes(&lane_lists, &self.verses, opts, effective_weights);
            let decisive = [unique_ordinal_head(&results), None];
            (results, decisive)
        };

        // Confidence is a presentation claim, not a score echo. Only the lead
        // candidate may claim `Recovered`; alternatives remain useful but are
        // explicitly `Related` unless they carry the stronger misquote label.
        for result in results.iter_mut().skip(1) {
            if matches!(result.confidence, crate::result::Confidence::Recovered) {
                result.confidence = crate::result::Confidence::Related;
            }
        }

        // Exact wording in the canonical text is not necessarily a unique
        // remembered verse. If a complete hidden translation uses the same
        // phrase recurrently, retain the ranking but lower the claim.
        if blocks_unique_recovery {
            for result in &mut results {
                if matches!(result.confidence, crate::result::Confidence::Recovered) {
                    result.confidence = crate::result::Confidence::Related;
                }
            }
        }

        // A recognized misquote is an explicit ambiguity signal. Competing
        // lexical candidates may still be useful, but they must not claim an
        // exact recovery merely because they share the misremembered words.
        if results.iter().any(|result| {
            matches!(
                result.confidence,
                crate::result::Confidence::OftenMisremembered
            )
        }) {
            for result in &mut results {
                if matches!(result.confidence, crate::result::Confidence::Recovered) {
                    result.confidence = crate::result::Confidence::Related;
                }
            }
        }

        let protect_head = results.first().is_some_and(|result| {
            direct_head_candidates.contains(&result.verse_index)
                || matches!(
                    result.confidence,
                    crate::result::Confidence::Recovered
                        | crate::result::Confidence::OftenMisremembered
                )
        });
        let head_is_decisive = results.first().is_some_and(|result| {
            protect_head
                || decisive_heads
                    .iter()
                    .flatten()
                    .any(|verse_index| *verse_index == result.verse_index)
                || matches!(
                    result.confidence,
                    crate::result::Confidence::OftenMisremembered
                )
        });
        Ok(InternalSearchResponse {
            response: SearchResponse {
                query: nq.raw,
                results,
            },
            protect_head,
            head_is_decisive,
        })
    }

    /// Resolve independently meaningful clauses before whole-query fusion.
    /// This uses the same ordered planner as embedded composition and remains
    /// evidence-gated: only Strong/Additive groups create obligations,
    /// alternatives compete within one group, subordinate/contrastive views do
    /// not introduce results, and duplicate answers collapse instead of
    /// manufacturing another intent.
    fn multi_intent_hits(
        &self,
        query: &crate::normalize::NormalizedQuery,
        opts: &SearchOpts,
    ) -> Option<Vec<crate::lanes::LaneHit>> {
        let plan = plan_query_composition(&query.raw);
        let groups = build_query_intent_groups(plan.segments.len(), &plan.relations);
        if groups.len() < 2 {
            return None;
        }
        let segment_opts = opts.clone().with_top_k(3);
        let mut resolved = Vec::with_capacity(groups.len());
        let mut seen = HashSet::new();
        for group in groups {
            let mut representative = None;
            // Main and Alternative views compete for one group result. Source
            // order is the deterministic tie-break; refinement views are not
            // permitted to introduce a result in this lexical-only fallback.
            for segment_index in group.introducers {
                let text = plan.segments.get(segment_index)?;
                let segment = normalize_query(text);
                let mut lists = Vec::new();
                for lane in &self.lanes {
                    if lane.available() {
                        lists.push((lane.id(), lane.search(&segment, LANE_CANDIDATE_DEPTH)));
                    }
                }
                let Some(result) = fuse_lanes(&lists, &self.verses, &segment_opts, self.weights)
                    .into_iter()
                    .next()
                else {
                    continue;
                };
                let corroborated = result.lanes.agreeing_lanes() >= 2
                    || !result.passages.is_empty()
                    || matches!(
                        result.confidence,
                        crate::result::Confidence::Recovered
                            | crate::result::Confidence::OftenMisremembered
                    );
                if corroborated {
                    representative = Some((segment, result));
                    break;
                }
            }
            let (segment, result) = representative?;
            if !seen.insert(result.verse_index) {
                return None;
            }
            resolved.push((segment, result));
        }
        if resolved.len() < 2 {
            return None;
        }
        Some(
            resolved
                .into_iter()
                .map(|(segment, result)| crate::lanes::LaneHit {
                    verse_index: result.verse_index,
                    score: 1.0,
                    matched_terms: segment.lemmas,
                    expanded_terms: vec![format!("multi-intent-clause:{}", segment.raw)],
                    passages: result.passages,
                    confidence_hint: crate::result::Confidence::Related,
                    ranking_tier: 1,
                    direct_tier: 0,
                })
                .collect(),
        )
    }

    /// JSON ABI helper for FFI/WASM.
    pub fn search_json(&self, query: &str, opts: &SearchOpts) -> CoreResult<String> {
        let resp = self.search(query, opts);
        serde_json::to_string(&resp).map_err(|_| CoreError::Internal("json serialize"))
    }

    pub fn search_json_with_embedding(
        &self,
        query: &str,
        embedding: &[f32],
        opts: &SearchOpts,
    ) -> CoreResult<String> {
        let resp = self.search_with_embedding(query, embedding, opts)?;
        serde_json::to_string(&resp).map_err(|_| CoreError::Internal("json serialize"))
    }

    pub fn search_json_with_embedding_in_space(
        &self,
        query: &str,
        embedding_space_id: &[u8; 32],
        embedding: &[f32],
        opts: &SearchOpts,
    ) -> CoreResult<String> {
        let resp =
            self.search_with_embedding_in_space(query, embedding_space_id, embedding, opts)?;
        serde_json::to_string(&resp).map_err(|_| CoreError::Internal("json serialize"))
    }

    /// JSON ABI helper for compound semantic search. This is additive to the
    /// existing single-vector entry points and uses the same validated segment
    /// search path as native Rust callers.
    pub fn search_json_with_segment_embeddings_in_space(
        &self,
        query: &str,
        embedding_space_id: &[u8; 32],
        whole_embedding: &[f32],
        segments: &[QuerySegmentEmbedding<'_>],
        opts: &SearchOpts,
    ) -> CoreResult<String> {
        let resp = self.search_with_segment_embeddings_in_space(
            query,
            embedding_space_id,
            whole_embedding,
            segments,
            opts,
        )?;
        serde_json::to_string(&resp).map_err(|_| CoreError::Internal("json serialize"))
    }
}

/// A hard query group is introduced only by Strong/Additive syntax.
/// Alternative segments are competing primary views of that same group;
/// Subordinate and Contrast segments are refinements and cannot introduce an
/// answer or demand an output slot.
#[derive(Clone, Debug, Eq, PartialEq)]
struct QueryIntentGroup {
    introducers: Vec<usize>,
    refiners: Vec<usize>,
}

fn build_query_intent_groups(
    segment_count: usize,
    relations: &[BoundaryRelation],
) -> Vec<QueryIntentGroup> {
    if segment_count == 0 {
        return Vec::new();
    }
    let mut groups = vec![QueryIntentGroup {
        introducers: vec![0],
        refiners: Vec::new(),
    }];
    for next in 1..segment_count {
        let relation = relations
            .get(next - 1)
            .copied()
            .unwrap_or(BoundaryRelation::Strong);
        match relation {
            BoundaryRelation::Strong | BoundaryRelation::Additive => {
                groups.push(QueryIntentGroup {
                    introducers: vec![next],
                    refiners: Vec::new(),
                });
            }
            BoundaryRelation::Alternative => groups
                .last_mut()
                .expect("first intent group exists")
                .introducers
                .push(next),
            BoundaryRelation::Subordinate | BoundaryRelation::Contrast => groups
                .last_mut()
                .expect("first intent group exists")
                .refiners
                .push(next),
        }
    }
    groups
}

/// Return the head only when its query-local rank is not tied. Exact equality
/// is intentional: this converts a provider's ordering into ordinal evidence
/// without a tuned magnitude threshold or a cross-query score comparison.
fn unique_ordinal_head(results: &[crate::result::SearchResult]) -> Option<u32> {
    let head = results.first()?;
    if results
        .get(1)
        .is_some_and(|next| next.score.total_cmp(&head.score).is_eq())
    {
        None
    } else {
        Some(head.verse_index)
    }
}

const REPRESENTATIVE_DEPTH: usize = 8;
const COMPOSITION_RRF_K: f64 = 60.0;

#[derive(Clone, Debug)]
struct AnswerClusters {
    cluster_of: HashMap<u32, u32>,
    members: BTreeMap<u32, Vec<u32>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CanonicalPoint {
    book: String,
    chapter: u32,
    verse: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CanonicalInterval {
    book: String,
    start: (u32, u32),
    end: (u32, u32),
}

fn canonical_point(reference: &str) -> Option<CanonicalPoint> {
    let (book, chapter_and_verse) = reference.rsplit_once(' ')?;
    let (chapter, verse) = chapter_and_verse.split_once(':')?;
    Some(CanonicalPoint {
        book: book.to_string(),
        chapter: chapter.parse().ok()?,
        verse: verse.parse().ok()?,
    })
}

fn canonical_interval(range: &crate::result::PassageRange) -> Option<CanonicalInterval> {
    let start = canonical_point(&range.start_ref)?;
    let end = canonical_point(&range.end_ref)?;
    if start.book != end.book || (start.chapter, start.verse) > (end.chapter, end.verse) {
        return None;
    }
    Some(CanonicalInterval {
        book: start.book,
        start: (start.chapter, start.verse),
        end: (end.chapter, end.verse),
    })
}

fn interval_contains(interval: &CanonicalInterval, point: &CanonicalPoint) -> bool {
    interval.book == point.book
        && interval.start <= (point.chapter, point.verse)
        && (point.chapter, point.verse) <= interval.end
}

fn intervals_overlap(left: &CanonicalInterval, right: &CanonicalInterval) -> bool {
    left.book == right.book && left.start <= right.end && right.start <= left.end
}

fn structured_answers_overlap(
    left: &crate::result::SearchResult,
    right: &crate::result::SearchResult,
) -> bool {
    if left.verse_index == right.verse_index {
        return true;
    }
    let left_anchor = canonical_point(&left.reference);
    let right_anchor = canonical_point(&right.reference);
    let left_ranges: Vec<_> = left
        .passages
        .iter()
        .filter_map(canonical_interval)
        .collect();
    let right_ranges: Vec<_> = right
        .passages
        .iter()
        .filter_map(canonical_interval)
        .collect();
    left_ranges.iter().any(|left_range| {
        right_ranges
            .iter()
            .any(|right_range| intervals_overlap(left_range, right_range))
            || right_anchor
                .as_ref()
                .is_some_and(|anchor| interval_contains(left_range, anchor))
    }) || right_ranges.iter().any(|right_range| {
        left_anchor
            .as_ref()
            .is_some_and(|anchor| interval_contains(right_range, anchor))
    })
}

fn union_find_root(parents: &mut [usize], index: usize) -> usize {
    if parents[index] != index {
        parents[index] = union_find_root(parents, parents[index]);
    }
    parents[index]
}

fn build_answer_clusters(
    candidates: &BTreeMap<u32, crate::result::SearchResult>,
) -> AnswerClusters {
    let verse_indices: Vec<u32> = candidates.keys().copied().collect();
    let mut parents: Vec<usize> = (0..verse_indices.len()).collect();
    for left in 0..verse_indices.len() {
        for right in left + 1..verse_indices.len() {
            let left_result = &candidates[&verse_indices[left]];
            let right_result = &candidates[&verse_indices[right]];
            if structured_answers_overlap(left_result, right_result) {
                let left_root = union_find_root(&mut parents, left);
                let right_root = union_find_root(&mut parents, right);
                if left_root != right_root {
                    parents[right_root] = left_root;
                }
            }
        }
    }
    let mut by_root: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
    for (position, verse_index) in verse_indices.into_iter().enumerate() {
        let root = union_find_root(&mut parents, position);
        by_root.entry(root).or_default().push(verse_index);
    }
    let mut cluster_of = HashMap::new();
    let mut members = BTreeMap::new();
    for mut component in by_root.into_values() {
        component.sort_unstable();
        let cluster = component[0];
        for verse_index in &component {
            cluster_of.insert(*verse_index, cluster);
        }
        members.insert(cluster, component);
    }
    AnswerClusters {
        cluster_of,
        members,
    }
}

fn cluster_rank(ranks: &HashMap<u32, usize>, members: &[u32]) -> Option<usize> {
    members
        .iter()
        .filter_map(|verse_index| ranks.get(verse_index).copied())
        .min()
}

/// Whether a clause search may introduce a composition obligation.
///
/// Decisive heads (protected recovery or unique provider ordinal) always
/// introduce. Non-decisive clauses still introduce when they return a ranked
/// list with a unique ordinal head so meaning-only multi-intent is not
/// collapsed solely for lack of lexical authority. Empty lists and tied/flat
/// heads remain inert.
fn segment_introduces_intent(
    results: &[crate::result::SearchResult],
    head_is_decisive: bool,
) -> bool {
    if results.is_empty() {
        return false;
    }
    head_is_decisive || unique_ordinal_head(results).is_some()
}

fn group_primary_rank(
    group: &QueryIntentGroup,
    members: &[u32],
    segment_ranks: &[HashMap<u32, usize>],
    segment_introduces: &[bool],
) -> Option<usize> {
    group
        .introducers
        .iter()
        .filter(|index| segment_introduces.get(**index).copied().unwrap_or(false))
        .filter_map(|index| segment_ranks.get(*index))
        .filter_map(|ranks| cluster_rank(ranks, members))
        .min()
}

fn group_refinement_rank(
    group: &QueryIntentGroup,
    members: &[u32],
    segment_ranks: &[HashMap<u32, usize>],
    segment_introduces: &[bool],
) -> Option<usize> {
    group
        .refiners
        .iter()
        .filter(|index| segment_introduces.get(**index).copied().unwrap_or(false))
        .filter_map(|index| segment_ranks.get(*index))
        .filter_map(|ranks| cluster_rank(ranks, members))
        .min()
}

#[derive(Clone, Copy, Debug)]
struct RepresentativeOption {
    cluster: u32,
    support_degree: usize,
    primary_rank: usize,
}

#[derive(Clone, Debug)]
struct RepresentativeMatching {
    assignments: Vec<Option<u32>>,
    assigned: usize,
    degree_sum: usize,
    rank_sum: usize,
}

impl RepresentativeMatching {
    fn is_better_than(&self, other: &Self) -> bool {
        self.assigned > other.assigned
            || (self.assigned == other.assigned
                && (self.degree_sum < other.degree_sum
                    || (self.degree_sum == other.degree_sum
                        && (self.rank_sum < other.rank_sum
                            || (self.rank_sum == other.rank_sum
                                && self.assignment_key() < other.assignment_key())))))
    }

    fn assignment_key(&self) -> Vec<u32> {
        self.assignments
            .iter()
            .map(|assignment| assignment.unwrap_or(u32::MAX))
            .collect()
    }
}

fn visit_representative_matchings(
    options: &[Vec<RepresentativeOption>],
    group_position: usize,
    used: &mut HashSet<u32>,
    assignments: &mut Vec<Option<u32>>,
    degree_sum: usize,
    rank_sum: usize,
    best: &mut Option<RepresentativeMatching>,
) {
    if group_position == options.len() {
        let candidate = RepresentativeMatching {
            assignments: assignments.clone(),
            assigned: assignments.iter().flatten().count(),
            degree_sum,
            rank_sum,
        };
        if best
            .as_ref()
            .is_none_or(|current| candidate.is_better_than(current))
        {
            *best = Some(candidate);
        }
        return;
    }

    assignments.push(None);
    visit_representative_matchings(
        options,
        group_position + 1,
        used,
        assignments,
        degree_sum,
        rank_sum,
        best,
    );
    assignments.pop();

    for option in &options[group_position] {
        if !used.insert(option.cluster) {
            continue;
        }
        assignments.push(Some(option.cluster));
        visit_representative_matchings(
            options,
            group_position + 1,
            used,
            assignments,
            degree_sum + option.support_degree,
            rank_sum + option.primary_rank,
            best,
        );
        assignments.pop();
        used.remove(&option.cluster);
    }
}

fn retain_one_structured_range(result: &mut crate::result::SearchResult) {
    if result.passages.len() > 1 {
        result.passages.sort_by(|left, right| {
            let left_interval = canonical_interval(left);
            let right_interval = canonical_interval(right);
            left_interval
                .as_ref()
                .map(|interval| (&interval.book, interval.start, interval.end))
                .cmp(
                    &right_interval
                        .as_ref()
                        .map(|interval| (&interval.book, interval.start, interval.end)),
                )
        });
        result.passages.truncate(1);
    }
}

fn mark_composed_result(result: &mut crate::result::SearchResult, from_segment: bool) {
    if from_segment
        && !result
            .expanded_terms
            .iter()
            .any(|term| term == "multi-intent-segment")
    {
        result.expanded_terms.push("multi-intent-segment".into());
    }
    retain_one_structured_range(result);
    if !matches!(
        result.confidence,
        crate::result::Confidence::OftenMisremembered
    ) {
        result.confidence = crate::result::Confidence::Related;
    }
}

/// Compose query-local ordinal rankings with a cardinal coverage phase.
///
/// Every active Strong/Additive group receives a distinct representative
/// before any whole-only fallback when such a matching exists. Shared hubs are
/// retained, but their support degree makes group-specific candidates the
/// preferred matching. No raw similarity magnitude is compared, and the full
/// ranking is completed before `top_k` truncation for exact prefix stability.
fn diversify_segment_results(
    whole: &[crate::result::SearchResult],
    segment_results: &[(String, Vec<crate::result::SearchResult>)],
    segment_decisive: &[bool],
    relations: &[BoundaryRelation],
    protect_whole_head: bool,
    top_k: usize,
) -> Vec<crate::result::SearchResult> {
    let output_limit = top_k.clamp(1, MAX_TOP_K);
    let groups = build_query_intent_groups(segment_results.len(), relations);
    let whole_ranks: HashMap<u32, usize> = whole
        .iter()
        .enumerate()
        .map(|(rank, result)| (result.verse_index, rank))
        .collect();
    let segment_ranks: Vec<HashMap<u32, usize>> = segment_results
        .iter()
        .map(|(_, results)| {
            results
                .iter()
                .enumerate()
                .map(|(rank, result)| (result.verse_index, rank))
                .collect()
        })
        .collect();
    let segment_introduces: Vec<bool> = segment_results
        .iter()
        .enumerate()
        .map(|(index, (_, results))| {
            segment_introduces_intent(
                results,
                segment_decisive.get(index).copied().unwrap_or(false),
            )
        })
        .collect();

    let mut candidates = BTreeMap::new();
    for result in whole {
        merge_result_evidence(&mut candidates, result);
    }
    for (_, results) in segment_results {
        for result in results {
            merge_result_evidence(&mut candidates, result);
        }
    }
    if candidates.is_empty() {
        return Vec::new();
    }
    let clusters = build_answer_clusters(&candidates);
    let active_groups: Vec<usize> = groups
        .iter()
        .enumerate()
        .filter(|(_, group)| {
            group
                .introducers
                .iter()
                .any(|index| segment_introduces.get(*index).copied().unwrap_or(false))
        })
        .map(|(index, _)| index)
        .collect();
    if active_groups.len() < 2 {
        let mut unchanged = whole.to_vec();
        unchanged.truncate(output_limit);
        return unchanged;
    }

    let protected_verse = protect_whole_head
        .then(|| whole.first().map(|result| result.verse_index))
        .flatten();
    let protected_cluster = protected_verse.and_then(|verse| clusters.cluster_of.get(&verse));

    let support_degree = |cluster: u32| {
        let members = &clusters.members[&cluster];
        active_groups
            .iter()
            .filter(|group_index| {
                group_primary_rank(
                    &groups[**group_index],
                    members,
                    &segment_ranks,
                    &segment_introduces,
                )
                .is_some_and(|rank| rank == 0)
            })
            .count()
            .max(1)
    };
    let mut representative_options = Vec::with_capacity(active_groups.len());
    for group_index in &active_groups {
        let mut options = Vec::new();
        for (&cluster, members) in &clusters.members {
            if protected_cluster == Some(&cluster) {
                continue;
            }
            if let Some(primary_rank) = group_primary_rank(
                &groups[*group_index],
                members,
                &segment_ranks,
                &segment_introduces,
            )
            .filter(|rank| *rank < REPRESENTATIVE_DEPTH)
            {
                options.push(RepresentativeOption {
                    cluster,
                    support_degree: support_degree(cluster),
                    primary_rank,
                });
            }
        }
        options.sort_by_key(|option| (option.support_degree, option.primary_rank, option.cluster));
        representative_options.push(options);
    }
    let mut best_matching = None;
    visit_representative_matchings(
        &representative_options,
        0,
        &mut HashSet::new(),
        &mut Vec::new(),
        0,
        0,
        &mut best_matching,
    );
    let matching = best_matching.expect("at least the empty representative matching exists");

    let ranking_limit = clusters.members.len().min(MAX_TOP_K);
    let mut results = Vec::with_capacity(ranking_limit);
    let mut selected_clusters = HashSet::new();
    if let Some(verse_index) = protected_verse {
        if let Some(result) = whole
            .iter()
            .find(|result| result.verse_index == verse_index)
        {
            let cluster = clusters.cluster_of[&verse_index];
            selected_clusters.insert(cluster);
            // Preserve the exact direct/correction claim and provenance. No
            // independently searched clause may strengthen or rewrite it.
            results.push(result.clone());
        }
    }

    for (active_position, assignment) in matching.assignments.iter().enumerate() {
        let Some(cluster) = assignment else {
            continue;
        };
        if !selected_clusters.insert(*cluster) {
            continue;
        }
        let group_index = active_groups[active_position];
        let members = &clusters.members[cluster];
        let verse_index = members
            .iter()
            .min_by_key(|verse_index| {
                let primary = groups[group_index]
                    .introducers
                    .iter()
                    .filter(|index| segment_introduces.get(**index).copied().unwrap_or(false))
                    .filter_map(|index| segment_ranks.get(*index)?.get(verse_index).copied())
                    .min()
                    .unwrap_or(usize::MAX);
                (primary, **verse_index)
            })
            .copied()
            .expect("answer cluster is nonempty");
        let mut result = candidates[&verse_index].clone();
        mark_composed_result(&mut result, true);
        results.push(result);
    }

    #[derive(Clone, Copy)]
    struct FallbackCandidate {
        cluster: u32,
        score: f64,
        refinement_count: usize,
        refinement_rank: usize,
        primary_rank: usize,
        whole_rank: usize,
    }
    let reciprocal_rank = |rank: usize| 1.0 / (COMPOSITION_RRF_K + rank as f64 + 1.0);
    let mut fallback = Vec::new();
    for (&cluster, members) in &clusters.members {
        if selected_clusters.contains(&cluster) {
            continue;
        }
        let whole_rank = cluster_rank(&whole_ranks, members);
        let mut score = whole_rank.map_or(0.0, reciprocal_rank);
        let mut primary_best = usize::MAX;
        let mut refinement_best = usize::MAX;
        let mut refinement_count = 0usize;
        let mut has_primary = false;
        for group_index in &active_groups {
            if let Some(primary_rank) = group_primary_rank(
                &groups[*group_index],
                members,
                &segment_ranks,
                &segment_introduces,
            ) {
                has_primary = true;
                primary_best = primary_best.min(primary_rank);
                // Alternative views contribute one normalized group vote.
                score += reciprocal_rank(primary_rank);
                if let Some(refinement_rank) = group_refinement_rank(
                    &groups[*group_index],
                    members,
                    &segment_ranks,
                    &segment_introduces,
                ) {
                    refinement_count += 1;
                    refinement_best = refinement_best.min(refinement_rank);
                }
            }
        }
        // Tied/flat primary views and refinement-only views cannot introduce a
        // candidate. Whole-query candidates remain a bounded fallback.
        if has_primary || whole_rank.is_some() {
            fallback.push(FallbackCandidate {
                cluster,
                score,
                refinement_count,
                refinement_rank: refinement_best,
                primary_rank: primary_best,
                whole_rank: whole_rank.unwrap_or(usize::MAX),
            });
        }
    }
    fallback.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| right.refinement_count.cmp(&left.refinement_count))
            .then_with(|| left.refinement_rank.cmp(&right.refinement_rank))
            .then_with(|| left.primary_rank.cmp(&right.primary_rank))
            .then_with(|| left.whole_rank.cmp(&right.whole_rank))
            .then_with(|| left.cluster.cmp(&right.cluster))
    });
    for item in fallback {
        if results.len() >= ranking_limit || !selected_clusters.insert(item.cluster) {
            continue;
        }
        let members = &clusters.members[&item.cluster];
        let verse_index = members
            .iter()
            .min_by_key(|verse_index| {
                let primary = active_groups
                    .iter()
                    .filter_map(|group_index| {
                        groups[*group_index]
                            .introducers
                            .iter()
                            .filter(|index| {
                                segment_introduces.get(**index).copied().unwrap_or(false)
                            })
                            .filter_map(|index| {
                                segment_ranks.get(*index)?.get(verse_index).copied()
                            })
                            .min()
                    })
                    .min()
                    .unwrap_or(usize::MAX);
                let whole = whole_ranks.get(verse_index).copied().unwrap_or(usize::MAX);
                (primary.min(whole), primary, whole, **verse_index)
            })
            .copied()
            .expect("answer cluster is nonempty");
        let from_segment = active_groups.iter().any(|group_index| {
            groups[*group_index]
                .introducers
                .iter()
                .filter(|index| segment_introduces.get(**index).copied().unwrap_or(false))
                .any(|index| segment_ranks[*index].contains_key(&verse_index))
        });
        let mut result = candidates[&verse_index].clone();
        mark_composed_result(&mut result, from_segment);
        results.push(result);
    }

    rebase_merged_scores(&mut results);
    // diversify_segment_results does not have the verse table; the caller
    // re-applies passage stitching when verses are available. Keep the
    // rank-stable prefix here.
    results.truncate(output_limit);
    results
}

fn merge_result_evidence(
    candidates: &mut BTreeMap<u32, crate::result::SearchResult>,
    incoming: &crate::result::SearchResult,
) {
    let Some(existing) = candidates.get_mut(&incoming.verse_index) else {
        candidates.insert(incoming.verse_index, incoming.clone());
        return;
    };
    merge_search_result_evidence(existing, incoming);
}

fn merge_search_result_evidence(
    existing: &mut crate::result::SearchResult,
    incoming: &crate::result::SearchResult,
) {
    existing.lanes.a = existing.lanes.a.max(incoming.lanes.a);
    existing.lanes.b = existing.lanes.b.max(incoming.lanes.b);
    existing.lanes.c = existing.lanes.c.max(incoming.lanes.c);
    existing.lanes.d = existing.lanes.d.max(incoming.lanes.d);
    merge_unique_strings(&mut existing.matched_terms, &incoming.matched_terms);
    merge_unique_strings(&mut existing.expanded_terms, &incoming.expanded_terms);
    merge_unique_strings(&mut existing.chips, &incoming.chips);
    for passage in &incoming.passages {
        if !existing.passages.contains(passage) {
            existing.passages.push(passage.clone());
        }
    }
    existing.confidence = match (existing.confidence, incoming.confidence) {
        (crate::result::Confidence::OftenMisremembered, _)
        | (_, crate::result::Confidence::OftenMisremembered) => {
            crate::result::Confidence::OftenMisremembered
        }
        (crate::result::Confidence::Recovered, _) | (_, crate::result::Confidence::Recovered) => {
            crate::result::Confidence::Recovered
        }
        _ => crate::result::Confidence::Related,
    };
}

fn merge_unique_strings(target: &mut Vec<String>, incoming: &[String]) {
    for value in incoming {
        if !target.contains(value) {
            target.push(value.clone());
        }
    }
}

/// Re-rank the existing canonical dense pool when an independently encoded
/// passage agrees with it.
///
/// A passage is never allowed to introduce its anchor (or any other verse) on
/// its own. It must contain a candidate already returned by canonical LVEC at
/// the fixed lane depth, and it is remapped to the best-ranked such candidate.
/// The two representations then contribute equally through their query-local
/// ranks. Raw cosine values are deliberately not compared across corpora.
fn merge_supported_passage_dense(
    verse_hits: Vec<crate::lanes::LaneHit>,
    passage_hits: Vec<crate::lanes::LaneHit>,
    verses: &[Verse],
) -> Vec<crate::lanes::LaneHit> {
    const RRF_K: f64 = 60.0;

    if verse_hits.is_empty() || passage_hits.is_empty() {
        return verse_hits;
    }
    let reference_indices: HashMap<&str, u32> = verses
        .iter()
        .map(|verse| (verse.reference.as_str(), verse.index))
        .collect();
    let reciprocal_rank = |rank: usize| 1.0 / (RRF_K + rank as f64 + 1.0);

    // One supporting verse may occur in several overlapping source passages.
    // Retain only the passage with the strongest rank consensus so structured
    // provenance cannot be widened by unioning unrelated ranges.
    let mut best_by_support: HashMap<u32, (f64, usize, crate::lanes::LaneHit)> = HashMap::new();
    for (passage_rank, passage_hit) in passage_hits
        .into_iter()
        .take(LANE_CANDIDATE_DEPTH)
        .enumerate()
    {
        let [passage] = passage_hit.passages.as_slice() else {
            continue;
        };
        let (Some(&start), Some(&end)) = (
            reference_indices.get(passage.start_ref.as_str()),
            reference_indices.get(passage.end_ref.as_str()),
        ) else {
            continue;
        };
        if start > end {
            continue;
        }
        let Some((verse_rank, support)) = verse_hits
            .iter()
            .take(LANE_CANDIDATE_DEPTH)
            .enumerate()
            .find(|(_, hit)| start <= hit.verse_index && hit.verse_index <= end)
        else {
            continue;
        };
        let consensus = 0.5 * reciprocal_rank(verse_rank) + 0.5 * reciprocal_rank(passage_rank);
        let mut agreed = support.clone();
        merge_unique_strings(&mut agreed.matched_terms, &passage_hit.matched_terms);
        merge_unique_strings(&mut agreed.expanded_terms, &passage_hit.expanded_terms);
        merge_unique_strings(
            &mut agreed.expanded_terms,
            &["semantic-agreement:verse+passage".into()],
        );
        if !agreed.passages.contains(passage) {
            agreed.passages.push(passage.clone());
        }
        // Agreement changes semantic order and explanation, never authority.
        agreed.confidence_hint = crate::result::Confidence::Related;
        agreed.ranking_tier = 0;
        agreed.direct_tier = 0;

        let replace = match best_by_support.get(&support.verse_index) {
            None => true,
            Some((current_consensus, current_passage_rank, _)) => {
                consensus.total_cmp(current_consensus).is_gt()
                    || (consensus.total_cmp(current_consensus).is_eq()
                        && passage_rank < *current_passage_rank)
            }
        };
        if replace {
            best_by_support.insert(support.verse_index, (consensus, passage_rank, agreed));
        }
    }

    let mut ranked: Vec<(f64, usize, crate::lanes::LaneHit)> = verse_hits
        .into_iter()
        .enumerate()
        .map(|(verse_rank, hit)| {
            let base = reciprocal_rank(verse_rank);
            match best_by_support.remove(&hit.verse_index) {
                Some((consensus, _, agreed)) => (base.max(consensus), verse_rank, agreed),
                None => (base, verse_rank, hit),
            }
        })
        .collect();
    ranked.sort_by(|left, right| {
        right
            .0
            .total_cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.verse_index.cmp(&right.2.verse_index))
    });
    ranked.into_iter().map(|(_, _, hit)| hit).collect()
}

/// Let two independent representations corroborate one another without
/// reverting to all-lane score summation. Dense order remains authoritative:
/// rank one is fixed, only top-24 intersections with at least 60% lexical
/// query coverage may move, and movement is capped at two positions.
fn promote_dense_context_agreement(
    dense_hits: &mut Vec<crate::lanes::LaneHit>,
    context_hits: &[crate::lanes::LaneHit],
    query: &crate::normalize::NormalizedQuery,
) {
    const AGREEMENT_DEPTH: usize = 24;
    const COVERAGE_NUMERATOR: usize = 3;
    const COVERAGE_DENOMINATOR: usize = 5;
    const MIN_MATCHED_TERMS: usize = 2;
    const MAX_PROMOTION: usize = 2;

    let query_stems: HashSet<&str> = query
        .lemmas
        .iter()
        .map(String::as_str)
        .filter(|stem| stem.len() > 1)
        .collect();
    if query_stems.len() < MIN_MATCHED_TERMS || dense_hits.len() < 2 {
        return;
    }
    let required_coverage = query_stems
        .len()
        .saturating_mul(COVERAGE_NUMERATOR)
        .div_ceil(COVERAGE_DENOMINATOR)
        .max(MIN_MATCHED_TERMS);
    let context_by_verse: HashMap<u32, &crate::lanes::LaneHit> = context_hits
        .iter()
        .take(AGREEMENT_DEPTH)
        .filter(|hit| {
            hit.matched_terms
                .iter()
                .map(String::as_str)
                .filter(|term| query_stems.contains(term))
                .collect::<HashSet<_>>()
                .len()
                >= required_coverage
        })
        .map(|hit| (hit.verse_index, hit))
        .collect();
    if context_by_verse.is_empty() {
        return;
    }

    let mut ranked: Vec<(usize, usize, crate::lanes::LaneHit)> = std::mem::take(dense_hits)
        .into_iter()
        .enumerate()
        .map(|(rank, mut hit)| {
            let context = (rank < AGREEMENT_DEPTH)
                .then(|| context_by_verse.get(&hit.verse_index).copied())
                .flatten();
            let promotion = if let Some(context) = context {
                for term in &context.matched_terms {
                    if !hit.matched_terms.contains(term) {
                        hit.matched_terms.push(term.clone());
                    }
                }
                for evidence in ["context:study", "semantic-agreement:dense+context"] {
                    if !hit.expanded_terms.iter().any(|term| term == evidence) {
                        hit.expanded_terms.push(evidence.to_string());
                    }
                }
                // Agreement changes rank evidence, never authority.
                hit.confidence_hint = crate::result::Confidence::Related;
                hit.ranking_tier = 0;
                hit.direct_tier = 0;
                MAX_PROMOTION
            } else {
                0
            };
            (rank.saturating_sub(promotion), rank, hit)
        })
        .collect();
    ranked.sort_by_key(|(adjusted_rank, original_rank, _)| (*adjusted_rank, *original_rank));
    dense_hits.extend(ranked.into_iter().map(|(_, _, hit)| hit));
}

fn attach_dense_window_passages(hits: &mut [crate::lanes::LaneHit], verses: &[Verse]) {
    for hit in hits {
        if !hit
            .expanded_terms
            .iter()
            .any(|term| term == "semantic:window")
        {
            continue;
        }
        let center = hit.verse_index as usize;
        let Some(center_verse) = verses.get(center) else {
            continue;
        };
        let Some(center_chapter) = canonical_chapter_key(&center_verse.reference) else {
            continue;
        };
        let start = center.checked_sub(1).filter(|index| {
            verses
                .get(*index)
                .and_then(|verse| canonical_chapter_key(&verse.reference))
                .is_some_and(|chapter| chapter == center_chapter)
        });
        let end = (center + 1 < verses.len())
            .then_some(center + 1)
            .filter(|index| {
                verses
                    .get(*index)
                    .and_then(|verse| canonical_chapter_key(&verse.reference))
                    .is_some_and(|chapter| chapter == center_chapter)
            });
        hit.passages.push(crate::result::PassageRange {
            start_ref: verses[start.unwrap_or(center)].reference.clone(),
            end_ref: verses[end.unwrap_or(center)].reference.clone(),
        });
    }
}

fn canonical_chapter_key(reference: &str) -> Option<(&str, &str)> {
    let (book, chapter_and_verse) = reference.rsplit_once(' ')?;
    let (chapter, verse) = chapter_and_verse.split_once(':')?;
    if book.is_empty()
        || chapter.is_empty()
        || verse.is_empty()
        || !chapter.bytes().all(|byte| byte.is_ascii_digit())
        || !verse.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((book, chapter))
}

fn merge_lexical_head_and_semantic_tail(
    lexical: Vec<crate::result::SearchResult>,
    semantic: Vec<crate::result::SearchResult>,
    top_k: usize,
) -> Vec<crate::result::SearchResult> {
    const LEXICAL_HEAD: usize = 6;

    let mut merged = Vec::with_capacity(top_k.max(1));
    let mut positions = HashMap::new();
    // When the dense leader is independently present in the protected lexical
    // head, that cross-representation agreement is stronger than either raw
    // rank alone. An explicit misquote label remains immovable; otherwise
    // promote the agreed candidate even when lexical evidence alone was
    // overconfident about a neighboring or parallel passage.
    let lexical_claims_explicit_misquote = lexical.first().is_some_and(|result| {
        matches!(
            result.confidence,
            crate::result::Confidence::OftenMisremembered
        )
    });
    if !lexical_claims_explicit_misquote {
        if let Some(semantic_leader) = semantic.first().filter(|semantic_leader| {
            lexical
                .iter()
                .take(LEXICAL_HEAD)
                .any(|lexical_result| lexical_result.verse_index == semantic_leader.verse_index)
        }) {
            append_or_merge_result(&mut merged, &mut positions, semantic_leader);
        }
    }
    for result in lexical.iter().take(LEXICAL_HEAD) {
        append_or_merge_result(&mut merged, &mut positions, result);
    }
    for result in &semantic {
        append_or_merge_result(&mut merged, &mut positions, result);
    }
    for result in lexical.iter().skip(LEXICAL_HEAD) {
        append_or_merge_result(&mut merged, &mut positions, result);
    }
    merged.truncate(top_k.max(1));
    rebase_merged_scores(&mut merged);
    merged
}

fn merge_semantic_and_lexical(
    semantic: Vec<crate::result::SearchResult>,
    lexical: Vec<crate::result::SearchResult>,
    top_k: usize,
) -> Vec<crate::result::SearchResult> {
    const SEMANTIC_HEAD: usize = 3;
    const INTERLEAVED_LEXICAL_RESERVE: usize = 4;

    let mut merged = Vec::with_capacity(top_k.max(1));
    let mut positions = HashMap::new();
    for result in semantic.iter().take(SEMANTIC_HEAD) {
        append_or_merge_result(&mut merged, &mut positions, result);
    }

    // With no direct lexical resolution, Lane C is the primary retrieval
    // view. Preserve its stable top three, then alternate a bounded lexical
    // reserve with the dense tail before returning to dense-first order. The
    // former six-result lexical block pushed dense ranks 4-9 outside a top-ten
    // response even though the semantic encoder had already retrieved them.
    // The bounded alternation is rank-only and query-agnostic: it improves
    // semantic tail exposure without sacrificing deep dense recall, comparing
    // uncalibrated scores, or changing the protected exact/event/translation
    // path above.
    let mut semantic_tail = semantic.iter().skip(SEMANTIC_HEAD);
    let mut lexical_tail = lexical.iter();
    for _ in 0..INTERLEAVED_LEXICAL_RESERVE {
        let semantic_next = semantic_tail.next();
        let lexical_next = lexical_tail.next();
        if semantic_next.is_none() && lexical_next.is_none() {
            break;
        }
        if let Some(result) = semantic_next {
            append_or_merge_result(&mut merged, &mut positions, result);
        }
        if let Some(result) = lexical_next {
            append_or_merge_result(&mut merged, &mut positions, result);
        }
    }
    for result in semantic_tail {
        append_or_merge_result(&mut merged, &mut positions, result);
    }
    for result in lexical_tail {
        append_or_merge_result(&mut merged, &mut positions, result);
    }
    merged.truncate(top_k.max(1));

    rebase_merged_scores(&mut merged);
    merged
}

fn append_or_merge_result(
    merged: &mut Vec<crate::result::SearchResult>,
    positions: &mut HashMap<u32, usize>,
    incoming: &crate::result::SearchResult,
) {
    if let Some(&position) = positions.get(&incoming.verse_index) {
        merge_search_result_evidence(&mut merged[position], incoming);
        return;
    }
    positions.insert(incoming.verse_index, merged.len());
    merged.push(incoming.clone());
}

fn rebase_merged_scores(merged: &mut [crate::result::SearchResult]) {
    // Fusion scores are response-local. Rebase after deterministic
    // interleaving so the public score remains monotonic with returned order.
    let merged_len = merged.len();
    let denominator = merged_len.max(1) as f32;
    for (rank, result) in merged.iter_mut().enumerate() {
        result.score = (merged_len - rank) as f32 / denominator;
    }
}

/// Share one immutable compact index between Lane A and expansion Lane B.
struct SharedPackedLexicalLane(Arc<PackedLexicalLane>);

impl Lane for SharedPackedLexicalLane {
    fn id(&self) -> LaneId {
        LaneId::A
    }

    fn available(&self) -> bool {
        self.0.available()
    }

    fn search(
        &self,
        query: &crate::normalize::NormalizedQuery,
        top_k: usize,
    ) -> Vec<crate::lanes::LaneHit> {
        self.0.search(query, top_k)
    }
}

/// Packed semantic neighbors are broad evidence. Contrastive phrasing such as
/// “depend on God rather than my own judgment” must not expand the rejected
/// side and let its vocabulary dominate Lane B.
struct ContrastAwarePackedExpandLane(Arc<PackedExpandLane>);

impl Lane for ContrastAwarePackedExpandLane {
    fn id(&self) -> LaneId {
        LaneId::B
    }

    fn available(&self) -> bool {
        self.0.available()
    }

    fn search(
        &self,
        query: &crate::normalize::NormalizedQuery,
        top_k: usize,
    ) -> Vec<crate::lanes::LaneHit> {
        let stems = semantic_query_stems(query);
        self.0.search_stems(&stems, top_k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lanes::LaneHit;
    use crate::normalize::NormalizedQuery;
    use crate::result::{Confidence, PassageRange};
    use recall_pack::{
        LexSection, LexTerm, MetaSection, PackWriter, PostSection, PostTerm, Posting, PvecRow,
        PvecSection, PvecSourceKind, TextSection,
    };

    fn merge_fixture_result(index: u32, label: &str) -> crate::result::SearchResult {
        crate::result::SearchResult {
            reference: format!("{label} {index}"),
            text: label.to_string(),
            score: 1.0,
            confidence: Confidence::Related,
            lanes: crate::lanes::LaneScores::default(),
            matched_terms: Vec::new(),
            expanded_terms: Vec::new(),
            passages: Vec::new(),
            chips: Vec::new(),
            verse_index: index,
        }
    }

    fn passage_consensus_verses() -> Vec<Verse> {
        TextSection::from_verses(
            (1..=6)
                .map(|verse| (format!("Genesis 1:{verse}"), format!("verse {verse}")))
                .collect(),
        )
        .verses
    }

    fn semantic_hit(verse_index: u32) -> LaneHit {
        LaneHit {
            verse_index,
            score: 0.8,
            matched_terms: Vec::new(),
            expanded_terms: vec!["semantic:verse".into()],
            passages: Vec::new(),
            confidence_hint: Confidence::Related,
            ranking_tier: 0,
            direct_tier: 0,
        }
    }

    fn semantic_passage_hit(anchor: u32, start: &str, end: &str) -> LaneHit {
        LaneHit {
            verse_index: anchor,
            score: 0.9,
            matched_terms: Vec::new(),
            expanded_terms: vec!["semantic:passage:study-bible-passage".into()],
            passages: vec![PassageRange {
                start_ref: start.into(),
                end_ref: end.into(),
            }],
            confidence_hint: Confidence::Related,
            ranking_tier: 0,
            direct_tier: 0,
        }
    }

    #[test]
    fn passage_dense_cannot_introduce_an_unsupported_candidate() {
        let verses = passage_consensus_verses();
        let verse_hits = (0..4).map(semantic_hit).collect::<Vec<_>>();
        let output = merge_supported_passage_dense(
            verse_hits,
            vec![semantic_passage_hit(4, "Genesis 1:5", "Genesis 1:6")],
            &verses,
        );

        assert_eq!(
            output.iter().map(|hit| hit.verse_index).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert!(output.iter().all(|hit| hit.passages.is_empty()));
    }

    #[test]
    fn passage_dense_rank_consensus_elevates_support_without_replacing_the_head() {
        let verses = passage_consensus_verses();
        let verse_hits = (0..6).map(semantic_hit).collect::<Vec<_>>();
        let output = merge_supported_passage_dense(
            verse_hits,
            vec![semantic_passage_hit(5, "Genesis 1:6", "Genesis 1:6")],
            &verses,
        );

        assert_eq!(
            output.iter().map(|hit| hit.verse_index).collect::<Vec<_>>(),
            [0, 1, 2, 5, 3, 4]
        );
        let agreed = &output[3];
        assert_eq!(agreed.confidence_hint, Confidence::Related);
        assert_eq!(agreed.ranking_tier, 0);
        assert_eq!(agreed.direct_tier, 0);
        assert_eq!(
            agreed.passages,
            [PassageRange {
                start_ref: "Genesis 1:6".into(),
                end_ref: "Genesis 1:6".into(),
            }]
        );
        assert!(agreed
            .expanded_terms
            .iter()
            .any(|term| term == "semantic-agreement:verse+passage"));
    }

    #[test]
    fn semantic_mode_preserves_head_then_interleaves_a_bounded_lexical_reserve() {
        let semantic: Vec<_> = (0..20)
            .map(|index| merge_fixture_result(index, "semantic"))
            .collect();
        let lexical: Vec<_> = (100..120)
            .map(|index| merge_fixture_result(index, "lexical"))
            .collect();

        let merged = merge_semantic_and_lexical(semantic, lexical, 15);
        let indices: Vec<_> = merged.iter().map(|result| result.verse_index).collect();
        assert_eq!(
            indices,
            [0, 1, 2, 3, 100, 4, 101, 5, 102, 6, 103, 7, 8, 9, 10]
        );
    }

    fn fixture_pack() -> Vec<u8> {
        let verses = vec![
            (
                "John 3:16".into(),
                "For God so loved the world that He gave His one and only Son, that everyone who believes in Him shall not perish but have eternal life.".into(),
            ),
            (
                "Luke 15:4".into(),
                "What man among you, if he has a hundred sheep and loses one of them, does not leave the ninety-nine in the pasture and go after the one that is lost, until he finds it?".into(),
            ),
            (
                "Psalm 23:1".into(),
                "The LORD is my shepherd; I shall not want.".into(),
            ),
            (
                "Psalm 23:4".into(),
                "Even though I walk through the valley of the shadow of death, I will fear no evil, for You are with me; Your rod and Your staff, they comfort me.".into(),
            ),
            (
                "Matthew 5:3".into(),
                "Blessed are the poor in spirit, for theirs is the kingdom of heaven.".into(),
            ),
            (
                "Genesis 1:1".into(),
                "In the beginning God created the heavens and the earth.".into(),
            ),
            (
                "Ephesians 6:11".into(),
                "Put on the full armor of God, so that you can make your stand against the devil’s schemes.".into(),
            ),
            (
                "Galatians 5:22".into(),
                "But the fruit of the Spirit is love, joy, peace, patience, kindness, goodness, faithfulness,".into(),
            ),
            (
                "1 Timothy 6:10".into(),
                "For the love of money is the root of all kinds of evil. By craving it, some have wandered away from the faith and pierced themselves with many sorrows.".into(),
            ),
            (
                "Proverbs 3:5".into(),
                "Trust in the LORD with all your heart, and lean not on your own understanding;".into(),
            ),
            (
                "Genesis 3:1".into(),
                "Now the serpent was more crafty than any beast of the field that the LORD God had made.".into(),
            ),
            (
                "Matthew 21:44".into(),
                "He who falls on this stone will be broken to pieces, but he on whom it falls will be crushed.".into(),
            ),
            (
                "Luke 20:18".into(),
                "He who falls on this stone will be broken to pieces, but he on whom it falls will be crushed.".into(),
            ),
            (
                "Hebrews 11:1".into(),
                "Now faith is the assurance of what we hope for and the certainty of what we do not see.".into(),
            ),
        ];
        let text = TextSection::from_verses(verses);
        let meta = MetaSection::new("BSB", text.verses.len() as u32);
        PackWriter::new().add_meta(&meta).add_text(&text).finish()
    }

    fn fixture_context_pack(
        target_index: u32,
        terms: &[&str],
        translation: &str,
        verse_count: usize,
    ) -> Vec<u8> {
        let mut terms: Vec<String> = terms
            .iter()
            .map(|term| crate::normalize::stem(term))
            .collect();
        terms.sort();
        terms.dedup();
        let lex = LexSection::new(
            verse_count as u32,
            terms
                .iter()
                .map(|term| LexTerm {
                    text: term.clone(),
                    document_frequency: 1,
                })
                .collect(),
        )
        .unwrap();
        let post = PostSection::new(
            vec![1; verse_count],
            terms
                .iter()
                .map(|_| PostTerm {
                    postings: vec![Posting {
                        document_id: target_index,
                        term_frequency: 1,
                    }],
                })
                .collect(),
        )
        .unwrap();
        PackWriter::new()
            .add_meta(&MetaSection::new(translation, verse_count as u32))
            .add_context_lex(&lex)
            .add_context_post(&post)
            .finish()
    }

    #[test]
    fn context_sparse_is_auxiliary_and_ablation_keeps_related_confidence() {
        let base = fixture_pack();
        let context = fixture_context_pack(0, &["hypostatic", "mediation", "kenosis"], "BSB", 14);
        let engine = Engine::open(&[&base, &context]).unwrap();
        assert_eq!(engine.available_lanes(), vec![LaneId::A, LaneId::C]);

        let response = engine.search(
            "hypostatic mediation kenosis",
            &SearchOpts::default().with_top_k(3),
        );
        assert!(
            response.results.is_empty(),
            "installing auxiliary prose alone must not create an ordinary retrieval vote"
        );

        let ablation = engine.search_context_only(
            "hypostatic mediation kenosis",
            &SearchOpts::default().with_top_k(3),
        );
        assert_eq!(ablation.results[0].reference, "John 3:16");
        assert_eq!(ablation.results[0].confidence, Confidence::Related);
        assert_eq!(ablation.results[0].lanes.a, 0.0);
        assert!(ablation.results[0].lanes.c > 0.0);
        assert!(ablation.results[0]
            .expanded_terms
            .iter()
            .any(|term| term == "context:study"));
        assert!(
            ablation.results[0].score < 1.0,
            "context must not receive a direct tier"
        );
    }

    #[test]
    fn context_sparse_sections_fail_closed_when_unpaired_or_mistargeted() {
        let base = fixture_pack();
        let valid = fixture_context_pack(0, &["hypostatic"], "BSB", 14);
        let valid_reader = PackReader::open(&valid).unwrap();
        let unpaired = PackWriter::new()
            .add_meta(&MetaSection::new("BSB", 14))
            .add_raw(
                SectionTag::Clex,
                valid_reader
                    .section_bytes(SectionTag::Clex)
                    .unwrap()
                    .to_vec(),
            )
            .finish();
        let error = Engine::open(&[&base, &unpaired])
            .err()
            .expect("unpaired context sections must fail");
        assert!(error
            .to_string()
            .contains("CLEX and CPOST must be supplied together"));

        let mistargeted = fixture_context_pack(0, &["hypostatic"], "OTHER", 14);
        let error = Engine::open(&[&base, &mistargeted])
            .err()
            .expect("mistargeted context sections must fail");
        assert!(error
            .to_string()
            .contains("CLEX/CPOST metadata does not match canonical TEXT"));
    }

    #[test]
    fn malformed_context_sparse_index_is_rejected_at_open() {
        let base = fixture_pack();
        let valid = fixture_context_pack(0, &["hypostatic"], "BSB", 14);
        let valid_reader = PackReader::open(&valid).unwrap();
        let malformed = PackWriter::new()
            .add_meta(&MetaSection::new("BSB", 14))
            .add_raw(SectionTag::Clex, b"malformed".to_vec())
            .add_raw(
                SectionTag::Cpost,
                valid_reader
                    .section_bytes(SectionTag::Cpost)
                    .unwrap()
                    .to_vec(),
            )
            .finish();

        assert!(matches!(
            Engine::open(&[&base, &malformed]),
            Err(CoreError::Pack(recall_pack::PackError::InvalidLex))
        ));
    }

    #[test]
    fn dense_context_agreement_is_bounded_and_never_direct() {
        let mut dense: Vec<LaneHit> = (0..6)
            .map(|verse_index| LaneHit {
                verse_index,
                score: 1.0 - verse_index as f32 * 0.01,
                matched_terms: Vec::new(),
                expanded_terms: vec!["semantic:verse".into()],
                passages: Vec::new(),
                confidence_hint: Confidence::Related,
                ranking_tier: 0,
                direct_tier: 0,
            })
            .collect();
        let context = vec![LaneHit {
            verse_index: 3,
            score: 8.0,
            matched_terms: vec!["alpha".into(), "beta".into()],
            expanded_terms: vec!["context:study".into()],
            passages: Vec::new(),
            // Even malformed/upstream authority is stripped at agreement.
            confidence_hint: Confidence::Recovered,
            ranking_tier: 3,
            direct_tier: 3,
        }];

        promote_dense_context_agreement(&mut dense, &context, &normalize_query("alpha beta"));

        assert_eq!(
            dense.iter().map(|hit| hit.verse_index).collect::<Vec<_>>(),
            vec![0, 1, 3, 2, 4, 5]
        );
        let agreed = &dense[2];
        assert_eq!(agreed.confidence_hint, Confidence::Related);
        assert_eq!(agreed.ranking_tier, 0);
        assert_eq!(agreed.direct_tier, 0);
        assert!(agreed
            .expanded_terms
            .iter()
            .any(|term| term == "semantic-agreement:dense+context"));
    }

    #[test]
    fn weak_context_overlap_does_not_reorder_dense_results() {
        let mut dense: Vec<LaneHit> = (0..4)
            .map(|verse_index| LaneHit {
                verse_index,
                score: 1.0,
                matched_terms: Vec::new(),
                expanded_terms: vec!["semantic:verse".into()],
                passages: Vec::new(),
                confidence_hint: Confidence::Related,
                ranking_tier: 0,
                direct_tier: 0,
            })
            .collect();
        let context = vec![LaneHit {
            verse_index: 3,
            score: 8.0,
            matched_terms: vec!["alpha".into()],
            expanded_terms: vec!["context:study".into()],
            passages: Vec::new(),
            confidence_hint: Confidence::Related,
            ranking_tier: 0,
            direct_tier: 0,
        }];

        promote_dense_context_agreement(&mut dense, &context, &normalize_query("alpha beta"));

        assert_eq!(
            dense.iter().map(|hit| hit.verse_index).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert!(dense.iter().all(|hit| !hit
            .expanded_terms
            .iter()
            .any(|term| term == "semantic-agreement:dense+context")));
    }

    fn fixture_semantic_pack(target_index: usize) -> Vec<u8> {
        let verse_count = 14usize;
        let dims = 2u16;
        let mut body = Vec::with_capacity(16 + verse_count * usize::from(dims));
        body.extend_from_slice(b"LVC1");
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&dims.to_le_bytes());
        body.extend_from_slice(&(verse_count as u32).to_le_bytes());
        body.push(1);
        body.extend_from_slice(&[0, 0, 0]);
        for index in 0..verse_count {
            body.extend_from_slice(if index == target_index {
                &[127, 0]
            } else {
                &[0, 127]
            });
        }
        PackWriter::new()
            .add_meta(&MetaSection::new("BSB", verse_count as u32))
            .add_raw(SectionTag::Lvec, body)
            .finish()
    }

    const TEST_EMBEDDING_SPACE_ID: [u8; 32] = [0x5a; 32];

    fn fixture_modern_semantic_pack(target_index: usize, embedding_space_id: [u8; 32]) -> Vec<u8> {
        let verse_count = 14usize;
        let mut table = Vec::with_capacity(verse_count * 2);
        for index in 0..verse_count {
            table.extend_from_slice(if index == target_index {
                &[127, 0]
            } else {
                &[0, 127]
            });
        }
        let body = DenseLane::encode_v2(2, verse_count, &[&table], embedding_space_id).unwrap();
        PackWriter::new()
            .add_meta(&MetaSection::new("BSB", verse_count as u32))
            .add_raw(SectionTag::Lvec, body)
            .finish()
    }

    fn fixture_passage_semantic_pack(embedding_space_id: [u8; 32], dims: u16) -> Vec<u8> {
        let mut vector = vec![0; usize::from(dims)];
        vector[0] = 127;
        let body = PvecSection::new(
            dims,
            14,
            embedding_space_id,
            vec![PvecRow {
                anchor_index: 2,
                start_index: 2,
                end_index: 3,
                source_kind: PvecSourceKind::StudyBiblePassage,
            }],
            vector,
        )
        .unwrap()
        .encode()
        .unwrap();
        PackWriter::new()
            .add_meta(&MetaSection::new("BSB", 14))
            .add_raw(SectionTag::Pvec, body)
            .finish()
    }

    #[test]
    fn passage_dense_requires_matching_canonical_dense_space_and_dimensions() {
        let base = fixture_pack();
        let passage = fixture_passage_semantic_pack(TEST_EMBEDDING_SPACE_ID, 2);
        let missing_dense = Engine::open(&[&base, &passage])
            .err()
            .expect("PVEC without LVEC must fail closed");
        assert!(missing_dense
            .to_string()
            .contains("PVEC requires a canonical LVEC"));

        let dense = fixture_modern_semantic_pack(3, TEST_EMBEDDING_SPACE_ID);
        let wrong_space = fixture_passage_semantic_pack([0x33; 32], 2);
        let mismatch = Engine::open(&[&base, &dense, &wrong_space])
            .err()
            .expect("mismatched spaces must fail closed");
        assert!(mismatch
            .to_string()
            .contains("embedding-space ID must exactly match LVEC"));

        let wrong_dims = fixture_passage_semantic_pack(TEST_EMBEDDING_SPACE_ID, 3);
        let mismatch = Engine::open(&[&base, &dense, &wrong_dims])
            .err()
            .expect("mismatched dimensions must fail closed");
        assert!(mismatch
            .to_string()
            .contains("dimensions and embedding-space ID must exactly match LVEC"));
    }

    #[test]
    fn supported_passage_dense_preserves_direct_head_and_depth_prefixes() {
        let base = fixture_pack();
        let dense = fixture_modern_semantic_pack(3, TEST_EMBEDDING_SPACE_ID);
        let passage = fixture_passage_semantic_pack(TEST_EMBEDDING_SPACE_ID, 2);
        let engine = Engine::open(&[&base, &dense, &passage]).unwrap();
        assert_eq!(engine.passage_dense_metadata().unwrap().passage_count, 1);

        let deep = engine
            .search_with_embedding_in_space(
                "the Lord is my shepherd",
                &TEST_EMBEDDING_SPACE_ID,
                &[1.0, 0.0],
                &SearchOpts::default().with_top_k(10),
            )
            .unwrap();
        assert_eq!(deep.results[0].reference, "Psalm 23:1");
        assert!(deep.results.iter().any(|result| {
            result.reference == "Psalm 23:4"
                && result
                    .expanded_terms
                    .iter()
                    .any(|term| term == "semantic-agreement:verse+passage")
                && result.passages.contains(&PassageRange {
                    start_ref: "Psalm 23:1".into(),
                    end_ref: "Psalm 23:4".into(),
                })
        }));
        for depth in [1, 3, 5] {
            let shallow = engine
                .search_with_embedding_in_space(
                    "the Lord is my shepherd",
                    &TEST_EMBEDDING_SPACE_ID,
                    &[1.0, 0.0],
                    &SearchOpts::default().with_top_k(depth),
                )
                .unwrap();
            assert_eq!(
                shallow
                    .results
                    .iter()
                    .map(|result| result.verse_index)
                    .collect::<Vec<_>>(),
                deep.results
                    .iter()
                    .take(depth)
                    .map(|result| result.verse_index)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn dense_window_passages_match_chapter_bounded_build_windows() {
        let verses = TextSection::from_verses(vec![
            ("Genesis 1:31".into(), "chapter one".into()),
            ("Genesis 2:1".into(), "chapter two one".into()),
            ("Genesis 2:2".into(), "chapter two two".into()),
            ("Exodus 1:1".into(), "new book".into()),
        ])
        .verses;
        let mut hits: Vec<LaneHit> = (0..4)
            .map(|verse_index| LaneHit {
                verse_index,
                score: 1.0,
                matched_terms: Vec::new(),
                expanded_terms: vec!["semantic:window".into()],
                passages: Vec::new(),
                confidence_hint: Confidence::Related,
                ranking_tier: 0,
                direct_tier: 0,
            })
            .collect();
        hits.push(LaneHit {
            verse_index: 1,
            score: 1.0,
            matched_terms: Vec::new(),
            expanded_terms: vec!["semantic:verse".into()],
            passages: Vec::new(),
            confidence_hint: Confidence::Related,
            ranking_tier: 0,
            direct_tier: 0,
        });
        hits.push(LaneHit {
            verse_index: 1,
            score: 1.0,
            matched_terms: Vec::new(),
            expanded_terms: vec!["semantic:study-note".into()],
            passages: Vec::new(),
            confidence_hint: Confidence::Related,
            ranking_tier: 0,
            direct_tier: 0,
        });

        attach_dense_window_passages(&mut hits, &verses);

        let ranges: Vec<Vec<(&str, &str)>> = hits
            .iter()
            .map(|hit| {
                hit.passages
                    .iter()
                    .map(|range| (range.start_ref.as_str(), range.end_ref.as_str()))
                    .collect()
            })
            .collect();
        assert_eq!(
            ranges,
            vec![
                vec![("Genesis 1:31", "Genesis 1:31")],
                vec![("Genesis 2:1", "Genesis 2:2")],
                vec![("Genesis 2:1", "Genesis 2:2")],
                vec![("Exodus 1:1", "Exodus 1:1")],
                vec![],
                vec![],
            ]
        );
    }

    const SEGMENT_SPACE_ID: [u8; 32] = [0x6d; 32];

    fn synthetic_result(
        verse_index: u32,
        confidence: Confidence,
        passage_id: Option<&str>,
    ) -> crate::result::SearchResult {
        crate::result::SearchResult {
            reference: format!("document-{verse_index}"),
            text: format!("synthetic content {verse_index}"),
            score: 1.0,
            confidence,
            lanes: crate::lanes::LaneScores::default(),
            matched_terms: Vec::new(),
            expanded_terms: Vec::new(),
            passages: passage_id
                .map(|id| crate::result::PassageRange {
                    start_ref: format!("{id}-start"),
                    end_ref: format!("{id}-end"),
                })
                .into_iter()
                .collect(),
            chips: Vec::new(),
            verse_index,
        }
    }

    fn with_semantic_scores(
        mut results: Vec<crate::result::SearchResult>,
        scores: &[f32],
    ) -> Vec<crate::result::SearchResult> {
        assert_eq!(results.len(), scores.len());
        for (result, score) in results.iter_mut().zip(scores) {
            result.lanes.c = *score;
        }
        results
    }

    /// Distinct public scores so `unique_ordinal_head` can treat a clause as
    /// introducing even when the decisive/authority flag is false.
    fn with_public_scores(
        mut results: Vec<crate::result::SearchResult>,
        scores: &[f32],
    ) -> Vec<crate::result::SearchResult> {
        assert_eq!(results.len(), scores.len());
        for (result, score) in results.iter_mut().zip(scores) {
            result.score = *score;
            result.lanes.c = *score;
        }
        results
    }

    fn fixture_segment_semantic_pack() -> Vec<u8> {
        let verse_count = 14usize;
        let dims = 3usize;
        let mut table = vec![0i8; verse_count * dims];
        // Three independent semantic intents in the fixture. Every other row
        // is deliberately orthogonal so each vector has one unique head.
        table[0..3].copy_from_slice(&[127, 0, 0]); // John 3:16
        table[2 * dims..3 * dims].copy_from_slice(&[0, 0, 127]); // Psalm 23:1
        table[5 * dims..6 * dims].copy_from_slice(&[0, 127, 0]); // Genesis 1:1
        let body = DenseLane::encode_v2(dims, verse_count, &[&table], SEGMENT_SPACE_ID)
            .expect("encode segment fixture");
        PackWriter::new()
            .add_meta(&MetaSection::new("BSB", verse_count as u32))
            .add_raw(SectionTag::Lvec, body)
            .finish()
    }

    #[test]
    fn segment_composition_uses_marginal_coverage_without_repeating_a_passage() {
        let mut head = synthetic_result(10, Confidence::Recovered, None);
        head.reference = "Genesis 1:1".into();
        head.passages.push(PassageRange {
            start_ref: "Genesis 1:1".into(),
            end_ref: "Genesis 1:3".into(),
        });
        let other_intent = synthetic_result(20, Confidence::Related, None);
        let mut duplicate_anchor = synthetic_result(11, Confidence::Related, None);
        duplicate_anchor.reference = "Genesis 1:2".into();
        duplicate_anchor.passages.push(PassageRange {
            start_ref: "Genesis 1:2".into(),
            end_ref: "Genesis 1:4".into(),
        });
        let alternate = synthetic_result(12, Confidence::Related, None);
        let aggregate_tail = synthetic_result(30, Confidence::Related, None);
        let whole = vec![head.clone(), aggregate_tail];
        let segments = vec![
            (
                "clause-a".to_string(),
                with_semantic_scores(
                    vec![head.clone(), duplicate_anchor.clone(), alternate.clone()],
                    &[0.95, 0.75, 0.60],
                ),
            ),
            (
                "clause-b".to_string(),
                with_semantic_scores(
                    vec![
                        other_intent.clone(),
                        synthetic_result(21, Confidence::Related, None),
                    ],
                    &[0.90, 0.20],
                ),
            ),
        ];

        let composed = diversify_segment_results(
            &whole,
            &segments,
            &[true, true],
            &[BoundaryRelation::Additive],
            true,
            4,
        );
        let indices: Vec<u32> = composed.iter().map(|result| result.verse_index).collect();
        assert_eq!(indices[0], head.verse_index);
        assert!(indices[..3].contains(&other_intent.verse_index));
        assert!(!indices.contains(&duplicate_anchor.verse_index));
        assert_eq!(composed[0].confidence, Confidence::Recovered);
        assert!(composed
            .iter()
            .skip(1)
            .all(|result| result.confidence == Confidence::Related));
        assert!(composed.iter().skip(1).any(|result| result
            .expanded_terms
            .iter()
            .any(|term| term == "multi-intent-segment")));
    }

    #[test]
    fn segment_secondary_coverage_is_monotonic_at_the_exact_head() {
        let head = synthetic_result(1, Confidence::Recovered, None);
        let whole = vec![
            head.clone(),
            synthetic_result(2, Confidence::Related, None),
            synthetic_result(3, Confidence::Related, None),
        ];
        let segments = vec![
            (
                "clause-a".to_string(),
                vec![
                    head.clone(),
                    synthetic_result(4, Confidence::Recovered, None),
                ],
            ),
            (
                "clause-b".to_string(),
                vec![
                    synthetic_result(5, Confidence::Recovered, None),
                    synthetic_result(6, Confidence::Recovered, None),
                ],
            ),
        ];

        for depth in 1..=6 {
            let composed = diversify_segment_results(
                &whole,
                &segments,
                &[true, true],
                &[BoundaryRelation::Additive],
                true,
                depth,
            );
            assert_eq!(composed[0].verse_index, head.verse_index);
            assert_eq!(composed[0].confidence, Confidence::Recovered);
            assert_eq!(composed[0].score, 1.0);
            if depth >= 3 {
                let indices: Vec<u32> = composed.iter().map(|result| result.verse_index).collect();
                assert_eq!(&indices[..3], &[1, 4, 5]);
            }
            assert!(composed
                .iter()
                .skip(1)
                .all(|result| result.confidence == Confidence::Related));
        }
    }

    #[test]
    fn non_direct_whole_head_competes_but_direct_head_is_protected() {
        let whole_head = synthetic_result(100, Confidence::Related, None);
        let whole = vec![
            whole_head.clone(),
            synthetic_result(101, Confidence::Related, None),
            synthetic_result(102, Confidence::Related, None),
        ];
        let segments = vec![
            (
                "first view".to_string(),
                with_semantic_scores(
                    vec![
                        synthetic_result(10, Confidence::Related, None),
                        synthetic_result(11, Confidence::Related, None),
                    ],
                    &[0.95, 0.10],
                ),
            ),
            (
                "second view".to_string(),
                with_semantic_scores(
                    vec![
                        synthetic_result(20, Confidence::Related, None),
                        synthetic_result(21, Confidence::Related, None),
                    ],
                    &[0.90, 0.10],
                ),
            ),
        ];

        let composed = diversify_segment_results(
            &whole,
            &segments,
            &[true, true],
            &[BoundaryRelation::Additive],
            false,
            3,
        );
        assert_ne!(composed[0].verse_index, whole_head.verse_index);
        assert!(composed[..2].iter().any(|result| result.verse_index == 10));
        assert!(composed[..2].iter().any(|result| result.verse_index == 20));

        let protected = diversify_segment_results(
            &whole,
            &segments,
            &[true, true],
            &[BoundaryRelation::Additive],
            true,
            3,
        );
        assert_eq!(protected[0].verse_index, whole_head.verse_index);
    }

    #[test]
    fn shared_hub_does_not_collapse_hard_groups_and_flat_view_is_inert() {
        let whole = vec![
            synthetic_result(10, Confidence::Related, None),
            synthetic_result(1, Confidence::Related, None),
            synthetic_result(2, Confidence::Related, None),
            synthetic_result(3, Confidence::Related, None),
        ];
        let redundant_a = with_semantic_scores(
            vec![
                synthetic_result(10, Confidence::Related, None),
                synthetic_result(11, Confidence::Related, None),
            ],
            &[0.90, 0.20],
        );
        let redundant_b = with_semantic_scores(
            vec![
                synthetic_result(10, Confidence::Related, None),
                synthetic_result(12, Confidence::Related, None),
            ],
            &[0.85, 0.15],
        );
        let flat = with_semantic_scores(
            vec![
                synthetic_result(90, Confidence::Related, None),
                synthetic_result(91, Confidence::Related, None),
                synthetic_result(92, Confidence::Related, None),
            ],
            &[0.50, 0.50, 0.50],
        );
        let segments = vec![
            ("same view one".to_string(), redundant_a),
            ("same view two".to_string(), redundant_b),
            ("flat unresolved view".to_string(), flat),
        ];
        let composed = diversify_segment_results(
            &whole,
            &segments,
            &[true, true, false],
            &[BoundaryRelation::Additive, BoundaryRelation::Additive],
            false,
            4,
        );
        let indices: Vec<u32> = composed.iter().map(|result| result.verse_index).collect();
        assert!(indices[..2].contains(&11));
        assert!(indices[..2].contains(&12));
        assert_eq!(indices[2], 10);
        assert!(indices.iter().all(|index| !matches!(index, 90..=92)));
    }

    #[test]
    fn alternatives_share_one_obligation_and_soft_only_plans_do_not_diversify() {
        let whole = vec![synthetic_result(90, Confidence::Related, None)];
        let segments = vec![
            (
                "first alternative".to_string(),
                vec![synthetic_result(10, Confidence::Related, None)],
            ),
            (
                "second alternative".to_string(),
                vec![synthetic_result(20, Confidence::Related, None)],
            ),
            (
                "independent memory".to_string(),
                vec![synthetic_result(30, Confidence::Related, None)],
            ),
        ];
        let composed = diversify_segment_results(
            &whole,
            &segments,
            &[true, true, true],
            &[BoundaryRelation::Alternative, BoundaryRelation::Additive],
            false,
            3,
        );
        let indices: Vec<u32> = composed.iter().map(|result| result.verse_index).collect();
        assert_eq!(&indices[..2], &[10, 30]);
        assert!(!indices[..2].contains(&20));

        let unchanged = diversify_segment_results(
            &whole,
            &segments[..2],
            &[true, true],
            &[BoundaryRelation::Alternative],
            false,
            3,
        );
        assert_eq!(
            unchanged
                .iter()
                .map(|result| result.verse_index)
                .collect::<Vec<_>>(),
            [90]
        );
    }

    #[test]
    fn subordinate_and_contrast_views_refine_but_cannot_introduce() {
        for soft_relation in [BoundaryRelation::Subordinate, BoundaryRelation::Contrast] {
            let whole = vec![synthetic_result(90, Confidence::Related, None)];
            let main = synthetic_result(10, Confidence::Related, None);
            let soft_only = synthetic_result(20, Confidence::Related, None);
            let independent = synthetic_result(30, Confidence::Related, None);
            let segments = vec![
                (
                    "main memory".to_string(),
                    vec![
                        main.clone(),
                        synthetic_result(11, Confidence::Related, None),
                    ],
                ),
                (
                    "refining memory".to_string(),
                    vec![soft_only.clone(), main.clone()],
                ),
                ("independent memory".to_string(), vec![independent.clone()]),
            ];
            let composed = diversify_segment_results(
                &whole,
                &segments,
                &[true, true, true],
                &[soft_relation, BoundaryRelation::Additive],
                false,
                4,
            );
            let indices: Vec<u32> = composed.iter().map(|result| result.verse_index).collect();
            assert_eq!(&indices[..2], &[10, 30], "{soft_relation:?}");
            assert!(!indices.contains(&20), "{soft_relation:?}");
        }
    }

    #[test]
    fn equal_representatives_follow_query_order_not_canonical_index() {
        let segments = vec![
            (
                "first memory".to_string(),
                vec![synthetic_result(100, Confidence::Related, None)],
            ),
            (
                "second memory".to_string(),
                vec![synthetic_result(1, Confidence::Related, None)],
            ),
        ];
        let composed = diversify_segment_results(
            &[],
            &segments,
            &[true, true],
            &[BoundaryRelation::Additive],
            false,
            2,
        );
        assert_eq!(
            composed
                .iter()
                .map(|result| result.verse_index)
                .collect::<Vec<_>>(),
            [100, 1]
        );
    }

    #[test]
    fn decisive_view_is_an_exact_ordinal_tie_decision() {
        let mut head = synthetic_result(1, Confidence::Related, None);
        let mut tied = synthetic_result(2, Confidence::Related, None);
        head.score = 0.5;
        tied.score = 0.5;
        assert_eq!(unique_ordinal_head(&[head.clone(), tied.clone()]), None);

        head.score = f32::from_bits(0.5f32.to_bits() + 1);
        assert_eq!(unique_ordinal_head(&[head, tied]), Some(1));
    }

    #[test]
    fn non_decisive_unique_heads_still_reserve_multi_intent_slots() {
        // Clause heads are marked non-decisive (no lexical authority) but each
        // list has a unique ordinal nearest neighbor. Composition must diversify
        // instead of truncating whole-query order solely for the decisive gate.
        let whole_head = synthetic_result(100, Confidence::Related, None);
        let whole = vec![
            whole_head.clone(),
            synthetic_result(101, Confidence::Related, None),
            synthetic_result(102, Confidence::Related, None),
        ];
        let segments = vec![
            (
                "first memory".to_string(),
                with_public_scores(
                    vec![
                        synthetic_result(10, Confidence::Related, None),
                        synthetic_result(11, Confidence::Related, None),
                    ],
                    &[0.92, 0.40],
                ),
            ),
            (
                "second memory".to_string(),
                with_public_scores(
                    vec![
                        synthetic_result(20, Confidence::Related, None),
                        synthetic_result(21, Confidence::Related, None),
                    ],
                    &[0.88, 0.35],
                ),
            ),
        ];
        assert!(segment_introduces_intent(&segments[0].1, false));
        assert!(segment_introduces_intent(&segments[1].1, false));

        let composed = diversify_segment_results(
            &whole,
            &segments,
            &[false, false],
            &[BoundaryRelation::Additive],
            false,
            3,
        );
        let indices: Vec<u32> = composed.iter().map(|result| result.verse_index).collect();
        assert!(
            indices[..2].contains(&10) && indices[..2].contains(&20),
            "expected both group primaries in head slots, got {indices:?}"
        );
        assert_ne!(composed[0].verse_index, whole_head.verse_index);
        assert!(composed
            .iter()
            .all(|result| result.confidence == Confidence::Related));
        assert!(composed.iter().any(|result| result
            .expanded_terms
            .iter()
            .any(|term| term == "multi-intent-segment")));
    }

    #[test]
    fn non_decisive_segment_recovered_is_demoted_to_related() {
        let whole = vec![synthetic_result(90, Confidence::Related, None)];
        let segments = vec![
            (
                "clause-a".to_string(),
                with_public_scores(
                    vec![
                        synthetic_result(10, Confidence::Recovered, None),
                        synthetic_result(11, Confidence::Related, None),
                    ],
                    &[0.95, 0.20],
                ),
            ),
            (
                "clause-b".to_string(),
                with_public_scores(
                    vec![
                        synthetic_result(20, Confidence::Recovered, None),
                        synthetic_result(21, Confidence::Related, None),
                    ],
                    &[0.90, 0.20],
                ),
            ),
        ];
        let composed = diversify_segment_results(
            &whole,
            &segments,
            &[false, false],
            &[BoundaryRelation::Additive],
            false,
            3,
        );
        assert!(composed[..2].iter().any(|r| r.verse_index == 10));
        assert!(composed[..2].iter().any(|r| r.verse_index == 20));
        assert!(
            composed
                .iter()
                .all(|result| result.confidence == Confidence::Related),
            "segment merge must not mint Recovered: {:?}",
            composed
                .iter()
                .map(|r| (r.verse_index, r.confidence))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn fewer_than_two_introducing_groups_keeps_whole_query_order() {
        let whole = vec![
            synthetic_result(90, Confidence::Related, None),
            synthetic_result(91, Confidence::Related, None),
        ];
        // Only one hard group introduces: second segment is flat/tied.
        let segments = vec![
            (
                "clear memory".to_string(),
                with_public_scores(
                    vec![
                        synthetic_result(10, Confidence::Related, None),
                        synthetic_result(11, Confidence::Related, None),
                    ],
                    &[0.9, 0.2],
                ),
            ),
            (
                "flat unresolved".to_string(),
                with_public_scores(
                    vec![
                        synthetic_result(20, Confidence::Related, None),
                        synthetic_result(21, Confidence::Related, None),
                    ],
                    &[0.5, 0.5],
                ),
            ),
        ];
        assert!(!segment_introduces_intent(&segments[1].1, false));
        let composed = diversify_segment_results(
            &whole,
            &segments,
            &[false, false],
            &[BoundaryRelation::Additive],
            false,
            2,
        );
        assert_eq!(
            composed
                .iter()
                .map(|result| result.verse_index)
                .collect::<Vec<_>>(),
            [90, 91]
        );

        // Single-segment plans never diversify.
        let single = diversify_segment_results(
            &whole,
            &segments[..1],
            &[true],
            &[],
            false,
            2,
        );
        assert_eq!(
            single
                .iter()
                .map(|result| result.verse_index)
                .collect::<Vec<_>>(),
            [90, 91]
        );
    }

    #[test]
    fn segment_composition_has_stable_top_k_prefixes() {
        let whole = (1..=12)
            .map(|index| synthetic_result(index, Confidence::Related, None))
            .collect::<Vec<_>>();
        let segments = vec![
            (
                "first view".to_string(),
                with_semantic_scores(
                    vec![
                        synthetic_result(30, Confidence::Related, None),
                        synthetic_result(31, Confidence::Related, None),
                        synthetic_result(32, Confidence::Related, None),
                    ],
                    &[0.9, 0.6, 0.2],
                ),
            ),
            (
                "second view".to_string(),
                with_semantic_scores(
                    vec![
                        synthetic_result(40, Confidence::Related, None),
                        synthetic_result(41, Confidence::Related, None),
                        synthetic_result(42, Confidence::Related, None),
                    ],
                    &[0.85, 0.5, 0.1],
                ),
            ),
        ];
        let full = diversify_segment_results(
            &whole,
            &segments,
            &[true, true],
            &[BoundaryRelation::Additive],
            false,
            10,
        );
        for depth in 1..=10 {
            let prefix = diversify_segment_results(
                &whole,
                &segments,
                &[true, true],
                &[BoundaryRelation::Additive],
                false,
                depth,
            );
            assert_eq!(
                prefix
                    .iter()
                    .map(|result| (result.verse_index, result.score))
                    .collect::<Vec<_>>(),
                full.iter()
                    .take(depth)
                    .map(|result| (result.verse_index, result.score))
                    .collect::<Vec<_>>()
            );
        }
    }

    fn fixture_overlay(canonical_verse_count: usize) -> Vec<u8> {
        fixture_overlay_with_meta(canonical_verse_count, "OVERLAY:BSB", canonical_verse_count)
    }

    fn fixture_overlay_with_meta(
        canonical_verse_count: usize,
        meta_translation: &str,
        meta_verse_count: usize,
    ) -> Vec<u8> {
        let jsonl = format!(
            concat!(
                "{{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":{},\"concept_sources\":[{{\"concept_type\":\"curated\",\"record_count\":1,\"alias_count\":0}},{{\"concept_type\":\"event\",\"record_count\":1}},{{\"concept_type\":\"topic\",\"record_count\":4,\"generated_alias_count\":0}}],\"alternate_sources\":[{{\"translation\":\"KJV\",\"matched_verse_count\":1}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event\",\"key\":\"the fall\",\"label\":\"The Fall\",\"aliases\":[],\"source\":\"BSB.events\",\"anchor_ref\":\"Genesis 3:1\",\"target_ranges\":[{{\"start_ref\":\"Genesis 3:1\",\"end_ref\":\"Genesis 3:1\"}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"topic\",\"key\":\"animal molesters\",\"label\":\"Animal molesters\",\"aliases\":[],\"source\":\"Selah.test\",\"anchor_ref\":\"John 3:16\",\"target_ranges\":[{{\"start_ref\":\"John 3:16\",\"end_ref\":\"John 3:16\"}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"topic\",\"key\":\"god loved world\",\"label\":\"God loved world\",\"aliases\":[],\"source\":\"Selah.test\",\"anchor_ref\":\"John 3:16\",\"target_ranges\":[{{\"start_ref\":\"John 3:16\",\"end_ref\":\"John 3:16\"}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"topic\",\"key\":\"valley death story\",\"label\":\"Valley death story\",\"aliases\":[],\"source\":\"Selah.test\",\"anchor_ref\":\"Psalm 23:1\",\"target_ranges\":[{{\"start_ref\":\"Psalm 23:1\",\"end_ref\":\"Psalm 23:4\"}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"curated\",\"key\":\"curated valley story\",\"label\":\"Curated valley story\",\"aliases\":[],\"source\":\"curated.test\",\"anchor_ref\":\"Psalm 23:4\",\"target_ranges\":[{{\"start_ref\":\"Psalm 23:4\",\"end_ref\":\"Psalm 23:4\"}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"topic\",\"key\":\"curated valley story\",\"label\":\"Curated valley story\",\"aliases\":[],\"source\":\"Selah.test\",\"anchor_ref\":\"Psalm 23:1\",\"target_ranges\":[{{\"start_ref\":\"Psalm 23:1\",\"end_ref\":\"Psalm 23:4\"}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",\"target_ref\":\"Hebrews 11:1\",\"text\":\"Now faith is the substance of things hoped for, the evidence of things not seen.\"}}\n"
            ),
            canonical_verse_count
        );
        let meta = MetaSection::new(meta_translation, meta_verse_count as u32);
        PackWriter::new()
            .add_meta(&meta)
            .add_raw(SectionTag::Phrase, jsonl.into_bytes())
            .finish()
    }

    fn fixture_canonical_phrase_section(translation: &str) -> Vec<u8> {
        let meta = MetaSection::new(translation, 14);
        PackWriter::new()
            .add_meta(&meta)
            .add_raw(SectionTag::Phrase, vec![0, 1, 2, 3])
            .finish()
    }

    fn fixture_unknown_concept_overlay(canonical_verse_count: usize) -> Vec<u8> {
        let jsonl = format!(
            concat!(
                "{{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":{},\"concept_sources\":[{{\"concept_type\":\"topci\",\"record_count\":1}}],\"alternate_sources\":[]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"topci\",\"key\":\"the fall\",\"label\":\"The Fall\",\"aliases\":[],\"source\":\"typo\",\"anchor_ref\":\"Genesis 3:1\",\"target_ranges\":[{{\"start_ref\":\"Genesis 3:1\",\"end_ref\":\"Genesis 3:1\"}}]}}\n"
            ),
            canonical_verse_count
        );
        let meta = MetaSection::new("OVERLAY:BSB", canonical_verse_count as u32);
        PackWriter::new()
            .add_meta(&meta)
            .add_raw(SectionTag::Phrase, jsonl.into_bytes())
            .finish()
    }

    fn fixture_graph_shadow_overlay(canonical_verse_count: usize) -> Vec<u8> {
        let jsonl = format!(
            concat!(
                "{{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":{},\"concept_sources\":[{{\"concept_type\":\"person\",\"record_count\":1,\"alias_count\":1}},{{\"concept_type\":\"event_shadow\",\"record_count\":1,\"alias_count\":0}}],\"alternate_sources\":[]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"person\",\"key\":\"shepherd\",\"label\":\"Shepherd\",\"aliases\":[\"Pastor\"],\"source\":\"Theographic.test\",\"anchor_ref\":\"Psalm 23:1\",\"target_ranges\":[{{\"start_ref\":\"Psalm 23:1\",\"end_ref\":\"Psalm 23:4\"}},{{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:1\"}}]}}\n",
                "{{\"schema_version\":1,\"kind\":\"concept\",\"concept_type\":\"event_shadow\",\"key\":\"creation of all things\",\"label\":\"Creation of all things\",\"aliases\":[],\"source\":\"Theographic.test\",\"anchor_ref\":\"Genesis 1:1\",\"target_ranges\":[{{\"start_ref\":\"Genesis 1:1\",\"end_ref\":\"Genesis 1:1\"}}]}}\n"
            ),
            canonical_verse_count
        );
        let meta = MetaSection::new("OVERLAY:BSB", canonical_verse_count as u32);
        PackWriter::new()
            .add_meta(&meta)
            .add_raw(SectionTag::Phrase, jsonl.into_bytes())
            .finish()
    }

    struct DepthSensitiveLane;

    impl Lane for DepthSensitiveLane {
        fn id(&self) -> LaneId {
            LaneId::A
        }

        fn available(&self) -> bool {
            true
        }

        fn search(&self, _query: &NormalizedQuery, candidate_depth: usize) -> Vec<LaneHit> {
            let verse_index = if candidate_depth < 10 { 0 } else { 1 };
            vec![LaneHit {
                verse_index,
                score: 1.0,
                matched_terms: vec!["depth".into()],
                expanded_terms: Vec::new(),
                passages: Vec::new(),
                confidence_hint: Confidence::Related,
                ranking_tier: 0,
                direct_tier: 0,
            }]
        }
    }

    #[test]
    fn output_depth_does_not_change_candidate_generation() {
        let verses = vec![
            Verse {
                index: 0,
                reference: "Genesis 1:1".into(),
                text: "First".into(),
            },
            Verse {
                index: 1,
                reference: "Genesis 1:2".into(),
                text: "Second".into(),
            },
        ];
        let eng = Engine {
            verses: verses.into(),
            translation_id: "BSB".into(),
            weights: FuseWeights::default(),
            lanes: vec![Box::new(DepthSensitiveLane)],
            context_lane: None,
            dense_lane: None,
            passage_dense_lane: None,
        };

        let shallow = eng.search("depth", &SearchOpts::default().with_top_k(1));
        let deep = eng.search("depth", &SearchOpts::default().with_top_k(50));
        assert_eq!(shallow.results[0].reference, deep.results[0].reference);
    }

    #[test]
    fn real_search_results_are_prefix_invariant_across_output_depths() {
        let pack = fixture_pack();
        let overlay = fixture_overlay(14);
        let engine = Engine::open(&[&pack, &overlay]).expect("open fixture packs");

        for query in [
            "lost sheep shepherd psalm",
            "the fall",
            "faith things hoped",
        ] {
            let deep = engine.search(query, &SearchOpts::default().with_top_k(50));
            for depth in [1, 3, 10] {
                let shallow = engine.search(query, &SearchOpts::default().with_top_k(depth));
                let expected: Vec<&str> = deep
                    .results
                    .iter()
                    .take(depth)
                    .map(|result| result.reference.as_str())
                    .collect();
                let actual: Vec<&str> = shallow
                    .results
                    .iter()
                    .map(|result| result.reference.as_str())
                    .collect();
                assert_eq!(actual, expected, "query={query:?}, depth={depth}");
            }
        }
    }

    #[test]
    fn text_only_finds_lost_sheep() {
        let pack = fixture_pack();
        let eng = Engine::open(&[&pack]).expect("open");
        assert_eq!(eng.translation_id(), "BSB");
        assert!(eng.available_lanes().contains(&LaneId::A));
        assert_eq!(eng.available_lanes(), vec![LaneId::A]);

        let resp = eng.search("lost sheep", &SearchOpts::default().with_top_k(3));
        assert!(!resp.results.is_empty());
        assert_eq!(resp.results[0].reference, "Luke 15:4");
        assert!(resp.results[0]
            .matched_terms
            .iter()
            .any(|t| t == "sheep" || t == "lose"));
    }

    #[test]
    fn semantic_pack_adds_a_real_lane_c_candidate() {
        let pack = fixture_pack();
        let semantic = fixture_semantic_pack(5);
        let engine = Engine::open(&[&pack, &semantic]).expect("open semantic overlay");
        assert!(engine.available_lanes().contains(&LaneId::C));

        let response = engine
            .search_with_embedding(
                "world originated by divine action",
                &[1.0, 0.0],
                &SearchOpts::default().with_top_k(3),
            )
            .expect("valid embedding");
        let creation = response
            .results
            .iter()
            .find(|result| result.reference == "Genesis 1:1")
            .expect("dense target is recovered in the top three");
        assert!(creation.lanes.c > 0.0);
    }

    #[test]
    fn semantic_evidence_cannot_displace_a_strong_exact_recovery() {
        let pack = fixture_pack();
        let semantic = fixture_semantic_pack(5);
        let engine = Engine::open(&[&pack, &semantic]).expect("open semantic overlay");
        let opts = SearchOpts::default().with_top_k(3);
        let lexical = engine.search("the Lord is my shepherd", &opts);
        let dense = engine
            .search_with_embedding("the Lord is my shepherd", &[1.0, 0.0], &opts)
            .expect("valid embedding");

        assert_eq!(lexical.results[0].reference, "Psalm 23:1");
        assert_eq!(dense.results[0].reference, lexical.results[0].reference);
        assert_eq!(dense.results[0].confidence, lexical.results[0].confidence);

        let deep = engine
            .search_with_embedding(
                "the Lord is my shepherd",
                &[1.0, 0.0],
                &SearchOpts::default().with_top_k(10),
            )
            .expect("valid embedding");
        let semantic_tail = deep
            .results
            .iter()
            .position(|result| result.reference == "Genesis 1:1")
            .expect("semantic alternative remains in the bounded tail");
        assert!(semantic_tail >= 1);
        assert!(semantic_tail < 10);
    }

    #[test]
    fn heuristic_topic_provider_cannot_replace_the_dense_semantic_ordering() {
        let pack = fixture_pack();
        let overlay = fixture_overlay(14);
        let semantic = fixture_semantic_pack(5);
        let engine =
            Engine::open(&[&pack, &overlay, &semantic]).expect("open all semantic providers");

        // The overlay has an exact Selah topic surface for this query pointing
        // at John 3:16, while the synthetic LVEC query points at Genesis 1:1.
        // In semantic mode a heuristic provider that merely shares Lane C
        // must not masquerade as part of the encoder's ranked list.
        let response = engine
            .search_with_embedding(
                "god loved world",
                &[1.0, 0.0],
                &SearchOpts::default().with_top_k(3),
            )
            .expect("valid embedding");
        assert_eq!(response.results[0].reference, "Genesis 1:1");
        assert!(response.results[0]
            .expanded_terms
            .iter()
            .any(|term| term.starts_with("semantic:")));
        assert!(!response.results[0]
            .expanded_terms
            .iter()
            .any(|term| term.starts_with("semantic-topic:")));

        let deep = engine
            .search_with_embedding(
                "god loved world",
                &[1.0, 0.0],
                &SearchOpts::default().with_top_k(10),
            )
            .expect("valid embedding");
        let exact_topic = deep
            .results
            .iter()
            .find(|result| result.reference == "John 3:16")
            .expect("independently corroborated exact topic remains in the lexical reserve");
        assert_eq!(exact_topic.confidence, Confidence::Related);
        assert!(exact_topic
            .expanded_terms
            .iter()
            .any(|term| term == "topic:God loved world"));
    }

    #[test]
    fn meaning_only_and_situation_tot() {
        let pack = fixture_pack();
        let eng = Engine::open(&[&pack]).unwrap();
        let opts = SearchOpts::default().with_top_k(3);

        let meaning = eng.search(
            "the parable where a guy leaves ninety nine sheep to find one",
            &opts,
        );
        assert_eq!(meaning.results[0].reference, "Luke 15:4");

        let situation = eng.search(
            "verse about God loving the whole world and giving his son",
            &opts,
        );
        assert_eq!(situation.results[0].reference, "John 3:16");

        let armour = eng.search("Put on the whole armour of God", &opts);
        assert_eq!(armour.results[0].reference, "Ephesians 6:11");
    }

    #[test]
    fn multi_intent_resolution_is_generic_and_evidence_gated() {
        let split =
            plan_query_composition("be strong and courageous do not fear because i am with you");
        assert_eq!(
            split.segments,
            [
                "be strong and courageous",
                "do not fear because i am with you"
            ]
        );
        assert_eq!(split.relations, [BoundaryRelation::Contrast]);
        assert!(plan_query_composition("love and kindness")
            .segments
            .is_empty());

        let pack = fixture_pack();
        let overlay = fixture_overlay(14);
        let engine = Engine::open(&[&pack, &overlay]).expect("open fixture packs");
        let response = engine.search(
            "God loved world and the fall",
            &SearchOpts::default().with_top_k(3),
        );
        let references: Vec<&str> = response
            .results
            .iter()
            .map(|result| result.reference.as_str())
            .collect();
        assert!(references.contains(&"John 3:16"));
        assert!(
            references.contains(&"Genesis 3:1"),
            "unexpected result set: {references:?}"
        );
        assert!(response.results.iter().any(|result| {
            result
                .expanded_terms
                .iter()
                .any(|term| term.starts_with("multi-intent-clause:"))
        }));
    }

    #[test]
    fn canonical_planner_is_utf8_safe_and_preserves_negation() {
        let unicode =
            plan_query_composition("İlyas spoke with courage and created the heavens above");
        assert_eq!(unicode.segments[0], "İlyas spoke with courage");
        assert_eq!(unicode.segments[1], "created the heavens above");

        let contrast =
            plan_query_composition("stand firm with courage DO NOT fear those before you");
        assert_eq!(contrast.segments[1], "DO NOT fear those before you");
        assert_eq!(contrast.relations, [BoundaryRelation::Contrast]);
    }

    #[test]
    fn segment_embeddings_require_canonical_order_and_recover_two_intents() {
        let pack = fixture_pack();
        let semantic = fixture_segment_semantic_pack();
        let engine = Engine::open(&[&pack, &semantic]).expect("open segment fixture");
        let opts = SearchOpts::default().with_top_k(4);
        let whole = [1.0, 1.0, 0.0];
        let love = [1.0, 0.0, 0.0];
        let creation = [0.0, 1.0, 0.0];
        let forward = [
            QuerySegmentEmbedding {
                text: "a father loving the whole world",
                embedding: &love,
            },
            QuerySegmentEmbedding {
                text: "the universe comes into existence",
                embedding: &creation,
            },
        ];
        let reverse = [forward[1], forward[0]];
        let query = "a father loving the whole world and the universe comes into existence";

        let first = engine
            .search_with_segment_embeddings_in_space(
                query,
                &SEGMENT_SPACE_ID,
                &whole,
                &forward,
                &opts,
            )
            .expect("valid segment search");
        let error = engine
            .search_with_segment_embeddings_in_space(
                query,
                &SEGMENT_SPACE_ID,
                &whole,
                &reverse,
                &opts,
            )
            .expect_err("reordered clauses must fail closed");
        assert!(error.to_string().contains("canonical ordered"));

        let first_refs: Vec<&str> = first
            .results
            .iter()
            .map(|result| result.reference.as_str())
            .collect();
        assert!(first_refs[..2].contains(&"John 3:16"));
        assert!(first_refs[..2].contains(&"Genesis 1:1"));
        assert!(first
            .results
            .iter()
            .take(2)
            .all(|result| result.confidence == Confidence::Related));
        assert!(first.results[1]
            .expanded_terms
            .iter()
            .any(|term| term == "multi-intent-segment"));
    }

    #[test]
    fn existing_embedding_api_retains_its_whole_query_behavior() {
        let pack = fixture_pack();
        let semantic = fixture_segment_semantic_pack();
        let engine = Engine::open(&[&pack, &semantic]).expect("open segment fixture");
        let response = engine
            .search_with_embedding_in_space(
                "the universe comes into existence",
                &SEGMENT_SPACE_ID,
                &[0.0, 1.0, 0.0],
                &SearchOpts::default().with_top_k(3),
            )
            .expect("existing whole-query API remains valid");
        assert_eq!(response.results[0].reference, "Genesis 1:1");
        assert!(response.results[0].lanes.c > 0.0);
    }

    #[test]
    fn segment_composition_protects_a_direct_whole_query_head() {
        let pack = fixture_pack();
        let semantic = fixture_segment_semantic_pack();
        let engine = Engine::open(&[&pack, &semantic]).expect("open segment fixture");
        let opts = SearchOpts::default().with_top_k(4);
        let whole = [0.0, 0.0, 1.0];
        let shepherd = [0.0, 0.0, 1.0];
        let creation = [0.0, 1.0, 0.0];
        let ordinary = engine
            .search_with_embedding_in_space(
                "the Lord is my shepherd and the universe comes into existence",
                &SEGMENT_SPACE_ID,
                &whole,
                &opts,
            )
            .expect("ordinary whole-query search");
        let composed = engine
            .search_with_segment_embeddings_in_space(
                "the Lord is my shepherd and the universe comes into existence",
                &SEGMENT_SPACE_ID,
                &whole,
                &[
                    QuerySegmentEmbedding {
                        text: "the Lord is my shepherd",
                        embedding: &shepherd,
                    },
                    QuerySegmentEmbedding {
                        text: "the universe comes into existence",
                        embedding: &creation,
                    },
                ],
                &opts,
            )
            .expect("composed search");
        assert_eq!(ordinary.results[0].reference, "Psalm 23:1");
        assert_eq!(composed.results[0].reference, ordinary.results[0].reference);
        assert_eq!(
            composed.results[0].confidence,
            ordinary.results[0].confidence
        );
        assert!(composed
            .results
            .iter()
            .any(|result| result.reference == "Genesis 1:1"));
    }

    #[test]
    fn segment_embedding_contract_fails_closed() {
        let pack = fixture_pack();
        let semantic = fixture_segment_semantic_pack();
        let engine = Engine::open(&[&pack, &semantic]).expect("open segment fixture");
        let opts = SearchOpts::default();
        let vector = [1.0, 0.0, 0.0];
        let short = [1.0, 0.0];

        assert!(engine
            .search_with_segment_embeddings_in_space(
                "compound query",
                &SEGMENT_SPACE_ID,
                &vector,
                &[],
                &opts,
            )
            .is_err());
        let too_many = [
            QuerySegmentEmbedding {
                text: "first meaningful clause",
                embedding: &vector,
            },
            QuerySegmentEmbedding {
                text: "second meaningful clause",
                embedding: &vector,
            },
            QuerySegmentEmbedding {
                text: "third meaningful clause",
                embedding: &vector,
            },
            QuerySegmentEmbedding {
                text: "fourth meaningful clause",
                embedding: &vector,
            },
            QuerySegmentEmbedding {
                text: "fifth meaningful clause",
                embedding: &vector,
            },
        ];
        assert!(engine
            .search_with_segment_embeddings_in_space(
                "compound query",
                &SEGMENT_SPACE_ID,
                &vector,
                &too_many,
                &opts,
            )
            .is_err());
        assert!(engine
            .search_with_segment_embeddings_in_space(
                "compound query",
                &SEGMENT_SPACE_ID,
                &vector,
                &[QuerySegmentEmbedding {
                    text: "single",
                    embedding: &vector,
                }],
                &opts,
            )
            .is_err());
        assert!(engine
            .search_with_segment_embeddings_in_space(
                "compound query",
                &SEGMENT_SPACE_ID,
                &vector,
                &[
                    QuerySegmentEmbedding {
                        text: "loved the world",
                        embedding: &vector,
                    },
                    QuerySegmentEmbedding {
                        text: "loves the world",
                        embedding: &vector,
                    },
                ],
                &opts,
            )
            .is_err());
        assert!(engine
            .search_with_segment_embeddings_in_space(
                "compound query",
                &SEGMENT_SPACE_ID,
                &vector,
                &[QuerySegmentEmbedding {
                    text: "valid meaningful clause",
                    embedding: &short,
                }],
                &opts,
            )
            .is_err());
        assert!(engine
            .search_with_segment_embeddings_in_space(
                "compound query",
                &[0x22; 32],
                &vector,
                &[QuerySegmentEmbedding {
                    text: "valid meaningful clause",
                    embedding: &vector,
                }],
                &opts,
            )
            .is_err());
    }

    #[test]
    fn segment_embedding_contract_rejects_noncanonical_clause_texts() {
        let pack = fixture_pack();
        let semantic = fixture_segment_semantic_pack();
        let engine = Engine::open(&[&pack, &semantic]).expect("open segment fixture");
        let query = "first remembered thought and second remembered image";
        let vector = [1.0, 0.0, 0.0];

        for segments in [
            [
                QuerySegmentEmbedding {
                    text: "first remembered thought",
                    embedding: &vector,
                },
                QuerySegmentEmbedding {
                    text: "an injected unrelated clause",
                    embedding: &vector,
                },
            ],
            [
                QuerySegmentEmbedding {
                    text: " first remembered thought",
                    embedding: &vector,
                },
                QuerySegmentEmbedding {
                    text: "second remembered image",
                    embedding: &vector,
                },
            ],
            [
                QuerySegmentEmbedding {
                    text: "first remembered thought",
                    embedding: &vector,
                },
                QuerySegmentEmbedding {
                    text: "first remembered thought",
                    embedding: &vector,
                },
            ],
        ] {
            let error = engine
                .search_with_segment_embeddings_in_space(
                    query,
                    &SEGMENT_SPACE_ID,
                    &vector,
                    &segments,
                    &SearchOpts::default(),
                )
                .expect_err("noncanonical segment text must fail closed");
            assert!(error
                .to_string()
                .contains("canonical ordered query-only planner"));
        }
    }

    #[test]
    fn engine_bounds_directly_mutated_top_k_options() {
        let oversized = SearchOpts {
            top_k: usize::MAX,
            book_hint: Some("John".into()),
        };
        let bounded = bounded_search_opts(&oversized);
        assert_eq!(bounded.top_k, MAX_TOP_K);
        assert_eq!(bounded.book_hint.as_deref(), Some("John"));

        let zero = SearchOpts {
            top_k: 0,
            book_hint: None,
        };
        assert_eq!(bounded_search_opts(&zero).top_k, 1);
        assert_eq!(
            diversify_segment_results(&[], &[], &[], &[], false, usize::MAX).len(),
            0
        );
    }

    #[test]
    fn unknown_aphorism_does_not_receive_a_prompt_specific_label() {
        let pack = fixture_pack();
        let eng = Engine::open(&[&pack]).unwrap();
        let resp = eng.search(
            "God helps those who help themselves",
            &SearchOpts::default().with_top_k(3),
        );
        assert!(resp
            .results
            .iter()
            .all(|result| result.confidence != Confidence::OftenMisremembered));
    }

    #[test]
    fn canonical_text_is_not_labeled_by_a_builtin_misquote_table() {
        let pack = fixture_pack();
        let eng = Engine::open(&[&pack]).unwrap();
        let response = eng.search(
            "the love of money is a root of all kinds of evil",
            &SearchOpts::default().with_top_k(3),
        );
        assert_eq!(response.results[0].reference, "1 Timothy 6:10");
        assert!(response
            .results
            .iter()
            .all(|result| result.confidence != Confidence::OftenMisremembered));
    }

    #[test]
    fn only_lead_candidate_may_claim_recovered() {
        let pack = fixture_pack();
        let engine = Engine::open(&[&pack]).unwrap();
        let response = engine.search("armor of God", &SearchOpts::default().with_top_k(5));
        assert!(response
            .results
            .iter()
            .skip(1)
            .all(|result| result.confidence != Confidence::Recovered));
    }

    #[test]
    fn search_json_roundtrips() {
        let pack = fixture_pack();
        let eng = Engine::open(&[&pack]).unwrap();
        let json = eng
            .search_json("shepherd", &SearchOpts::default().with_top_k(2))
            .unwrap();
        assert!(json.contains("Psalm 23:1") || json.contains("shepherd"));
        let parsed: SearchResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.query, "shepherd");
    }

    #[test]
    fn overlay_recovers_event_and_hidden_translation_but_returns_bsb() {
        let pack = fixture_pack();
        let overlay = fixture_overlay(14);
        let eng = Engine::open(&[&pack, &overlay]).expect("open base with overlay");
        assert_eq!(eng.available_lanes(), vec![LaneId::A, LaneId::B, LaneId::D]);
        let opts = SearchOpts::default().with_top_k(3);

        let event = eng.search("the fall", &opts);
        assert_eq!(event.results[0].reference, "Genesis 3:1");
        assert_eq!(event.results[0].confidence, Confidence::Related);
        assert_eq!(event.results[0].passages.len(), 1);
        assert_eq!(event.results[0].passages[0].start_ref, "Genesis 3:1");
        assert!(event
            .results
            .windows(2)
            .all(|pair| pair[0].score >= pair[1].score));

        let generic = eng.search("fall", &opts);
        assert_eq!(generic.results[0].reference, "Matthew 21:44");
        assert_eq!(generic.results[0].lanes.b, 0.0);

        let alternate = eng.search("faith is the substance of things hoped for", &opts);
        assert_eq!(alternate.results[0].reference, "Hebrews 11:1");
        assert!(alternate.results[0].lanes.b > 0.0);
        assert!(alternate.results[0].text.contains("assurance"));
        assert!(!alternate.results[0].text.contains("substance"));
        assert_eq!(eng.translation_id(), "BSB");

        let noisy_topic = eng.search("animal molesters", &opts);
        assert!(
            noisy_topic.results.is_empty(),
            "a heuristic topic association must not generate a candidate"
        );

        let corroborated_topic = eng.search("god loved world", &opts);
        assert_eq!(corroborated_topic.results[0].reference, "John 3:16");
        assert_eq!(corroborated_topic.results[0].lanes.c, 0.0);
        assert!(corroborated_topic.results[0]
            .expanded_terms
            .iter()
            .any(|term| term == "topic:God loved world"));

        let ranged_topic = eng.search("valley death story", &opts);
        // Multi-verse exact topics may surface as a passage unit. Prefer the
        // structured range over any single interior lexical verse.
        assert!(
            ranged_topic.results[0].passages.iter().any(|passage| {
                passage.start_ref == "Psalm 23:1" && passage.end_ref == "Psalm 23:4"
            }),
            "expected Psalm 23:1–4 passage on {:?}",
            ranged_topic.results[0]
        );
        assert!(
            ranged_topic.results[0].reference == "Psalm 23:1"
                || ranged_topic.results[0].reference == "Psalm 23:4",
            "unexpected head {}",
            ranged_topic.results[0].reference
        );

        let curated_over_shadow = eng.search("curated valley story", &opts);
        assert_eq!(curated_over_shadow.results[0].reference, "Psalm 23:4");
        assert_eq!(curated_over_shadow.results[0].passages.len(), 1);
        assert_eq!(
            curated_over_shadow.results[0].passages[0].start_ref,
            "Psalm 23:4"
        );
        assert_eq!(
            curated_over_shadow.results[0].passages[0].end_ref,
            "Psalm 23:4"
        );
    }

    #[test]
    fn exact_canonical_phrase_outranks_conflicting_hidden_translation() {
        let text = TextSection::from_verses(vec![
            ("Genesis 1:1".into(), "Alpha beta gamma delta.".into()),
            ("Genesis 1:2".into(), "Other canonical words here.".into()),
        ]);
        let base_meta = MetaSection::new("BSB", 2);
        let base = PackWriter::new()
            .add_meta(&base_meta)
            .add_text(&text)
            .finish();
        let jsonl = concat!(
            "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":2,",
            "\"concept_sources\":[],\"alternate_sources\":[{\"translation\":\"KJV\",\"matched_verse_count\":2}]}\n",
            "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",",
            "\"target_ref\":\"Genesis 1:1\",\"text\":\"Different hidden wording here now\"}\n",
            "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",",
            "\"target_ref\":\"Genesis 1:2\",\"text\":\"Alpha beta gamma delta\"}\n"
        );
        let overlay_meta = MetaSection::new("OVERLAY:BSB", 2);
        let overlay = PackWriter::new()
            .add_meta(&overlay_meta)
            .add_raw(SectionTag::Phrase, jsonl.as_bytes().to_vec())
            .finish();

        let engine = Engine::open(&[&base, &overlay]).expect("valid synthetic packs");
        let response = engine.search(
            "alpha beta gamma delta",
            &SearchOpts::default().with_top_k(2),
        );
        assert_eq!(response.results[0].reference, "Genesis 1:1");
        assert_eq!(response.results[0].confidence, Confidence::Recovered);
        assert_eq!(response.results[1].reference, "Genesis 1:2");
    }

    #[test]
    fn recurrent_hidden_phrase_downgrades_unique_canonical_claim() {
        let text = TextSection::from_verses(vec![
            (
                "Genesis 1:1".into(),
                "Who said it and it came to pass in the canonical text?".into(),
            ),
            ("Genesis 1:2".into(), "Second canonical verse.".into()),
            ("Genesis 1:3".into(), "Third canonical verse.".into()),
            ("Genesis 1:4".into(), "Fourth canonical verse.".into()),
        ]);
        let base = PackWriter::new()
            .add_meta(&MetaSection::new("BSB", 4))
            .add_text(&text)
            .finish();
        let jsonl = concat!(
            "{\"schema_version\":1,\"kind\":\"manifest\",\"canonical_translation\":\"BSB\",\"canonical_verse_count\":4,",
            "\"concept_sources\":[],\"alternate_sources\":[{\"translation\":\"KJV\",\"matched_verse_count\":4}]}\n",
            "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",\"target_ref\":\"Genesis 1:1\",\"text\":\"And it came to pass one\"}\n",
            "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",\"target_ref\":\"Genesis 1:2\",\"text\":\"And it came to pass two\"}\n",
            "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",\"target_ref\":\"Genesis 1:3\",\"text\":\"And it came to pass three\"}\n",
            "{\"schema_version\":1,\"kind\":\"alternate_text\",\"source_translation\":\"KJV\",\"target_ref\":\"Genesis 1:4\",\"text\":\"And it came to pass four\"}\n"
        );
        let overlay = PackWriter::new()
            .add_meta(&MetaSection::new("OVERLAY:BSB", 4))
            .add_raw(SectionTag::Phrase, jsonl.as_bytes().to_vec())
            .finish();

        let engine = Engine::open(&[&base, &overlay]).expect("valid synthetic packs");
        let response = engine.search("and it came to pass", &SearchOpts::default().with_top_k(2));

        assert_eq!(response.results[0].reference, "Genesis 1:1");
        assert_eq!(response.results[0].confidence, Confidence::Related);
        assert!(response
            .results
            .iter()
            .all(|result| result.confidence != Confidence::Recovered));
    }

    #[test]
    fn overlay_manifest_must_match_canonical_pack() {
        let pack = fixture_pack();
        let overlay = fixture_overlay(13);
        let err = Engine::open(&[&pack, &overlay])
            .err()
            .expect("mismatched overlay must fail");
        assert!(matches!(err, CoreError::InvalidOverlay(_)));
        assert!(err.to_string().contains("verse count 13"));
    }

    #[test]
    fn overlay_meta_must_match_base_and_embedded_manifest() {
        let pack = fixture_pack();
        let wrong_translation = fixture_overlay_with_meta(14, "OVERLAY:KJV", 14);
        let error = Engine::open(&[&pack, &wrong_translation])
            .err()
            .expect("overlay META translation mismatch must fail");
        assert!(error.to_string().contains("META targets"));

        let wrong_count = fixture_overlay_with_meta(14, "OVERLAY:BSB", 13);
        let error = Engine::open(&[&pack, &wrong_count])
            .err()
            .expect("overlay META count mismatch must fail");
        assert!(error.to_string().contains("META verse count 13"));
    }

    #[test]
    fn base_meta_verse_count_must_match_text() {
        let text = TextSection::from_verses(vec![(
            "Genesis 1:1".into(),
            "In the beginning God created the heavens and the earth.".into(),
        )]);
        let meta = MetaSection::new("BSB", 2);
        let pack = PackWriter::new().add_meta(&meta).add_text(&text).finish();
        let error = Engine::open(&[&pack])
            .err()
            .expect("mismatched META verse count must fail");
        assert!(matches!(
            error,
            CoreError::Pack(recall_pack::PackError::InvalidMeta)
        ));
    }

    #[test]
    fn overlay_rejects_unknown_concept_types() {
        let pack = fixture_pack();
        let overlay = fixture_unknown_concept_overlay(14);
        let err = Engine::open(&[&pack, &overlay])
            .err()
            .expect("unknown concept types must fail closed");
        assert!(err.to_string().contains("unsupported concept_type topci"));
    }

    #[test]
    fn graph_entities_explain_single_token_hits_without_promoting_them() {
        let pack = fixture_pack();
        let overlay = fixture_graph_shadow_overlay(14);
        let engine = Engine::open(&[&pack, &overlay]).expect("graph overlay opens");
        let response = engine.search("shepherd", &SearchOpts::default().with_top_k(3));
        let psalm = response
            .results
            .iter()
            .find(|result| result.reference == "Psalm 23:1")
            .expect("lexical shepherd hit remains present");
        assert!(psalm
            .expanded_terms
            .iter()
            .any(|term| term == "person:Shepherd"));
        assert_eq!(psalm.confidence, Confidence::Related);
        assert_eq!(psalm.passages.len(), 1);
        assert_eq!(psalm.passages[0].start_ref, "Psalm 23:1");
        assert_eq!(psalm.passages[0].end_ref, "Psalm 23:4");
    }

    #[test]
    fn tied_results_are_deterministic() {
        let pack = fixture_pack();
        let eng = Engine::open(&[&pack]).expect("open");
        let opts = SearchOpts::default().with_top_k(5);
        let expected: Vec<String> = eng
            .search("fall", &opts)
            .results
            .into_iter()
            .map(|result| result.reference)
            .collect();

        for _ in 0..25 {
            let actual: Vec<String> = eng
                .search("fall", &opts)
                .results
                .into_iter()
                .map(|result| result.reference)
                .collect();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn canonical_phrase_section_is_not_misparsed_as_recall_overlay() {
        let text = TextSection::from_verses(vec![(
            "Genesis 1:1".into(),
            "In the beginning God created the heavens and the earth.".into(),
        )]);
        let meta = MetaSection::new("BSB", 1);
        let pack = PackWriter::new()
            .add_meta(&meta)
            .add_text(&text)
            .add_raw(SectionTag::Phrase, vec![0, 1, 2, 3])
            .finish();

        let eng = Engine::open(&[&pack]).expect("canonical PHRASE data stays compatible");
        assert_eq!(eng.verse_count(), 1);
    }

    #[test]
    fn section_only_canonical_phrase_pack_is_order_independent() {
        let base = fixture_pack();
        let phrase = fixture_canonical_phrase_section("BSB");

        Engine::open(&[&base, &phrase]).expect("base then canonical section pack");
        Engine::open(&[&phrase, &base]).expect("canonical section pack then base");

        let wrong = fixture_canonical_phrase_section("KJV");
        let err = Engine::open(&[&base, &wrong])
            .err()
            .expect("mismatched canonical section pack must fail");
        assert!(err.to_string().contains("targets KJV"));
    }
}

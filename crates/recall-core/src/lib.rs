//! Runtime engine for verse-recall.
//!
//! Pure deterministic CPU over read-only pack bytes. No filesystem I/O,
//! network, or bundled model runtime. Hosts may supply a query embedding for
//! the optional scored Lane C.

#![forbid(unsafe_code)]

mod engine;
mod error;
mod fuse;
mod lanes;
mod normalize;
mod planner;
mod result;
mod types;

pub use engine::{Engine, QuerySegmentEmbedding};
pub use error::{CoreError, CoreResult};
pub use fuse::{fuse_lanes, FuseWeights};
pub use lanes::{
    DenseLane, DenseMetadata, DenseTableRole, Lane, LaneHit, LaneId, LaneScores, PackedExpandLane,
    PackedExpansionQuery, PackedLexicalLane, PassageDenseLane, PassageDenseMetadata, RecallOverlay,
    WeightedStem,
};
pub use normalize::{normalize_query, stem, tokenize_text, NormalizedQuery, NORMALIZER_VERSION};
pub use planner::{
    plan_query_segments, QuerySegmentPlan, MAX_QUERY_SEGMENTS, QUERY_SEGMENT_PLANNER,
};
pub use result::{Confidence, PassageRange, SearchResponse, SearchResult};
pub use types::{SearchOpts, ABI_VERSION, MAX_TOP_K};

/// Re-export pack types hosts may need when constructing engines.
pub use recall_pack::{PackError, PackReader, SectionTag};

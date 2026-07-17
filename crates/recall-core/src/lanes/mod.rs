//! Retrieval lanes. Each lane is optional depending on pack sections present.

mod context;
mod dense;
mod overlay;
mod packed_expand;
mod packed_lexical;
mod passage_dense;
mod scan;
mod traits;

pub use context::ContextLexicalLane;
pub use dense::{DenseLane, DenseMetadata, DenseTableRole};
pub use overlay::RecallOverlay;
pub use packed_expand::{PackedExpandLane, PackedExpansionQuery};
pub use packed_lexical::{PackedLexicalLane, WeightedStem};
pub use passage_dense::{PassageDenseLane, PassageDenseMetadata};
pub use scan::TextScanLane;
pub use traits::{Lane, LaneHit, LaneId, LaneScores};

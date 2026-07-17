use crate::normalize::NormalizedQuery;
use crate::result::{Confidence, PassageRange};

/// Which retrieval lane produced a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LaneId {
    /// BM25 / lexical (POST+LEX), or TEXT scan fallback.
    A,
    /// Expansion re-BM25.
    B,
    /// Dense int8 cosine (WVEC/LVEC).
    C,
    /// Phrase memory.
    D,
}

impl LaneId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
            Self::C => "c",
            Self::D => "d",
        }
    }
}

/// Per-lane score bag on a fused result.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LaneScores {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
}

impl LaneScores {
    pub fn set(&mut self, id: LaneId, score: f32) {
        match id {
            LaneId::A => self.a = score,
            LaneId::B => self.b = score,
            LaneId::C => self.c = score,
            LaneId::D => self.d = score,
        }
    }

    pub fn get(&self, id: LaneId) -> f32 {
        match id {
            LaneId::A => self.a,
            LaneId::B => self.b,
            LaneId::C => self.c,
            LaneId::D => self.d,
        }
    }

    pub fn agreeing_lanes(&self) -> usize {
        [self.a, self.b, self.c, self.d]
            .iter()
            .filter(|&&s| s > 0.0)
            .count()
    }
}

/// A single lane's candidate.
#[derive(Debug, Clone)]
pub struct LaneHit {
    pub verse_index: u32,
    pub score: f32,
    pub matched_terms: Vec<String>,
    pub expanded_terms: Vec<String>,
    /// Structured passage evidence. Empty for ordinary verse matches.
    pub passages: Vec<PassageRange>,
    pub confidence_hint: Confidence,
    /// Deterministic resolver ordering used before ordinary RRF ordering.
    /// This may promote an exact event name while its uncurated provenance
    /// still limits `direct_tier` and confidence.
    pub ranking_tier: u8,
    /// Trust tier for a direct recovery claim.
    ///
    /// Zero means this is a normal ranked candidate. Non-zero is reserved for
    /// exact, data-backed mappings such as a named event or a sufficiently long
    /// contiguous phrase in an alternate translation. It is intentionally not
    /// serialized in the public result ABI.
    pub direct_tier: u8,
}

/// Per-query lane output, including evidence that is not tied to one verse.
///
/// A complete alternate translation can show that an otherwise exact-looking
/// phrase is highly recurrent. That ambiguity must lower a presentation claim
/// without inventing a candidate or changing retrieval rank.
pub struct LaneSearchOutput {
    pub hits: Vec<LaneHit>,
    pub blocks_unique_recovery: bool,
}

/// Trait every lane implements. Lanes that lack pack data return empty.
pub trait Lane: Send + Sync {
    fn id(&self) -> LaneId;
    fn available(&self) -> bool;
    /// Whether this implementation represents a scored public lane. Internal
    /// evidence-only providers run during search but stay out of lane
    /// availability/attribution.
    fn advertised(&self) -> bool {
        self.available()
    }
    fn search(&self, query: &NormalizedQuery, top_k: usize) -> Vec<LaneHit>;
    fn search_with_risk(&self, query: &NormalizedQuery, top_k: usize) -> LaneSearchOutput {
        LaneSearchOutput {
            hits: self.search(query, top_k),
            blocks_unique_recovery: false,
        }
    }
}

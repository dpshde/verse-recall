use crate::lanes::LaneScores;
use serde::{Deserialize, Serialize};

/// Confidence tier for a hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    /// Strong lexical / multi-lane agreement.
    Recovered,
    /// Plausible but weaker.
    Related,
    /// Known false memory / misquote — never present as exact recovered.
    OftenMisremembered,
}

/// A structured canonical passage attached to a concept result.
///
/// Keeping this separate from the anchor verse lets clients display an event
/// or story as a passage instead of implying that its first/salient verse is
/// the whole answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassageRange {
    pub start_ref: String,
    pub end_ref: String,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recovered => "recovered",
            Self::Related => "related",
            Self::OftenMisremembered => "often-misremembered",
        }
    }
}

/// One ranked result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    #[serde(rename = "ref")]
    pub reference: String,
    pub text: String,
    /// Opaque, response-local ordering score. Integer resolver-priority
    /// priority plus fractional RRF; do not persist or threshold across engine
    /// versions.
    pub score: f32,
    pub confidence: Confidence,
    pub lanes: LaneScores,
    pub matched_terms: Vec<String>,
    pub expanded_terms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub passages: Vec<PassageRange>,
    pub chips: Vec<String>,
    #[serde(skip)]
    pub verse_index: u32,
}

/// Full search response (JSON ABI).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
}

// Serde for LaneScores
use serde::ser::SerializeStruct;

impl Serialize for LaneScores {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut st = serializer.serialize_struct("LaneScores", 4)?;
        st.serialize_field("a", &self.a)?;
        st.serialize_field("b", &self.b)?;
        st.serialize_field("c", &self.c)?;
        st.serialize_field("d", &self.d)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for LaneScores {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            a: f32,
            #[serde(default)]
            b: f32,
            #[serde(default)]
            c: f32,
            #[serde(default)]
            d: f32,
        }
        let r = Raw::deserialize(deserializer)?;
        Ok(LaneScores {
            a: r.a,
            b: r.b,
            c: r.c,
            d: r.d,
        })
    }
}

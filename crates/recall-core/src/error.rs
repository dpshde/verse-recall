use recall_pack::PackError;

/// Engine-level errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Pack(PackError),
    EmptyPacks,
    NoTextSection,
    MultipleTextSections,
    InvalidOverlay(String),
    InvalidQueryEmbedding(String),
    Internal(&'static str),
}

impl core::fmt::Display for CoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Pack(e) => write!(f, "pack error: {e}"),
            Self::EmptyPacks => write!(f, "no pack buffers provided"),
            Self::NoTextSection => write!(f, "no TEXT section in any pack"),
            Self::MultipleTextSections => write!(
                f,
                "multiple TEXT sections; pass one canonical pack plus section-only overlays"
            ),
            Self::InvalidOverlay(m) => write!(f, "invalid recall overlay: {m}"),
            Self::InvalidQueryEmbedding(m) => write!(f, "invalid query embedding: {m}"),
            Self::Internal(m) => write!(f, "internal: {m}"),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<PackError> for CoreError {
    fn from(value: PackError) -> Self {
        Self::Pack(value)
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

use core::fmt;

/// Errors from pack read/write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    TooShort { need: usize, got: usize },
    BadMagic,
    UnsupportedVersion(u32),
    UnsupportedFlags(u32),
    UnsupportedMetaVersion(u32),
    UnsupportedLexVersion(u16),
    UnsupportedLexNormalizerVersion(u16),
    UnsupportedPostVersion(u16),
    UnsupportedPvecVersion(u16),
    UnsupportedExpandVersion(u16),
    UnsupportedExpandNormalizerVersion(u16),
    UnsupportedOverlayVersion(u16),
    BadSectionTable,
    MissingSection(&'static str),
    TruncatedSection { tag: &'static str },
    InvalidUtf8 { context: &'static str },
    InvalidMeta,
    InvalidText,
    InvalidLex,
    InvalidPost,
    InvalidPvec,
    IncompatibleLexPost,
    InvalidExpand,
    IncompatibleExpand,
    InvalidOverlay,
    Overflow,
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooShort { need, got } => {
                write!(f, "pack too short: need {need} bytes, got {got}")
            }
            Self::BadMagic => write!(f, "bad pack magic"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported format version {v}"),
            Self::UnsupportedFlags(flags) => {
                write!(f, "unsupported pack header flags {flags:#x}")
            }
            Self::UnsupportedMetaVersion(version) => {
                write!(f, "unsupported META version {version}")
            }
            Self::UnsupportedLexVersion(version) => {
                write!(f, "unsupported LEX version {version}")
            }
            Self::UnsupportedLexNormalizerVersion(version) => {
                write!(f, "unsupported LEX normalizer version {version}")
            }
            Self::UnsupportedPostVersion(version) => {
                write!(f, "unsupported POST version {version}")
            }
            Self::UnsupportedPvecVersion(version) => {
                write!(f, "unsupported PVEC version {version}")
            }
            Self::UnsupportedExpandVersion(version) => {
                write!(f, "unsupported EXPAND version {version}")
            }
            Self::UnsupportedExpandNormalizerVersion(version) => {
                write!(f, "unsupported EXPAND normalizer version {version}")
            }
            Self::UnsupportedOverlayVersion(version) => {
                write!(f, "unsupported binary overlay version {version}")
            }
            Self::BadSectionTable => write!(f, "invalid section table"),
            Self::MissingSection(t) => write!(f, "missing section {t}"),
            Self::TruncatedSection { tag } => write!(f, "truncated section {tag}"),
            Self::InvalidUtf8 { context } => write!(f, "invalid utf-8 in {context}"),
            Self::InvalidMeta => write!(f, "invalid META section"),
            Self::InvalidText => write!(f, "invalid TEXT section"),
            Self::InvalidLex => write!(f, "invalid LEX section"),
            Self::InvalidPost => write!(f, "invalid POST section"),
            Self::InvalidPvec => write!(f, "invalid PVEC section"),
            Self::IncompatibleLexPost => write!(f, "LEX and POST sections are incompatible"),
            Self::InvalidExpand => write!(f, "invalid EXPAND section"),
            Self::IncompatibleExpand => {
                write!(f, "EXPAND section targets a different canonical corpus")
            }
            Self::InvalidOverlay => write!(f, "invalid binary overlay section"),
            Self::Overflow => write!(f, "integer overflow"),
        }
    }
}

impl std::error::Error for PackError {}

pub type PackResult<T> = Result<T, PackError>;

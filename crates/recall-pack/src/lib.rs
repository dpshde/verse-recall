//! Pack format for verse-recall: little-endian, 64-byte-aligned sections.
//!
//! Runtime hosts mmap or fetch the whole file; the engine reads sections
//! zero-copy from `&[u8]`.

#![forbid(unsafe_code)]

mod error;
mod expand;
mod header;
mod lex;
mod meta;
mod overlay;
mod post;
mod pvec;
mod reader;
mod section;
mod text;
mod writer;

pub use error::{PackError, PackResult};
pub use expand::{
    ExpandEntry, ExpandSection, ExpandTarget, ExpandView, Expansions, OwnedExpand,
    WeightedExpansion, EXPAND_FORMAT_VERSION, MAX_EXPAND_DICTIONARY_TERMS, MAX_EXPAND_EDGES,
    MAX_EXPAND_ENTRIES, MAX_EXPAND_SECTION_BYTES, MAX_EXPAND_STEM_BYTES, MAX_EXPANSIONS_PER_SOURCE,
};
pub use header::{PackHeader, FORMAT_VERSION, HEADER_SIZE, MAGIC};
pub use lex::{LexSection, LexTerm, LexView, OwnedLex, LEX_NORMALIZER_VERSION};
pub use meta::MetaSection;
pub use overlay::{
    is_binary_overlay, OverlayAlternate, OverlayConcept, OverlayRange, OverlaySection,
    BINARY_OVERLAY_MAGIC,
};
pub use post::{validate_lex_post, OwnedPost, PostSection, PostTerm, PostView, Posting, Postings};
pub use pvec::{
    PvecRow, PvecSection, PvecSourceKind, PVEC_EMBEDDING_SPACE_ID_LEN, PVEC_FORMAT_VERSION,
    PVEC_HEADER_LEN, PVEC_MAGIC, PVEC_ROW_LEN,
};
pub use reader::PackReader;
pub use section::{SectionEntry, SectionTag, SECTION_ENTRY_SIZE};
pub use text::{TextSection, Verse};
pub use writer::PackWriter;

/// Align `n` up to the next multiple of `align` (power of two).
#[inline]
pub fn align_up(n: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    (n + align - 1) & !(align - 1)
}

pub const SECTION_ALIGN: usize = 64;

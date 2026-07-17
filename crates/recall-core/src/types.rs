/// FFI / API ABI version. Bump when C surface breaks.
pub const ABI_VERSION: u32 = 1;

/// Maximum number of results any public search entry point may return.
///
/// This is also an allocation-safety boundary. `SearchOpts` has public fields
/// for source compatibility, so engine entry points defensively apply the
/// same cap even when a caller bypasses the builder.
pub const MAX_TOP_K: usize = 100;

/// Search options (host-provided).
#[derive(Debug, Clone)]
pub struct SearchOpts {
    pub top_k: usize,
    /// Soft book/testament prior, e.g. "John" or "NT". Parsed loosely.
    pub book_hint: Option<String>,
}

impl Default for SearchOpts {
    fn default() -> Self {
        Self {
            top_k: 10,
            book_hint: None,
        }
    }
}

impl SearchOpts {
    pub fn with_top_k(mut self, k: usize) -> Self {
        self.top_k = k.clamp(1, MAX_TOP_K);
        self
    }

    pub fn with_book_hint(mut self, hint: impl Into<String>) -> Self {
        self.book_hint = Some(hint.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_k_builder_enforces_the_public_safety_boundary() {
        assert_eq!(SearchOpts::default().with_top_k(0).top_k, 1);
        assert_eq!(SearchOpts::default().with_top_k(MAX_TOP_K).top_k, MAX_TOP_K);
        assert_eq!(
            SearchOpts::default().with_top_k(usize::MAX).top_k,
            MAX_TOP_K
        );
    }
}

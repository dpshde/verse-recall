//! Hardened C ABI for verse-recall.
//!
//! Hosts pass pack buffers; results come back as UTF-8 JSON owned by the
//! library (free with [`recall_free_str`]). Every exported function contains
//! Rust panics before they can cross the C boundary. The additive `v2` entry
//! points also return a structured error code, while a thread-local diagnostic
//! message remains available to both legacy and v2 callers.

#![allow(non_camel_case_types, non_snake_case)]

use recall_core::{
    plan_query_segments, CoreError, Engine, QuerySegmentEmbedding, SearchOpts, ABI_VERSION,
    MAX_QUERY_SEGMENTS, MAX_TOP_K,
};
use serde_json::json;
use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_uint};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

const EMBEDDING_SPACE_ID_LEN: usize = 32;
const MAX_PACK_COUNT: usize = 1_024;

thread_local! {
    /// Most recent failure on this calling thread. Success does not erase it;
    /// callers may explicitly use `recall_clear_last_error` when desired.
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Opaque engine handle.
pub struct recall_engine {
    inner: Engine,
}

/// Error codes for the C ABI. Existing numeric values are stable.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum recall_err {
    RECALL_OK = 0,
    RECALL_ERR_NULL = 1,
    RECALL_ERR_OPEN = 2,
    RECALL_ERR_UTF8 = 3,
    RECALL_ERR_INTERNAL = 4,
    RECALL_ERR_INVALID_ARGUMENT = 5,
    RECALL_ERR_SEARCH = 6,
    RECALL_ERR_EMBEDDING = 7,
    RECALL_ERR_EMBEDDING_SPACE = 8,
    RECALL_ERR_PANIC = 9,
    RECALL_ERR_JSON = 10,
}

/// Optional search parameters. Pass NULL for defaults.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct recall_opts {
    /// Max results (0 = default 10).
    pub top_k: c_uint,
    /// Soft book prior, UTF-8 C string or NULL.
    pub book_hint: *const c_char,
    /// Reserved flags. V2 functions reject nonzero values.
    pub flags: c_uint,
}

/// One query-only clause and the host embedding produced for that exact text.
/// The pointed-to memory only needs to remain live for the search call.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct recall_query_segment {
    /// Valid NUL-terminated UTF-8 clause text.
    pub text_utf8: *const c_char,
    /// Readable, properly aligned `f32` embedding values.
    pub embedding: *const f32,
    /// Number of values at `embedding`.
    pub embedding_len: usize,
}

#[derive(Debug)]
struct FfiFailure {
    code: recall_err,
    message: String,
}

impl FfiFailure {
    fn new(code: recall_err, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// ABI version (bump on breaking C surface changes).
#[no_mangle]
pub extern "C" fn recall_abi_version() -> u32 {
    ffi_boundary(ptr::null_mut(), 0, || Ok(ABI_VERSION))
}

/// Open an engine over `n` pack buffers (legacy symbol).
///
/// `packs[i]` points to `lens[i]` bytes. Buffers are copied; hosts may free
/// them after return. On failure this returns NULL, writes `err` when non-NULL,
/// and records a thread-local diagnostic.
///
/// # Safety
/// - `packs` and `lens` must be valid for `n` elements when `n > 0`.
/// - Each `packs[i]` must point to at least `lens[i]` readable bytes.
/// - `err` must be NULL or writable for one `recall_err`.
#[no_mangle]
pub unsafe extern "C" fn recall_open(
    packs: *const *const u8,
    lens: *const usize,
    n: usize,
    err: *mut recall_err,
) -> *mut recall_engine {
    ffi_boundary(err, ptr::null_mut(), || unsafe {
        open_impl(packs, lens, n)
    })
}

/// Open an engine with structured error reporting.
///
/// This additive v2 symbol currently shares the same pack ownership contract as
/// `recall_open`; it exists so versioned host bindings never need to infer an
/// error from a NULL handle alone.
///
/// # Safety
/// The requirements are identical to [`recall_open`].
#[no_mangle]
pub unsafe extern "C" fn recall_open_v2(
    packs: *const *const u8,
    lens: *const usize,
    n: usize,
    out_err: *mut recall_err,
) -> *mut recall_engine {
    ffi_boundary(out_err, ptr::null_mut(), || unsafe {
        open_impl(packs, lens, n)
    })
}

/// Search and return a newly allocated JSON string (legacy symbol).
/// Free the result with [`recall_free_str`].
///
/// # Safety
/// - `eng` must be a live engine from `recall_open`/`recall_open_v2`.
/// - `query_utf8` must be a valid NUL-terminated C string.
/// - `opts` may be NULL; otherwise it must be readable.
#[no_mangle]
pub unsafe extern "C" fn recall_search_json(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    opts: *const recall_opts,
) -> *mut c_char {
    ffi_boundary(ptr::null_mut(), ptr::null_mut(), || unsafe {
        search_impl(eng, query_utf8, opts, false)
    })
}

/// Search with explicit structured error reporting.
///
/// # Safety
/// The pointer requirements match [`recall_search_json`]. `out_err` must be
/// NULL or writable for one `recall_err`.
#[no_mangle]
pub unsafe extern "C" fn recall_search_json_v2(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    opts: *const recall_opts,
    out_err: *mut recall_err,
) -> *mut c_char {
    ffi_boundary(out_err, ptr::null_mut(), || unsafe {
        search_impl(eng, query_utf8, opts, true)
    })
}

/// Plan conservative query-only clauses and return a newly allocated JSON
/// object with `planner` and `segments` fields. No engine or corpus is read.
/// Free the result with [`recall_free_str`].
///
/// # Safety
/// `query_utf8` must be a valid NUL-terminated C string. `out_err` must be NULL
/// or writable for one [`recall_err`].
#[no_mangle]
pub unsafe extern "C" fn recall_plan_query_segments_json(
    query_utf8: *const c_char,
    out_err: *mut recall_err,
) -> *mut c_char {
    ffi_boundary(out_err, ptr::null_mut(), || unsafe {
        let query = required_utf8(query_utf8, "query")?;
        let json = serde_json::to_string(&plan_query_segments(query)).map_err(|error| {
            FfiFailure::new(
                recall_err::RECALL_ERR_JSON,
                format!("clause-plan JSON serialization failed: {error}"),
            )
        })?;
        c_string(json)
    })
}

/// Search with a host-computed embedding (legacy LVEC v1 symbol).
///
/// LVEC v2 packs deliberately reject this function because they require an
/// embedding-space ID. Use `recall_search_with_embedding_json_v2` for all new
/// integrations.
///
/// # Safety
/// In addition to [`recall_search_json`] requirements, `embedding` must point
/// to `embedding_len` readable, properly aligned `f32` values.
#[no_mangle]
pub unsafe extern "C" fn recall_search_with_embedding_json(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    embedding: *const f32,
    embedding_len: usize,
    opts: *const recall_opts,
) -> *mut c_char {
    ffi_boundary(ptr::null_mut(), ptr::null_mut(), || unsafe {
        semantic_search_v1_impl(eng, query_utf8, embedding, embedding_len, opts)
    })
}

/// Search an LVEC v2 lane with a host-computed embedding and encoder-space ID.
///
/// `embedding_space_id_len` must be exactly 32. A mismatched ID fails closed
/// with `RECALL_ERR_EMBEDDING_SPACE`; dimensions/non-finite/zero-norm vectors
/// fail with `RECALL_ERR_EMBEDDING`.
///
/// # Safety
/// - The engine/query/options requirements match [`recall_search_json_v2`].
/// - `embedding_space_id` must point to `embedding_space_id_len` readable bytes.
/// - `embedding` must point to `embedding_len` readable, properly aligned `f32`
///   values.
/// - `out_err` must be NULL or writable for one `recall_err`.
#[no_mangle]
pub unsafe extern "C" fn recall_search_with_embedding_json_v2(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    embedding_space_id: *const u8,
    embedding_space_id_len: usize,
    embedding: *const f32,
    embedding_len: usize,
    opts: *const recall_opts,
    out_err: *mut recall_err,
) -> *mut c_char {
    ffi_boundary(out_err, ptr::null_mut(), || unsafe {
        semantic_search_v2_impl(
            eng,
            query_utf8,
            embedding_space_id,
            embedding_space_id_len,
            embedding,
            embedding_len,
            opts,
        )
    })
}

/// Search an LVEC v2 lane with a whole-query vector and one to four
/// independently embedded query clauses. Use
/// [`recall_plan_query_segments_json`] to obtain the canonical clause texts.
///
/// # Safety
/// - The engine/query/options, embedding-space ID, and whole-vector
///   requirements match [`recall_search_with_embedding_json_v2`].
/// - `segment_count` must be between one and four, and `segments` must point
///   to that many readable, properly aligned [`recall_query_segment`] values.
/// - Every segment text and vector must satisfy the pointer requirements
///   documented on [`recall_query_segment`].
/// - `out_err` must be NULL or writable for one [`recall_err`].
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn recall_search_with_segment_embeddings_json_v2(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    embedding_space_id: *const u8,
    embedding_space_id_len: usize,
    whole_embedding: *const f32,
    whole_embedding_len: usize,
    segments: *const recall_query_segment,
    segment_count: usize,
    opts: *const recall_opts,
    out_err: *mut recall_err,
) -> *mut c_char {
    ffi_boundary(out_err, ptr::null_mut(), || unsafe {
        semantic_segment_search_v2_impl(
            eng,
            query_utf8,
            embedding_space_id,
            embedding_space_id_len,
            whole_embedding,
            whole_embedding_len,
            segments,
            segment_count,
            opts,
        )
    })
}

/// Return runtime engine metadata as newly allocated UTF-8 JSON.
///
/// The object contains `abi_version`, `translation_id`, `verse_count`, `lanes`,
/// and `dense`. `dense` is NULL without LVEC, otherwise it contains `version`,
/// `dims`, `table_count`, `table_roles`, and a lowercase hexadecimal
/// `embedding_space_id` (NULL for legacy LVEC v1). Free the string with
/// [`recall_free_str`].
///
/// # Safety
/// `eng` must be a live engine. `out_err` must be NULL or writable for one
/// `recall_err`.
#[no_mangle]
pub unsafe extern "C" fn recall_engine_info_json(
    eng: *mut recall_engine,
    out_err: *mut recall_err,
) -> *mut c_char {
    ffi_boundary(out_err, ptr::null_mut(), || unsafe {
        engine_info_impl(eng)
    })
}

/// Copy the most recent failure message for this thread.
///
/// Returns NULL when no failure has been recorded. Otherwise returns a newly
/// allocated string that must be released with [`recall_free_str`]. Calling
/// this function does not clear the stored diagnostic.
#[no_mangle]
pub extern "C" fn recall_last_error_message() -> *mut c_char {
    ffi_boundary(ptr::null_mut(), ptr::null_mut(), || {
        let message = LAST_ERROR.with(|slot| slot.borrow().clone());
        match message {
            Some(message) => c_string(message),
            None => Ok(ptr::null_mut()),
        }
    })
}

/// Clear the most recent failure message for this thread.
#[no_mangle]
pub extern "C" fn recall_clear_last_error() {
    ffi_boundary(ptr::null_mut(), (), || {
        LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
        Ok(())
    });
}

/// Free a string returned by any recall JSON/message function.
///
/// # Safety
/// `s` must be NULL or a pointer returned by this library that has not already
/// been freed.
#[no_mangle]
pub unsafe extern "C" fn recall_free_str(s: *mut c_char) {
    ffi_boundary(ptr::null_mut(), (), || {
        if !s.is_null() {
            unsafe { drop(CString::from_raw(s)) };
        }
        Ok(())
    });
}

/// Close an engine and free its resources.
///
/// # Safety
/// `eng` must be NULL or a live engine pointer, and a non-NULL pointer may be
/// closed exactly once.
#[no_mangle]
pub unsafe extern "C" fn recall_close(eng: *mut recall_engine) {
    ffi_boundary(ptr::null_mut(), (), || {
        if !eng.is_null() {
            unsafe { drop(Box::from_raw(eng)) };
        }
        Ok(())
    });
}

unsafe fn open_impl(
    packs: *const *const u8,
    lens: *const usize,
    n: usize,
) -> Result<*mut recall_engine, FfiFailure> {
    if n == 0 || packs.is_null() || lens.is_null() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_NULL,
            "open requires at least one pack and non-NULL pack/lens arrays",
        ));
    }
    if n > MAX_PACK_COUNT {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_INVALID_ARGUMENT,
            format!("pack count {n} exceeds the ABI limit of {MAX_PACK_COUNT}"),
        ));
    }

    let pack_ptrs = unsafe { std::slice::from_raw_parts(packs, n) };
    let lengths = unsafe { std::slice::from_raw_parts(lens, n) };
    let mut slices = Vec::with_capacity(n);
    for (index, (&pack, &len)) in pack_ptrs.iter().zip(lengths).enumerate() {
        if pack.is_null() {
            return Err(FfiFailure::new(
                recall_err::RECALL_ERR_NULL,
                format!("pack pointer at index {index} is NULL"),
            ));
        }
        if len > isize::MAX as usize {
            return Err(FfiFailure::new(
                recall_err::RECALL_ERR_INVALID_ARGUMENT,
                format!("pack length at index {index} exceeds isize::MAX"),
            ));
        }
        slices.push(unsafe { std::slice::from_raw_parts(pack, len) });
    }

    let inner = Engine::open(&slices).map_err(|error| {
        FfiFailure::new(
            recall_err::RECALL_ERR_OPEN,
            format!("engine open failed: {error}"),
        )
    })?;
    Ok(Box::into_raw(Box::new(recall_engine { inner })))
}

unsafe fn search_impl(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    opts: *const recall_opts,
    strict_v2: bool,
) -> Result<*mut c_char, FfiFailure> {
    let engine = unsafe { engine_ref(eng)? };
    let query = unsafe { required_utf8(query_utf8, "query")? };
    let search_opts = unsafe { parse_opts(opts, strict_v2)? };
    let json = engine
        .inner
        .search_json(query, &search_opts)
        .map_err(search_failure)?;
    c_string(json)
}

unsafe fn semantic_search_v1_impl(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    embedding: *const f32,
    embedding_len: usize,
    opts: *const recall_opts,
) -> Result<*mut c_char, FfiFailure> {
    let engine = unsafe { engine_ref(eng)? };
    let query = unsafe { required_utf8(query_utf8, "query")? };
    let values = unsafe { embedding_slice(embedding, embedding_len)? };
    let search_opts = unsafe { parse_opts(opts, false)? };
    let json = engine
        .inner
        .search_json_with_embedding(query, values, &search_opts)
        .map_err(search_failure)?;
    c_string(json)
}

#[allow(clippy::too_many_arguments)]
unsafe fn semantic_search_v2_impl(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    embedding_space_id: *const u8,
    embedding_space_id_len: usize,
    embedding: *const f32,
    embedding_len: usize,
    opts: *const recall_opts,
) -> Result<*mut c_char, FfiFailure> {
    let engine = unsafe { engine_ref(eng)? };
    let query = unsafe { required_utf8(query_utf8, "query")? };
    if embedding_space_id.is_null() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_NULL,
            "embedding-space ID pointer is NULL",
        ));
    }
    if embedding_space_id_len != EMBEDDING_SPACE_ID_LEN {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_EMBEDDING_SPACE,
            format!(
                "embedding-space ID has {embedding_space_id_len} bytes; exactly {EMBEDDING_SPACE_ID_LEN} required"
            ),
        ));
    }
    let raw_space =
        unsafe { std::slice::from_raw_parts(embedding_space_id, EMBEDDING_SPACE_ID_LEN) };
    let space_id: &[u8; EMBEDDING_SPACE_ID_LEN] = raw_space.try_into().map_err(|_| {
        FfiFailure::new(
            recall_err::RECALL_ERR_EMBEDDING_SPACE,
            "embedding-space ID must contain exactly 32 bytes",
        )
    })?;
    let values = unsafe { embedding_slice(embedding, embedding_len)? };
    let search_opts = unsafe { parse_opts(opts, true)? };
    let json = engine
        .inner
        .search_json_with_embedding_in_space(query, space_id, values, &search_opts)
        .map_err(search_failure)?;
    c_string(json)
}

#[allow(clippy::too_many_arguments)]
unsafe fn semantic_segment_search_v2_impl(
    eng: *mut recall_engine,
    query_utf8: *const c_char,
    embedding_space_id: *const u8,
    embedding_space_id_len: usize,
    whole_embedding: *const f32,
    whole_embedding_len: usize,
    segments: *const recall_query_segment,
    segment_count: usize,
    opts: *const recall_opts,
) -> Result<*mut c_char, FfiFailure> {
    let engine = unsafe { engine_ref(eng)? };
    let query = unsafe { required_utf8(query_utf8, "query")? };
    if segment_count == 0 || segment_count > MAX_QUERY_SEGMENTS {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_INVALID_ARGUMENT,
            format!("segment count must be between 1 and {MAX_QUERY_SEGMENTS}"),
        ));
    }
    if segments.is_null() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_NULL,
            "query-segment pointer is NULL",
        ));
    }
    if embedding_space_id.is_null() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_NULL,
            "embedding-space ID pointer is NULL",
        ));
    }
    if embedding_space_id_len != EMBEDDING_SPACE_ID_LEN {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_EMBEDDING_SPACE,
            format!(
                "embedding-space ID has {embedding_space_id_len} bytes; exactly {EMBEDDING_SPACE_ID_LEN} required"
            ),
        ));
    }
    let raw_space =
        unsafe { std::slice::from_raw_parts(embedding_space_id, EMBEDDING_SPACE_ID_LEN) };
    let space_id: &[u8; EMBEDDING_SPACE_ID_LEN] = raw_space.try_into().map_err(|_| {
        FfiFailure::new(
            recall_err::RECALL_ERR_EMBEDDING_SPACE,
            "embedding-space ID must contain exactly 32 bytes",
        )
    })?;
    let whole_values = unsafe { embedding_slice(whole_embedding, whole_embedding_len)? };
    let raw_segments = unsafe { std::slice::from_raw_parts(segments, segment_count) };
    let mut query_segments = Vec::with_capacity(raw_segments.len());
    for (index, segment) in raw_segments.iter().enumerate() {
        let text =
            unsafe { required_utf8(segment.text_utf8, &format!("segment text at index {index}"))? };
        let embedding = unsafe { embedding_slice(segment.embedding, segment.embedding_len)? };
        query_segments.push(QuerySegmentEmbedding { text, embedding });
    }
    let search_opts = unsafe { parse_opts(opts, true)? };
    let json = engine
        .inner
        .search_json_with_segment_embeddings_in_space(
            query,
            space_id,
            whole_values,
            &query_segments,
            &search_opts,
        )
        .map_err(search_failure)?;
    c_string(json)
}

unsafe fn engine_info_impl(eng: *mut recall_engine) -> Result<*mut c_char, FfiFailure> {
    let engine = unsafe { engine_ref(eng)? };
    let dense = engine.inner.dense_metadata().map(|metadata| {
        let table_roles: Vec<&str> = metadata.table_roles[..metadata.table_count]
            .iter()
            .map(|role| role.as_str())
            .collect();
        json!({
            "version": metadata.version,
            "dims": metadata.dims,
            "table_count": metadata.table_count,
            "table_roles": table_roles,
            "embedding_space_id": metadata.embedding_space_id.map(hex_space_id),
        })
    });
    let lanes: Vec<&str> = engine
        .inner
        .available_lanes()
        .into_iter()
        .map(|lane| lane.as_str())
        .collect();
    let info = json!({
        "abi_version": ABI_VERSION,
        "translation_id": engine.inner.translation_id(),
        "verse_count": engine.inner.verse_count(),
        "lanes": lanes,
        "dense": dense,
    });
    let encoded = serde_json::to_string(&info).map_err(|error| {
        FfiFailure::new(
            recall_err::RECALL_ERR_JSON,
            format!("engine-info JSON serialization failed: {error}"),
        )
    })?;
    c_string(encoded)
}

unsafe fn engine_ref<'a>(eng: *mut recall_engine) -> Result<&'a recall_engine, FfiFailure> {
    if eng.is_null() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_NULL,
            "engine pointer is NULL",
        ));
    }
    Ok(unsafe { &*eng })
}

unsafe fn required_utf8<'a>(value: *const c_char, field: &str) -> Result<&'a str, FfiFailure> {
    if value.is_null() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_NULL,
            format!("{field} pointer is NULL"),
        ));
    }
    unsafe { CStr::from_ptr(value) }.to_str().map_err(|error| {
        FfiFailure::new(
            recall_err::RECALL_ERR_UTF8,
            format!("{field} is not valid UTF-8: {error}"),
        )
    })
}

unsafe fn parse_opts(opts: *const recall_opts, strict_v2: bool) -> Result<SearchOpts, FfiFailure> {
    if opts.is_null() {
        return Ok(SearchOpts::default());
    }
    let raw = unsafe { &*opts };
    if strict_v2 && raw.flags != 0 {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_INVALID_ARGUMENT,
            format!("unsupported recall_opts.flags value {}", raw.flags),
        ));
    }
    let mut value = SearchOpts::default();
    if raw.top_k > 0 {
        if strict_v2 && raw.top_k as usize > MAX_TOP_K {
            return Err(FfiFailure::new(
                recall_err::RECALL_ERR_INVALID_ARGUMENT,
                format!("top_k must be between 1 and {MAX_TOP_K}"),
            ));
        }
        value = value.with_top_k(raw.top_k as usize);
    }
    if !raw.book_hint.is_null() {
        match unsafe { CStr::from_ptr(raw.book_hint) }.to_str() {
            Ok(hint) if !hint.is_empty() => value.book_hint = Some(hint.to_string()),
            Ok(_) => {}
            // Keep the legacy symbol's historical behavior while making v2
            // reject malformed option strings with a typed error.
            Err(error) if strict_v2 => {
                return Err(FfiFailure::new(
                    recall_err::RECALL_ERR_UTF8,
                    format!("book_hint is not valid UTF-8: {error}"),
                ));
            }
            Err(_) => {}
        }
    }
    Ok(value)
}

unsafe fn embedding_slice<'a>(
    embedding: *const f32,
    embedding_len: usize,
) -> Result<&'a [f32], FfiFailure> {
    if embedding.is_null() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_NULL,
            "embedding pointer is NULL",
        ));
    }
    if embedding_len == 0 || embedding_len > isize::MAX as usize / std::mem::size_of::<f32>() {
        return Err(FfiFailure::new(
            recall_err::RECALL_ERR_EMBEDDING,
            format!("embedding length {embedding_len} is invalid"),
        ));
    }
    Ok(unsafe { std::slice::from_raw_parts(embedding, embedding_len) })
}

fn search_failure(error: CoreError) -> FfiFailure {
    match error {
        CoreError::InvalidQueryEmbedding(message)
            if message.contains("space") || message.contains("LVEC v2 requires") =>
        {
            FfiFailure::new(recall_err::RECALL_ERR_EMBEDDING_SPACE, message)
        }
        CoreError::InvalidQueryEmbedding(message) => {
            FfiFailure::new(recall_err::RECALL_ERR_EMBEDDING, message)
        }
        CoreError::Internal(message) if message.contains("json") => {
            FfiFailure::new(recall_err::RECALL_ERR_JSON, message)
        }
        other => FfiFailure::new(recall_err::RECALL_ERR_SEARCH, other.to_string()),
    }
}

fn c_string(value: impl Into<String>) -> Result<*mut c_char, FfiFailure> {
    CString::new(value.into())
        .map(CString::into_raw)
        .map_err(|_| {
            FfiFailure::new(
                recall_err::RECALL_ERR_JSON,
                "generated UTF-8 output contains an interior NUL byte",
            )
        })
}

fn hex_space_id(id: [u8; EMBEDDING_SPACE_ID_LEN]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(EMBEDDING_SPACE_ID_LEN * 2);
    for byte in id {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn ffi_boundary<T: Copy>(
    out_err: *mut recall_err,
    fallback: T,
    operation: impl FnOnce() -> Result<T, FfiFailure>,
) -> T {
    let outcome = catch_unwind(AssertUnwindSafe(|| match operation() {
        Ok(value) => {
            unsafe { write_error(out_err, recall_err::RECALL_OK) };
            value
        }
        Err(failure) => {
            set_last_error(failure.message);
            unsafe { write_error(out_err, failure.code) };
            fallback
        }
    }));

    match outcome {
        Ok(value) => value,
        Err(payload) => {
            // Error recording itself is guarded as well, so a TLS teardown
            // edge case can never unwind into a foreign caller.
            let message = format!(
                "panic contained at C ABI boundary: {}",
                panic_text(&payload)
            );
            let _ = catch_unwind(AssertUnwindSafe(|| {
                set_last_error(message);
                unsafe { write_error(out_err, recall_err::RECALL_ERR_PANIC) };
            }));
            fallback
        }
    }
}

fn set_last_error(message: String) {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = Some(message));
}

fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

unsafe fn write_error(out_err: *mut recall_err, code: recall_err) {
    if !out_err.is_null() {
        unsafe { ptr::write(out_err, code) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use recall_core::DenseLane;
    use recall_pack::{MetaSection, PackWriter, SectionTag, TextSection};

    const SPACE_ID: [u8; EMBEDDING_SPACE_ID_LEN] = [0x5a; EMBEDDING_SPACE_ID_LEN];

    fn base_pack() -> Vec<u8> {
        let text = TextSection::from_verses(vec![
            (
                "Psalm 23:1".into(),
                "The LORD is my shepherd; I shall not want.".into(),
            ),
            (
                "Genesis 1:1".into(),
                "In the beginning God created the heavens and the earth.".into(),
            ),
        ]);
        PackWriter::new()
            .add_meta(&MetaSection::new("BSB", 2))
            .add_text(&text)
            .finish()
    }

    fn semantic_pack() -> Vec<u8> {
        let vectors = [127_i8, 0, 0, 127];
        let body = DenseLane::encode_v2(2, 2, &[&vectors], SPACE_ID).expect("LVEC fixture");
        PackWriter::new()
            .add_meta(&MetaSection::new("BSB", 2))
            .add_raw(SectionTag::Lvec, body)
            .finish()
    }

    unsafe fn open_fixture(include_dense: bool) -> *mut recall_engine {
        let base = base_pack();
        let dense = semantic_pack();
        let mut pointers = vec![base.as_ptr()];
        let mut lengths = vec![base.len()];
        if include_dense {
            pointers.push(dense.as_ptr());
            lengths.push(dense.len());
        }
        let mut error = recall_err::RECALL_ERR_INTERNAL;
        let engine = unsafe {
            recall_open_v2(
                pointers.as_ptr(),
                lengths.as_ptr(),
                pointers.len(),
                &mut error,
            )
        };
        assert!(!engine.is_null());
        assert_eq!(error, recall_err::RECALL_OK);
        engine
    }

    unsafe fn owned_string(pointer: *mut c_char) -> String {
        assert!(!pointer.is_null());
        let value = unsafe { CStr::from_ptr(pointer) }
            .to_str()
            .expect("UTF-8")
            .to_string();
        unsafe { recall_free_str(pointer) };
        value
    }

    #[test]
    fn legacy_symbols_still_round_trip() {
        let pack = base_pack();
        let pointers = [pack.as_ptr()];
        let lengths = [pack.len()];
        let mut error = recall_err::RECALL_ERR_INTERNAL;
        let engine = unsafe {
            recall_open(
                pointers.as_ptr(),
                lengths.as_ptr(),
                pointers.len(),
                &mut error,
            )
        };
        assert!(!engine.is_null());
        assert_eq!(error, recall_err::RECALL_OK);

        let query = CString::new("the lord is my shepherd").unwrap();
        let result = unsafe { recall_search_json(engine, query.as_ptr(), ptr::null()) };
        let json = unsafe { owned_string(result) };
        assert!(json.contains("Psalm 23:1"));
        unsafe { recall_close(engine) };
    }

    #[test]
    fn v2_reports_typed_errors_and_allocated_tls_diagnostics() {
        recall_clear_last_error();
        let engine = unsafe { open_fixture(false) };
        let mut error = recall_err::RECALL_OK;
        let result = unsafe { recall_search_json_v2(engine, ptr::null(), ptr::null(), &mut error) };
        assert!(result.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_NULL);
        let message = recall_last_error_message();
        let message = unsafe { owned_string(message) };
        assert!(message.contains("query pointer is NULL"));

        recall_clear_last_error();
        assert!(recall_last_error_message().is_null());
        unsafe { recall_close(engine) };
    }

    #[test]
    fn open_v2_reports_null_and_invalid_pack_failures() {
        recall_clear_last_error();
        let mut error = recall_err::RECALL_OK;
        let engine = unsafe { recall_open_v2(ptr::null(), ptr::null(), 0, &mut error) };
        assert!(engine.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_NULL);
        let message = unsafe { owned_string(recall_last_error_message()) };
        assert!(message.contains("at least one pack"));

        let invalid_pack = [0_u8; 32];
        let pointers = [invalid_pack.as_ptr()];
        let lengths = [invalid_pack.len()];
        let engine = unsafe {
            recall_open_v2(
                pointers.as_ptr(),
                lengths.as_ptr(),
                pointers.len(),
                &mut error,
            )
        };
        assert!(engine.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_OPEN);
        let message = unsafe { owned_string(recall_last_error_message()) };
        assert!(message.contains("engine open failed"));
    }

    #[test]
    fn v2_rejects_malformed_utf8_and_reserved_flags() {
        let engine = unsafe { open_fixture(false) };
        let invalid_utf8 = [0xff_u8, 0];
        let mut error = recall_err::RECALL_OK;
        let result = unsafe {
            recall_search_json_v2(
                engine,
                invalid_utf8.as_ptr().cast(),
                ptr::null(),
                &mut error,
            )
        };
        assert!(result.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_UTF8);

        let query = CString::new("shepherd").unwrap();
        let opts = recall_opts {
            top_k: 0,
            book_hint: ptr::null(),
            flags: 1,
        };
        let result = unsafe { recall_search_json_v2(engine, query.as_ptr(), &opts, &mut error) };
        assert!(result.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_INVALID_ARGUMENT);
        unsafe { recall_close(engine) };
    }

    #[test]
    fn v2_rejects_oversized_top_k_while_legacy_clamps_it() {
        let engine = unsafe { open_fixture(false) };
        let query = CString::new("shepherd").unwrap();
        let opts = recall_opts {
            top_k: (MAX_TOP_K + 1) as c_uint,
            book_hint: ptr::null(),
            flags: 0,
        };
        let mut error = recall_err::RECALL_OK;

        let rejected = unsafe { recall_search_json_v2(engine, query.as_ptr(), &opts, &mut error) };
        assert!(rejected.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_INVALID_ARGUMENT);
        let message = unsafe { owned_string(recall_last_error_message()) };
        assert!(message.contains("top_k"));
        assert!(message.contains(&MAX_TOP_K.to_string()));

        let legacy = unsafe { recall_search_json(engine, query.as_ptr(), &opts) };
        assert!(!legacy.is_null());
        let _: serde_json::Value =
            serde_json::from_str(&unsafe { owned_string(legacy) }).expect("legacy search JSON");
        let parsed = unsafe { parse_opts(&opts, false) }.expect("legacy options clamp");
        assert_eq!(parsed.top_k, MAX_TOP_K);

        unsafe { recall_close(engine) };
    }

    #[test]
    fn engine_info_exposes_dense_compatibility_contract() {
        let engine = unsafe { open_fixture(true) };
        let mut error = recall_err::RECALL_ERR_INTERNAL;
        let info = unsafe { recall_engine_info_json(engine, &mut error) };
        assert_eq!(error, recall_err::RECALL_OK);
        let info: serde_json::Value =
            serde_json::from_str(&unsafe { owned_string(info) }).expect("info JSON");
        assert_eq!(info["abi_version"], ABI_VERSION);
        assert_eq!(info["translation_id"], "BSB");
        assert_eq!(info["verse_count"], 2);
        assert_eq!(info["dense"]["version"], 2);
        assert_eq!(info["dense"]["dims"], 2);
        assert_eq!(info["dense"]["table_count"], 1);
        assert_eq!(info["dense"]["table_roles"], serde_json::json!(["verse"]));
        assert_eq!(info["dense"]["embedding_space_id"], "5a".repeat(32));
        assert!(info["lanes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|lane| lane == "c"));
        unsafe { recall_close(engine) };
    }

    #[test]
    fn semantic_v2_requires_exact_matching_space_id() {
        let engine = unsafe { open_fixture(true) };
        let query = CString::new("a creator begins everything").unwrap();
        let embedding = [1.0_f32, 0.0];
        let mut error = recall_err::RECALL_ERR_INTERNAL;

        let json = unsafe {
            recall_search_with_embedding_json_v2(
                engine,
                query.as_ptr(),
                SPACE_ID.as_ptr(),
                SPACE_ID.len(),
                embedding.as_ptr(),
                embedding.len(),
                ptr::null(),
                &mut error,
            )
        };
        assert_eq!(error, recall_err::RECALL_OK);
        let json = unsafe { owned_string(json) };
        assert!(json.contains("Psalm 23:1"));

        let wrong_space = [0x33_u8; EMBEDDING_SPACE_ID_LEN];
        let failed = unsafe {
            recall_search_with_embedding_json_v2(
                engine,
                query.as_ptr(),
                wrong_space.as_ptr(),
                wrong_space.len(),
                embedding.as_ptr(),
                embedding.len(),
                ptr::null(),
                &mut error,
            )
        };
        assert!(failed.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_EMBEDDING_SPACE);

        let failed = unsafe {
            recall_search_with_embedding_json_v2(
                engine,
                query.as_ptr(),
                SPACE_ID.as_ptr(),
                SPACE_ID.len() - 1,
                embedding.as_ptr(),
                embedding.len(),
                ptr::null(),
                &mut error,
            )
        };
        assert!(failed.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_EMBEDDING_SPACE);

        let legacy = unsafe {
            recall_search_with_embedding_json(
                engine,
                query.as_ptr(),
                embedding.as_ptr(),
                embedding.len(),
                ptr::null(),
            )
        };
        assert!(legacy.is_null());
        let legacy_error = unsafe { owned_string(recall_last_error_message()) };
        assert!(legacy_error.contains("requires an embedding-space ID"));
        unsafe { recall_close(engine) };
    }

    #[test]
    fn semantic_v2_rejects_bad_dimensions_nonfinite_and_zero_norm() {
        let engine = unsafe { open_fixture(true) };
        let query = CString::new("meaning-only query").unwrap();
        let mut error = recall_err::RECALL_OK;

        for embedding in [vec![1.0_f32], vec![f32::NAN, 0.0], vec![0.0_f32, 0.0]] {
            let result = unsafe {
                recall_search_with_embedding_json_v2(
                    engine,
                    query.as_ptr(),
                    SPACE_ID.as_ptr(),
                    SPACE_ID.len(),
                    embedding.as_ptr(),
                    embedding.len(),
                    ptr::null(),
                    &mut error,
                )
            };
            assert!(result.is_null());
            assert_eq!(error, recall_err::RECALL_ERR_EMBEDDING);
            let message = unsafe { owned_string(recall_last_error_message()) };
            assert!(message.contains("embedding"));
        }
        unsafe { recall_close(engine) };
    }

    #[test]
    fn planner_is_engine_independent_unicode_safe_and_conservative() {
        let query = CString::new("Élie prie — Jésus nourrit la foule").unwrap();
        let mut error = recall_err::RECALL_ERR_INTERNAL;
        let json = unsafe { recall_plan_query_segments_json(query.as_ptr(), &mut error) };
        assert_eq!(error, recall_err::RECALL_OK);
        let plan: serde_json::Value =
            serde_json::from_str(&unsafe { owned_string(json) }).expect("plan JSON");
        assert_eq!(plan["planner"], "verse-recall-clause-v2");
        assert_eq!(
            plan["segments"],
            serde_json::json!(["Élie prie", "Jésus nourrit la foule"])
        );

        let short = CString::new("faith and hope").unwrap();
        let json = unsafe { recall_plan_query_segments_json(short.as_ptr(), &mut error) };
        let plan: serde_json::Value =
            serde_json::from_str(&unsafe { owned_string(json) }).expect("plan JSON");
        assert_eq!(plan["segments"], serde_json::json!([]));
    }

    #[test]
    fn segmented_v2_search_round_trips_and_validates_ffi_shape() {
        let engine = unsafe { open_fixture(true) };
        let query = CString::new("first remembered thought and second remembered image").unwrap();
        let first_text = CString::new("first remembered thought").unwrap();
        let second_text = CString::new("second remembered image").unwrap();
        let whole = [1.0_f32, 0.0];
        let first = [1.0_f32, 0.0];
        let second = [0.0_f32, 1.0];
        let segments = [
            recall_query_segment {
                text_utf8: first_text.as_ptr(),
                embedding: first.as_ptr(),
                embedding_len: first.len(),
            },
            recall_query_segment {
                text_utf8: second_text.as_ptr(),
                embedding: second.as_ptr(),
                embedding_len: second.len(),
            },
        ];
        let mut error = recall_err::RECALL_ERR_INTERNAL;
        let json = unsafe {
            recall_search_with_segment_embeddings_json_v2(
                engine,
                query.as_ptr(),
                SPACE_ID.as_ptr(),
                SPACE_ID.len(),
                whole.as_ptr(),
                whole.len(),
                segments.as_ptr(),
                segments.len(),
                ptr::null(),
                &mut error,
            )
        };
        assert_eq!(error, recall_err::RECALL_OK);
        let response: serde_json::Value =
            serde_json::from_str(&unsafe { owned_string(json) }).expect("search JSON");
        assert_eq!(response["results"].as_array().unwrap().len(), 2);

        let failed = unsafe {
            recall_search_with_segment_embeddings_json_v2(
                engine,
                query.as_ptr(),
                SPACE_ID.as_ptr(),
                SPACE_ID.len(),
                whole.as_ptr(),
                whole.len(),
                ptr::null(),
                0,
                ptr::null(),
                &mut error,
            )
        };
        assert!(failed.is_null());
        assert_eq!(error, recall_err::RECALL_ERR_INVALID_ARGUMENT);
        unsafe { recall_close(engine) };
    }

    #[test]
    fn panic_is_contained_and_reported() {
        recall_clear_last_error();
        let mut error = recall_err::RECALL_OK;
        let value = ffi_boundary(&mut error, 41_u32, || -> Result<u32, FfiFailure> {
            panic!("test boundary panic")
        });
        assert_eq!(value, 41);
        assert_eq!(error, recall_err::RECALL_ERR_PANIC);
        let message = unsafe { owned_string(recall_last_error_message()) };
        assert!(message.contains("test boundary panic"));
    }

    #[test]
    fn last_error_is_thread_local() {
        recall_clear_last_error();
        let mut error = recall_err::RECALL_OK;
        let _: u8 = ffi_boundary(&mut error, 0, || {
            Err(FfiFailure::new(
                recall_err::RECALL_ERR_SEARCH,
                "main-thread failure",
            ))
        });
        assert!(!recall_last_error_message().is_null());
        std::thread::spawn(|| assert!(recall_last_error_message().is_null()))
            .join()
            .unwrap();
        recall_clear_last_error();
    }

    #[test]
    fn checked_in_header_contains_every_public_symbol() {
        let header = include_str!("../include/recall.h");
        for symbol in [
            "recall_abi_version",
            "recall_open",
            "recall_open_v2",
            "recall_search_json",
            "recall_search_json_v2",
            "recall_search_with_embedding_json",
            "recall_search_with_embedding_json_v2",
            "recall_plan_query_segments_json",
            "recall_search_with_segment_embeddings_json_v2",
            "recall_engine_info_json",
            "recall_last_error_message",
            "recall_clear_last_error",
            "recall_free_str",
            "recall_close",
        ] {
            assert!(header.contains(symbol), "header is missing {symbol}");
        }
    }
}

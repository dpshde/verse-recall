//! WASM bindings: `openEngine(buffers) / search(query, opts)`.

use recall_core::{plan_query_segments, Engine, QuerySegmentEmbedding, SearchOpts, MAX_TOP_K};
use wasm_bindgen::prelude::*;

/// WASM engine handle.
#[wasm_bindgen]
pub struct WasmEngine {
    inner: Engine,
}

/// Open engine from one or more pack ArrayBuffers / Uint8Arrays (as `Vec<u8>` from JS).
#[wasm_bindgen(js_name = openEngine)]
pub fn open_engine(packs: js_sys::Array) -> Result<WasmEngine, JsValue> {
    let mut slices_owned: Vec<Vec<u8>> = Vec::new();
    for i in 0..packs.length() {
        let val = packs.get(i);
        let arr = js_sys::Uint8Array::new(&val);
        let mut buf = vec![0u8; arr.length() as usize];
        arr.copy_to(&mut buf);
        slices_owned.push(buf);
    }
    if slices_owned.is_empty() {
        return Err(JsValue::from_str("no pack buffers"));
    }
    let refs: Vec<&[u8]> = slices_owned.iter().map(|b| b.as_slice()).collect();
    Engine::open(&refs)
        .map(|inner| WasmEngine { inner })
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Plan conservative query-only clauses without opening an engine. The JSON
/// object contains the stable planner ID and zero or more segment strings.
#[wasm_bindgen(js_name = planQuerySegments)]
pub fn plan_query_segments_json(query: &str) -> Result<String, JsValue> {
    serde_json::to_string(&plan_query_segments(query))
        .map_err(|error| JsValue::from_str(&format!("clause-plan serialization failed: {error}")))
}

#[wasm_bindgen]
impl WasmEngine {
    /// Search; returns JSON string (same shape as C ABI).
    #[wasm_bindgen]
    pub fn search(
        &self,
        query: &str,
        top_k: Option<u32>,
        book_hint: Option<String>,
    ) -> Result<String, JsValue> {
        let mut opts = SearchOpts::default();
        if let Some(k) = top_k {
            if k == 0 || k as usize > MAX_TOP_K {
                return Err(JsValue::from_str("top_k must be between 1 and 100"));
            }
            opts.top_k = k as usize;
        }
        opts.book_hint = book_hint.filter(|s| !s.is_empty());
        self.inner
            .search_json(query, &opts)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Search with a host-computed offline embedding for optional Lane C.
    #[wasm_bindgen(js_name = searchWithEmbedding)]
    pub fn search_with_embedding(
        &self,
        query: &str,
        embedding: js_sys::Float32Array,
        top_k: Option<u32>,
        book_hint: Option<String>,
    ) -> Result<String, JsValue> {
        let mut opts = SearchOpts::default();
        if let Some(k) = top_k {
            if k == 0 || k as usize > MAX_TOP_K {
                return Err(JsValue::from_str("top_k must be between 1 and 100"));
            }
            opts.top_k = k as usize;
        }
        opts.book_hint = book_hint.filter(|s| !s.is_empty());
        let mut values = vec![0.0f32; embedding.length() as usize];
        embedding.copy_to(&mut values);
        self.inner
            .search_json_with_embedding(query, &values, &opts)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Search an LVEC v2/v3 pack with its fail-closed encoder identity.
    #[wasm_bindgen(js_name = searchWithEmbeddingInSpace)]
    pub fn search_with_embedding_in_space(
        &self,
        query: &str,
        embedding_space_id: js_sys::Uint8Array,
        embedding: js_sys::Float32Array,
        top_k: Option<u32>,
        book_hint: Option<String>,
    ) -> Result<String, JsValue> {
        if embedding_space_id.length() != 32 {
            return Err(JsValue::from_str(
                "embedding-space ID must contain exactly 32 bytes",
            ));
        }
        let mut space_id = [0u8; 32];
        embedding_space_id.copy_to(&mut space_id);
        let mut values = vec![0.0f32; embedding.length() as usize];
        embedding.copy_to(&mut values);
        let mut opts = SearchOpts::default();
        if let Some(k) = top_k {
            if k == 0 || k as usize > MAX_TOP_K {
                return Err(JsValue::from_str("top_k must be between 1 and 100"));
            }
            opts.top_k = k as usize;
        }
        opts.book_hint = book_hint.filter(|s| !s.is_empty());
        self.inner
            .search_json_with_embedding_in_space(query, &space_id, &values, &opts)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Search a compound query with one whole-query vector and matching
    /// vectors for every clause returned by `planQuerySegments`.
    #[wasm_bindgen(js_name = searchWithSegmentEmbeddingsInSpace)]
    #[allow(clippy::too_many_arguments)]
    pub fn search_with_segment_embeddings_in_space(
        &self,
        query: &str,
        embedding_space_id: js_sys::Uint8Array,
        whole_embedding: js_sys::Float32Array,
        segment_texts: js_sys::Array,
        segment_embeddings: js_sys::Array,
        top_k: Option<u32>,
        book_hint: Option<String>,
    ) -> Result<String, JsValue> {
        if embedding_space_id.length() != 32 {
            return Err(JsValue::from_str(
                "embedding-space ID must contain exactly 32 bytes",
            ));
        }
        if segment_texts.length() != segment_embeddings.length() {
            return Err(JsValue::from_str(
                "segment texts and embeddings must have equal lengths",
            ));
        }

        let mut space_id = [0u8; 32];
        embedding_space_id.copy_to(&mut space_id);
        let mut whole_values = vec![0.0f32; whole_embedding.length() as usize];
        whole_embedding.copy_to(&mut whole_values);

        let mut texts = Vec::with_capacity(segment_texts.length() as usize);
        let mut vectors = Vec::with_capacity(segment_embeddings.length() as usize);
        for index in 0..segment_texts.length() {
            let text = segment_texts.get(index).as_string().ok_or_else(|| {
                JsValue::from_str(&format!("segment text at index {index} is not a string"))
            })?;
            let value = segment_embeddings.get(index);
            if !value.is_instance_of::<js_sys::Float32Array>() {
                return Err(JsValue::from_str(&format!(
                    "segment embedding at index {index} is not a Float32Array"
                )));
            }
            let array = js_sys::Float32Array::new(&value);
            let mut vector = vec![0.0f32; array.length() as usize];
            array.copy_to(&mut vector);
            texts.push(text);
            vectors.push(vector);
        }
        let segments: Vec<QuerySegmentEmbedding<'_>> = texts
            .iter()
            .zip(&vectors)
            .map(|(text, embedding)| QuerySegmentEmbedding { text, embedding })
            .collect();

        let mut opts = SearchOpts::default();
        if let Some(k) = top_k {
            if k == 0 || k as usize > MAX_TOP_K {
                return Err(JsValue::from_str("top_k must be between 1 and 100"));
            }
            opts.top_k = k as usize;
        }
        opts.book_hint = book_hint.filter(|s| !s.is_empty());
        self.inner
            .search_json_with_segment_embeddings_in_space(
                query,
                &space_id,
                &whole_values,
                &segments,
                &opts,
            )
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Optional LVEC compatibility contract as JSON, or `null` when absent.
    #[wasm_bindgen(js_name = denseMetadata)]
    pub fn dense_metadata(&self) -> String {
        match self.inner.dense_metadata() {
            Some(metadata) => {
                let space_id = metadata.embedding_space_id.map(hex_space_id);
                let table_roles: Vec<&str> = metadata.table_roles[..metadata.table_count]
                    .iter()
                    .map(|role| role.as_str())
                    .collect();
                serde_json::json!({
                    "version": metadata.version,
                    "dims": metadata.dims,
                    "verseCount": metadata.verse_count,
                    "tableCount": metadata.table_count,
                    "tableRoles": table_roles,
                    "embeddingSpaceId": space_id,
                })
                .to_string()
            }
            None => "null".into(),
        }
    }

    #[wasm_bindgen(js_name = verseCount)]
    pub fn verse_count(&self) -> u32 {
        self.inner.verse_count() as u32
    }

    #[wasm_bindgen(js_name = translationId)]
    pub fn translation_id(&self) -> String {
        self.inner.translation_id().to_string()
    }
}

fn hex_space_id(value: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// ABI version mirror.
#[wasm_bindgen(js_name = abiVersion)]
pub fn abi_version() -> u32 {
    recall_core::ABI_VERSION
}

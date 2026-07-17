/**
 * Portable JavaScript host for the wasm-bindgen verse-recall engine.
 *
 * The generated WASM module is loaded only when an engine is first initialized.
 * Tests and nonstandard runtimes may inject a compatible bindings namespace.
 */

export const ABI_VERSION = 1;
export const QUERY_SEGMENT_PLANNER = "verse-recall-clause-v2";

const DEFAULT_TOP_K = 10;
const MAX_TOP_K = 100;
const initializedBindings = new WeakMap();
let generatedBindingsPromise;

/**
 * Run the canonical query-only clause planner without opening any packs.
 *
 * @param {string} query
 * @param {InitOptions} [options]
 * @returns {Promise<{planner: string, segments: string[]}>}
 */
export async function planQuerySegments(query, options = {}) {
  const normalized = normalizeSearchInput(query, {});
  if (options === null || typeof options !== "object") {
    throw new VerseRecallInitError("initialization options must be an object", {
      code: "INVALID_OPTIONS",
    });
  }
  const bindings = await resolveBindings(options.bindings, options.wasm);
  if (typeof bindings.planQuerySegments !== "function") {
    throw new VerseRecallInitError("WASM bindings do not expose clause planning", {
      code: "UNSUPPORTED_FEATURE",
    });
  }
  try {
    return parseSegmentPlan(bindings.planQuerySegments(normalized.query));
  } catch (error) {
    if (error instanceof VerseRecallError) {
      throw error;
    }
    throw new VerseRecallSearchError(`clause planning failed: ${messageOf(error)}`, {
      code: "PLANNING_FAILED",
      cause: error,
    });
  }
}

/** Base error for all wrapper failures. */
export class VerseRecallError extends Error {
  /**
   * @param {string} message
   * @param {{code?: string, cause?: unknown}} [options]
   */
  constructor(message, options = {}) {
    super(message, options.cause === undefined ? undefined : { cause: options.cause });
    this.name = "VerseRecallError";
    this.code = options.code ?? "UNKNOWN";
  }
}

/** WASM loading, ABI, or pack-opening failure. */
export class VerseRecallInitError extends VerseRecallError {
  constructor(message, options = {}) {
    super(message, options);
    this.name = "VerseRecallInitError";
  }
}

/** Invalid search input, engine search failure, or malformed engine response. */
export class VerseRecallSearchError extends VerseRecallError {
  constructor(message, options = {}) {
    super(message, options);
    this.name = "VerseRecallSearchError";
  }
}

/** Pack download failure. */
export class VerseRecallFetchError extends VerseRecallError {
  constructor(message, options = {}) {
    super(message, options);
    this.name = "VerseRecallFetchError";
  }
}

/**
 * Parse the portable VRQADP01 query-only adapter format.
 *
 * @param {ArrayBuffer|ArrayBufferView} source
 * @returns {{inputDims: number, outputDims: number, matrix: Float32Array}}
 */
export function parseQueryAdapter(source) {
  if (!isBufferSource(source) || source.byteLength < 16) {
    throw new VerseRecallInitError("query adapter is truncated", {
      code: "INVALID_QUERY_ADAPTER",
    });
  }
  const bytes =
    source instanceof ArrayBuffer
      ? new Uint8Array(source)
      : new Uint8Array(source.buffer, source.byteOffset, source.byteLength);
  const magic = "VRQADP01";
  for (let index = 0; index < magic.length; index += 1) {
    if (bytes[index] !== magic.charCodeAt(index)) {
      throw new VerseRecallInitError("query adapter has invalid metadata", {
        code: "INVALID_QUERY_ADAPTER",
      });
    }
  }
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const inputDims = view.getUint32(8, true);
  const outputDims = view.getUint32(12, true);
  const valueCount = inputDims * outputDims;
  const expectedBytes = 16 + valueCount * 4;
  if (
    inputDims === 0 ||
    outputDims === 0 ||
    !Number.isSafeInteger(valueCount) ||
    expectedBytes !== bytes.byteLength
  ) {
    throw new VerseRecallInitError("query adapter dimensions are invalid", {
      code: "INVALID_QUERY_ADAPTER",
    });
  }
  const matrix = new Float32Array(valueCount);
  for (let index = 0; index < valueCount; index += 1) {
    const value = view.getFloat32(16 + index * 4, true);
    if (!Number.isFinite(value)) {
      throw new VerseRecallInitError("query adapter contains a non-finite value", {
        code: "INVALID_QUERY_ADAPTER",
      });
    }
    matrix[index] = value;
  }
  return { inputDims, outputDims, matrix };
}

/**
 * Apply a parsed query-only adapter and L2-normalize its output.
 *
 * @param {Float32Array} embedding
 * @param {{inputDims: number, outputDims: number, matrix: Float32Array}} adapter
 * @returns {Float32Array}
 */
export function applyQueryAdapter(embedding, adapter) {
  validateEmbedding(embedding);
  if (
    adapter === null ||
    typeof adapter !== "object" ||
    !Number.isInteger(adapter.inputDims) ||
    !Number.isInteger(adapter.outputDims) ||
    !(adapter.matrix instanceof Float32Array) ||
    adapter.inputDims !== embedding.length ||
    adapter.matrix.length !== adapter.inputDims * adapter.outputDims
  ) {
    throw new VerseRecallSearchError("query adapter does not match the embedding", {
      code: "QUERY_ADAPTER_MISMATCH",
    });
  }
  let inputNormSquared = 0;
  for (const value of embedding) {
    inputNormSquared += value * value;
  }
  if (!(inputNormSquared > 0) || !Number.isFinite(inputNormSquared)) {
    throw new VerseRecallSearchError("embedding must have a finite non-zero norm", {
      code: "INVALID_EMBEDDING",
    });
  }
  const inputScale = 1 / Math.sqrt(inputNormSquared);
  // VRQADP01 uses Float64 intermediates and rounds only the normalized result
  // to Float32.  Accumulating directly into Float32Array makes cancellation
  // dependent on the host implementation and can erase low-order evidence.
  const accumulated = new Float64Array(adapter.outputDims);
  for (let input = 0; input < adapter.inputDims; input += 1) {
    const value = embedding[input] * inputScale;
    const offset = input * adapter.outputDims;
    for (let outputIndex = 0; outputIndex < adapter.outputDims; outputIndex += 1) {
      accumulated[outputIndex] += value * adapter.matrix[offset + outputIndex];
    }
  }
  let outputNormSquared = 0;
  for (const value of accumulated) {
    outputNormSquared += value * value;
  }
  if (!(outputNormSquared > 0) || !Number.isFinite(outputNormSquared)) {
    throw new VerseRecallSearchError("query adapter produced an invalid vector", {
      code: "INVALID_QUERY_ADAPTER_OUTPUT",
    });
  }
  const outputScale = 1 / Math.sqrt(outputNormSquared);
  const output = new Float32Array(adapter.outputDims);
  for (let index = 0; index < output.length; index += 1) {
    output[index] = accumulated[index] * outputScale;
  }
  return output;
}

/**
 * @typedef {object} SearchOpts
 * @property {number} [topK]
 * @property {string} [bookHint]
 */

/**
 * @typedef {object} InitOptions
 * @property {object|Promise<object>|(() => object|Promise<object>)} [bindings]
 *   A wasm-bindgen namespace or lazy loader. Primarily useful for custom
 *   runtimes and tests; the packaged bindings are used by default.
 * @property {unknown} [wasm]
 *   Optional input forwarded to wasm-bindgen's default initializer.
 */

/**
 * Tip-of-the-tongue scripture search engine backed by WebAssembly.
 */
export class VerseRecall {
  /**
   * @param {(ArrayBuffer|ArrayBufferView)[]} packBuffers
   * @param {InitOptions} [options]
   */
  constructor(packBuffers, options = {}) {
    if (!Array.isArray(packBuffers) || packBuffers.length === 0) {
      throw new VerseRecallInitError("at least one pack buffer is required", {
        code: "NO_PACKS",
      });
    }
    for (const pack of packBuffers) {
      if (!isBufferSource(pack) || pack.byteLength === 0) {
        throw new VerseRecallInitError(
          "every pack must be a non-empty ArrayBuffer or ArrayBufferView",
          { code: "INVALID_PACK_BUFFER" },
        );
      }
    }
    if (options === null || typeof options !== "object") {
      throw new VerseRecallInitError("initialization options must be an object", {
        code: "INVALID_OPTIONS",
      });
    }

    this._packs = packBuffers.map((pack) =>
      pack instanceof ArrayBuffer
        ? pack
        : new Uint8Array(pack.buffer, pack.byteOffset, pack.byteLength),
    );
    this._options = { ...options };
    this._engine = null;
    this._initPromise = null;
    this._disposed = false;
  }

  /**
   * Construct and initialize an engine in one call.
   *
   * @param {(ArrayBuffer|ArrayBufferView)[]} packBuffers
   * @param {InitOptions} [options]
   * @returns {Promise<VerseRecall>}
   */
  static async create(packBuffers, options = {}) {
    const recall = new VerseRecall(packBuffers, options);
    await recall.init();
    return recall;
  }

  /** Whether the WASM engine has been opened successfully. */
  get initialized() {
    return this._engine !== null;
  }

  /** Whether this wrapper has been permanently disposed. */
  get disposed() {
    return this._disposed;
  }

  /**
   * Lazy-load and initialize WASM, then copy the supplied packs into the Rust
   * engine. Concurrent calls share one initialization attempt. A failed attempt
   * may be retried; a disposed engine may not.
   *
   * @param {InitOptions} [options]
   * @returns {Promise<void>}
   */
  async init(options = {}) {
    this._assertNotDisposed();
    if (this._engine) {
      return;
    }
    if (options === null || typeof options !== "object") {
      throw new VerseRecallInitError("initialization options must be an object", {
        code: "INVALID_OPTIONS",
      });
    }
    if (this._initPromise) {
      return this._initPromise;
    }

    const resolvedOptions = { ...this._options, ...options };
    const attempt = this._initialize(resolvedOptions);
    this._initPromise = attempt;
    try {
      await attempt;
    } finally {
      if (this._initPromise === attempt) {
        this._initPromise = null;
      }
    }
  }

  /** @param {InitOptions} options */
  async _initialize(options) {
    let bindings;
    try {
      bindings = await resolveBindings(options.bindings, options.wasm);
      validateBindings(bindings);
    } catch (error) {
      if (error instanceof VerseRecallError) {
        throw error;
      }
      throw new VerseRecallInitError(`failed to initialize WASM: ${messageOf(error)}`, {
        code: "WASM_INIT_FAILED",
        cause: error,
      });
    }

    let actualAbi;
    try {
      actualAbi = bindings.abiVersion();
    } catch (error) {
      throw new VerseRecallInitError(`failed to read WASM ABI version: ${messageOf(error)}`, {
        code: "ABI_CHECK_FAILED",
        cause: error,
      });
    }
    if (actualAbi !== ABI_VERSION) {
      throw new VerseRecallInitError(
        `WASM ABI ${actualAbi} is incompatible with JavaScript ABI ${ABI_VERSION}`,
        { code: "ABI_MISMATCH" },
      );
    }

    if (this._disposed) {
      throw new VerseRecallInitError("engine was disposed during initialization", {
        code: "DISPOSED",
      });
    }

    let engine;
    try {
      engine = bindings.openEngine(this._packs);
      validateEngine(engine);
    } catch (error) {
      safeFree(engine);
      throw new VerseRecallInitError(`failed to open recall packs: ${messageOf(error)}`, {
        code: "PACK_OPEN_FAILED",
        cause: error,
      });
    }

    if (this._disposed) {
      safeFree(engine);
      throw new VerseRecallInitError("engine was disposed during initialization", {
        code: "DISPOSED",
      });
    }

    this._engine = engine;
    // Rust owns pack copies after openEngine returns, so the wrapper need not
    // retain another reference. Callers remain free to retain their buffers.
    this._packs = [];
  }

  /**
   * @param {string} query
   * @param {SearchOpts} [opts]
   * @returns {Promise<object>}
   */
  async search(query, opts = {}) {
    const normalized = normalizeSearchInput(query, opts);
    const engine = await this._readyEngine();
    let json;
    try {
      json = engine.search(normalized.query, normalized.topK, normalized.bookHint);
    } catch (error) {
      throw new VerseRecallSearchError(`search failed: ${messageOf(error)}`, {
        code: "SEARCH_FAILED",
        cause: error,
      });
    }
    return parseResponse(json);
  }

  /**
   * Search the optional semantic pack using an embedding produced by the
   * matching offline host model.
   *
   * @param {string} query
   * @param {Float32Array} embedding
   * @param {SearchOpts} [opts]
   * @returns {Promise<object>}
   */
  async searchWithEmbedding(query, embedding, opts = {}) {
    validateEmbedding(embedding);

    const normalized = normalizeSearchInput(query, opts);
    const engine = await this._readyEngine();
    let json;
    try {
      json = engine.searchWithEmbedding(
        normalized.query,
        embedding,
        normalized.topK,
        normalized.bookHint,
      );
    } catch (error) {
      throw new VerseRecallSearchError(`semantic search failed: ${messageOf(error)}`, {
        code: "SEARCH_FAILED",
        cause: error,
      });
    }
    return parseResponse(json);
  }

  /**
   * Search an LVEC v2 pack after validating the complete encoder contract.
   * The ID is exposed by denseMetadata() and by the build-time space-id tool.
   *
   * @param {string} query
   * @param {Uint8Array|string} embeddingSpaceId 32 bytes or 64 hex characters
   * @param {Float32Array} embedding
   * @param {SearchOpts} [opts]
   * @returns {Promise<object>}
   */
  async searchWithEmbeddingInSpace(query, embeddingSpaceId, embedding, opts = {}) {
    validateEmbedding(embedding);
    const spaceId = normalizeSpaceId(embeddingSpaceId);
    const normalized = normalizeSearchInput(query, opts);
    const engine = await this._readyEngine();
    let json;
    try {
      json = engine.searchWithEmbeddingInSpace(
        normalized.query,
        spaceId,
        embedding,
        normalized.topK,
        normalized.bookHint,
      );
    } catch (error) {
      throw new VerseRecallSearchError(`semantic search failed: ${messageOf(error)}`, {
        code: "SEARCH_FAILED",
        cause: error,
      });
    }
    return parseResponse(json);
  }

  /**
   * Search a compound query with the whole-query vector and vectors for every
   * clause returned by planQuerySegments().
   *
   * @param {string} query
   * @param {Uint8Array|string} embeddingSpaceId
   * @param {Float32Array} wholeEmbedding
   * @param {{text: string, embedding: Float32Array}[]} segments
   * @param {SearchOpts} [opts]
   * @returns {Promise<object>}
   */
  async searchWithSegmentEmbeddingsInSpace(
    query,
    embeddingSpaceId,
    wholeEmbedding,
    segments,
    opts = {},
  ) {
    validateEmbedding(wholeEmbedding);
    validateSegments(segments);
    const spaceId = normalizeSpaceId(embeddingSpaceId);
    const normalized = normalizeSearchInput(query, opts);
    const engine = await this._readyEngine();
    if (typeof engine.searchWithSegmentEmbeddingsInSpace !== "function") {
      throw new VerseRecallSearchError("WASM engine does not expose segmented search", {
        code: "UNSUPPORTED_FEATURE",
      });
    }
    let json;
    try {
      json = engine.searchWithSegmentEmbeddingsInSpace(
        normalized.query,
        spaceId,
        wholeEmbedding,
        segments.map((segment) => segment.text),
        segments.map((segment) => segment.embedding),
        normalized.topK,
        normalized.bookHint,
      );
    } catch (error) {
      throw new VerseRecallSearchError(`segmented search failed: ${messageOf(error)}`, {
        code: "SEARCH_FAILED",
        cause: error,
      });
    }
    return parseResponse(json);
  }

  /** @returns {Promise<object|null>} */
  async denseMetadata() {
    const engine = await this._readyEngine();
    try {
      return parseJsonValue(engine.denseMetadata(), "dense metadata");
    } catch (error) {
      if (error instanceof VerseRecallError) {
        throw error;
      }
      throw new VerseRecallSearchError(`failed to read dense metadata: ${messageOf(error)}`, {
        code: "INVALID_RESPONSE",
        cause: error,
      });
    }
  }

  /** Release the Rust/WASM engine. Idempotent. */
  dispose() {
    if (this._disposed) {
      return;
    }
    this._disposed = true;
    const engine = this._engine;
    this._engine = null;
    this._packs = [];
    safeFree(engine);
  }

  async _readyEngine() {
    this._assertNotDisposed();
    if (!this._engine) {
      await this.init();
    }
    this._assertNotDisposed();
    if (!this._engine) {
      throw new VerseRecallInitError("engine initialization did not produce an engine", {
        code: "WASM_INIT_FAILED",
      });
    }
    return this._engine;
  }

  _assertNotDisposed() {
    if (this._disposed) {
      throw new VerseRecallError("engine has been disposed", { code: "DISPOSED" });
    }
  }
}

/**
 * Fetch a pack into an ArrayBuffer. HTTP and transport failures use a distinct
 * error class so hosts can distinguish asset delivery from search failures.
 *
 * @param {RequestInfo|URL} input
 * @param {RequestInit} [init]
 * @returns {Promise<ArrayBuffer>}
 */
export async function fetchPack(input, init = undefined) {
  let response;
  try {
    response = await fetch(input, init);
  } catch (error) {
    throw new VerseRecallFetchError(`pack request failed: ${messageOf(error)}`, {
      code: "FETCH_FAILED",
      cause: error,
    });
  }
  if (!response.ok) {
    throw new VerseRecallFetchError(
      `pack request returned HTTP ${response.status} ${response.statusText}`.trim(),
      { code: "HTTP_ERROR" },
    );
  }
  try {
    const buffer = await response.arrayBuffer();
    if (buffer.byteLength === 0) {
      throw new Error("response body is empty");
    }
    return buffer;
  } catch (error) {
    throw new VerseRecallFetchError(`failed to read pack response: ${messageOf(error)}`, {
      code: "FETCH_BODY_FAILED",
      cause: error,
    });
  }
}

/** @param {unknown} source @param {unknown} wasmInput */
async function resolveBindings(source, wasmInput) {
  if (source === undefined) {
    return loadGeneratedBindings(wasmInput);
  }
  const loaded = await (typeof source === "function" ? source() : source);
  return initializeBindings(loaded, wasmInput);
}

/** @param {unknown} wasmInput */
function loadGeneratedBindings(wasmInput) {
  if (!generatedBindingsPromise) {
    const attempt = import("../pkg/recall_wasm.js")
      .then((bindings) => initializeBindings(bindings, wasmInput))
      .catch((error) => {
        if (generatedBindingsPromise === attempt) {
          generatedBindingsPromise = undefined;
        }
        throw error;
      });
    generatedBindingsPromise = attempt;
  }
  return generatedBindingsPromise;
}

/** @param {unknown} bindings @param {unknown} wasmInput */
async function initializeBindings(bindings, wasmInput) {
  if ((typeof bindings !== "object" && typeof bindings !== "function") || bindings === null) {
    throw new VerseRecallInitError("bindings loader did not return a module namespace", {
      code: "INVALID_BINDINGS",
    });
  }
  if (typeof bindings.default !== "function") {
    return bindings;
  }

  let promise = initializedBindings.get(bindings);
  if (!promise) {
    promise = Promise.resolve(bindings.default(wasmInput))
      .then(() => bindings)
      .catch((error) => {
        initializedBindings.delete(bindings);
        throw error;
      });
    initializedBindings.set(bindings, promise);
  }
  return promise;
}

/** @param {unknown} bindings */
function validateBindings(bindings) {
  if (
    !bindings ||
    typeof bindings.openEngine !== "function" ||
    typeof bindings.abiVersion !== "function"
  ) {
    throw new VerseRecallInitError(
      "WASM bindings must export openEngine() and abiVersion()",
      { code: "INVALID_BINDINGS" },
    );
  }
}

/** @param {unknown} engine */
function validateEngine(engine) {
  if (
    !engine ||
    typeof engine.search !== "function" ||
    typeof engine.searchWithEmbedding !== "function" ||
    typeof engine.searchWithEmbeddingInSpace !== "function" ||
    typeof engine.denseMetadata !== "function" ||
    typeof engine.free !== "function"
  ) {
    throw new VerseRecallInitError(
      "openEngine() returned an incompatible engine object",
      { code: "INVALID_BINDINGS" },
    );
  }
}

/** @param {unknown} embedding */
function validateEmbedding(embedding) {
  if (!(embedding instanceof Float32Array) || embedding.length === 0) {
    throw new VerseRecallSearchError("embedding must be a non-empty Float32Array", {
      code: "INVALID_EMBEDDING",
    });
  }
  for (const value of embedding) {
    if (!Number.isFinite(value)) {
      throw new VerseRecallSearchError("embedding values must all be finite", {
        code: "INVALID_EMBEDDING",
      });
    }
  }
}

/** @param {unknown} segments */
function validateSegments(segments) {
  if (!Array.isArray(segments) || segments.length === 0 || segments.length > 4) {
    throw new VerseRecallSearchError("segments must contain between one and four clauses", {
      code: "INVALID_SEGMENTS",
    });
  }
  for (const [index, segment] of segments.entries()) {
    if (
      !segment ||
      typeof segment !== "object" ||
      typeof segment.text !== "string" ||
      segment.text.trim().length === 0 ||
      segment.text.includes("\0")
    ) {
      throw new VerseRecallSearchError(`segment ${index} has invalid text`, {
        code: "INVALID_SEGMENTS",
      });
    }
    validateEmbedding(segment.embedding);
  }
}

/** @param {unknown} encoded */
function parseSegmentPlan(encoded) {
  const value = parseJsonValue(encoded, "clause plan");
  if (
    !value ||
    typeof value !== "object" ||
    value.planner !== QUERY_SEGMENT_PLANNER ||
    !Array.isArray(value.segments) ||
    value.segments.some((segment) => typeof segment !== "string")
  ) {
    throw new VerseRecallSearchError("WASM returned an invalid clause plan", {
      code: "INVALID_RESPONSE",
    });
  }
  return value;
}

/** @param {unknown} value */
function normalizeSpaceId(value) {
  if (value instanceof Uint8Array && value.length === 32) {
    return value;
  }
  if (typeof value === "string" && /^[0-9a-fA-F]{64}$/.test(value)) {
    const bytes = new Uint8Array(32);
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Number.parseInt(value.slice(index * 2, index * 2 + 2), 16);
    }
    return bytes;
  }
  throw new VerseRecallSearchError(
    "embeddingSpaceId must be 32 bytes or exactly 64 hexadecimal characters",
    { code: "INVALID_EMBEDDING_SPACE" },
  );
}

/** @param {unknown} query @param {unknown} opts */
function normalizeSearchInput(query, opts) {
  if (typeof query !== "string" || query.trim().length === 0) {
    throw new VerseRecallSearchError("query must be a non-empty string", {
      code: "INVALID_QUERY",
    });
  }
  if (opts === null || typeof opts !== "object") {
    throw new VerseRecallSearchError("search options must be an object", {
      code: "INVALID_OPTIONS",
    });
  }
  const topK = opts.topK ?? DEFAULT_TOP_K;
  if (!Number.isInteger(topK) || topK < 1 || topK > MAX_TOP_K) {
    throw new VerseRecallSearchError(`topK must be an integer from 1 to ${MAX_TOP_K}`, {
      code: "INVALID_OPTIONS",
    });
  }
  const bookHint = opts.bookHint;
  if (bookHint !== undefined && typeof bookHint !== "string") {
    throw new VerseRecallSearchError("bookHint must be a string when provided", {
      code: "INVALID_OPTIONS",
    });
  }
  return {
    query,
    topK,
    bookHint: bookHint?.trim() || undefined,
  };
}

/** @param {unknown} json */
function parseResponse(json) {
  if (typeof json !== "string") {
    throw new VerseRecallSearchError("WASM engine returned a non-string response", {
      code: "INVALID_RESPONSE",
    });
  }
  try {
    return JSON.parse(json);
  } catch (error) {
    throw new VerseRecallSearchError("WASM engine returned invalid JSON", {
      code: "INVALID_RESPONSE",
      cause: error,
    });
  }
}

/** @param {unknown} json @param {string} label */
function parseJsonValue(json, label) {
  if (typeof json !== "string") {
    throw new VerseRecallSearchError(`WASM engine returned non-string ${label}`, {
      code: "INVALID_RESPONSE",
    });
  }
  try {
    return JSON.parse(json);
  } catch (error) {
    throw new VerseRecallSearchError(`WASM engine returned invalid ${label} JSON`, {
      code: "INVALID_RESPONSE",
      cause: error,
    });
  }
}

/** @param {unknown} value */
function isBufferSource(value) {
  return value instanceof ArrayBuffer || ArrayBuffer.isView(value);
}

/** @param {unknown} engine */
function safeFree(engine) {
  try {
    engine?.free?.();
  } catch {
    // Disposal is best-effort and idempotent. A wasm-bindgen object that has
    // already been freed must not make host cleanup fail.
  }
}

/** @param {unknown} error */
function messageOf(error) {
  return error instanceof Error ? error.message : String(error);
}

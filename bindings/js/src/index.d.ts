export interface SearchOpts {
  topK?: number;
  bookHint?: string;
}

export interface QuerySegmentPlan {
  planner: "verse-recall-clause-v2";
  segments: string[];
}

export interface QuerySegmentEmbedding {
  text: string;
  embedding: Float32Array;
}

export interface LaneScores {
  a: number;
  b: number;
  c: number;
  d: number;
}

export interface PassageRange {
  start_ref: string;
  end_ref: string;
}

export interface SearchResult {
  ref: string;
  text: string;
  /** Opaque response-local ordering score; do not persist or threshold across engine versions. */
  score: number;
  confidence: "recovered" | "related" | "often-misremembered";
  lanes: LaneScores;
  matched_terms: string[];
  expanded_terms: string[];
  /** Structured concept/event passages; absent for ordinary verse hits. */
  passages?: PassageRange[];
  chips: string[];
}

export interface SearchResponse {
  query: string;
  results: SearchResult[];
}

export interface DenseMetadata {
  version: number;
  dims: number;
  verseCount: number;
  tableCount: number;
  /** Aligned representation roles in table order. */
  tableRoles: Array<"verse" | "window" | "study-note">;
  /** Lowercase SHA-256-style identifier; absent only for legacy LVEC v1. */
  embeddingSpaceId: string | null;
}

export interface QueryAdapter {
  inputDims: number;
  outputDims: number;
  /** Row-major inputDims × outputDims matrix. */
  matrix: Float32Array;
}

export interface RawWasmEngine {
  search(query: string, topK?: number, bookHint?: string): string;
  searchWithEmbedding(
    query: string,
    embedding: Float32Array,
    topK?: number,
    bookHint?: string,
  ): string;
  searchWithEmbeddingInSpace(
    query: string,
    embeddingSpaceId: Uint8Array,
    embedding: Float32Array,
    topK?: number,
    bookHint?: string,
  ): string;
  searchWithSegmentEmbeddingsInSpace?(
    query: string,
    embeddingSpaceId: Uint8Array,
    wholeEmbedding: Float32Array,
    segmentTexts: string[],
    segmentEmbeddings: Float32Array[],
    topK?: number,
    bookHint?: string,
  ): string;
  denseMetadata(): string;
  verseCount(): number;
  translationId(): string;
  free(): void;
}

export interface WasmBindings {
  default?: (wasm?: unknown) => unknown | Promise<unknown>;
  openEngine(packs: (ArrayBuffer | ArrayBufferView)[]): RawWasmEngine;
  planQuerySegments?(query: string): string;
  abiVersion(): number;
}

export type WasmBindingsSource =
  | WasmBindings
  | Promise<WasmBindings>
  | (() => WasmBindings | Promise<WasmBindings>);

export interface InitOptions {
  /** Custom wasm-bindgen namespace or lazy loader. The packaged module is used by default. */
  bindings?: WasmBindingsSource;
  /** Optional input forwarded to wasm-bindgen's default initializer. */
  wasm?: unknown;
}

export declare class VerseRecallError extends Error {
  readonly code: string;
  constructor(message: string, options?: { code?: string; cause?: unknown });
}

export declare class VerseRecallInitError extends VerseRecallError {}
export declare class VerseRecallSearchError extends VerseRecallError {}
export declare class VerseRecallFetchError extends VerseRecallError {}

export declare function parseQueryAdapter(
  source: ArrayBuffer | ArrayBufferView,
): QueryAdapter;

export declare function applyQueryAdapter(
  embedding: Float32Array,
  adapter: QueryAdapter,
): Float32Array;

export declare class VerseRecall {
  constructor(packBuffers: (ArrayBuffer | ArrayBufferView)[], options?: InitOptions);
  static create(
    packBuffers: (ArrayBuffer | ArrayBufferView)[],
    options?: InitOptions,
  ): Promise<VerseRecall>;
  readonly initialized: boolean;
  readonly disposed: boolean;
  init(options?: InitOptions): Promise<void>;
  search(query: string, opts?: SearchOpts): Promise<SearchResponse>;
  searchWithEmbedding(
    query: string,
    embedding: Float32Array,
    opts?: SearchOpts,
  ): Promise<SearchResponse>;
  searchWithEmbeddingInSpace(
    query: string,
    embeddingSpaceId: Uint8Array | string,
    embedding: Float32Array,
    opts?: SearchOpts,
  ): Promise<SearchResponse>;
  searchWithSegmentEmbeddingsInSpace(
    query: string,
    embeddingSpaceId: Uint8Array | string,
    wholeEmbedding: Float32Array,
    segments: QuerySegmentEmbedding[],
    opts?: SearchOpts,
  ): Promise<SearchResponse>;
  denseMetadata(): Promise<DenseMetadata | null>;
  dispose(): void;
}

export declare function fetchPack(
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<ArrayBuffer>;

export declare function planQuerySegments(
  query: string,
  options?: InitOptions,
): Promise<QuerySegmentPlan>;

export declare const ABI_VERSION: 1;
export declare const QUERY_SEGMENT_PLANNER: "verse-recall-clause-v2";

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  ABI_VERSION,
  QUERY_SEGMENT_PLANNER,
  VerseRecall,
  VerseRecallError,
  VerseRecallFetchError,
  VerseRecallInitError,
  VerseRecallSearchError,
  applyQueryAdapter,
  fetchPack,
  parseQueryAdapter,
  planQuerySegments,
} from "../src/index.js";

function queryAdapter(inputDims, outputDims, values) {
  const bytes = new ArrayBuffer(16 + values.length * 4);
  const view = new DataView(bytes);
  for (const [index, value] of Array.from("VRQADP01").entries()) {
    view.setUint8(index, value.charCodeAt(0));
  }
  view.setUint32(8, inputDims, true);
  view.setUint32(12, outputDims, true);
  values.forEach((value, index) => view.setFloat32(16 + index * 4, value, true));
  return bytes;
}

function response(query, ref = "Luke 15:4") {
  return JSON.stringify({
    query,
    results: [
      {
        ref,
        text: "Suppose one of you has a hundred sheep and loses one of them.",
        score: 1,
        confidence: "related",
        lanes: { a: 1, b: 0, c: 0, d: 0 },
        matched_terms: ["lost", "sheep"],
        expanded_terms: [],
        chips: ["lost", "sheep"],
      },
    ],
  });
}

function fakeBindings(overrides = {}) {
  const state = {
    initCalls: 0,
    openCalls: 0,
    freeCalls: 0,
    searches: [],
  };
  const engine = {
    search(query, topK, bookHint) {
      state.searches.push({ kind: "lexical", query, topK, bookHint });
      return response(query);
    },
    searchWithEmbedding(query, embedding, topK, bookHint) {
      state.searches.push({
        kind: "semantic",
        query,
        embedding: Array.from(embedding),
        topK,
        bookHint,
      });
      return response(query, "Genesis 1:1");
    },
    searchWithEmbeddingInSpace(query, embeddingSpaceId, embedding, topK, bookHint) {
      state.searches.push({
        kind: "semantic-v2",
        query,
        embeddingSpaceId: Array.from(embeddingSpaceId),
        embedding: Array.from(embedding),
        topK,
        bookHint,
      });
      return response(query, "Genesis 1:1");
    },
    searchWithSegmentEmbeddingsInSpace(
      query,
      embeddingSpaceId,
      wholeEmbedding,
      segmentTexts,
      segmentEmbeddings,
      topK,
      bookHint,
    ) {
      state.searches.push({
        kind: "semantic-segments-v2",
        query,
        embeddingSpaceId: Array.from(embeddingSpaceId),
        wholeEmbedding: Array.from(wholeEmbedding),
        segmentTexts: Array.from(segmentTexts),
        segmentEmbeddings: Array.from(segmentEmbeddings, (value) => Array.from(value)),
        topK,
        bookHint,
      });
      return response(query, "Mark 12:31");
    },
    denseMetadata() {
      return JSON.stringify({
        version: 2,
        dims: 2,
        verseCount: 31_086,
        tableCount: 2,
        tableRoles: ["verse", "window"],
        embeddingSpaceId: "5a".repeat(32),
      });
    },
    verseCount() {
      return 31_086;
    },
    translationId() {
      return "BSB";
    },
    free() {
      state.freeCalls += 1;
    },
    ...overrides.engine,
  };
  const bindings = {
    async default() {
      state.initCalls += 1;
      await Promise.resolve();
    },
    abiVersion() {
      return ABI_VERSION;
    },
    openEngine(packs) {
      state.openCalls += 1;
      assert.equal(packs.length, 1);
      return engine;
    },
    planQuerySegments(query) {
      const segments = query.includes(" and ")
        ? query.split(" and ")
        : [];
      return JSON.stringify({ planner: QUERY_SEGMENT_PLANNER, segments });
    },
    ...overrides.bindings,
  };
  return { bindings, engine, state };
}

test("lazy initialization is shared and search returns parsed typed data", async () => {
  const { bindings, state } = fakeBindings();
  const recall = new VerseRecall([new Uint8Array([1, 2, 3])], { bindings });

  const [first, second] = await Promise.all([
    recall.search("lost sheep", { topK: 5, bookHint: "Luke" }),
    recall.search("lost sheep", { topK: 5, bookHint: "Luke" }),
  ]);

  assert.equal(state.initCalls, 1);
  assert.equal(state.openCalls, 1);
  assert.equal(first.results[0].ref, "Luke 15:4");
  assert.deepEqual(second, first);
  assert.equal(recall.initialized, true);
});

test("portable query adapter parses, transforms, and fails closed", () => {
  const parsed = parseQueryAdapter(queryAdapter(2, 3, [0, 1, 0, 1, 0, 1]));
  assert.equal(parsed.inputDims, 2);
  assert.equal(parsed.outputDims, 3);
  const transformed = applyQueryAdapter(new Float32Array([3, 4]), parsed);
  assert.deepEqual(
    Array.from(transformed, (value) => Math.round(value * 1_000_000) / 1_000_000),
    [0.624695, 0.468521, 0.624695],
  );
  const norm = Math.hypot(...transformed);
  assert(Math.abs(norm - 1) < 1e-6);

  assert.throws(
    () => parseQueryAdapter(new Uint8Array([1, 2, 3])),
    (error) => error instanceof VerseRecallInitError && error.code === "INVALID_QUERY_ADAPTER",
  );
  assert.throws(
    () => applyQueryAdapter(new Float32Array([1, 2, 3]), parsed),
    (error) =>
      error instanceof VerseRecallSearchError && error.code === "QUERY_ADAPTER_MISMATCH",
  );
});

test("query adapter matches the shared double-accumulation golden", () => {
  const fixture = JSON.parse(
    readFileSync(
      new URL("../../../tests/fixtures/vrqadp01-golden-v1.json", import.meta.url),
      "utf8",
    ),
  );
  assert.equal(fixture.schema, "verse-recall-vrqadp01-arithmetic-v1");
  const parsed = parseQueryAdapter(
    queryAdapter(fixture.input_dimensions, fixture.output_dimensions, fixture.matrix),
  );
  const output = applyQueryAdapter(new Float32Array(fixture.embedding), parsed);
  const bits = Array.from(output, (_, index) => {
    const buffer = new ArrayBuffer(4);
    const view = new DataView(buffer);
    view.setFloat32(0, output[index], false);
    return view.getUint32(0, false).toString(16).padStart(8, "0");
  });
  assert.deepEqual(bits, fixture.expected_f32_bits);
});

test("create supports semantic search and dispose is idempotent", async () => {
  const { bindings, state } = fakeBindings();
  const recall = await VerseRecall.create([new ArrayBuffer(8)], { bindings });
  const result = await recall.searchWithEmbedding(
    "God brought everything into being",
    new Float32Array([0.25, 0.75]),
    { topK: 3 },
  );

  assert.equal(result.results[0].ref, "Genesis 1:1");
  assert.deepEqual(state.searches[0], {
    kind: "semantic",
    query: "God brought everything into being",
    embedding: [0.25, 0.75],
    topK: 3,
    bookHint: undefined,
  });

  recall.dispose();
  recall.dispose();
  assert.equal(state.freeCalls, 1);
  assert.equal(recall.disposed, true);
  await assert.rejects(() => recall.search("lost sheep"), (error) => {
    assert(error instanceof VerseRecallError);
    assert.equal(error.code, "DISPOSED");
    return true;
  });
});

test("v2 semantic search validates its encoder identity", async () => {
  const { bindings, state } = fakeBindings();
  const recall = await VerseRecall.create([new ArrayBuffer(8)], { bindings });
  const metadata = await recall.denseMetadata();
  assert.equal(metadata.embeddingSpaceId, "5a".repeat(32));
  assert.deepEqual(metadata.tableRoles, ["verse", "window"]);
  const result = await recall.searchWithEmbeddingInSpace(
    "God made all things",
    metadata.embeddingSpaceId,
    new Float32Array([0.25, 0.75]),
    { topK: 3 },
  );
  assert.equal(result.results[0].ref, "Genesis 1:1");
  assert.equal(state.searches[0].kind, "semantic-v2");
  assert.deepEqual(state.searches[0].embeddingSpaceId, Array(32).fill(0x5a));

  await assert.rejects(
    () => recall.searchWithEmbeddingInSpace("query", "bad", new Float32Array([1, 0])),
    (error) => error.code === "INVALID_EMBEDDING_SPACE",
  );
});

test("canonical planning and segmented semantic search are portable", async () => {
  assert.equal(QUERY_SEGMENT_PLANNER, "verse-recall-clause-v2");
  const { bindings, state } = fakeBindings();
  const plan = await planQuerySegments("lost sheep and prodigal son", { bindings });
  assert.deepEqual(plan, {
    planner: QUERY_SEGMENT_PLANNER,
    segments: ["lost sheep", "prodigal son"],
  });

  const recall = await VerseRecall.create([new ArrayBuffer(8)], { bindings });
  const result = await recall.searchWithSegmentEmbeddingsInSpace(
    "lost sheep and prodigal son",
    "5a".repeat(32),
    new Float32Array([0.5, 0.5]),
    [
      { text: plan.segments[0], embedding: new Float32Array([1, 0]) },
      { text: plan.segments[1], embedding: new Float32Array([0, 1]) },
    ],
    { topK: 4, bookHint: "Luke" },
  );
  assert.equal(result.results[0].ref, "Mark 12:31");
  assert.deepEqual(state.searches[0], {
    kind: "semantic-segments-v2",
    query: "lost sheep and prodigal son",
    embeddingSpaceId: Array(32).fill(0x5a),
    wholeEmbedding: [0.5, 0.5],
    segmentTexts: ["lost sheep", "prodigal son"],
    segmentEmbeddings: [[1, 0], [0, 1]],
    topK: 4,
    bookHint: "Luke",
  });

  await assert.rejects(
    () => recall.searchWithSegmentEmbeddingsInSpace(
      "query",
      "5a".repeat(32),
      new Float32Array([1, 0]),
      [],
    ),
    (error) => error.code === "INVALID_SEGMENTS",
  );
});

test("ABI and engine-shape mismatches fail with typed initialization errors", async () => {
  const wrongAbi = fakeBindings({
    bindings: { abiVersion: () => ABI_VERSION + 1 },
  });
  await assert.rejects(
    () => VerseRecall.create([new ArrayBuffer(1)], { bindings: wrongAbi.bindings }),
    (error) => {
      assert(error instanceof VerseRecallInitError);
      assert.equal(error.code, "ABI_MISMATCH");
      return true;
    },
  );

  const badEngine = fakeBindings({
    bindings: { openEngine: () => ({ search() {} }) },
  });
  await assert.rejects(
    () => VerseRecall.create([new ArrayBuffer(1)], { bindings: badEngine.bindings }),
    (error) => {
      assert(error instanceof VerseRecallInitError);
      assert.equal(error.code, "PACK_OPEN_FAILED");
      return true;
    },
  );
});

test("search validates inputs and wraps invalid engine JSON", async () => {
  const { bindings } = fakeBindings({
    engine: { search: () => "not json" },
  });
  const recall = await VerseRecall.create([new ArrayBuffer(1)], { bindings });

  await assert.rejects(() => recall.search(" "), (error) => {
    assert(error instanceof VerseRecallSearchError);
    assert.equal(error.code, "INVALID_QUERY");
    return true;
  });
  await assert.rejects(() => recall.search("query", { topK: 101 }), (error) => {
    assert(error instanceof VerseRecallSearchError);
    assert.equal(error.code, "INVALID_OPTIONS");
    return true;
  });
  await assert.rejects(
    () => recall.searchWithEmbedding("query", new Float32Array([Number.NaN])),
    (error) => {
      assert(error instanceof VerseRecallSearchError);
      assert.equal(error.code, "INVALID_EMBEDDING");
      return true;
    },
  );
  await assert.rejects(() => recall.search("query"), (error) => {
    assert(error instanceof VerseRecallSearchError);
    assert.equal(error.code, "INVALID_RESPONSE");
    return true;
  });
});

test("failed initialization can be retried", async () => {
  const { bindings, state } = fakeBindings();
  let loads = 0;
  const recall = new VerseRecall([new ArrayBuffer(1)], {
    bindings: async () => {
      loads += 1;
      if (loads === 1) {
        throw new Error("temporary loader failure");
      }
      return bindings;
    },
  });

  await assert.rejects(() => recall.init(), (error) => {
    assert(error instanceof VerseRecallInitError);
    assert.equal(error.code, "WASM_INIT_FAILED");
    return true;
  });
  await recall.init();
  assert.equal(loads, 2);
  assert.equal(state.openCalls, 1);
});

test("disposing during asynchronous initialization never opens an engine", async () => {
  const { bindings, state } = fakeBindings();
  let releaseLoader;
  const loaderGate = new Promise((resolve) => {
    releaseLoader = resolve;
  });
  const recall = new VerseRecall([new ArrayBuffer(1)], {
    bindings: async () => {
      await loaderGate;
      return bindings;
    },
  });

  const initialization = recall.init();
  recall.dispose();
  releaseLoader();

  await assert.rejects(() => initialization, (error) => {
    assert(error instanceof VerseRecallInitError);
    assert.equal(error.code, "DISPOSED");
    return true;
  });
  assert.equal(state.openCalls, 0);
  assert.equal(state.freeCalls, 0);
});

test("fetchPack distinguishes success, HTTP failure, and transport failure", async (t) => {
  const originalFetch = globalThis.fetch;
  t.after(() => {
    globalThis.fetch = originalFetch;
  });

  globalThis.fetch = async () => new Response(new Uint8Array([1, 2, 3]), { status: 200 });
  const pack = await fetchPack("https://example.test/bsb.pack");
  assert.deepEqual(Array.from(new Uint8Array(pack)), [1, 2, 3]);

  globalThis.fetch = async () => new Response("missing", { status: 404, statusText: "Not Found" });
  await assert.rejects(() => fetchPack("https://example.test/missing.pack"), (error) => {
    assert(error instanceof VerseRecallFetchError);
    assert.equal(error.code, "HTTP_ERROR");
    return true;
  });

  globalThis.fetch = async () => {
    throw new Error("offline");
  };
  await assert.rejects(() => fetchPack("https://example.test/bsb.pack"), (error) => {
    assert(error instanceof VerseRecallFetchError);
    assert.equal(error.code, "FETCH_FAILED");
    return true;
  });
});

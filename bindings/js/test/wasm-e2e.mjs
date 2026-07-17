import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import init, {
  abiVersion,
  openEngine,
} from "../pkg/recall_wasm.js";

const packPath = process.env.VERSE_RECALL_PACK;
if (!packPath) {
  throw new Error("VERSE_RECALL_PACK must name a canonical .pack artifact");
}

const packageDirectory = resolve(import.meta.dirname, "..");
const wasm = await readFile(resolve(packageDirectory, "pkg/recall_wasm_bg.wasm"));
await init({ module_or_path: wasm });

assert.equal(abiVersion(), 1);
const pack = new Uint8Array(await readFile(resolve(packPath)));
const engine = openEngine([pack]);

try {
  assert.equal(engine.translationId(), "BSB");
  assert.equal(engine.verseCount(), 31_086);
  assert.equal(engine.denseMetadata(), "null");

  const response = JSON.parse(engine.search("the Lord is my shepherd", 3));
  assert.equal(response.results[0].ref, "Psalm 23:1");
  assert.equal(response.results[0].confidence, "recovered");
  assert.equal(response.results.length, 3);
} finally {
  engine.free();
}

process.stdout.write("real WASM engine opened 31,086 verses and recovered Psalm 23:1\n");

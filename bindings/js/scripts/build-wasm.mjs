import { createHash } from "node:crypto";
import { access, readFile, rm, writeFile } from "node:fs/promises";
import { constants } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const workspaceDir = resolve(packageDir, "../..");
const crateDir = join(workspaceDir, "crates/recall-wasm");
const outputDir = join(packageDir, "pkg");

const version = spawnSync("wasm-pack", ["--version"], {
  cwd: workspaceDir,
  encoding: "utf8",
});
if (version.error?.code === "ENOENT") {
  throw new Error(
    "wasm-pack is required to build @selah-tools/verse-recall. " +
      "Install it from https://rustwasm.github.io/wasm-pack/installer/",
  );
}
if (version.error) {
  throw version.error;
}
if (version.status !== 0) {
  throw new Error(`wasm-pack --version failed: ${(version.stderr || version.stdout).trim()}`);
}

await rm(outputDir, { recursive: true, force: true });
const build = spawnSync(
  "wasm-pack",
  [
    "build",
    crateDir,
    "--target",
    "web",
    "--release",
    "--out-dir",
    outputDir,
    "--out-name",
    "recall_wasm",
  ],
  { cwd: workspaceDir, encoding: "utf8", stdio: "inherit" },
);
if (build.error) {
  throw build.error;
}
if (build.status !== 0) {
  throw new Error(`wasm-pack build failed with exit status ${build.status}`);
}

// wasm-pack places `*` in a generated pkg/.gitignore. npm also treats nested
// .gitignore files as package exclusion rules, so leaving it in place silently
// drops the complete WASM payload from `npm pack` even though `pkg` appears in
// this package's `files` allowlist. The workspace root already ignores build
// products; the publish directory must not carry that sentinel.
await rm(join(outputDir, ".gitignore"), { force: true });

const required = [
  "recall_wasm.js",
  "recall_wasm.d.ts",
  "recall_wasm_bg.wasm",
  "recall_wasm_bg.wasm.d.ts",
  "package.json",
];
for (const file of required) {
  await access(join(outputDir, file), constants.R_OK);
}

// The nested package boundary must explicitly remain ESM for Node and bundlers,
// even if an older wasm-pack template omits `type` and only supplies `module`.
const generatedPackagePath = join(outputDir, "package.json");
const generatedPackage = JSON.parse(await readFile(generatedPackagePath, "utf8"));
generatedPackage.type = "module";
await writeFile(generatedPackagePath, `${JSON.stringify(generatedPackage, null, 2)}\n`, "utf8");

const files = {};
for (const file of required) {
  const bytes = await readFile(join(outputDir, file));
  files[file] = {
    bytes: bytes.byteLength,
    sha256: createHash("sha256").update(bytes).digest("hex"),
  };
}
const manifest = {
  schema: 1,
  crate: "recall-wasm",
  target: "web",
  profile: "release",
  wasmPack: (version.stdout || version.stderr).trim(),
  files,
};
await writeFile(
  join(outputDir, "build-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
  "utf8",
);

process.stdout.write(
  `generated ${required.length} WASM package files (${files["recall_wasm_bg.wasm"].bytes} byte module)\n`,
);

#!/usr/bin/env node
// Regenerate the committed Zod golden files for the valid-zod-matrix IR
// fixture (issue #45, plan §5). Invoked explicitly by maintainers and
// reviewed in PRs — never run inside test paths. Writes only under
// tests/fixtures/ir/valid-zod-matrix/expected/.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixtureDir = join(root, "tests", "fixtures", "ir", "valid-zod-matrix");

const { mapProject } = await import(
  pathToFileURL(join(root, "adapters", "node-typescript", "src", "zod-map.mjs"))
);
const { emitFiles, sha256 } = await import(
  pathToFileURL(join(root, "adapters", "node-typescript", "src", "zod-emit.mjs"))
);

const bytes = readFileSync(join(fixtureDir, "ir.json"));
const ir = JSON.parse(bytes.toString("utf8"));
const files = emitFiles({
  modules: mapProject(ir).modules,
  inputDigest: sha256(bytes.toString("utf8")),
  adapterVersion: "0.4.0",
  irIdentity: "dev.lekalo.ir@0.2.16",
});

const expectedDir = join(fixtureDir, "expected");
rmSync(expectedDir, { recursive: true, force: true });
mkdirSync(expectedDir, { recursive: true });
for (const emitted of files) {
  writeFileSync(join(expectedDir, emitted.path.split("/").pop()), emitted.text);
}
process.stdout.write(
  JSON.stringify({
    ok: true,
    regenerated: files.map((emitted) => emitted.path.split("/").pop()),
  }) + "\n",
);

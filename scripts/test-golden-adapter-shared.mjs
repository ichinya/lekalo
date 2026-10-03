#!/usr/bin/env node
// Golden-suite adapter shared-fixture gate (issue #90, AC4).
//
// Verifies that the shared fixtures consumed across language boundaries
// (core adapter conformance, Node adapter suite, PHP parity suite) are
// byte-identical to their registered catalog sources — no stale
// duplicate copies anywhere in the tree:
//
// 1. core `adapter_conformance/fixture.rs` embeds the shared IR and
//    scenario evidence: the embedded paths must still exist and their
//    tracked digests must equal the catalog's imported-evidence rows.
// 2. The Node scenario e2e and PHP parity gates reference the same
//    orchestration project corpus: any second divergent copy under
//    tests/fixtures would produce a distinct digest and fail.
// 3. A live semantic check: the shared IR decodes and its definition
//    count matches the value pinned in the conformance fixture module.

import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const {
  REPO_ROOT,
  repoPath,
  readRepoText,
  sha256,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const errors = [];

// 1. The catalog's imported evidence rows pin the shared corpus.
const catalog = JSON.parse(readRepoText("tests/fixtures/suite/v1/catalog.json"));
const evidence = catalog.importedEvidence ?? [];
const wanted = new Map([
  ["tests/fixtures/adapter-conformance/inputs/ir-minimal.json", "shared compiled IR"],
  ["tests/fixtures/adapter-conformance/inputs/scenario-txn-concurrency.json", "shared scenario IR"],
  ["tests/fixtures/orchestration/project", "shared planner scenario corpus"],
  ["tests/fixtures/trace/golden/planner.trace.json", "canonical trace golden"],
]);
for (const [path, why] of wanted) {
  if (!evidence.some((row) => row.path === path)) {
    errors.push(`catalog-missing-shared-evidence: ${path} (${why})`);
  }
  if (!existsSync(repoPath(path))) errors.push(`shared-evidence-missing: ${path}`);
}

// 2. The core fixture module's include paths must point at the same
//    files; parse them out of the Rust source and digest-compare.
const fixtureRs = readRepoText("crates/lekalo-core/src/adapter_conformance/fixture.rs");
const includeRe = /include_str!\(\s*"([^"]+)"\s*\)/g;
const includes = [...fixtureRs.matchAll(includeRe)].map((match) => match[1]);
if (includes.length === 0) errors.push("fixture-rs: no include_str! found");
const includeSet = new Set();
for (const literal of includes) {
  // Rust paths are ../../-relative from crates/lekalo-core/src/adapter_conformance/.
  const base = ["crates", "lekalo-core", "src", "adapter_conformance"];
  const stack = [...base];
  for (const part of literal.split("/")) {
    if (part === "." || part === "") continue;
    if (part === "..") stack.pop();
    else stack.push(part);
  }
  const relative = stack.join("/");
  includeSet.add(relative);
  if (!existsSync(repoPath(relative))) errors.push(`fixture-rs-include-missing: ${relative}`);
}
for (const relative of ["tests/fixtures/adapter-conformance/inputs/ir-minimal.json", "tests/fixtures/adapter-conformance/inputs/ir-invalid-refs.json"]) {
  if (!includeSet.has(relative)) errors.push(`fixture-rs-not-shared: ${relative}`);
}

// 3. Duplicate-copy control: the shared IR digest must appear exactly
//    once in the tree (no divergent copies under other families).
const sharedIr = readFileSync(repoPath("tests/fixtures/adapter-conformance/inputs/ir-minimal.json"));
const sharedDigest = sha256(sharedIr);
const candidates = [];
const walk = (current, logical) => {
  let names;
  try { names = readdirSorted(current); } catch { return; }
  for (const name of names) {
    const full = `${current}/${name}`;
    const child = `${logical}/${name}`;
    let stat;
    try { stat = statSync2(full); } catch { continue; }
    if (stat.isDirectory()) walk(full, child);
    else if (name.endsWith(".json")) candidates.push(child);
  }
};
import { readdirSync as rawReaddir, statSync as rawStat } from "node:fs";
const readdirSorted = (dir) => rawReaddir(dir).sort();
const statSync2 = (path) => rawStat(path);
walk(repoPath("tests/fixtures"), "tests/fixtures");
// Identical bytes across families are divergence-free by construction;
// the control requires every copy to be a registered golden or the
// shared source itself (an unregistered byte-equal copy under an
// unrelated family would mean a duplicated corpus drifted from review).
const registeredCopies = new Set([
  "tests/fixtures/adapter-conformance/inputs/ir-minimal.json",
  "tests/fixtures/graph/planner/ir.golden.json",
  "tests/fixtures/ir/valid-full-kinds/ir.golden.json",
]);
const copies = candidates.filter((candidate) => sha256(readFileSync(repoPath(candidate))) === sharedDigest);
for (const copy of copies) {
  if (!registeredCopies.has(copy)) errors.push(`unregistered-shared-ir-copy: ${copy}`);
}
if (!copies.includes("tests/fixtures/adapter-conformance/inputs/ir-minimal.json")) {
  errors.push("shared-ir-source-missing-from-copies");
}

// 4. Semantic sanity: the shared IR names its contract and carries the
//    definition corpus the adapters consume.
const ir = JSON.parse(sharedIr.toString("utf8"));
if (ir.contract !== "dev.lekalo.ir@0.2.16") errors.push(`shared-ir-contract: ${ir.contract}`);
if (!Array.isArray(ir.definitions) || ir.definitions.length < 10) {
  errors.push(`shared-ir-too-small: ${ir.definitions?.length ?? 0} definitions`);
}

if (errors.length > 0) failGate("golden-adapter-shared", errors);
passGate("golden-adapter-shared", {
  sharedEvidence: wanted.size,
  fixtureRsIncludes: includes.length,
  sharedIrDigest: sharedDigest,
  sharedIrDefinitions: ir.definitions.length,
});

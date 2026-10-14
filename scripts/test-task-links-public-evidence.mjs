#!/usr/bin/env node
// Issue #52 public-evidence gate: every public artifact of the
// task-external-links family carries synthetic identities only, the
// synthetic canary token stays at the runtime boundary (it reaches no
// payload, evidence, report, portability, log, or export surface, and
// no CLI output), no private consumer application name appears
// anywhere, and provenance/catalog/coverage registration is honest.

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const gate = "task-links-public-evidence";
const failures = [];
const fail = (reason, detail) => failures.push({ reason, detail });

const CANARY = "LEKALO-SYNTHETIC-TOKEN-CANARY-0F0F";
const HANDLE = "handle-primary-1";
const RUNTIME_BOUNDARY =
  "tests/fixtures/task-external-links/providers/github-issues/runtime-boundary.json";
// The private consumer application of the observed pilot stays unnamed
// in this family (its own sanctioned fixtures live elsewhere; #52
// public artifacts must not reveal a consumer application name).
const FORBIDDEN_NAMES = ["taskhub"];

// 1. The canary exists exactly once (the runtime boundary fixture) and
//    nowhere else in the entire family.
const fixtureRoot = join(root, "tests/fixtures/task-external-links");
const files = [];
const walk = (current) => {
  for (const name of readdirSync(current).sort()) {
    const full = join(current, name);
    if (statSync(full).isDirectory()) walk(full);
    else files.push(full);
  }
};
walk(fixtureRoot);
for (const file of files) {
  const text = readFileSync(file, "utf8");
  const relative = file.slice(root.length + 1);
  if (text.includes(CANARY)) {
    if (!relative.replace(/\\/g, "/").endsWith(RUNTIME_BOUNDARY)) {
      fail("canary-leak", relative);
    }
  }
  if (text.includes(HANDLE) && !relative.includes("runtime-boundary")) {
    fail("credential-handle-leak", relative);
  }
  for (const name of FORBIDDEN_NAMES) {
    if (text.toLowerCase().includes(name)) {
      fail("consumer-name-leak", `${relative}: ${name}`);
    }
  }
  // No live provider endpoints: fixtures are synthetic snapshots, and
  // this family never names a fetchable provider URL.
  if (/https?:\/\//.test(text)) {
    fail("absolute-url", relative);
  }
}
if (!existsSync(join(root, RUNTIME_BOUNDARY))) fail("boundary-missing", RUNTIME_BOUNDARY);
else if (!readFileSync(join(root, RUNTIME_BOUNDARY), "utf8").includes(CANARY)) {
  fail("canary-missing-at-boundary", RUNTIME_BOUNDARY);
}

// 2. The CLI surface never emits the canary or the handle: run the
//    three commands against the boundary-adjacent fixtures and scan
//    stdout AND stderr.
const binaryCandidates = [
  join(root, "target/debug/lekalo"),
  join(root, "target/debug/lekalo.exe"),
];
const binary = binaryCandidates.find((candidate) => existsSync(candidate));
if (!binary) {
  fail("binary-missing", "build the workspace first: cargo build --workspace --locked");
} else {
  const runs = [
    ["--json", "task-links", "validate", "tests/fixtures/task-external-links/contracts/integration.json", "--project", "tests/fixtures/task-external-links/project", "--profile", "strict"],
    ["--json", "task-links", "inspect", "tasklinks.task_external_link", "--project", "tests/fixtures/task-external-links/project", "--attachment", "tests/fixtures/task-external-links/contracts/integration.json", "--evidence", "tests/fixtures/task-external-links/evidence/mock-run.json"],
    ["--json", "task-links", "portability", "tests/fixtures/task-external-links/contracts/integration.json", "--project", "tests/fixtures/task-external-links/project", "--from-profile", "tests/fixtures/task-external-links/profiles/laravel.json", "--to-profile", "tests/fixtures/task-external-links/profiles/node.json", "--evidence", "tests/fixtures/task-external-links/evidence/mock-run.json", "--profile", "permissive"],
    // A failed run leaks too: the stale-evidence refusal path.
    ["--json", "task-links", "inspect", "tasklinks.task_external_link", "--project", "tests/fixtures/task-external-links/project", "--attachment", "tests/fixtures/task-external-links/contracts/integration.json", "--evidence", "tests/fixtures/task-external-links/invalid/evidence-stale.json"],
  ];
  for (const args of runs) {
    const result = spawnSync(binary, ["--no-cache", ...args], {
      cwd: root,
      encoding: "utf8",
      timeout: 120000,
    });
    for (const channel of [result.stdout ?? "", result.stderr ?? ""]) {
      if (channel.includes(CANARY)) fail("cli-canary-leak", args[2]);
      if (channel.includes(HANDLE)) fail("cli-handle-leak", args[2]);
      for (const name of FORBIDDEN_NAMES) {
        if (channel.toLowerCase().includes(name)) fail("cli-consumer-name", args[2]);
      }
    }
  }

  // 2b. Valid-grammar confidential markers are admitted but never
  //    echoed: a schema-valid evidence id naming a forbidden consumer
  //    identity rides through admission (grammar is not
  //    classification) and the public projection carries only the
  //    opaque admitted reference derived from the exact bytes — in the
  //    success path AND in the failure path. The marker document lives
  //    outside the checkout, like every mutated runtime input.
  {
    const { mkdtempSync, writeFileSync } = await import("node:fs");
    const { tmpdir } = await import("node:os");
    const tmp = mkdtempSync(join(tmpdir(), "lekalo-privacy-marker-"));
    const marker = "taskhub.evidence.synthetic_secret_52";
    const marked = JSON.parse(
      readFileSync(join(root, "tests/fixtures/task-external-links/evidence/mock-run.json"), "utf8"),
    );
    marked.evidenceId = marker;
    const markedPath = join(tmp, "marked-evidence.json");
    writeFileSync(markedPath, JSON.stringify(marked, null, 2));
    const refused = JSON.parse(JSON.stringify(marked));
    refused.scenarioId = "tasklinks.scenario.ghost_scenario";
    const refusedPath = join(tmp, "refused-evidence.json");
    writeFileSync(refusedPath, JSON.stringify(refused, null, 2));
    const probe = (path) =>
      spawnSync(
        binary,
        ["--no-cache", "--json", "task-links", "inspect", "tasklinks.task_external_link",
         "--project", "tests/fixtures/task-external-links/project",
         "--attachment", "tests/fixtures/task-external-links/contracts/integration.json",
         "--evidence", path],
        { cwd: root, encoding: "utf8", timeout: 120000 },
      );
    const admitted = probe(markedPath);
    if (admitted.status !== 0) {
      fail("marker-admission", String(admitted.status) + (admitted.stderr ?? "").slice(0, 200));
    }
    const failure = probe(refusedPath);
    if (failure.status === 0) fail("marker-refusal-exit", String(failure.status));
    for (const result of [admitted, failure]) {
      for (const channel of [result.stdout ?? "", result.stderr ?? ""]) {
        if (channel.includes(marker)) {
          fail("cli-confidential-marker-leak", "the producer-supplied marker reached a public channel");
        }
      }
    }
    if (admitted.status === 0 && !(admitted.stdout ?? "").includes("tasklinks.evidence.admitted.")) {
      fail("marker-opaque-reference", "the admitted projection must carry the opaque reference");
    }
  }
}

// 3. Provenance registration: the family is declared synthetic.
{
  const manifest = JSON.parse(
    readFileSync(join(root, "tests/fixtures/fixture-provenance.json"), "utf8"),
  );
  const family = manifest.families.find((entry) => entry.family === "task-external-links");
  if (!family) fail("provenance-missing", "task-external-links");
  else if (family.origin !== "synthetic") fail("provenance-origin", family.origin);
}

// 4. Every evidence document labels its origin and every scenario doc
//    stays mock-labeled (native claims are the native gate's custody).
for (const file of files) {
  if (!file.endsWith(".json")) continue;
  const relative = file.slice(root.length + 1).replace(/\\/g, "/");
  if (relative.includes("/evidence/") && !relative.includes("/invalid/")) {
    const document = JSON.parse(readFileSync(file, "utf8"));
    if (!["mock", "reference", "native"].includes(document.origin)) {
      fail("evidence-origin-label", relative);
    }
  }
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({ ok: true, gate, filesScanned: files.length, canaryConfined: true })}\n`,
);

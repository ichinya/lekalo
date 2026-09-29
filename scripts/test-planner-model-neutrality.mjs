#!/usr/bin/env node
// Issue #114 planner-model neutrality gate.
//
// Acceptance criterion: "the planner Model carries no Laravel, Eloquent,
// Vue or TypeScript concepts". The semantic model is the language-
// independent layer — the same planner definitions compile to the PHP/
// Laravel pilot and stay the observed Node baseline — so the model
// surfaces may never name a framework, language, runtime, storage
// engine, or analysis tool.
//
// The gate:
//   1. scans the closed planner model surfaces (the routes-corpus model
//      home and the contracted planner-slice model home; the derived
//      `.lekalo` cache and the opaque `lekalo/targets/` target config
//      are excluded — a target-binding's `target` field is the one
//      contract-sanctioned place a target name may appear, and it is
//      tolerated only there);
//   2. checks every definition kind against the closed Model IR
//      vocabulary;
//   3. verifies (or, with --write, re-emits) the committed audit
//      artifact `tests/fixtures/php-laravel/routes/evidence/
//      model-neutrality.audit.json` — file list, per-file digests, the
//      forbidden-vocabulary digest, and the zero-hit summary.
//
// Dependency-free; run from the repo root.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const AUDIT_PATH = "tests/fixtures/php-laravel/routes/evidence/model-neutrality.audit.json";

/** The scan roots: every semantic model surface of the planner pilot. */
const SCAN_ROOTS = [
  { root: "tests/fixtures/php-laravel/routes/model/lekalo", skip: [".lekalo"] },
  { root: "tests/fixtures/php-laravel/routes/model/openspec", skip: [] },
  { root: "tests/fixtures/contracted/planner-slice/lekalo/modules", skip: [] },
  { root: "tests/fixtures/contracted/planner-slice/lekalo/project.yaml", skip: null },
];

/** The closed forbidden vocabulary: framework, language, runtime,
 * package-manager, storage-engine, and analysis-tool naming. A hit
 * anywhere in the model surfaces (outside the target-binding exception)
 * is a neutrality failure. */
const FORBIDDEN = [
  "laravel", "eloquent", "illuminate", "artisan", "blade", "inertia",
  "livewire", "fortify", "sanctum",
  "vue", "vuex", "vite", "nuxt", "pinia",
  "typescript", "javascript", "node", "deno", "bun", "npm", "pnpm", "yarn",
  "php", "composer", "psr", "symfony", "doctrine", "testo", "laratesto", "mago",
  "postgres", "mysql", "sqlite", "redis", "mongodb",
  "react", "angular", "svelte", "django", "rails", "activerecord", "hibernate",
];

const KINDS = [
  "project", "module", "scalar", "enum", "value-object", "entity",
  "command", "query", "policy", "event", "effect", "endpoint",
  "scenario", "target-binding",
];

const SOURCE_EXTENSIONS = new Set([".yaml", ".yml", ".json", ".md"]);

const walk = (absolute, relative, skip, files) => {
  if (skip === null || statSync(absolute).isFile()) {
    if (SOURCE_EXTENSIONS.has("." + absolute.split(".").pop())) files.push(relative);
    return files;
  }
  for (const entry of readdirSync(absolute)) {
    if (skip.includes(entry)) continue;
    const abs = join(absolute, entry);
    const rel = relative === "" ? entry : `${relative}/${entry}`;
    if (statSync(abs).isDirectory()) walk(abs, rel, skip, files);
    else if (SOURCE_EXTENSIONS.has("." + entry.split(".").pop())) files.push(rel);
  }
  return files;
};

const listModelFiles = () => {
  const files = [];
  for (const scan of SCAN_ROOTS) {
    walk(join(repoRoot, ...scan.root.split("/")), scan.root, scan.skip, files);
  }
  return files.sort();
};

/** Strip the one contract-sanctioned occurrence: a target-binding
 * definition's `target` field may name its target id. */
const stripTargetBindingTargets = (text) =>
  text.replace(/"kind":"target-binding"[^\n]*/g, (line) =>
    line.replace(/"target":"[a-z0-9-]+"/g, "\"target\":\"stripped\""));

const findHits = (text) => {
  const hits = [];
  const scrubbed = stripTargetBindingTargets(text);
  for (const token of FORBIDDEN) {
    const pattern = new RegExp(`\\b${token}\\b`, "gi");
    let match;
    while ((match = pattern.exec(scrubbed)) !== null) {
      hits.push({ token, line: scrubbed.slice(0, match.index).split("\n").length });
    }
  }
  return hits;
};

let failures = 0;
const step = (name, body) => {
  try {
    body();
    process.stdout.write(`ok - ${name}\n`);
  } catch (error) {
    failures += 1;
    process.stdout.write(`FAIL - ${name}\n${error?.stack ?? error}\n`);
  }
};

const files = listModelFiles();
const perFile = files.map((relative) => {
  const bytes = readFileSync(join(repoRoot, ...relative.split("/")));
  const text = bytes.toString("utf8");
  const kinds = [...text.matchAll(/"kind":"([a-z-]+)"/g)].map((m) => m[1]);
  return {
    path: relative,
    digest: "sha256:" + createHash("sha256").update(bytes).digest("hex"),
    hits: findHits(text),
    kinds,
  };
});

step("the forbidden vocabulary is closed and stable", () => {
  assert.ok(FORBIDDEN.length >= 40, "the vocabulary stays substantial");
  assert.deepEqual([...FORBIDDEN].sort(), [...new Set(FORBIDDEN)].sort(), "no duplicate tokens");
  const vocabularyDigest = "sha256:" + createHash("sha256")
    .update(JSON.stringify(FORBIDDEN)).digest("hex");
  process.stdout.write(`    vocabulary digest: ${vocabularyDigest}\n`);
});

step("every scanned definition kind is in the closed Model IR vocabulary", () => {
  for (const file of perFile) {
    // Scenario documents are Scenario IR, not model definitions: their
    // step/assertion kinds have their own closed vocabulary and are
    // validated by the Scenario IR contract gates.
    if (file.path.includes("lekalo/scenarios/")) continue;
    for (const kind of file.kinds) {
      assert.ok(
        KINDS.includes(kind),
        `${file.path}: kind ${kind} is outside the closed vocabulary`,
      );
    }
  }
});

step("the planner model surfaces carry zero target-specific concepts", () => {
  const offenders = perFile.filter((file) => file.hits.length > 0);
  assert.deepEqual(offenders, [], `forbidden hits: ${JSON.stringify(offenders.map((f) => [f.path, f.hits]))}`);
});

step("the committed audit artifact is fresh (or --write re-emits it)", () => {
  const auditPath = join(repoRoot, ...AUDIT_PATH.split("/"));
  const current = {
    schema_version: "lekalo/model-neutrality-audit/v0.1.0",
    identity: "dev.lekalo.model-neutrality-audit@0.1.0",
    role: "neutrality-audit",
    note: "Evidence for the issue #114 acceptance criterion: the planner Model surfaces (the routes-corpus model home and the contracted planner-slice model home) carry no Laravel, Eloquent, Vue, TypeScript, or other target-specific concepts. The one sanctioned exception is a target-binding definition's target field; the derived .lekalo cache and the opaque lekalo/targets/ config are not model surfaces. Every hit of the closed forbidden vocabulary is a neutrality failure.",
    forbidden_vocabulary: FORBIDDEN,
    forbidden_vocabulary_digest: "sha256:" + createHash("sha256")
      .update(JSON.stringify(FORBIDDEN)).digest("hex"),
    closed_kinds: KINDS,
    scanned_file_count: perFile.length,
    scanned_files: perFile.map((file) => ({ path: file.path, digest: file.digest })),
    hits: [],
    result: "neutral",
  };
  const canonical = JSON.stringify(current, null, 2) + "\n";
  if (process.argv.includes("--write")) {
    mkdirSync(dirname(auditPath), { recursive: true });
    writeFileSync(auditPath, canonical);
    process.stdout.write(`    audit re-emitted: ${AUDIT_PATH}\n`);
    return;
  }
  assert.ok(
    existsSync(auditPath),
    `audit artifact missing: ${AUDIT_PATH} (run with --write to emit it)`,
  );
  const committed = readFileSync(auditPath, "utf8");
  assert.equal(committed, canonical, "the audit is stale - re-run with --write after a model change");
});

if (failures > 0) {
  process.stderr.write(`${failures} model-neutrality gate step(s) failed\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "planner-model-neutrality", files: perFile.length })}\n`);

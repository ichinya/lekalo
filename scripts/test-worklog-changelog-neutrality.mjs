#!/usr/bin/env node
// Issue #51 neutrality gate.
//
// Acceptance criterion: "Public artifacts не раскрывают название
// consumer application." The complete public artifact inventory of the
// pilot — the neutral Model, the OpenSpec requirements, the
// attachments, the scenario corpus, the evidence report, the public DTO
// schema, the port manifest, the Go refusal, the gates, and the
// generators — is scanned for:
//   1. framework, language, runtime, storage-engine, and analysis-tool
//      naming (the closed #114 vocabulary; a target-binding's
//      `target` field is the one contract-sanctioned place a target
//      name may appear, tolerated only in the bindings documents);
//   2. identity leaks: every semantic namespace root must be inside
//      the synthetic pilot vocabulary (`taskhub`, `worklog`,
//      `changelog`), every branded compound identifier (PascalCase
//      humps) must be schema vocabulary, and no foreign package or
//      product identity may appear;
//   3. physical host paths, wall-clock timestamps outside the
//      deterministic scenario clocks, credentials, and non-synthetic
//      URLs.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const FIXTURE = "tests/fixtures/pilot/worklog-changelog";
const SCRIPTS = [
  "scripts/test-worklog-changelog-contracts.mjs",
  "scripts/test-worklog-changelog-pilot.mjs",
  "scripts/test-worklog-changelog-impact.mjs",
  "scripts/gen-worklog-changelog-contracts.mjs",
  "scripts/gen-worklog-changelog-attachments.mjs",
  "scripts/gen-worklog-changelog-pilot-fixture.mjs",
  "scripts/gen-worklog-changelog-scenarios.mjs",
];
// This gate is not in its own scan list: it necessarily carries the
// forbidden vocabulary it rejects.

const FORBIDDEN = [
  "laravel", "eloquent", "illuminate", "artisan", "blade", "inertia",
  "livewire", "fortify", "sanctum",
  "vue", "vuex", "vite", "nuxt", "pinia",
  "typescript", "javascript", "deno", "bun", "npm", "pnpm", "yarn",
  "psr", "symfony", "doctrine", "testo", "laratesto", "mago",
  "postgres", "mysql", "sqlite", "redis", "mongodb",
  "react", "angular", "svelte", "django", "rails", "activerecord",
  "hibernate",
];

// The closed synthetic identity of the pilot.
const PILOT_PROJECT_ID = "taskhub";

// The closed branded-compound (PascalCase humps) schema vocabulary.
const COMPOUND_ALLOWLIST = new Set([
  "SchemaVersion", "SnapshotSchema", "PayloadVersion", "ContentDigest",
  "EntryId", "PublishedAt", "TaskWorklog", "ProviderAction", "ChangelogEntry",
  "TaskId", "TenantId", "WorkspaceId", "AuthorId", "ActionId", "ApprovalId",
  "AuditId", "CorrelationKey", "PayloadDigest", "DeliveryState", "RecordRevision",
  "PreviewRevision", "PreviewDigest", "PublishingAuthority", "PolicyRevision",
  "SourceWorklog", "WorklogId", "WrittenAt", "RecordedAt", "DraftBody",
  "JsonSchema", "TypeRef", "Draft",
  // Language built-ins the runtime lane cannot spell otherwise: the
  // typed store fault must extend the interpreter's exception base to
  // be throwable at all. Not consumer-application identity.
  "RuntimeException",
]);

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const listFiles = (dir, files = []) => {
  for (const entry of readdirSync(dir)) {
    const abs = join(dir, entry);
    // The documented generated runtime residue (the loader's cache
    // under the model project's .lekalo directory, gitignored as such)
    // is not public fixture inventory: a CLI run may regenerate it at
    // any moment, and it is an opaque build artifact, never content.
    if (abs.replaceAll("\\", "/").includes(`${FIXTURE}/model/.lekalo/`)) continue;
    if (statSync(abs).isDirectory()) listFiles(abs, files);
    else files.push(abs);
  }
  return files;
};

const stripTargetBindingTargets = (text) =>
  text.replace(/"kind":"target-binding"[^}]*\}/g, (line) =>
    line.replace(/"target":"[a-z0-9-]+"/g, "\"target\":\"stripped\""));

const inventory = [];
for (const file of listFiles(resolve(root, FIXTURE))) {
  inventory.push(file);
}
for (const script of SCRIPTS) {
  inventory.push(resolve(root, script));
}
// The native runtime lane is part of the public inventory and is
// scanned like every other source file.
const SOURCE_EXTENSIONS = new Set([".json", ".yaml", ".yml", ".md", ".mjs", ".php"]);
// Every file without a scannable text extension is opaque inventory:
// it cannot be neutrality-checked, so it fails the gate instead of
// silently evading the scan (no binary blobs in a public fixture).
for (const file of inventory) {
  const suffix = file.includes(".") ? `.${file.slice(file.lastIndexOf(".") + 1)}` : "";
  if (SOURCE_EXTENSIONS.has(suffix)) continue;
  fail(
    "unscannable-inventory",
    `${file.slice(root.length + 1)}: no text extension; the public-artifact inventory must stay fully scannable`,
  );
}
let scanned = 0;
const namespaceSeen = new Set();
for (const file of inventory) {
  const suffix = file.includes(".") ? `.${file.slice(file.lastIndexOf(".") + 1)}` : "";
  if (!SOURCE_EXTENSIONS.has(suffix)) continue;
  const text = readFileSync(file, "utf8");
  const relative = file.slice(root.length + 1);
  const isBindings = relative.endsWith("bindings.yaml");
  const scrubbed = isBindings ? stripTargetBindingTargets(text) : text;
  // 1. Framework and tool vocabulary.
  for (const token of FORBIDDEN) {
    const pattern = new RegExp(`\\b${token}\\b`, "gi");
    if (pattern.test(scrubbed)) fail("framework-vocabulary", `${relative}: ${token}`);
  }
  // 2a. Identity closure: the contract's project id is the synthetic
  //     pilot identity, and every package-patterned identity string is
  //     a Lekalo or pilot-owned one.
  for (const match of scrubbed.matchAll(/"projectId"\s*:\s*"([a-z0-9_-]+)"/g)) {
    if (match[1] !== PILOT_PROJECT_ID) fail("project-identity", relative);
  }
  for (const match of scrubbed.matchAll(/\bdev\.([a-z][a-z-]*)/g)) {
    namespaceSeen.add(match[1]);
    if (!["lekalo", "taskhub"].includes(match[1])) {
      fail("package-identity", `${relative}: dev.${match[1]}`);
    }
  }
  // 2b. Branded compounds: PascalCase with at least two humps must be
  //     schema vocabulary.
  for (const match of scrubbed.matchAll(/\b[A-Z][a-z]+[A-Z][A-Za-z]*\b/g)) {
    if (!COMPOUND_ALLOWLIST.has(match[0])) {
      fail("compound-identity", `${relative}: ${match[0]}`);
    }
  }
  // 3. Host paths, wall clocks, credentials, non-synthetic URLs.
  if (/[A-Z]:\\|\/home\/|\/Users\/|runner\.temp/.test(text)) {
    fail("host-path", relative);
  }
  if (/20[0-9]{2}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}/.test(text) && !relative.includes("scenarios")) {
    // Scenario clock preconditions are explicit deterministic data;
    // anywhere else a wall-clock timestamp is a leak.
    fail("wall-clock", relative);
  }
  if (/(password|secret|api[_-]?key|bearer)\s*["'=]/i.test(text)) {
    fail("credential-shape", relative);
  }
  const urls = text.match(/https?:\/\/[^\s"')]+/g) ?? [];
  for (const url of urls) {
    if (
      !url.startsWith("https://json-schema.org") &&
      !url.startsWith("https://taskhub.synthetic/") &&
      !url.startsWith("https://dev.lekalo/")
    ) {
      fail("non-synthetic-url", `${relative}: ${url}`);
    }
  }
  scanned += 1;
}
for (const identity of namespaceSeen) {
  if (!["lekalo", "taskhub"].includes(identity)) fail("package-identity-global", identity);
}

process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate: "test-worklog-changelog-neutrality",
    scanned,
    inventory: inventory.length,
    namespaces: [...namespaceSeen].sort(),
  })}\n`,
);

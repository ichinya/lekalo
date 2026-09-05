// Issue #21 fixture generator: builds the ownership-manifest wire matrix
// under tests/fixtures/artifacts/wire. Run once with
// `node scripts/gen-artifact-fixtures.mjs`; the output files are committed
// and never regenerated in CI. Digests inside the fixtures are opaque
// sample values (sha256 over fixed filler hex), except `manifest_digest`,
// which is always the real SHA-256 over the canonical payload of the same
// document with the `manifest_digest` property removed — the exact
// non-self-referential domain the checker verifies.

import { createHash } from "node:crypto";
import { mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const out = join(root, "tests", "fixtures", "artifacts", "wire");

const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const filler = (seed) => `sha256:${sha256(String(seed))}`;
const hex64 = (seed) => sha256(String(seed));

// Recursive key sort by unsigned byte order and compact emission: the
// same canonical discipline the Rust writer enforces.
function sortValue(value) {
  if (Array.isArray(value)) return value.map(sortValue);
  if (value && typeof value === "object") {
    const sorted = {};
    for (const key of Object.keys(value).sort()) sorted[key] = sortValue(value[key]);
    return sorted;
  }
  return value;
}

function canonicalBytes(document) {
  return Buffer.from(JSON.stringify(sortValue(document)), "utf8");
}

// Stamp the non-self-referential manifest digest and emit the canonical
// file bytes (compact, sorted keys, exactly one trailing LF).
function emit(relative, document, { sortArrays = true, stamp = true } = {}) {
  if (!stamp) {
    // Write the document exactly as given (sorted keys, one LF): the
    // caller already decided the digest it wants on the wire.
    const final = sortValue(document);
    writeFileSync(join(out, relative + ".manifest.json"), Buffer.from(JSON.stringify(final) + "\n", "utf8"));
    return;
  }
  if (!sortArrays) {
    // Author an explicitly order-violating document: stamp the digest
    // over its own (unsorted) payload so only the sort invariant fails.
    const draft = { ...document };
    delete draft.manifest_digest;
    const digest = `sha256:${sha256(canonicalBytes(draft))}`;
    const final = sortValue({ ...document, manifest_digest: digest });
    writeFileSync(join(out, relative + ".manifest.json"), Buffer.from(JSON.stringify(final) + "\n", "utf8"));
    return;
  }
  // Author the arrays exactly as the closed format mandates: artifacts
  const keyOf = (a) => `${a.semantic_owner}\u0000${a.path}\u0000${a.artifact_kind}`;
  const sorted = sortValue({
    ...document,
    artifacts: [...(document.artifacts ?? [])].sort((a, b) => (keyOf(a) < keyOf(b) ? -1 : 1)),
    source_maps: [...(document.source_maps ?? [])].map((map) => ({
      ...map,
      entries: [...map.entries].sort((a, b) =>
        a.start - b.start || a.end - b.end || (a.semantic_id < b.semantic_id ? -1 : 1)),
    })),
  });
  for (const artifact of sorted.artifacts ?? []) {
    artifact.input_refs = [...(artifact.input_refs ?? [])].sort();
  }
  const draft = { ...sorted };
  delete draft.manifest_digest;
  const digest = `sha256:${sha256(canonicalBytes(draft))}`;
  const final = sortValue({ ...sorted, manifest_digest: digest });
  writeFileSync(join(out, relative + ".manifest.json"), Buffer.from(JSON.stringify(final) + "\n", "utf8"));
}

const ADAPTER = (protocol = "1.0.0") => ({
  id: "node-typescript",
  version: "0.1.2",
  digest: filler("adapter-package"),
  artifacts: [{ platform: "any", digest: filler("adapter-any") }],
  protocol_version: protocol,
});

const ARTIFACT = (over = {}) => ({
  semantic_owner: "planner.create_task",
  path: "apps/api/src/planner/focus-task.ts",
  artifact_kind: "source",
  lifecycle: "generated",
  adapter_ref: ADAPTER(),
  generator_ref: { id: "planner-gen", version: "0.3.0" },
  content: {
    algorithm: "sha256",
    digest: filler("content"),
    canonicalization: "exact-file-bytes",
  },
  input_refs: ["planner.create_task", "planner.task"],
  regeneration_policy: "on-input-change",
  ...over,
});

const BASE = (over = {}) => ({
  schema_version: "lekalo/artifact-manifest/v1.0.0",
  identity: "dev.lekalo.artifact-manifest@1.0.0",
  project_ref: "planner",
  lock_ref: { schema_version: "lekalo/lock/v1.0.0", digest: filler("lock") },
  inputs: {
    model: { version: "1.0.0", digest: filler("model") },
    ir: { version: "0.1.0", digest: filler("ir") },
  },
  artifacts: [],
  source_maps: [],
  ...over,
});

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "valid"), { recursive: true });
mkdirSync(join(out, "invalid"), { recursive: true });

// Valid documents.
emit(join("valid", "empty"), BASE());
emit(join("valid", "one-generated"), BASE({
  artifacts: [ARTIFACT()],
  source_maps: [{
    artifact: {
      semantic_owner: "planner.create_task",
      path: "apps/api/src/planner/focus-task.ts",
      artifact_kind: "source",
    },
    input_revision: filler("inputs-revision"),
    entries: [
      { semantic_id: "planner.create_task", start: 0, end: 615 },
      { semantic_id: "planner.task", start: 700, end: 904 },
    ],
  }],
}));

const lifecycles = [
  ["generated", "on-input-change", "generated.ts", "source"],
  ["scaffolded", "once", "scaffolded.ts", "source"],
  ["checked", "validate-only", "checked.ts", "source"],
  ["external", "reference-only", "external.d.ts", "schema"],
  ["custom", "manual-only", "custom.ts", "config"],
];
const invalid = (relative, document, options) => emit(relative, document, options);
emit(join("valid", "every-lifecycle"), BASE({
  artifacts: lifecycles.map(([lifecycle, policy, leaf, kind]) => ARTIFACT({
    semantic_owner: `planner.task_${lifecycle}`,
    path: `apps/api/src/planner/${leaf}`,
    artifact_kind: kind,
    lifecycle,
    regeneration_policy: policy,
    input_refs: [`planner.task_${lifecycle}`],
  })),
}));

emit(join("valid", "multi-target"), BASE({
  artifacts: [
    ARTIFACT({
      adapter_ref: ADAPTER(),
    }),
    ARTIFACT({
      path: "apps/web/src/planner/focus-task.ts",
      adapter_ref: { ...ADAPTER(), id: "python-fastapi", digest: filler("adapter2-package") },
    }),
  ],
}));


invalid(join("invalid", "missing-identity"), (() => {
  const doc = BASE({ artifacts: [ARTIFACT()] });
  delete doc.identity;
  return doc;
})());
invalid(join("invalid", "extra-field"), { ...BASE(), extra: true });
invalid(join("invalid", "unknown-lifecycle"), BASE({
  artifacts: [ARTIFACT({ lifecycle: "owned", regeneration_policy: "on-input-change" })],
}));
invalid(join("invalid", "unknown-kind"), BASE({
  artifacts: [ARTIFACT({ artifact_kind: "binary" })],
}));
invalid(join("invalid", "policy-lifecycle-mismatch"), BASE({
  artifacts: [ARTIFACT({ regeneration_policy: "once" })],
}));
invalid(join("invalid", "unknown-canonicalization"), BASE({
  artifacts: [ARTIFACT({
    content: {
      algorithm: "sha256",
      digest: filler("content"),
      canonicalization: "normalized-lf",
    },
  })],
}));
invalid(join("invalid", "malformed-digest-uppercase"), BASE({
  artifacts: [ARTIFACT({
    content: {
      algorithm: "sha256",
      digest: `sha256:${hex64("up").toUpperCase()}`,
      canonicalization: "exact-file-bytes",
    },
  })],
}));
invalid(join("invalid", "malformed-semver"), BASE({
  artifacts: [ARTIFACT({ adapter_ref: { ...ADAPTER(), version: "v0.1.2" } })],
}));
invalid(join("invalid", "bad-grammar-owner"), BASE({
  artifacts: [ARTIFACT({ semantic_owner: "Planner.Create_Task" })],
}));
invalid(join("invalid", "absolute-path"), BASE({
  artifacts: [ARTIFACT({ path: "/apps/api/src/planner/focus-task.ts" })],
}));
invalid(join("invalid", "backslash-path"), BASE({
  artifacts: [ARTIFACT({ path: "apps\\api\\src\\planner\\focus-task.ts" })],
}));
invalid(join("invalid", "unsorted-artifacts"), BASE({
  artifacts: [
    ARTIFACT({ semantic_owner: "planner.render_task" }),
    ARTIFACT({ semantic_owner: "planner.create_task" }),
  ],
}), { sortArrays: false });
invalid(join("invalid", "duplicate-artifact-keys"), BASE({
  artifacts: [ARTIFACT(), ARTIFACT()],
}));
invalid(join("invalid", "future-schema"), {
  ...BASE(),
  schema_version: "lekalo/artifact-manifest/v2.0.0",
});
invalid(join("invalid", "wrong-manifest-digest"), (() => {
  const document = BASE({ artifacts: [ARTIFACT()] });
  delete document.manifest_digest;
  document.manifest_digest = filler("bogus");
  return document;
})(), { stamp: false });
invalid(join("invalid", "map-without-artifact"), BASE({
  artifacts: [ARTIFACT()],
  source_maps: [{
    artifact: {
      semantic_owner: "planner.delete_task",
      path: "apps/api/src/planner/focus-task.ts",
      artifact_kind: "source",
    },
    input_revision: filler("inputs-revision"),
    entries: [{ semantic_id: "planner.create_task", start: 0, end: 10 }],
  }],
}));
invalid(join("invalid", "reversed-range"), BASE({
  artifacts: [ARTIFACT()],
  source_maps: [{
    artifact: {
      semantic_owner: "planner.create_task",
      path: "apps/api/src/planner/focus-task.ts",
      artifact_kind: "source",
    },
    input_revision: filler("inputs-revision"),
    entries: [{ semantic_id: "planner.create_task", start: 10, end: 5 }],
  }],
}));
invalid(join("invalid", "wrong-lock-ref-schema"), BASE({
  lock_ref: { schema_version: "lekalo/lock/v9.9.9", digest: filler("lock") },
}));

// Sanity: every valid fixture carries a verifiable self-digest and the
// tampered one deliberately does not.
for (const dir of ["valid", "invalid"]) {
  for (const name of readdirSync(join(out, dir)).sort()) {
    if (!name.endsWith(".json")) continue;
    const document = JSON.parse(readFileSync(join(out, dir, name), "utf8"));
    const draft = { ...document };
    delete draft.manifest_digest;
    const expected = `sha256:${sha256(canonicalBytes(draft))}`;
    if (dir === "valid" && document.manifest_digest !== expected) {
      console.error(`self-digest mismatch in ${dir}/${name}`);
      process.exit(1);
    }
  }
}
console.log("artifact wire fixtures written");

#!/usr/bin/env node
// Issue #52 fixture regeneration: recompute every derived pin of the
// task-external-links family from the canonical base attachment.
//
// - Attachment documents (schemaVersion lekalo/task-links-integration/*)
//   get their deliveryRules.outboxStates aligned to the canonical base
//   and their `attachmentRevision` recomputed over the exact canonical
//   revision members (compact JSON, insertion order = canonical member
//   order). Mutations inside invalid/ triggers are preserved; only the
//   derived pins move, so a trigger reaches its targeted rule instead
//   of being masked by a stale pin.
// - Evidence documents (schemaVersion lekalo/task-links-evidence/*)
//   get their `attachmentRevision`, `modelRef.schemaVersion`, and
//   `modelRef.irDigest` synced from the base attachment. The committed
//   stale-evidence trigger keeps its zero digest: the stale pin IS its
//   targeted refusal.
// - The impact change pair gets its base/changed revision tokens
//   recomputed (the changed side replaces exactly the upstream pin).
//
// The script never invents mutations: it only recomputes derived pins
// over the committed bytes, exactly like the runtime verification does.

import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixtureRoot = join(root, "tests/fixtures/task-external-links");
const baseRel = "contracts/integration.json";

const REVISION_MEMBERS = [
  "schemaVersion",
  "identity",
  "projectId",
  "modelRef",
  "irRef",
  "ownerContractPins",
  "entities",
  "relations",
  "uniqueConstraints",
  "providerContracts",
  "linkCapabilityRequirements",
  "fieldPolicies",
  "actionPolicies",
  "operationBindings",
  "deliveryRules",
  "contributions",
];

const OUTBOX_STATES = [
  "pending",
  "leased",
  "accepted_pending_ack",
  "delivered",
  "retry_wait",
  "conflicted",
  "cancelled",
  "dead_letter",
  "unknown_delivery",
];

const sha = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
// The runtime canonical form: serde_json Value maps sort every object's
// keys recursively (BTreeMap order) and serialize compact. Replicate
// exactly so the recomputed revision matches the verifier byte-for-byte.
const canonicalBytes = (value) => {
  if (Array.isArray(value)) {
    return Buffer.from(`[${value.map((item) => canonicalBytes(item).toString("utf8")).join(",")}]`, "utf8");
  }
  if (value && typeof value === "object") {
    const keys = Object.keys(value).sort();
    const body = keys
      .map((key) => `${JSON.stringify(key)}:${canonicalBytes(value[key]).toString("utf8")}`)
      .join(",");
    return Buffer.from(`{${body}}`, "utf8");
  }
  return Buffer.from(JSON.stringify(value), "utf8");
};
const revisionOf = (document) => {
  const revision = {};
  for (const member of REVISION_MEMBERS) {
    if (document[member] === undefined) return null;
    revision[member] = document[member];
  }
  return sha(canonicalBytes(revision));
};

const base = JSON.parse(readFileSync(join(fixtureRoot, baseRel), "utf8"));
base.deliveryRules.outboxStates = [...OUTBOX_STATES];

// The live canonical IR digest of the pilot fixture project, rendered
// by the built CLI and hashed over the same canonical bytes the core
// verifier computes. A missing binary is a hard error: the pins must
// never be regenerated from a stale compilation.
const projectRel = "tests/fixtures/task-external-links/project";
const binaryCandidates = [
  join(root, "target/debug/lekalo"),
  join(root, "target/debug/lekalo.exe"),
];
const binary = binaryCandidates.find((candidate) => {
  try {
    return statSync(candidate).isFile();
  } catch {
    return false;
  }
});
if (!binary) {
  process.stderr.write(
    "gen-task-links-fixtures: build first (cargo build --workspace --locked); the IR pins must be regenerated from the live compilation\n",
  );
  process.exit(1);
}
const irRun = spawnSync(binary, ["--no-cache", "--json", "load", "--project", projectRel, "--ir"], {
  cwd: root,
  encoding: "utf8",
  timeout: 120000,
});
if (irRun.error || irRun.status !== 0) {
  process.stderr.write(
    `gen-task-links-fixtures: the pilot project does not compile: ${String(irRun.error ?? irRun.stderr).slice(0, 400)}\n`,
  );
  process.exit(1);
}
const irObject = JSON.parse(irRun.stdout).ir;
const irDigest = sha(canonicalBytes(irObject));
base.irRef.digest = irDigest;
base.irRef.identity = "dev.lekalo.ir@0.8.0";
base.modelRef.irDigest = irDigest;
base.modelRef.schemaVersion = "0.8.0";

const baseRevision = revisionOf(base);

// The changed side of the impact pair: exactly the upstream pin moves.
const pairPath = join(fixtureRoot, "impact/provider-change-pair.json");
const pair = JSON.parse(readFileSync(pairPath, "utf8"));
const changed = JSON.parse(JSON.stringify(base));
for (const contract of changed.providerContracts) {
  if (contract.contractId === pair.changed.contractId) {
    contract.upstreamPin = pair.changed.upstreamPin;
  }
}
const changedRevision = revisionOf(changed);

const written = [];
const walk = (current) => {
  for (const name of readdirSync(current).sort()) {
    const full = join(current, name);
    if (statSync(full).isDirectory()) {
      walk(full);
      continue;
    }
    if (!name.endsWith(".json") || name === "expect.json") continue;
    const relativePath = relative(fixtureRoot, full).replaceAll("\\", "/");
    const raw = readFileSync(full, "utf8");
    let document;
    try {
      document = JSON.parse(raw);
    } catch {
      continue;
    }
    if (!document || typeof document !== "object") continue;
    const schemaVersion = document.schemaVersion;
    let touched = false;
    if (relativePath === "contracts/detected-effects.json") {
      // The detected-evidence envelope joins THIS compilation: its
      // source revision is always the live canonical IR digest. The
      // evidence digest stays the pinned extended-effects document's
      // exact bytes (the declaration the envelope witnesses).
      const sourceRevision = `sha256:${createHash("sha256").update(canonicalBytes(irObject)).digest("hex")}`;
      if (document.sourceRevision !== sourceRevision) {
        document.sourceRevision = sourceRevision;
        touched = true;
      }
    } else if (
      typeof schemaVersion === "string"
      && schemaVersion.startsWith("lekalo/task-links-integration/")
      && relativePath !== baseRel
    ) {
      if (document.deliveryRules) {
        document.deliveryRules.outboxStates = [...OUTBOX_STATES];
      }
      // The stale-IR-digest trigger keeps its committed stale pin: the
      // divergent digest IS its targeted refusal. Every other
      // attachment joins the live compilation's digest so its targeted
      // rule is never masked by a stale pin.
      const staleIrTrigger = relativePath
        .replaceAll("\\", "/")
        .endsWith("invalid/pin-ir-digest-stale.json");
      if (document.irRef && document.irRef.identity !== base.irRef.identity) {
        document.irRef.identity = base.irRef.identity;
        touched = true;
      }
      if (document.modelRef && document.modelRef.schemaVersion !== base.modelRef.schemaVersion) {
        document.modelRef.schemaVersion = base.modelRef.schemaVersion;
        touched = true;
      }
      if (!staleIrTrigger) {
        if (document.irRef && document.irRef.digest !== irDigest) {
          document.irRef.digest = irDigest;
          touched = true;
        }
        if (document.modelRef && document.modelRef.irDigest !== irDigest) {
          document.modelRef.irDigest = irDigest;
          touched = true;
        }
      }
      // The provider wire-schema refs are declared definitions now;
      // fixtures derived before the rename still carry the old
      // free-symbol spelling. Migrate the refs so a trigger reaches
      // its targeted rule instead of the schema-ref join.
      const rename = (reference) => {
        if (typeof reference !== "string") return reference;
        const match = reference.match(/^tasklinks\.provider\.(primary|secondary)_(request|response|error)$/);
        return match ? `tasklinks.${match[1]}_${match[2]}` : reference;
      };
      for (const contract of document.providerContracts ?? []) {
        for (const member of ["requestSchemaRef", "responseSchemaRef", "errorSchemaRef"]) {
          const renamed = rename(contract[member]);
          if (renamed !== contract[member]) {
            contract[member] = renamed;
            touched = true;
          }
        }
      }
      const revision = revisionOf(document);
      if (revision && document.attachmentRevision !== revision) {
        document.attachmentRevision = revision;
        touched = true;
      }
    } else if (
      typeof schemaVersion === "string"
      && schemaVersion.startsWith("lekalo/task-links-evidence/")
    ) {
      // The stale-evidence trigger keeps its zero digest on purpose.
      const staleTrigger = relativePath.replaceAll("\\", "/").endsWith("invalid/evidence-stale.json");
      if (!staleTrigger && document.attachmentRevision !== baseRevision) {
        document.attachmentRevision = baseRevision;
        touched = true;
      }
      if (document.modelRef) {
        if (document.modelRef.irDigest !== base.irRef.digest) {
          document.modelRef.irDigest = base.irRef.digest;
          touched = true;
        }
        if (document.modelRef.schemaVersion !== base.modelRef.schemaVersion) {
          document.modelRef.schemaVersion = base.modelRef.schemaVersion;
          touched = true;
        }
      }
    }
    if (touched) {
      writeFileSync(full, `${JSON.stringify(document, null, 2)}\n`);
      written.push(relativePath);
    }
  }
};
walk(fixtureRoot);

// The base attachment itself.
{
  const revision = revisionOf(base);
  base.attachmentRevision = revision;
  writeFileSync(join(fixtureRoot, baseRel), `${JSON.stringify(base, null, 2)}\n`);
  written.push(baseRel);
}

// The impact pair tokens.
{
  pair.baseRevisionToken = baseRevision;
  pair.changedRevision = changedRevision;
  writeFileSync(pairPath, `${JSON.stringify(pair, null, 2)}\n`);
  written.push("impact/provider-change-pair.json");
}

process.stdout.write(
  `${JSON.stringify({ ok: true, regenerated: written.length, irDigest, baseRevision, changedRevision, files: written }, null, 1)}\n`,
);

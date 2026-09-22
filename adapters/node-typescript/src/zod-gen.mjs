/**
 * The Zod schema generation extension of `lekalo-target-node-typescript`
 * (issue #45): `generate` and `verify` over the compiled project IR.
 *
 * `generate` reads the canonical IR evidence through the kernel read
 * view, resolves the target-document codegen policy, maps and emits the
 * deterministic Zod modules (zod-map/zod-emit/zod-policy), and returns
 * the sorted write plan; on apply (`dry_run:false` with the echoed plan
 * id) it writes the exact bytes through the kernel's bounded write view
 * inside the staged private view. Constructs outside the mappable subset
 * classify as `zod.unsupported-construct` findings; because the v0.3.2
 * wire reserves the findings member for validate/verify, a generate run
 * with any finding surfaces as an honest partial error and nothing is
 * emitted. `verify` recomputes the expected bytes and diffs them against
 * the on-disk files, reporting `zod.drift` findings — the semantic
 * companion to the byte-digest drift gate.
 */

import { createHash } from "node:crypto";
import { canonicalJson, emitFiles, sha256 } from "./zod-emit.mjs";
import {
  DEFAULT_POLICY,
  IR_IDENTITY,
  ZOD_DIR,
  mapProject,
} from "./zod-map.mjs";
import { POLICY_PATH, resolvePolicy } from "./zod-policy.mjs";

/** The extension descriptor version (the adapter's release line). */
export const ZOD_EXTENSION_VERSION = "0.4.0";

/** The write scope every generated file lives under. */
export const ZOD_WRITE_SCOPES = [`${ZOD_DIR}/**`];

/** The closed drift finding code of the verify operation. */
export const DRIFT = "zod.drift";

/** The descriptor the bundle and the source entry both register. */
export const descriptor = {
  id: "zod-schema-generator",
  version: ZOD_EXTENSION_VERSION,
  operations: ["generate", "verify"],
  namedCapabilities: { "generate.zod": "full" },
  acceptedIrVersions: ["0.2.16"],
  writeScopes: ZOD_WRITE_SCOPES,
  invoke: (context) => zodOperation(context),
};

/**
 * The single extension entry. Every failure path is an honest outcome:
 * refused inputs return `failed` outcomes with bounded diagnostics
 * (projected as in-envelope errors), unsupported constructs travel as
 * typed findings, and successful applies report the exact write receipt.
 */
export function zodOperation(context) {
  const { operation, request, readView } = context;
  try {
    const policy = resolvePolicyFromContext(readView);
    if (policy.refusal) {
      return {
        state: "failed",
        diagnostics: [{ reason: `policy-${policy.refusal}` }],
      };
    }
    const ir = readIr(readView, request.ir_path);
    if (ir.refusal) {
      return { state: "failed", diagnostics: [{ reason: ir.refusal }] };
    }
    const inputDigest = sha256(ir.text);
    if (ir.document.contract !== IR_IDENTITY) {
      return {
        state: "failed",
        diagnostics: [{ reason: "ir-version-unsupported" }],
      };
    }
    const mapped = mapProject(ir.document, policy.policy);
    const files = emitFiles({
      modules: mapped.modules,
      inputDigest,
      adapterVersion: ZOD_EXTENSION_VERSION,
      irIdentity: IR_IDENTITY,
    });
    const byteFiles = files.map((emitted) => ({
      path: emitted.path,
      bytes: Buffer.from(emitted.text, "utf8"),
    }));
    if (operation === "generate") {
      return generateOperation(context, byteFiles, mapped.findings);
    }
    if (operation === "verify") {
      return verifyOperation(context, byteFiles, mapped.findings);
    }
    return { state: "unsupported" };
  } catch (error) {
    // Emitted as extension-failed by the kernel boundary; the local sink
    // keeps the bounded reason for diagnosis.
    throw new Error(`zod-schema-generator: ${bounded(error?.message)}`);
  }
}

/**
 * The plan identity of one write set: `plan-` plus the sha256 of the
 * canonical write entries — the same domain the core computes over the
 * received plan, so both sides derive the same opaque token.
 */
export function planIdOf(writes) {
  return "plan-" + sha256(canonicalJson(writes)).slice("sha256:".length);
}

function generateOperation(context, byteFiles, findings) {
  const { request, writeView } = context;
  if (findings.length > 0) {
    // Capability honesty: the declared subset cannot express the IR.
    // Nothing is emitted and nothing is written — a partial claim with
    // the per-symbol tokens in the refusal detail.
    return {
      state: "complete",
      data: { writes: [], findings },
    };
  }
  const writes = [];
  const bodies = new Map();
  for (const emitted of byteFiles) {
    const exists = writeView.exists(emitted.path);
    const action = exists ? "replace" : "create";
    writes.push({ path: emitted.path, action, sha256: sha256Bytes(emitted.bytes) });
    bodies.set(emitted.path, emitted.bytes);
  }
  writes.sort(byPath);
  if (request.dry_run === false) {
    if (!request.plan_id) {
      return { state: "failed", diagnostics: [{ reason: "missing-plan-id" }] };
    }
    for (const entry of writes) {
      writeView.write(entry.path, entry.action, bodies.get(entry.path));
    }
  }
  return {
    state: "complete",
    data: { writes, findings: [], plan_id: planIdOf(writes) },
  };
}

function verifyOperation(context, byteFiles, findings) {
  const { readView } = context;
  const verification = [];
  for (const emitted of byteFiles) {
    if (!readView.canRead(emitted.path)) {
      verification.push({
        path: emitted.path,
        code: DRIFT,
        detail: "unreadable-or-missing",
      });
      continue;
    }
    const observed = readView.readFile(emitted.path);
    if (!observed.equals(emitted.bytes)) {
      verification.push({
        path: emitted.path,
        code: DRIFT,
        detail: `expected:${sha256Bytes(emitted.bytes).slice(7, 19)} observed:${sha256Bytes(observed).slice(7, 19)}`,
      });
    }
  }
  const all = [...findings, ...verification];
  return { state: "complete", data: { writes: [], findings: all } };
}

/** Resolve the policy document through the read view (absent → defaults). */
function resolvePolicyFromContext(readView) {
  if (!readView.canRead(POLICY_PATH)) {
    return { policy: DEFAULT_POLICY, source: "absent" };
  }
  const bytes = readView.readFile(POLICY_PATH);
  return resolvePolicy(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
}

/** Read and parse the IR evidence through the read view. */
function readIr(readView, irPath) {
  if (!irPath || !readView.canRead(irPath)) {
    return { refusal: "ir-unreadable" };
  }
  let bytes;
  try {
    bytes = readView.readFile(irPath);
  } catch {
    return { refusal: "ir-unreadable" };
  }
  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return { refusal: "ir-encoding" };
  }
  let document;
  try {
    document = JSON.parse(text);
  } catch {
    return { refusal: "ir-json" };
  }
  if (document === null || typeof document !== "object"
    || !Array.isArray(document.definitions)) {
    return { refusal: "ir-shape" };
  }
  return { document, text };
}

function sha256Bytes(bytes) {
  // sha256 over exact bytes (the write-plan digest domain).
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

function byPath(left, right) {
  return left.path < right.path ? -1 : left.path > right.path ? 1 : 0;
}

function bounded(text) {
  return String(text ?? "unknown").replace(/[^a-zA-Z0-9._: -]+/g, "?").slice(0, 128);
}

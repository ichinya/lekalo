/**
 * The scenario-test compiler extension of `lekalo-target-node-typescript`
 * (issue #47, plans S3/S4/S7): `generate` and `verify` over Scenario IR
 * documents.
 *
 * The closed wire has exactly one `generate` operation, so this extension
 * never registers alone: it joins the generation composite (issues
 * #45/#70/#47), which routes on the IR document identity at `ir_path` —
 * a `dev.lekalo.scenario-ir@0.2.16` document lands here, a
 * `dev.lekalo.ir@0.2.16` document lands in the Zod/transport pipeline.
 * The composite advertises the existing reviewed capability id
 * `verify.scenarios` (registry `dev.lekalo.target-capabilities@0.4.0`)
 * as `full` while this extension is joined.
 *
 * `generate` reads the scenario document, the compiled project IR
 * evidence from the canonical cache home, and the project test-port
 * declaration; maps and emits the deterministic test files (scenario-map
 * / scenario-emit); and returns the sorted write plan. Findings veto
 * emission exactly like the Zod pipeline: nothing is written from a run
 * with any compile-time finding. `verify` recomputes the expected bytes
 * and reports `scenario.drift` per drifted, missing, or unreadable file.
 *
 * The port MODULE is never executed inside the confined adapter process:
 * the adapter validates the declaration (closed shape) and the module
 * surface is proven by execution in the project harness (the port
 * self-test plus the generated tests themselves).
 */

import { createHash } from "node:crypto";
import { canonicalJson, sha256 } from "./zod-emit.mjs";
import {
  IR_REF_MISMATCH,
  PORT_DOC_PATH,
  SCENARIO_IDENTITY,
  mapScenario,
} from "./scenario-map.mjs";
import {
  SCENARIO_DIR,
  emitScenarioTests,
} from "./scenario-emit.mjs";

/** The extension descriptor version (the adapter's release line). */
export const SCENARIO_EXTENSION_VERSION = "0.4.0";

/** The write scope every generated scenario-test file lives under. */
export const SCENARIO_WRITE_SCOPES = [`${SCENARIO_DIR}/**`];

/** The closed drift finding code of the verify operation. */
export const SCENARIO_DRIFT = "scenario.drift";

/** The canonical evidence home of the compiled project IR (core-owned). */
export const IR_EVIDENCE_HOME = ".lekalo/cache/ir";

/**
 * The scenario compiler descriptor. Like the transport generator it is a
 * composite member, never a registered launch extension — the composite
 * routes to it and merges its outcome.
 */
export const scenarioDescriptor = {
  id: "scenario-test-compiler",
  version: SCENARIO_EXTENSION_VERSION,
  operations: ["generate", "verify"],
  namedCapabilities: { "verify.scenarios": "full" },
  acceptedIrVersions: ["0.2.16"],
  writeScopes: SCENARIO_WRITE_SCOPES,
  invoke: (context) => scenarioOperation(context),
};

/**
 * The single extension entry. Failure paths are honest outcomes: refused
 * inputs return `failed` outcomes with bounded diagnostics, semantic
 * findings travel as typed findings, and successful applies report the
 * exact write receipt.
 */
export function scenarioOperation(context) {
  const { operation, request, readView } = context;
  try {
    const scenario = readDocument(readView, request.ir_path);
    if (scenario.refusal) {
      return { state: "failed", diagnostics: [{ reason: scenario.refusal }] };
    }
    if (scenario.document?.schemaVersion !== "lekalo/scenario-ir/v0.2.16") {
      return { state: "failed", diagnostics: [{ reason: "ir-version-unsupported" }] };
    }
    const projectId = scenario.document.projectId;
    if (typeof projectId !== "string" || !/^[a-z][a-z0-9-]*$/.test(projectId)) {
      return { state: "failed", diagnostics: [{ reason: "scenario-project-id" }] };
    }
    const irEvidence = readDocument(
      readView,
      `${IR_EVIDENCE_HOME}/${projectId}.json`,
    );
    if (irEvidence.refusal) {
      return {
        state: "failed",
        diagnostics: [{ reason: `ir-evidence-${irEvidence.refusal}` }],
      };
    }
    const portPresent = readView.canRead(PORT_DOC_PATH);
    let port = null;
    if (portPresent) {
      const portDocument = readDocument(readView, PORT_DOC_PATH);
      if (portDocument.refusal) {
        return {
          state: "failed",
          diagnostics: [{ reason: `port-${portDocument.refusal}` }],
        };
      }
      port = portDocument.document;
    }
    const portModulePath = port?.port?.path ?? null;
    if (typeof portModulePath !== "string" || portModulePath.length === 0) {
      // Generation without a declared port cannot bind anything: the
      // honest veto, projected through the shared findings path.
      const finding = { code: "scenario.port-missing", detail: "declaration-absent" };
      if (operation === "generate") {
        return { state: "complete", data: { writes: [], findings: [finding] } };
      }
      return verifyOperation(context, [], [finding]);
    }
    const mapped = mapScenario({
      scenario: scenario.document,
      ir: irEvidence.document,
      irDigest: sha256(irEvidence.text),
      port,
      portPresent,
      profileCapabilities: context.profile?.targetResolution?.capabilities ?? null,
    });
    if (mapped.state === "refused") {
      return {
        state: "failed",
        diagnostics: [{ reason: `scenario-${mapped.refusal}` }],
      };
    }
    const inputDigest = sha256(scenario.text);
    const files = emitScenarioTests({
      models: mapped.scenarios,
      inputDigest,
      adapterVersion: SCENARIO_EXTENSION_VERSION,
      portModulePath,
    }).map((emitted) => ({
      path: emitted.path,
      bytes: Buffer.from(emitted.text, "utf8"),
    }));
    if (operation === "generate") {
      return generateOperation(context, files, mapped.findings);
    }
    if (operation === "verify") {
      return verifyOperation(context, files, mapped.findings);
    }
    return { state: "unsupported" };
  } catch (error) {
    // Emitted as extension-failed by the kernel boundary; the local sink
    // keeps the bounded reason for diagnosis.
    throw new Error(`scenario-test-compiler: ${bounded(error?.message)}`);
  }
}

/** The plan identity of one write set (the shared composite domain). */
export function scenarioPlanIdOf(writes) {
  return "plan-" + sha256(canonicalJson(writes)).slice("sha256:".length);
}

function generateOperation(context, byteFiles, findings) {
  const { request, writeView } = context;
  if (findings.length > 0) {
    // Capability honesty: the mapper cannot express the document. Nothing
    // is emitted and nothing is written.
    return { state: "complete", data: { writes: [], findings } };
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
    data: { writes, findings: [], plan_id: scenarioPlanIdOf(writes) },
  };
}

function verifyOperation(context, byteFiles, findings) {
  const { readView } = context;
  const verification = [];
  for (const emitted of byteFiles) {
    if (!readView.canRead(emitted.path)) {
      verification.push({
        path: emitted.path,
        code: SCENARIO_DRIFT,
        detail: "unreadable-or-missing",
      });
      continue;
    }
    const observed = readView.readFile(emitted.path);
    if (!observed.equals(emitted.bytes)) {
      verification.push({
        path: emitted.path,
        code: SCENARIO_DRIFT,
        detail: `expected:${sha256Bytes(emitted.bytes).slice(7, 19)} observed:${sha256Bytes(observed).slice(7, 19)}`,
      });
    }
  }
  const all = [...findings, ...verification];
  return { state: "complete", data: { writes: [], findings: all } };
}

/** Read, decode, and parse one JSON document through the read view. */
function readDocument(readView, path) {
  if (!path || !readView.canRead(path)) {
    return { refusal: "unreadable" };
  }
  let bytes;
  try {
    bytes = readView.readFile(path);
  } catch {
    return { refusal: "unreadable" };
  }
  let text;
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return { refusal: "encoding" };
  }
  let document;
  try {
    document = JSON.parse(text);
  } catch {
    return { refusal: "json" };
  }
  if (document === null || typeof document !== "object") {
    return { refusal: "shape" };
  }
  return { document, text };
}

function sha256Bytes(bytes) {
  return "sha256:" + createHash("sha256").update(bytes).digest("hex");
}

function byPath(left, right) {
  return left.path < right.path ? -1 : left.path > right.path ? 1 : 0;
}

function bounded(text) {
  return String(text ?? "unknown").replace(/[^a-zA-Z0-9._: -]+/g, "?").slice(0, 128);
}

/** Re-export for the composite's document-identity routing. */
export { IR_REF_MISMATCH, SCENARIO_IDENTITY };

/**
 * The generation composite (merge of issues #45 and #70): the closed
 * target-protocol wire has exactly one `generate` operation and the
 * extension registry refuses duplicate operation claims, so the Zod
 * schema generator and the HTTP transport route generator compose under
 * this single descriptor rather than competing for the dispatch slot.
 *
 * Semantics of the composition:
 * - `generate` runs the Zod generator first (its writes go through the
 *   bounded staged write view) and then — only when the bound profile's
 *   read roots cover the transport evidence homes — the transport
 *   generator (issue #70's deployment gating, applied per invocation so
 *   an un-gated profile still receives the Zod output). The response is
 *   the sorted union of both write plans; plan identity is derived over
 *   the union, so the apply echo binds both halves.
 * - A profile that cannot read the transport evidence contributes zero
 *   transport writes rather than failing the union; a profile that can
 *   read it but lacks the evidence file still fails honestly (the
 *   transport extension's own refusal).
 * - Zod findings on generate keep their honest veto: an IR carrying
 *   constructs outside the declared subset projects as the partial
 *   error and nothing is emitted — including transport writes.
 * - `verify` delegates to the Zod verifier, the only registered
 *   verification implementation.
 */
import {
  descriptor as zodDescriptor,
  planIdOf,
  ZOD_WRITE_SCOPES,
} from "./zod-gen.mjs";
import {
  transportExtensionDescriptor,
  TRANSPORT_READ_ROOT,
  IR_READ_ROOT,
  ROUTE_WRITE_ROOT,
  TRANSPORT_CAPABILITY,
  OPENAPI_CAPABILITY,
} from "./transport-extension.mjs";
import {
  OPENAPI_WRITE_SCOPES,
  openapiGenerateOperation,
  openapiVerifyOperation,
} from "./openapi-gen.mjs";
import {
  scenarioDescriptor,
  SCENARIO_WRITE_SCOPES,
} from "./scenario-gen.mjs";
import {
  CLIENT_SDK_CAPABILITY,
  CLIENT_SDK_WRITE_SCOPES,
  CLIENT_SDK_WRITE_ROOT,
  CLIENT_SDK_EVIDENCE_DIR as CLIENT_SDK_READ_ROOT,
  clientSdkGenerateOperation,
  clientSdkVerifyOperation,
} from "./client-sdk-gen.mjs";

/** The resolved transport descriptor (never registered separately). */
const transport = transportExtensionDescriptor();

/** The composite descriptor version (the reserved product version). */
const COMPOSITE_VERSION = "0.4.0";

/**
 * Whether the bound profile's read roots cover the evidence homes one
 * generator reads — the same coverage predicate the kernel applies to
 * descriptor `readRoots`, evaluated per invocation inside the
 * composite. The transport and OpenAPI generators read the same two
 * evidence homes (transport plus compiled IR).
 */
function evidenceApplicable(profile) {
  if (!profile) {
    return false;
  }
  const required = transport.readRoots ?? [TRANSPORT_READ_ROOT, IR_READ_ROOT];
  return required.every((root) =>
    profile.readRoots.some((candidate) =>
      (candidate.kind === "tree"
        && (candidate.path === root || candidate.path.startsWith(root + "/")))
      || (candidate.kind === "file" && candidate.path.startsWith(root + "/"))));
}

/**
 * Whether the bound profile's read roots cover the client-SDK
 * evidence home one generator reads (issue #72). Evaluated per
 * invocation inside the composite; the SDK generator is opt-in.
 */
function sdkEvidenceApplicable(profile) {
  if (!profile) {
    return false;
  }
  return profile.readRoots.some((candidate) =>
    (candidate.kind === "tree"
      && (candidate.path === CLIENT_SDK_READ_ROOT
        || candidate.path.startsWith(CLIENT_SDK_READ_ROOT + "/")))
    || (candidate.kind === "file"
      && candidate.path.startsWith(CLIENT_SDK_READ_ROOT + "/")));
}

/** The deterministic path ordering shared by both write plans. */
function byPath(left, right) {
  return left.path < right.path ? -1 : left.path > right.path ? 1 : 0;
}

/**
 * Union any number of generation outcomes into one. Any non-complete
 * part fails the union with the concatenated bounded diagnostics; the
 * complete parts merge into sorted disjoint writes, concatenated
 * findings, and the union of evidence members.
 */
function unionOutcomes(outcomes) {
  if (outcomes.some((outcome) => outcome.state !== "complete")) {
    return {
      state: "failed",
      diagnostics: outcomes
        .flatMap((outcome) => outcome.diagnostics ?? [])
        .slice(0, 16),
    };
  }
  const writes = outcomes
    .flatMap((outcome) => outcome.data?.writes ?? [])
    .sort(byPath);
  const findings = outcomes
    .flatMap((outcome) => outcome.data?.findings ?? []);
  const data = { writes, findings };
  if (writes.length > 0 && findings.length === 0) {
    // The dry-run plan identity covers the union — the apply echo then
    // binds every half, and the core derives the same token over the
    // received writes.
    data.plan_id = planIdOf(writes);
  }
  return {
    state: "complete",
    data,
    evidence: Object.assign(
      {},
      ...outcomes.map((outcome) => outcome.evidence ?? {}),
    ),
  };
}

/**
 * The identity of the document at `ir_path`, read through the view:
 * `"scenario"` for a Scenario IR document (routed to the scenario-test
 * compiler), `"project"` for the compiled project IR (the Zod/transport
 * pipeline), `null` when the document carries neither closed identity —
 * the Zod pipeline then reports its own honest refusal.
 */
function documentIdentity(readView, irPath) {
  if (!irPath || typeof irPath !== "string" || !readView.canRead(irPath)) {
    return null;
  }
  let bytes;
  try {
    bytes = readView.readFile(irPath);
  } catch {
    return null;
  }
  let document;
  try {
    document = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    return null;
  }
  if (document === null || typeof document !== "object") {
    return null;
  }
  if (document.schemaVersion === "lekalo/scenario-ir/v0.2.16"
    && document.identity === "dev.lekalo.scenario-ir@0.2.16") {
    return "scenario";
  }
  // Issue #72: the client-SDK evidence is a first-class primary input
  // with its own closed identity; it routes to the SDK pipeline.
  if (document.schemaVersion === "lekalo/client-sdk/v0.4.0"
    && document.identity === "dev.lekalo.client-sdk@0.4.0") {
    return "client-sdk";
  }
  if (document.contract === "dev.lekalo.ir@0.2.16") {
    return "project";
  }
  return null;
}

/** The dispatch entry: document-identity routing; generate unions. */
function compositeOperation(context) {
  const { operation, request, readView } = context;
  const identity = documentIdentity(readView, request?.ir_path);
  if (identity === "scenario") {
    return scenarioDescriptor.invoke(context);
  }
  // Issue #72: a client-SDK evidence document is the SDK pipeline's
  // primary input — generate runs the SDK generator alone and verify
  // recomputes the SDK drift; the Zod/OpenAPI/transport halves are
  // inert for this document kind.
  if (identity === "client-sdk") {
    if (operation === "verify") {
      return clientSdkVerifyOperation(context);
    }
    return clientSdkGenerateOperation(context);
  }
  if (operation === "verify") {
    // The verify union: the Zod verifier plus the OpenAPI and
    // client-SDK verifiers when the evidence homes are readable.
    // Findings concatenate.
    const openapi = evidenceApplicable(context.profile)
      ? openapiVerifyOperation(context)
      : { state: "complete", data: { writes: [], findings: [] } };
    const sdk = sdkEvidenceApplicable(context.profile)
      ? clientSdkVerifyOperation(context)
      : { state: "complete", data: { writes: [], findings: [] } };
    const zod = zodDescriptor.invoke(context);
    if (zod.state !== "complete" || openapi.state !== "complete" || sdk.state !== "complete") {
      return {
        state: "failed",
        diagnostics: [zod, openapi, sdk]
          .filter((outcome) => outcome.state !== "complete")
          .flatMap((outcome) => outcome.diagnostics ?? [])
          .slice(0, 16),
      };
    }
    return {
      state: "complete",
      data: {
        writes: [],
        findings: [
          ...zod.data?.findings ?? [],
          ...openapi.data?.findings ?? [],
          ...sdk.data?.findings ?? [],
        ],
      },
    };
  }
  const zodOutcome = zodDescriptor.invoke(context);
  const applicable = evidenceApplicable(context.profile);
  const transportOutcome = applicable
    ? transport.invoke(context)
    : { state: "complete", data: { writes: [] } };
  const openapiOutcome = applicable
    ? openapiGenerateOperation(context)
    : { state: "complete", data: { writes: [] } };
  // Issue #72: the client-SDK generator is opt-in through the SDK
  // evidence home; a profile that cannot read it contributes zero SDK
  // writes instead of failing the union, so unrelated Zod/server-route
  // limitations never suppress it and vice versa. A profile that CAN
  // read the home but lacks the evidence file fails honestly.
  const sdkApplicable = sdkEvidenceApplicable(context.profile);
  const sdkOutcome = sdkApplicable
    ? clientSdkGenerateOperation(context)
    : { state: "complete", data: { writes: [] } };
  // The SDK evidence document carries its own closed identity, not an
  // IR `contract` member: when the SDK generator applies, the Zod
  // verifier's IR-shape refusal for the same file is expected and is
  // not the union's failure — the union keeps the honest SDK plan and
  // reports the Zod refusal only when the document is genuinely an
  // IR document (documentIdentity === "project").
  if (sdkOutcome.state === "complete" && !sdkIsPrimary(context)) {
    return unionOutcomes([zodOutcome, transportOutcome, openapiOutcome, sdkOutcome]);
  }
  if (zodOutcome.state === "failed" && sdkAppliesAndCompleted(sdkOutcome)) {
    return unionOutcomes([transportOutcome, openapiOutcome, sdkOutcome]);
  }
  return unionOutcomes([zodOutcome, transportOutcome, openapiOutcome, sdkOutcome]);
}

/** Whether the SDK evidence document is the dispatch's primary input
 * (the ir_path names a client-SDK document). */
function sdkIsPrimary(context) {
  const { request, readView } = context;
  const path = request?.ir_path;
  if (!path || typeof path !== "string" || !readView.canRead(path)) {
    return false;
  }
  try {
    const document = JSON.parse(
      new TextDecoder("utf-8", { fatal: true }).decode(readView.readFile(path)),
    );
    return (
      document !== null
      && typeof document === "object"
      && document.schemaVersion === "lekalo/client-sdk/v0.4.0"
    );
  } catch {
    return false;
  }
}

/** Whether the SDK generation applied while the Zod half failed. */
function sdkAppliesAndCompleted(sdkOutcome) {
  return (
    sdkOutcome.state === "complete"
    && (sdkOutcome.data?.writes ?? []).length > 0
  );
}

/** The descriptor the bundle entry registers for generation. */
export const descriptor = {
  id: "node-generation-composite",
  version: COMPOSITE_VERSION,
  operations: ["generate", "verify"],
  namedCapabilities: {
    "generate.zod": "full",
    [TRANSPORT_CAPABILITY]: "partial",
    [OPENAPI_CAPABILITY]: "partial",
    [CLIENT_SDK_CAPABILITY]: "partial",
    // Issue #47: the scenario-test compiler joins the composite and
    // advertises the existing reviewed capability id; the kernel's
    // default map keeps `verify.scenarios: "unsupported"` for the
    // extension-free describe.
    "verify.scenarios": "full",
  },
  acceptedIrVersions: ["0.2.16"],
  writeScopes: [...ZOD_WRITE_SCOPES, ROUTE_WRITE_ROOT, ...OPENAPI_WRITE_SCOPES, ...SCENARIO_WRITE_SCOPES, ...CLIENT_SDK_WRITE_SCOPES],
  writeRoots: [CLIENT_SDK_WRITE_ROOT],
  invoke: (context) => compositeOperation(context),
};

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
  scenarioDescriptor,
  SCENARIO_WRITE_SCOPES,
} from "./scenario-gen.mjs";

/** The resolved transport descriptor (never registered separately). */
const transport = transportExtensionDescriptor();

/** The composite descriptor version (the reserved product version). */
const COMPOSITE_VERSION = "0.4.0";

/**
 * Whether the bound profile's read roots cover the transport evidence
 * homes — the same coverage predicate the kernel applies to descriptor
 * `readRoots`, evaluated per invocation inside the composite.
 */
function transportApplicable(profile) {
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

/** The deterministic path ordering shared by both write plans. */
function byPath(left, right) {
  return left.path < right.path ? -1 : left.path > right.path ? 1 : 0;
}

/**
 * Union two generation outcomes into one. Any non-complete half fails
 * the union with the concatenated bounded diagnostics; two complete
 * halves merge into sorted disjoint writes, concatenated findings, and
 * the union of evidence members.
 */
function unionOutcomes(zodOutcome, transportOutcome) {
  const outcomes = [zodOutcome, transportOutcome];
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
    // binds both halves, and the core derives the same token over the
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
  if (document.contract === "dev.lekalo.ir@0.2.16") {
    return "project";
  }
  return null;
}

/** The dispatch entry: document-identity routing; generate unions. */
function compositeOperation(context) {
  const { operation, request, readView } = context;
  if (documentIdentity(readView, request?.ir_path) === "scenario") {
    return scenarioDescriptor.invoke(context);
  }
  if (operation === "verify") {
    return zodDescriptor.invoke(context);
  }
  const zodOutcome = zodDescriptor.invoke(context);
  const transportOutcome = transportApplicable(context.profile)
    ? transport.invoke(context)
    : { state: "complete", data: { writes: [] } };
  return unionOutcomes(zodOutcome, transportOutcome);
}

/** The descriptor the bundle entry registers for generation. */
export const descriptor = {
  id: "node-generation-composite",
  version: COMPOSITE_VERSION,
  operations: ["generate", "verify"],
  namedCapabilities: {
    "generate.zod": "full",
    [TRANSPORT_CAPABILITY]: "partial",
    [OPENAPI_CAPABILITY]: "unsupported",
    // Issue #47: the scenario-test compiler joins the composite and
    // advertises the existing reviewed capability id; the kernel's
    // default map keeps `verify.scenarios: "unsupported"` for the
    // extension-free describe.
    "verify.scenarios": "full",
  },
  acceptedIrVersions: ["0.2.16"],
  writeScopes: [...ZOD_WRITE_SCOPES, ROUTE_WRITE_ROOT, ...SCENARIO_WRITE_SCOPES],
  invoke: (context) => compositeOperation(context),
};

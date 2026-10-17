/**
 * The public surface of `@lekalo/adapter-sdk` (issue #106).
 *
 * Generated protocol/capability/diagnostic data plus the hand-maintained
 * runtime: strict decode, semantic validate, honest builders, the
 * validate-before-write boundary, path/scope helpers, deterministic
 * ids and canonical bytes, bounded stderr logging, and the per-exchange
 * serve loop. No ambient I/O beyond the serve loop; no network; no
 * telemetry; no `lekalo-core` coupling beyond the wire contracts.
 */

export {
  DIAGNOSTIC_CODES,
  DIAGNOSTIC_REGISTRY_DIGEST,
  DIAGNOSTIC_REGISTRY_IDENTITY,
} from "./generated/diagnostics.mjs";

export {
  CAPABILITY_DEFINITIONS,
  CAPABILITY_ENTRIES_DIGEST,
  CAPABILITY_IDENTITY,
} from "./generated/capabilities.mjs";

export {
  BASE_OPERATIONS,
  BOUNDS,
  ENUMS,
  IR_VERSIONS,
  LATEST_OPERATIONS,
  OPERATIONS_BY_VERSION,
  SUPPORTED_PROTOCOL_VERSIONS,
  WIRE_TOKEN,
} from "./generated/protocol.mjs";

export { COMPATIBILITY_MATRIX } from "./generated/compatibility_tests.mjs";

export {
  canonicalJson,
  duplicateKeys,
  readSingleObject,
  strictParse,
  utf8Length,
} from "./runtime/json.mjs";

export {
  isPlanId,
  isRequestId,
  isSha256Digest,
  planId,
  requestId,
  sha256,
  sha256Hex,
} from "./runtime/ids.mjs";

export {
  isCapabilityId,
  isContractVersion,
  isLogicalPath,
  isScope,
  isToken,
  pathViolation,
  planScopeViolation,
  protectedHome,
  scopeCovers,
  scopeTouchesProtectedHome,
} from "./runtime/paths.mjs";

export {
  buildArtifactManifest,
  canonicalPlanId,
  normalizeScenarioResult,
  sortWritePlan,
  writeEntryDigest,
  writePlanViolation,
} from "./runtime/canonical.mjs";

export { isLoggableToken, stderrLogger } from "./runtime/logger.mjs";

export { DECODE_REASONS, decodeRequest, decodeResponse } from "./runtime/decode.mjs";

export {
  VALIDATE_REASONS,
  isBaseOperation,
  isKnownOperation,
  isSupportState,
  validateRequest,
  validateResponseIdentity,
  validateResponsePayload,
} from "./runtime/validate.mjs";

export {
  SdkBuildError,
  buildAdapterIdentity,
  buildCapabilities,
  buildDiagnostic,
  buildError,
  buildFinding,
  buildOperationError,
  buildSuccess,
  responseBytes,
} from "./runtime/build.mjs";

export { admitResponse, runConformance, serve } from "./runtime/serve.mjs";

/**
 * Public type declarations of `@lekalo/adapter-sdk` (issue #106).
 *
 * The runtime is plain ESM with runtime-enforced closed contracts; these
 * declarations give TypeScript authors the same closed vocabulary. All
 * wire members keep the exact upstream snake_case spellings.
 */

export declare const WIRE_TOKEN: "lekalo.target/v1";
export declare const SUPPORTED_PROTOCOL_VERSIONS: readonly ["0.3.2", "0.6.4"];
export declare const BASE_OPERATIONS: readonly string[];
export declare const LATEST_OPERATIONS: readonly string[];
export declare const IR_VERSIONS: readonly string[];
export declare const OPERATIONS_BY_VERSION: readonly { operations: string[]; protocolVersion: string }[];

export declare const BOUNDS: Readonly<Record<string, number>>;
export declare const ENUMS: Readonly<Record<string, readonly string[]>>;

export declare const CAPABILITY_IDENTITY: string;
export declare const CAPABILITY_ENTRIES_DIGEST: string;
export declare const CAPABILITY_DEFINITIONS: readonly { definitionVersion: string; domain: string; id: string }[];

export declare const DIAGNOSTIC_REGISTRY_IDENTITY: string;
export declare const DIAGNOSTIC_REGISTRY_DIGEST: string;
export declare const DIAGNOSTIC_CODES: readonly { code: string; id: string }[];

export declare const COMPATIBILITY_MATRIX: readonly Record<string, unknown>[];

export type SdkResult<T> = { ok: true; value: T } | { ok: false; reason: string; detail?: string };

export interface AdapterIdentity {
  digest: string;
  id: string;
  version: string;
}

export interface Evidence {
  adapter: AdapterIdentity;
  plan_id?: string;
}

export interface RequestEnvelope {
  dry_run?: boolean;
  ir_path?: string;
  limits?: { max_output_bytes?: number; timeout_ms?: number };
  lint_request?: Record<string, unknown>;
  native_request?: Record<string, unknown>;
  operation: string;
  plan_id?: string;
  profile?: string;
  profile_capabilities?: { id: string; support: string }[];
  profile_digest?: string;
  project_root: string;
  protocol: string;
  protocol_version: string;
  request_id: string;
  target?: string;
}

export interface ResponseEnvelope {
  capabilities?: Record<string, unknown>;
  error?: {
    class: string;
    code: string;
    detail?: string[];
    message: string;
    partial?: boolean;
    retryable?: boolean;
  };
  evidence: Evidence;
  operation: string;
  progress?: { detail?: string; state: string; step: string }[];
  protocol: string;
  protocol_version: string;
  request_id: string;
  result?: Record<string, unknown>;
  status: string;
  writes?: { action: string; path: string; sha256?: string }[];
}

export declare function decodeRequest(text: string, maxBytes?: number): SdkResult<RequestEnvelope>;
export declare function decodeResponse(text: string, maxBytes?: number): SdkResult<ResponseEnvelope>;
export declare function validateRequest(request: RequestEnvelope): SdkResult<null>;
export declare function validateResponseIdentity(
  response: ResponseEnvelope,
  expected: { operation: string; protocolVersion: string; requestId: string },
): SdkResult<null>;
export declare function validateResponsePayload(request: RequestEnvelope, response: ResponseEnvelope): SdkResult<null>;

export declare class SdkBuildError extends Error {
  constructor(reason: string, detail?: string);
  reason: string;
}

export interface HandlerOutput {
  payload?: Record<string, unknown>;
  planId?: string;
  progress?: { detail?: string; state: string; step: string }[];
  writes?: { action: string; path: string; sha256?: string }[];
}

export interface Adapter {
  handlers: Record<string, (context: HandlerContext) => Promise<unknown> | unknown>;
  identity: AdapterIdentity;
}

export interface HandlerContext {
  cancel: { cancelled: boolean };
  io:unknown;
  log: { debug(message: string): void; error(message: string): void; info(message: string): void; warn(message: string): void };
  request: RequestEnvelope;
  token: { cancelled: boolean; deadlineMs?: number; readonly remainingMs?: number };
}

export declare function serve(adapter: Adapter, io?: unknown): Promise<number>;

export declare function runConformance(options: {
  adapterArgs: string[];
  executable: string;
  profile?: string;
  report?: string;
  repeats?: number;
  timeoutMs?: number;
}): Promise<{ code: number | null; stderr: string; stdout: string }>;

export declare function buildAdapterIdentity(identity: AdapterIdentity): AdapterIdentity;
export declare function buildCapabilities(options: Record<string, unknown>): Record<string, unknown>;
export declare function buildSuccess(options: {
  adapter: AdapterIdentity;
  capabilities?: Record<string, unknown>;
  payload?: Record<string, unknown>;
  planId?: string;
  progress?: unknown[];
  request: RequestEnvelope;
  writes?: unknown[];
}): ResponseEnvelope;
export declare function buildError(options: {
  adapter: AdapterIdentity;
  detail?: string[];
  errorClass: string;
  errorCode: string;
  message: string;
  request: RequestEnvelope;
}): ResponseEnvelope;
export declare function buildFinding(finding: { code: string; detail?: string; path: string }): Record<string, unknown>;
export declare function buildOperationError(options: Record<string, unknown>): Record<string, unknown>;
export declare function buildDiagnostic(
  codes: readonly { code: string; id: string }[],
  diagnostic: { code: string; data?: Record<string, string>; message: string; rule: string },
): Record<string, unknown>;
export declare function responseBytes(envelope: ResponseEnvelope): string;

export declare function canonicalJson(value: unknown): string;
export declare function strictParse(text: string): SdkResult<unknown>;
export declare function duplicateKeys(text: string): string[];
export declare function utf8Length(text: string): number;
export declare function readSingleObject(text: string, maxBytes: number): SdkResult<Record<string, unknown>>;

export declare function sha256(text: string): string;
export declare function sha256Hex(text: string): string;
export declare function isSha256Digest(value: string): boolean;
export declare function isRequestId(value: string): boolean;
export declare function isPlanId(value: string): boolean;
export declare function requestId(envelope: Record<string, unknown>): string;
export declare function planId(entries: unknown[]): string;

export declare function isLogicalPath(value: string): boolean;
export declare function isScope(value: string): boolean;
export declare function scopeCovers(scope: string, path: string): boolean;
export declare function protectedHome(path: string | null): string | null;
export declare function scopeTouchesProtectedHome(scope: string): string | null;
export declare function isToken(value: string): boolean;
export declare function isCapabilityId(value: string): boolean;
export declare function isContractVersion(value: string): boolean;
export declare function pathViolation(path: string): string | null;
export declare function planScopeViolation(entry: { action: string; path: string; sha256?: string }, writeScopes: string[]): string | null;

export declare function sortWritePlan(entries: unknown[]): unknown[] | null;
export declare function canonicalPlanId(sortedEntries: unknown[]): string;
export declare function writeEntryDigest(contentBytes: string): string;
export declare function buildArtifactManifest(entries: unknown[]): Record<string, unknown> | null;
export declare function normalizeScenarioResult(result: Record<string, unknown>): Record<string, unknown> | null;
export declare function writePlanViolation(entry: { action: string; path: string; sha256?: string }, writeScopes: string[]): string | null;

export declare function stderrLogger(tag: string): {
  debug(message: string): void;
  error(message: string): void;
  info(message: string): void;
  warn(message: string): void;
};
export declare function isLoggableToken(value: unknown): boolean;

export declare const DECODE_REASONS: readonly string[];
export declare const VALIDATE_REASONS: readonly string[];
export declare function isBaseOperation(operation: string): boolean;
export declare function isKnownOperation(operation: string): boolean;
export declare function isSupportState(value: string): boolean;

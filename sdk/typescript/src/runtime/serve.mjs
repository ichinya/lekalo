/**
 * The per-exchange process lifetime (issue #106): read exactly one
 * bounded request from stdin or the host-owned `--lekalo-request-file`
 * coordinate, decode and validate it, dispatch one operation callback,
 * and emit exactly one compact canonical response on stdout.
 *
 * Lifecycle rules implemented here: ambiguous input sources are refused,
 * cooperative deadline tokens reach the callbacks, logs stay on bounded
 * redacted stderr, an unknown request that cannot be answered yields a
 * bounded stderr line and a nonzero exit (never a fabricated success),
 * and a known request with failing builder output yields a valid closed
 * error envelope when possible.
 */
import { readFileSync, readSync } from "node:fs";
import { BOUNDS } from "../generated/protocol.mjs";
import { decodeRequest, decodeResponse } from "./decode.mjs";
import { validateRequest, validateResponseIdentity } from "./validate.mjs";
import { buildError, buildSuccess, responseBytes } from "./build.mjs";
import { stderrLogger } from "./logger.mjs";

const MAX_REQUEST_BYTES = BOUNDS.MAX_REQUEST_BYTES;

/**
 * Serve exactly one exchange. The adapter provides:
 * - `identity`: the `{id, version, digest}` identity block
 * - `handlers`: one function per operation. The `describe` handler
 *   returns the capability map; every other handler returns the builder
 *   fragments `{payload, planId, progress, writes}`.
 */
export async function serve(adapter, io = defaultIo()) {
  const log = stderrLogger(adapter.identity.id);
  if (io.fileRequested() && io.stdinPiped()) {
    log.error("transport: ambiguous input sources");
    return 1;
  }
  let raw;
  try {
    raw = io.readInput();
  } catch {
    log.error("transport: request input unreadable");
    return 1;
  }
  const decoded = decodeRequest(raw, MAX_REQUEST_BYTES);
  if (!decoded.ok) {
    log.error(`request refused: ${decoded.reason}${decoded.detail ? `: ${decoded.detail}` : ""}`);
    return 1;
  }
  const request = decoded.value;
  const validated = validateRequest(request);
  if (!validated.ok) {
    log.error(`request refused: ${validated.reason}`);
    return 1;
  }
  let response;
  try {
    response = await dispatch(adapter, request, log, io);
  } catch (error) {
    response = buildFailureResponse(adapter, request, error, log);
  }
  if (response === null) return 1;
  io.writeOutput(responseBytes(response));
  return 0;
}

async function dispatch(adapter, request, log, io) {
  const cancel = { cancelled: false };
  const token = makeDeadline(request.limits?.timeout_ms, cancel);
  const handlers = adapter.handlers ?? {};
  const handler = handlers[request.operation];
  if (typeof handler !== "function") {
    return buildError({
      adapter: adapter.identity,
      errorClass: "unsupported",
      errorCode: "operation-unsupported",
      message: "the operation is not implemented by this adapter",
      request,
    });
  }
  const output = await handler({ cancel, io, log, request, token });
  if (request.operation === "describe") {
    return buildSuccess({ adapter: adapter.identity, capabilities: output, request });
  }
  return buildSuccess({ adapter: adapter.identity, ...output, request });
}

function makeDeadline(timeoutMs, cancel) {
  const startedAt = Date.now();
  const token = {
    cancelled: false,
    deadlineMs: timeoutMs,
    get remainingMs() {
      if (timeoutMs === undefined) return undefined;
      return Math.max(0, timeoutMs - (Date.now() - startedAt));
    },
  };
  if (timeoutMs !== undefined) {
    setTimeout(() => {
      token.cancelled = true;
      cancel.cancelled = true;
    }, timeoutMs).unref();
  }
  return token;
}

/** Build the best legal failure response for a known request. */
function buildFailureResponse(adapter, request, error, log) {
  const reason = error && typeof error.reason === "string" ? error.reason : "builder";
  log.error(`response refused: ${reason}`);
  try {
    return buildError({
      adapter: adapter.identity,
      errorClass: "infrastructure",
      errorCode: "sdk-build-refused",
      message: "the adapter refused to emit an invalid response",
      request,
    });
  } catch {
    return null;
  }
}

/** The real process IO: stdin/file in, canonical stdout out. */
function defaultIo() {
  return {
    fileRequested: () => process.argv.includes("--lekalo-request-file"),
    readInput: readRequestInput,
    stdinPiped: () => process.stdin.isTTY === false,
    writeOutput: (bytes) => process.stdout.write(bytes),
  };
}

/** Read exactly the host-delivered request document. */
function readRequestInput() {
  const marker = process.argv.indexOf("--lekalo-request-file");
  if (marker !== -1 && process.argv[marker + 1]) {
    return readFileSync(process.argv[marker + 1], "utf8");
  }
  const chunk = 65_536;
  const parts = [];
  const buffer = Buffer.alloc(chunk);
  let total = 0;
  while (total <= MAX_REQUEST_BYTES) {
    let got;
    try {
      got = readSync(0, buffer, 0, chunk, total === 0 ? null : total);
    } catch {
      break;
    }
    if (got === 0) break;
    parts.push(buffer.toString("utf8", 0, got));
    total += got;
  }
  return parts.join("");
}

/**
 * Conformance client helper (issue #106): run the one real host battery
 * against an adapter executable. A bounded wrapper over the existing
 * `lekalo adapter test` command — executable plus argv, no shell.
 * Native verdicts, report documents, and exit codes pass through
 * unchanged; the wrapper never awards a badge itself and never reduces
 * a refusal to boolean success.
 */
export async function runConformance({ adapterArgs, executable, profile = "strict", report, repeats, timeoutMs }) {
  const { spawn } = await import("node:child_process");
  const argv = [executable, "adapter", "test", "--profile", profile];
  if (report) argv.push("--report", report);
  if (repeats !== undefined) argv.push("--repeats", String(repeats));
  if (timeoutMs !== undefined) argv.push("--timeout-ms", String(timeoutMs));
  argv.push(...adapterArgs);
  const child = spawn(argv[0], argv.slice(1), { stdio: ["ignore", "pipe", "pipe"] });
  let stdout = "";
  let stderr = "";
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  child.stderr.setEncoding("utf8");
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const code = await new Promise((resolve) => child.on("close", resolve));
  return { code, stderr, stdout };
}

/** Exposed for the shared vector corpus gates: strict response admission. */
export const admitResponse = decodeResponse;

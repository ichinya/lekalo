#!/usr/bin/env node
/**
 * The target adapter kernel `lekalo-target-node-typescript` — the issue
 * #43 protocol kernel plus the issue #44 launch seam for the vendored
 * TypeScript symbol scanner.
 *
 * The kernel remains, by itself, a read-only Node.js program built on
 * Node built-ins only: the protocol lifecycle (strict bounded request
 * decode, canonical response serialization, mandatory `describe`
 * handshake), injected-profile validation, and the extension seam. The
 * TypeScript compiler used by the #44 scanner is vendored into the
 * committed single-file `adapter.mjs` artifact (no external runtime
 * installation, no project node_modules, no NODE_PATH); this source file
 * never imports it.
 *
 * The kernel validates an injected resolved project profile and all of
 * its read roots lexically and physically before any extension dispatch,
 * refuses any scan request whose claimed profile resolution does not
 * exactly match the trusted `targetResolution` snapshot (review finding
 * F1), and normalizes extension outcomes into internal evidence envelopes
 * whose public wire projection stays inside the closed contract. It never
 * launches shell, package-manager, or project commands (#48 owns native
 * gates). Unsupported operations are honest `unsupported` errors, never
 * fake successes.
 *
 * Runtime entry points:
 *   node adapter.mjs                       one-shot protocol process
 *                                          (stdin or --lekalo-request-file)
 *   node adapter.mjs --version-json        local metadata probe on stdout:
 *                                          exact adapter id/version/entry
 *                                          digest and process.versions.node
 *   node adapter.mjs --lekalo-project-profile-json '<compact-json>'
 *                                          bounded explicit launch input
 *                                          (after the entry argument) that
 *                                          roots the kernel at one
 *                                          already-resolved project
 *                                          profile for scan-capable
 *                                          deployments
 *
 * Library consumers (tests and future owners #48) import `createKernel`
 * and drive the pure APIs directly; the one-shot main only runs when the
 * artifact is the process entry.
 *
 * Boundary decisions frozen for #43 (see README.md):
 * - The v0.2.16 wire has no runtime-version slot and no root-bearing
 *   profile transport; Node runtime metadata is reported by the local
 *   `--version-json` probe and internal evidence only, and read roots
 *   come exclusively from the injected `ResolvedProjectProfile`.
 * - Extension evidence is preserved completely in the internal envelope
 *   and the local-only sink; the public wire carries only what the
 *   closed contract can represent, and refuses lossy projections.
 */

import { createHash } from "node:crypto";
import {
  closeSync,
  lstatSync,
  openSync,
  fstatSync,
  readSync,
  readFileSync,
  realpathSync,
  statSync,
} from "node:fs";
import { isAbsolute, resolve, win32 } from "node:path";
import { fileURLToPath } from "node:url";

/** The stable wire token of the protocol line (issue #27). */
export const PROTOCOL_TOKEN = "lekalo.target/v1";
/** The sole protocol version this kernel speaks (the current contract). */
export const VERSION = "0.3.2";
/** The closed supported-version set: exact membership, never ranges. */
export const SUPPORTED_VERSIONS = Object.freeze([VERSION]);
/** The adapter identity token. */
export const ADAPTER_ID = "lekalo-target-node-typescript";
/** The adapter release version (the reserved product version). */
export const ADAPTER_VERSION = "0.3.2";
/** The maximum request size this kernel reads (mirrors the core bound). */
export const MAX_REQUEST_BYTES = 1024 * 1024;
/** The maximum response size this kernel writes (mirrors the core cap). */
export const MAX_RESPONSE_BYTES = 8 * 1024 * 1024;
/** Maximum lexer input, canonical output, and value bounds. */
export const MAX_JSON_DEPTH = 64;
/**
 * The maximum accepted size of the `--lekalo-project-profile-json` launch
 * input. The value arrives through a single Windows argv element, whose
 * practical element budget is 32 KiB, so 16 KiB of compact JSON is the
 * closed bound for this trusted input.
 */
export const MAX_PROFILE_JSON_BYTES = 16 * 1024;
/**
 * The closed evidence member carried by one scan entry (the v0.3.1
 * additive wire extension): a structural `sha256:` signature digest and
 * outbound semantic references. `signature` mirrors the observed
 * evidence field of the same name; `references` carries the existing
 * observed reference rows `{target, role, confidence}` as typed data —
 * never arbitrary JSON.
 */
const SCAN_ENTRY_EVIDENCE_KEYS = Object.freeze(["references", "signature"]);
const SCAN_ENTRY_REFERENCE_KEYS = Object.freeze(["confidence", "role", "target"]);
const SCAN_ENTRY_ROLES = Object.freeze([
  "read", "create", "update", "delete", "emit", "call", "reference",
]);
const SCAN_ENTRY_CONFIDENCES = Object.freeze([
  "exact", "high", "medium", "low", "unknown",
]);
/** The maximum number of references one scan entry may carry. */
export const MAX_SCAN_ENTRY_REFERENCES = 8;
/**
 * The maximum total serialized size in UTF-8 bytes of one entry's
 * evidence member; the core merges it into an 8 MiB response across up
 * to 10,000 entries, so the per-entry budget stays bounded.
 */
export const MAX_SCAN_ENTRY_EVIDENCE_BYTES = 4096;
/** The maximum single file read (mirrors the core's per-file byte bound). */
export const MAX_FILE_BYTES = 4 * 1024 * 1024;

const OPERATION_TOKENS = Object.freeze([
  "describe",
  "scan",
  "bind",
  "validate",
  "generate",
  "verify",
  "clean",
  "plan-clean",
  "plan-native",
]);

const SUPPORT_STATES = Object.freeze(["full", "partial", "unsupported", "unknown"]);

const CAPABILITY_IDS = Object.freeze([
  "generate.openapi",
  "generate.transport-http",
  "generate.ui",
  "generate.zod",
  "scan.symbols",
  "verify.scenarios",
  "verify.transport-http",
  "plan.native-gates",
]);

/** The exact entry digest: sha256 over the launched script's own bytes. */
export function entryDigest() {
  return "sha256:" + createHash("sha256").update(readSelfBytes()).digest("hex");
}

let selfBytes;
function readSelfBytes() {
  if (!selfBytes) {
    selfBytes = readFileSync(entryPath());
  }
  return selfBytes;
}

/** The absolute path of this script file, whatever the launch spelling. */
export function entryPath() {
  if (typeof process !== "undefined" && process.argv?.[1]
    && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
    return process.argv[1];
  }
  // Imported as a module (or launched through a runner that keeps argv
  // pointed at its own entry): this module's own URL is the artifact.
  return new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
}

/** The exact runtime metadata probe (also the local `--version-json`). */
export function runtimeMetadata() {
  return Object.freeze({
    adapter: Object.freeze({
      id: ADAPTER_ID,
      version: ADAPTER_VERSION,
      digest: entryDigest(),
    }),
    compiler: deepFreeze({ ...compilerMetadata }),
    node: String(process.versions.node),
  });
}

/**
 * Metadata of the vendored compiler inside this artifact. The default is
 * the compiler-free source deployment; the bundled artifact re-exports
 * this module with the exact build metadata injected (see build.mjs).
 */
export let compilerMetadata = Object.freeze({
  vendored: false,
});

/**
 * Set the vendored compiler metadata of this deployment. Called by the
 * bundle shim generated by `build.mjs`; never by the wire protocol.
 */
export function __setCompilerMetadata(metadata) {
  const invalid = (reason) => {
    throw new RequestRefusal("compiler-metadata", `invalid compiler metadata: ${reason}`);
  };
  if (typeof metadata !== "object" || metadata === null) invalid("not an object");
  const keys = Object.keys(metadata).sort();
  if (keys.join(",") !== "esbuild,typescript,vendored") invalid("members");
  if (metadata.vendored !== true) invalid("vendored");
  if (!/^\d+\.\d+\.\d+$/.test(metadata.typescript)) invalid("typescript");
  if (!/^\d+\.\d+\.\d+$/.test(metadata.esbuild)) invalid("esbuild");
  compilerMetadata = deepFreeze({
    vendored: true,
    typescript: metadata.typescript,
    esbuild: metadata.esbuild,
  });
}

let vendoredCompiler = null;
let embeddedLibs = null;
let launchExtensions = [];

/**
 * Register the production launch extensions (the scanner descriptor).
 * Called by the bundle entry generated by `build.mjs`; the source
 * deployment launches kernel-only and stays honest about it.
 */
export function __setLaunchExtensions(extensions) {
  if (!Array.isArray(extensions)) {
    throw new RequestRefusal("launch", "launch extensions must be an array");
  }
  launchExtensions = extensions;
}

/**
 * Attach the vendored compiler namespace and the embedded standard-library
 * map. Called once by the bundle entry generated by `build.mjs`; the
 * source deployment never attaches either, and scanner construction
 * without an attached compiler is a construction refusal, never a
 * project-environment fallback.
 */
export function __attachVendoredCompiler(ts, libFiles) {
  if (typeof ts !== "object" || ts === null || typeof ts.createProgram !== "function") {
    throw new RequestRefusal("compiler", "the vendored compiler namespace is malformed");
  }
  if (!(libFiles instanceof Map) || libFiles.size === 0) {
    throw new RequestRefusal("compiler", "the embedded library map is malformed");
  }
  vendoredCompiler = ts;
  embeddedLibs = libFiles;
}

/** The attached vendored compiler, or `null` on the source deployment. */
export function vendoredTs() {
  return vendoredCompiler;
}

/** The attached embedded standard-library map (name → text). */
export function embeddedLibFiles() {
  return embeddedLibs;
}

/** Whether one string is `sha256:` plus exactly 64 lowercase hex digits. */
export function isSha256Digest(value) {
  return typeof value === "string"
    && value.length === 71
    && value.startsWith("sha256:")
    && /^[0-9a-f]{64}$/.test(value.slice(7));
}

/** Whether one string is a `req-` request identifier. */
export function isRequestId(value) {
  return typeof value === "string"
    && value.length === 68
    && value.startsWith("req-")
    && /^[0-9a-f]{64}$/.test(value.slice(4));
}

/** Whether one string is a `plan-` plan identifier. */
export function isPlanId(value) {
  return typeof value === "string"
    && value.length === 69
    && value.startsWith("plan-")
    && /^[0-9a-f]{64}$/.test(value.slice(5));
}

/** Whether one string is a lowercase target/profile/adapter token. */
export function isToken(value) {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 64
    && /^[a-z][a-z0-9-]*$/.test(value);
}

/** Whether one string is a dotted capability id segment-wise. */
export function isCapabilityId(value) {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 128
    && value.split(".").every((segment) => /^[a-z0-9][a-z0-9_-]*$/.test(segment));
}

/** Whether one string is a canonical `x.y.z` contract version. */
export function isContractVersion(value) {
  return typeof value === "string"
    && value.length >= 5
    && value.length <= 32
    && /^\d+\.\d+\.\d+$/.test(value)
    && !/^\d{2,}|^0\d/.test(value.split(".")[0])
    && value.split(".").every((part) => part.length <= 8 && !part.startsWith("0") || part === "0");
}

/** Whether one string is a logical scope: a logical path or `.../**`. */
export function isScope(value) {
  const parts = logicalSegments(value);
  if (!parts) {
    return false;
  }
  const recursive = parts[parts.length - 1] === "**";
  const head = recursive ? parts.slice(0, -1) : parts;
  // A recursive scope is exactly a valid prefix plus the `/**` tail.
  return head.length >= 1 && head.every(isScopeSegment);
}

/** Whether one string is a logical path (no recursion marker). */
export function isLogicalPath(value) {
  const parts = logicalSegments(value);
  return Boolean(parts) && parts.every((segment) => segment !== "**" && isScopeSegment(segment));
}

function logicalSegments(value) {
  if (typeof value !== "string" || value.length === 0 || value.length > 512) {
    return null;
  }
  if (value.startsWith("/") || value.includes("\\")) {
    return null;
  }
  const parts = value.split("/");
  if (parts.some((part) => part.length === 0)) {
    return null;
  }
  return parts;
}

/** Whether one path segment is a portable scope/path segment. */
export function isScopeSegment(segment) {
  return typeof segment === "string"
    && segment.length >= 1
    && segment.length <= 64
    && segment !== "." && segment !== ".."
    && !segment.endsWith(".")
    && !segment.endsWith(" ")
    && !/~\d/.test(segment)
    && !isDosDevice(segment)
    && /^[a-z0-9.][a-z0-9._-]*$/.test(segment)
    && !isDriveOrScheme(segment);
}

function isDriveOrScheme(segment) {
  if (!/^[A-Za-z]/.test(segment)) {
    return false;
  }
  const run = segment.slice(1).match(/^[A-Za-z0-9+.\-]*/)[0].length;
  return segment[1] === ":" || segment[1 + run] === ":";
}

/** DOS device detection over the checker's closed device list. */
export function isDosDevice(segment) {
  const devices = [
    "con", "prn", "aux", "nul",
    "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
  ];
  const base = segment.split(".")[0];
  return devices.includes(base) || devices.includes(base.replace(/\$/g, ""));
}

/**
 * Segment-wise scope containment, mirroring the core: `src/**` covers
 * everything strictly below `src` and never `src` itself or `srcx/...`;
 * an exact scope covers exactly the equal path.
 */
export function scopeCovers(scope, path) {
  if (!isScope(scope) || !isLogicalPath(path)) {
    return false;
  }
  const scopeParts = scope.split("/");
  const pathParts = path.split("/");
  const recursive = scopeParts[scopeParts.length - 1] === "**";
  const prefix = recursive ? scopeParts.slice(0, -1) : scopeParts;
  if (recursive) {
    return pathParts.length > prefix.length
      && prefix.every((segment, index) => segment === pathParts[index]);
  }
  return pathParts.length === prefix.length
    && prefix.every((segment, index) => segment === pathParts[index]);
}

// ---------------------------------------------------------------------------
// 2. Bounded strict JSON: duplicate-aware lexer/parser and canonical writer.
// ---------------------------------------------------------------------------

/**
 * Decode bytes to text with fatal UTF-8: any malformed byte sequence is a
 * request refusal, never a best-effort replacement character.
 */
export function decodeUtf8Fatal(bytes) {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new RequestRefusal("utf-8", "request bytes are not valid UTF-8");
  }
}

/**
 * Decode exactly one JSON document from bytes with fatal UTF-8, closed
 * duplicate-key, depth, and trailing-document refusals. `JSON.parse`
 * alone silently collapses duplicate keys (including `{"a":1,"\u0061":2}`
 * aliases), accepts trailing whitespace-and-noise ambiguities poorly, and
 * never bounds depth; the kernel never uses it on untrusted bytes.
 */
export function decodeJsonDocument(bytes, { maxBytes = MAX_REQUEST_BYTES } = {}) {
  if (bytes.length > maxBytes) {
    throw new RequestRefusal("request-too-large", "request bytes exceed the transport bound");
  }
  const text = decodeUtf8Fatal(bytes);
  const parser = new StrictJsonParser(text);
  const value = parser.parseDocument();
  return value;
}

class StrictJsonParser {
  constructor(text) {
    this.text = text;
    this.position = 0;
    // Every decoded key spelling, per object, in arrival order.
    this.keyAliases = null;
  }

  parseDocument() {
    this.skipWhitespace();
    const value = this.parseValue(0);
    this.skipWhitespace();
    if (this.position !== this.text.length) {
      throw new RequestRefusal("trailing-content", "more than one JSON document");
    }
    return value;
  }

  skipWhitespace() {
    while (this.position < this.text.length) {
      const code = this.text.charCodeAt(this.position);
      if (code === 0x20 || code === 0x09 || code === 0x0a || code === 0x0d) {
        this.position += 1;
      } else {
        break;
      }
    }
  }

  parseValue(depth) {
    if (depth > MAX_JSON_DEPTH) {
      throw new RequestRefusal("depth", "JSON nesting exceeds the closed bound");
    }
    const next = this.text[this.position];
    switch (next) {
      case "{":
        return this.parseObject(depth);
      case "[":
        return this.parseArray(depth);
      case '"':
        return this.parseString();
      case "t":
        return this.parseLiteral("true", true);
      case "f":
        return this.parseLiteral("false", false);
      case "n":
        return this.parseLiteral("null", null);
      default:
        if (next === "-" || (next >= "0" && next <= "9")) {
          return this.parseNumber();
        }
        throw new RequestRefusal("syntax", "malformed JSON document");
    }
  }

  parseLiteral(word, value) {
    if (this.text.startsWith(word, this.position)) {
      this.position += word.length;
      return value;
    }
    throw new RequestRefusal("syntax", `malformed JSON literal`);
  }

  parseNumber() {
    const start = this.position;
    if (this.text[this.position] === "-") {
      this.position += 1;
    }
    const digits = /^\d*/;
    const head = digits.exec(this.text.slice(this.position))[0];
    if (head.length === 0 || (head.length > 1 && head.startsWith("0"))) {
      throw new RequestRefusal("syntax", "malformed JSON number");
    }
    this.position += head.length;
    if (this.text[this.position] === ".") {
      this.position += 1;
      const fraction = digits.exec(this.text.slice(this.position))[0];
      if (fraction.length === 0) {
        throw new RequestRefusal("syntax", "malformed JSON fraction");
      }
      this.position += fraction.length;
    }
    if (this.text[this.position] === "e" || this.text[this.position] === "E") {
      this.position += 1;
      if (this.text[this.position] === "+" || this.text[this.position] === "-") {
        this.position += 1;
      }
      const exponent = digits.exec(this.text.slice(this.position))[0];
      if (exponent.length === 0) {
        throw new RequestRefusal("syntax", "malformed JSON exponent");
      }
      this.position += exponent.length;
    }
    const raw = this.text.slice(start, this.position);
    const value = Number(raw);
    if (!Number.isFinite(value)) {
      throw new RequestRefusal("syntax", "non-finite JSON number");
    }
    return value;
  }

  parseString() {
    this.position += 1; // opening quote
    let result = "";
    for (;;) {
      const character = this.text[this.position];
      if (character === undefined) {
        throw new RequestRefusal("syntax", "unterminated JSON string");
      }
      if (character === '"') {
        this.position += 1;
        return result;
      }
      if (character === "\\") {
        this.position += 1;
        const escape = this.text[this.position];
        switch (escape) {
          case '"': result += '"'; this.position += 1; break;
          case "\\": result += "\\"; this.position += 1; break;
          case "/": result += "/"; this.position += 1; break;
          case "b": result += "\b"; this.position += 1; break;
          case "f": result += "\f"; this.position += 1; break;
          case "n": result += "\n"; this.position += 1; break;
          case "r": result += "\r"; this.position += 1; break;
          case "t": result += "\t"; this.position += 1; break;
          case "u": {
            const hex = this.text.slice(this.position + 1, this.position + 5);
            if (!/^[0-9a-fA-F]{4}$/.test(hex)) {
              throw new RequestRefusal("syntax", "malformed unicode escape");
            }
            const code = Number.parseInt(hex, 16);
            result += String.fromCharCode(code);
            this.position += 5;
            break;
          }
          default:
            throw new RequestRefusal("syntax", "malformed JSON escape");
        }
        continue;
      }
      const code = character.charCodeAt(0);
      if (code < 0x20) {
        throw new RequestRefusal("syntax", "unescaped control character");
      }
      result += character;
      this.position += 1;
    }
  }

  parseArray(depth) {
    this.position += 1;
    const array = [];
    this.skipWhitespace();
    if (this.text[this.position] === "]") {
      this.position += 1;
      return array;
    }
    for (;;) {
      this.skipWhitespace();
      array.push(this.parseValue(depth + 1));
      this.skipWhitespace();
      const character = this.text[this.position];
      if (character === ",") {
        this.position += 1;
        continue;
      }
      if (character === "]") {
        this.position += 1;
        return array;
      }
      throw new RequestRefusal("syntax", "malformed JSON array");
    }
  }

  parseObject(depth) {
    this.position += 1;
    const object = Object.create(null);
    const seen = new Set();
    this.skipWhitespace();
    if (this.text[this.position] === "}") {
      this.position += 1;
      return object;
    }
    for (;;) {
      this.skipWhitespace();
      if (this.text[this.position] !== '"') {
        throw new RequestRefusal("syntax", "malformed JSON object key");
      }
      const key = this.parseString();
      if (seen.has(key)) {
        throw new RequestRefusal("duplicate-key", `duplicate decoded object key`);
      }
      seen.add(key);
      this.skipWhitespace();
      if (this.text[this.position] !== ":") {
        throw new RequestRefusal("syntax", "malformed JSON object separator");
      }
      this.position += 1;
      this.skipWhitespace();
      object[key] = this.parseValue(depth + 1);
      this.skipWhitespace();
      const character = this.text[this.position];
      if (character === ",") {
        this.position += 1;
        continue;
      }
      if (character === "}") {
        this.position += 1;
        return object;
      }
      throw new RequestRefusal("syntax", "malformed JSON object");
    }
  }
}

/**
 * Canonical compact JSON: keys in UTF-8 byte order, no whitespace, LF
 * never inserted. Arrays keep explicit identity order. Mirrors the core
 * serializer's output byte for byte on closed shapes.
 */
export function canonicalJson(value) {
  const text = writeCanonical(value);
  const size = Buffer.byteLength(text, "utf8");
  if (size > MAX_RESPONSE_BYTES) {
    throw new RequestRefusal("response-too-large", "canonical response exceeds the transport bound");
  }
  return text;
}

function writeCanonical(value) {
  if (value === null) {
    return "null";
  }
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) {
        throw new RequestRefusal("syntax", "non-finite number cannot be serialized");
      }
      return canonicalNumber(value);
    case "string":
      return JSON.stringify(value);
    case "object":
      break;
    default:
      throw new RequestRefusal("syntax", "unserializable JSON value");
  }
  if (Array.isArray(value)) {
    return `[${value.map(writeCanonical).join(",")}]`;
  }
  const keys = Object.keys(value);
  const canonical = Array.from(new Uint8Array(Buffer.from(keys[0] ?? "", "utf8")));
  let canonicalKey = keys[0] ?? "";
  const sorted = keys.sort((a, b) => {
    const left = Buffer.from(a, "utf8");
    const right = Buffer.from(b, "utf8");
    const length = Math.min(left.length, right.length);
    for (let index = 0; index < length; index += 1) {
      if (left[index] !== right[index]) {
        return left[index] - right[index];
      }
    }
    return left.length - right.length;
  });
  void canonical;
  void canonicalKey;
  const body = sorted
    .map((key) => `${JSON.stringify(key)}:${writeCanonical(value[key])}`)
    .join(",");
  return `{${body}}`;
}

function canonicalNumber(value) {
  if (Number.isInteger(value) && Math.abs(value) < 1e15) {
    return String(value);
  }
  return JSON.stringify(value);
}

/** SHA-256 hex of the given UTF-8 bytes (internal binding anchors). */
export function sha256Hex(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

// ---------------------------------------------------------------------------
// 3. Closed request/response envelope validation.
// ---------------------------------------------------------------------------

/** Why a request was refused before any operation could run. */
export class RequestRefusal extends Error {
  constructor(code, message) {
    super(message ?? code);
    this.name = "RequestRefusal";
    this.code = code;
  }
}

function hasOwn(object, key) {
  return Object.prototype.hasOwnProperty.call(object, key);
}

/**
 * Canonical JSON text of one closed plain value (sorted keys, compact),
 * used for binding-tuple digests and strict comparisons. Distinct from
 * the bounded transport writer: this one has no response-size gate and
 * refuses nothing but unserializable values.
 */
function canonicalJsonText(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean": return value ? "true" : "false";
    case "number": {
      if (!Number.isFinite(value)) {
        throw new RequestRefusal("syntax", "non-finite number cannot be canonicalized");
      }
      return Number.isInteger(value) && Math.abs(value) < 1e15 ? String(value) : JSON.stringify(value);
    }
    case "string": return JSON.stringify(value);
    case "object": break;
    default:
      throw new RequestRefusal("syntax", "unserializable value cannot be canonicalized");
  }
  if (Array.isArray(value)) {
    return `[${value.map(canonicalJsonText).join(",")}]`;
  }
  const keys = Object.keys(value).sort((left, right) => {
    const leftBytes = Buffer.from(left, "utf8");
    const rightBytes = Buffer.from(right, "utf8");
    const length = Math.min(leftBytes.length, rightBytes.length);
    for (let index = 0; index < length; index += 1) {
      if (leftBytes[index] !== rightBytes[index]) {
        return leftBytes[index] - rightBytes[index];
      }
    }
    return leftBytes.length - rightBytes.length;
  });
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJsonText(value[key])}`).join(",")}}`;
}

/**
 * Validate one closed native_request member (issue #48, protocol 0.3.2):
 * bounded changed inputs plus digest-addressed custody references. A
 * content reference is a digest plus optional provenance — never a
 * command or an absolute URL.
 */
export function validateNativeRequest(nativeRequest) {
  if (typeof nativeRequest !== "object" || nativeRequest === null || Array.isArray(nativeRequest)) {
    throw new RequestRefusal("native-request", "native_request must be an object");
  }
  for (const key of Object.keys(nativeRequest)) {
    if (!NATIVE_REQUEST_KEYS.includes(key)) {
      throw new RequestRefusal("native-request", "unknown native_request member");
    }
  }
  for (const key of ["changes", "scan_ref", "execution_policy_ref",
    "input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"]) {
    if (!hasOwn(nativeRequest, key)) {
      throw new RequestRefusal("native-request", "missing native_request member");
    }
  }
  const changes = nativeRequest.changes;
  if (typeof changes !== "object" || changes === null || Array.isArray(changes)) {
    throw new RequestRefusal("native-request", "changes must be an object");
  }
  if (!Array.isArray(changes.files) || changes.files.length > 1024) {
    throw new RequestRefusal("native-request", "changes.files bound");
  }
  for (const file of changes.files) {
    if (typeof file !== "object" || file === null || Array.isArray(file)) {
      throw new RequestRefusal("native-request", "changes.files entry");
    }
    if (!isLogicalPath(file.path)) {
      throw new RequestRefusal("native-request", "changes.files path");
    }
    if (!["added", "modified", "deleted", "renamed"].includes(file.change)) {
      throw new RequestRefusal("native-request", "changes.files change");
    }
    if (file.before_digest !== undefined && !isSha256Digest(file.before_digest)) {
      throw new RequestRefusal("native-request", "changes.files before_digest");
    }
    if (file.after_digest !== undefined && !isSha256Digest(file.after_digest)) {
      throw new RequestRefusal("native-request", "changes.files after_digest");
    }
  }
  if (!Array.isArray(changes.symbols) || changes.symbols.length > 1024) {
    throw new RequestRefusal("native-request", "changes.symbols bound");
  }
  validateNativeContentRef(nativeRequest.scan_ref);
  if (nativeRequest.observed_ref !== undefined) {
    validateNativeContentRef(nativeRequest.observed_ref);
  }
  const policy = nativeRequest.execution_policy_ref;
  if (typeof policy !== "object" || policy === null || Array.isArray(policy)
    || typeof policy.id !== "string" || policy.id.length === 0 || policy.id.length > 128
    || !isContractVersion(policy.version) || !isSha256Digest(policy.digest)) {
    throw new RequestRefusal("native-request", "execution_policy_ref");
  }
  for (const key of ["input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"]) {
    if (!isSha256Digest(nativeRequest[key])) {
      throw new RequestRefusal("native-request", key);
    }
  }
}

function validateNativeContentRef(reference) {
  if (typeof reference !== "object" || reference === null || Array.isArray(reference)
    || !isSha256Digest(reference.digest)) {
    throw new RequestRefusal("native-request", "content reference");
  }
  if (reference.revision !== undefined
    && (typeof reference.revision !== "string" || reference.revision.length === 0 || reference.revision.length > 128)) {
    throw new RequestRefusal("native-request", "content reference revision");
  }
  if (reference.adapter !== undefined && !isToken(reference.adapter)) {
    throw new RequestRefusal("native-request", "content reference adapter");
  }
}

/** The closed request envelope keys and their per-operation legality. */
const REQUEST_KEYS = Object.freeze([
  "protocol", "protocol_version", "operation", "request_id", "project_root",
  "ir_path", "target", "profile", "profile_digest", "profile_capabilities",
  "dry_run", "limits", "plan_id", "native_request",
]);

/** The closed native_request member keys (issue #48, protocol 0.3.2). */
const NATIVE_REQUEST_KEYS = Object.freeze([
  "changes", "scan_ref", "observed_ref", "execution_policy_ref",
  "input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest",
]);

/**
 * Decode and semantically validate one request envelope object already
 * parsed by the strict decoder. Mirrors every rule of the core's
 * `validate_request` so both sides refuse the same documents.
 */
export function validateRequestObject(document) {
  if (typeof document !== "object" || document === null || Array.isArray(document)) {
    throw new RequestRefusal("shape", "request must be a JSON object");
  }
  for (const key of Object.keys(document)) {
    if (!REQUEST_KEYS.includes(key)) {
      throw new RequestRefusal("unknown-key", `unknown request member`);
    }
    if (document[key] === null) {
      throw new RequestRefusal("null-member", "explicit null is never part of v1");
    }
  }
  // An `undefined` value on a present key is an absent member (mirrors
  // serde's Option handling); normalize before pairing checks.
  const request = { ...document };
  for (const key of Object.keys(request)) {
    if (request[key] === undefined) {
      delete request[key];
    }
  }
  for (const key of ["protocol", "protocol_version", "operation", "request_id", "project_root"]) {
    if (!hasOwn(request, key)) {
      throw new RequestRefusal("missing-key", `missing required request member`);
    }
  }
  if (request.protocol !== PROTOCOL_TOKEN) {
    throw new RequestRefusal("protocol-token", "unknown protocol token");
  }
  if (!SUPPORTED_VERSIONS.includes(request.protocol_version)) {
    throw new RequestRefusal("protocol-version", "unsupported protocol version");
  }
  if (!isRequestId(request.request_id)) {
    throw new RequestRefusal("request-id", "malformed request identifier");
  }
  if (!OPERATION_TOKENS.includes(request.operation)) {
    throw new RequestRefusal("operation", "unknown operation");
  }
  if (request.project_root !== ".") {
    throw new RequestRefusal("project-root", "project_root must denote the core's private view");
  }
  const optionalStrings = [
    ["ir_path", isLogicalPath],
    ["target", isToken],
    ["profile", isToken],
  ];
  for (const [key, check] of optionalStrings) {
    if (hasOwn(request, key) && request[key] !== undefined && !check(request[key])) {
      throw new RequestRefusal("grammar", `malformed ${key}`);
    }
  }
  if (hasOwn(request, "limits")) {
    const limits = request.limits;
    if (typeof limits !== "object" || limits === null || Array.isArray(limits)) {
      throw new RequestRefusal("shape", "limits must be an object");
    }
    const keys = Object.keys(limits).sort();
    if (keys.join(",") !== "max_output_bytes,timeout_ms") {
      throw new RequestRefusal("shape", "limits members are closed");
    }
    const timeout = limits.timeout_ms;
    const output = limits.max_output_bytes;
    if (!Number.isInteger(timeout) || timeout < 1 || timeout > 3600000
      || !Number.isInteger(output) || output < 1 || output > 1073741824) {
      throw new RequestRefusal("shape", "limits values are outside the closed bounds");
    }
  }
  const operation = request.operation;
  const apply = operation === "clean" || (operation === "generate" && request.dry_run === false);
  if (apply !== hasOwn(request, "plan_id")
    || (hasOwn(request, "plan_id") && !isPlanId(document.plan_id))) {
    throw new RequestRefusal("plan-id", "plan identity pairing is wrong");
  }
  // Issue #48: native_request is required on plan-native and forbidden
  // on every other operation; plan-native is read-only (no dry_run,
  // no plan_id) and requires the current version.
  if ((operation === "plan-native") !== hasOwn(request, "native_request")) {
    throw new RequestRefusal("native-request", "native_request pairing is wrong");
  }
  if (operation === "plan-native") {
    if (hasOwn(request, "dry_run") || hasOwn(request, "plan_id")) {
      throw new RequestRefusal("plan-id", "plan-native is read-only");
    }
    if (request.protocol_version !== VERSION) {
      throw new RequestRefusal("member", "plan-native requires the current version");
    }
    validateNativeRequest(request.native_request);
  }
  if ((operation === "generate") !== hasOwn(request, "dry_run")) {
    throw new RequestRefusal("dry-run", "dry_run pairing is wrong");
  }
  if (hasOwn(request, "dry_run") && typeof request.dry_run !== "boolean") {
    throw new RequestRefusal("dry-run", "dry_run must be a boolean");
  }
  if ((operation === "validate" || operation === "generate" || operation === "verify")
    && !hasOwn(request, "ir_path")) {
    throw new RequestRefusal("ir-path", "IR-carrying operations require ir_path");
  }
  if ((operation === "generate" || operation === "bind") && !hasOwn(request, "target")) {
    throw new RequestRefusal("target", "this operation requires target");
  }
  if (operation === "bind" && !hasOwn(request, "profile")) {
    throw new RequestRefusal("profile", "bind requires profile");
  }
  if (operation === "describe"
    && ["target", "profile", "profile_digest", "profile_capabilities", "ir_path"]
      .some((key) => hasOwn(request, key))) {
    throw new RequestRefusal("member", "describe carries no operation members");
  }
  const resolution = hasOwn(request, "profile_digest") || hasOwn(request, "profile_capabilities");
  if (resolution && (!hasOwn(request, "profile_digest") || !hasOwn(request, "profile_capabilities"))) {
    throw new RequestRefusal("profile-capabilities", "profile resolution members are paired");
  }
  if (resolution) {
    if (request.protocol_version !== VERSION) {
      throw new RequestRefusal("member", "profile resolution requires the current version");
    }
    if (!hasOwn(request, "profile")) {
      throw new RequestRefusal("profile", "profile resolution requires the profile token");
    }
    if (!isSha256Digest(request.profile_digest)) {
      throw new RequestRefusal("profile-digest", "malformed profile digest");
    }
    validateProfileCapabilities(request.profile_capabilities);
  }
  return request;
}

function validateProfileCapabilities(capabilities) {
  if (!Array.isArray(capabilities) || capabilities.length === 0 || capabilities.length > 64) {
    throw new RequestRefusal("profile-capabilities", "capability snapshot size is outside the bounds");
  }
  let previous = "";
  for (const capability of capabilities) {
    if (typeof capability !== "object" || capability === null || Array.isArray(capability)) {
      throw new RequestRefusal("profile-capabilities", "capability entries are objects");
    }
    const keys = Object.keys(capability).sort();
    if (keys.join(",") !== "id,support") {
      throw new RequestRefusal("profile-capabilities", "capability entry members are closed");
    }
    if (!isCapabilityId(capability.id) || !SUPPORT_STATES.includes(capability.support)) {
      throw new RequestRefusal("profile-capabilities", "capability entry is malformed");
    }
    if (Buffer.compare(Buffer.from(capability.id, "utf8"), Buffer.from(previous, "utf8")) <= 0) {
      throw new RequestRefusal("profile-capabilities", "capability snapshot is not strictly sorted");
    }
    previous = capability.id;
  }
}

/** The fixed code/message of every in-envelope error this kernel emits. */
const ERROR_CODES = Object.freeze({
  unsupported: Object.freeze({
    code: "operation-unsupported",
    message: "this kernel does not implement the requested operation",
  }),
  invalidProfile: Object.freeze({
    code: "profile-invalid",
    message: "the resolved project profile is not valid for this kernel",
  }),
  rootInvalid: Object.freeze({
    code: "root-invalid",
    message: "a configured read root failed lexical or physical validation",
  }),
  extensionFailed: Object.freeze({
    code: "extension-failed",
    message: "the registered extension failed inside the dispatch boundary",
  }),
});

export { ERROR_CODES };

/**
 * The trusted launch profile: the closed projection of one
 * `--lekalo-project-profile-json` input after strict decoding. Shape
 * mirrors the resolved project profile plus the target/operation
 * metadata the descriptor projects; authority stays with the owner of
 * the `AdapterCommand` argv, never with the wire request.
 */
export function decodeProjectProfileJson(text) {
  if (typeof text !== "string") {
    throw new RequestRefusal("profile-input", "the launch profile must be one JSON text");
  }
  const bytes = Buffer.from(text, "utf8");
  if (bytes.length === 0 || bytes.length > MAX_PROFILE_JSON_BYTES) {
    throw new RequestRefusal("profile-input", "the launch profile exceeds the closed input bound");
  }
  const document = decodeJsonDocument(bytes, { maxBytes: MAX_PROFILE_JSON_BYTES });
  const profile = validateResolvedProjectProfile(document);
  return deepFreeze({ profile });
}

/**
 * The single pre-dispatch profile-binding validation (review finding F1):
 * the request profile must equal the trusted profile id, the request
 * target must equal the trusted target when present, and a claimed
 * profile resolution must exactly match the trusted `targetResolution`
 * snapshot — both members present or both absent, digest equal, and the
 * capability set deeply equal as a sorted canonical sequence. Subset,
 * superset, reordered, or otherwise-different capability claims are
 * refused; a snapshot without the request pair, or a pair without the
 * snapshot, is refused. Returns one bounded refusal detail, or `null`
 * when the binding holds.
 */
export function validateProfileBinding(request, profile) {
  const refusal = (detail) => ({ detail });
  if (!profile) {
    // No trusted profile: the kernel dispatches nothing but describe
    // (enforced by the caller); any operation request is a mismatch.
    return refusal("profile-absent");
  }
  if (request.profile !== profile.id) {
    return refusal("profile-id");
  }
  if (request.target !== undefined && request.target !== profile.target) {
    return refusal("target");
  }
  const claimed = request.profile_digest !== undefined || request.profile_capabilities !== undefined;
  const trusted = profile.targetResolution !== undefined;
  if (claimed !== trusted) {
    return refusal(claimed ? "snapshot-missing" : "resolution-missing");
  }
  if (!trusted) {
    return null;
  }
  if (request.profile_digest !== profile.targetResolution.digest) {
    return refusal("digest");
  }
  const expected = profile.targetResolution.capabilities;
  const claimedCapabilities = request.profile_capabilities;
  if (claimedCapabilities.length !== expected.length) {
    return refusal("capability-count");
  }
  for (let index = 0; index < expected.length; index += 1) {
    const left = claimedCapabilities[index];
    const right = expected[index];
    if (left.id !== right.id || left.support !== right.support) {
      return refusal("capability-set");
    }
  }
  return null;
}

/** The descriptor this production kernel serves (no roots, no extensions). */
export function describeCapabilities(profile = null, extensions = []) {
  const capabilities = {
    adapter: {
      id: ADAPTER_ID,
      version: ADAPTER_VERSION,
      digest: entryDigest(),
    },
    protocol_versions: [...SUPPORTED_VERSIONS],
    operations: ["describe"],
    transports: ["stdin", "file"],
    targets: ["node-typescript"],
    profiles: ["standalone"],
    read_scopes: [],
    write_scopes: [],
    progress: false,
    ir_versions: [],
    capabilities: {
      "generate.openapi": "unsupported",
      "generate.ui": "unsupported",
      "generate.zod": "unsupported",
      "scan.symbols": "unsupported",
      "verify.scenarios": "unsupported",
    },
  };
  if (profile && extensions.length > 0) {
    // F6/F7: the extension projection uses exactly the same gating
    // predicate as dispatch — extensions are advertised only when a
    // validated profile is bound, and `ir_versions` becomes the sorted
    // unique union of the enabled descriptors' accepted versions.
    const operations = new Set(["describe"]);
    const capabilitiesMap = { ...capabilities.capabilities };
    const irVersions = new Set();
    for (const extension of extensions) {
      for (const operation of extension.operations) {
        operations.add(operation);
      }
      for (const [id, state] of Object.entries(extension.namedCapabilities ?? {})) {
        capabilitiesMap[id] = state;
      }
      for (const version of extension.acceptedIrVersions ?? []) {
        irVersions.add(version);
      }
    }
    capabilities.operations = [...operations].sort();
    capabilities.capabilities = capabilitiesMap;
    capabilities.read_scopes = profile.readRoots.map((root) => root.scope);
    capabilities.profiles = [profile.id];
    capabilities.ir_versions = [...irVersions].sort();
  }
  return capabilities;
}

/**
 * Build one response envelope for `request`. `payload` may carry
 * `capabilities`, `result`, `writes`, `progress`, `error`; pairing rules
 * are enforced before serialization.
 */
export function buildResponse(request, payload = {}) {
  const status = hasOwn(payload, "error") ? "error" : "ok";
  const forbidden = status === "error"
    ? ["result", "capabilities", "progress", "writes"]
    : ["error"];
  for (const key of forbidden) {
    if (hasOwn(payload, key)) {
      throw new RequestRefusal("error-pairing", `a ${status} response cannot carry ${key}`);
    }
  }
  const envelope = {
    protocol: PROTOCOL_TOKEN,
    protocol_version: request.protocol_version,
    operation: request.operation,
    request_id: request.request_id,
    status,
    evidence: buildEvidence(request, payload.evidence),
  };
  for (const key of ["capabilities", "result", "writes", "progress", "error"]) {
    if (hasOwn(payload, key)) {
      envelope[key] = payload[key];
    }
  }
  return envelope;
}

function buildEvidence(request, internal) {
  // The closed wire evidence carries adapter identity and — for planned
  // or applied writes — the plan identity. Everything else stays local.
  const identity = internal?.adapter ?? {
    id: ADAPTER_ID,
    version: ADAPTER_VERSION,
    digest: entryDigest(),
  };
  const evidence = { adapter: identity };
  if (hasOwn(request, "plan_id") && request.plan_id !== undefined) {
    evidence.plan_id = request.plan_id;
  }
  return evidence;
}

// ---------------------------------------------------------------------------
// 5-6. Resolved project profile validation and root resolution.
// ---------------------------------------------------------------------------

/** The frozen, normalized resolved project profile, or a refusal. */
export function validateResolvedProjectProfile(candidate) {
  const invalid = (reason) => {
    throw new RequestRefusal("profile-invalid", `invalid resolved project profile: ${reason}`);
  };
  if (typeof candidate !== "object" || candidate === null || Array.isArray(candidate)) {
    invalid("not an object");
  }
  const allowed = [
    "id", "mode", "target", "readRoots", "exclusions", "targetResolution",
    "provenance", "localReference",
  ];
  for (const key of Object.keys(candidate)) {
    if (!allowed.includes(key)) {
      invalid(`unknown member ${key}`);
    }
  }
  if (!isToken(candidate.id)) {
    invalid("id");
  }
  if (candidate.mode !== "observed") {
    invalid("mode");
  }
  if (candidate.target !== "node-typescript") {
    invalid("target");
  }
  if (!Array.isArray(candidate.readRoots) || candidate.readRoots.length === 0) {
    // Zero roots are a policy refusal, never an implicit all-files grant.
    invalid("readRoots");
  }
  const seen = new Set();
  const readRoots = [];
  for (const root of candidate.readRoots) {
    if (typeof root !== "object" || root === null || Array.isArray(root)) {
      invalid("readRoots entries");
    }
    const keys = Object.keys(root).sort();
    // "scope" is derived data the kernel itself attaches after
    // validation; re-validating a validated profile must accept it.
    if (keys.join(",") !== "kind,path" && keys.join(",") !== "kind,path,scope") {
      invalid("readRoots members");
    }
    if (root.kind !== "file" && root.kind !== "tree") {
      invalid("readRoots kind");
    }
    if (typeof root.path !== "string") {
      invalid("readRoots path");
    }
    const spelling = root.path;
    // Every root spelling is validated lexically at profile-validation
    // time too, so an invalid root can never reach the filesystem stage.
    const violation = lexicalRootViolation(spelling);
    if (violation) {
      invalid(`readRoots spelling (${violation})`);
    }
    if (root.kind === "tree" && spelling.endsWith("/")) {
      invalid("readRoots spelling");
    }
    const scope = root.kind === "tree" ? `${spelling}/**` : spelling;
    const key = `${root.kind}:${spelling}`;
    if (seen.has(key)) {
      invalid("duplicate root");
    }
    seen.add(key);
    for (const other of readRoots) {
      // Overlapping or nested root declarations are ambiguous: refuse,
      // never widen or deduplicate into an ancestor. Scope-recursive
      // roots are compared by prefix segments, so `src` and `src-other`
      // stay distinct while `src` and `src/inner` collide.
      const newSegments = scope.split("/");
      const oldSegments = other.scope.split("/");
      const prefixOf = (prefix, candidateSegments) =>
        prefix.length <= candidateSegments.length
        && prefix.every((segment, index) => candidateSegments[index] === segment);
      const prefixNew = newSegments[newSegments.length - 1] === "**"
        ? newSegments.slice(0, -1) : newSegments;
      const prefixOld = oldSegments[oldSegments.length - 1] === "**"
        ? oldSegments.slice(0, -1) : oldSegments;
      if (prefixOf(prefixNew, prefixOld) || prefixOf(prefixOld, prefixNew)) {
        invalid("overlapping roots");
      }
    }
    readRoots.push({ kind: root.kind, path: spelling, scope });
  }
  if (hasOwn(candidate, "exclusions") && candidate.exclusions !== undefined) {
    if (!Array.isArray(candidate.exclusions)) {
      invalid("exclusions");
    }
    for (const exclusion of candidate.exclusions) {
      // F3 (sharpened by review F-4): exclusions have exactly one
      // meaning, shared by the kernel read view and the scanner
      // inventory walk — `dir/**` excludes the whole subtree, and a
      // bare logical path excludes the named entry together with
      // everything below it (both consumers prune the subtree of a
      // bare-excluded directory, and an exact file is its own entry).
      // Both use the same spelling-based rule: subtree pruning works
      // with or without the /** tail — bare names are simply the
      // concise spelling of the same subtree exclusion.
      if (typeof exclusion !== "string"
        || !(isScope(exclusion) && exclusion.endsWith("/**") || isLogicalPath(exclusion))) {
        invalid("exclusions entries");
      }
    }
  }
  let targetResolution;
  if (candidate.targetResolution !== undefined && candidate.targetResolution !== null) {
    const resolution = candidate.targetResolution;
    if (typeof resolution !== "object" || Array.isArray(resolution)) {
      invalid("targetResolution");
    }
    const keys = Object.keys(resolution).sort();
    if (keys.join(",") !== "capabilities,digest") {
      invalid("targetResolution members");
    }
    if (!isSha256Digest(resolution.digest)) {
      invalid("targetResolution digest");
    }
    if (!Array.isArray(resolution.capabilities) || resolution.capabilities.length === 0) {
      invalid("targetResolution capabilities");
    }
    let previous = "";
    for (const capability of resolution.capabilities) {
      if (typeof capability !== "object" || capability === null
        || Object.keys(capability).sort().join(",") !== "id,support"
        || !isCapabilityId(capability.id)
        || !SUPPORT_STATES.includes(capability.support)) {
        invalid("targetResolution capability entry");
      }
      if (Buffer.compare(Buffer.from(capability.id, "utf8"), Buffer.from(previous, "utf8")) <= 0) {
        invalid("targetResolution capability order");
      }
      previous = capability.id;
    }
    targetResolution = {
      digest: resolution.digest,
      capabilities: resolution.capabilities.map((capability) => ({ ...capability })),
    };
  } else if (candidate.targetResolution === undefined) {
    targetResolution = undefined;
  } else {
    targetResolution = undefined;
  }
  let provenance;
  if (candidate.provenance === undefined || candidate.provenance === null) {
    invalid("provenance");
  } else {
    const raw = candidate.provenance;
    if (typeof raw !== "object" || Array.isArray(raw)) {
      invalid("provenance");
    }
    const keys = Object.keys(raw).sort();
    if (keys.join(",") !== "disposition,origin,revision") {
      invalid("provenance members");
    }
    if (typeof raw.origin !== "string"
      || !["declared", "observed", "inferred"].includes(raw.origin)
      || typeof raw.revision !== "string"
      || raw.revision.length === 0
      || raw.revision.length > 256
      || typeof raw.disposition !== "string"
      || !["public-fixture", "user-confirmed", "unconfirmed"].includes(raw.disposition)) {
      invalid("provenance values");
    }
    provenance = { ...raw };
  }
  let localReference;
  if (candidate.localReference !== undefined && candidate.localReference !== null) {
    if (typeof candidate.localReference !== "object" || Array.isArray(candidate.localReference)) {
      invalid("localReference");
    }
    // Kept verbatim on the local-only side; never serialized publicly.
    localReference = deepFreeze(structuredClone(candidate.localReference));
  }
  return deepFreeze({
    id: candidate.id,
    mode: candidate.mode,
    target: candidate.target,
    readRoots: readRoots.map((root) => ({ ...root })),
    exclusions: Object.freeze([...(candidate.exclusions ?? [])]),
    ...(targetResolution !== undefined ? { targetResolution } : {}),
    provenance,
    ...(localReference !== undefined ? { localReference } : {}),
  });
}

function deepFreeze(value) {
  if (value !== null && typeof value === "object") {
    for (const key of Object.keys(value)) {
      deepFreeze(value[key]);
    }
    Object.freeze(value);
  }
  return value;
}

/**
 * Validate every root spelling lexically, then resolve every root
 * physically inside `permittedRoot` (an absolute host path of the
 * trusted private project view). One invalid root — lexical or physical,
 * first or last — fails the whole resolution before any extension runs.
 */
export function resolveReadRoots(permittedRoot, profile) {
  const failures = [];
  const resolved = [];
  for (const root of profile.readRoots) {
    const lexical = lexicalRootViolation(root.path);
    if (lexical) {
      failures.push({ path: root.path, reason: lexical });
      continue;
    }
    const physical = physicalRootViolation(permittedRoot, root);
    if (physical) {
      failures.push({ path: root.path, reason: physical });
      continue;
    }
    resolved.push({
      kind: root.kind,
      path: root.path,
      scope: root.scope,
      absolute: resolve(permittedRoot, ...root.path.split("/")),
    });
  }
  if (failures.length > 0) {
    const refusal = new RequestRefusal("root-invalid", "read root validation failed");
    refusal.failures = failures;
    throw refusal;
  }
  return deepFreeze(resolved);
}

/** The closed lexical root refusal, or `null` when the spelling is legal. */
export function lexicalRootViolation(path) {
  if (typeof path !== "string" || path.length === 0) {
    return "empty";
  }
  if (path.length > 512) {
    return "overlong";
  }
  if (path === ".") {
    // `.` is the protocol project root, never an all-files read root.
    return "project-root";
  }
  if (path.startsWith("/") || path.startsWith("\\") || path.startsWith("//")
    || path.startsWith("\\\\")) {
    return "absolute";
  }
  if (/^[A-Za-z]:/.test(path)) {
    return "drive";
  }
  if (/^[A-Za-z][A-Za-z0-9+.-]*:/.test(path)) {
    return "uri";
  }
  if (path.includes("\\")) {
    return "backslash";
  }
  if (/%[0-9a-fA-F]{2}/.test(path)) {
    return "percent-escape";
  }
  if (/[\u0000-\u001f\u007f]/.test(path) || /[:<>"|?*]/.test(path)) {
    return "control-character";
  }
  for (const segment of path.split("/")) {
    if (segment === "." || segment === "..") {
      return "traversal";
    }
    if (isDosDevice(segment)) {
      return "dos-device";
    }
    if (/~\d/.test(segment)) {
      return "short-name";
    }
    if (/[A-Z]/.test(segment)) {
      return "uppercase";
    }
    if (/\s/.test(segment)) {
      return "whitespace";
    }
  }
  if (!isLogicalPath(path)) {
    return "grammar";
  }
  return null;
}

/**
 * One physical root refusal, or `null`. Rejects missing entries,
 * links/junctions/special files, and any resolution that escapes the
 * permitted root or changes spelling. `lstat` inspects the entry itself
 * (a link is refused as a link, never followed implicitly); `realpath`
 * proves the canonical physical containment; `stat` refuses special
 * files.
 */
export function physicalRootViolation(permittedRoot, root) {
  let target;
  try {
    target = resolve(permittedRoot, ...root.path.split("/"));
  } catch {
    return "resolve";
  }
  if (!isInsideRoot(permittedRoot, target)) {
    return "containment";
  }
  // F5: every path component from the permitted root down to the entry
  // is inspected, so an inside-pointing intermediate link is refused
  // exactly like an outside-pointing one.
  const segments = root.path.split("/");
  for (let depth = 1; depth <= segments.length; depth += 1) {
    const component = resolve(permittedRoot, ...segments.slice(0, depth));
    let componentMetadata;
    try {
      componentMetadata = lstatSync(component);
    } catch (error) {
      return error?.code === "ENOENT" ? "missing" : "uninspectable";
    }
    if (componentMetadata.isSymbolicLink()) {
      return "symlink";
    }
    if (componentMetadata.nlink > 1 && process.platform === "win32" && componentMetadata.isSymbolicLink()) {
      return "junction";
    }
  }
  // realpathSync can fail on filesystems that deny READ_CONTROL to the
  // confined child. The F5 component walk above already lstat-ed every
  // component (links refused) and resolve() proved lexical containment,
  // so containment is established without the canonical spelling; the
  // canonical comparison stays best-effort here.
  let physical = null;
  try {
    physical = realpathSync(target);
  } catch {
    physical = null;
  }
  if (physical !== null && !isInsideRoot(permittedRoot, physical)) {
    return "containment";
  }
  let follow;
  try {
    follow = statSync(target);
  } catch {
    return "uninspectable";
  }
  if (root.kind === "tree" ? !follow.isDirectory() : !follow.isFile()) {
    return "kind";
  }
  return null;
}

/**
 * Windows-aware containment: compare case-insensitively on win32 after
 * drive-prefix check; lexical drive equality alone is not enough, so the
 * canonical physical spelling must sit strictly inside the permitted
 * root (never equal to it).
 */
export function isInsideRoot(root, candidate) {
  const normalize = (value) => {
    if (process.platform === "win32") {
      const lower = value.toLowerCase();
      const withBackslashes = lower.replaceAll("/", "\\");
      const prefix = win32.parse(withBackslashes);
      return prefix.root ? withBackslashes.replace(/\\+$/, "") : withBackslashes;
    }
    const trimmed = value.replace(/\/+$/, "");
    return trimmed === "" ? "/" : trimmed;
  };
  const rootNormalized = normalize(root);
  const candidateNormalized = normalize(candidate);
  const separator = process.platform === "win32" ? "\\" : "/";
  const bounded = candidateNormalized === rootNormalized
    ? false
    : candidateNormalized.startsWith(rootNormalized + separator);
  return Boolean(bounded && rootNormalized.length > 0);
}

// ---------------------------------------------------------------------------
// 7-8. Extension registry, dispatch, evidence normalization.
// ---------------------------------------------------------------------------

/**
 * Validate one extension descriptor at registration time. A registered
 * function alone never proves semantic coverage: the capability map is
 * computed from installed, enabled implementations, and the kernel
 * advertises only what a validated descriptor supplies.
 */
export function validateExtensionDescriptor(descriptor) {
  const invalid = (reason) => {
    throw new RequestRefusal("extension-invalid", `invalid extension descriptor: ${reason}`);
  };
  if (typeof descriptor !== "object" || descriptor === null) {
    invalid("not an object");
  }
  const allowed = ["id", "version", "operations", "namedCapabilities", "acceptedIrVersions", "invoke"];
  for (const key of Object.keys(descriptor)) {
    if (!allowed.includes(key)) {
      invalid(`unknown member ${key}`);
    }
  }
  if (!isToken(descriptor.id)) {
    invalid("id");
  }
  if (!isContractVersion(descriptor.version)) {
    invalid("version");
  }
  if (!Array.isArray(descriptor.operations) || descriptor.operations.length === 0
    || !descriptor.operations.every((operation) => OPERATION_TOKENS.includes(operation))) {
    invalid("operations");
  }
  if (descriptor.operations.includes("describe")) {
    invalid("describe is kernel-owned");
  }
  if (hasOwn(descriptor, "namedCapabilities") && descriptor.namedCapabilities !== undefined) {
    if (typeof descriptor.namedCapabilities !== "object" || Array.isArray(descriptor.namedCapabilities)) {
      invalid("namedCapabilities");
    }
    for (const [id, state] of Object.entries(descriptor.namedCapabilities)) {
      if (!CAPABILITY_IDS.includes(id) || !SUPPORT_STATES.includes(state)) {
        invalid(`named capability ${id}`);
      }
    }
  }
  if (hasOwn(descriptor, "acceptedIrVersions") && descriptor.acceptedIrVersions !== undefined) {
    if (!Array.isArray(descriptor.acceptedIrVersions)
      || !descriptor.acceptedIrVersions.every(isContractVersion)) {
      invalid("acceptedIrVersions");
    }
  }
  if (typeof descriptor.invoke !== "function") {
    invalid("invoke");
  }
  return deepFreeze({
    id: descriptor.id,
    version: descriptor.version,
    operations: Object.freeze([...descriptor.operations]),
    namedCapabilities: descriptor.namedCapabilities
      ? deepFreeze({ ...descriptor.namedCapabilities })
      : undefined,
    acceptedIrVersions: descriptor.acceptedIrVersions
      ? Object.freeze([...descriptor.acceptedIrVersions])
      : undefined,
    invoke: descriptor.invoke,
  });
}

/** One internal dispatch outcome before any wire projection. */
export function normalizeExtensionOutcome(outcome) {
  const invalid = (reason) => {
    throw new RequestRefusal("outcome-invalid", `invalid extension outcome: ${reason}`);
  };
  if (typeof outcome !== "object" || outcome === null) {
    invalid("not an object");
  }
  const allowed = ["state", "data", "evidence", "diagnostics"];
  for (const key of Object.keys(outcome)) {
    if (!allowed.includes(key)) {
      invalid(`unknown member ${key}`);
    }
  }
  if (!["complete", "partial", "unknown", "unsupported", "failed"].includes(outcome.state)) {
    invalid("state");
  }
  return deepFreeze({
    state: outcome.state,
    data: outcome.data === undefined ? undefined : structuredClone(outcome.data),
    evidence: outcome.evidence === undefined ? undefined : structuredClone(outcome.evidence),
    diagnostics: outcome.diagnostics === undefined ? undefined : structuredClone(outcome.diagnostics),
  });
}

/**
 * The stable internal dispatch boundary. `createKernel` wires identity,
 * an optional resolved project profile, an optional extension registry,
 * and an optional local-only evidence sink into one kernel object.
 *
 * Production describes itself with no roots and no extensions; standalone
 * synthetic tests inject the committed fixture profile and callback spies
 * through exactly this seam (#44 scanners and #48 runners attach here
 * without changing the external process boundary).
 */
export function createKernel(options = {}) {
  const identity = options.identity ?? {
    id: ADAPTER_ID,
    version: ADAPTER_VERSION,
    digest: entryDigest(),
  };
  const profile = options.resolvedProjectProfile
    ? validateResolvedProjectProfile(options.resolvedProjectProfile)
    : null;
  const extensions = new Map();
  for (const descriptor of options.extensionRegistry ?? []) {
    const validated = validateExtensionDescriptor(descriptor);
    // Duplicate descriptor ids and duplicate operation claims across
    // descriptors are registration refusals, never silent
    // first-registered-wins (review nit #10).
    if (extensions.has(validated.id)) {
      throw new RequestRefusal("extension-invalid", `duplicate extension descriptor id`);
    }
    for (const operation of validated.operations) {
      if ([...extensions.values()].some((candidate) => candidate.operations.includes(operation))) {
        throw new RequestRefusal("extension-invalid", `duplicate operation claim ${operation}`);
      }
    }
    extensions.set(validated.id, validated);
  }
  const sink = options.localEvidenceSink ?? null;

  return {
    identity: deepFreeze({ ...identity }),
    profile,
    extensions: Object.freeze([...extensions.keys()]),

    /** The closed process descriptor (mandatory describe). */
    describe() {
      const capabilities = describeCapabilities(
        extensions.size > 0 ? profile : null,
        [...extensions.values()],
      );
      return capabilities;
    },

    /**
     * Dispatch one already-validated request through the internal
     * boundary. Roots are validated (lexically, then physically) before
     * any extension callback runs; one invalid late root prevents every
     * callback. `trustedExecutionContext` must carry the absolute
     * permitted project root; it is never read from the request.
     */
    dispatch(validatedRequest, trustedExecutionContext) {
      const operation = validatedRequest.operation;
      if (operation === "describe") {
        return {
          response: buildResponse(validatedRequest, { capabilities: this.describe() }),
          internal: {
            state: "complete",
            evidence: {
              adapter: { ...identity },
              node: String(process.versions.node),
              profile: profile ? {
                id: profile.id,
                readRoots: profile.readRoots.map((root) => ({ ...root })),
                provenance: { ...profile.provenance },
                ...(profile.localReference !== undefined
                  ? { localReference: structuredClone(profile.localReference) } : {}),
              } : null,
              extensions: [...extensions.keys()],
            },
          },
        };
      }
      const extension = [...extensions.values()].find((candidate) =>
        candidate.operations.includes(operation));
      if (!extension) {
        return {
          response: buildUnsupportedResponse(validatedRequest),
          internal: {
            state: "unsupported",
            evidence: {
              adapter: { ...identity },
              operation,
              reason: "no-installed-extension",
            },
          },
        };
      }
      // F6: describe/dispatch share one gating predicate. A registered
      // extension without a validated profile is a structured refusal,
      // never a dereference of a missing profile.
      if (!profile) {
        return {
          response: buildResponse(validatedRequest, {
            error: {
              class: "unsupported",
              code: ERROR_CODES.unsupported.code,
              message: ERROR_CODES.unsupported.message,
              retryable: false,
              partial: false,
              detail: ["profile-absent"],
            },
          }),
          internal: {
            state: "unsupported",
            evidence: {
              adapter: { ...identity },
              operation,
              reason: "profile-absent",
            },
          },
        };
      }
      // F1: the request's claimed profile binding must match the trusted
      // snapshot exactly before any extension callback can run. No read
      // counter may move on a mismatch.
      const bindingRefusal = validateProfileBinding(validatedRequest, profile);
      if (bindingRefusal) {
        return {
          response: buildResponse(validatedRequest, {
            error: {
              class: "invalid",
              code: "profile-binding-mismatch",
              message: "the request profile binding does not match the trusted profile",
              retryable: false,
              partial: false,
              detail: [bindingRefusal.detail],
            },
          }),
          internal: {
            state: "failed",
            evidence: {
              adapter: { ...identity },
              operation,
              reason: "profile-binding-mismatch",
              detail: bindingRefusal.detail,
            },
          },
        };
      }
      // Validate ALL roots before the single extension invocation.
      const permittedRoot = trustedExecutionContext?.permittedProjectRoot;
      if (!permittedRoot) {
        throw new RequestRefusal("context", "dispatch requires the trusted execution context");
      }
      let roots;
      try {
        roots = resolveReadRoots(permittedRoot, profile);
      } catch (error) {
        if (error instanceof RequestRefusal && error.code === "root-invalid") {
          return {
            response: buildResponse(validatedRequest, {
              error: {
                class: "invalid",
                code: ERROR_CODES.rootInvalid.code,
                message: ERROR_CODES.rootInvalid.message,
                retryable: false,
                partial: false,
                detail: error.failures.map((failure) => boundToken(failure.reason)).slice(0, 16),
              },
            }),
            internal: {
              state: "failed",
              evidence: {
                adapter: { ...identity },
                operation,
                rootFailures: error.failures.map((failure) => ({ ...failure })),
              },
            },
          };
        }
        throw error;
      }
      const readView = createReadView(permittedRoot, roots, profile);
      readView.permittedProjectRoot = permittedRoot;
      let outcome;
      try {
        outcome = normalizeExtensionOutcome(extension.invoke({
          operation,
          request: validatedRequest,
          profile,
          readView,
          cancellation: trustedExecutionContext.cancellation ?? null,
          limits: trustedExecutionContext.limits ?? { files: 4096, bytes: 4 * 1024 * 1024 },
        }));
      } catch (error) {
        // Malformed extension output cannot bypass response validation:
        // one boundary catches rejection, throw, and malformed results.
        return {
          response: buildResponse(validatedRequest, {
            error: {
              class: "infrastructure",
              code: ERROR_CODES.extensionFailed.code,
              message: ERROR_CODES.extensionFailed.message,
              retryable: false,
              partial: false,
            },
          }),
          internal: {
            state: "failed",
            evidence: {
              adapter: { ...identity },
              operation,
              extension: extension.id,
              failure: "extension-threw",
              reason: boundToken(String(error?.message ?? "unknown").slice(0, 64)),
            },
          },
        };
      }
      recordSink(sink, validatedRequest, outcome, profile);
      return {
        response: projectOutcome(validatedRequest, outcome),
        internal: outcome,
      };
    },
  };
}

function boundToken(text) {
  const bounded = text.replace(/[^a-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 128);
  return bounded === "" ? "unspecified" : bounded;
}

function recordSink(sink, request, outcome, profile) {
  if (!sink) {
    return;
  }
  sink.push({
    request_id: request.request_id,
    operation: request.operation,
    state: outcome.state,
    profile: { id: profile.id, provenance: { ...profile.provenance } },
    ...(profile.localReference !== undefined
      ? { localReference: structuredClone(profile.localReference) } : {}),
    evidence: structuredClone(outcome.evidence ?? null),
    diagnostics: structuredClone(outcome.diagnostics ?? null),
  });
}

function buildUnsupportedResponse(request) {
  return buildResponse(request, {
    error: {
      class: "unsupported",
      code: ERROR_CODES.unsupported.code,
      message: ERROR_CODES.unsupported.message,
      retryable: false,
      partial: false,
    },
  });
}

/**
 * Project an internal outcome onto the closed wire. Ambiguity, partial
 * work, and unknown results never become optimistic success; values the
 * closed result shape cannot represent are refused with an honest
 * in-envelope error instead of being truncated or smuggled into extra
 * members. No `status: "partial"` invention: partial failure sets
 * `error.partial: true`.
 */
function projectOutcome(request, outcome) {
  if (outcome.state === "complete") {
    const result = projectResult(outcome.data);
    if (result) {
      return buildResponse(request, { result });
    }
    return buildResponse(request, {
      error: {
        class: "conflict",
        code: "outcome-unrepresentable",
        message: "the extension outcome cannot be represented on the closed wire",
        retryable: false,
        partial: true,
      },
    });
  }
  if (outcome.state === "partial" || outcome.state === "unknown" || outcome.state === "failed") {
    const reason = outcome.diagnostics?.[0]?.reason ?? "extension-outcome";
    return buildResponse(request, {
      error: {
        class: outcome.state === "failed" ? "conflict" : "invalid",
        code: boundToken(`outcome-${outcome.state}-${reason}`),
        message: `the operation ended ${outcome.state}; no success is claimed`,
        retryable: false,
        partial: outcome.state === "partial",
      },
    });
  }
  return buildUnsupportedResponse(request);
}

/**
 * Project extension data onto the closed operation result members. Only
 * scan entries (`path`, `kind`, optional bounded `detail`, optional
 * typed `evidence`) survive; a `truncated` flag is never projected
 * because the current merging caller ignores it — partial data must not
 * become a silently complete receipt. The evidence member carries the
 * observed reference rows and the structural signature digest as typed
 * data; anything else is refused, never flattened into pseudo-JSON.
 */
function projectResult(data) {
  if (data === undefined || data === null || typeof data !== "object") {
    return undefined;
  }
  // Issue #48: plan-native results carry the closed native_plan summary.
  if (data.native_plan !== undefined) {
    const np = data.native_plan;
    if (typeof np !== "object" || np === null || np.kind !== "native-plan" || !isSha256Digest(np.plan_digest)) {
      return undefined;
    }
    return { native_plan: { digest: np.plan_digest, kind: np.kind } };
  }
  if (!Array.isArray(data.entries)) {
    return undefined;
  }
  if (data.entries.length > 10000) {
    // F2: the wire bounds entries at 10,000; an over-limit complete
    // inventory is unrepresentable and is refused, never sliced into a
    // silently complete receipt.
    return undefined;
  }
  const entries = [];
  for (const entry of data.entries) {
    if (typeof entry !== "object" || entry === null
      || !isLogicalPath(entry.path)
      || typeof entry.kind !== "string"
      || [...entry.kind].length > 128
      || (entry.detail !== undefined && (typeof entry.detail !== "string" || [...entry.detail].length > 128))) {
      return undefined;
    }
    const projected = entry.detail === undefined
      ? { path: entry.path, kind: entry.kind }
      : { path: entry.path, kind: entry.kind, detail: entry.detail };
    if (entry.evidence !== undefined) {
      const evidence = projectScanEntryEvidence(entry.evidence);
      if (!evidence) {
        return undefined;
      }
      projected.evidence = evidence;
    }
    entries.push(projected);
  }
  const result = { entries };
  if (data.complete === false) {
    // The caller that merges scan results currently ignores `truncated`;
    // keep partial outcomes off the wire result and surface the honest
    // error above instead of enabling a silently complete scan receipt.
    return undefined;
  }
  return result;
}

/**
 * Project one entry's closed typed evidence member: the structural
 * signature digest and up to eight reference rows, canonically ordered.
 * Every other shape is refused.
 */
function projectScanEntryEvidence(evidence) {
  if (typeof evidence !== "object" || evidence === null || Array.isArray(evidence)) {
    return undefined;
  }
  for (const key of Object.keys(evidence)) {
    if (!SCAN_ENTRY_EVIDENCE_KEYS.includes(key)) {
      return undefined;
    }
  }
  const projected = {};
  if (evidence.signature !== undefined) {
    if (evidence.signature !== null && !isSha256Digest(evidence.signature)) {
      return undefined;
    }
    projected.signature = evidence.signature;
  }
  if (evidence.references !== undefined) {
    if (!Array.isArray(evidence.references)
      || evidence.references.length > MAX_SCAN_ENTRY_REFERENCES) {
      return undefined;
    }
    const references = [];
    for (const reference of evidence.references) {
      if (typeof reference !== "object" || reference === null || Array.isArray(reference)) {
        return undefined;
      }
      const keys = Object.keys(reference).sort();
      if (keys.join(",") !== SCAN_ENTRY_REFERENCE_KEYS.join(",")) {
        return undefined;
      }
      if (typeof reference.target !== "string"
        || reference.target.length === 0
        || reference.target.length > 192
        || !isSemanticId(reference.target)
        || !SCAN_ENTRY_ROLES.includes(reference.role)
        || !SCAN_ENTRY_CONFIDENCES.includes(reference.confidence)) {
        return undefined;
      }
      references.push({
        target: reference.target,
        role: reference.role,
        confidence: reference.confidence,
      });
    }
    projected.references = references;
  }
  if (canonicalJsonText(projected).length > MAX_SCAN_ENTRY_EVIDENCE_BYTES) {
    return undefined;
  }
  return projected;
}

/** Whether one string is a bounded semantic id (the observed grammar). */
export function isSemanticId(value) {
  return typeof value === "string"
    && value.length >= 1
    && value.length <= 192
    && /^[A-Za-z0-9_][A-Za-z0-9_.:-]*$/.test(value);
}

/**
 * The restricted read facade handed to extensions: only resolved roots,
 * revalidated physically at every actual read, with file-count and
 * byte-count caps and explicit-exclusion enforcement. No raw absolute
 * root authority is ever exposed.
 */
export function createReadView(permittedRoot, roots, profile) {
  let filesRead = 0;
  let bytesRead = 0;
  const exclusions = profile.exclusions ?? [];
  const excluded = (path) => exclusions.some((exclusion) => scopeCovers(exclusion, path)
    || exclusion === path);
  /**
   * F3: a denied subtree prunes descent. Directory-shaped checks test
   * every ancestor prefix against the exclusions, so a file under an
   * excluded directory is refused without ever touching it.
   */
  const excludedPath = (logicalPath) => {
    if (excluded(logicalPath)) {
      return true;
    }
    const segments = logicalPath.split("/");
    for (let depth = 1; depth < segments.length; depth += 1) {
      const ancestor = segments.slice(0, depth).join("/");
      // F-4 shared semantics: a /** exclusion covers everything under
      // its head, AND a bare-excluded ancestor (the exact-entry
      // spelling) prunes the whole subtree below it in this read view
      // exactly as the scanner inventory walk does structurally.
      if (exclusions.some((exclusion) => scopeCovers(exclusion, `${ancestor}/probe`)
        || exclusion === ancestor)) {
        return true;
      }
    }
    return false;
  };
  const revalidate = (logicalPath, expectedKind) => {
    const root = roots.find((candidate) => scopeCovers(candidate.scope, logicalPath)
      || (candidate.kind === "file" && candidate.path === logicalPath));
    if (!root) {
      throw new RequestRefusal("read-denied", "path outside the resolved roots");
    }
    if (excludedPath(logicalPath)) {
      throw new RequestRefusal("read-denied", "path is excluded");
    }
    const absolute = resolve(permittedRoot, ...logicalPath.split("/"));
    if (!isInsideRoot(permittedRoot, absolute)) {
      throw new RequestRefusal("read-denied", "path escapes the permitted root");
    }
    // F5: every component of the read path is walked with lstat, so an
    // intermediate link inside a root is refused on reads too.
    const segments = logicalPath.split("/");
    for (let depth = 1; depth <= segments.length; depth += 1) {
      const component = resolve(permittedRoot, ...segments.slice(0, depth));
      let componentMetadata;
      try {
        componentMetadata = lstatSync(component);
      } catch (error) {
        throw new RequestRefusal("read-denied", error?.code === "ENOENT" ? "missing" : "uninspectable");
      }
      if (componentMetadata.isSymbolicLink()) {
        throw new RequestRefusal("read-denied", "link");
      }
    }
    const follow = statSync(absolute);
    if (expectedKind === "file" && !follow.isFile()) {
      throw new RequestRefusal("read-denied", "kind");
    }
    return absolute;
  };
  return {
    roots: roots.map((root) => ({ kind: root.kind, path: root.path, scope: root.scope })),
    canRead(logicalPath) {
      try {
        revalidate(logicalPath, "file");
        return true;
      } catch {
        return false;
      }
    },
    readFile(logicalPath, limits = { files: 4096, bytes: 4 * 1024 * 1024 }) {
      if (filesRead >= limits.files) {
        throw new RequestRefusal("read-denied", "file cap exhausted");
      }
      const absolute = revalidate(logicalPath, "file");
      // Bytes are bounded before allocation: the file size is checked
      // first, then read with a hard cap; a file that grows between the
      // check and the read still cannot exceed the per-read bound.
      let size;
      try {
        size = statSync(absolute).size;
      } catch {
        throw new RequestRefusal("read-denied", "uninspectable");
      }
      if (size > MAX_FILE_BYTES || bytesRead + size > limits.bytes) {
        throw new RequestRefusal("read-denied", "byte cap exhausted");
      }
      const bytes = readFileSync(absolute);
      if (bytes.length > MAX_FILE_BYTES || bytesRead + bytes.length > limits.bytes) {
        throw new RequestRefusal("read-denied", "byte cap exhausted");
      }
      filesRead += 1;
      bytesRead += bytes.length;
      return bytes;
    },
    counters: () => ({ filesRead, bytesRead }),
  };
}

// ---------------------------------------------------------------------------
// 9. One-shot main.
// ---------------------------------------------------------------------------

/** Read the request bytes from stdin or `--lekalo-request-file PATH`. */
export function readRequestBytes(argv = process.argv) {
  const markers = argv.filter((argument) => argument === "--lekalo-request-file");
  if (markers.length > 1) {
    throw new RequestRefusal("transport", "ambiguous transport flags");
  }
  const marker = argv.indexOf("--lekalo-request-file");
  if (marker !== -1) {
    const value = argv[marker + 1];
    if (value === undefined || value === "" || argv.slice(marker + 2).includes("--lekalo-request-file")) {
      throw new RequestRefusal("transport", "missing request-file value");
    }
    // F8: the file is read through the same bounded stream as stdin —
    // never a full readFileSync of an arbitrary-size file.
    let fd;
    try {
      fd = openSync(value, "r");
    } catch {
      throw new RequestRefusal("transport", "the request file could not be opened");
    }
    try {
      const metadata = fstatSync(fd);
      if (!metadata.isFile()) {
        throw new RequestRefusal("transport", "the request file is not a regular file");
      }
      return readBoundedStream(fd);
    } finally {
      closeSyncSafe(fd);
    }
  }
  return readBoundedStream(0);
}

function closeSyncSafe(fd) {
  try {
    closeSync(fd);
  } catch {
    // The bound read already refused or consumed the stream; a close
    // failure on an owned descriptor cannot fabricate a response.
  }
}

/**
 * One bounded read of at most `bound + 1` bytes: exactly the bound is a
 * legal document, one more byte is a transport refusal. Never buffers a
 * hostile unbounded stream (review finding F8).
 */
export function readBoundedStream(fd, bound = MAX_REQUEST_BYTES) {
  const chunks = [];
  let total = 0;
  const chunkSize = 64 * 1024;
  for (;;) {
    const chunk = Buffer.allocUnsafe(Math.min(chunkSize, bound + 1 - total));
    let read;
    try {
      read = readSyncFailable(fd, chunk);
    } catch (error) {
      if (error?.code === "EAGAIN") {
        continue; // non-blocking fd, retry synchronously
      }
      if (error?.code === "EINTR") {
        continue;
      }
      throw new RequestRefusal("transport", "the request stream could not be read");
    }
    if (read === 0) {
      break; // EOF
    }
    total += read;
    chunks.push(chunk.subarray(0, read));
    if (total > bound) {
      throw new RequestRefusal("request-too-large", "request bytes exceed the transport bound");
    }
    if (total === bound + 1) {
      throw new RequestRefusal("request-too-large", "request bytes exceed the transport bound");
    }
  }
  return Buffer.concat(chunks);
}

function readSyncFailable(fd, buffer) {
  // fs.readSync with position 0 tracks the file offset on repeated calls.
  return readSync(fd, buffer, 0, buffer.length, null);
}

/** The fixed bounded stderr diagnostics for a transport-level refusal. */
export function stderrDiagnostic(code) {
  const bounded = boundToken(code);
  return JSON.stringify({ kernel: ADAPTER_ID, diagnostic: bounded });
}

/**
 * Extract the exactly-positioned launch input value: the profile JSON
 * must directly follow its marker and both must sit after the entry
 * argument (review nit #9's stricter sibling for the new option).
 */
export function extractProjectProfileJson(argv = process.argv.slice(2)) {
  const markers = argv.filter((argument) => argument === "--lekalo-project-profile-json");
  if (markers.length > 1) {
    throw new RequestRefusal("profile-input", "ambiguous launch profile inputs");
  }
  const marker = argv.indexOf("--lekalo-project-profile-json");
  if (marker === -1) {
    return undefined;
  }
  const value = argv[marker + 1];
  if (value === undefined || value === "") {
    throw new RequestRefusal("profile-input", "missing launch profile value");
  }
  return value;
}

/**
 * The one-shot protocol process. Exported for the source entry shim and
 * the bundle; never runs automatically on import.
 */
export async function main() {
  const argvWithoutEntry = process.argv.slice(2);
  // F9: the version probe is honored only in its exact argv position —
  // the sole argument — never as a stray flag on a transport launch.
  if (argvWithoutEntry.length === 1 && argvWithoutEntry[0] === "--version-json") {
    process.stdout.write(canonicalJson(runtimeMetadata()) + "\n");
    return;
  }
  let requestBytes;
  try {
    requestBytes = readRequestBytes();
  } catch (error) {
    process.stderr.write(stderrDiagnostic(error?.code ?? "transport") + "\n");
    process.exitCode = 1;
    return;
  }
  try {
    const options = {};
    const profileJson = extractProjectProfileJson();
    if (profileJson !== undefined) {
      const trusted = decodeProjectProfileJson(profileJson);
      const kernel = createKernel({
        resolvedProjectProfile: trusted.profile,
        extensionRegistry: launchExtensions,
      });
      options.kernel = kernel;
    }
    const document = decodeJsonDocument(requestBytes);
    const request = validateRequestObject(document);
    const kernel = options.kernel ?? createKernel();
    const dispatched = kernel.dispatch(request, { permittedProjectRoot: process.cwd() });
    process.stdout.write(canonicalJson(dispatched.response));
  } catch (error) {
    // No valid echo identity may be fabricated for a malformed request:
    // bounded stderr plus nonzero exit, never a synthetic envelope.
    process.stderr.write(stderrDiagnostic(error?.code ?? "invalid") + "\n");
    process.exitCode = 1;
  }
}

/**
 * Run the one-shot main when the entry module whose URL is given is the
 * process entry. Each entry (the source `main.mjs` and the bundle
 * generated by build.mjs) passes its own `import.meta.url`; the probe
 * compares module URLs, never argv alone (test runners and importers
 * keep argv[1] pointed at their own entry, and stdin may be an open
 * pipe that never delivers EOF, so a wrong main detection would block
 * forever reading a request that does not exist).
 */
export function runIfEntry(entryUrl) {
  if (typeof process === "undefined" || !process.argv?.[1] || typeof entryUrl !== "string") {
    return;
  }
  try {
    if (resolve(process.argv[1]) === resolve(fileURLToPath(entryUrl))) {
      return main();
    }
  } catch {
    return undefined;
  }
  return undefined;
}

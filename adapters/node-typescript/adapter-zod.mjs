#!/usr/bin/env node
/**
 * The committed single-file generation artifact of
 * `lekalo-target-node-typescript` (issue #45): the protocol kernel plus
 * the Zod schema generator, without the vendored compiler (generation
 * never typechecks; consumers own that). Self-contained — it resolves no
 * package at runtime. GENERATED FILE — regenerate with
 * `node adapters/node-typescript/build.mjs`; verify with
 * `node adapters/node-typescript/build.mjs --check`. Never edit.
 */

import { createRequire as __lekaloCreateRequire } from "node:module";var require = __lekaloCreateRequire(import.meta.url);var __filename = import.meta.url;var __dirname = __filename.slice(0, __filename.lastIndexOf("/"));
var __defProp = Object.defineProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};

// src/kernel.mjs
var kernel_exports = {};
__export(kernel_exports, {
  ADAPTER_ID: () => ADAPTER_ID,
  ADAPTER_VERSION: () => ADAPTER_VERSION,
  ERROR_CODES: () => ERROR_CODES,
  MAX_FILE_BYTES: () => MAX_FILE_BYTES,
  MAX_JSON_DEPTH: () => MAX_JSON_DEPTH,
  MAX_PROFILE_JSON_BYTES: () => MAX_PROFILE_JSON_BYTES,
  MAX_REQUEST_BYTES: () => MAX_REQUEST_BYTES,
  MAX_RESPONSE_BYTES: () => MAX_RESPONSE_BYTES,
  MAX_SCAN_ENTRY_EVIDENCE_BYTES: () => MAX_SCAN_ENTRY_EVIDENCE_BYTES,
  MAX_SCAN_ENTRY_REFERENCES: () => MAX_SCAN_ENTRY_REFERENCES,
  MAX_WRITE_FILES: () => MAX_WRITE_FILES,
  MAX_WRITE_FILE_BYTES: () => MAX_WRITE_FILE_BYTES,
  PROTOCOL_TOKEN: () => PROTOCOL_TOKEN,
  RequestRefusal: () => RequestRefusal,
  SUPPORTED_VERSIONS: () => SUPPORTED_VERSIONS,
  VERSION: () => VERSION,
  __attachVendoredCompiler: () => __attachVendoredCompiler,
  __setCompilerMetadata: () => __setCompilerMetadata,
  __setLaunchExtensions: () => __setLaunchExtensions,
  buildResponse: () => buildResponse,
  canonicalJson: () => canonicalJson,
  compilerMetadata: () => compilerMetadata,
  createKernel: () => createKernel,
  createReadView: () => createReadView,
  createWriteView: () => createWriteView,
  decodeJsonDocument: () => decodeJsonDocument,
  decodeProjectProfileJson: () => decodeProjectProfileJson,
  decodeUtf8Fatal: () => decodeUtf8Fatal,
  describeCapabilities: () => describeCapabilities,
  embeddedLibFiles: () => embeddedLibFiles,
  entryDigest: () => entryDigest,
  entryPath: () => entryPath,
  extractProjectProfileJson: () => extractProjectProfileJson,
  isCapabilityId: () => isCapabilityId,
  isContractVersion: () => isContractVersion,
  isDosDevice: () => isDosDevice,
  isInsideRoot: () => isInsideRoot,
  isLogicalPath: () => isLogicalPath,
  isPlanId: () => isPlanId,
  isRequestId: () => isRequestId,
  isScope: () => isScope,
  isScopeSegment: () => isScopeSegment,
  isSemanticId: () => isSemanticId,
  isSha256Digest: () => isSha256Digest,
  isToken: () => isToken,
  lexicalRootViolation: () => lexicalRootViolation,
  main: () => main,
  normalizeExtensionOutcome: () => normalizeExtensionOutcome,
  physicalRootViolation: () => physicalRootViolation,
  readBoundedStream: () => readBoundedStream,
  readRequestBytes: () => readRequestBytes,
  resolveReadRoots: () => resolveReadRoots,
  runIfEntry: () => runIfEntry,
  runtimeMetadata: () => runtimeMetadata,
  scopeCovers: () => scopeCovers,
  sha256Hex: () => sha256Hex,
  stderrDiagnostic: () => stderrDiagnostic,
  validateExtensionDescriptor: () => validateExtensionDescriptor,
  validateNativeRequest: () => validateNativeRequest,
  validateProfileBinding: () => validateProfileBinding,
  validateRequestObject: () => validateRequestObject,
  validateResolvedProjectProfile: () => validateResolvedProjectProfile,
  vendoredTs: () => vendoredTs
});
import { createHash } from "node:crypto";
import {
  closeSync,
  lstatSync,
  mkdirSync,
  openSync,
  fstatSync,
  readSync,
  readFileSync,
  realpathSync,
  renameSync,
  statSync,
  writeFileSync
} from "node:fs";
import { dirname, isAbsolute, resolve, win32 } from "node:path";
import { fileURLToPath } from "node:url";
var PROTOCOL_TOKEN = "lekalo.target/v1";
var VERSION = "0.3.2";
var SUPPORTED_VERSIONS = Object.freeze([VERSION]);
var ADAPTER_ID = "lekalo-target-node-typescript";
var ADAPTER_VERSION = "0.3.2";
var MAX_REQUEST_BYTES = 1024 * 1024;
var MAX_RESPONSE_BYTES = 8 * 1024 * 1024;
var MAX_JSON_DEPTH = 64;
var MAX_PROFILE_JSON_BYTES = 16 * 1024;
var SCAN_ENTRY_EVIDENCE_KEYS = Object.freeze(["references", "signature"]);
var SCAN_ENTRY_REFERENCE_KEYS = Object.freeze(["confidence", "role", "target"]);
var SCAN_ENTRY_ROLES = Object.freeze([
  "read",
  "create",
  "update",
  "delete",
  "emit",
  "call",
  "reference"
]);
var SCAN_ENTRY_CONFIDENCES = Object.freeze([
  "exact",
  "high",
  "medium",
  "low",
  "unknown"
]);
var MAX_SCAN_ENTRY_REFERENCES = 8;
var MAX_SCAN_ENTRY_EVIDENCE_BYTES = 4096;
var MAX_FILE_BYTES = 4 * 1024 * 1024;
var OPERATION_TOKENS = Object.freeze([
  "describe",
  "scan",
  "bind",
  "validate",
  "generate",
  "verify",
  "clean",
  "plan-clean",
  "plan-native"
]);
var SUPPORT_STATES = Object.freeze(["full", "partial", "unsupported", "unknown"]);
var CAPABILITY_IDS = Object.freeze([
  "generate.openapi",
  "generate.ui",
  "generate.zod",
  "scan.symbols",
  "verify.scenarios",
  "plan.native-gates"
]);
function entryDigest() {
  return "sha256:" + createHash("sha256").update(readSelfBytes()).digest("hex");
}
var selfBytes;
function readSelfBytes() {
  if (!selfBytes) {
    selfBytes = readFileSync(entryPath());
  }
  return selfBytes;
}
function entryPath() {
  if (typeof process !== "undefined" && process.argv?.[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) {
    return process.argv[1];
  }
  return new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
}
function runtimeMetadata() {
  return Object.freeze({
    adapter: Object.freeze({
      id: ADAPTER_ID,
      version: ADAPTER_VERSION,
      digest: entryDigest()
    }),
    compiler: deepFreeze({ ...compilerMetadata }),
    node: String(process.versions.node)
  });
}
var compilerMetadata = Object.freeze({
  vendored: false
});
function __setCompilerMetadata(metadata) {
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
    esbuild: metadata.esbuild
  });
}
var vendoredCompiler = null;
var embeddedLibs = null;
var launchExtensions = [];
function __setLaunchExtensions(extensions) {
  if (!Array.isArray(extensions)) {
    throw new RequestRefusal("launch", "launch extensions must be an array");
  }
  launchExtensions = extensions;
}
function __attachVendoredCompiler(ts, libFiles) {
  if (typeof ts !== "object" || ts === null || typeof ts.createProgram !== "function") {
    throw new RequestRefusal("compiler", "the vendored compiler namespace is malformed");
  }
  if (!(libFiles instanceof Map) || libFiles.size === 0) {
    throw new RequestRefusal("compiler", "the embedded library map is malformed");
  }
  vendoredCompiler = ts;
  embeddedLibs = libFiles;
}
function vendoredTs() {
  return vendoredCompiler;
}
function embeddedLibFiles() {
  return embeddedLibs;
}
function isSha256Digest(value) {
  return typeof value === "string" && value.length === 71 && value.startsWith("sha256:") && /^[0-9a-f]{64}$/.test(value.slice(7));
}
function isRequestId(value) {
  return typeof value === "string" && value.length === 68 && value.startsWith("req-") && /^[0-9a-f]{64}$/.test(value.slice(4));
}
function isPlanId(value) {
  return typeof value === "string" && value.length === 69 && value.startsWith("plan-") && /^[0-9a-f]{64}$/.test(value.slice(5));
}
function isToken(value) {
  return typeof value === "string" && value.length >= 1 && value.length <= 64 && /^[a-z][a-z0-9-]*$/.test(value);
}
function isCapabilityId(value) {
  return typeof value === "string" && value.length >= 1 && value.length <= 128 && value.split(".").every((segment) => /^[a-z0-9][a-z0-9_-]*$/.test(segment));
}
function isContractVersion(value) {
  return typeof value === "string" && value.length >= 5 && value.length <= 32 && /^\d+\.\d+\.\d+$/.test(value) && !/^\d{2,}|^0\d/.test(value.split(".")[0]) && value.split(".").every((part) => part.length <= 8 && !part.startsWith("0") || part === "0");
}
function isScope(value) {
  const parts = logicalSegments(value);
  if (!parts) {
    return false;
  }
  const recursive = parts[parts.length - 1] === "**";
  const head = recursive ? parts.slice(0, -1) : parts;
  return head.length >= 1 && head.every(isScopeSegment);
}
function isLogicalPath(value) {
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
function isScopeSegment(segment) {
  return typeof segment === "string" && segment.length >= 1 && segment.length <= 64 && segment !== "." && segment !== ".." && !segment.endsWith(".") && !segment.endsWith(" ") && !/~\d/.test(segment) && !isDosDevice(segment) && /^[a-z0-9.][a-z0-9._-]*$/.test(segment) && !isDriveOrScheme(segment);
}
function isDriveOrScheme(segment) {
  if (!/^[A-Za-z]/.test(segment)) {
    return false;
  }
  const run = segment.slice(1).match(/^[A-Za-z0-9+.\-]*/)[0].length;
  return segment[1] === ":" || segment[1 + run] === ":";
}
function isDosDevice(segment) {
  const devices = [
    "con",
    "prn",
    "aux",
    "nul",
    "com1",
    "com2",
    "com3",
    "com4",
    "com5",
    "com6",
    "com7",
    "com8",
    "com9",
    "lpt1",
    "lpt2",
    "lpt3",
    "lpt4",
    "lpt5",
    "lpt6",
    "lpt7",
    "lpt8",
    "lpt9"
  ];
  const base = segment.split(".")[0];
  return devices.includes(base) || devices.includes(base.replace(/\$/g, ""));
}
function scopeCovers(scope, path) {
  if (!isScope(scope) || !isLogicalPath(path)) {
    return false;
  }
  const scopeParts = scope.split("/");
  const pathParts = path.split("/");
  const recursive = scopeParts[scopeParts.length - 1] === "**";
  const prefix = recursive ? scopeParts.slice(0, -1) : scopeParts;
  if (recursive) {
    return pathParts.length > prefix.length && prefix.every((segment, index) => segment === pathParts[index]);
  }
  return pathParts.length === prefix.length && prefix.every((segment, index) => segment === pathParts[index]);
}
function decodeUtf8Fatal(bytes) {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new RequestRefusal("utf-8", "request bytes are not valid UTF-8");
  }
}
function decodeJsonDocument(bytes, { maxBytes = MAX_REQUEST_BYTES } = {}) {
  if (bytes.length > maxBytes) {
    throw new RequestRefusal("request-too-large", "request bytes exceed the transport bound");
  }
  const text = decodeUtf8Fatal(bytes);
  const parser = new StrictJsonParser(text);
  const value = parser.parseDocument();
  return value;
}
var StrictJsonParser = class {
  constructor(text) {
    this.text = text;
    this.position = 0;
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
      if (code === 32 || code === 9 || code === 10 || code === 13) {
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
        if (next === "-" || next >= "0" && next <= "9") {
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
    if (head.length === 0 || head.length > 1 && head.startsWith("0")) {
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
    this.position += 1;
    let result = "";
    for (; ; ) {
      const character = this.text[this.position];
      if (character === void 0) {
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
          case '"':
            result += '"';
            this.position += 1;
            break;
          case "\\":
            result += "\\";
            this.position += 1;
            break;
          case "/":
            result += "/";
            this.position += 1;
            break;
          case "b":
            result += "\b";
            this.position += 1;
            break;
          case "f":
            result += "\f";
            this.position += 1;
            break;
          case "n":
            result += "\n";
            this.position += 1;
            break;
          case "r":
            result += "\r";
            this.position += 1;
            break;
          case "t":
            result += "	";
            this.position += 1;
            break;
          case "u": {
            const hex = this.text.slice(this.position + 1, this.position + 5);
            if (!/^[0-9a-fA-F]{4}$/.test(hex)) {
              throw new RequestRefusal("syntax", "malformed unicode escape");
            }
            const code2 = Number.parseInt(hex, 16);
            result += String.fromCharCode(code2);
            this.position += 5;
            break;
          }
          default:
            throw new RequestRefusal("syntax", "malformed JSON escape");
        }
        continue;
      }
      const code = character.charCodeAt(0);
      if (code < 32) {
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
    for (; ; ) {
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
    const object = /* @__PURE__ */ Object.create(null);
    const seen = /* @__PURE__ */ new Set();
    this.skipWhitespace();
    if (this.text[this.position] === "}") {
      this.position += 1;
      return object;
    }
    for (; ; ) {
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
};
function canonicalJson(value) {
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
  const body = sorted.map((key) => `${JSON.stringify(key)}:${writeCanonical(value[key])}`).join(",");
  return `{${body}}`;
}
function canonicalNumber(value) {
  if (Number.isInteger(value) && Math.abs(value) < 1e15) {
    return String(value);
  }
  return JSON.stringify(value);
}
function sha256Hex(text) {
  return createHash("sha256").update(text, "utf8").digest("hex");
}
var RequestRefusal = class extends Error {
  constructor(code, message) {
    super(message ?? code);
    this.name = "RequestRefusal";
    this.code = code;
  }
};
function hasOwn(object, key) {
  return Object.prototype.hasOwnProperty.call(object, key);
}
function canonicalJsonText(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number": {
      if (!Number.isFinite(value)) {
        throw new RequestRefusal("syntax", "non-finite number cannot be canonicalized");
      }
      return Number.isInteger(value) && Math.abs(value) < 1e15 ? String(value) : JSON.stringify(value);
    }
    case "string":
      return JSON.stringify(value);
    case "object":
      break;
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
function validateNativeRequest(nativeRequest) {
  if (typeof nativeRequest !== "object" || nativeRequest === null || Array.isArray(nativeRequest)) {
    throw new RequestRefusal("native-request", "native_request must be an object");
  }
  for (const key of Object.keys(nativeRequest)) {
    if (!NATIVE_REQUEST_KEYS.includes(key)) {
      throw new RequestRefusal("native-request", "unknown native_request member");
    }
  }
  for (const key of [
    "changes",
    "scan_ref",
    "execution_policy_ref",
    "input_manifest_digest",
    "tool_catalog_digest",
    "capability_snapshot_digest"
  ]) {
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
  for (const file2 of changes.files) {
    if (typeof file2 !== "object" || file2 === null || Array.isArray(file2)) {
      throw new RequestRefusal("native-request", "changes.files entry");
    }
    if (!isLogicalPath(file2.path)) {
      throw new RequestRefusal("native-request", "changes.files path");
    }
    if (!["added", "modified", "deleted", "renamed"].includes(file2.change)) {
      throw new RequestRefusal("native-request", "changes.files change");
    }
    if (file2.before_digest !== void 0 && !isSha256Digest(file2.before_digest)) {
      throw new RequestRefusal("native-request", "changes.files before_digest");
    }
    if (file2.after_digest !== void 0 && !isSha256Digest(file2.after_digest)) {
      throw new RequestRefusal("native-request", "changes.files after_digest");
    }
  }
  if (!Array.isArray(changes.symbols) || changes.symbols.length > 1024) {
    throw new RequestRefusal("native-request", "changes.symbols bound");
  }
  validateNativeContentRef(nativeRequest.scan_ref);
  if (nativeRequest.observed_ref !== void 0) {
    validateNativeContentRef(nativeRequest.observed_ref);
  }
  const policy = nativeRequest.execution_policy_ref;
  if (typeof policy !== "object" || policy === null || Array.isArray(policy) || typeof policy.id !== "string" || policy.id.length === 0 || policy.id.length > 128 || !isContractVersion(policy.version) || !isSha256Digest(policy.digest)) {
    throw new RequestRefusal("native-request", "execution_policy_ref");
  }
  for (const key of ["input_manifest_digest", "tool_catalog_digest", "capability_snapshot_digest"]) {
    if (!isSha256Digest(nativeRequest[key])) {
      throw new RequestRefusal("native-request", key);
    }
  }
}
function validateNativeContentRef(reference) {
  if (typeof reference !== "object" || reference === null || Array.isArray(reference) || !isSha256Digest(reference.digest)) {
    throw new RequestRefusal("native-request", "content reference");
  }
  if (reference.revision !== void 0 && (typeof reference.revision !== "string" || reference.revision.length === 0 || reference.revision.length > 128)) {
    throw new RequestRefusal("native-request", "content reference revision");
  }
  if (reference.adapter !== void 0 && !isToken(reference.adapter)) {
    throw new RequestRefusal("native-request", "content reference adapter");
  }
}
var REQUEST_KEYS = Object.freeze([
  "protocol",
  "protocol_version",
  "operation",
  "request_id",
  "project_root",
  "ir_path",
  "target",
  "profile",
  "profile_digest",
  "profile_capabilities",
  "dry_run",
  "limits",
  "plan_id",
  "native_request"
]);
var NATIVE_REQUEST_KEYS = Object.freeze([
  "changes",
  "scan_ref",
  "observed_ref",
  "execution_policy_ref",
  "input_manifest_digest",
  "tool_catalog_digest",
  "capability_snapshot_digest"
]);
function validateRequestObject(document) {
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
  const request = { ...document };
  for (const key of Object.keys(request)) {
    if (request[key] === void 0) {
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
    ["profile", isToken]
  ];
  for (const [key, check] of optionalStrings) {
    if (hasOwn(request, key) && request[key] !== void 0 && !check(request[key])) {
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
    if (!Number.isInteger(timeout) || timeout < 1 || timeout > 36e5 || !Number.isInteger(output) || output < 1 || output > 1073741824) {
      throw new RequestRefusal("shape", "limits values are outside the closed bounds");
    }
  }
  const operation = request.operation;
  const apply = operation === "clean" || operation === "generate" && request.dry_run === false;
  if (apply !== hasOwn(request, "plan_id") || hasOwn(request, "plan_id") && !isPlanId(document.plan_id)) {
    throw new RequestRefusal("plan-id", "plan identity pairing is wrong");
  }
  if (operation === "plan-native" !== hasOwn(request, "native_request")) {
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
  if (operation === "generate" !== hasOwn(request, "dry_run")) {
    throw new RequestRefusal("dry-run", "dry_run pairing is wrong");
  }
  if (hasOwn(request, "dry_run") && typeof request.dry_run !== "boolean") {
    throw new RequestRefusal("dry-run", "dry_run must be a boolean");
  }
  if ((operation === "validate" || operation === "generate" || operation === "verify") && !hasOwn(request, "ir_path")) {
    throw new RequestRefusal("ir-path", "IR-carrying operations require ir_path");
  }
  if ((operation === "generate" || operation === "bind") && !hasOwn(request, "target")) {
    throw new RequestRefusal("target", "this operation requires target");
  }
  if (operation === "bind" && !hasOwn(request, "profile")) {
    throw new RequestRefusal("profile", "bind requires profile");
  }
  if (operation === "describe" && ["target", "profile", "profile_digest", "profile_capabilities", "ir_path"].some((key) => hasOwn(request, key))) {
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
var ERROR_CODES = Object.freeze({
  unsupported: Object.freeze({
    code: "operation-unsupported",
    message: "this kernel does not implement the requested operation"
  }),
  invalidProfile: Object.freeze({
    code: "profile-invalid",
    message: "the resolved project profile is not valid for this kernel"
  }),
  rootInvalid: Object.freeze({
    code: "root-invalid",
    message: "a configured read root failed lexical or physical validation"
  }),
  extensionFailed: Object.freeze({
    code: "extension-failed",
    message: "the registered extension failed inside the dispatch boundary"
  })
});
function decodeProjectProfileJson(text) {
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
function validateProfileBinding(request, profile) {
  const refusal = (detail) => ({ detail });
  if (!profile) {
    return refusal("profile-absent");
  }
  if (request.profile !== profile.id) {
    return refusal("profile-id");
  }
  if (request.target !== void 0 && request.target !== profile.target) {
    return refusal("target");
  }
  const claimed = request.profile_digest !== void 0 || request.profile_capabilities !== void 0;
  const trusted = profile.targetResolution !== void 0;
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
function describeCapabilities(profile = null, extensions = []) {
  const capabilities = {
    adapter: {
      id: ADAPTER_ID,
      version: ADAPTER_VERSION,
      digest: entryDigest()
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
      "verify.scenarios": "unsupported"
    }
  };
  if (profile && extensions.length > 0) {
    const operations = /* @__PURE__ */ new Set(["describe"]);
    const capabilitiesMap = { ...capabilities.capabilities };
    const irVersions = /* @__PURE__ */ new Set();
    const writeScopes = /* @__PURE__ */ new Set();
    for (const extension of extensions) {
      for (const operation of extension.operations) {
        operations.add(operation);
      }
      for (const [id, state] of Object.entries(extension.namedCapabilities ?? {})) {
        capabilitiesMap[id] = state;
      }
      for (const scope of extension.writeScopes ?? []) {
        writeScopes.add(scope);
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
    capabilities.write_scopes = [...writeScopes].sort();
  }
  return capabilities;
}
function buildResponse(request, payload = {}) {
  const status = hasOwn(payload, "error") ? "error" : "ok";
  const forbidden = status === "error" ? ["result", "capabilities", "progress", "writes"] : ["error"];
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
    evidence: buildEvidence(request, payload.evidence)
  };
  for (const key of ["capabilities", "result", "writes", "progress", "error"]) {
    if (hasOwn(payload, key)) {
      envelope[key] = payload[key];
    }
  }
  return envelope;
}
function buildEvidence(request, internal) {
  const identity = internal?.adapter ?? {
    id: ADAPTER_ID,
    version: ADAPTER_VERSION,
    digest: entryDigest()
  };
  const evidence = { adapter: identity };
  if (hasOwn(request, "plan_id") && request.plan_id !== void 0) {
    evidence.plan_id = request.plan_id;
  }
  return evidence;
}
function validateResolvedProjectProfile(candidate) {
  const invalid = (reason) => {
    throw new RequestRefusal("profile-invalid", `invalid resolved project profile: ${reason}`);
  };
  if (typeof candidate !== "object" || candidate === null || Array.isArray(candidate)) {
    invalid("not an object");
  }
  const allowed = [
    "id",
    "mode",
    "target",
    "readRoots",
    "exclusions",
    "targetResolution",
    "provenance",
    "localReference"
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
    invalid("readRoots");
  }
  const seen = /* @__PURE__ */ new Set();
  const readRoots = [];
  for (const root of candidate.readRoots) {
    if (typeof root !== "object" || root === null || Array.isArray(root)) {
      invalid("readRoots entries");
    }
    const keys = Object.keys(root).sort();
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
      const newSegments = scope.split("/");
      const oldSegments = other.scope.split("/");
      const prefixOf = (prefix, candidateSegments) => prefix.length <= candidateSegments.length && prefix.every((segment, index) => candidateSegments[index] === segment);
      const prefixNew = newSegments[newSegments.length - 1] === "**" ? newSegments.slice(0, -1) : newSegments;
      const prefixOld = oldSegments[oldSegments.length - 1] === "**" ? oldSegments.slice(0, -1) : oldSegments;
      if (prefixOf(prefixNew, prefixOld) || prefixOf(prefixOld, prefixNew)) {
        invalid("overlapping roots");
      }
    }
    readRoots.push({ kind: root.kind, path: spelling, scope });
  }
  if (hasOwn(candidate, "exclusions") && candidate.exclusions !== void 0) {
    if (!Array.isArray(candidate.exclusions)) {
      invalid("exclusions");
    }
    for (const exclusion of candidate.exclusions) {
      if (typeof exclusion !== "string" || !(isScope(exclusion) && exclusion.endsWith("/**") || isLogicalPath(exclusion))) {
        invalid("exclusions entries");
      }
    }
  }
  let targetResolution;
  if (candidate.targetResolution !== void 0 && candidate.targetResolution !== null) {
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
      if (typeof capability !== "object" || capability === null || Object.keys(capability).sort().join(",") !== "id,support" || !isCapabilityId(capability.id) || !SUPPORT_STATES.includes(capability.support)) {
        invalid("targetResolution capability entry");
      }
      if (Buffer.compare(Buffer.from(capability.id, "utf8"), Buffer.from(previous, "utf8")) <= 0) {
        invalid("targetResolution capability order");
      }
      previous = capability.id;
    }
    targetResolution = {
      digest: resolution.digest,
      capabilities: resolution.capabilities.map((capability) => ({ ...capability }))
    };
  } else if (candidate.targetResolution === void 0) {
    targetResolution = void 0;
  } else {
    targetResolution = void 0;
  }
  let provenance;
  if (candidate.provenance === void 0 || candidate.provenance === null) {
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
    if (typeof raw.origin !== "string" || !["declared", "observed", "inferred"].includes(raw.origin) || typeof raw.revision !== "string" || raw.revision.length === 0 || raw.revision.length > 256 || typeof raw.disposition !== "string" || !["public-fixture", "user-confirmed", "unconfirmed"].includes(raw.disposition)) {
      invalid("provenance values");
    }
    provenance = { ...raw };
  }
  let localReference;
  if (candidate.localReference !== void 0 && candidate.localReference !== null) {
    if (typeof candidate.localReference !== "object" || Array.isArray(candidate.localReference)) {
      invalid("localReference");
    }
    localReference = deepFreeze(structuredClone(candidate.localReference));
  }
  return deepFreeze({
    id: candidate.id,
    mode: candidate.mode,
    target: candidate.target,
    readRoots: readRoots.map((root) => ({ ...root })),
    exclusions: Object.freeze([...candidate.exclusions ?? []]),
    ...targetResolution !== void 0 ? { targetResolution } : {},
    provenance,
    ...localReference !== void 0 ? { localReference } : {}
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
function resolveReadRoots(permittedRoot, profile) {
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
      absolute: resolve(permittedRoot, ...root.path.split("/"))
    });
  }
  if (failures.length > 0) {
    const refusal = new RequestRefusal("root-invalid", "read root validation failed");
    refusal.failures = failures;
    throw refusal;
  }
  return deepFreeze(resolved);
}
function lexicalRootViolation(path) {
  if (typeof path !== "string" || path.length === 0) {
    return "empty";
  }
  if (path.length > 512) {
    return "overlong";
  }
  if (path === ".") {
    return "project-root";
  }
  if (path.startsWith("/") || path.startsWith("\\") || path.startsWith("//") || path.startsWith("\\\\")) {
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
function physicalRootViolation(permittedRoot, root) {
  let target;
  try {
    target = resolve(permittedRoot, ...root.path.split("/"));
  } catch {
    return "resolve";
  }
  if (!isInsideRoot(permittedRoot, target)) {
    return "containment";
  }
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
function isInsideRoot(root, candidate) {
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
  const bounded2 = candidateNormalized === rootNormalized ? false : candidateNormalized.startsWith(rootNormalized + separator);
  return Boolean(bounded2 && rootNormalized.length > 0);
}
function validateExtensionDescriptor(descriptor2) {
  const invalid = (reason) => {
    throw new RequestRefusal("extension-invalid", `invalid extension descriptor: ${reason}`);
  };
  if (typeof descriptor2 !== "object" || descriptor2 === null) {
    invalid("not an object");
  }
  const allowed = ["id", "version", "operations", "namedCapabilities", "acceptedIrVersions", "writeScopes", "invoke"];
  for (const key of Object.keys(descriptor2)) {
    if (!allowed.includes(key)) {
      invalid(`unknown member ${key}`);
    }
  }
  if (!isToken(descriptor2.id)) {
    invalid("id");
  }
  if (!isContractVersion(descriptor2.version)) {
    invalid("version");
  }
  if (!Array.isArray(descriptor2.operations) || descriptor2.operations.length === 0 || !descriptor2.operations.every((operation) => OPERATION_TOKENS.includes(operation))) {
    invalid("operations");
  }
  if (descriptor2.operations.includes("describe")) {
    invalid("describe is kernel-owned");
  }
  if (hasOwn(descriptor2, "namedCapabilities") && descriptor2.namedCapabilities !== void 0) {
    if (typeof descriptor2.namedCapabilities !== "object" || Array.isArray(descriptor2.namedCapabilities)) {
      invalid("namedCapabilities");
    }
    for (const [id, state] of Object.entries(descriptor2.namedCapabilities)) {
      if (!CAPABILITY_IDS.includes(id) || !SUPPORT_STATES.includes(state)) {
        invalid(`named capability ${id}`);
      }
    }
  }
  if (hasOwn(descriptor2, "acceptedIrVersions") && descriptor2.acceptedIrVersions !== void 0) {
    if (!Array.isArray(descriptor2.acceptedIrVersions) || !descriptor2.acceptedIrVersions.every(isContractVersion)) {
      invalid("acceptedIrVersions");
    }
  }
  if (hasOwn(descriptor2, "writeScopes") && descriptor2.writeScopes !== void 0) {
    if (!Array.isArray(descriptor2.writeScopes) || descriptor2.writeScopes.length > MAX_WRITE_SCOPES || !descriptor2.writeScopes.every(isScope) || descriptor2.writeScopes.some((scope) => scopesOverlap(descriptor2.writeScopes, scope))) {
      invalid("writeScopes");
    }
  }
  if (typeof descriptor2.invoke !== "function") {
    invalid("invoke");
  }
  return deepFreeze({
    id: descriptor2.id,
    version: descriptor2.version,
    operations: Object.freeze([...descriptor2.operations]),
    namedCapabilities: descriptor2.namedCapabilities ? deepFreeze({ ...descriptor2.namedCapabilities }) : void 0,
    acceptedIrVersions: descriptor2.acceptedIrVersions ? Object.freeze([...descriptor2.acceptedIrVersions]) : void 0,
    writeScopes: descriptor2.writeScopes ? Object.freeze([...descriptor2.writeScopes]) : void 0,
    invoke: descriptor2.invoke
  });
}
var MAX_WRITE_SCOPES = 8;
function scopesOverlap(scopes, candidate) {
  return scopes.some((other) => other !== candidate && (scopeCovers(other, candidate.replace(/\/\*\*$/, "")) || scopeCovers(candidate, other.replace(/\/\*\*$/, "")) || scopeCovers(other, candidate) || scopeCovers(candidate, other)));
}
function normalizeExtensionOutcome(outcome) {
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
    data: outcome.data === void 0 ? void 0 : structuredClone(outcome.data),
    evidence: outcome.evidence === void 0 ? void 0 : structuredClone(outcome.evidence),
    diagnostics: outcome.diagnostics === void 0 ? void 0 : structuredClone(outcome.diagnostics)
  });
}
function createKernel(options = {}) {
  const identity = options.identity ?? {
    id: ADAPTER_ID,
    version: ADAPTER_VERSION,
    digest: entryDigest()
  };
  const profile = options.resolvedProjectProfile ? validateResolvedProjectProfile(options.resolvedProjectProfile) : null;
  const extensions = /* @__PURE__ */ new Map();
  for (const descriptor2 of options.extensionRegistry ?? []) {
    const validated = validateExtensionDescriptor(descriptor2);
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
        [...extensions.values()]
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
                ...profile.localReference !== void 0 ? { localReference: structuredClone(profile.localReference) } : {}
              } : null,
              extensions: [...extensions.keys()]
            }
          }
        };
      }
      const extension = [...extensions.values()].find((candidate) => candidate.operations.includes(operation));
      if (!extension) {
        return {
          response: buildUnsupportedResponse(validatedRequest),
          internal: {
            state: "unsupported",
            evidence: {
              adapter: { ...identity },
              operation,
              reason: "no-installed-extension"
            }
          }
        };
      }
      if (!profile) {
        return {
          response: buildResponse(validatedRequest, {
            error: {
              class: "unsupported",
              code: ERROR_CODES.unsupported.code,
              message: ERROR_CODES.unsupported.message,
              retryable: false,
              partial: false,
              detail: ["profile-absent"]
            }
          }),
          internal: {
            state: "unsupported",
            evidence: {
              adapter: { ...identity },
              operation,
              reason: "profile-absent"
            }
          }
        };
      }
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
              detail: [bindingRefusal.detail]
            }
          }),
          internal: {
            state: "failed",
            evidence: {
              adapter: { ...identity },
              operation,
              reason: "profile-binding-mismatch",
              detail: bindingRefusal.detail
            }
          }
        };
      }
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
                detail: error.failures.map((failure) => boundToken(failure.reason)).slice(0, 16)
              }
            }),
            internal: {
              state: "failed",
              evidence: {
                adapter: { ...identity },
                operation,
                rootFailures: error.failures.map((failure) => ({ ...failure }))
              }
            }
          };
        }
        throw error;
      }
      const readView = createReadView(permittedRoot, roots, profile);
      readView.permittedProjectRoot = permittedRoot;
      let writeView = null;
      if (operation === "generate") {
        writeView = createWriteView(
          permittedRoot,
          extension.writeScopes ?? [],
          { writable: validatedRequest.dry_run === false }
        );
      }
      let outcome;
      try {
        outcome = normalizeExtensionOutcome(extension.invoke({
          operation,
          request: validatedRequest,
          profile,
          readView,
          ...writeView !== null ? { writeView } : {},
          cancellation: trustedExecutionContext.cancellation ?? null,
          limits: trustedExecutionContext.limits ?? { files: 4096, bytes: 4 * 1024 * 1024 }
        }));
      } catch (error) {
        return {
          response: buildResponse(validatedRequest, {
            error: {
              class: "infrastructure",
              code: ERROR_CODES.extensionFailed.code,
              message: ERROR_CODES.extensionFailed.message,
              retryable: false,
              partial: false
            }
          }),
          internal: {
            state: "failed",
            evidence: {
              adapter: { ...identity },
              operation,
              extension: extension.id,
              failure: "extension-threw",
              reason: boundToken(String(error?.message ?? "unknown").slice(0, 64))
            }
          }
        };
      }
      recordSink(sink, validatedRequest, outcome, profile);
      return {
        response: projectOutcome(validatedRequest, outcome),
        internal: outcome
      };
    }
  };
}
function boundToken(text) {
  const bounded2 = text.replace(/[^a-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "").slice(0, 128);
  return bounded2 === "" ? "unspecified" : bounded2;
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
    ...profile.localReference !== void 0 ? { localReference: structuredClone(profile.localReference) } : {},
    evidence: structuredClone(outcome.evidence ?? null),
    diagnostics: structuredClone(outcome.diagnostics ?? null)
  });
}
function buildUnsupportedResponse(request) {
  return buildResponse(request, {
    error: {
      class: "unsupported",
      code: ERROR_CODES.unsupported.code,
      message: ERROR_CODES.unsupported.message,
      retryable: false,
      partial: false
    }
  });
}
function projectOutcome(request, outcome) {
  if (outcome.state === "complete") {
    const writes = projectWrites(outcome.data);
    if (writes) {
      if (request.operation === "generate") {
        if (Array.isArray(outcome.data?.findings) && outcome.data.findings.length > 0) {
          return buildResponse(request, {
            error: {
              class: "invalid",
              code: "outcome-partial-unsupported-constructs",
              message: "the IR carries constructs outside the declared generation subset; nothing was emitted",
              retryable: false,
              partial: true,
              detail: outcome.data.findings.slice(0, 16).map((finding) => boundToken(`${finding.code}:${finding.detail ?? ""}`))
            }
          });
        }
        const response = buildResponse(request, { writes });
        if (!hasOwn(request, "plan_id") && isPlanId(outcome.data?.plan_id)) {
          response.evidence.plan_id = outcome.data.plan_id;
        }
        return response;
      }
      const result2 = projectVerifyFindings(request, outcome.data);
      if (result2) {
        return buildResponse(request, { result: result2 });
      }
      return buildResponse(request, {
        error: {
          class: "conflict",
          code: "outcome-unrepresentable",
          message: "the extension outcome cannot be represented on the closed wire",
          retryable: false,
          partial: true
        }
      });
    }
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
        partial: true
      }
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
        partial: outcome.state === "partial"
      }
    });
  }
  return buildUnsupportedResponse(request);
}
function projectWrites(data) {
  if (data === void 0 || data === null || typeof data !== "object" || !Array.isArray(data.writes)) {
    return void 0;
  }
  if (data.writes.length > 1e4) {
    return void 0;
  }
  const writes = [];
  for (const entry of data.writes) {
    if (typeof entry !== "object" || entry === null || !isLogicalPath(entry.path) || entry.action !== "create" && entry.action !== "replace" && entry.action !== "delete") {
      return void 0;
    }
    if (entry.action === "delete") {
      if (entry.sha256 !== void 0) return void 0;
      writes.push({ path: entry.path, action: "delete" });
      continue;
    }
    if (!isSha256Digest(entry.sha256)) {
      return void 0;
    }
    writes.push({ path: entry.path, action: entry.action, sha256: entry.sha256 });
  }
  const paths = writes.map((entry) => entry.path);
  const sorted = [...paths].sort();
  if (paths.join("\0") !== sorted.join("\0")) {
    return void 0;
  }
  return writes;
}
function projectVerifyFindings(request, data) {
  if (request.operation !== "verify") {
    return void 0;
  }
  if (!Array.isArray(data?.findings)) {
    return void 0;
  }
  if (data.findings.length > 1e4) {
    return void 0;
  }
  const findings = [];
  for (const finding of data.findings) {
    if (typeof finding !== "object" || finding === null || !isLogicalPath(finding.path) || typeof finding.code !== "string" || finding.code.length === 0 || [...finding.code].length > 128 || finding.detail !== void 0 && (typeof finding.detail !== "string" || [...finding.detail].length > 128)) {
      return void 0;
    }
    findings.push(finding.detail === void 0 ? { path: finding.path, code: finding.code } : { path: finding.path, code: finding.code, detail: finding.detail });
  }
  return findings.length > 0 ? { ok: true, findings } : { ok: true };
}
function projectResult(data) {
  if (data === void 0 || data === null || typeof data !== "object") {
    return void 0;
  }
  if (data.native_plan !== void 0) {
    const np = data.native_plan;
    if (typeof np !== "object" || np === null || np.kind !== "native-plan" || !isSha256Digest(np.plan_digest)) {
      return void 0;
    }
    return { native_plan: { digest: np.plan_digest, kind: np.kind } };
  }
  if (!Array.isArray(data.entries)) {
    return void 0;
  }
  if (data.entries.length > 1e4) {
    return void 0;
  }
  const entries = [];
  for (const entry of data.entries) {
    if (typeof entry !== "object" || entry === null || !isLogicalPath(entry.path) || typeof entry.kind !== "string" || [...entry.kind].length > 128 || entry.detail !== void 0 && (typeof entry.detail !== "string" || [...entry.detail].length > 128)) {
      return void 0;
    }
    const projected = entry.detail === void 0 ? { path: entry.path, kind: entry.kind } : { path: entry.path, kind: entry.kind, detail: entry.detail };
    if (entry.evidence !== void 0) {
      const evidence = projectScanEntryEvidence(entry.evidence);
      if (!evidence) {
        return void 0;
      }
      projected.evidence = evidence;
    }
    entries.push(projected);
  }
  const result = { entries };
  if (data.complete === false) {
    return void 0;
  }
  return result;
}
function projectScanEntryEvidence(evidence) {
  if (typeof evidence !== "object" || evidence === null || Array.isArray(evidence)) {
    return void 0;
  }
  for (const key of Object.keys(evidence)) {
    if (!SCAN_ENTRY_EVIDENCE_KEYS.includes(key)) {
      return void 0;
    }
  }
  const projected = {};
  if (evidence.signature !== void 0) {
    if (evidence.signature !== null && !isSha256Digest(evidence.signature)) {
      return void 0;
    }
    projected.signature = evidence.signature;
  }
  if (evidence.references !== void 0) {
    if (!Array.isArray(evidence.references) || evidence.references.length > MAX_SCAN_ENTRY_REFERENCES) {
      return void 0;
    }
    const references = [];
    for (const reference of evidence.references) {
      if (typeof reference !== "object" || reference === null || Array.isArray(reference)) {
        return void 0;
      }
      const keys = Object.keys(reference).sort();
      if (keys.join(",") !== SCAN_ENTRY_REFERENCE_KEYS.join(",")) {
        return void 0;
      }
      if (typeof reference.target !== "string" || reference.target.length === 0 || reference.target.length > 192 || !isSemanticId(reference.target) || !SCAN_ENTRY_ROLES.includes(reference.role) || !SCAN_ENTRY_CONFIDENCES.includes(reference.confidence)) {
        return void 0;
      }
      references.push({
        target: reference.target,
        role: reference.role,
        confidence: reference.confidence
      });
    }
    projected.references = references;
  }
  if (canonicalJsonText(projected).length > MAX_SCAN_ENTRY_EVIDENCE_BYTES) {
    return void 0;
  }
  return projected;
}
function isSemanticId(value) {
  return typeof value === "string" && value.length >= 1 && value.length <= 192 && /^[A-Za-z0-9_][A-Za-z0-9_.:-]*$/.test(value);
}
function createReadView(permittedRoot, roots, profile) {
  let filesRead = 0;
  let bytesRead = 0;
  const exclusions = profile.exclusions ?? [];
  const excluded = (path) => exclusions.some((exclusion) => scopeCovers(exclusion, path) || exclusion === path);
  const excludedPath = (logicalPath) => {
    if (excluded(logicalPath)) {
      return true;
    }
    const segments = logicalPath.split("/");
    for (let depth = 1; depth < segments.length; depth += 1) {
      const ancestor = segments.slice(0, depth).join("/");
      if (exclusions.some((exclusion) => scopeCovers(exclusion, `${ancestor}/probe`) || exclusion === ancestor)) {
        return true;
      }
    }
    return false;
  };
  const revalidate = (logicalPath, expectedKind) => {
    const root = roots.find((candidate) => scopeCovers(candidate.scope, logicalPath) || candidate.kind === "file" && candidate.path === logicalPath);
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
    counters: () => ({ filesRead, bytesRead })
  };
}
var MAX_WRITE_FILES = 1024;
var MAX_WRITE_FILE_BYTES = 4 * 1024 * 1024;
function createWriteView(permittedRoot, scopes, { writable = false } = {}) {
  let filesWritten = 0;
  let bytesWritten = 0;
  const authorize = (logicalPath) => {
    if (!isLogicalPath(logicalPath)) {
      throw new RequestRefusal("write-denied", "path is not a logical path");
    }
    if (!scopes.some((scope) => scopeCovers(scope, logicalPath))) {
      throw new RequestRefusal("write-denied", "path outside the declared write scopes");
    }
    if (protectedHomeViolation(logicalPath)) {
      throw new RequestRefusal("write-denied", "path is a protected home");
    }
    return resolve(permittedRoot, ...logicalPath.split("/"));
  };
  return {
    scopes: [...scopes],
    writable,
    /** Whether the staged view already has this exact file. */
    exists(logicalPath) {
      const absolute = authorize(logicalPath);
      let metadata;
      try {
        metadata = lstatSync(absolute);
      } catch (error) {
        if (error?.code === "ENOENT") return false;
        throw new RequestRefusal("write-denied", "uninspectable");
      }
      if (!metadata.isFile()) {
        throw new RequestRefusal("write-denied", "not a regular file");
      }
      return true;
    },
    /**
     * Write one file's exact bytes. `action` must match the observed
     * state: create requires absence, replace requires presence. Returns
     * the byte count written.
     */
    write(logicalPath, action, bytes) {
      if (!writable) {
        throw new RequestRefusal("write-denied", "this dispatch is read-only");
      }
      if (action !== "create" && action !== "replace") {
        throw new RequestRefusal("write-denied", "action must be create or replace");
      }
      if (!Buffer.isBuffer(bytes)) {
        throw new RequestRefusal("write-denied", "bytes must be a buffer");
      }
      if (bytes.length > MAX_WRITE_FILE_BYTES) {
        throw new RequestRefusal("write-denied", "file exceeds the write bound");
      }
      if (filesWritten >= MAX_WRITE_FILES) {
        throw new RequestRefusal("write-denied", "file count cap exhausted");
      }
      if (bytesWritten + bytes.length > MAX_WRITE_FILE_BYTES * MAX_WRITE_FILES) {
        throw new RequestRefusal("write-denied", "byte cap exhausted");
      }
      const absolute = authorize(logicalPath);
      const exists = this.exists(logicalPath);
      if (action === "create" && exists) {
        throw new RequestRefusal("write-denied", "create on an existing file");
      }
      if (action === "replace" && !exists) {
        throw new RequestRefusal("write-denied", "replace on a missing file");
      }
      mkdirSync(dirname(absolute), { recursive: true });
      const stage = `${absolute}.lekalo-stage`;
      writeFileSync(stage, bytes);
      renameSync(stage, absolute);
      filesWritten += 1;
      bytesWritten += bytes.length;
      return bytes.length;
    },
    counters: () => ({ filesWritten, bytesWritten })
  };
}
function protectedHomeViolation(logicalPath) {
  const segments = logicalPath.split("/");
  if (segments[0] === "lekalo" || segments[0] === "openspec/") {
    return true;
  }
  if (segments[0] === ".lekalo") {
    const second = segments[1];
    return second === "ir" || second === "cache" || second === "import" || second === "privacy" || second === "consumer" || second === "generated";
  }
  return segments.length === 1 && segments[0] === "lekalo.lock";
}
function readRequestBytes(argv = process.argv) {
  const markers = argv.filter((argument) => argument === "--lekalo-request-file");
  if (markers.length > 1) {
    throw new RequestRefusal("transport", "ambiguous transport flags");
  }
  const marker = argv.indexOf("--lekalo-request-file");
  if (marker !== -1) {
    const value = argv[marker + 1];
    if (value === void 0 || value === "" || argv.slice(marker + 2).includes("--lekalo-request-file")) {
      throw new RequestRefusal("transport", "missing request-file value");
    }
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
  }
}
function readBoundedStream(fd, bound = MAX_REQUEST_BYTES) {
  const chunks = [];
  let total = 0;
  const chunkSize = 64 * 1024;
  for (; ; ) {
    const chunk = Buffer.allocUnsafe(Math.min(chunkSize, bound + 1 - total));
    let read;
    try {
      read = readSyncFailable(fd, chunk);
    } catch (error) {
      if (error?.code === "EAGAIN") {
        continue;
      }
      if (error?.code === "EINTR") {
        continue;
      }
      throw new RequestRefusal("transport", "the request stream could not be read");
    }
    if (read === 0) {
      break;
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
  return readSync(fd, buffer, 0, buffer.length, null);
}
function stderrDiagnostic(code) {
  const bounded2 = boundToken(code);
  return JSON.stringify({ kernel: ADAPTER_ID, diagnostic: bounded2 });
}
function extractProjectProfileJson(argv = process.argv.slice(2)) {
  const markers = argv.filter((argument) => argument === "--lekalo-project-profile-json");
  if (markers.length > 1) {
    throw new RequestRefusal("profile-input", "ambiguous launch profile inputs");
  }
  const marker = argv.indexOf("--lekalo-project-profile-json");
  if (marker === -1) {
    return void 0;
  }
  const value = argv[marker + 1];
  if (value === void 0 || value === "") {
    throw new RequestRefusal("profile-input", "missing launch profile value");
  }
  return value;
}
async function main() {
  const argvWithoutEntry = process.argv.slice(2);
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
    if (profileJson !== void 0) {
      const trusted = decodeProjectProfileJson(profileJson);
      const kernel2 = createKernel({
        resolvedProjectProfile: trusted.profile,
        extensionRegistry: launchExtensions
      });
      options.kernel = kernel2;
    }
    const document = decodeJsonDocument(requestBytes);
    const request = validateRequestObject(document);
    const kernel = options.kernel ?? createKernel();
    const dispatched = kernel.dispatch(request, { permittedProjectRoot: process.cwd() });
    process.stdout.write(canonicalJson(dispatched.response));
  } catch (error) {
    process.stderr.write(stderrDiagnostic(error?.code ?? "invalid") + "\n");
    process.exitCode = 1;
  }
}
function runIfEntry(entryUrl) {
  if (typeof process === "undefined" || !process.argv?.[1] || typeof entryUrl !== "string") {
    return;
  }
  try {
    if (resolve(process.argv[1]) === resolve(fileURLToPath(entryUrl))) {
      return main();
    }
  } catch {
    return void 0;
  }
  return void 0;
}

// src/zod-gen.mjs
import { createHash as createHash3 } from "node:crypto";

// src/zod-emit.mjs
import { createHash as createHash2 } from "node:crypto";

// src/zod-map.mjs
var MAP_CONTRACT = "lekalo/zod-map/v0.3.2";
var IR_IDENTITY = "dev.lekalo.ir@0.2.16";
var ZOD_DIR = "src/generated/node-typescript/zod";
var UNSUPPORTED = "zod.unsupported-construct";
var DEFAULT_POLICY = deepFreeze2({
  date: "date-string",
  unknownKeys: "strict"
});
var SCHEMA_KINDS = deepFreeze2([
  "scalar",
  "enum",
  "value-object",
  "entity",
  "command",
  "query",
  "event"
]);
var DATE_POLICIES = deepFreeze2(["date-string", "date-native"]);
var UNKNOWN_KEY_POLICIES = deepFreeze2(["strict", "strip"]);
var MAX_FLATTEN_DEPTH = 8;
function deepFreeze2(value) {
  if (value !== null && typeof value === "object") {
    for (const key of Object.keys(value)) deepFreeze2(value[key]);
    Object.freeze(value);
  }
  return value;
}
function pascal(text) {
  return text.split("_").filter((part) => part.length > 0).map((part) => part[0].toUpperCase() + part.slice(1)).join("");
}
var KIND_SUFFIX = deepFreeze2({
  command: "Input",
  query: "Result",
  event: "Payload"
});
var Unsupported = class extends Error {
  constructor(detail) {
    super(detail);
    this.name = "Unsupported";
  }
};
function mapProject(ir, policy = DEFAULT_POLICY) {
  const definitions = indexDefinitions(ir);
  const names = allocateNames(ir);
  const branded = collectBrandedScalars(ir);
  const context = { policy, definitions, names, branded, findings: [] };
  const modules = /* @__PURE__ */ new Map();
  for (const definition of ir.definitions ?? []) {
    if (!SCHEMA_KINDS.includes(definition.kind)) continue;
    const mapped = mapDefinition(definition, context);
    if (!mapped) continue;
    const moduleId = definition.id.split(".")[0];
    let module = modules.get(moduleId);
    if (!module) {
      module = { id: moduleId, declarations: [], imports: [], fields: {} };
      modules.set(moduleId, module);
    }
    module.declarations.push(mapped);
  }
  const byExport = /* @__PURE__ */ new Map();
  for (const module of modules.values()) {
    for (const declaration of module.declarations) {
      byExport.set(`${declaration.module}/${declaration.exportName}`, declaration);
    }
  }
  const result = [];
  for (const moduleId of [...modules.keys()].sort()) {
    const module = modules.get(moduleId);
    module.declarations.sort(bySemanticId);
    module.imports = collectImports(module);
    module.fields = {};
    for (const declaration of module.declarations) {
      recordFieldPaths(module, declaration, byExport);
    }
    result.push(module);
  }
  context.findings.sort(compareFindings);
  return { modules: result, findings: context.findings };
}
function indexDefinitions(ir) {
  const index = /* @__PURE__ */ new Map();
  for (const definition of ir.definitions ?? []) {
    index.set(definition.id, definition);
  }
  return index;
}
function collectBrandedScalars(ir) {
  const branded = /* @__PURE__ */ new Set();
  const visit = (type) => {
    if (type === null || typeof type !== "object" || Array.isArray(type)) {
      return;
    }
    if (typeof type.ref === "string") {
      branded.add(type.ref);
      return;
    }
    if (type.optional !== void 0) {
      visit(type.optional);
    }
  };
  for (const definition of ir.definitions ?? []) {
    if (definition.kind !== "entity") continue;
    const byName = new Map(
      (definition.fields ?? []).map((field) => [field.name, field])
    );
    for (const name of definition.identity ?? []) {
      visit(byName.get(name)?.type);
    }
  }
  return branded;
}
function allocateNames(ir) {
  const names = /* @__PURE__ */ new Map();
  const used = /* @__PURE__ */ new Map();
  for (const definition of ir.definitions ?? []) {
    if (!SCHEMA_KINDS.includes(definition.kind)) continue;
    const moduleId = definition.id.split(".")[0];
    const local = definition.id.slice(moduleId.length + 1);
    let candidate = pascal(moduleId) + pascal(local) + (KIND_SUFFIX[definition.kind] ?? "");
    const seen = used.get(moduleId) ?? /* @__PURE__ */ new Set();
    used.set(moduleId, seen);
    let suffix = 1;
    while (seen.has(candidate)) {
      suffix += 1;
      candidate = `${candidate.replace(/\d+$/, "")}${suffix}`;
    }
    seen.add(candidate);
    names.set(definition.id, {
      exportName: `${candidate}Schema`,
      typeName: candidate
    });
  }
  return names;
}
function mapDefinition(definition, context) {
  const moduleId = definition.id.split(".")[0];
  const naming = context.names.get(definition.id);
  try {
    switch (definition.kind) {
      case "scalar":
        return mapScalar(definition, naming, context, moduleId);
      case "enum":
        return mapEnum(definition, naming, moduleId);
      case "value-object":
      case "entity":
      case "command":
      case "event":
        return mapObject(definition, naming, context, moduleId);
      case "query":
        return mapQuery(definition, naming, context, moduleId);
      default:
        return void 0;
    }
  } catch (error) {
    if (error instanceof Unsupported) {
      context.findings.push({
        path: `${ZOD_DIR}/${moduleId}.ts`,
        code: UNSUPPORTED,
        detail: `symbol:${definition.id}`
      });
      return void 0;
    }
    throw error;
  }
}
function mapScalar(definition, naming, context, moduleId) {
  const expr = scalarExpr(definition.base, context.policy);
  const branded = context.branded.has(definition.id);
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "scalar",
    exportName: naming.exportName,
    typeName: naming.typeName,
    branded,
    expr: branded ? { k: "brand", inner: expr, brand: definition.id } : expr
  };
}
function mapEnum(definition, naming, moduleId) {
  const values = (definition.values ?? []).map((value) => value?.value);
  if (values.length === 0 || values.some((value) => typeof value !== "string")) {
    throw new Unsupported(definition.id);
  }
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "enum",
    exportName: naming.exportName,
    typeName: naming.typeName,
    // Declared order is semantic; never sort enum members.
    values,
    expr: { k: "enum", values }
  };
}
function mapObject(definition, naming, context, moduleId) {
  const strict = context.policy.unknownKeys === "strict";
  const members = definition.kind === "command" ? definition.input : definition.kind === "event" ? definition.payload : definition.fields;
  const fields = (members ?? []).map((field) => {
    const inner = mapType(field.type, context);
    const expr = field.required === true ? inner : { k: "optional", inner };
    return { name: field.name, required: field.required === true, expr };
  });
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "object",
    objectKind: definition.kind,
    exportName: naming.exportName,
    typeName: naming.typeName,
    strict,
    fields,
    expr: {
      k: "object",
      strict,
      fields: fields.map((field) => ({ name: field.name, expr: field.expr }))
    }
  };
}
function mapQuery(definition, naming, context, moduleId) {
  if (!definition.returns) {
    throw new Unsupported(definition.id);
  }
  return {
    semanticId: definition.id,
    module: moduleId,
    kind: "alias",
    exportName: naming.exportName,
    typeName: naming.typeName,
    expr: mapType(definition.returns, context)
  };
}
function mapType(type, context) {
  if (type === null || typeof type !== "object" || Array.isArray(type)) {
    throw new Unsupported("type-shape");
  }
  if (typeof type.ref === "string") {
    return mapRef(type.ref, context);
  }
  if (type.list !== void 0) {
    return { k: "array", item: mapType(type.list, context) };
  }
  if (type.optional !== void 0) {
    return { k: "nullable", inner: mapType(type.optional, context) };
  }
  throw new Unsupported("type-shape");
}
function mapRef(id, context) {
  const target = context.definitions.get(id);
  if (!target || !SCHEMA_KINDS.includes(target.kind)) {
    throw new Unsupported(`ref:${id}`);
  }
  const naming = context.names.get(id);
  return {
    k: "ref",
    name: naming.exportName,
    module: id.split(".")[0]
  };
}
function scalarExpr(base, policy) {
  switch (base) {
    case "string":
      return { k: "string" };
    case "number":
      return { k: "number" };
    case "boolean":
      return { k: "boolean" };
    case "date":
      return policy.date === "date-native" ? { k: "dateNative" } : { k: "dateString" };
    case "datetime":
      return { k: "datetime" };
    case "uuid":
      return { k: "uuid" };
    case "uri":
      return { k: "uri" };
    default:
      throw new Unsupported(`base:${base}`);
  }
}
function collectImports(module) {
  const byModule = /* @__PURE__ */ new Map();
  const visit = (expr) => {
    if (!expr || typeof expr !== "object") return;
    if (expr.k === "ref") {
      if (expr.module === module.id) return;
      let entry = byModule.get(expr.module);
      if (!entry) {
        entry = /* @__PURE__ */ new Set();
        byModule.set(expr.module, entry);
      }
      entry.add(expr.name);
      return;
    }
    if (expr.k === "array") {
      visit(expr.item);
      return;
    }
    if (expr.k === "object") {
      for (const field of expr.fields) visit(field.expr);
      return;
    }
    if (expr.k === "nullable" || expr.k === "optional" || expr.k === "brand") {
      visit(expr.inner);
    }
  };
  for (const declaration of module.declarations) visit(declaration.expr);
  return [...byModule.keys()].sort().map((id) => ({
    module: id,
    names: [...byModule.get(id)].sort()
  }));
}
function recordFieldPaths(module, mapped, byExport) {
  const fields = module.fields;
  if (!Object.hasOwn(fields, mapped.exportName)) {
    fields[mapped.exportName] = mapped.semanticId;
  }
  if (mapped.kind !== "object") return;
  if (!Object.hasOwn(fields, "")) {
    fields[""] = mapped.semanticId;
  }
  for (const field of mapped.fields) {
    if (!Object.hasOwn(fields, field.name)) {
      fields[field.name] = mapped.semanticId;
    }
    flattenFieldPath(fields, field.expr, field.name, mapped, byExport, 0);
  }
}
function flattenFieldPath(fields, expr, prefix, owner, byExport, depth) {
  if (depth >= MAX_FLATTEN_DEPTH) return;
  if (!expr || typeof expr !== "object") return;
  if (expr.k === "nullable" || expr.k === "optional") {
    flattenFieldPath(fields, expr.inner, prefix, owner, byExport, depth + 1);
    return;
  }
  if (expr.k === "array") {
    flattenFieldPath(
      fields,
      expr.item,
      `${prefix}.0`,
      owner,
      byExport,
      depth + 1
    );
    return;
  }
  if (expr.k !== "ref" || expr.module !== owner.module) return;
  const declaration = byExport.get(`${expr.module}/${expr.name}`);
  if (!declaration || declaration.kind !== "object") return;
  for (const field of declaration.fields) {
    const path = `${prefix}.${field.name}`;
    if (!Object.hasOwn(fields, path)) {
      fields[path] = declaration.semanticId;
    }
    flattenFieldPath(fields, field.expr, path, declaration, byExport, depth + 1);
  }
}
function bySemanticId(left, right) {
  return left.semanticId < right.semanticId ? -1 : left.semanticId > right.semanticId ? 1 : 0;
}
function compareFindings(left, right) {
  const key = (finding) => `${finding.path}\0${finding.code}\0${finding.detail ?? ""}`;
  const leftKey = key(left);
  const rightKey = key(right);
  return leftKey < rightKey ? -1 : leftKey > rightKey ? 1 : 0;
}

// src/zod-emit.mjs
var ADAPTER_ID2 = "lekalo-target-node-typescript";
function sha256(text) {
  return "sha256:" + createHash2("sha256").update(text, "utf8").digest("hex");
}
function canonicalJson2(value) {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number":
      if (!Number.isFinite(value)) throw new TypeError("non-finite number");
      return Number.isInteger(value) && Math.abs(value) < 1e15 ? String(value) : JSON.stringify(value);
    case "string":
      return JSON.stringify(value);
    case "object": {
      if (Array.isArray(value)) {
        return `[${value.map(canonicalJson2).join(",")}]`;
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
      return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJson2(value[key])}`).join(",")}}`;
    }
    default:
      throw new TypeError("unserializable value");
  }
}
function file(path, text, map) {
  return { path, text, map };
}
function emitFiles({ modules, inputDigest, adapterVersion, irIdentity }) {
  const files = [file(`${ZOD_DIR}/runtime.ts`, runtimeText(adapterVersion))];
  const groups = emissionGroups(modules);
  for (const group of groups) {
    const others = groups.filter((candidate) => candidate !== group);
    const outbound = groupImports(group.modules, new Set(group.modules.map((module) => module.id)), others);
    const emitGroup = { ...group, imports: outbound };
    const emitted = emitModule(emitGroup, {
      inputDigest,
      adapterVersion,
      irIdentity
    });
    files.push(file(`${ZOD_DIR}/${group.id}.ts`, emitted.text));
    files.push(
      file(
        `${ZOD_DIR}/${group.id}.map.json`,
        `${canonicalJson2(emitted.map)}
`,
        emitted.map
      )
    );
  }
  files.push(file(`${ZOD_DIR}/index.ts`, barrelText(groups)));
  files.sort(
    (left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0
  );
  return files;
}
function runtimeText(adapterVersion) {
  return `// Generated by ${ADAPTER_ID2}@${adapterVersion} (zod-schema-generator).
// Adapter-owned runtime helpers shared by every generated module. Content
// depends only on the adapter version, so this file is itself a
// determinism probe: any byte drift means a stale artifact. Do not edit;
// regenerate with \`lekalo generate\`.
import * as z from "zod";

/** A calendar date string (YYYY-MM-DD): the JSON-faithful date form. */
export const LekaloDateString = z.string().regex(/^\\d{4}-\\d{2}-\\d{2}$/);

/**
 * Brand one schema with its Lekalo semantic id: \`z.infer\` yields
 * \`<base> & z.BRAND<"module.name">\`, so raw values cannot masquerade as
 * opaque ids \u2014 they must pass \`.parse\`. The brand applies over the
 * scalar's own declared base schema (uuid, string, number, date, \u2026);
 * branding never tightens validation beyond the base. The two-argument
 * form keeps the emitted files plain-JS executable.
 *
 * @template {{ safeParse: Function }} T
 * @param {T} schema
 * @param {string} semanticId
 * @returns {T}
 */
export function lekaloBrand(schema, semanticId) {
  return schema.brand(semanticId);
}

/**
 * Map raw zod issues to Lekalo semantic ids through the field map of the
 * sibling \`<module>.map.json\` sidecar. Exact field paths win; otherwise
 * the closest enclosing path wins; otherwise the module owner. Unknown
 * input paths are attributed, never dropped. The issue parameter is the
 * structural shape every zod issue satisfies, so plain JavaScript
 * consumers can call this helper without importing zod.
 *
 * @param {{ path: (string | number)[], code: string }[]} issues
 * @param {Record<string, string>} fields
 * @param {string} owner
 * @returns {{ path: string, semanticId: string, code: string }[]}
 */
export function normalizeIssues(issues, fields, owner) {
  return issues.map((issue) => {
    const path = issue.path.join(".");
    return {
      path,
      semanticId: resolveFieldOwner(fields, path, owner),
      code: issue.code,
    };
  });
}

/**
 * Exact path first, then the closest enclosing mapped path, then the
 * mapped root symbol (the \`""\` entry), and only then the owner
 * argument \u2014 one fallback chain, coherent with the sidecar bytes.
 *
 * @param {Record<string, string>} fields
 * @param {string} path
 * @param {string} owner
 * @returns {string}
 */
function resolveFieldOwner(fields, path, owner) {
  if (Object.prototype.hasOwnProperty.call(fields, path)) {
    return fields[path];
  }
  let prefix = path;
  for (;;) {
    const cut = prefix.lastIndexOf(".");
    if (cut <= 0) break;
    prefix = prefix.slice(0, cut);
    if (Object.prototype.hasOwnProperty.call(fields, prefix)) {
      return fields[prefix];
    }
  }
  if (Object.prototype.hasOwnProperty.call(fields, "")) {
    return fields[""];
  }
  return owner;
}
`;
}
function barrelText(groups) {
  const lines = [
    `// Generated by barrel emission (issue #45). One stable import root for`,
    `// every generated Zod group; entries are sorted and the set changes`,
    `// only when the set of schema-bearing modules changes.`
  ];
  const entries = ["runtime", ...groups.map((group) => group.id)].sort();
  for (const entry of entries) {
    lines.push(`export * from "./${entry}";`);
  }
  return `${lines.join("\n")}
`;
}
function emissionGroups(modules) {
  const ids = modules.map((module) => module.id);
  const byId = new Map(modules.map((module) => [module.id, module]));
  const neighbors = new Map(ids.map((id) => [id, /* @__PURE__ */ new Set()]));
  for (const module of modules) {
    for (const entry of module.imports) {
      if (!byId.has(entry.module)) continue;
      neighbors.get(module.id).add(entry.module);
      neighbors.get(entry.module).add(module.id);
    }
  }
  const visited = /* @__PURE__ */ new Set();
  const groups = [];
  for (const id of ids) {
    if (visited.has(id)) continue;
    const component = [];
    const queue = [id];
    visited.add(id);
    while (queue.length > 0) {
      const current = queue.shift();
      component.push(current);
      for (const next of [...neighbors.get(current)].sort()) {
        if (!visited.has(next)) {
          visited.add(next);
          queue.push(next);
        }
      }
    }
    component.sort();
    const members = component.map((member) => byId.get(member));
    groups.push({
      id: component[0],
      modules: members,
      declarations: members.flatMap((member) => member.declarations),
      imports: members.flatMap((member) => member.imports),
      // First-wins merge in the members' (sorted) order: a colliding
      // field path keeps the first deterministic owner instead of
      // silently moving to the last writer (issue #45 review F-2).
      fields: members.reduce((merged, member) => {
        for (const key of Object.keys(member.fields)) {
          if (!Object.hasOwn(merged, key)) {
            merged[key] = member.fields[key];
          }
        }
        return merged;
      }, {})
    });
  }
  return groups;
}
function emitModule(module, context) {
  const header = [
    `// Generated by ${ADAPTER_ID2}@${context.adapterVersion}`,
    `// (zod-schema-generator) from ${context.irIdentity} input ${context.inputDigest}.`,
    `// Do not edit: regenerate with \`lekalo generate\`. Presence (required)`,
    `// and nullability (optional wrapper) are orthogonal axes here:`,
    `// \`required\` governs key presence, the optional wrapper emits`,
    `// \`.nullable()\`. Closed objects mirror the closed model (.strict()).`
  ];
  const imports = [
    `import * as z from "zod";`,
    ...collectRuntimeImports(module),
    ...module.imports.map(
      (entry) => `import { ${entry.names.join(", ")} } from "./${entry.module}";`
    )
  ];
  const body = [];
  const declarations = [];
  let cursor = byteLength(`${header.join("\n")}

${imports.join("\n")}

`);
  for (const declaration of orderDeclarations(module.declarations)) {
    const text2 = renderDeclaration(declaration);
    const start = cursor;
    const end = start + byteLength(text2);
    declarations.push({
      id: declaration.semanticId,
      export: declaration.exportName,
      start,
      end
    });
    body.push(text2);
    cursor = end + 1;
  }
  const text = `${[...header, "", ...imports, "", ...body].join("\n")}
`;
  const owner = Object.hasOwn(module.fields, "") ? module.fields[""] : module.modules[0].id;
  return {
    text,
    map: {
      contract: MAP_CONTRACT,
      adapter: { id: ADAPTER_ID2, version: context.adapterVersion },
      owner,
      fields: module.fields,
      declarations
    }
  };
}
function groupImports(modules, ownIds, otherGroups) {
  const otherIds = new Set(otherGroups.flatMap((group) => group.modules.map((module) => module.id)));
  const otherExports = new Set(otherGroups.flatMap((group) => group.declarations.map((decl) => decl.exportName)));
  const byModule = /* @__PURE__ */ new Map();
  for (const module of modules) {
    for (const entry of module.imports) {
      if (ownIds.has(entry.module) || !otherIds.has(entry.module)) continue;
      const names = entry.names.filter((name) => otherExports.has(name));
      if (names.length === 0) continue;
      let bucket = byModule.get(entry.module);
      if (!bucket) {
        bucket = /* @__PURE__ */ new Set();
        byModule.set(entry.module, bucket);
      }
      for (const name of names) bucket.add(name);
    }
  }
  return [...byModule.keys()].sort().map((moduleId) => {
    const names = [...byModule.get(moduleId)].sort();
    return { module: moduleId, names };
  });
}
function collectRuntimeImports(module) {
  const used = /* @__PURE__ */ new Set();
  const visit = (expr) => {
    if (!expr || typeof expr !== "object") return;
    if (expr.k === "dateString") {
      used.add("LekaloDateString");
      return;
    }
    if (expr.k === "brand") {
      used.add("lekaloBrand");
      visit(expr.inner);
      return;
    }
    if (expr.k === "array") {
      visit(expr.item);
      return;
    }
    if (expr.k === "object") {
      for (const field of expr.fields) visit(field.expr);
      return;
    }
    if (expr.k === "nullable" || expr.k === "optional" || expr.k === "brand") {
      visit(expr.inner);
    }
  };
  for (const declaration of module.declarations) visit(declaration.expr);
  return used.size > 0 ? [`import { ${[...used].sort().join(", ")} } from "./runtime";`] : [];
}
function orderDeclarations(declarations) {
  const byExport = new Map(
    declarations.map((declaration) => [declaration.exportName, declaration])
  );
  const dependencies = new Map(
    declarations.map((declaration) => [
      declaration.exportName,
      intraModuleDeps(declaration, byExport)
    ])
  );
  const ordered = [];
  const emitted = /* @__PURE__ */ new Set();
  const visiting = /* @__PURE__ */ new Set();
  const visit = (declaration) => {
    if (emitted.has(declaration.exportName)) return;
    if (visiting.has(declaration.exportName)) return;
    visiting.add(declaration.exportName);
    for (const dependency of dependencies.get(declaration.exportName)) {
      visit(byExport.get(dependency));
    }
    visiting.delete(declaration.exportName);
    emitted.add(declaration.exportName);
    ordered.push(declaration);
  };
  for (const declaration of declarations) visit(declaration);
  return ordered;
}
function intraModuleDeps(declaration, byExport) {
  const deps = /* @__PURE__ */ new Set();
  const visit = (expr) => {
    if (!expr || typeof expr !== "object") return;
    if (expr.k === "ref") {
      if (byExport.has(expr.name)) {
        deps.add(expr.name);
      }
      return;
    }
    if (expr.k === "array") {
      visit(expr.item);
      return;
    }
    if (expr.k === "object") {
      for (const field of expr.fields) visit(field.expr);
      return;
    }
    if (expr.k === "nullable" || expr.k === "optional" || expr.k === "brand") {
      visit(expr.inner);
    }
  };
  visit(declaration.expr);
  return [...deps].sort();
}
function renderDeclaration(declaration) {
  const schema = renderExpr(declaration.expr, "");
  const lines = [
    `export const ${declaration.exportName} = ${schema};`,
    `export type ${declaration.typeName} = z.infer<typeof ${declaration.exportName}>;`
  ];
  return lines.join("\n");
}
function renderExpr(expr, indent) {
  const inner = indentUnit(indent);
  switch (expr.k) {
    case "string":
      return `z.string()`;
    case "number":
      return `z.number().finite()`;
    case "boolean":
      return `z.boolean()`;
    case "dateString":
      return `LekaloDateString`;
    case "dateNative":
      return `z.date()`;
    case "datetime":
      return `z.string().datetime({ offset: true })`;
    case "uuid":
      return `z.string().uuid()`;
    case "uri":
      return `z.string().url()`;
    case "enum":
      return `z.enum([${expr.values.map((value) => JSON.stringify(value)).join(", ")}])`;
    case "brand":
      return `lekaloBrand(${renderExpr(expr.inner, indent)}, ${JSON.stringify(expr.brand)})`;
    case "ref":
      return expr.name;
    case "array":
      return `z.array(${renderExpr(expr.item, indent)})`;
    case "nullable":
      return `${renderExpr(expr.inner, indent)}.nullable()`;
    case "optional":
      return `${renderExpr(expr.inner, indent)}.optional()`;
    case "object": {
      if (expr.fields.length === 0) {
        return expr.strict ? `z.object({}).strict()` : `z.object({}).strip()`;
      }
      const body = expr.fields.map(
        (field) => `${inner}  ${JSON.stringify(field.name)}: ${renderExpr(field.expr, `${inner}  `)},`
      ).join("\n");
      return `z.object({
${body}
${inner}})${expr.strict ? ".strict()" : ".strip()"}`;
    }
    default:
      throw new TypeError(`unrenderable expression kind ${expr?.k}`);
  }
}
function indentUnit(indent) {
  return indent;
}
function byteLength(text) {
  return Buffer.byteLength(text, "utf8");
}

// src/zod-policy.mjs
var POLICY_PATH = "lekalo/targets/node-typescript.yaml";
var MAX_POLICY_BYTES = 16 * 1024;
function resolvePolicy(text) {
  if (text === null || text === void 0) {
    return { policy: { ...DEFAULT_POLICY }, source: "defaults" };
  }
  if (typeof text !== "string") {
    return { refusal: "not-text" };
  }
  const bytes = Buffer.byteLength(text, "utf8");
  if (bytes > MAX_POLICY_BYTES) {
    return { refusal: "overbound" };
  }
  const parsed = parsePolicyYaml(text);
  if (parsed.refusal) {
    return { refusal: parsed.refusal };
  }
  return { policy: parsed.policy, source: "document" };
}
function parsePolicyYaml(text) {
  const lines = text.split(/\r?\n/);
  let inZod = false;
  const seen = /* @__PURE__ */ new Set();
  const policy = {};
  for (let index = 0; index < lines.length; index += 1) {
    const raw = lines[index];
    const stripped = raw.replace(/(^|\s)#.*$/, "");
    if (stripped.trim() === "") {
      continue;
    }
    if (stripped.includes("	")) {
      return { refusal: "tab-indentation" };
    }
    if (!stripped.startsWith(" ") && !stripped.startsWith("-")) {
      const match = stripped.match(/^([a-z][a-z0-9_-]*):\s*$/);
      if (!match) {
        return { refusal: "top-level-key" };
      }
      if (match[1] !== "zod") {
        return { refusal: "unknown-section" };
      }
      if (inZod) {
        return { refusal: "duplicate-section" };
      }
      inZod = true;
      continue;
    }
    if (!inZod) {
      return { refusal: "orphan-key" };
    }
    const keyMatch = stripped.match(/^ {2}([a-z][a-z0-9_-]*):\s*(\S.*)?$/);
    if (!keyMatch || stripped.startsWith("    ")) {
      return { refusal: "key-shape" };
    }
    const key = keyMatch[1];
    const value = keyMatch[2]?.trim();
    if (seen.has(key)) {
      return { refusal: "duplicate-key" };
    }
    seen.add(key);
    if (key === "date") {
      if (!DATE_POLICIES.includes(value)) {
        return { refusal: "date-value" };
      }
      policy.date = value;
      continue;
    }
    if (key === "unknown-keys") {
      if (!UNKNOWN_KEY_POLICIES.includes(value)) {
        return { refusal: "unknown-keys-value" };
      }
      policy.unknownKeys = value;
      continue;
    }
    return { refusal: "unknown-key" };
  }
  return {
    policy: { ...DEFAULT_POLICY, ...policy }
  };
}

// src/zod-gen.mjs
var ZOD_EXTENSION_VERSION = "0.3.2";
var ZOD_WRITE_SCOPES = [`${ZOD_DIR}/**`];
var DRIFT = "zod.drift";
var descriptor = {
  id: "zod-schema-generator",
  version: ZOD_EXTENSION_VERSION,
  operations: ["generate", "verify"],
  namedCapabilities: { "generate.zod": "full" },
  acceptedIrVersions: ["0.2.16"],
  writeScopes: ZOD_WRITE_SCOPES,
  invoke: (context) => zodOperation(context)
};
function zodOperation(context) {
  const { operation, request, readView } = context;
  try {
    const policy = resolvePolicyFromContext(readView);
    if (policy.refusal) {
      return {
        state: "failed",
        diagnostics: [{ reason: `policy-${policy.refusal}` }]
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
        diagnostics: [{ reason: "ir-version-unsupported" }]
      };
    }
    const mapped = mapProject(ir.document, policy.policy);
    const files = emitFiles({
      modules: mapped.modules,
      inputDigest,
      adapterVersion: ZOD_EXTENSION_VERSION,
      irIdentity: IR_IDENTITY
    });
    const byteFiles = files.map((emitted) => ({
      path: emitted.path,
      bytes: Buffer.from(emitted.text, "utf8")
    }));
    if (operation === "generate") {
      return generateOperation(context, byteFiles, mapped.findings);
    }
    if (operation === "verify") {
      return verifyOperation(context, byteFiles, mapped.findings);
    }
    return { state: "unsupported" };
  } catch (error) {
    throw new Error(`zod-schema-generator: ${bounded(error?.message)}`);
  }
}
function planIdOf(writes) {
  return "plan-" + sha256(canonicalJson2(writes)).slice("sha256:".length);
}
function generateOperation(context, byteFiles, findings) {
  const { request, writeView } = context;
  if (findings.length > 0) {
    return {
      state: "complete",
      data: { writes: [], findings }
    };
  }
  const writes = [];
  const bodies = /* @__PURE__ */ new Map();
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
    data: { writes, findings: [], plan_id: planIdOf(writes) }
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
        detail: "unreadable-or-missing"
      });
      continue;
    }
    const observed = readView.readFile(emitted.path);
    if (!observed.equals(emitted.bytes)) {
      verification.push({
        path: emitted.path,
        code: DRIFT,
        detail: `expected:${sha256Bytes(emitted.bytes).slice(7, 19)} observed:${sha256Bytes(observed).slice(7, 19)}`
      });
    }
  }
  const all = [...findings, ...verification];
  return { state: "complete", data: { writes: [], findings: all } };
}
function resolvePolicyFromContext(readView) {
  if (!readView.canRead(POLICY_PATH)) {
    return { policy: DEFAULT_POLICY, source: "absent" };
  }
  const bytes = readView.readFile(POLICY_PATH);
  return resolvePolicy(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
}
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
  if (document === null || typeof document !== "object" || !Array.isArray(document.definitions)) {
    return { refusal: "ir-shape" };
  }
  return { document, text };
}
function sha256Bytes(bytes) {
  return "sha256:" + createHash3("sha256").update(bytes).digest("hex");
}
function byPath(left, right) {
  return left.path < right.path ? -1 : left.path > right.path ? 1 : 0;
}
function bounded(text) {
  return String(text ?? "unknown").replace(/[^a-zA-Z0-9._: -]+/g, "?").slice(0, 128);
}

// src/main.mjs
__setLaunchExtensions([descriptor]);
var __lekaloKernel = kernel_exports;
var __lekaloAdapterIdentity = { id: "lekalo-target-node-typescript", version: "0.3.2", digest: entryDigest() };
await runIfEntry(import.meta.url);
export {
  __lekaloAdapterIdentity,
  __lekaloKernel
};

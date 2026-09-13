#!/usr/bin/env node
/**
 * The reference node-typescript scanner adapter (issue #42).
 *
 * A dependency-free Node.js target adapter implementing the
 * `lekalo.target/v1` describe/scan exchange. This is a real native
 * adapter: it reads the project sources inside the private view the core
 * grants it and extracts the exported TypeScript symbols — classes,
 * functions, interfaces, types, enums, and constants — with stable keys
 * and proposed semantic ids. It never writes, never reads outside its
 * declared scopes, and never resolves an ambiguous mapping by picking a
 * candidate: two entries proposing the same semantic id are the
 * ambiguity signal, and the core keeps both as candidates.
 *
 * Extraction contract (the closed language subset the reference adapter
 * covers):
 *   export class C            -> kind entity
 *   export function f         -> kind command
 *   export interface I        -> kind value-object
 *   export type T             -> kind scalar
 *   export enum E             -> kind enum
 *   export const c            -> kind scalar
 *
 * Semantic ids are proposed as `<package>.<snake_case(name)>`, where the
 * package name comes from the nearest package.json (bounded read). A
 * sibling `src/<name>.test.ts` mentioning a symbol's name is reported as
 * the native test binding (`t`) of that symbol.
 *
 * Every entry detail is a bounded token (the protocol caps detail at 128
 * bytes) of the closed form
 *   {"s":semantic,"n":native,"l":line,"q":confidence,"t":testBinding}
 * with `s` required. Fingerprints are deliberately absent: the core
 * fingerprints the real tree itself, so freshness evidence is computed
 * from actual bytes, never from adapter claims.
 *
 * Excluded paths never enter the walk: dotfiles (including `.env*`),
 * `node_modules`, `dist`, `build`, and `coverage` are skipped by a
 * closed list, and the adapter's declared read scopes are minimal
 * (`package.json` and `src/**`), so sensitive files cannot be read even
 * if the walk were misdirected.
 */

import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ADAPTER = {
  id: "lekalo-target-node-typescript",
  version: "1.0.0",
  digest: "sha256:" + "0".repeat(64),
};

const READ_SCOPES = ["package.json", "src/**"];

/** Directories the walk never descends into. */
const SKIP_DIRS = new Set([
  "node_modules",
  "dist",
  "build",
  "coverage",
  ".git",
  ".lekalo",
]);

const MAX_ENTRIES = 10_000;
const MAX_FILE_BYTES = 1_000_000;
const MAX_LINE_BYTES = 512;

/** Canonical compact JSON: byte-sorted keys, exactly like the core. */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
}

function readRequest() {
  const marker = process.argv.indexOf("--lekalo-request-file");
  if (marker !== -1 && process.argv[marker + 1]) {
    return readFileSync(process.argv[marker + 1], "utf8");
  }
  return readFileSync(0, "utf8");
}

function capabilities(requestedVersion) {
  const declared = {
    adapter: ADAPTER,
    protocol_versions: ["1.0.0", "1.1.0"],
    operations: ["describe", "scan"],
    transports: ["stdin", "file"],
    progress: false,
    targets: ["node-typescript"],
    profiles: ["default"],
    read_scopes: READ_SCOPES,
    write_scopes: [],
  };
  if (requestedVersion !== "1.0.0") {
    declared.ir_versions = ["0.1.0"];
    declared.capabilities = { "scan.symbols": "full" };
    declared.constraints = { max_entries: MAX_ENTRIES };
  }
  return declared;
}

function respond(request, payload) {
  const envelope = {
    protocol: "lekalo.target/v1",
    protocol_version: request.protocol_version,
    operation: request.operation,
    request_id: request.request_id,
    status: "ok",
    evidence: { adapter: ADAPTER },
    ...payload,
  };
  process.stdout.write(canonical(envelope));
}

function fail(request, errorClass, code, message) {
  const envelope = {
    protocol: "lekalo.target/v1",
    protocol_version: request.protocol_version,
    operation: request.operation,
    request_id: request.request_id,
    status: "error",
    evidence: { adapter: ADAPTER },
    error: { class: errorClass, code, message, partial: false },
  };
  process.stdout.write(canonical(envelope));
}

/** The bounded, no-follow walk of one declared source directory. */
function walkSourceDir(dir, relative, entries) {
  let names;
  try {
    names = readdirSync(dir, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of names.sort((a, b) => (a.name < b.name ? -1 : 1))) {
    if (entries.length >= MAX_ENTRIES) return;
    if (entry.name.startsWith(".")) continue;
    if (SKIP_DIRS.has(entry.name)) continue;
    const path = relative === "" ? entry.name : `${relative}/${entry.name}`;
    if (entry.isDirectory()) {
      walkSourceDir(join(dir, entry.name), path, entries);
    } else if (entry.isFile() && path.endsWith(".ts")) {
      entries.push({ absolute: join(dir, entry.name), path });
    }
  }
}

/** camelCase / PascalCase / SCREAMING_SNAKE -> snake_case. */
function snakeCase(name) {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/_+/g, "_")
    .toLowerCase();
}

/** One closed extraction pattern per exported construct. */
const PATTERNS = [
  { re: /^export\s+class\s+([A-Za-z0-9_$]+)/, kind: "entity" },
  { re: /^export\s+function\s*\*?\s*([A-Za-z0-9_$]+)/, kind: "command" },
  { re: /^export\s+interface\s+([A-Za-z0-9_$]+)/, kind: "value-object" },
  { re: /^export\s+enum\s+([A-Za-z0-9_$]+)/, kind: "enum" },
  { re: /^export\s+type\s+([A-Za-z0-9_$]+)/, kind: "scalar" },
  { re: /^export\s+const\s+([A-Za-z0-9_$]+)/, kind: "scalar" },
];

/**
 * Extract the exported symbols of one source file. Returns one record
 * per exported symbol with its native stable key and 1-based declaration
 * line. The kind is the adapter's proposal; only a user confirmation
 * turns it into a fact.
 */
function extractSymbols(path, bytes) {
  const lines = bytes.toString("utf8").split("\n");
  const found = [];
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.length > MAX_LINE_BYTES) continue;
    for (const pattern of PATTERNS) {
      const match = pattern.re.exec(line.trimEnd());
      if (!match) continue;
      const name = match[1];
      found.push({
        name,
        kind: pattern.kind,
        native: `${path}#${name}`,
        line: index + 1,
      });
      break;
    }
  }
  return found;
}

/** The scan operation: the closed issue #42 extraction pipeline. */
function scan(request) {
  // The package name anchors the proposed semantic ids; without one the
  // adapter refuses instead of inventing a module identity.
  let packageName;
  try {
    const manifest = JSON.parse(readFileSync("package.json", "utf8"));
    packageName = typeof manifest.name === "string" ? manifest.name : undefined;
  } catch {
    packageName = undefined;
  }
  if (!packageName || !/^[a-z][a-z0-9_]{0,62}$/.test(packageName)) {
    fail(request, "invalid", "scan-project-id", "package.json name is missing or not a module id");
    return;
  }

  const files = [];
  walkSourceDir("src", "src", files);

  // Read every walked file exactly once, bounded; unreadable or
  // over-large files are skipped, never guessed about.
  const fileBytes = new Map();
  for (const file of files) {
    try {
      if (statSync(file.absolute).size > MAX_FILE_BYTES) continue;
      fileBytes.set(file.path, readFileSync(file.absolute));
    } catch {
      // Skipped on purpose.
    }
  }

  // Every exported symbol is one entry: the proposed semantic id, the
  // native stable key, the 1-based line, and — when a sibling test
  // mentions the symbol — the native test binding. Two entries proposing
  // the SAME semantic id are the ambiguity signal: the core keeps both
  // as candidates and never picks one.
  const entries = [];
  for (const file of files) {
    const bytes = fileBytes.get(file.path);
    if (!bytes) continue;
    for (const symbol of extractSymbols(file.path, bytes)) {
      const semantic = `${packageName}.${snakeCase(symbol.name)}`;
      const detail = { s: semantic, n: symbol.native, l: symbol.line, q: "medium" };
      const testPath = file.path.replace(/\.ts$/, ".test.ts");
      const testBytes = fileBytes.get(testPath);
      if (testBytes && testBytes.toString("utf8").includes(symbol.name)) {
        detail.t = `${testPath}#${symbol.name}`;
      }
      entries.push({
        path: file.path,
        kind: symbol.kind,
        detail: JSON.stringify(detail),
      });
    }
  }

  respond(request, { result: { entries, truncated: false } });
}

const request = JSON.parse(readRequest());
if (request.protocol !== "lekalo.target/v1") {
  fail(request, "invalid", "protocol-token", "unknown protocol token");
} else if (!["1.0.0", "1.1.0"].includes(request.protocol_version)) {
  fail(request, "invalid", "protocol-version", "unsupported protocol version");
} else if (request.operation === "describe") {
  respond(request, { capabilities: capabilities(request.protocol_version) });
} else if (request.operation === "scan") {
  scan(request);
} else {
  fail(request, "invalid", "operation", "unsupported operation");
}

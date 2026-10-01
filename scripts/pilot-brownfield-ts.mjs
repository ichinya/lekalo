#!/usr/bin/env node
/**
 * Issue #118 brownfield-pilot harness: connect one existing private
 * TypeScript consumer in observed mode, end to end, without mutating
 * it. The consumer is copied into a disposable working directory
 * (never scanned in place unless `--in-place` is passed explicitly —
 * the shipped pilot run does not use it); the copy is adopted,
 * scanned, bound, projected, and stress-tested with one controlled
 * change, and the run answers with privacy-safe aggregate metrics and
 * a findings report.
 *
 *   node scripts/pilot-brownfield-ts.mjs --consumer <abs-path> \
 *     [--workdir <dir>] [--lekalo <binary>] [--out <report-dir>] \
 *     [--keep] [--in-place] [--bind <file>:<export>[:<semanticId>]] \
 *     [--budget <tokens>] [--disposition public-fixture|unconfirmed]
 *
 * Dependency-free; run from the repo root.
 *
 * Privacy contract of the emitted artifacts (`metrics.json`,
 * `report.md`): counts, sizes, durations, digests, and statuses only.
 * No absolute paths, no file names, no identifiers, no source text,
 * no test names. The consumer is named only as `consumer` plus the
 * sha256 of its canonical path. The scan document and the observed
 * index stay inside the working directory (the private side) and are
 * deleted with it unless `--keep` is passed.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync, existsSync, mkdirSync, readFileSync, readdirSync, realpathSync,
  rmSync, statSync, writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const adapterPath = join(repoRoot, "adapters", "node-typescript", "adapter.mjs");

// ---------------------------------------------------------------------------
// argument parsing
// ---------------------------------------------------------------------------

function parseArgs(argv) {
  const args = {
    consumer: null, workdir: null, lekalo: null, out: null,
    keep: false, inPlace: false,
    bind: null, budget: 4096, disposition: "unconfirmed",
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const value = () => {
      if (argv[index + 1] === undefined) fail(`missing value for ${arg}`);
      return argv[++index];
    };
    if (arg === "--consumer") args.consumer = value();
    else if (arg === "--workdir") args.workdir = value();
    else if (arg === "--lekalo") args.lekalo = value();
    else if (arg === "--out") args.out = value();
    else if (arg === "--keep") args.keep = true;
    else if (arg === "--in-place") args.inPlace = true;
    else if (arg === "--bind") args.bind = value();
    else if (arg === "--budget") args.budget = Number(value());
    else if (arg === "--disposition") args.disposition = value();
    else fail(`unknown argument ${arg}`);
  }
  if (args.consumer === null) fail("--consumer <abs-path> is required");
  if (args.disposition !== "public-fixture" && args.disposition !== "unconfirmed") {
    fail("--disposition must be public-fixture or unconfirmed");
  }
  args.bind ??= "packages/api/src/repositories/tasks.ts:createTask:task.submit";
  return args;
}

function fail(message) {
  process.stderr.write(`pilot-brownfield-ts: ${message}\n`);
  process.exit(2);
}

const args = parseArgs(process.argv.slice(2));
const bindTarget = (() => {
  const [file = "", exportName = "", semanticId = "task.submit"] = args.bind.split(":");
  if (file === "" || exportName === "") fail("--bind must be <file>:<export>[:<semanticId>]");
  return { file, exportName, semanticId };
})();

// ---------------------------------------------------------------------------
// small utilities
// ---------------------------------------------------------------------------

const sha = (bytes) => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
const sha256Hex = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** Canonical JSON text (sorted keys, compact) — the digest spelling the
 * committed generator scripts use. */
function canonicalText(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.map(canonicalText).join(",")}]`;
  if (typeof value === "object") {
    return `{${Object.keys(value).sort()
      .map((key) => `${JSON.stringify(key)}:${canonicalText(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

/** Full recursive inventory of one directory: relative POSIX path → sha256. */
function inventory(directory) {
  const map = new Map();
  const walk = (dir, prefix) => {
    for (const entry of readdirSync(dir).sort()) {
      const absolute = join(dir, entry);
      const relativePath = prefix === "" ? entry : `${prefix}/${entry}`;
      const metadata = statSync(absolute);
      if (metadata.isSymbolicLink()) {
        throw new Error(`symlink inside the tree: ${relativePath}`);
      }
      if (metadata.isDirectory()) walk(absolute, relativePath);
      else if (metadata.isFile()) {
        map.set(relativePath.split("\\").join("/"), sha256Hex(readFileSync(absolute)));
      }
    }
  };
  walk(directory, "");
  return map;
}

function inventoryDiffers(before, after) {
  const beforeKeys = [...before.keys()].sort().join("\n");
  const afterKeys = [...after.keys()].sort().join("\n");
  if (beforeKeys !== afterKeys) return true;
  for (const [path, digest] of before) {
    if (after.get(path) !== digest) return true;
  }
  return false;
}

/** The directories a copy never descends into (.gitignore-lite). */
const SKIP_DIRS = new Set(["node_modules", ".git", "dist", "coverage"]);

function copyConsumer(source, target, skipCounts) {
  mkdirSync(dirname(target), { recursive: true });
  cpSync(source, target, {
    recursive: true,
    filter: (src) => {
      const name = src.split(/[\\/]/).pop();
      if (SKIP_DIRS.has(name) && statSync(src).isDirectory()) {
        skipCounts[name] = (skipCounts[name] ?? 0) + 1;
        return false;
      }
      return true;
    },
  });
}

// ---------------------------------------------------------------------------
// lekalo CLI driver
// ---------------------------------------------------------------------------

function lekaloBinary() {
  if (args.lekalo !== null) return resolve(args.lekalo);
  const exe = process.platform === "win32" ? "lekalo.exe" : "lekalo";
  const candidate = join(repoRoot, "target", "debug", exe);
  if (!existsSync(candidate)) {
    const build = spawnSync("cargo", ["build", "-p", "lekalo-cli"], {
      cwd: repoRoot, encoding: "utf8", timeout: 900_000,
    });
    if (build.status !== 0 || !existsSync(candidate)) {
      fail(`no lekalo binary at target/debug and cargo build failed: ${build.stderr?.slice(-400)}`);
    }
  }
  return candidate;
}

const lekalo = lekaloBinary();

/** Run the lekalo CLI inside the consumer copy. Returns the parsed
 * `--json` envelope (when stdout is one), the raw stdout, the exit
 * code, and a bounded stderr tail. */
function runLekalo(cwd, jsonArgs, { timeoutMs = 600_000 } = {}) {
  const argv = jsonArgs.includes("--json") ? jsonArgs : ["--json", ...jsonArgs];
  const started = Date.now();
  const child = spawnSync(lekalo, argv, {
    cwd, encoding: "utf8", timeout: timeoutMs, maxBuffer: 256 * 1024 * 1024,
  });
  const durationMs = Date.now() - started;
  const exitCode = child.status ?? -1;
  const stdout = child.stdout ?? "";
  const stderr = child.stderr ?? "";
  // The CLI renders valid receipts on stdout and refusal envelopes on
  // stderr; parse either.
  let envelope = null;
  for (const text of [stdout, stderr]) {
    try {
      envelope = JSON.parse(text.trim());
      break;
    } catch {
      const first = text.indexOf("{");
      const last = text.lastIndexOf("}");
      if (first !== -1 && last > first) {
        try {
          envelope = JSON.parse(text.slice(first, last + 1));
          break;
        } catch { envelope = null; }
      }
    }
  }
  return { exitCode, envelope, stdout, durationMs, stderrTail: stderr.slice(-2000) };
}

// ---------------------------------------------------------------------------
// framework policy and launch profile
// ---------------------------------------------------------------------------

const FRAMEWORK_POLICY = {
  schema: "lekalo/framework-policy",
  version: 1,
  providers: [{ id: "hono", state: "enabled" }],
};

const adapter = await import(`file:///${adapterPath.split("\\").join("/")}`);
const kernel = adapter.__lekaloKernel;

function workspacePackageDirs(copyRoot) {
  const dirs = [];
  const rootPackagePath = join(copyRoot, "package.json");
  if (existsSync(rootPackagePath)) {
    const rootPackage = JSON.parse(readFileSync(rootPackagePath, "utf8"));
    for (const glob of rootPackage.workspaces ?? []) {
      const base = glob.replace(/\*+.*$/, "").replace(/\/$/, "");
      const parent = join(copyRoot, base);
      if (!existsSync(parent)) continue;
      for (const entry of readdirSync(parent).sort()) {
        if (existsSync(join(parent, entry, "package.json"))) {
          dirs.push(base === "" ? entry : `${base}/${entry}`);
        }
      }
    }
  }
  return dirs;
}

function buildProfile(copyRoot) {
  const readRoots = [];
  const declareTree = (path) => {
    if (existsSync(join(copyRoot, path))) readRoots.push({ kind: "tree", path });
  };
  const declareFile = (path) => {
    if (existsSync(join(copyRoot, path))) readRoots.push({ kind: "file", path });
  };
  declareFile("package.json");
  declareFile("tsconfig.json");
  declareFile("drizzle.bindings.json");
  const packages = workspacePackageDirs(copyRoot);
  for (const pkg of packages) {
    declareTree(`${pkg}/src`);
    declareTree(`${pkg}/types`);
    declareFile(`${pkg}/package.json`);
    declareFile(`${pkg}/tsconfig.json`);
  }
  // The read-view grammar refuses entries the portable path grammar
  // cannot spell (uppercase- or underscore-leading segments, e.g. the
  // vitest `__tests__` convention). Excluding them keeps the scan
  // alive; the count is honest pilot data.
  const grammarExcluded = [];
  const coveredByRoot = (relativePath) => readRoots.some((root) => {
    if (root.kind !== "tree") return false;
    const scope = `${root.path}/**`;
    return kernel.scopeCovers(scope, relativePath) || root.path === relativePath;
  });
  const walk = (dir, prefix) => {
    for (const entry of readdirSync(dir).sort()) {
      const relativePath = prefix === "" ? entry : `${prefix}/${entry}`;
      const absolute = join(dir, entry);
      const metadata = statSync(absolute);
      if (metadata.isSymbolicLink()) continue;
      if (metadata.isDirectory()) {
        if (!kernel.isLogicalPath(relativePath) || !coveredByRoot(relativePath)) {
          if (coveredByRoot(relativePath)) {
            grammarExcluded.push(relativePath);
            readRoots.push({ kind: "file", path: relativePath });
          }
          continue;
        }
        walk(absolute, relativePath);
      } else if (metadata.isFile() && /\.(ts|tsx|mts|cts|json)$/.test(entry)) {
        if (coveredByRoot(relativePath) && !kernel.isLogicalPath(relativePath)) {
          grammarExcluded.push(relativePath);
          readRoots.push({ kind: "file", path: relativePath });
        }
      }
    }
  };
  walk(copyRoot, "");
  // Exact-entry exclusions: a declared file root that fails the
  // portable grammar reads as an exclusion of that entry (the walk
  // refuses to spell it), keeping the rest of the tree scannable.
  const exclusions = grammarExcluded;
  const seen = new Set();
  const roots = readRoots.filter((root) => {
    const key = `${root.kind}:${root.path}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
  if (roots.length === 0) fail("the profile derivation found no readable roots");
  return {
    profile: {
      id: "standalone",
      mode: "observed",
      target: "node-typescript",
      readRoots: roots,
      exclusions,
      provenance: {
        origin: "declared",
        revision: `pilot-${sha256Hex(Buffer.from(`${exclusions.length}`)).slice(0, 12)}`,
        disposition: args.disposition,
      },
    },
    packages,
    grammarExcludedCount: grammarExcluded.length,
  };
}

// ---------------------------------------------------------------------------
// observed-scan document assembly (the wire-fallback document builder)
// ---------------------------------------------------------------------------

const KIND_BY_FAMILY = {
  function: "command",
  class: "entity",
  interface: "value-object",
  enum: "enum",
  "type-alias": "scalar",
  property: "scalar",
  variable: "scalar",
};

function snakeCase(name) {
  return name
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/([A-Z]+)([A-Z][a-z])/g, "$1_$2")
    .replace(/[^a-zA-Z0-9]+/g, "_")
    .replace(/_+/g, "_")
    .toLowerCase();
}

function buildScanDocument({ index, identity, project, rename }) {
  const scopeOf = (modulePath) => {
    const pkg = index.packages.find((candidate) =>
      modulePath === candidate.root || modulePath.startsWith(`${candidate.root}/`));
    return (pkg?.name ?? "project")
      .replace(/^@/, "")
      .split(/[\\/._-]+/)
      .filter((part) => /^[a-z0-9]+$/i.test(part))
      .join("_")
      .toLowerCase() || "project";
  };
  const semanticOf = (symbol) => [scopeOf(symbol.module), ...symbol.qualifiedName.split(".").map(snakeCase)].join(".");
  const moduleAnchor = (modulePath) => {
    const suffix = modulePath.replace(/\.(ts|tsx|mts|cts|d\.ts|d\.mts|d\.cts)$/i, "").split("/").join(".");
    return `${scopeOf(modulePath)}.${suffix}`.slice(0, 192);
  };
  const fileFingerprint = (relativePath) => {
    try { return sha(readFileSync(join(copyRoot, relativePath))); } catch { return null; }
  };
  const byId = new Map();
  let renameMatches = 0;
  for (const symbol of index.symbols) {
    if (symbol.memberOf !== null) continue;
    let id = semanticOf(symbol);
    if (rename !== null && symbol.module === rename.file && symbol.qualifiedName === rename.exportName) {
      id = rename.semanticId;
      renameMatches += 1;
    }
    if (!byId.has(id)) byId.set(id, []);
    byId.get(id).push(symbol);
  }
  const symbols = [];
  const endpoints = [];
  const testBindings = [];
  for (const id of [...byId.keys()].sort()) {
    const group = byId.get(id);
    const candidates = group.map((symbol) => ({
      native: symbol.native,
      path: symbol.module,
      line: symbol.line,
      fingerprint: fileFingerprint(symbol.module),
      confidence: symbol.declarationOnly ? "low" : "medium",
    })).sort((left, right) => left.native.localeCompare(right.native));
    const first = group[0];
    const symbol = {
      id,
      kind: KIND_BY_FAMILY[first.family] ?? "scalar",
      mappingConfidence: first.declarationOnly ? "low" : "medium",
    };
    if (group.length === 1) {
      symbol.stableKey = first.native;
      symbol.location = { path: first.module, line: first.line };
      symbol.fingerprint = fileFingerprint(first.module);
    }
    const evidence = {};
    if (first.signature !== null) evidence.signature = first.signature;
    const references = index.references
      .filter((row) => row.from === first.module)
      .slice(0, 8)
      .map((row) => ({ target: moduleAnchor(row.to, index), role: row.role, confidence: row.confidence }));
    if (references.length > 0) evidence.references = references;
    if (Object.keys(evidence).length > 0) symbol.evidence = evidence;
    symbol.candidates = candidates;
    symbols.push(symbol);
  }
  // Native test bindings from the scan index, mirroring the wire's
  // `t` slot: a claim rides the first top-level symbol of its own
  // module; a test module with no scanned symbol has no carrier and
  // stays unrecorded here (the attach step binds the use case's own
  // tests explicitly instead).
  const symbolsByModule = new Map();
  for (const [id, group] of byId) {
    for (const symbol of group) {
      if (!symbolsByModule.has(symbol.module)) symbolsByModule.set(symbol.module, id);
    }
  }
  const seenTests = new Set();
  let carrierlessTests = 0;
  for (const test of index.tests) {
    const carrier = symbolsByModule.get(test.path);
    if (carrier === undefined) {
      carrierlessTests += 1;
      continue;
    }
    const id = `${test.path}#${test.name}`;
    if (seenTests.has(id)) continue;
    seenTests.add(id);
    testBindings.push({ id, symbol: carrier, path: test.path, fingerprint: fileFingerprint(test.path) });
  }
  // Endpoints from the enabled framework providers' route records: a
  // route whose handler is an indexed scanned symbol binds the route
  // to that symbol's semantic id; unindexed handlers stay unbound
  // (counted, never guessed). The first bound handler native feeds
  // the confirmation step.
  const nativeToSemantic = new Map();
  for (const [id, group] of byId) {
    for (const symbol of group) nativeToSemantic.set(symbol.native, id);
  }
  let unboundRoutes = 0;
  let boundHandlerNative = null;
  for (const providerId of FRAMEWORK_POLICY.providers.map((entry) => entry.id)) {
    const records = index.frameworks?.[providerId]?.records ?? [];
    for (const record of records) {
      if (record.relation !== `dev.lekalo.${providerId}/route-handler`) continue;
      const semantic = record.to?.indexed === true
        ? nativeToSemantic.get(record.to.native) ?? null
        : null;
      if (semantic === null) {
        unboundRoutes += 1;
        continue;
      }
      if (boundHandlerNative === null) boundHandlerNative = record.to.native;
      endpoints.push({
        id: `${scopeOf(record.from.module)}.route_${snakeCase(record.method)}_${snakeCase(record.path)}`,
        method: record.method,
        path: record.path,
        symbol: semantic,
      });
    }
  }
  // The scan revision binds the adapter identity, the profile digest,
  // and the input manifest — deterministic per tree and policy.
  const manifestKey = sha256Hex(Buffer.from(canonicalText(index.inputManifest), "utf8"));
  const revision = sha(Buffer.from(canonicalText([
    identity.id, identity.version, identity.digest, index.profileDigest, manifestKey,
  ]), "utf8"));
  const document = {
    schemaVersion: "lekalo/observed-scan/v0.2.16",
    adapter: { id: identity.id, version: identity.version, digest: identity.digest },
    project,
    revision,
    symbols,
    endpoints,
    target: "node-typescript",
    profile: "standalone",
  };
  if (testBindings.length > 0) document.testBindings = testBindings;
  return { document, revision, unboundRoutes, boundHandlerNative, renameMatches, carrierlessTests };
}

// ---------------------------------------------------------------------------
// run state
// ---------------------------------------------------------------------------

const steps = [];
function step(name, fn, { required = true } = {}) {
  const started = Date.now();
  const record = { step: name, status: "running", required, durationMs: 0 };
  steps.push(record);
  try {
    const detail = fn();
    record.status = "ok";
    record.durationMs = Date.now() - started;
    if (detail !== undefined) record.detail = detail;
    process.stdout.write(`ok   - ${name} (${record.durationMs}ms)\n`);
  } catch (error) {
    record.status = "failed";
    record.durationMs = Date.now() - started;
    record.error = String(error?.message ?? error).slice(0, 500);
    process.stdout.write(`FAIL - ${name}: ${record.error}\n`);
  }
  return record;
}

const consumerRoot = realpathSync(resolve(args.consumer));
const consumerPathDigest = sha(Buffer.from(consumerRoot.split("\\").join("/")));
const outDir = resolve(args.out ?? join(realpathSync(tmpdir()), "lekalo-pilot-brownfield-ts"));
const workdir = resolve(args.workdir ?? join(outDir, "work"));
const copyRoot = args.inPlace ? consumerRoot : join(workdir, "consumer-copy");

const metrics = {
  schema: "lekalo/pilot-brownfield-ts-metrics/v0.1.0",
  consumer: { pathDigest: consumerPathDigest, label: "consumer" },
  mode: args.inPlace ? "in-place" : "copy",
  disposition: args.disposition,
  steps: [],
  totals: { durationMs: 0, ok: 0, failed: 0 },
};

const findings = [];

const runStarted = Date.now();

// ---------------------------------------------------------------------------
// phase 1 — copy and adopt
// ---------------------------------------------------------------------------

let adopt = null;
let profileBundle = null;
let scanDoc = null;
let scanDocPath = null;

if (!args.inPlace) {
  step("copy", () => {
    const skipCounts = {};
    rmSync(workdir, { recursive: true, force: true });
    copyConsumer(consumerRoot, copyRoot, skipCounts);
    const files = inventory(copyRoot);
    return { files: files.size, skippedDirEntries: skipCounts };
  });
} else {
  step("in-place", () => ({ accepted: true, note: "the copy discipline is bypassed" }));
}

step("adopt-dry-run", () => {
  const before = inventory(copyRoot);
  const dry = runLekalo(copyRoot, ["init", "--adopt", "--target", "node-typescript", "--dry-run", "--project", "."]);
  if (dry.exitCode !== 0 || dry.envelope?.status !== "valid") {
    throw new Error(`dry-run refused: exit ${dry.exitCode} ${dry.stderrTail}`);
  }
  const after = inventory(copyRoot);
  if (inventoryDiffers(before, after)) throw new Error("the adopt dry-run wrote to the tree");
  return { plannedWrites: dry.envelope.writes?.length ?? 0, purity: "intact" };
});

step("adopt", () => {
  const real = runLekalo(copyRoot, ["init", "--adopt", "--target", "node-typescript", "--project", "."]);
  if (real.exitCode !== 0 || real.envelope?.status !== "valid") {
    throw new Error(`adopt refused: exit ${real.exitCode} ${real.stderrTail}`);
  }
  adopt = real.envelope;
  return { projectIdDigest: sha256Hex(Buffer.from(adopt.projectId)).slice(0, 16), created: adopt.created };
});

// ---------------------------------------------------------------------------
// phase 2 — the scan: wire attempt, then the in-process kernel fallback
// ---------------------------------------------------------------------------

step("profile", () => {
  profileBundle = buildProfile(copyRoot);
  writeFileSync(join(workdir, "profile.json"), JSON.stringify(profileBundle.profile, null, 1));
  return {
    readRoots: profileBundle.profile.readRoots.length,
    exclusions: profileBundle.profile.exclusions.length,
    packages: profileBundle.packages.length,
    grammarExcludedEntries: profileBundle.grammarExcludedCount,
  };
});

step("scan-wire", () => {
  const profileJson = JSON.stringify(profileBundle.profile);
  const policyJson = JSON.stringify(FRAMEWORK_POLICY);
  const attempt = (adapterForWire) => {
    const wire = runLekalo(copyRoot, [
      "scan",
      "--target", "node-typescript",
      "--profile", "standalone",
      "--project", ".",
      "node", adapterForWire,
      "--lekalo-project-profile-json", profileJson,
      "--lekalo-framework-policy-json", policyJson,
    ], { timeoutMs: 900_000 });
    if (wire.exitCode === 0 && wire.envelope?.status === "valid") {
      return { used: true, symbols: wire.envelope.symbols, endpoints: wire.envelope.endpoints };
    }
    const diagnostic = wire.envelope?.diagnostics?.[0];
    return {
      used: false,
      reason: diagnostic?.id ?? `exit-${wire.exitCode}`,
      detail: diagnostic?.data?.detail ?? diagnostic?.data?.field ?? null,
    };
  };
  // Attempt one: the committed adapter in place (the manifested wire
  // path — the manifest pins the profile and read-scope surface).
  const manifested = attempt(adapterPath);
  if (manifested.used) {
    scanDoc = { wireUsed: true };
    return { variant: "manifested", ...manifested };
  }
  // Attempt two: a manifest-free copy of the same committed bundle in
  // the working directory (the implicit local-development wire path:
  // no manifest consistency gate, strict default confinement). This
  // isolates the manifest-pinning refusal from the wire's own ability
  // to carry the exchange.
  const standalone = join(workdir, "adapter-standalone.mjs");
  cpSync(adapterPath, standalone);
  const unmanifested = attempt(standalone);
  if (unmanifested.used) {
    scanDoc = { wireUsed: true };
    return { variant: "local-development", manifestedRefusal: `${manifested.reason}/${manifested.detail ?? ""}`, ...unmanifested };
  }
  return {
    used: false,
    manifestedRefusal: `${manifested.reason}/${manifested.detail ?? ""}`,
    localDevRefusal: `${unmanifested.reason}/${unmanifested.detail ?? ""}`,
  };
}, { required: false });

step("scan-fallback", () => {
  if (scanDoc?.wireUsed === true) return { used: false, note: "the wire scan already recorded the index" };
  const scanner = adapter.__lekaloScanner;
  const profile = kernel.validateResolvedProjectProfile(profileBundle.profile);
  const roots = profile.readRoots.map((root) => ({
    ...root,
    scope: root.kind === "tree" ? `${root.path}/**` : root.path,
  }));
  const readView = kernel.createReadView(copyRoot, roots, profile);
  const session = new scanner.ScannerSession();
  const policyFrameworks = FRAMEWORK_POLICY.providers
    .filter((entry) => entry.state === "enabled")
    .map((entry) => entry.id);
  const coldStart = Date.now();
  const cold = session.scan({
    profile, readView, permittedProjectRoot: copyRoot, frameworks: policyFrameworks,
  });
  const coldMs = Date.now() - coldStart;
  const warmStart = Date.now();
  const warm = session.scan({
    profile, readView, permittedProjectRoot: copyRoot, frameworks: policyFrameworks,
  });
  const warmMs = Date.now() - warmStart;
  const index = cold.index;
  if (index.state !== "complete") throw new Error(`scan state ${index.state}`);
  const warmByteIdentical = canonicalText(cold.index) === canonicalText(warm.index);
  const built = buildScanDocument({
    index, identity: adapter.__lekaloAdapterIdentity, project: adopt.projectId,
    rename: bindTarget,
  });
  scanDoc = built;
  scanDocPath = join(workdir, "fallback-scan.json");
  writeFileSync(scanDocPath, JSON.stringify(built.document, null, 1));
  if (built.renameMatches !== 1) {
    throw new Error(`the bound use case matched ${built.renameMatches} top-level symbols, expected 1`);
  }
  return {
    used: true,
    coldMs, warmMs, warmByteIdentical,
    scannedSymbols: index.symbols.length,
    scanEntries: built.document.symbols.length,
    uncertaintyRows: index.anyUncertainty.length,
    diagnostics: index.diagnostics.length,
    endpoints: built.document.endpoints.length,
    unboundRoutes: built.unboundRoutes,
    testBindings: built.document.testBindings?.length ?? 0,
  };
});

step("modules", () => {
  if (scanDoc === null || scanDoc.wireUsed === true) return { modules: 0, note: "no fallback document" };
  if (scanDoc.document === undefined) {
    scanDoc = { wireUsed: true, document: { symbols: [], endpoints: [], testBindings: [] }, boundHandlerNative: null };
    return { modules: 0, note: "no fallback document" };
  }
  const scopes = [...new Set(scanDoc.document.symbols.map((symbol) => symbol.id.split(".")[0]))].sort();
  for (const scope of scopes) {
    const created = runLekalo(copyRoot, ["module", "new", scope]);
    if (created.envelope?.status === "valid") continue;
    const refusal = created.stderrTail + JSON.stringify(created.envelope?.diagnostics?.[0]?.data ?? {});
    if (!/exist|present|duplicate|already|declaration/i.test(refusal)) {
      throw new Error(`module new refused: ${refusal.slice(0, 200)}`);
    }
  }
  return { modules: scopes.length };
});

step("observe-update", () => {
  if (scanDoc?.wireUsed === true) return { note: "the wire scan already recorded the index" };
  const update = runLekalo(copyRoot, ["observe", "update", "--scan", relative(copyRoot, scanDocPath).split("\\").join("/"), "--project", "."]);
  if (update.exitCode !== 0 || update.envelope?.status !== "valid") {
    throw new Error(`update refused: ${update.stderrTail}`);
  }
  return {
    symbols: update.envelope.symbols,
    endpoints: update.envelope.endpoints,
    inferred: update.envelope.inferred,
    explicit: update.envelope.explicit,
  };
});

// ---------------------------------------------------------------------------
// phase 3 — bind one use case
// ---------------------------------------------------------------------------

let boundNative = null;
let boundLocation = null;

step("bind-use-case", () => {
  const docSymbol = scanDoc.document.symbols.find((symbol) => symbol.id === bindTarget.semanticId);
  if (docSymbol === undefined) throw new Error("the renamed use-case symbol is absent from the scan document");
  boundNative = docSymbol.stableKey ?? docSymbol.candidates[0].native;
  boundLocation = docSymbol.location ?? {
    path: docSymbol.candidates[0].path,
    line: docSymbol.candidates[0].line,
  };
  const bind = runLekalo(copyRoot, [
    "observe", "bind", docSymbol.id,
    "--key", boundNative,
    "--path", boundLocation.path,
    "--line", String(boundLocation.line ?? 1),
    "--project", ".",
  ]);
  if (bind.exitCode !== 0 || bind.envelope?.status !== "valid") {
    throw new Error(`bind refused: ${bind.stderrTail}`);
  }
  return { state: bind.envelope.state, binding: bind.envelope.binding };
});

step("confirm-handler-binding", () => {
  const propose = runLekalo(copyRoot, ["bindings", "propose", "--project", "."]);
  if (propose.exitCode !== 0 || propose.envelope?.status !== "valid") {
    throw new Error(`propose refused: ${propose.stderrTail}`);
  }
  const handlerNative = scanDoc.boundHandlerNative ?? boundNative;
  const proposal = propose.envelope.proposals.find((entry) =>
    entry.ambiguous === false && entry.candidates.some((candidate) => candidate.native === handlerNative))
    ?? propose.envelope.proposals.find((entry) => entry.ambiguous === false);
  if (proposal === undefined) return { confirmed: 0, note: "no unambiguous proposal" };
  const confirm = runLekalo(copyRoot, ["bindings", "confirm", proposal.proposal, "--project", "."]);
  if (confirm.exitCode !== 0 || confirm.envelope?.status !== "valid") {
    throw new Error(`confirm refused: ${confirm.stderrTail}`);
  }
  return { confirmed: 1, ambiguousProposals: propose.envelope.proposals.filter((entry) => entry.ambiguous).length };
});

function globFallbackTestIds() {
  const boundPackage = boundLocation.path.split("/").slice(0, 2).join("/");
  const packageRoot = join(copyRoot, boundPackage);
  const ids = [];
  if (!existsSync(packageRoot)) return ids;
  const moduleStem = boundLocation.path.split("/").pop().replace(/\.[cm]?tsx?$/, "");
  const walk = (dir, prefix) => {
    for (const entry of readdirSync(dir).sort()) {
      const rel = prefix === "" ? entry : `${prefix}/${entry}`;
      const absolute = join(dir, entry);
      if (statSync(absolute).isDirectory()) walk(absolute, rel);
      else if (/\.(test|spec)\.[cm]?tsx?$/.test(entry)) {
        const text = readFileSync(absolute, "utf8");
        if (text.includes(moduleStem)) ids.push(`${boundPackage}/${rel}`);
      }
    }
  };
  walk(packageRoot, "");
  return ids;
}

step("attach-native-tests", () => {
  const boundPackage = boundLocation.path.split("/").slice(0, 2).join("/");
  const scanTests = (scanDoc.document?.testBindings ?? [])
    .filter((binding) => binding.path.startsWith(`${boundPackage}/`))
    .map((binding) => binding.id);
  const ids = scanTests.length > 0 ? scanTests : globFallbackTestIds();
  if (ids.length === 0) return { attached: 0, source: "none" };
  const attach = runLekalo(copyRoot, [
    "observe", "attach", bindTarget.semanticId,
    "--native-test", ids.slice(0, 8).join(","),
    "--project", ".",
  ]);
  if (attach.exitCode !== 0 || attach.envelope?.status !== "valid") {
    throw new Error(`attach refused: ${attach.stderrTail}`);
  }
  return {
    attached: attach.envelope.native_tests.length,
    source: scanTests.length > 0 ? "scan-index" : "glob-fallback",
  };
});

// ---------------------------------------------------------------------------
// phase 4 — projections
// ---------------------------------------------------------------------------

const projections = {};

step("promote-use-case", () => {
  const dry = runLekalo(copyRoot, ["observe", "promote", "--symbol", bindTarget.semanticId, "--dry-run", "--project", "."]);
  if (dry.exitCode !== 0 || dry.envelope?.status !== "valid") {
    throw new Error(`promote plan refused: ${dry.stderrTail}`);
  }
  const apply = runLekalo(copyRoot, [
    "observe", "promote", "--symbol", bindTarget.semanticId, "--confirm", dry.envelope.plan, "--project", ".",
  ]);
  if (apply.exitCode !== 0 || apply.envelope?.status !== "valid") {
    throw new Error(`promote apply refused: ${apply.stderrTail}`);
  }
  return { symbols: apply.envelope.symbols.length };
});

step("inspect", () => {
  const inspect = runLekalo(copyRoot, ["observe", "inspect", bindTarget.semanticId, "--project", "."]);
  if (inspect.exitCode !== 0 || inspect.envelope?.status === "invalid") {
    throw new Error(`inspect refused: ${inspect.stderrTail}`);
  }
  const card = inspect.envelope;
  projections.inspect = {
    binding: card.binding, state: card.state, promoted: card.promoted,
    nativeTests: card.native_tests.length, endpoints: card.endpoints.length,
    completeness: card.completeness,
  };
  return projections.inspect;
});

step("impact", () => {
  const impact = runLekalo(copyRoot, ["observe", "impact", bindTarget.semanticId, "--project", "."]);
  if (impact.exitCode !== 0 || impact.envelope?.status === "invalid") {
    throw new Error(`impact refused: ${impact.stderrTail}`);
  }
  projections.impact = {
    dependents: impact.envelope.dependents.length,
    recordedSymbols: impact.envelope.recorded_symbols,
    stale: impact.envelope.stale,
    unknown: impact.envelope.unknown,
    completeness: impact.envelope.completeness,
  };
  return projections.impact;
});

step("context", () => {
  const markdown = runLekalo(copyRoot, ["context", "--budget", String(args.budget), bindTarget.semanticId, "--project", "."]);
  if (markdown.exitCode !== 0) throw new Error(`context refused: ${markdown.stderrTail}`);
  const json = runLekalo(copyRoot, ["context", "--json", "--budget", String(args.budget), bindTarget.semanticId, "--project", "."]);
  if (json.exitCode !== 0 || json.envelope?.status !== "valid") {
    throw new Error(`context json refused: ${json.stderrTail}`);
  }
  const capsule = json.envelope.context;
  projections.context = {
    budget: capsule.budget.limit,
    estimatedTokens: capsule.budget.estimated,
    fits: capsule.budget.fits,
    includedSections: capsule.coverage.included,
    excludedSections: capsule.coverage.excluded,
    gaps: capsule.gaps.length,
    complete: capsule.complete,
    markdownBytes: Buffer.byteLength(markdown.stdout, "utf8"),
  };
  return projections.context;
});

step("baseline", () => {
  const baseline = runLekalo(copyRoot, ["observe", "baseline", "--project", "."]);
  if (baseline.exitCode !== 0 || baseline.envelope?.status !== "valid") {
    throw new Error(`baseline refused: ${baseline.stderrTail}`);
  }
  projections.baseline = {
    indexDigestPrefix: baseline.envelope.index_digest.slice(0, 19),
    counts: baseline.envelope.counts,
  };
  return { symbols: baseline.envelope.counts.symbols };
});

step("bindings-list", () => {
  const list = runLekalo(copyRoot, ["bindings", "list", "--project", "."]);
  if (list.exitCode !== 0 || list.envelope?.status !== "valid") {
    throw new Error(`bindings list refused: ${list.stderrTail}`);
  }
  projections.bindings = list.envelope.counts;
  return projections.bindings;
});

step("status-doctor", () => {
  const status = runLekalo(copyRoot, ["status", "--project", "."]);
  const doctor = runLekalo(copyRoot, ["doctor", "--json", "--project", "."]);
  projections.status = {
    statusOk: status.exitCode === 0 && status.envelope?.status === "valid",
    doctorVerdict: doctor.envelope?.verdict ?? `exit-${doctor.exitCode}`,
  };
  return projections.status;
});

// ---------------------------------------------------------------------------
// phase 5 — controlled change and staleness
// ---------------------------------------------------------------------------

step("controlled-change", () => {
  const boundFile = join(copyRoot, boundLocation.path);
  const original = readFileSync(boundFile, "utf8");
  const name = bindTarget.exportName;
  let mutated = original.replace(
    new RegExp(`((?:export\\s+)?(?:async\\s+)?function\\s+${name}\\s*\\()`),
    "$1__pilotProbe: string = 'pilot', ",
  );
  let strategy = "parameter-inserted";
  if (mutated === original) {
    mutated = original.replace(
      new RegExp(`(const\\s+${name}\\s*=\\s*(?:async\\s*)?\\()`),
      "$1__pilotProbe: string = 'pilot', ",
    );
    strategy = "arrow-parameter-inserted";
  }
  if (mutated === original) {
    mutated = `${original}\n// pilot controlled-change probe\n`;
    strategy = "append-marker";
  }
  writeFileSync(boundFile, mutated);
  const audit = runLekalo(copyRoot, ["bindings", "audit", "--project", "."]);
  const staleDuring = audit.envelope?.status === "invalid"
    && (audit.envelope.diagnostics ?? []).some((diagnostic) => diagnostic.id === "observed.stale-binding");
  const check = runLekalo(copyRoot, ["observe", "check", "--project", "."]);
  const checkFailed = check.exitCode !== 0 || check.envelope?.status === "invalid";
  writeFileSync(boundFile, original);
  const reverted = runLekalo(copyRoot, ["bindings", "audit", "--project", "."]);
  const cleanAfter = reverted.exitCode === 0 && reverted.envelope?.status === "valid";
  if (!staleDuring) throw new Error(`staleness did not fire (strategy ${strategy})`);
  if (!checkFailed) throw new Error("the staleness gate passed over a stale binding");
  if (!cleanAfter) throw new Error("the audit stayed stale after revert");
  return {
    strategy,
    staleDiagnostics: (audit.envelope.diagnostics ?? [])
      .filter((diagnostic) => diagnostic.id === "observed.stale-binding").length,
    checkFailed: true,
    cleanAfterRevert: cleanAfter,
  };
});

// ---------------------------------------------------------------------------
// metrics and report
// ---------------------------------------------------------------------------

function finalizeMetrics() {
  metrics.totals.durationMs = Date.now() - runStarted;
  metrics.totals.ok = steps.filter((entry) => entry.status === "ok").length;
  metrics.totals.failed = steps.filter((entry) => entry.status === "failed").length;
  metrics.steps = steps.map((entry) => ({
    step: entry.step, status: entry.status, required: entry.required,
    durationMs: entry.durationMs,
    ...(entry.detail ?? {}),
    ...(entry.error ? { error: entry.error } : {}),
  }));
  metrics.projections = projections;
  return metrics;
}

function writeReport(dir) {
  const m = metrics;
  const detail = (name) => m.steps.find((entry) => entry.step === name) ?? {};
  const wire = detail("scan-wire");
  const fallback = detail("scan-fallback");
  const mutation = detail("controlled-change");
  const ctx = m.projections.context ?? {};
  const impact = m.projections.impact ?? {};
  const bindings = m.projections.bindings ?? {};
  const inspect = m.projections.inspect ?? {};
  const lines = [];
  lines.push("# Brownfield TypeScript consumer pilot (issue #118) — run report");
  lines.push("");
  lines.push("Observed-mode pilot over a copied private TypeScript consumer.");
  lines.push("Privacy: aggregates only; the consumer is named by the sha256 of its");
  lines.push("canonical path; no file names, identifiers, snippets, or absolute");
  lines.push("paths appear below.");
  lines.push("");
  lines.push("| Field | Value |");
  lines.push("| --- | --- |");
  lines.push(`| consumer | \`consumer\` (${m.consumer.pathDigest.slice(0, 19)}…) |`);
  lines.push(`| mode | ${m.mode} |`);
  lines.push(`| provenance disposition | ${m.disposition} |`);
  lines.push(`| total duration | ${m.totals.durationMs} ms |`);
  lines.push(`| steps ok / failed | ${m.totals.ok} / ${m.totals.failed} |`);
  lines.push("");
  lines.push("## Steps");
  lines.push("");
  lines.push("| Step | Status | Duration (ms) | Key detail |");
  lines.push("| --- | --- | --- | --- |");
  for (const entry of m.steps) {
    const detailBits = Object.entries(entry)
      .filter(([key]) => !["step", "status", "required", "durationMs", "error"].includes(key))
      .map(([key, value]) => `${key}=${typeof value === "object" ? JSON.stringify(value) : value}`);
    lines.push(`| ${entry.step}${entry.required ? "" : " (optional)"} | ${entry.status} | ${entry.durationMs} | ${detailBits.join(", ").replace(/\|/g, "\\|").slice(0, 160)} |`);
  }
  lines.push("");
  lines.push("## Pilot metrics");
  lines.push("");
  lines.push(`- cold scan: ${fallback.coldMs ?? "?"} ms; warm scan: ${fallback.warmMs ?? "?"} ms; warm byte-identical: ${fallback.warmByteIdentical ?? "?"}`);
  lines.push(`- scanned symbols: ${fallback.scannedSymbols ?? "?"}; exported scan entries: ${fallback.scanEntries ?? "?"}`);
  lines.push(`- scan uncertainty rows: ${fallback.uncertaintyRows ?? "?"}; scan diagnostics: ${fallback.diagnostics ?? "?"}`);
  lines.push(`- endpoints derived: ${fallback.endpoints ?? 0}; routes without an indexed handler: ${fallback.unboundRoutes ?? "?"}`);
  lines.push(`- native test bindings from the scan: ${fallback.testBindings ?? 0}`);
  lines.push(`- wire-path scan used: ${wire.used ?? false}${wire.used ? "" : ` (manifested: ${wire.manifestedRefusal ?? "?"}; local-development: ${wire.localDevRefusal ?? "?"})`}`);
  lines.push(`- context capsule: ${ctx.estimatedTokens ?? "?"} tokens (budget ${ctx.budget ?? "?"}, fits ${ctx.fits ?? "?"}), sections included ${ctx.includedSections ?? "?"}, gaps ${ctx.gaps ?? "?"}`);
  lines.push(`- impact: recorded symbols ${impact.recordedSymbols ?? "?"}, dependents ${impact.dependents ?? "?"}, unknown edges ${impact.unknown ?? "?"}, completeness ${impact.completeness ?? "?"}`);
  lines.push(`- binding registry: ${bindings.bindings ?? "?"} bindings, ${bindings.endpoints ?? "?"} endpoints, ${bindings.tests ?? "?"} tests`);
  lines.push(`- use-case card: binding ${inspect.binding ?? "?"}, state ${inspect.state ?? "?"}, promoted ${inspect.promoted ?? "?"}, completeness ${inspect.completeness ?? "?"}`);
  lines.push(`- controlled change: strategy ${mutation.strategy ?? "?"}, stale diagnostics ${mutation.staleDiagnostics ?? 0}, staleness gate failed as expected ${mutation.checkFailed ?? "?"}, clean after revert ${mutation.cleanAfterRevert ?? "?"}`);
  lines.push("");
  lines.push("## Findings");
  lines.push("");
  lines.push(...(findings.length > 0 ? findings.map((finding) => `- ${finding}`) : ["- none recorded by this run."]));
  lines.push("");
  writeFileSync(join(dir, "report.md"), lines.join("\n"));
  writeFileSync(join(dir, "metrics.json"), `${JSON.stringify(m, null, 1)}\n`);
}

step("report", () => {
  const wire = steps.find((entry) => entry.step === "scan-wire");
  if (wire?.detail?.used === false) {
    findings.push(`The \`lekalo scan\` wire path refused the exchange twice: the manifested committed bundle (${wire.detail.manifestedRefusal}) and the same bundle as implicit local-development (${wire.detail.localDevRefusal}). The manifest pins the profile surface and read scopes the manifested bundle may describe, and the wire's own outcome projection refuses uncertainty-bearing scans and caps every entry detail token at 128 bytes, which scoped-package monorepos and test-bearing modules exceed. The harness fell back to driving the committed kernel directly (the same derivation the committed taskhub scan generator uses) and merged the resulting document through \`observe update\`.`);
  }
  const fallback = steps.find((entry) => entry.step === "scan-fallback");
  if (fallback?.detail?.used === true && fallback.detail.uncertaintyRows > 0) {
    findings.push(`The scan carries ${fallback.detail.uncertaintyRows} honest uncertainty rows (unresolved imports and unknown surfaces — expected when the copy has no node_modules and when the consumer imports dialect subpaths the adapter's embedded declaration closure does not map). Unknown edges are data, never guesses.`);
  }
  const scanDiagnostics = fallback?.detail?.diagnostics ?? 0;
  if (scanDiagnostics > 0) {
    findings.push(`The scan records ${scanDiagnostics} compiler diagnostics over the copy — resolution gaps of the node_modules-free scan, kept as evidence rather than suppressed.`);
  }
  mkdirSync(outDir, { recursive: true });
  return {};
});

// Finalize twice: the report step's own record must appear with its
// final status inside the emitted metrics and report.
finalizeMetrics();
writeReport(outDir);
const reportRecord = steps[steps.length - 1];
reportRecord.detail = {
  reportBytes: statSync(join(outDir, "report.md")).size,
  metricsBytes: statSync(join(outDir, "metrics.json")).size,
};
finalizeMetrics();
writeReport(outDir);

// cleanup: the working directory (consumer copy + scan document) is the
// private side; it is removed unless --keep was passed.
if (!args.keep) {
  rmSync(workdir, { recursive: true, force: true });
}

// ---------------------------------------------------------------------------
// exit
// ---------------------------------------------------------------------------

const failedRequired = steps.filter((entry) => entry.required && entry.status === "failed");
process.stdout.write(`\n${failedRequired.length === 0 ? "PILOT GREEN" : "PILOT FAILED"} — ${metrics.totals.ok} ok / ${metrics.totals.failed} failed (report: ${join(outDir, "report.md")})\n`);
if (failedRequired.length > 0) process.exit(1);

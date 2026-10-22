#!/usr/bin/env node
/**
 * Cross-language codec parity gate for the adapter SDKs (issue #106).
 *
 * Runs the shared vector corpus (`tests/fixtures/adapter-sdk/`) through
 * every provisioned language runtime and compares the outcomes with each
 * other, including the exact canonical bytes. Schema-valid semantic
 * negatives are compared too: a codec that accepts what production
 * admission refuses fails this gate, not the corpus.
 *
 * Since the round-2/round-3 reviews (issue #106) the gate also enforces:
 *   - the complete declared family set (config.testVectorFamilies) with
 *     CORPUS-RELATIVE completeness: per-directory executed counts and the
 *     canonical output name set are compared against the files on disk,
 *     so an omission shared by every runner fails instead of hiding in
 *     the baseline (review r3 #9);
 *   - the lifecycle recipes execute through the real reference adapters
 *     on every lane, the control lane is identified by NAME (never argv
 *     sniffing), the hostile fault classes actually execute against the
 *     real SDK serve loops through the templates' `--lekalo-fault` knobs,
 *     the file-transport fixture drives its declared ["file"] transport,
 *     and the executed recipe×lane pairs are counted and asserted —
 *     zero hostile or file legs is a gate failure, not a green verdict
 *     (review r3 #1);
 *   - the native-template bootstrap provisions missing binaries with
 *     per-language build descriptors (cargo --manifest-path, go build
 *     -o) selected by executable identity, so a clean checkout passes
 *     on every OS (review r3 #2);
 *   - production admission agreement over the FULL corpus as raw BYTES
 *     in hex framing: valid, invalid, semantic-request, and upgrade rows
 *     replay through the lekalo-core wire decoder unchanged (numeric
 *     spellings, duplicate keys, CR/LF preserved), and the response
 *     corpus replays through the production response decoder (review
 *     r3 #8);
 *   - process-level transport vectors: a raw invalid-UTF-8 request file
 *     must refuse on every language, and a valid request whose multibyte
 *     character straddles the stdin chunk boundary must decode intact
 *     (review r3 #6).
 *
 * Usage:
 *   node scripts/test-adapter-sdk-vectors.mjs --language all
 *   node scripts/test-adapter-sdk-vectors.mjs --language typescript
 *
 * Languages are executed through their own runners:
 *   typescript  node sdk/typescript/test/vectors.mjs
 *   rust        cargo test (sdk/rust) — runner prints the same verdict JSON
 *   go          go test (sdk/go)
 *   php         php sdk/php/test/vectors.php
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a JSON
 * failure report on stderr otherwise.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPT_DIR, "..");

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-vectors", reason, detail })}\n`);
  process.exit(1);
};

const args = process.argv.slice(2);
const languageFlag = args.indexOf("--language");
const language = languageFlag !== -1 ? args[languageFlag + 1] : "all";
const ALL = ["typescript", "rust", "go", "php"];
const languages = language === "all" ? ALL : ALL.includes(language) ? [language] : null;
if (!languages) fail("usage", "--language must be one of rust|typescript|go|php|all");
if (languages.includes('typescript')) {
  const stream = spawnSync(process.execPath, ['--test', join(ROOT, 'sdk/typescript/test/stream.mjs'), join(ROOT, 'sdk/typescript/test/conformance.mjs')], { encoding: 'utf8', timeout: 30000 });
  if (stream.status !== 0) fail('typescript-stream', `${stream.stdout ?? ''}${stream.stderr ?? ''}`);
}

const run = (program, args_, cwd, options) =>
  spawnSync(program, args_, { cwd: cwd ?? ROOT, encoding: "utf8", timeout: 600_000, shell: process.platform === "win32" && program !== process.execPath, ...options });

// The Rust runner compiles the SDK crate into the binary; a poisoned
// incremental target dir once served a stale binary that certified old
// admission behavior (review r2 #15 disposition). Build it in a fresh
// target dir so the verdict always reflects current sources. The native
// lifecycle templates get the same treatment below (review r3 #2).
const rustTargetDir = mkdtempSync(join(tmpdir(), "adapter-sdk-vectors-rust-"));
const nativeTemplateDir = mkdtempSync(join(tmpdir(), "adapter-sdk-vectors-native-"));
process.on("exit", () => {
  for (const dir of [rustTargetDir, nativeTemplateDir]) {
    try { rmSync(dir, { recursive: true, force: true }); } catch { /* best effort */ }
  }
});

const RUNNERS = {
  go: { command: "go", args: ["run", "./internal/vectorsrunner"], cwd: join(ROOT, "sdk/go"), label: "go run ./internal/vectorsrunner" },
  // Confined Linux PHP has no host ini/extensions. Also disable ctype on
  // builds that compile it in, so every OS exercises the same dependency-free
  // path/version grammar instead of masking an optional-extension dependency.
  php: { command: "php", args: ["-n", "-d", "disable_functions=ctype_alpha,ctype_lower,ctype_digit", "test/vectors.php"], cwd: join(ROOT, "sdk/php"), label: "php -n (without ctype) test/vectors.php" },
  rust: {
    command: "cargo",
    args: ["run", "--quiet", "--bin", "vectors"],
    cwd: join(ROOT, "sdk/rust"),
    label: "cargo run --bin vectors (fresh target dir)",
    env: { CARGO_TARGET_DIR: rustTargetDir },
  },
  typescript: { command: process.execPath, args: ["test/vectors.mjs"], cwd: join(ROOT, "sdk/typescript"), label: "node test/vectors.mjs" },
};

const trace = (message) => { if (process.env.LEKALO_SDK_VECTOR_TRACE === "1") process.stderr.write(`[adapter-sdk-vectors] ${message}\n`); };
const verdicts = {};
const missing = [];
for (const lang of languages) {
  trace(`runner ${lang}`);
  const runner = RUNNERS[lang];
  if (!existsSync(join(ROOT, "sdk", lang))) {
    missing.push(lang);
    continue;
  }
  const run = spawnSync(runner.command, runner.args, {
    cwd: runner.cwd,
    encoding: "utf8",
    timeout: 600_000,
    input: "",
    shell: process.platform === "win32",
    env: { ...process.env, ...(runner.env ?? {}) },
  });
  if (run.error || run.status !== 0) {
    const detail = run.error ? String(run.error) : `${run.stdout ?? ""}\n${run.stderr ?? ""}`.slice(-4000);
    if (!missing.includes(lang)) missing.push(lang);
    process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-vectors", reason: "runner-failed", language: lang, label: runner.label, detail })}\n`);
    process.exit(1);
  }
  const line = (run.stdout ?? "").trim().split("\n").filter(Boolean).pop();
  let verdict;
  try {
    verdict = JSON.parse(line);
  } catch {
    fail("verdict-unparseable", `${lang}: ${String(line).slice(0, 200)}`);
  }
  if (verdict.ok !== true) {
    process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-vectors", language: lang, verdict })}\n`);
    process.exit(1);
  }
  verdicts[lang] = verdict;
}

if (missing.length > 0) {
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "adapter-sdk-vectors", note: "package roots not present (skipped explicitly, never silently)", skipped: missing.sort() })}\n`);
  process.exit(0);
}

const config = JSON.parse(readFileSync(join(ROOT, "sdk/schema-inputs.json"), "utf8"));
const corpusRoot = join(ROOT, "tests/fixtures/adapter-sdk");
const FAMILY_DIRECTORIES = {
  canonical: ["canonical"],
  codec: ["valid", "invalid"],
  security: ["security"],
  semantic: ["semantic"],
  upgrade: ["upgrade"],
};

/** Corpus-relative completeness (issue #106 review r3 #9): the expected
 * case names and per-directory counts derive from the files on disk, so
 * an omission shared by every runner still fails. */
const corpusFiles = (directory) =>
  readdirSync(join(corpusRoot, directory))
    .filter((name) => name.endsWith(".json") && !name.endsWith(".expect.json"))
    .sort();
const corpusCanonicalNames = corpusFiles("canonical").map((name) => `canonical/${name}`);

// Cross-language canonical byte comparison: every language must emit the
// exact same canonical bytes for the same values, and the OUTPUT NAME SET
// must equal the corpus set exactly — a missing or extra canonical output
// is a mismatch, never a skip.
if (corpusCanonicalNames.length === 0) {
  fail("canonical-corpus-empty", "the canonical/ directory holds no vectors");
}
for (const [lang, verdict] of Object.entries(verdicts)) {
  const outputs = verdict.canonicalOutputs ?? {};
  const actualNames = Object.keys(outputs).sort();
  const missingNames = corpusCanonicalNames.filter((name) => outputs[name] === undefined);
  const extra = actualNames.filter((name) => !corpusCanonicalNames.includes(name));
  if (missingNames.length > 0 || extra.length > 0) {
    fail("canonical-output-set", lang + ": missing " + JSON.stringify(missingNames) + " extra " + JSON.stringify(extra));
  }
  for (const name of corpusCanonicalNames) {
    const vector = JSON.parse(readFileSync(join(corpusRoot, name), "utf8"));
    if (outputs[name] !== vector.canonical) {
      fail("canonical-bytes-corpus", `${lang} emitted different bytes than the corpus declares for ${name}`);
    }
  }
}
// Cross-language byte equality against the first runner (kept from r2):
// every language must agree byte-for-byte on every corpus row.
const baseline = Object.entries(verdicts)[0][1].canonicalOutputs ?? {};
for (const [lang, verdict] of Object.entries(verdicts)) {
  const outputs = verdict.canonicalOutputs ?? {};
  for (const [name, bytes] of Object.entries(baseline)) {
    if (outputs[name] !== bytes) {
      fail("canonical-bytes", `${lang} emitted different bytes for ${name}`);
    }
  }
}

const totals = Object.fromEntries(Object.entries(verdicts).map(([lang, verdict]) => [lang, verdict.total]));

// Complete family coverage: every declared family must exist in the
// corpus and be executed by every runner with the FULL per-directory
// count (issue #106 review r3 #9: the old check looked up family names
// in a directory-keyed map, so `codec` resolved to undefined and a
// runner reporting zero executions passed).
for (const family of config.testVectorFamilies) {
  const directories = FAMILY_DIRECTORIES[family];
  if (!directories) fail("family-mapping", family);
  for (const directory of directories) {
    const dir = join(corpusRoot, directory);
    if (!existsSync(dir)) fail("family-directory-missing", directory);
    if (corpusFiles(directory).length === 0) fail("family-directory-empty", directory);
  }
  for (const [lang, verdict] of Object.entries(verdicts)) {
    const executed = verdict.executedFamilies ?? [];
    if (!executed.includes(family)) {
      fail("family-not-executed", `${lang}: ${family} absent from the runner's executed families`);
    }
    const counts = verdict.executedVectors ?? {};
    for (const directory of directories) {
      const corpusCount = corpusFiles(directory).length;
      const executedCount = counts[directory];
      if (executedCount === undefined) {
        fail("family-count-missing", `${lang}: ${family} reports no executed count for ${directory}`);
      }
      if (executedCount !== corpusCount) {
        fail("family-count", `${lang}: ${directory} executed ${executedCount} vectors, the corpus holds ${corpusCount}`);
      }
    }
  }
}

// Lifecycle recipes execute through the real reference adapters with
// their declared adapterMode knobs (issue #106 review: the lifecycle
// directory was declared but never executed).
const lifecycleDir = join(corpusRoot, "lifecycle");
if (!existsSync(lifecycleDir)) fail("family-directory-missing", "lifecycle");
const lifecycleNames = readdirSync(lifecycleDir).filter((name) => name.endsWith(".json")).sort();
if (lifecycleNames.length === 0) fail("family-directory-empty", "lifecycle");

const hostCli = join(ROOT, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(hostCli)) {
  const build = run("cargo", ["build", "--locked", "-p", "lekalo-cli"]);
  if (build.status !== 0) fail("cli-missing", `build the host CLI first: ${(build.stderr ?? "").slice(-400)}`);
}

/** Terminal classification over the battery's closed verdict vocabulary
 * (issue #106 review r4 #7): every predicate binds to the SPECIFIC
 * failing check (id + detail) the battery emits for that class, so an
 * unrelated setup failure can no longer qualify a hostile terminal —
 * the r3 predicates accepted any failing check. The negative controls
 * below pin that property: a battery that never starts and a synthetic
 * unrelated failure must qualify NO hostile class. */
const failingChecks = (report) => (report?.checks ?? []).filter((entry) => entry.state === "fail");
const hasFailure = (report, id, detail) =>
  failingChecks(report).some((entry) => entry.id === id && (detail === undefined || entry.detail === detail));
const expectedTerminals = {
  ok: (report) => report?.verdict === "pass",
  // The adapter process died before any response: crash detail.
  "process-failure": (report) => report !== null && report.verdict === "process" && hasFailure(report, "describe.handshake", "crash"),
  // Non-JSON bytes on the wire surface of the describe exchanges.
  "response-invalid": (report) => report?.verdict === "protocol" && hasFailure(report, "describe.handshake", "not-json"),
  // The real callback-failure path: the SDK answered the handshake with
  // a closed error envelope (operation-failed) instead of capabilities.
  "operation-error": (report) => report?.verdict === "feature" && hasFailure(report, "describe.handshake", "operation-failed"),
  // The stderr bound tripped while the serve loop was live.
  "output-limit": (report) => report?.verdict === "process" && hasFailure(report, "describe.handshake", "output-limit"),
  // The host's deadline enforcement terminated a stalled handler.
  timeout: (report) => report?.verdict === "process" && hasFailure(report, "describe.handshake", "deadline"),
  "dry-run-mutation": (report) =>
    // Bound to the dry-run check id: the SDK lanes surface the mutation
    // as a closed operation failure; the control's raw write throws in
    // the sandbox and the process dies — both are the mutation surfacing
    // on exactly this check, never a silent success.
    hasFailure(report, "generate.dry-run-plan", "operation-failed") || hasFailure(report, "generate.dry-run-plan", "crash"),
  "extra-write": (report) => report?.verdict === "feature" && hasFailure(report, "generate.apply-plan", "undeclared"),
};

/** Real per-language SDK lifecycle lanes (issue #106 review r2 #8, r3
 * #1/#2): each lane replays every recipe through THAT language's emitted
 * reference adapter, so a serve-loop defect (output-cap bypass, exit-code
 * loss, evidence drift) fails here instead of hiding behind the
 * handwritten control. The control lane is identified by its NAME — the
 * old argv-sniffing (`adapterArgs.includes("fake-adapter.mjs")` on an
 * args ARRAY) never matched, so zero hostile recipes and zero file legs
 * executed while the gate printed `lifecycle:10`. The reference
 * templates carry the control's closed `--lekalo-fault` vocabulary, so
 * the hostile classes execute against the real SDK serve loops, and the
 * `file-only` fixture drives the `--lekalo-request-file` transport on
 * its declared ["file"] coordinate. */
const rustReference = join(nativeTemplateDir, "rust", "debug", process.platform === "win32" ? "sdk-reference-adapter.exe" : "sdk-reference-adapter");
const goReference = join(nativeTemplateDir, process.platform === "win32" ? "sdk-reference.exe" : "sdk-reference");
const lifecycleAdapters = {
  typescript: ["node", join(ROOT, "sdk/templates/typescript/reference/adapter.mjs")],
  rust: [rustReference],
  go: [goReference],
  php: ["php", join(ROOT, "sdk/templates/php/reference/adapter.php")],
  control: ["node", "tests/fixtures/target-protocol/fake-adapter.mjs", "--lekalo-adapter-variant", "fluent"],
};
const FILE_ONLY_ADAPTER = ["node", "tests/fixtures/target-protocol/file-only-adapter.mjs"];

/** Provision the native template binaries (issue #106 review r3 #2):
 * per-language build descriptors selected by the executable's identity —
 * cargo with an explicit --manifest-path for Rust, `go build -o` for Go
 * — and FRESH target dirs, so the lanes always execute binaries built
 * from current sources. The old substring match on a platform-dependent
 * path picked the WRONG language on Windows, `cargo build` inside the Go
 * directory walked up to the root workspace and never produced the Go
 * executable, and trusting a previously existing binary let a stale
 * build certify old behavior. */
const templateBuilds = [
  {
    executable: rustReference,
    program: "cargo",
    args: ["build", "--quiet", "--manifest-path", join(ROOT, "sdk/templates/rust/reference/Cargo.toml")],
    env: { CARGO_TARGET_DIR: join(nativeTemplateDir, "rust") },
    label: "cargo --manifest-path sdk/templates/rust/reference/Cargo.toml (fresh target dir)",
  },
  {
    executable: goReference,
    program: "go",
    args: ["build", "-o", goReference, "."],
    cwd: join(ROOT, "sdk/templates/go/reference"),
    label: "go build -o sdk-reference[.exe] (fresh output path)",
  },
];
for (const build of templateBuilds) {
  const result = run(build.program, build.args, build.cwd, { env: { ...process.env, ...(build.env ?? {}) } });
  if (result.status !== 0) fail("lifecycle-template-build", `${build.label}: ${(result.stderr ?? "").slice(-300)}`);
  if (!existsSync(build.executable)) fail("lifecycle-template-binary", `${build.label}: ${build.executable} still missing after build`);
}

/** adapterMode → the closed fault vocabulary shared by the control and
 * the reference templates (null = none). A hostile recipe must never
 * report pass; a cooperative recipe must report pass. The in-serve
 * knobs (issue #106 review r4 #7) drive the real SDK paths inside the
 * serve loop: callback-error returns an error, handler-crash throws/panics,
 * callback-hang stalls the handler past every deadline, and log-verbose
 * floods the bounded SDK logger from inside the handler. */
const MODE_FAULTS = {
  cooperative: null,
  "callback-error": "callback-error",
  "handler-crash": "handler-crash",
  "stdout-noise": "garbage",
  "stderr-verbose": "log-verbose", // bounded logging must not disturb a good run
  slow: "callback-hang", // deadline enforcement with the serve loop live
  oversize: "noise", // hostile probe: the run must still fail, never normalize
  "write-through": "mutate-dry",
  "undeclared-write": "extra-write",
};
const lifecycleLanguages = languages.filter((lang) => lifecycleAdapters[lang] !== undefined);
if (lifecycleLanguages.length === 0 && languages.length > 0) {
  // The battery lane needs a real adapter executable; the control is
  // always present, so at least one language runs it per invocation.
  lifecycleLanguages.push("typescript");
}
// The independence control leg rides along exactly once per invocation,
// as a named lane — never inferred from argv.
if (!lifecycleLanguages.includes("control")) lifecycleLanguages.push("control");

/** Executed recipe×lane accounting (issue #106 review r3 #1): the green
 * verdict must certify recipes that RAN. A recipe whose declared
 * terminal class never executed on any lane, a file-exchange leg that
 * never ran, or a skip anywhere is a gate failure. */
const lifecycleAccounting = { pairs: 0, expectedPairs: 0, hostile: 0, cooperative: 0, fileLegs: 0, recipes: lifecycleNames.length, lanes: lifecycleLanguages.length };
const runBattery = (adapterArgs, profile, timeoutMs) => {
  const battery = run(hostCli, [
    "adapter", "test",
    "--profile", profile,
    "--report", "json",
    "--repeats", "1",
    "--timeout-ms", String(timeoutMs),
    ...adapterArgs,
  ]);
  let report = null;
  const stdout = battery.stdout ?? "";
  const start = stdout.indexOf("{");
  if (start !== -1) {
    try { report = JSON.parse(stdout.slice(start, stdout.lastIndexOf("}") + 1)).report; } catch { report = null; }
  }
  return report;
};
const assertTerminal = (lane, recipe, report, expect) => {
  const classify = expectedTerminals[expect];
  if (!classify) fail("lifecycle-terminal-class", `${lane}/${recipe.case}: unknown expected terminal ${expect}`);
  if (report === null) {
    // No report at all: with the specific predicates nothing qualifies
    // from a missing report — the recipe failed to produce evidence.
    fail("lifecycle-terminal", `${lane}/${recipe.case}: expected ${expect}, battery produced no report`);
    return;
  }
  if (!classify(report)) {
    fail("lifecycle-terminal", `${lane}/${recipe.case}: expected ${expect}, verdict ${report.verdict}, failing: ${JSON.stringify((report.checks ?? []).filter((c) => c.state === "fail").map((c) => [c.id, c.detail]).slice(0, 3))}`);
  }
};
for (const lang of lifecycleLanguages) {
  const adapterArgs = [...lifecycleAdapters[lang]];
  for (const name of lifecycleNames) {
    trace(`lifecycle ${lang}/${name}`);
    const recipe = JSON.parse(readFileSync(join(lifecycleDir, name), "utf8"));
    // The file-exchange transport recipe drives the file-only fixture
    // exactly once per invocation (from the named control lane): the
    // fixture declares transports ["file"], so every session exchange
    // after the stdin-delivered handshake must arrive through the file
    // coordinate.
    if (recipe.adapter === "file-only") {
      if (lang !== "control") continue;
      lifecycleAccounting.fileLegs += 1;
      lifecycleAccounting.pairs += 1;
      const report = runBattery(FILE_ONLY_ADAPTER, "default", 10000);
      assertTerminal("file-exchange", recipe, report, recipe.expectedTerminal);
      continue;
    }
    const hostile = MODE_FAULTS[recipe.adapterMode] !== null && MODE_FAULTS[recipe.adapterMode] !== undefined;
    const argv = [...adapterArgs];
    const fault = MODE_FAULTS[recipe.adapterMode];
    if (fault) argv.push("--lekalo-fault", fault);
    lifecycleAccounting.pairs += 1;
    lifecycleAccounting.expectedPairs += 1;
    if (hostile) lifecycleAccounting.hostile += 1;
    else lifecycleAccounting.cooperative += 1;
    const report = runBattery(argv, "strict", recipe.adapterMode === "slow" ? 3000 : 10000);
    assertTerminal(lang, recipe, report, recipe.expectedByLane?.[lang] ?? recipe.expectedTerminal);
  }
}
// Every non-file recipe must have executed on EVERY lane, and the
// hostile/file classes must never be zero (the r3 gate printed
// `lifecycle:10` — a file count — while executing zero hostile legs).
const nonFileRecipes = lifecycleNames.map((name) => JSON.parse(readFileSync(join(lifecycleDir, name), "utf8"))).filter((recipe) => recipe.adapter !== "file-only");
lifecycleAccounting.expectedPairs = nonFileRecipes.length * lifecycleLanguages.length + 1;
if (lifecycleAccounting.pairs !== lifecycleAccounting.expectedPairs) {
  fail("lifecycle-accounting", `executed ${lifecycleAccounting.pairs} recipe×lane pairs, expected ${lifecycleAccounting.expectedPairs}`);
}
const hostileRecipes = nonFileRecipes.filter((recipe) => MODE_FAULTS[recipe.adapterMode] !== null && MODE_FAULTS[recipe.adapterMode] !== undefined);
if (lifecycleAccounting.hostile !== hostileRecipes.length * lifecycleLanguages.length) {
  fail("lifecycle-hostile-skipped", `executed ${lifecycleAccounting.hostile} hostile legs, expected ${hostileRecipes.length * lifecycleLanguages.length} (every declared hostile class must execute on every lane)`);
}
if (lifecycleAccounting.fileLegs !== 1) {
  fail("lifecycle-file-leg", `the file-exchange fixture executed ${lifecycleAccounting.fileLegs} times, expected exactly 1`);
}

// Negative controls (issue #106 review r4 #7): predicate specificity is
// proved, not assumed. A battery that never starts (broken adapter
// executable) and a synthetic unrelated setup failure must qualify NO
// hostile terminal — the r3 predicates accepted any failing check, so
// both of these qualified every class at once.
lifecycleAccounting.negativeControls = 0;
{
  const hostileTerminals = Object.keys(expectedTerminals).filter((terminal) => terminal !== "ok");
  const broken = runBattery(["node", join(tmpdir(), "lekalo-negative-control-missing.mjs")], "strict", 5000);
  const brokenQualified = hostileTerminals.filter((terminal) => expectedTerminals[terminal](broken));
  // A battery that never starts may legitimately be a process failure —
  // but NOTHING ELSE: one unrelated failure must never qualify several
  // hostile classes at once (the r3 predicates qualified all of them).
  if (brokenQualified.length > 1 || (brokenQualified.length === 1 && brokenQualified[0] !== "process-failure")) {
    fail("lifecycle-negative-control", `a broken-adapter battery qualified ${JSON.stringify(brokenQualified)} — an unrelated failure must not satisfy hostile terminals`);
  }
  lifecycleAccounting.negativeControls += 1;
  const unrelated = {
    verdict: "feature",
    checks: [{ id: "describe.negotiation", class: "feature", state: "fail", detail: "setup" }],
  };
  for (const terminal of hostileTerminals) {
    if (expectedTerminals[terminal](unrelated)) {
      fail("lifecycle-negative-control", `an unrelated describe.negotiation setup failure qualified the ${terminal} terminal`);
    }
  }
  lifecycleAccounting.negativeControls += 1;
}

// Exact emitted bytes, retained for production replay below. Invalid
// callback output must never become a stdout success or a refusal object.
const emittedOracleRows = [];
// Native-plan callbacks and direct corpus requests are not describe
// C-mode contexts. Replay their EXACT emitted bytes through V mode.
const nativeEmissionOracleRows = [];

// Real per-SDK lifecycle lanes beyond the battery (issue #106 review r4
// #7): a POSITIVE valid file exchange and an oversized-response refusal
// through the real bounded serializer, one leg per SDK lane — the
// battery's file fixture is control-only and its stderr class is not
// the SDK stdout budget.
lifecycleAccounting.fileValidLegs = 0;
lifecycleAccounting.outputLimitLegs = 0;
lifecycleAccounting.outputLimitControlLegs = 0;
lifecycleAccounting.deadlineTokenLegs = 0;
lifecycleAccounting.invalidBuilderLegs = 0;
lifecycleAccounting.realCrashLegs = 0;
lifecycleAccounting.stdoutHygieneLegs = 0;
lifecycleAccounting.nativePlanBuilderLegs = 0;
const r10HostileRequests = corpusFiles('invalid').filter((name) => name.startsWith('r10-')).flatMap((name) => {
  const bytes = readFileSync(join(corpusRoot, 'invalid', name));
  const value = JSON.parse(bytes.toString('utf8'));
  if (value.status !== undefined || value.operation === undefined) return [];
  const expect = JSON.parse(readFileSync(join(corpusRoot, 'invalid', name.replace(/\.json$/, '.expect.json')), 'utf8'));
  if (expect.stage !== 'decode') fail('lifecycle-stdout-hygiene-inventory', name);
  return [bytes];
});
if (r10HostileRequests.length === 0) fail('lifecycle-stdout-hygiene-inventory', 'r10 request regression set is empty');
const hygieneInvocations = (lang, args) => lang === 'php' ? [
  args, [args[0], '-d', 'display_errors=1', ...args.slice(1)],
  [args[0], '-d', 'display_errors=0', ...args.slice(1)],
  [args[0], '-n', '-d', 'display_errors=1', '-d', 'disable_functions=ctype_alpha,ctype_lower,ctype_digit', ...args.slice(1)],
] : [args];
{
  const fileExchangeDir = mkdtempSync(join(tmpdir(), "adapter-sdk-vectors-file-exchange-"));
  process.on("exit", () => {
    try { rmSync(fileExchangeDir, { recursive: true, force: true }); } catch { /* best effort */ }
  });
  // This sorted projection is the exact deterministic request the public
  // production TargetClient emits with these limits. C-mode's child verifies
  // it, and never rewrites the response to make an echo match.
  const describeContext = {
    limits: { max_output_bytes: 8388608, timeout_ms: 60000 },
    operation: "describe",
    project_root: ".",
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
  };
  const validDescribe = JSON.stringify({ ...describeContext, request_id: `req-${createHash("sha256").update(JSON.stringify(describeContext)).digest("hex")}` });
  // r11 composes legal positional Limits with actual serve consumers.
  // The inventory is corpus-backed and cannot omit the seq/tiny pair.
  const limitNames = corpusFiles('valid').filter((name) => name.startsWith('r11-serve-limits-'));
  const expectedLimitNames = ['roomy', 'tiny'].flatMap((budget) => ['map', 'seq'].map((spelling) => `r11-serve-limits-${budget}-${spelling}.json`)).sort();
  if (JSON.stringify(limitNames) !== JSON.stringify(expectedLimitNames)) fail('serve-limits-inventory', limitNames);
  const limitCases = limitNames.map((name) => {
    const bytes = readFileSync(join(corpusRoot, 'valid', name));
    const request = JSON.parse(bytes.toString('utf8'));
    const tiny = name.includes('-tiny-');
    const maximum = tiny ? 1 : 8388608;
    const expected = name.endsWith('-seq.json') ? [1000, maximum] : { max_output_bytes: maximum, timeout_ms: 1000 };
    if (JSON.stringify(request.limits) !== JSON.stringify(expected)) {
      fail('serve-limits-corpus', name);
    }
    return { name, bytes, tiny, maximum };
  });
  for (const lang of lifecycleLanguages.filter((lane) => ALL.includes(lane))) {
    const adapterArgs = lifecycleAdapters[lang];
    // Positive file exchange: the host-owned --lekalo-request-file
    // coordinate carries a VALID request; the run must complete with a
    // real success envelope (status ok, capabilities present).
    const requestFile = join(fileExchangeDir, `valid-${lang}.json`);
    writeFileSync(requestFile, validDescribe);
    const fileLeg = run(adapterArgs[0], [...adapterArgs.slice(1), "--lekalo-request-file", requestFile], ROOT, { input: Buffer.alloc(0), timeout: 60000 });
    if (fileLeg.status !== 0) {
      fail("lifecycle-file-exchange-valid", `${lang}: a valid request through the file coordinate refused: ${(fileLeg.stderr ?? "").slice(-200)}`);
    }
    let envelope = null;
    try { envelope = JSON.parse((fileLeg.stdout ?? "").trim()); } catch { /* not json */ }
    if (envelope === null || envelope.status !== "ok" || envelope.capabilities === undefined) {
      fail("lifecycle-file-exchange-valid", `${lang}: the file-coordinate exchange did not emit a success capabilities envelope`);
    }
    lifecycleAccounting.fileValidLegs += 1;
    // Both transports × map/seq × PHP default/on/off/-n. Tiny limits
    // must refuse at emission; roomy twins must succeed byte-identically.
    // Probe the real TS serve token / Rust+Go callback token helpers;
    // PHP exposes the same deadline as callback-facing named limits.
    for (const [configuration, invocation] of hygieneInvocations(lang, adapterArgs).entries()) {
      const twins = new Map();
      for (const transport of ['stdin', 'file']) {
        for (const { name, bytes, tiny, maximum } of limitCases) {
          const label = `${lang}/${configuration}/${transport}/${name}`;
          const limitFile = join(fileExchangeDir, `limits-${lang}.json`);
          writeFileSync(limitFile, bytes);
          const argv = [...invocation.slice(1), ...(transport === 'file' ? ['--lekalo-request-file', limitFile] : [])];
          const options = { input: transport === 'file' ? Buffer.from('not-json: file must win') : bytes, encoding: null, timeout: 60000 };
          for (const probe of tiny ? [false] : [false, true]) {
            const leg = run(invocation[0], [...argv, ...(probe ? ['--lekalo-fault', 'limits-probe'] : [])], ROOT, options);
            const stdout = leg.stdout ?? Buffer.alloc(0);
            const stderr = leg.stderr ?? Buffer.alloc(0);
            if (leg.error || leg.status === null || stderr.length > 65536 || /Warning:|Fatal error:|Notice:/.test(stderr.toString('utf8'))) {
              fail('serve-limits-process', label);
            }
            if (tiny) {
              if (leg.status === 0 || stdout.length !== 0) fail('lifecycle-output-limit', `${label}: expected nonzero exit and zero stdout, got ${leg.status}/${stdout.length}`);
              lifecycleAccounting.outputLimitLegs += 1;
              continue;
            }
            let response;
            try { response = JSON.parse(stdout.toString('utf8')); } catch { fail('serve-limits-control', label); }
            if (leg.status !== 0 || response.status !== 'ok' || response.capabilities === undefined || stdout.length > maximum) {
              fail('serve-limits-control', label);
            }
            if (probe) {
              const constraints = response.capabilities.constraints;
              if (constraints?.max_entries !== 1000 || constraints?.max_writes !== maximum) fail('serve-deadline-token', label);
              lifecycleAccounting.deadlineTokenLegs += 1;
            } else lifecycleAccounting.outputLimitControlLegs += 1;
            const key = probe ? 'token' : 'control';
            if (twins.has(key) && !stdout.equals(twins.get(key))) fail('serve-limits-twin-bytes', label);
            twins.set(key, stdout);
            nativeEmissionOracleRows.push({ relative: `emission/limits/${label}/${key}`, bytes: stdout });
          }
        }
      }
    }
    // Hostile-admission stdout hygiene (issue #106 review r6 #2): a
    // request whose typed members are MISSING (not merely wrong) must
    // refuse with ZERO stdout bytes on every runtime — a runtime
    // warning leaking onto the wire surface corrupts the single-JSON
    // stdout contract exactly like the oversize refusal would.
    const hostileMissingMembers = [
      // native file change without `path` and without `change`:
      JSON.stringify({
        operation: "plan-native", project_root: ".", protocol: "lekalo.target/v1",
        protocol_version: "0.3.2", request_id: `req-${"a".repeat(64)}`,
        native_request: {
          capability_snapshot_digest: `sha256:${"c".repeat(64)}`,
          changes: { files: [{ change: "added" }, { path: "src/x.ts" }] },
          execution_policy_ref: { digest: `sha256:${"d".repeat(64)}` },
          input_manifest_digest: `sha256:${"e".repeat(64)}`,
          scan_ref: { digest: `sha256:${"f".repeat(64)}` },
          tool_catalog_digest: `sha256:${"0".repeat(64)}`,
        },
      }),
      // error envelope without `class` + write entry without `action`:
      JSON.stringify({
        evidence: { adapter: { digest: `sha256:${"d".repeat(64)}`, id: "sdk-reference", version: "0.1.0" } },
        error: { code: "boom", message: "m" },
        operation: "scan", protocol: "lekalo.target/v1", protocol_version: "0.3.2",
        request_id: `req-${"a".repeat(64)}`, status: "error",
      }),
      JSON.stringify({
        evidence: { adapter: { digest: `sha256:${"d".repeat(64)}`, id: "sdk-reference", version: "0.1.0" } },
        operation: "generate", protocol: "lekalo.target/v1", protocol_version: "0.3.2",
        request_id: `req-${"a".repeat(64)}`, status: "ok",
        writes: [{ path: "src/a.ts", sha256: `sha256:${"a".repeat(64)}` }],
      }),
    ];
    // Per-struct SHORT-POSITIONAL-SEQ sweep (issue #106 review r8 #4):
    // a seq shorter than the declared fields leaves members ABSENT
    // after the remap, and an unguarded deref leaks a runtime warning
    // onto stdout (the r7 PHP leak class). Every lane must refuse with
    // zero stdout bytes.
    const hostileShortSeqs = [
      // lint binding 3-element seq (the exact r7 serve-level repro): the
      // tail members are absent after the remap;
      JSON.stringify({
        operation: "lint", project_root: ".", protocol: "lekalo.target/v1",
        protocol_version: "0.6.4", request_id: `req-${"b".repeat(64)}`, target: "sdk-reference",
        lint_request: {
          bindings: [["src/domain/user.php", "app-models-user", "user"]],
          files: ["src/domain/user.php"],
          pins: {
            capabilities: ["unknown", null], ir: ["known", `sha256:${"1".repeat(64)}`],
            model: ["known", `sha256:${"2".repeat(64)}`], observed: ["unknown", null],
            profile: ["unknown", null], revision: ["unknown", null],
          },
          scope: ["src"],
        },
      }),
      // lint binding EMPTY seq: every member is absent;
      JSON.stringify({
        operation: "lint", project_root: ".", protocol: "lekalo.target/v1",
        protocol_version: "0.6.4", request_id: `req-${"c".repeat(64)}`, target: "sdk-reference",
        lint_request: {
          bindings: [[]],
          files: ["src/domain/user.php"],
          pins: {
            capabilities: ["unknown", null], ir: ["known", `sha256:${"1".repeat(64)}`],
            model: ["known", `sha256:${"2".repeat(64)}`], observed: ["unknown", null],
            profile: ["unknown", null], revision: ["unknown", null],
          },
          scope: ["src"],
        },
      }),
      // profile capabilities: an EMPTY capability seq leaves id/support absent;
      JSON.stringify({
        operation: "generate", project_root: ".", protocol: "lekalo.target/v1",
        protocol_version: "0.3.2", request_id: `req-${"d".repeat(64)}`, target: "sdk-reference",
        profile: "default", profile_digest: `sha256:${"e".repeat(64)}`,
        profile_capabilities: [[]], dry_run: true, ir_path: ".lekalo/ir/planner.json",
      }),
      // lint pins as a 1-element seq: five pin members are absent;
      JSON.stringify({
        operation: "lint", project_root: ".", protocol: "lekalo.target/v1",
        protocol_version: "0.6.4", request_id: `req-${"f".repeat(64)}`, target: "sdk-reference",
        lint_request: {
          bindings: [], files: [], pins: [["unknown", null]], scope: ["src"],
        },
      }),
      // native_request as a 1-element seq: the nested struct members are
      // absent after the remap;
      JSON.stringify({
        operation: "plan-native", project_root: ".", protocol: "lekalo.target/v1",
        protocol_version: "0.3.2", request_id: `req-${"9".repeat(64)}`,
        native_request: [`sha256:${"0".repeat(64)}`],
      }),
    ];
    const hygieneRows = [...hostileMissingMembers, ...hostileShortSeqs, ...r10HostileRequests];
    for (const invocation of hygieneInvocations(lang, adapterArgs)) {
      for (const [row, hostile] of hygieneRows.entries()) {
        const leg = run(invocation[0], invocation.slice(1), fileExchangeDir, { input: Buffer.from(hostile), timeout: 60000 });
        if (leg.error || (leg.stderr ?? '').length > 8192) fail('lifecycle-stdout-hygiene-process', `${lang}/row-${row}`);
        if ((leg.stdout ?? '').length > 0) {
          fail('lifecycle-stdout-hygiene', `${lang}/row-${row}: refused request leaked stdout bytes`);
        }
        if (leg.status === 0) fail('lifecycle-stdout-hygiene', `${lang}/row-${row}: hostile request admitted`);
        lifecycleAccounting.stdoutHygieneLegs += 1;
      }
    }
    // r10 A: the intact native-plan control must succeed, and forbidden
    // entries/units must NOT become status:ok or exit-0 empty output.
    // Both transports and every PHP warning configuration execute.
    const nativeRequest = readFileSync(join(corpusRoot, 'valid', 'plan-native-request.json'));
    const nativeFile = join(fileExchangeDir, `native-${lang}.json`);
    writeFileSync(nativeFile, nativeRequest);
    for (const [configuration, invocation] of hygieneInvocations(lang, adapterArgs).entries()) {
      for (const transport of ['stdin', 'file']) {
        for (const mutant of ['native-plan-control', 'invalid-native-entries', 'invalid-native-units']) {
          const args = [...invocation.slice(1), '--lekalo-fault', mutant];
          if (transport === 'file') args.push('--lekalo-request-file', nativeFile);
          const leg = run(invocation[0], args, fileExchangeDir, { input: transport === 'file' ? Buffer.alloc(0) : nativeRequest, encoding: null, timeout: 60000 });
          const stdout = leg.stdout ?? Buffer.alloc(0);
          const positive = mutant === 'native-plan-control';
          if (leg.error || (leg.stderr ?? Buffer.alloc(0)).length > 8192) fail('native-emission-process', `${lang}/${mutant}`);
          if (stdout.length === 0) {
            if (positive || leg.status === 0) fail('native-emission-empty', `${lang}/${mutant}`);
          } else {
            let response;
            try { response = JSON.parse(stdout.toString('utf8')); } catch { fail('native-emission-json', `${lang}/${mutant}`); }
            if (response.status !== (positive ? 'ok' : 'error') || (positive && leg.status !== 0)) fail('native-emission-status', `${lang}/${mutant}`);
            if (positive && response.result?.native_plan?.kind !== 'native-plan') fail('native-emission-control', lang);
            if (!positive && response.error?.code !== 'sdk-build-refused') fail('native-emission-mutant', `${lang}/${mutant}: fault did not reach the builder`);
            if (stdout[stdout.length - 1] === 10) fail('native-emission-newline', `${lang}/${mutant}`);
            nativeEmissionOracleRows.push({ relative: `emission-native/${lang}/${configuration}/${transport}/${mutant}`, bytes: stdout });
          }
          lifecycleAccounting.nativePlanBuilderLegs += 1;
        }
      }
    }
    for (const mutant of ["operations", "protocol_versions", "transports", "max_entries", "max_writes", "capability", "identity", "negotiation", "describe"]) {
      trace(`emission ${lang}/${mutant}`);
      const leg = run(adapterArgs[0], [...adapterArgs.slice(1), "--lekalo-fault", `invalid-${mutant}`], fileExchangeDir, { input: Buffer.from(validDescribe), encoding: null, timeout: 60000 });
      const stdout = leg.stdout ?? Buffer.alloc(0);
      const stderr = leg.stderr ?? Buffer.alloc(0);
      if (leg.error || stderr.length > 8192) fail("emission-negative-process", `${lang}/${mutant}: unbounded or failed process`);
      if (stdout.length === 0) {
        if (leg.status === 0) fail("emission-negative-empty", `${lang}/${mutant}: silently succeeded without an envelope`);
      } else {
        let response;
        try { response = JSON.parse(stdout.toString("utf8")); } catch { fail("emission-negative-json", `${lang}/${mutant}`); }
        if (response.status !== "error") fail("emission-negative-success", `${lang}/${mutant}: invalid callback output reached stdout without a closed error envelope`);
        emittedOracleRows.push({ relative: `emission/${lang}/${mutant}`, bytes: stdout, request: Buffer.from(validDescribe) });
      }
      lifecycleAccounting.invalidBuilderLegs += 1;
    }
    const crash = run(adapterArgs[0], [...adapterArgs.slice(1), "--lekalo-fault", "handler-crash"], fileExchangeDir, { input: Buffer.from(validDescribe), encoding: null, timeout: 60000, env: { ...process.env, RUST_BACKTRACE: "0" } });
    const crashStdout = crash.stdout ?? Buffer.alloc(0);
    if (crash.error || (crash.stderr ?? Buffer.alloc(0)).length > 8192) fail("real-crash-bounds", lang);
    if (lang === "typescript") {
      let response;
      try { response = JSON.parse(crashStdout.toString("utf8")); } catch { fail("real-crash-recovery", lang); }
      if (response.status !== "error") fail("real-crash-success", lang);
      emittedOracleRows.push({ relative: `emission/${lang}/handler-crash`, bytes: crashStdout, request: Buffer.from(validDescribe) });
    } else if (crash.status === 0 || crashStdout.length > 0) {
      fail("real-crash-refusal", `${lang}: a real throw/panic must exit nonzero with no stdout`);
    }
    lifecycleAccounting.realCrashLegs += 1;
  }
  const expectedLegs = lifecycleLanguages.filter((lane) => ALL.includes(lane)).length;
  if (lifecycleAccounting.fileValidLegs !== expectedLegs) {
    fail("lifecycle-file-exchange-count", `executed ${lifecycleAccounting.fileValidLegs} valid file legs, expected ${expectedLegs} (one per SDK lane)`);
  }
  const invocationCount = expectedLegs + (lifecycleLanguages.includes('php') ? 3 : 0);
  for (const counter of ['outputLimitLegs', 'outputLimitControlLegs', 'deadlineTokenLegs']) {
    if (lifecycleAccounting[counter] !== invocationCount * 4) {
      fail('serve-limits-count', `${counter}: ${lifecycleAccounting[counter]} vs ${invocationCount * 4} (map/seq, stdin/file, PHP default/on/off/-n)`);
    }
  }
  const expectedHygiene = invocationCount * (8 + r10HostileRequests.length);
  if (lifecycleAccounting.stdoutHygieneLegs !== expectedHygiene) {
    fail('lifecycle-stdout-hygiene-count', `${lifecycleAccounting.stdoutHygieneLegs} vs ${expectedHygiene} (missing/short-seq and every r10 hostile request, PHP default/on/off/-n)`);
  }
  if (lifecycleAccounting.nativePlanBuilderLegs !== invocationCount * 6) {
    fail('native-emission-count', `${lifecycleAccounting.nativePlanBuilderLegs} vs ${invocationCount * 6}`);
  }
}

// Process-level transport vectors (issue #106 review r3 #6): the input
// layer is part of admission. A raw invalid-UTF-8 request must refuse on
// every language (no U+FFFD repair reaches the codec), and a valid
// request whose multibyte character straddles the stdin read-chunk
// boundary must decode intact (chunk-local decoding corrupted it).
const transportDir = mkdtempSync(join(tmpdir(), "adapter-sdk-vectors-transport-"));
process.on("exit", () => {
  try { rmSync(transportDir, { recursive: true, force: true }); } catch { /* best effort */ }
});
const transportAccounting = { invalidBytes: 0, straddle: 0 };
{
  // A lint request whose revision value contains a raw 0xFF byte: the
  // repaired text would pass every length bound, so only byte-exact
  // fatal decoding admits nothing here.
  const validLint = JSON.stringify({
    operation: "lint",
    project_root: ".",
    protocol: "lekalo.target/v1",
    protocol_version: "0.6.4",
    request_id: `req-${"a".repeat(64)}`,
    target: "sdk-reference",
    lint_request: {
      bindings: [],
      files: ["src/main.ts"],
      pins: {
        capabilities: { state: "unknown" },
        ir: { state: "unknown" },
        model: { state: "unknown" },
        observed: { state: "unknown" },
        profile: { state: "unknown" },
        revision: { state: "unknown" },
      },
      scope: ["src"],
    },
  });
  const marker = '"revision":{"state":"unknown"}';
  const index = validLint.indexOf(marker) + Math.floor(marker.length / 2);
  const bytes = Buffer.from(validLint, "utf8");
  const invalidBytes = Buffer.concat([bytes.subarray(0, index), Buffer.from([0xff]), bytes.subarray(index)]);
  for (const lang of languages) {
    const adapterArgs = lifecycleAdapters[lang];
    // Both transports carry the same raw bytes: the host-owned file
    // coordinate (written verbatim, file-wins) and piped stdin.
    const fileFlagPath = join(transportDir, `invalid-${lang}.file.json`);
    writeFileSync(fileFlagPath, invalidBytes);
    const fileLeg = run(adapterArgs[0], [...adapterArgs.slice(1), "--lekalo-request-file", fileFlagPath], ROOT, { input: Buffer.alloc(0), timeout: 60000 });
    if (fileLeg.status === 0) {
      fail("transport-invalid-utf8", `${lang}: a raw invalid-UTF-8 request file was admitted (exit 0, file transport)`);
    }
    const stdinLeg = run(adapterArgs[0], [...adapterArgs.slice(1)], ROOT, { input: invalidBytes, timeout: 60000 });
    if (stdinLeg.status === 0) {
      fail("transport-invalid-utf8", `${lang}: a raw invalid-UTF-8 request stream was admitted (exit 0, stdin transport)`);
    }
    transportAccounting.invalidBytes += 2;
  }
  // Boundary-straddle positive control: native_request.changes.symbols
  // is the request member class with multibyte capacity (bounded
  // strings, no charset restriction), so a valid plan-native request can
  // carry a two-byte character whose bytes split across the stdin
  // read-chunk boundary. A chunk-local decoder corrupts the sequence and
  // refuses a valid request; a streaming decoder decodes it intact.
  const symbols = [];
  const render = (list) => JSON.stringify({
    operation: "plan-native",
    project_root: ".",
    protocol: "lekalo.target/v1",
    protocol_version: "0.3.2",
    request_id: `req-${"b".repeat(64)}`,
    native_request: {
      capability_snapshot_digest: `sha256:${"c".repeat(64)}`,
      changes: { files: [], symbols: list },
      execution_policy_ref: { digest: `sha256:${"d".repeat(64)}` },
      input_manifest_digest: `sha256:${"e".repeat(64)}`,
      scan_ref: { digest: `sha256:${"f".repeat(64)}` },
      tool_catalog_digest: `sha256:${"0".repeat(64)}`,
    },
  });
  // Grow with the largest legal filler symbols until the remaining gap
  // fits one final symbol, then size that final symbol (still within its
  // 128-character bound) so the first byte of the two-byte sequence is
  // the last byte of chunk one and its second byte opens chunk two. The
  // value of an appended symbol starts two bytes past the symbols
  // array's closing bracket (the last `]` in the document: the bracket
  // position becomes the separator + opening quote).
  const valueOffsetFor = (list) => render(list).lastIndexOf("]") + 2;
  let gap = 65_535 - valueOffsetFor(symbols);
  while (gap > 127) {
    symbols.push("s".repeat(Math.min(128, gap - 4)));
    gap = 65_535 - valueOffsetFor(symbols);
  }
  if (gap < 1 || gap > 127) {
    fail("transport-straddle-setup", `cannot place the boundary character: gap ${gap}`);
  }
  symbols.push("x".repeat(gap) + "é");
  const text = render(symbols);
  const finalOffset = Buffer.byteLength(text.slice(0, text.indexOf("é")));
  if (finalOffset !== 65_535) {
    fail("transport-straddle-setup", `expected the multibyte character at byte 65535, got ${finalOffset}`);
  }
  for (const lang of languages) {
    const adapterArgs = lifecycleAdapters[lang];
    const result = run(adapterArgs[0], [...adapterArgs.slice(1)], ROOT, { input: Buffer.from(text, "utf8"), timeout: 60000 });
    if (result.status !== 0) {
      fail("transport-straddle", `${lang}: a valid request with a chunk-boundary-straddling multibyte character was refused: ${(result.stderr ?? "").slice(-200)}`);
    }
    transportAccounting.straddle += 1;
  }
}

// Production admission oracle (issue #106 review r2 #10, r3 #8, r4 #8):
// the FULL corpus — requests AND responses, including the semantic
// vectors' embedded responses — replays through the production wire
// decoders. Framing is derived from the ORIGINAL file Buffers (hex), so
// numeric spellings, duplicate keys, CR/LF, control bytes, and
// genuinely invalid UTF-8 reach production unchanged — the r4 gate read
// corpus files as utf8 strings and re-encoded them, replacing every
// invalid byte before the decoder ever saw it. The row set is an
// EXPLICIT corpus-derived inventory: every case names the production
// layer it exercises, exclusions are justified inline, and the executed
// rows must equal the inventory exactly.
const oracle = join(ROOT, "target", "debug", process.platform === "win32" ? "adapter-sdk-oracle.exe" : "adapter-sdk-oracle");
{
  // Never certify sources with a stale already-present oracle binary.
  const build = run("cargo", ["build", "--locked", "-p", "lekalo-cli", "--bin", "adapter-sdk-oracle"]);
  if (build.status !== 0) fail("oracle-missing", (build.stderr ?? "").slice(-400));
}
// Inventory entries: {relative → {bytes, mode, accept, reason?, compareReason?}}.
// mode "B" = the production request decoder; "V" = the production
// response decoder. No cross-family dedup: the per-family accounting
// certifies that every declared row actually reached production.
const oracleInventory = new Map();
const pushRow = (relative, bytes, mode, accept, reason, compareReason = false) => {
  if (oracleInventory.has(relative)) fail("oracle-inventory-duplicate", relative);
  oracleInventory.set(relative, { bytes, mode, accept, reason, compareReason });
};
const pushClientRow = (relative, request, response, expect) => {
  pushRow(relative, request, "C", expect.accepted, expect.reason, true);
  oracleInventory.get(relative).responseBytes = response;
};
const readCorpusBytes = (relative) => readFileSync(join(corpusRoot, relative));
for (const name of corpusFiles("valid")) {
  const bytes = readCorpusBytes(`valid/${name}`);
  // Valid rows are well-formed UTF-8; the response/request split is the
  // same envelope-shape check the runners use.
  const text = bytes.toString("utf8");
  const document = JSON.parse(text);
  // Response routing covers the positional-seq envelope spelling too
  // (issue #106 review r6 #1): a top-level array whose declared status
  // position carries a response status is a response document.
  const isResponse = document.status !== undefined
    || (Array.isArray(document) && ["ok", "error"].includes(document[4]) && typeof document[2] === "string");
  if (isResponse) {
    pushRow(`valid/${name}`, bytes, "V", true, undefined); // response corpus: production must admit
  } else {
    pushRow(`valid/${name}`, bytes, "B", true, undefined);
  }
}
for (const name of corpusFiles("invalid")) {
  // RAW BYTES from disk (issue #106 review r4 #8): the invalid-utf8 row
  // must reach production byte-for-byte.
  const bytes = readCorpusBytes(`invalid/${name}`);
  const expect = JSON.parse(readFileSync(join(corpusRoot, "invalid", name.replace(/\.json$/, ".expect.json")), "utf8"));
  const isResponse = bytes.toString("utf8").includes('"status"') && bytes.toString("utf8").includes('"evidence"');
  if (isResponse) {
    // Response corpus: production must refuse, unless the expect row
    // explicitly records the reviewed safe-direction divergence where
    // the SDK is stricter at emission than the production wire decoder
    // (e.g. the native-plan digest spelling, which production's typed
    // wire decode accepts and its client rejects downstream).
    pushRow(`invalid/${name}`, bytes, "V", expect.productionAccepts === true, undefined);
    continue;
  }
  const isRequest = !expect.stage || expect.stage !== "none";
  if (!isRequest) continue;
  // Decode-stage refusals (shape/oversize/token/...) bind production
  // refusal vocabulary; validate-stage rows are semantic (production
  // accepts the wire shape and refuses in its own validate pass with
  // the same reason vocabulary).
  pushRow(`invalid/${name}`, bytes, "B", false, expect.reason);
}
const semanticWithResponse = [];
const semanticWithClientContext = [];
for (const name of corpusFiles("semantic")) {
  const vector = JSON.parse(readFileSync(join(corpusRoot, "semantic", name), "utf8"));
  // Request-bearing semantic rows: the runners replay
  // JSON.stringify(vector.request) bytes, so production must admit the
  // same bytes — these vectors pin client-layer semantics (pairing,
  // evidence depth), not wire shape, and a production refusal here is a
  // parity hole.
  if (vector.request !== undefined) {
    pushRow(`semantic/${name}`, Buffer.from(JSON.stringify(vector.request), "utf8"), "B", true, undefined);
  }
  // Response-bearing semantic rows (issue #106 review r4 #8): the
  // embedded response replays through the production RESPONSE decoder.
  // The default expectation is WIRE ADMISSIBILITY (the wire decoder is
  // the layer this oracle exercises): `accept` vectors and
  // semantic-only rejections (pairing, echo, plan-id, span uniqueness)
  // are wire-admissible, so they default to accept=true; the four
  // lint-evidence rows whose bytes production's wire decoder itself
  // refuses declare productionResponseAccepts:false. Vectors with
  // neither request nor response (canonical-plan, artifact-manifest,
  // scenario-result, decode-oversize text rows) carry no wire document
  // and are excluded from the oracle inventory by construction.
  if (vector.response !== undefined) {
    semanticWithResponse.push(name);
    const accept = vector.productionResponseAccepts !== undefined
      ? vector.productionResponseAccepts === true
      : true;
    const responseBytes = Buffer.from(JSON.stringify(vector.response), "utf8");
    pushRow(`semantic/${name}#response`, responseBytes, "V", accept, undefined);
    if (vector.productionClientExpect !== undefined) {
      if (vector.request?.operation !== "describe") fail("oracle-client-context", `${name}: C mode qualifies real describe sessions only`);
      semanticWithClientContext.push(name);
      pushClientRow(`semantic/${name}#client`, Buffer.from(JSON.stringify(vector.request)), responseBytes, vector.productionClientExpect);
    }
  }
}
for (const name of corpusFiles("upgrade")) {
  const row = JSON.parse(readFileSync(join(corpusRoot, "upgrade", name), "utf8"));
  // Raw-byte rows replay the exact fixture spelling (rawRequest);
  // object rows replay the same JSON.stringify bytes the runners use.
  const bytes = row.rawRequest !== undefined
    ? Buffer.from(row.rawRequest, "utf8")
    : Buffer.from(JSON.stringify(row.request), "utf8");
  pushRow(`upgrade/${name}`, bytes, "B", row.accept === true, row.reason, true);
}
for (const row of emittedOracleRows) {
  pushRow(row.relative, row.bytes, "V", true);
  pushClientRow(`${row.relative}#client`, row.request, row.bytes, { accepted: false, reason: "operation-failed" });
}
for (const row of nativeEmissionOracleRows) pushRow(row.relative, row.bytes, 'V', true);
const oracleRows = [...oracleInventory.entries()].map(([relative, entry]) => ({
  relative,
  hex: entry.bytes.toString("hex"),
  responseHex: entry.responseBytes?.toString("hex"),
  mode: entry.mode,
  accept: entry.accept,
  reason: entry.reason,
  compareReason: entry.compareReason,
}));
const oracleInput = oracleRows.map((row) => `${row.mode}\t${row.hex}${row.mode === "C" ? `\t${row.responseHex}` : ""}`).join("\n") + "\n";
const oracleRun = spawnSync(oracle, [], { input: oracleInput, encoding: "utf8", timeout: 300_000, shell: process.platform === "win32" });
if (oracleRun.status !== 0) fail("oracle-failed", (oracleRun.stderr ?? "").slice(-300));
const oracleVerdicts = (oracleRun.stdout ?? "").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
if (oracleVerdicts.length !== oracleRows.length) fail("oracle-row-count", `${oracleVerdicts.length} vs ${oracleRows.length}`);
let oracleChecked = 0;
const oracleAccounting = { requests: 0, responses: 0, semanticRequests: 0, semanticResponses: 0, clientContexts: 0, semanticClientContexts: 0 };
// The semantic-response gap stays pinned (issue #106 review r4 #8):
// every semantic vector that embeds a response must produce exactly one
// response oracle row — the r4 gate replayed zero of the fifteen.
if (semanticWithResponse.length === 0) {
  fail("oracle-semantic-responses-empty", "no semantic vector embeds a response");
}
oracleRows.forEach((row, index) => {
  const verdict = oracleVerdicts[index];
  oracleChecked += 1;
  if (row.mode === "C") {
    oracleAccounting.clientContexts += 1;
    if (row.relative.startsWith("semantic/")) oracleAccounting.semanticClientContexts += 1;
  } else if (row.mode === "V") {
    oracleAccounting.responses += 1;
    if (row.relative.startsWith("semantic/")) oracleAccounting.semanticResponses += 1;
  } else if (row.relative.startsWith("semantic/")) {
    oracleAccounting.semanticRequests += 1;
  } else {
    oracleAccounting.requests += 1;
  }
  if (verdict.accepted !== row.accept) {
    fail("oracle-disagreement", `${row.relative}: production ${verdict.accepted ? "accepted" : `refused (${verdict.reason})`}, corpus declares accept=${row.accept}`);
  }
  // Reason equality binds only where the corpus declares the production
  // refusal vocabulary (the upgrade rows); the invalid/ rows carry the
  // SDK runner's stage/reason spellings (bounds vs shape), whose
  // accept/refuse verdict is the parity contract. Tokens are advisory
  // across admission layers (sdk/spec.md section 4); only explicitly
  // marked productionAccepts rows may be stricter than production.
  if (!verdict.accepted && row.compareReason && row.reason !== undefined && row.reason !== verdict.reason) {
    fail("oracle-reason", `${row.relative}: production refused with ${verdict.reason}, corpus declares ${row.reason}`);
  }
});
if (oracleAccounting.semanticResponses !== semanticWithResponse.length) {
  fail("oracle-semantic-responses", `replayed ${oracleAccounting.semanticResponses} semantic responses, the corpus embeds ${semanticWithResponse.length}`);
}
if (semanticWithClientContext.length === 0 || oracleAccounting.semanticClientContexts !== semanticWithClientContext.length) {
  fail("oracle-client-context-accounting", "every annotated real-client semantic row must execute");
}
if (oracleAccounting.clientContexts !== semanticWithClientContext.length + emittedOracleRows.length) {
  fail("oracle-emission-client-accounting", "every emitted error envelope must reach the actual client");
}
// Byte-preservation regression (issue #106 review r4 #8): the invalid
// UTF-8 row's raw 0xFF must appear in the framed hex fed to production.
{
  const row = oracleRows.find((candidate) => candidate.relative === "invalid/request-invalid-utf8.json");
  if (!row || !row.hex.includes("ff")) {
    fail("oracle-byte-preservation", "the invalid-utf8 corpus row did not frame its raw bytes");
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "adapter-sdk-vectors",
  languages: Object.keys(verdicts).sort(),
  totals,
  lifecycle: lifecycleAccounting,
  transport: transportAccounting,
  oracle: { rows: oracleChecked, ...oracleAccounting },
})}\n`);

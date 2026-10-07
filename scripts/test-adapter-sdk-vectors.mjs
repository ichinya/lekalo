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
 * Since the round-1 review (issue #106) the gate also enforces:
 *   - the complete declared family set (config.testVectorFamilies): a
 *     runner that skips a family, or a corpus directory that loses its
 *     files, fails here instead of silently shrinking the comparison;
 *   - the lifecycle recipes execute through the real reference adapters
 *     (typescript + rust) with per-recipe terminal expectations;
 *   - production admission agreement: the upgrade rows replay through
 *     the lekalo-core wire decoder (the production oracle binary) and
 *     must accept/refuse exactly like the generated matrix.
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
import { existsSync, readFileSync, readdirSync, mkdtempSync, rmSync } from "node:fs";
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

const RUNNERS = {
  go: { command: "go", args: ["run", "./internal/vectorsrunner"], cwd: join(ROOT, "sdk/go"), label: "go run ./internal/vectorsrunner" },
  php: { command: "php", args: ["test/vectors.php"], cwd: join(ROOT, "sdk/php"), label: "php test/vectors.php" },
  rust: { command: "cargo", args: ["run", "--quiet", "--bin", "vectors"], cwd: join(ROOT, "sdk/rust"), label: "cargo run --bin vectors" },
  typescript: { command: process.execPath, args: ["test/vectors.mjs"], cwd: join(ROOT, "sdk/typescript"), label: "node test/vectors.mjs" },
};

const run = (program, args_, cwd) =>
  spawnSync(program, args_, { cwd: cwd ?? ROOT, encoding: "utf8", timeout: 600_000, shell: process.platform === "win32" && program !== process.execPath });

const verdicts = {};
const missing = [];
for (const lang of languages) {
  const runner = RUNNERS[lang];
  if (!existsSync(join(ROOT, "sdk", lang))) {
    missing.push(lang);
    continue;
  }
  const run = spawnSync(runner.command, runner.args, { cwd: runner.cwd, encoding: "utf8", timeout: 600_000, input: "", shell: process.platform === "win32" });
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

// Cross-language canonical byte comparison: every language must emit the
// exact same canonical bytes for the same values.
const baseline = Object.entries(verdicts)[0][1].canonicalOutputs ?? {};
for (const [lang, verdict] of Object.entries(verdicts)) {
  const outputs = verdict.canonicalOutputs ?? {};
  for (const [name, bytes] of Object.entries(baseline)) {
    if (outputs[name] !== undefined && outputs[name] !== bytes) {
      fail("canonical-bytes", `${lang} emitted different bytes for ${name}`);
    }
  }
}

const totals = Object.fromEntries(Object.entries(verdicts).map(([lang, verdict]) => [lang, verdict.total]));

// Complete family coverage: every declared family must exist in the
// corpus and be executed by every runner (issue #106 review: a runner
// that silently skipped the lifecycle family still printed a green
// verdict, so "141/141" certified a hole).
const config = JSON.parse(readFileSync(join(ROOT, "sdk/schema-inputs.json"), "utf8"));
const corpusRoot = join(ROOT, "tests/fixtures/adapter-sdk");
const FAMILY_DIRECTORIES = {
  canonical: ["canonical"],
  codec: ["valid", "invalid"],
  semantic: ["semantic"],
  upgrade: ["upgrade"],
};
for (const family of config.testVectorFamilies) {
  const directories = FAMILY_DIRECTORIES[family];
  if (!directories) fail("family-mapping", family);
  for (const directory of directories) {
    const dir = join(corpusRoot, directory);
    if (!existsSync(dir)) fail("family-directory-missing", directory);
    const files = readdirSync(dir).filter((name) => name.endsWith(".json") && !name.endsWith(".expect.json"));
    if (files.length === 0) fail("family-directory-empty", directory);
  }
  for (const [lang, verdict] of Object.entries(verdicts)) {
    const executed = verdict.executedFamilies ?? [];
    if (!executed.includes(family)) {
      fail("family-not-executed", `${lang}: ${family} absent from the runner's executed families`);
    }
    if (verdict.executedVectors?.[family] === 0) {
      fail("family-empty", `${lang}: ${family} executed zero vectors`);
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
 * (issue #106 review: the lifecycle recipes are executed for real, and
 * each declared terminal maps to the observable battery outcome).
 * Classes: pass = the run held; process = a process-level exchange could
 * not complete; response-invalid = the exchange completed but the wire
 * surface failed (protocol/feature classes); extra-write/dry-run = the
 * confinement classes surface as feature/process failures. The key
 * property under test: a recipe that must fail NEVER reports pass. */
const expectedTerminals = {
  ok: (report) => report.verdict === "pass",
  "process-failure": (report) => report.verdict === "process" || report.checks.some((entry) => entry.state === "fail"),
  "response-invalid": (report) => report.verdict !== "pass" && report.checks.some((entry) => entry.state === "fail"),
  "output-limit": (report) => report.verdict !== "pass" && report.checks.some((entry) => entry.state === "fail"),
  timeout: (report) => report.verdict === "process" || report.checks.some((entry) => entry.state === "fail"),
  "dry-run-mutation": (report) => report.verdict !== "pass" && report.checks.some((entry) => entry.state === "fail"),
  "extra-write": (report) => report.verdict !== "pass" && report.checks.some((entry) => entry.state === "fail"),
};

const lifecycleAdapters = {
  typescript: ["node", "tests/fixtures/target-protocol/fake-adapter.mjs", "--lekalo-adapter-variant", "fluent"],
  rust: ["node", "tests/fixtures/target-protocol/fake-adapter.mjs", "--lekalo-adapter-variant", "fluent"],
};
/** adapterMode → the control adapter's --lekalo-fault knob (null = none).
 * The mapping preserves each recipe's SEMANTIC with the control's closed
 * fault vocabulary: a hostile recipe must never report pass, a
 * cooperative recipe must report pass. The oversize mode has no control
 * fault (the control cannot emit an >8MB envelope), so its leg asserts
 * the negative direction: the hostile noise probe never passes. */
const MODE_FAULTS = {
  cooperative: null,
  "silent-exit": "crash",
  "stdout-noise": "garbage",
  "stderr-verbose": null, // bounded logging must not disturb a good run
  slow: "hang",
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
for (const lang of lifecycleLanguages) {
  const adapterArgs = [...lifecycleAdapters[lang]];
  for (const name of lifecycleNames) {
    const recipe = JSON.parse(readFileSync(join(lifecycleDir, name), "utf8"));
    const fault = MODE_FAULTS[recipe.adapterMode];
    const argv = [...adapterArgs];
    if (fault) argv.push("--lekalo-fault", fault);
    const battery = run(hostCli, [
      "adapter", "test",
      "--profile", "strict",
      "--report", "json",
      "--repeats", "1",
      "--timeout-ms", recipe.adapterMode === "slow" ? "3000" : "10000",
      ...argv,
    ]);
    let report = null;
    const stdout = battery.stdout ?? "";
    const start = stdout.indexOf("{");
    if (start !== -1) {
      try { report = JSON.parse(stdout.slice(start, stdout.lastIndexOf("}") + 1)).report; } catch { report = null; }
    }
    const expect = recipe.expectedTerminal;
    const classify = expectedTerminals[expect];
    if (report === null) {
      // No report at all: only the process-failure class accepts this.
      if (expect !== "process-failure") fail("lifecycle-terminal", `${lang}/${recipe.case}: expected ${expect}, battery produced no report (${battery.status})`);
    } else if (!classify || !classify(report)) {
      fail("lifecycle-terminal", `${lang}/${recipe.case}: expected ${expect}, verdict ${report.verdict}, failing: ${JSON.stringify((report.checks ?? []).filter((c) => c.state === "fail").map((c) => [c.id, c.detail]).slice(0, 3))}`);
    }
  }
}

// Production admission oracle (issue #106 review): every upgrade row must
// be accepted/refused by the production wire decoder exactly as the
// corpus declares. The oracle is built in this job from lekalo-core —
// the SDK codecs may never drift from production admission.
const oracle = join(ROOT, "target", "debug", process.platform === "win32" ? "adapter-sdk-oracle.exe" : "adapter-sdk-oracle");
if (!existsSync(oracle)) {
  const build = run("cargo", ["build", "--locked", "-p", "lekalo-cli", "--bin", "adapter-sdk-oracle"]);
  if (build.status !== 0) fail("oracle-missing", (build.stderr ?? "").slice(-400));
}
const upgradeLines = lifecycleNames && readdirSync(join(corpusRoot, "upgrade"))
  .filter((name) => name.endsWith(".json") && !name.endsWith(".expect.json"))
  .sort()
  .map((name) => JSON.stringify(JSON.parse(readFileSync(join(corpusRoot, "upgrade", name), "utf8"))));
const oracleRun = spawnSync(oracle, [], { input: upgradeLines.join("\n") + "\n", encoding: "utf8", timeout: 300_000, shell: process.platform === "win32" });
if (oracleRun.status !== 0) fail("oracle-failed", (oracleRun.stderr ?? "").slice(-300));
const oracleVerdicts = (oracleRun.stdout ?? "").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
if (oracleVerdicts.length !== upgradeLines.length) fail("oracle-row-count", `${oracleVerdicts.length} vs ${upgradeLines.length}`);
const upgradeNames = readdirSync(join(corpusRoot, "upgrade")).filter((name) => name.endsWith(".json")).sort();
upgradeNames.forEach((name, index) => {
  const row = JSON.parse(readFileSync(join(corpusRoot, "upgrade", name), "utf8"));
  const verdict = oracleVerdicts[index];
  if (verdict.accepted !== (row.accept === true)) {
    fail("oracle-disagreement", `${name}: production ${verdict.accepted ? "accepted" : `refused (${verdict.reason})`}, corpus declares accept=${row.accept}`);
  }
  if (!verdict.accepted && row.reason !== undefined && row.reason !== verdict.reason) {
    fail("oracle-reason", `${name}: production refused with ${verdict.reason}, corpus declares ${row.reason}`);
  }
});

process.stdout.write(`${JSON.stringify({
  ok: true,
  gate: "adapter-sdk-vectors",
  languages: Object.keys(verdicts).sort(),
  totals,
  lifecycle: lifecycleNames.length,
  oracle: upgradeNames.length,
})}\n`);

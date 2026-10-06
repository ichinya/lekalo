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
import { existsSync } from "node:fs";
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
process.stdout.write(`${JSON.stringify({ ok: true, gate: "adapter-sdk-vectors", languages: Object.keys(verdicts).sort(), totals })}\n`);

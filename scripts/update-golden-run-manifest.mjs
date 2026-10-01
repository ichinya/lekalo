#!/usr/bin/env node
// Suite-v1 run-manifest writer (issue #90, AC1).
//
// Executes every suite case once (the cold-1 lane) in a fresh external
// sandbox and writes the reviewed run manifest under
// tests/fixtures/suite/v1/run-manifest.json. The determinism gate
// re-executes all three lanes and refuses any drift from this pinned
// manifest. Deterministic; run through the reviewed update flow.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "..");
const {
  SUITE_V1,
  FIXTURE_SCHEMA,
  loadCatalog,
  loadCases,
  sha256,
  failGate,
  passGate,
} = await import(new URL(`file:///${join(here, "lib/fixture-catalog.mjs").split("\\").join("/")}`).href);

const { catalog, errors: catalogErrors } = loadCatalog();
if (catalogErrors.length > 0) failGate("golden-run-manifest", catalogErrors);
const { cases, errors: caseErrors } = loadCases(catalog);
if (caseErrors.length > 0) failGate("golden-run-manifest", caseErrors);

const binary = join(repoRoot, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(binary)) {
  failGate("golden-run-manifest", [{ reason: "binary-missing", hint: "cargo build -p lekalo-cli --locked" }]);
}

const root = mkdtempSync(join(tmpdir(), "lekalo-golden-manifest-"));
const outcomes = [];
try {
  for (const entry of cases) {
    const d = entry.descriptor;
    const sandbox = join(root, entry.id.replaceAll(".", "-"));
    mkdirSync(sandbox, { recursive: true });
    const caseDirLogical = entry.path.split("/").slice(0, -1).join("/");
    for (const input of (d.inputs ?? []).filter((row) => row.role === "project")) {
      const suffix = input.path.startsWith(`${caseDirLogical}/`)
        ? input.path.slice(caseDirLogical.length + 1)
        : null;
      const source = suffix ? join(repoRoot, caseDirLogical, suffix) : join(repoRoot, input.path);
      const name = suffix ? suffix.split("/").join("-") : "project";
      cpSync(source, join(sandbox, name), { recursive: true });
      const role = suffix === "trigger" ? "trigger" : suffix === "non-trigger" ? "non-trigger" : input.role;
      const want = role === "trigger"
        ? { status: "invalid", exit: 1, reasonCodes: d.expectation?.reasonCodes ?? [] }
        : role === "non-trigger"
          ? { status: "valid", exit: 0, reasonCodes: [] }
          : {
              status: d.expectation?.status ?? "valid",
              exit: d.expectation?.exit ?? 0,
              reasonCodes: d.expectation?.reasonCodes ?? [],
            };
      const result = spawnSync(
        binary,
        ["--no-cache", "validate", "--json", "--project", name],
        { cwd: sandbox, encoding: "utf8", timeout: d.timeoutMs ?? catalog.defaultTimeoutMs ?? 60000 },
      );
      const text = ((result.stdout ?? "") + (result.stderr ?? "")).trim();
      let envelope;
      try { envelope = JSON.parse(text); } catch {
        failGate("golden-run-manifest", [{ reason: "unparseable-envelope", caseId: entry.id, text: text.slice(0, 120) }]);
      }
      outcomes.push({
        caseId: entry.id,
        revision: entry.revision,
        status: envelope.status,
        exit: result.status,
        outputDigest: sha256(text),
        reasonCodes: envelope.reasonCodes ?? [],
      });
      // Refuse to pin an expectation-violating row.
      const codes = envelope.reasonCodes ?? [];
      const ok = envelope.status === want.status
        && result.status === want.exit
        && (want.reasonCodes.length === 0
          ? codes.length === 0
          : codes.length >= 1 && codes.includes(want.reasonCodes[0]));
      if (!ok) {
        failGate("golden-run-manifest", [{ reason: "expectation-violation", caseId: entry.id, project: name, got: { status: envelope.status, exit: result.status, reasonCodes: codes }, want }]);
      }
    }
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}

const manifest = {
  manifestId: "dev.lekalo.fixture-run-manifest",
  fixtureSchema: FIXTURE_SCHEMA,
  startedAtPolicy: "deterministic-logical-lane",
  platformPolicy: { normalizePaths: "logical-repository-relative", normalizeNewlines: "lf-only" },
  runs: [{ lane: "cold-1", rootPolicy: "independent-sandbox", outcomes }],
};
writeFileSync(
  join(repoRoot, SUITE_V1, "run-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
passGate("golden-run-manifest", { cases: cases.length, rows: outcomes.length });

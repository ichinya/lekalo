#!/usr/bin/env node
// Suite-v1 run-manifest writer (issue #90, AC1; fix round 2).
//
// Executes every suite case in two independent cold lanes (fresh
// external sandboxes, native-realpath resolved), requires the lanes to
// agree row-for-row including the per-stream digests, and writes the
// reviewed two-lane run manifest under
// tests/fixtures/suite/v1/run-manifest.json. The determinism gate
// re-executes all three of its lanes live and refuses any drift from
// this pinned manifest (its warm lane plus the committed cold lanes).
// Deterministic; run through the reviewed update flow.

import { spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
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

const binary = process.env.LEKALO_BIN
  ?? join(repoRoot, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!existsSync(binary)) {
  failGate("golden-run-manifest", [{ reason: "binary-missing", hint: "cargo build -p lekalo-cli --locked" }]);
}

const root = realpathSync.native(mkdtempSync(join(tmpdir(), "lekalo-golden-manifest-")));
const LANES = ["cold-1", "cold-2"];
const laneOutcomes = new Map();

try {
  for (const lane of LANES) {
    const laneRoot = join(root, lane);
    mkdirSync(laneRoot, { recursive: true });
    const outcomes = [];
    for (const entry of cases) {
      const d = entry.descriptor;
      const sandbox = join(laneRoot, entry.id.replaceAll(".", "-"));
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
          ? {
              status: d.expectation?.status ?? "invalid",
              exit: d.expectation?.exit ?? 1,
              reasonCodes: d.expectation?.reasonCodes ?? [],
            }
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
        // The envelope lives on stdout for valid outcomes and stderr for
        // invalid ones; parse whichever stream carries it.
        const parseText = ((result.stdout ?? "") + (result.stderr ?? "")).trim();
        let envelope;
        try { envelope = JSON.parse(parseText); } catch {
          failGate("golden-run-manifest", [{ reason: "unparseable-envelope", caseId: entry.id, text: parseText.slice(0, 120) }]);
        }
        const row = {
          caseId: entry.id,
          revision: entry.revision,
          status: envelope.status,
          exit: result.status,
          outputDigest: sha256(Buffer.from(result.stdout ?? "", "utf8")),
          reasonCodes: envelope.reasonCodes ?? [],
        };
        outcomes.push(row);
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
    laneOutcomes.set(lane, outcomes);
  }

  // Lane equality: the manifest only pins rows both lanes agree on.
  const a = laneOutcomes.get("cold-1");
  const b = laneOutcomes.get("cold-2");
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) {
    const rowA = JSON.stringify(a[i] ?? null);
    const rowB = JSON.stringify(b[i] ?? null);
    if (rowA !== rowB) {
      failGate("golden-run-manifest", [{ reason: "lane-divergence", row: i, cold1: rowA, cold2: rowB }]);
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
  runs: LANES.map((lane) => ({ lane, rootPolicy: "independent-sandbox", outcomes: laneOutcomes.get(lane) })),
};
writeFileSync(
  join(repoRoot, SUITE_V1, "run-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
passGate("golden-run-manifest", { cases: cases.length, rows: laneOutcomes.get("cold-1").length, lanes: LANES });

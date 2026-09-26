#!/usr/bin/env node
// Leak-corpus gate (issue #119, plan S8).
//
// `tests/fixtures/privacy-leaks/` is the synthetic secret/PII/path/
// private-id corpus. For every sample the gate requires:
// 1. the redaction diff to name every declared leak class (via the
//    read-only `lekalo privacy redact` contract, with the sample's
//    declared repository/protected terms as subject inputs);
// 2. an export attempt of the sample as an allowed synthetic fixture
//    to refuse with the exact `leak.<class>` codes - a residual leak
//    never silently ships;
// 3. the clean sample to pass with an empty diff and a successful
//    export.
// Fail-closed: any mismatch, unexpected residual, or missing refusal
// fails the gate. Every corpus value is a synthetic marker.

import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, relative as pathRelative } from "node:path";
import { fileURLToPath } from "node:url";
import { buildLekaloBinary, runLekalo } from "./privacy-runtime-helpers.mjs";

const rootDir = new URL("../", import.meta.url);
const binary = buildLekaloBinary();
// The CLI's startup alias check rejects non-canonical cwd spellings
// (8.3 short names on Windows runners, /var->/private/var on macOS).
const temp = await realpath(await mkdtemp(join(tmpdir(), "lekalo-privacy-leak-corpus-")));
const project = join(temp, "project");
await mkdir(join(project, ".lekalo"), { recursive: true });

const corpus = JSON.parse(
  await readFile(new URL("../tests/fixtures/privacy-leaks/manifest.json", import.meta.url), "utf8"),
);
assert.equal(corpus.corpusId, "dev.lekalo.privacy-leak-corpus", "corpus identity");
assert.equal(corpus.origin, "synthetic", "the corpus is synthetic");
assert.ok(Array.isArray(corpus.samples) && corpus.samples.length > 0, "samples");

let cases = 0;
// The project-local synthetic fixture family (fix round 2, C-F2):
// corpus envelopes claim `synthetic: true`, which is honored only
// under a declared family.
const FAMILY = "leak-corpus";
await mkdir(join(project, "tests", "fixtures", FAMILY), { recursive: true });
await writeFile(
  join(project, "tests", "fixtures", "fixture-provenance.json"),
  JSON.stringify({
    manifestId: "dev.lekalo.fixture-provenance",
    version: "0.1.0",
    families: [{ family: FAMILY, origin: "synthetic" }],
  }),
);
for (const sample of corpus.samples) {
  const payloadPath = fileURLToPath(new URL(".." + "/tests/fixtures/privacy-leaks/" + sample.file, import.meta.url));
  const text = await readFile(payloadPath, "utf8");

  // 1. The redaction diff names every declared class.
  const args = ["privacy", "redact", "--payload", relativePath(payloadPath)];
  if (sample.repository) args.push("--repository", sample.repository);
  for (const term of sample.protectedTerms ?? []) args.push("--term", term);
  const redactRun = runLekalo(binary, args, { cwd: temp });
  assert.equal(redactRun.status, 0, `${sample.file}: redact must succeed: ${redactRun.stderr}`);
  const report = JSON.parse(redactRun.stdout);
  const kinds = report.diff.map((finding) => finding.kind);
  for (const declared of sample.declaredClasses) {
    assert.ok(kinds.includes(declared), `${sample.file}: ${declared} must appear in the diff (${kinds.join(", ")})`);
  }
  if (sample.declaredClasses.length === 0) {
    assert.equal(report.diff.length, 0, `${sample.file}: the clean sample has no findings`);
  }

  // 2. The export attempt refuses with the exact leak codes.
  const artifactPath = join(
    project,
    "tests",
    "fixtures",
    FAMILY,
    sample.file.replace(/\.txt$/, ".json"),
  );
  await writeFile(
    artifactPath,
    JSON.stringify({
      artifactKind: "fixture",
      payload: text,
      class: ["public"],
      synthetic: true,
      ...(sample.repository ? { repository: sample.repository } : {}),
      ...(sample.protectedTerms ? { protectedTerms: sample.protectedTerms } : {}),
    }),
  );
  const exportRun = runLekalo(
    binary,
    ["privacy", "export", relativePath(artifactPath), "--destination", "publish", "--project", "project"],
    { cwd: temp },
  );
  if (sample.refusesExport) {
    assert.equal(exportRun.status, 3, `${sample.file}: the leak must refuse the export: ${exportRun.stdout}`);
    const refusal = JSON.parse(exportRun.stdout);
    assert.equal(refusal.status, "refused-leaks");
    for (const declared of sample.declaredClasses) {
      assert.ok(
        refusal.reasonCodes.includes(`leak.${declared}`),
        `${sample.file}: leak.${declared} must refuse (${refusal.reasonCodes.join(", ")})`,
      );
    }
  } else {
    assert.equal(exportRun.status, 0, `${sample.file}: the clean sample ships: ${exportRun.stdout}`);
    const summary = JSON.parse(exportRun.stdout);
    assert.equal(summary.status, "ready");
    if (sample.marker) {
      assert.ok(
        summary.payload.includes(sample.marker),
        `${sample.file}: the payload must ship pseudonymized under ${sample.marker}`,
      );
    }
  }
  cases += 1;
}

function relativePath(absolutePath) {
  return pathRelative(temp, absolutePath);
}

await rm(temp, { recursive: true, force: true });
console.log(JSON.stringify({ ok: true, samples: cases }));

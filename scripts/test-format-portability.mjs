#!/usr/bin/env node
// Issue #94 portability gate: the canonical artifact manifest. Fresh
// real-binary runs over the CRLF and Unicode seeds must reproduce the
// reviewed committed bytes byte-for-byte on this platform; the gate is
// wired into every OS job of build-test and the committed manifest pins
// the cross-platform comparison. `trim()`-style normalizers would hide
// drift, so the comparison is exact bytes.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
const fixtures = join(root, "tests/fixtures/format/portability");
if (!existsSync(binary)) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "binary-missing", binary }, null, 2)}\n`);
  process.exit(1);
}

const sha256 = (data) => createHash("sha256").update(data).digest("hex");
const manifest = JSON.parse(readFileSync(join(fixtures, "manifest.json"), "utf8"));
const workspace = mkdtempSync(join(tmpdir(), "lekalo-format-portability-"));
try {
  // 1. The committed manifest pins the reviewed bytes.
  for (const file of manifest.files) {
    const actual = sha256(readFileSync(join(root, file.path)));
    assert.equal(actual, file.sha256, `committed bytes drifted: ${file.path}`);
  }

  // 2. Fresh runs reproduce the reviewed outputs byte-for-byte.
  const cases = [
    ["crlf-seed", "crlf-expected"],
    ["unicode-seed", "unicode-expected"],
  ];
  for (const [seedFamily, expectedFamily] of cases) {
    const project = join(workspace, `${seedFamily}-${process.pid}`);
    cpSync(join(fixtures, seedFamily), project, { recursive: true });
    const run = spawnSync(binary, ["format"], { cwd: project, encoding: "utf8", timeout: 120000 });
    assert.ok(!run.error, `binary ran: ${run.error}`);
    assert.equal(run.status, 0, `format ${seedFamily}: ${run.stdout} ${run.stderr}`);
    const produced = join(project, "lekalo/project.yaml");
    const expected = join(fixtures, expectedFamily, "lekalo/project.yaml");
    assert.ok(
      readFileSync(produced).equals(readFileSync(expected)),
      `${seedFamily}: output differs from the reviewed bytes`,
    );
    // No CRLF survives and exactly one final newline is present.
    const bytes = readFileSync(produced);
    assert.ok(!bytes.includes(Buffer.from("\r\n")), "LF-only output");
    assert.equal(bytes[bytes.length - 1], 0x0a, "one final newline");
    // Idempotence on this platform: a second run is a no-op.
    const mtime = statSync(produced).mtimeMs;
    const again = spawnSync(binary, ["format"], { cwd: project, encoding: "utf8", timeout: 120000 });
    assert.equal(again.status, 0);
    assert.equal(statSync(produced).mtimeMs, mtime, "repeat run is a no-op");
    rmSync(project, { recursive: true, force: true });
  }

  // 3. The canonical golden tree formats to itself (repeat/no-op recipe).
  const goldenRoot = join(root, "tests/fixtures/format/planner-canonical");
  const goldenCopy = join(workspace, `golden-${process.pid}`);
  cpSync(goldenRoot, goldenCopy, { recursive: true });
  const walk = (base) => {
    const digest = createHash("sha256");
    const visit = (relative) => {
      for (const name of readdirSync(join(base, relative)).sort()) {
        const next = relative ? `${relative}/${name}` : name;
        if (statSync(join(base, next)).isDirectory()) visit(next);
        else digest.update(`${next}:${sha256(readFileSync(join(base, next)))}\n`);
      }
    };
    visit("");
    return digest.digest("hex");
  };
  const before = walk(goldenCopy);
  const run = spawnSync(binary, ["format"], { cwd: goldenCopy, encoding: "utf8", timeout: 120000 });
  assert.equal(run.status, 0, run.stdout + run.stderr);
  assert.equal(walk(goldenCopy), before, "the committed golden is a fixed point on this platform");
  rmSync(goldenCopy, { recursive: true, force: true });

  rmSync(workspace, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({
    ok: true,
    gate: "format-portability",
    platform: `${process.platform}-${process.arch}`,
    cases: cases.length + 1,
  }, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "format-portability", error: String(error?.message ?? error) }, null, 2)}\n`);
  process.exitCode = 1;
} finally {
  if (existsSync(workspace)) rmSync(workspace, { recursive: true, force: true });
}

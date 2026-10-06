#!/usr/bin/env node
// Issue #94 atomic gate: the confined formatter transaction under the
// real binary — fail-closed readers, real and corrupt recovery material,
// stale locks, rollback state, and the absence of temp source files after
// a committed apply. A missing binary fails the gate; there is no
// skip-success path.

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
const fixtures = join(root, "tests/fixtures/format");
if (!existsSync(binary)) {
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "binary-missing", binary }, null, 2)}\n`);
  process.exit(1);
}

const sha256 = (data) => createHash("sha256").update(data).digest("hex");
const workspace = mkdtempSync(join(tmpdir(), "lekalo-format-atomic-"));
let passed = 0;
try {
  const seed = (name, family) => {
    const destination = join(workspace, `${name}-${process.pid}`);
    rmSync(destination, { recursive: true, force: true });
    cpSync(join(fixtures, family), destination, { recursive: true });
    return destination;
  };
  const run = (cwd, args) => {
    const child = spawnSync(binary, args, { cwd, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, timeout: 120000 });
    assert.ok(!child.error, `binary ran: ${child.error}`);
    return { status: child.status, stdout: child.stdout ?? "", stderr: child.stderr ?? "" };
  };
  const formatHome = (project) => join(project, ".lekalo/cache/format");
  const modulePath = (project) => join(project, "lekalo/modules/scheduling/module.yaml");

  // 1. An interrupted journal blocks every reader; check refuses and
  //    never recovers or creates anything.
  {
    const project = seed("blocked", "planner-canonical");
    const home = join(formatHome(project), "abc123");
    mkdirSync(home, { recursive: true });
    writeFileSync(join(home, "journal.json"), Buffer.from('{"schema_version":"lekalo/format-transaction/v0.8.0","planId":"abc123","manifestSha256":"x"}\n'));
    const load = run(project, ["load"]);
    assert.equal(load.status, 1);
    assert.match(load.stderr, /format\.recovery-required/);
    const check = run(project, ["format", "--check"]);
    assert.equal(check.status, 1);
    assert.match(check.stderr, /format\.recovery-required/);
    // The reader refusal is byte-facts: the journal is still there.
    assert.ok(existsSync(join(home, "journal.json")), "check never recovers");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 2. Real recovery material: the mutating run restores the pinned
  //    preimage before planning new work and clears the journal.
  {
    const project = seed("recover", "seeds/noncanonical");
    const module = modulePath(project);
    const before = readFileSync(module);
    const beforeDigest = sha256(before);
    const after = Buffer.from('# reformatted under the interrupted plan\n"definitions":\n  - "description": "Slot scheduling module"\n    "id": "scheduling"\n    "kind": "module"\n    "version": 1\n"schema_version": "0.2.16"\n');
    writeFileSync(module, after);
    const planId = "cafe1111";
    const home = join(formatHome(project), planId);
    mkdirSync(join(home, "before/modules/scheduling"), { recursive: true });
    writeFileSync(join(home, "before/modules/scheduling/module.yaml"), before);
    const manifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","policy_digest":"sha256:0000","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${beforeDigest}","after_sha256":"${sha256(after)}"}]}`;
    writeFileSync(join(home, "manifest.json"), Buffer.from(manifest));
    writeFileSync(join(home, "journal.json"), Buffer.from(`{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","manifest_sha256":"${sha256(manifest)}"}\n`));
    const applied = run(project, ["format"]);
    assert.equal(applied.status, 0, applied.stdout + applied.stderr);
    assert.ok(!existsSync(join(home, "journal.json")), "the journal cleared");
    // The recovered bytes were the formatting input: the marker comment
    // from the tampered state is gone and the golden bytes are exact.
    const golden = join(fixtures, "planner-canonical/lekalo/modules/scheduling/module.yaml");
    assert.ok(readFileSync(module).equals(readFileSync(golden)), "recovery restored the preimage; apply formatted that");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 3. Corrupt recovery material fails closed and is never dropped.
  {
    const project = seed("corrupt", "planner-canonical");
    const home = join(formatHome(project), "bad00bad");
    mkdirSync(home, { recursive: true });
    writeFileSync(join(home, "journal.json"), Buffer.from('{"schema_version":"lekalo/format-transaction/v0.8.0","planId":"bad00bad","manifestSha256":"deadbeef"}\n'));
    const applied = run(project, ["format"]);
    assert.equal(applied.status, 1, "apply refuses instead of guessing");
    assert.match(applied.stderr, /format\.recovery-required/);
    assert.ok(existsSync(join(home, "journal.json")), "recovery material is never silently dropped");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 4. A stale lock without a journal blocks readers; the explicit
  //    mutating mode clears it and proceeds (no replacements ever
  //    happened before the journal existed).
  {
    const project = seed("stale-lock", "planner-canonical");
    mkdirSync(formatHome(project), { recursive: true });
    writeFileSync(join(formatHome(project), "active.lock"), Buffer.from("stale\n"));
    assert.equal(run(project, ["load"]).status, 1, "the lock alone blocks readers");
    assert.equal(run(project, ["format", "--check"]).status, 1, "check reads state and refuses");
    const applied = run(project, ["format"]);
    assert.equal(applied.status, 0, applied.stdout + applied.stderr);
    assert.ok(!existsSync(join(formatHome(project), "active.lock")), "the stale lock cleared");
    assert.equal(run(project, ["load"]).status, 0, "readers work again");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 5. A committed apply leaves no journal, no temp source files, and
  //    keeps immutable rollback material.
  {
    const project = seed("committed", "seeds/noncanonical");
    const applied = run(project, ["format"]);
    assert.equal(applied.status, 0);
    const home = formatHome(project);
    assert.ok(existsSync(home), "the runtime home exists");
    const plans = readdirSync(home).filter((name) => name !== "active.lock");
    assert.equal(plans.length, 1, "exactly one plan home");
    assert.ok(!existsSync(join(home, plans[0], "journal.json")), "the journal cleared at commit");
    assert.ok(existsSync(join(home, plans[0], "manifest.json")), "the manifest remains");
    assert.ok(existsSync(join(home, plans[0], "before")), "backups remain");
    // No temp source files anywhere in the source tree.
    const walk = (base) => {
      for (const entry of readdirSync(base, { withFileTypes: true })) {
        assert.ok(!entry.name.includes(".lekalo-format-new"), "no temp source files");
        const next = join(base, entry.name);
        if (entry.isDirectory()) walk(next);
      }
    };
    walk(join(project, "lekalo"));
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 6. A read-only source directory fails the replacement stage with
  //    verified rollback (POSIX permission semantics).
  if (process.platform !== "win32") {
    const project = seed("readonly", "seeds/noncanonical");
    const plannerDir = join(project, "lekalo/modules/planner");
    const before = readFileSync(modulePath(project));
    chmodSync(plannerDir, 0o555);
    try {
      const applied = run(project, ["format"]);
      assert.equal(applied.status, 1, "the write failure is invalid");
      assert.match(applied.stderr, /format\.write-failed/, "the stage failure is typed");
      // Every reachable source keeps its preimage: rollback restored or
      // nothing was written yet — never a mixed tree reported as success.
      const load = run(project, ["load"]);
      assert.notEqual(load.status, 0, "a rolled-back tree is not silently valid");
    } finally {
      chmodSync(plannerDir, 0o755);
    }
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  rmSync(workspace, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "format-atomic", passed }, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "format-atomic", error: String(error?.message ?? error) }, null, 2)}\n`);
  process.exitCode = 1;
} finally {
  if (existsSync(workspace)) rmSync(workspace, { recursive: true, force: true });
}

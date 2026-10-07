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
    const afterDigest = sha256(after);
    // The pinned record family: the plan home carries the 47-hex base
    // identity plus the attempt suffix, the policy digest is a full
    // sha256 spelling, and the manifest's pins recompute the base.
    const policyDigest = `sha256:${"0".repeat(64)}`;
    const base = sha256(Buffer.concat([
      Buffer.from(policyDigest), Buffer.from([0x1f]),
      Buffer.from("lekalo/modules/scheduling/module.yaml"), Buffer.from([0x1f]),
      Buffer.from(beforeDigest), Buffer.from([0x1f]),
      Buffer.from(afterDigest),
    ]));
    const planId = `${base.slice(0, 47)}-1`;
    const home = join(formatHome(project), planId);
    mkdirSync(join(home, "before/modules/scheduling"), { recursive: true });
    writeFileSync(join(home, "before/modules/scheduling/module.yaml"), before);
    const manifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","policy_digest":"${policyDigest}","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${beforeDigest}","after_sha256":"${afterDigest}"}]}`;
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

  // 4. A stale lock file without a journal is not a reader signal (the
  //    OS ownership lock is the real exclusion): readers work, and the
  //    explicit mutating run owns the home and rewrites the stamp.
  {
    const project = seed("stale-lock", "planner-canonical");
    mkdirSync(formatHome(project), { recursive: true });
    writeFileSync(join(formatHome(project), "active.lock"), Buffer.from("stale\n"));
    assert.equal(run(project, ["load"]).status, 0, "a journal-less lock never blocks readers");
    assert.equal(run(project, ["format", "--check"]).status, 0, "check is read-only and unblocked");
    const applied = run(project, ["format"]);
    assert.equal(applied.status, 0, applied.stdout + applied.stderr);
    assert.ok(existsSync(join(formatHome(project), "active.lock")), "the ownership file persists");
    assert.equal(run(project, ["load"]).status, 0, "readers work");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 4b. A LIVE writer is respected (finding C): with the ownership lock
  //     genuinely held by another process (util-linux flock on POSIX),
  //     the second mutating run refuses with the registered busy denial
  //     (exit 3, format.locked) and never rolls the live state back.
  if (process.platform !== "win32") {
    const project = seed("live-lock", "seeds/noncanonical");
    // The live writer has already replaced a source and journaled.
    const module = modulePath(project);
    const before = readFileSync(module);
    const beforeDigest = sha256(before);
    const applied = Buffer.from('# the live writer applied this\n"definitions":\n  - "description": "Slot scheduling module"\n    "id": "scheduling"\n    "kind": "module"\n    "version": 1\n"schema_version": "0.2.16"\n');
    writeFileSync(module, applied);
    const policyDigest = `sha256:${"0".repeat(64)}`;
    const base = sha256(Buffer.concat([
      Buffer.from(policyDigest), Buffer.from([0x1f]),
      Buffer.from("lekalo/modules/scheduling/module.yaml"), Buffer.from([0x1f]),
      Buffer.from(beforeDigest), Buffer.from([0x1f]),
      Buffer.from(sha256(applied)),
    ]));
    const planId = `${base.slice(0, 47)}-1`;
    const home = join(formatHome(project), planId);
    mkdirSync(join(home, "before/modules/scheduling"), { recursive: true });
    writeFileSync(join(home, "before/modules/scheduling/module.yaml"), before);
    const manifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","policy_digest":"${policyDigest}","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${beforeDigest}","after_sha256":"${sha256(applied)}"}]}`;
    writeFileSync(join(home, "manifest.json"), Buffer.from(manifest));
    writeFileSync(join(home, "journal.json"), Buffer.from(`{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","manifest_sha256":"${sha256(manifest)}"}\n`));
    // Hold the ownership lock exactly like a live formatter would.
    const holder = spawnSync(
      "flock",
      [join(formatHome(project), "active.lock"), "-c", "sleep 6"],
      { timeout: 15000 },
    );
    assert.ok(!holder.error, `flock holder ran: ${holder.error}`);
    const second = run(project, ["format"]);
    assert.equal(second.status, 3, `the live writer is refused as busy: ${second.stdout} ${second.stderr}`);
    assert.match(second.stdout + second.stderr, /format\.locked/, "the registered busy denial");
    assert.ok(readFileSync(module).equals(applied), "the live writer's applied bytes were not rolled back");
    assert.ok(existsSync(join(home, "journal.json")), "the journal stays");
    // Once the owner exits, recovery proceeds.
    const recovered = run(project, ["format"]);
    assert.equal(recovered.status, 0, recovered.stdout + recovered.stderr);
    assert.ok(!existsSync(join(home, "journal.json")), "the journal cleared after the owner left");
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
  //    verified rollback (POSIX permission semantics): the rolled-back
  //    tree loads clean (exit 0) and the sources are byte-for-byte the
  //    originals. An INCOMPLETE rollback (one source left replaced) is
  //    the fail-closed case instead: readers refuse until recovery.
  if (process.platform !== "win32") {
    const project = seed("readonly", "seeds/noncanonical");
    const before = readFileSync(modulePath(project));
    const entitiesBefore = readFileSync(join(project, "lekalo/modules/planner/entities.yaml"));
    chmodSync(join(project, "lekalo/modules/planner"), 0o555);
    try {
      const applied = run(project, ["format"]);
      assert.equal(applied.status, 1, "the write failure is invalid");
      assert.match(applied.stderr, /format\.write-failed/, "the stage failure is typed");
      // Verified rollback is byte-exact: every restored source equals its
      // preimage and the tree loads cleanly (exit 0).
      assert.ok(readFileSync(modulePath(project)).equals(before), "rollback restored the module bytes");
      assert.ok(
        readFileSync(join(project, "lekalo/modules/planner/entities.yaml")).equals(entitiesBefore),
        "rollback restored sibling bytes",
      );
      assert.equal(run(project, ["load"]).status, 0, "a fully rolled-back tree loads clean");
    } finally {
      chmodSync(join(project, "lekalo/modules/planner"), 0o755);
    }
    rmSync(project, { recursive: true, force: true });
    passed += 1;

    // 6b. An incomplete rollback (journal left behind, one source left
    //     replaced) fails closed: readers refuse with the recovery
    //     diagnostic, and the explicit mutating run restores the pinned
    //     preimage before new work.
    {
      const project = seed("incomplete-rollback", "seeds/noncanonical");
      const before = readFileSync(modulePath(project));
      const beforeDigest = sha256(before);
      const replaced = Buffer.from('# a half-applied replacement\n"definitions":\n  - "description": "Slot scheduling module"\n    "id": "scheduling"\n    "kind": "module"\n    "version": 1\n"schema_version": "0.2.16"\n');
      writeFileSync(modulePath(project), replaced);
      const planId = "d".repeat(47);
      const home = join(formatHome(project), planId);
      mkdirSync(join(home, "before/modules/scheduling"), { recursive: true });
      writeFileSync(join(home, "before/modules/scheduling/module.yaml"), before);
      const manifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","policy_digest":"sha256:${"0".repeat(64)}","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${beforeDigest}","after_sha256":"${sha256(replaced)}"}]}`;
      writeFileSync(join(home, "manifest.json"), Buffer.from(manifest));
      writeFileSync(join(home, "journal.json"), Buffer.from(`{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","manifest_sha256":"${sha256(manifest)}"}\n`));
      // The interrupted state blocks readers (fail closed).
      assert.equal(run(project, ["load"]).status, 1, "the journal blocks readers");
      // The mutating run recovers the pinned preimage, then formats.
      const applied = run(project, ["format"]);
      assert.equal(applied.status, 0, applied.stdout + applied.stderr);
      const golden = join(fixtures, "planner-canonical/lekalo/modules/scheduling/module.yaml");
      assert.ok(readFileSync(modulePath(project)).equals(readFileSync(golden)), "recovery restored the preimage; apply formatted that");
      assert.ok(!existsSync(join(home, "journal.json")), "the journal cleared");
      rmSync(project, { recursive: true, force: true });
      passed += 1;
    }
  }

  rmSync(workspace, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "format-atomic", passed }, null, 2)}\n`);
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "format-atomic", error: String(error?.message ?? error) }, null, 2)}\n`);
  process.exitCode = 1;
} finally {
  if (existsSync(workspace)) rmSync(workspace, { recursive: true, force: true });
}

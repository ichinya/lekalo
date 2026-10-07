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
import { spawn, spawnSync } from "node:child_process";

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
  //     genuinely held by another live process (util-linux flock on
  //     POSIX), the second mutating run refuses with the registered busy
  //     denial (exit 3, format.locked) and never rolls the live state
  //     back. The holder is asynchronous (finding r2 6): the lock is
  //     still held while the competitor runs, which is the exact state
  //     the gate exists to test — a synchronous holder would release the
  //     lock before the competitor ever started. The holder is only
  //     assumed on Linux, where util-linux flock is a base tool.
  if (process.platform === "linux") {
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
    // Establish the live lock and PROVE ownership before the competitor:
    // the holder takes the lock non-blocking (so a stale owner would fail
    // the gate immediately) and then sleeps, keeping the lock held.
    const lockPath = join(formatHome(project), "active.lock");
    const holder = spawn("flock", ["-n", lockPath, "-c", "sleep 30"], {
      stdio: "ignore",
    });
    try {
      // Ownership proof: while the holder lives, the same non-blocking
      // acquisition must refuse — the lock is genuinely contended.
      const proof = spawnSync("flock", ["-n", lockPath, "true"], { timeout: 10000 });
      assert.notEqual(proof.status, 0, "the holder owns the lock (contention is real)");
      const second = run(project, ["format"]);
      assert.equal(second.status, 3, `the live writer is refused as busy: ${second.stdout} ${second.stderr}`);
      assert.match(second.stdout + second.stderr, /format\.locked/, "the registered busy denial");
      assert.ok(readFileSync(module).equals(applied), "the live writer's applied bytes were not rolled back");
      assert.ok(existsSync(join(home, "journal.json")), "the journal stays");
    } finally {
      // Release: end the holder and wait for the kernel to drop the lock.
      holder.kill("SIGTERM");
      await new Promise((resolveHold) => {
        holder.once("exit", resolveHold);
        holder.once("error", resolveHold);
      });
    }
    // Once the owner exited, recovery proceeds.
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
    const plans = readdirSync(home).filter((name) => name !== "active.lock" && name !== "temps");
    assert.equal(plans.length, 1, "exactly one plan home");
    assert.ok(!existsSync(join(home, plans[0], "journal.json")), "the journal cleared at commit");
    assert.ok(existsSync(join(home, plans[0], "manifest.json")), "the manifest remains");
    assert.ok(existsSync(join(home, plans[0], "before")), "backups remain");
    // No temp files anywhere in the source tree: rename temps live in
    // the runtime temp home, never as source-tree siblings (finding
    // r2 3).
    const walk = (base) => {
      for (const entry of readdirSync(base, { withFileTypes: true })) {
        assert.ok(!entry.name.startsWith("."), "no dotfiles in the governed tree");
        assert.ok(!entry.name.includes("lekalo-format-new"), "no temp source files");
        const next = join(base, entry.name);
        if (entry.isDirectory()) walk(next);
      }
    };
    walk(join(project, "lekalo"));
    // A committed apply leaves no orphaned rename temps either.
    const temps = join(home, "temps");
    assert.ok(
      !existsSync(temps) || readdirSync(temps).length === 0,
      "no orphaned rename temps after commit",
    );
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 6. A read-only source directory fails the replacement stage with
  //    verified rollback (POSIX permission semantics): the rolled-back
  //    tree loads clean (exit 0) and the sources are byte-for-byte the
  //    originals. An INCOMPLETE rollback (one source left replaced) is
  //    the fail-closed case instead: readers refuse until recovery.
  //
  //    The sorted candidate order puts lekalo/project.yaml first and the
  //    planner module files after it, so the read-only planner directory
  //    fails at project.yaml+1: the planner module files were already
  //    replaced when the failure lands — a real prefix > 0 partial
  //    rollback (finding r2 16, previously only the index-0 prefix was
  //    exercised).
  if (process.platform !== "win32") {
    const project = seed("readonly", "seeds/noncanonical");
    const originals = new Map([
      [modulePath(project), readFileSync(modulePath(project))],
      [join(project, "lekalo/modules/planner/module.yaml"), readFileSync(join(project, "lekalo/modules/planner/module.yaml"))],
      [join(project, "lekalo/modules/planner/entities.yaml"), readFileSync(join(project, "lekalo/modules/planner/entities.yaml"))],
      [join(project, "lekalo/modules/planner/scenarios.yaml"), readFileSync(join(project, "lekalo/modules/planner/scenarios.yaml"))],
      [join(project, "lekalo/project.yaml"), readFileSync(join(project, "lekalo/project.yaml"))],
    ]);
    const changedBefore = run(project, ["format", "--check", "--json"]);
    assert.equal(changedBefore.status, 3, "the seed needs formatting");
    chmodSync(join(project, "lekalo/modules/planner"), 0o555);
    try {
      const applied = run(project, ["format"]);
      assert.equal(applied.status, 1, "the write failure is invalid");
      assert.match(applied.stderr, /format\.write-failed/, "the stage failure is typed");
      // Verified rollback is byte-exact for the whole replaced prefix:
      // every restored source equals its preimage (including the files
      // replaced BEFORE the failing one) and the tree loads clean.
      for (const [path, bytes] of originals) {
        assert.ok(readFileSync(path).equals(bytes), `rollback restored ${path}`);
      }
      assert.equal(run(project, ["load"]).status, 0, "a fully rolled-back tree loads clean");
    } finally {
      chmodSync(join(project, "lekalo/modules/planner"), 0o755);
    }
    rmSync(project, { recursive: true, force: true });
    passed += 1;

    // 6b. An incomplete rollback (journal left behind, one source left
    //     replaced) fails closed: readers refuse with the recovery
    //     diagnostic, and the explicit mutating run restores the pinned
    //     preimage before new work. The records are valid (finding
    //     r2 7): the plan home carries the recomputable 47-hex base
    //     identity plus the attempt suffix — the earlier "d".repeat(47)
    //     spelling can never decode, so the expected exit 0 was
    //     unreachable.
    {
      const project = seed("incomplete-rollback", "seeds/noncanonical");
      const before = readFileSync(modulePath(project));
      const beforeDigest = sha256(before);
      const replaced = Buffer.from('# a half-applied replacement\n"definitions":\n  - "description": "Slot scheduling module"\n    "id": "scheduling"\n    "kind": "module"\n    "version": 1\n"schema_version": "0.2.16"\n');
      writeFileSync(modulePath(project), replaced);
      const policyDigest = `sha256:${"0".repeat(64)}`;
      const base = sha256(Buffer.concat([
        Buffer.from(policyDigest), Buffer.from([0x1f]),
        Buffer.from("lekalo/modules/scheduling/module.yaml"), Buffer.from([0x1f]),
        Buffer.from(beforeDigest), Buffer.from([0x1f]),
        Buffer.from(sha256(replaced)),
      ]));
      const planId = `${base.slice(0, 47)}-1`;
      const home = join(formatHome(project), planId);
      mkdirSync(join(home, "before/modules/scheduling"), { recursive: true });
      writeFileSync(join(home, "before/modules/scheduling/module.yaml"), before);
      const manifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","policy_digest":"${policyDigest}","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${beforeDigest}","after_sha256":"${sha256(replaced)}"}]}`;
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

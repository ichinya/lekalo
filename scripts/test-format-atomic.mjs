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
// The exact policy pin valid recovery material must carry (finding r4
// 5): the digest embedded in the format-policy contract.
const embeddedPolicyDigest = JSON.parse(
  readFileSync(join(root, "contracts/format-policy.v0.8.0.json"), "utf8")
).policyDigest;
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

  // A formatter runtime directory is not authority over unrelated files.
  // This real-binary regression fails on the suffix-only cleanup owner.
  {
    const project = seed("foreign-runtime", "planner-canonical");
    const home = formatHome(project);
    const foreign = [
      "temps/user.lekalo-format-new",
      "temps/BAD.lekalo-format-new",
      "private-notes/sentinel.txt",
      `${"a".repeat(47)}-1/sentinel.txt`,
    ];
    for (const relative of foreign) {
      const file = join(home, relative);
      mkdirSync(join(file, ".."), { recursive: true });
      writeFileSync(file, "USER-OWNED-SENTINEL\n");
    }
    const applied = run(project, ["format"]);
    assert.equal(applied.status, 0, applied.stdout + applied.stderr);
    for (const relative of foreign) {
      assert.equal(readFileSync(join(home, relative), "utf8"), "USER-OWNED-SENTINEL\n", relative);
    }
    passed += 1;
  }

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
    const mutating = run(project, ["format"]);
    assert.notEqual(mutating.status, 0, "an unrecognized journal cannot be bypassed by mutation");
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
    const policyDigest = embeddedPolicyDigest;
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

  // 3b. A self-consistent record naming an UNSUPPORTED policy pin is
  //     refused before any restore (finding r4 5): hash-consistency is
  //     not authenticity. The record below is perfectly re-signed — the
  //     pins recompute its base identity — but its policy digest is the
  //     all-zeros spelling no supported policy ever produced.
  {
    const project = seed("stale-policy", "planner-canonical");
    const module = modulePath(project);
    const before = readFileSync(module);
    const beforeDigest = sha256(before);
    const afterDigest = sha256(before); // identical pins, forged pin below
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
    assert.equal(applied.status, 1, applied.stdout + applied.stderr);
    assert.match(applied.stderr, /format.recovery-required/);
    assert.ok(existsSync(join(home, "journal.json")), "the forged-pin material is never dropped");
    assert.ok(readFileSync(module).equals(before), "the source is never touched under a foreign pin");
    // The refused material is never silently dropped; an operator must
    // remove it explicitly before the home is usable again.
    rmSync(home, { recursive: true, force: true });
    // Control: the SAME record re-signed under the embedded policy pin
    // is admitted and recovers — the boundary is the pin, nothing else.
    const goodBase = sha256(Buffer.concat([
      Buffer.from(embeddedPolicyDigest), Buffer.from([0x1f]),
      Buffer.from("lekalo/modules/scheduling/module.yaml"), Buffer.from([0x1f]),
      Buffer.from(beforeDigest), Buffer.from([0x1f]),
      Buffer.from(afterDigest),
    ]));
    const goodId = `${goodBase.slice(0, 47)}-1`;
    const goodHome = join(formatHome(project), goodId);
    mkdirSync(join(goodHome, "before/modules/scheduling"), { recursive: true });
    writeFileSync(join(goodHome, "before/modules/scheduling/module.yaml"), before);
    const goodManifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${goodId}","policy_digest":"${embeddedPolicyDigest}","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${beforeDigest}","after_sha256":"${afterDigest}"}]}`;
    writeFileSync(join(goodHome, "manifest.json"), Buffer.from(goodManifest));
    writeFileSync(join(goodHome, "journal.json"), Buffer.from(`{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${goodId}","manifest_sha256":"${sha256(goodManifest)}"}\n`));
    const recovered = run(project, ["format"]);
    assert.equal(recovered.status, 0, recovered.stdout + recovered.stderr);
    assert.ok(!existsSync(join(goodHome, "journal.json")), "the supported-pin material recovers");
    rmSync(project, { recursive: true, force: true });
    passed += 1;
  }

  // 3c. A digest-pinned backup carrying a SEMANTIC edit is refused
  //     before any restore (finding r4 5): recovery must never change
  //     semantics ahead of the formatter's own oracle. The record is
  //     fully hash-consistent, pins the supported policy, and its after
  //     pin names the honest canonical source — so the preflight
  //     conflict class never fires; the restore itself is what would
  //     smuggle the edit into canonical source. The pre-restore
  //     semantic oracle refuses it.
  {
    const project = seed("forged-preimage", "planner-canonical");
    const module = modulePath(project);
    const before = readFileSync(module);
    const beforeDigest = sha256(before);
    const forged = Buffer.from("# forged semantic edit\n\"definitions\":\n  - \"description\": \"FORGED SEMANTIC EDIT\"\n    \"id\": \"scheduling\"\n    \"kind\": \"module\"\n    \"version\": 1\n\"schema_version\": \"0.2.16\"\n");
    const forgedDigest = sha256(forged);
    const base = sha256(Buffer.concat([
      Buffer.from(embeddedPolicyDigest), Buffer.from([0x1f]),
      Buffer.from("lekalo/modules/scheduling/module.yaml"), Buffer.from([0x1f]),
      Buffer.from(forgedDigest), Buffer.from([0x1f]),
      Buffer.from(beforeDigest),
    ]));
    const planId = `${base.slice(0, 47)}-1`;
    const home = join(formatHome(project), planId);
    mkdirSync(join(home, "before/modules/scheduling"), { recursive: true });
    writeFileSync(join(home, "before/modules/scheduling/module.yaml"), forged);
    const manifest = `{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","policy_digest":"${embeddedPolicyDigest}","files":[{"path":"lekalo/modules/scheduling/module.yaml","before_sha256":"${forgedDigest}","after_sha256":"${beforeDigest}"}]}`;
    writeFileSync(join(home, "manifest.json"), Buffer.from(manifest));
    writeFileSync(join(home, "journal.json"), Buffer.from(`{"schema_version":"lekalo/format-transaction/v0.8.0","plan_id":"${planId}","manifest_sha256":"${sha256(manifest)}"}\n`));
    const applied = run(project, ["format"]);
    assert.equal(applied.status, 1, applied.stdout + applied.stderr);
    assert.match(applied.stderr, /format.recovery-required/);
    assert.ok(existsSync(join(home, "journal.json")), "the forged-preimage material is never dropped");
    assert.ok(readFileSync(module).equals(before), "the canonical source never receives the forged semantics");
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
    const policyDigest = embeddedPolicyDigest;
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
    // the gate immediately) and then sleeps, keeping the lock held. The
    // holder is detached — its own process group — because flock(1)
    // forks the sleep child, which inherits the locked descriptor: the
    // lock lives on that shared open file description, so ending only
    // the parent would leave the child holding it forever (finding r3
    // 5). The teardown therefore kills the whole group, and both
    // handshakes below (held, then released) prove the lock's real
    // state through independent non-blocking acquisitions instead of
    // assuming it.
    const lockPath = join(formatHome(project), "active.lock");
    const holder = spawn("flock", ["-n", lockPath, "-c", "sleep 30"], {
      stdio: "ignore",
      detached: true,
    });
    const contention = () =>
      spawnSync("flock", ["-n", lockPath, "true"], { timeout: 10000 });
    try {
      // Readiness handshake: poll until a non-blocking acquisition
      // genuinely refuses — the lock is held right now, not merely
      // spawned.
      const ownedDeadline = Date.now() + 15000;
      while (contention().status === 0) {
        if (Date.now() > ownedDeadline) {
          throw new Error("the holder never acquired the lock");
        }
        if (holder.exitCode !== null || holder.signalCode !== null) {
          throw new Error("the holder exited before acquiring the lock");
        }
        await new Promise((resolveHold) => setTimeout(resolveHold, 50));
      }
      const second = run(project, ["format"]);
      assert.equal(second.status, 3, `the live writer is refused as busy: ${second.stdout} ${second.stderr}`);
      assert.match(second.stdout + second.stderr, /format\.locked/, "the registered busy denial");
      assert.ok(readFileSync(module).equals(applied), "the live writer's applied bytes were not rolled back");
      assert.ok(existsSync(join(home, "journal.json")), "the journal stays");
    } finally {
      // Release: end the holder AND its descriptor-inheriting children
      // (the whole process group), then await the leader's exit.
      if (holder.pid) {
        try {
          process.kill(-holder.pid, "SIGTERM");
        } catch {
          // Already gone — the release handshake below still verifies.
        }
      }
      await new Promise((resolveHold) => {
        holder.once("exit", resolveHold);
        holder.once("error", resolveHold);
      });
    }
    // Release handshake: poll until a non-blocking acquisition genuinely
    // succeeds — the lock died with its owner and the recovery below is
    // never racing a residual holder.
    const releasedDeadline = Date.now() + 15000;
    while (contention().status !== 0) {
      if (Date.now() > releasedDeadline) {
        throw new Error("the lock survived its holder (descriptor still inherited)");
      }
      await new Promise((resolveHold) => setTimeout(resolveHold, 50));
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
  //    The changed candidates iterate in raw path-byte order:
  //
  //      lekalo/modules/planner/entities.yaml
  //      lekalo/modules/planner/module.yaml
  //      lekalo/modules/planner/scenarios.yaml
  //      lekalo/modules/scheduling/module.yaml
  //      lekalo/project.yaml
  //
  //    so the read-only SCHEDULING directory fails at index 3: the
  //    three planner files were already replaced when the failure
  //    lands, and restore_prefix(3) walks them back in reverse from the
  //    verified backups — a real prefix > 0 partial rollback (finding
  //    r3 6: the former gate read-only'd the planner directory, which
  //    holds the FIRST three candidates, so the failure landed at index
  //    0 and only restore_prefix(0) was ever exercised; the planner
  //    directory must stay writable for that rollback to succeed).
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
    chmodSync(join(project, "lekalo/modules/scheduling"), 0o555);
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
      chmodSync(join(project, "lekalo/modules/scheduling"), 0o755);
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
      const policyDigest = embeddedPolicyDigest;
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

    // 6c. A CONCURRENT USER EDIT of an already replaced file is never
    //     erased by the ordinary rollback (finding r5 1): the rollback
    //     preflights every destination's live custody and re-derives it
    //     before each restore. The race is real: the changed candidates
    //     include ~200 q-modules sorting between planner and the
    //     read-only scheduling directory, so once planner/module.yaml is
    //     replaced the run keeps working for hundreds of replacements
    //     before the failure lands — the probe edits the file inside
    //     that window. The rollback must refuse: the typed source-changed
    //     conflict (exit 3), the edit preserved byte-exact, the journal
    //     retained, and readers fail closed.
    {
      const project = seed("race-edit", "seeds/noncanonical");
      // Widen the window: q-modules sort after planner and before
      // scheduling, each a changed candidate the run must replace
      // before it reaches the read-only failure.
      for (let index = 0; index < 200; index += 1) {
        const directory = `q${String(index).padStart(3, "0")}`;
        const module = join(project, "lekalo/modules", directory);
        mkdirSync(module, { recursive: true });
        writeFileSync(
          join(module, "module.yaml"),
          Buffer.from(`schema_version: '0.2.16'\ndefinitions:\n  - id: ${directory}\n    kind: module\n    version: 1\n    description: window module ${directory}\n`)
        );
      }
      const plannerModule = join(project, "lekalo/modules/planner/module.yaml");
      const original = readFileSync(plannerModule);
      const userEdit = Buffer.from("USER EDIT DURING TRANSACTION\n");
      chmodSync(join(project, "lekalo/modules/scheduling"), 0o555);
      let conflict;
      try {
        const child = spawn(binary, ["format"], {
          cwd: project,
          stdio: ["ignore", "pipe", "pipe"],
        });
        let output = "";
        child.stdout.on("data", (chunk) => { output += chunk; });
        child.stderr.on("data", (chunk) => { output += chunk; });
        const exited = new Promise((resolveExit) =>
          child.once("exit", (code, signal) => resolveExit({ code, signal }))
        );
        // Inject the edit the moment the formatter's own replacement of
        // planner/module.yaml is on disk (index 1 of the changed set).
        const deadline = Date.now() + 120000;
        let injected = false;
        while (Date.now() < deadline) {
          if (!readFileSync(plannerModule).equals(original)) {
            writeFileSync(plannerModule, userEdit);
            injected = true;
            break;
          }
          if (child.exitCode !== null || child.signalCode !== null) break;
          await new Promise((resolveTick) => setTimeout(resolveTick, 1));
        }
        assert.ok(injected, "the probe observed the replaced prefix");
        conflict = await exited;
        assert.equal(conflict.code, 3, `the typed conflict denial: ${conflict.code} ${conflict.signal} ${output}`);
        assert.match(output, /format\.source-changed/, "the registered conflict identity");
        assert.ok(readFileSync(plannerModule).equals(userEdit), "the concurrent edit is preserved byte-exact");
        // The journal is retained (fail closed), never cleared by a
        // rollback that did not restore.
        const plans = readdirSync(formatHome(project)).filter(
          (name) => name !== "active.lock" && name !== "temps"
        );
        assert.equal(plans.length, 1, "one plan home");
        assert.ok(existsSync(join(formatHome(project), plans[0], "journal.json")), "the journal stays armed");
        assert.equal(run(project, ["load"]).status, 1, "readers fail closed on the retained journal");
      } finally {
        chmodSync(join(project, "lekalo/modules/scheduling"), 0o755);
      }
      rmSync(project, { recursive: true, force: true });
      passed += 1;
    }

    // 6d. A PRESENT BUT UNREADABLE source is never overwritten by the
    //     phase-4 forward replace (finding r7 1): the pre-fix `Err` arm
    //     replaced a source whose bytes it failed to READ —
    //     `replace_file` needs only `symlink_metadata` and a directory
    //     rename — destroying unverified bytes. The probe overwrites the
    //     (late-sorting) scheduling module with a user edit and chmods
    //     it 000 inside the ~200-replacement window after
    //     planner/module.yaml flips; the refusal must preserve the edit
    //     byte-exact, roll the applied prefix back, retain the journal
    //     (readers fail closed), and report the typed custody conflict.
    //     Restoring the original bytes then lets the explicit format
    //     recover the armed journal and reach the canonical fixed point —
    //     the armed journal is a decision, never a wedge.
    {
      const project = seed("phase4-unreadable", "seeds/noncanonical");
      for (let index = 0; index < 200; index += 1) {
        const directory = `q${String(index).padStart(3, "0")}`;
        const module = join(project, "lekalo/modules", directory);
        mkdirSync(module, { recursive: true });
        writeFileSync(
          join(module, "module.yaml"),
          Buffer.from(`schema_version: '0.2.16'\ndefinitions:\n  - id: ${directory}\n    kind: module\n    version: 1\n    description: window module ${directory}\n`)
        );
      }
      const plannerModule = join(project, "lekalo/modules/planner/module.yaml");
      const originalPlanner = readFileSync(plannerModule);
      const scheduling = modulePath(project);
      const originalScheduling = readFileSync(scheduling);
      const userEdit = Buffer.from("USER EDIT UNDER UNREADABLE\n");
      let conflict;
      try {
        const child = spawn(binary, ["format"], {
          cwd: project,
          stdio: ["ignore", "pipe", "pipe"],
        });
        let output = "";
        child.stdout.on("data", (chunk) => { output += chunk; });
        child.stderr.on("data", (chunk) => { output += chunk; });
        const exited = new Promise((resolveExit) =>
          child.once("exit", (code, signal) => resolveExit({ code, signal }))
        );
        // Inject the edit plus mode-000 the moment the formatter's own
        // replacement of planner/module.yaml is on disk (index 1 of the
        // changed set): the ~200 q-modules that sort before scheduling
        // keep phase 4 busy, so the read of the scheduling module always
        // meets the unreadable bytes.
        const deadline = Date.now() + 120000;
        let injected = false;
        while (Date.now() < deadline) {
          if (!readFileSync(plannerModule).equals(originalPlanner)) {
            writeFileSync(scheduling, userEdit);
            chmodSync(scheduling, 0o000);
            injected = true;
            break;
          }
          if (child.exitCode !== null || child.signalCode !== null) break;
          await new Promise((resolveTick) => setTimeout(resolveTick, 1));
        }
        assert.ok(injected, "the probe observed the replaced prefix");
        conflict = await exited;
        assert.equal(conflict.code, 3, `the typed conflict denial: ${conflict.code} ${conflict.signal} ${output}`);
        assert.match(output, /format\.source-changed/, "the registered conflict identity");
        // The unread bytes survive untouched: readable again, the file
        // still carries the injected edit — never the formatted
        // candidate, never the original.
        chmodSync(scheduling, 0o644);
        assert.ok(readFileSync(scheduling).equals(userEdit), "the unread edit is preserved byte-exact");
        // The already-applied prefix was rolled back to its preimages.
        assert.ok(readFileSync(plannerModule).equals(originalPlanner), "the applied prefix is restored");
        // The journal is retained (fail closed), never cleared by the
        // refusal.
        const plans = readdirSync(formatHome(project)).filter(
          (name) => name !== "active.lock" && name !== "temps"
        );
        assert.equal(plans.length, 1, "one plan home");
        assert.ok(existsSync(join(formatHome(project), plans[0], "journal.json")), "the journal stays armed");
        assert.equal(run(project, ["load"]).status, 1, "readers fail closed on the retained journal");
        // The lifecycle completes once the user discards the edit: the
        // explicit format recovers the armed journal, then reapplies to
        // the canonical fixed point.
        writeFileSync(scheduling, originalScheduling);
        const recovered = run(project, ["format"]);
        assert.equal(recovered.status, 0, recovered.stdout + recovered.stderr);
        assert.ok(!existsSync(join(formatHome(project), plans[0], "journal.json")), "recovery cleared the journal");
        const golden = join(fixtures, "planner-canonical/lekalo/modules/scheduling/module.yaml");
        assert.ok(readFileSync(scheduling).equals(readFileSync(golden)), "the tree reaches the canonical fixed point");
        assert.equal(run(project, ["format", "--check"]).status, 0, "idempotent at the fixed point");
      } finally {
        chmodSync(scheduling, 0o644);
      }
      rmSync(project, { recursive: true, force: true });
      passed += 1;
    }
  }

  // 7. Boundary-size real crash and recovery (finding r5 2): an
  //    admitted plan just under the 256 KiB recovery-record bound —
  //    955 changed files with long valid 64-character module names — is
  //    SIGKILLed mid-replacement. Before the fix its own emitted
  //    manifest exceeded the bounded reader and every recovery refused
  //    with `manifest-unreadable` forever. Now producer and reader
  //    agree on the record grammar: the journal is readable, the next
  //    explicit format recovers and reapplies, the tree reaches the
  //    canonical fixed point, and every emitted manifest is within the
  //    bound.
  {
    const project = seed("boundary-crash", "seeds/noncanonical");
    // Long valid path names: 64-character module directories (the
    // governed path-segment maximum). The module IDs stay short and
    // grammar-valid (`^[a-z][a-z0-9_]{0,62}$`, ≤ 63 characters) — the
    // path grammar and the Model's ID grammar are distinct.
    const longName = (index) =>
      (`g${String(index).padStart(3, "0")}` + "x".repeat(60)).slice(0, 64);
    const firstDirectory = longName(0);
    for (let index = 0; index < 950; index += 1) {
      const directory = longName(index);
      const moduleId = `m${String(index).padStart(4, "0")}`;
      const module = join(project, "lekalo/modules", directory);
      mkdirSync(module, { recursive: true });
      writeFileSync(
        join(module, "module.yaml"),
        Buffer.from(`schema_version: '0.2.16'\ndefinitions:\n  - id: ${moduleId}\n    kind: module\n    version: 1\n    description: boundary module\n`)
      );
    }
    const checkFirst = run(project, ["format", "--check", "--json"]);
    assert.equal(checkFirst.status, 3, `the boundary plan is admitted: ${checkFirst.stdout}`);
    const firstModule = join(project, "lekalo/modules", firstDirectory, "module.yaml");
    const seedBytes = readFileSync(firstModule);
    const journalHome = (project) => {
      const home = formatHome(project);
      if (!existsSync(home)) return undefined;
      return readdirSync(home)
        .filter((name) => name !== "active.lock" && name !== "temps")
        .find((name) => existsSync(join(home, name, "journal.json")));
    };
    const child = spawn(binary, ["format"], { cwd: project, stdio: "ignore" });
    try {
      // Wait until replacements are durable in flight (the journal plus
      // the first long-named module actually replaced), then kill hard.
      const crashDeadline = Date.now() + 120000;
      while (Date.now() < crashDeadline) {
        if (journalHome(project) && !readFileSync(firstModule).equals(seedBytes)) break;
        await new Promise((resolveTick) => setTimeout(resolveTick, 5));
      }
      assert.ok(journalHome(project), "the durable journal exists before the kill");
      child.kill("SIGKILL");
      await new Promise((resolveExit) => child.once("exit", resolveExit));
      const interrupted = journalHome(project);
      assert.ok(interrupted, "the crash left a mid-transaction journal");
      assert.equal(run(project, ["load"]).status, 1, "readers fail closed mid-transaction");
      // The recovery the bounded record grammar must admit: restore every
      // pinned preimage, then apply the plan again.
      const recovered = run(project, ["format"]);
      assert.equal(recovered.status, 0, `recovery completes: ${recovered.stdout} ${recovered.stderr}`);
      assert.ok(!journalHome(project), "no journal survives the recovery");
      assert.equal(run(project, ["format", "--check"]).status, 0, "the canonical fixed point");
      for (const name of readdirSync(formatHome(project))) {
        if (name === "active.lock" || name === "temps") continue;
        const manifest = join(formatHome(project), name, "manifest.json");
        if (!existsSync(manifest)) continue;
        assert.ok(
          readFileSync(manifest).length <= 256 * 1024,
          `every emitted manifest is within the recovery record bound (${name})`
        );
      }
    } finally {
      // Never leave a live formatter holding the workspace.
      if (child.exitCode === null && child.signalCode === null) {
        child.kill("SIGKILL");
        await new Promise((resolveExit) => {
          child.once("exit", resolveExit);
          child.once("error", resolveExit);
        });
      }
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

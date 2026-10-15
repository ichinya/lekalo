#!/usr/bin/env node
// Issue #52 native gate (qualified-environment custody).
//
// The durable/race assertions of the pilot (real transactional DB,
// process restarts, synchronized competing workers, maintained Laravel
// port) require a provisioned environment. This gate never satisfies
// them from mocks: mock evidence and missing toolchains stay explicit
// `pending-provisioning` capability gaps, and any public artifact that
// CLAIMS native coverage without the provisioning flag fails the gate.
//
// Provisioned mode (the qualified CI job):
//   LEKALO_TASKLINKS_NATIVE=1 cargo test --locked -p lekalo-core \
//     --test task_links native  (plus the maintained-port battery)
// Without the flag the gate verifies non-claim and exits ok with the
// recorded gap (the repo's sanctioned runtime-qualification pattern).

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { resolve as resolvePath } from "node:path";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const gate = "task-links-native";
const failures = [];
const fail = (reason, detail) => failures.push({ reason, detail });
const provisioned = process.env.LEKALO_TASKLINKS_NATIVE === "1";
const read = (relative) => JSON.parse(readFileSync(join(root, relative), "utf8"));

// 1. The maintained-port fixture roots exist and stay synthetic.
for (const relative of [
  "tests/fixtures/task-external-links/providers/github-issues/contract.json",
  "tests/fixtures/task-external-links/providers/mock-secondary/contract.json",
]) {
  if (!existsSync(join(root, relative))) fail("fixture-missing", relative);
}
{
  const primary = JSON.parse(
    readFileSync(
      join(root, "tests/fixtures/task-external-links/providers/github-issues/contract.json"),
      "utf8",
    ),
  );
  if (primary.evidence.origin !== "synthetic") fail("provider-origin", primary.evidence.origin);
}

// 2. Non-claim discipline: no public artifact of the family may claim
//    native origin while the qualified environment is absent.
const fixtureRoot = join(root, "tests/fixtures/task-external-links");
const claimed = [];
const walk = (current) => {
  for (const name of readdirSync(current).sort()) {
    const full = join(current, name);
    if (statSync(full).isDirectory()) walk(full);
    else if (
      name.endsWith(".json")
      && !current.replaceAll("\\", "/").includes("/invalid/")
      && !current.replaceAll("\\", "/").endsWith("/invalid")
    ) {
      const text = readFileSync(full, "utf8");
      if (/"origin"\s*:\s*"native"/.test(text) && !text.includes("runtime-boundary")) {
        claimed.push(full.slice(root.length + 1));
      }
    }
  }
};
walk(fixtureRoot);
for (const path of claimed) fail("unprovisioned-native-claim", path);

// 3. Provisioned leg: only when explicitly qualified. The durable
//    restart/race battery runs through cargo with the native marker;
//    a provisioned run without the toolchain or the transactional DB
//    runner fails loudly. Env-var presence alone never qualifies: the
//    battery must actually execute and pass.
let nativeLeg = "pending-provisioning";
if (provisioned) {
  const database = process.env.LEKALO_TASKLINKS_NATIVE_DB;
  if (!database) {
    fail(
      "native-db-unprovisioned",
      "LEKALO_TASKLINKS_NATIVE_DB must name the transactional DB when LEKALO_TASKLINKS_NATIVE=1",
    );
  } else {
    // The battery itself is the proof: the pinned transactional runner
    // creates and opens the named DB file with WAL + FULL sync. A run
    // that does not execute and pass is a provisioning gap, never a
    // silent mock substitution.
    const rust = spawnSync(
      "cargo",
      [
        "test",
        "--locked",
        "-p",
        "lekalo-core",
        "--test",
        "task_links",
        "--",
        "--ignored",
        "--test-threads=1",
        "native_leg",
      ],
      {
        cwd: root,
        encoding: "utf8",
        timeout: 600000,
        env: {
          ...process.env,
          // Absolute diagnostic DB location, independent of caller cwd.
          // File/row custody is never CLI execution authority.
          LEKALO_TASKLINKS_NATIVE_DB: resolvePath(root, database),
        },
      },
    );
    if (rust.error || rust.status !== 0) {
      fail(
        "native-battery-failed",
        String(rust.error ?? (rust.stdout + rust.stderr).slice(0, 400)),
      );
    } else {
      // Four legs must execute and the battery must pass as a whole:
      // the process-kill leg legitimately interleaves the child
      // runner's banner into the parent's per-test output, so the
      // check anchors on the battery's own summary line (the killed
      // child never reaches a summary of its own).
      const passed = Number(rust.stdout.match(/test result: ok\. (\d+) passed/g)?.pop()?.match(/(\d+)/)[1] ?? 0);
      const started = (rust.stdout.match(/test native_leg\S+/g) ?? []).length;
      if (passed < 4 || started < 4) {
        fail("native-battery-empty", `passed=${passed} started=${started}`);
      } else {
        // Export-only diagnostics from the run this gate just observed.
        // Neither this receipt nor caller-writable DB rows can qualify
        // a later CLI report; the maintained owner executes separately.
        const receiptPath = resolvePath(root, database) + ".qualified.json";
        if (!existsSync(receiptPath)) {
          fail("native-receipt-missing", receiptPath);
        } else {
          let receipt;
          try {
            receipt = JSON.parse(readFileSync(receiptPath, "utf8"));
          } catch (error) {
            fail("native-receipt-shape", String(error).slice(0, 200));
          }
          if (receipt) {
            const requiredLegs = [
              "accepted_before_ack_survives_restart_as_unknown",
              "attempt_bound_survives_restart",
              "synchronized_identity_race_binds_one_task",
              "interrupted_transaction_recovers_durable_state",
            ];
            const boundary = "same-process-recovery-real-sqlite-process-kill";
            if (receipt.schemaVersion !== "lekalo/task-links-native-receipt/v0.8.0") {
              fail("native-receipt-schema", String(receipt.schemaVersion));
            }
            if (receipt.identity !== "dev.lekalo.task-links-native-receipt@0.8.0") {
              fail("native-receipt-identity", String(receipt.identity));
            }
            if (receipt.executionBoundary !== boundary) {
              fail(
                "native-receipt-boundary",
                String(receipt.executionBoundary),
              );
            }
            if (receipt.runner !== "cargo-test-lekalo-core-native_leg") {
              fail("native-receipt-runner", String(receipt.runner));
            }
            if (receipt.targetImplementation !== "lekalo-core-bundled-sqlite") {
              fail("native-receipt-target-implementation", String(receipt.targetImplementation));
            }
            if (receipt.targetProfile !== "cargo-test") {
              fail("native-receipt-target-profile", String(receipt.targetProfile));
            }
            if (!receipt.runId || receipt.runId.length === 0) {
              fail("native-receipt-run-id", String(receipt.runId));
            }
            const legs = Array.isArray(receipt.legs) ? receipt.legs : [];
            const legNames = legs.map(l => l.leg || l);
            for (const required of requiredLegs) {
              if (!legNames.includes(required)) {
                fail("native-receipt-leg-missing", required);
              }
            }
            const databasePath = resolvePath(root, database);
            if (!existsSync(databasePath)) {
              fail("native-db-missing", databasePath);
            } else {
              const header = readFileSync(databasePath).subarray(0, 16);
              if (Buffer.from(header).toString("latin1") !== "SQLite format 3\0") {
                fail("native-db-header", databasePath);
              }
            }
            // Read back the rows after this gate's observed subprocess
            // completed successfully. Row consistency records this observed
            // run; it is not authentication of a caller-imported artifact.
            const { DatabaseSync } = await import("node:sqlite").catch(() => ({}));
            if (!DatabaseSync) {
              fail(
                "node-sqlite-unavailable",
                "the provisioned gate verifies the battery-written durable rows through node:sqlite (Node >= 22.5)",
              );
            } else {
              const db = new DatabaseSync(databasePath, { readOnly: true });
              try {
                const rows = db
                  .prepare(
                    "SELECT leg, execution_boundary, runner, run_id, target_implementation, target_profile FROM qualification",
                  )
                  .all();
                if (rows.length !== requiredLegs.length) {
                  fail(
                    "native-qualification-row-count",
                    `expected ${requiredLegs.length}, got ${rows.length}`,
                  );
                }
                for (const row of rows) {
                  if (!requiredLegs.includes(row.leg)) {
                    fail("native-qualification-leg", String(row.leg));
                  }
                  if (row.execution_boundary !== boundary) {
                    fail("native-qualification-boundary", String(row.execution_boundary));
                  }
                  if (row.runner !== "cargo-test-lekalo-core-native_leg") {
                    fail("native-qualification-runner", String(row.runner));
                  }
                  if (row.run_id !== receipt.runId) {
                    fail("native-qualification-run-id", `${row.run_id} != ${receipt.runId}`);
                  }
                  if (row.target_implementation !== "lekalo-core-bundled-sqlite") {
                    fail("native-qualification-target-implementation", String(row.target_implementation));
                  }
                  if (row.target_profile !== "cargo-test") {
                    fail("native-qualification-target-profile", String(row.target_profile));
                  }
                }
              } finally {
                db.close();
              }
            }
            if (failures.length === 0) {
              nativeLeg = "local-battery-executed";
              // Independently check non-promotion on the named battery
              // target and unexecuted ports. A genuine gate execution is
              // recorded here; caller-readable files alone still cannot
              // confer authenticated execution custody on CLI reports.
              const binary = ["lekalo", "lekalo.exe"]
                .map((name) => join(root, "target", "debug", name))
                .find((candidate) => existsSync(candidate));
              if (!binary) {
                fail("binary-missing", "build the workspace first: cargo build --workspace --locked");
              } else {
                const scratch = mkdtempSync(join(root, "target", "native-gate-"));
                try {
                  // Do not re-label mock steps as a native scenario run.
                  // The battery executed its four tests, not this producer-
                  // authored trace. Even genuine local rows cannot supply
                  // authenticated scenario/input/build/result provenance
                  // to file admission; those guarantees must stay unknown.
                  const declaredRun = read("tests/fixtures/task-external-links/evidence/mock-run.json");
                  const evidencePath = join(scratch, "declared-run.json");
                  writeFileSync(evidencePath, JSON.stringify(declaredRun, null, 2));
                  const nativeEnv = {
                    ...process.env,
                    LEKALO_TASKLINKS_NATIVE: "1",
                    LEKALO_TASKLINKS_NATIVE_DB: databasePath,
                  };
                  const portability = (toProfile) =>
                    spawnSync(
                      binary,
                      [
                        "--no-cache",
                        "--json",
                        "task-links",
                        "portability",
                        "tests/fixtures/task-external-links/contracts/integration.json",
                        "--project",
                        "tests/fixtures/task-external-links/project",
                        "--from-profile",
                        "tests/fixtures/task-external-links/profiles/rust-core.json",
                        "--to-profile",
                        toProfile,
                        "--profile",
                        "permissive",
                        "--evidence",
                        evidencePath,
                      ],
                      { cwd: root, encoding: "utf8", timeout: 120000, env: nativeEnv },
                    );
                  const executed = portability("tests/fixtures/task-external-links/profiles/rust-core.json");
                  if (executed.status !== 0) {
                    fail("native-portability-executed-exit", String(executed.stderr).slice(0, 300));
                  } else {
                    const report = JSON.parse(executed.stdout).taskLinks;
                    const rowOf = (fact) => report.targetGuarantees.find((row) => row.fact === fact);
                    for (const fact of ["unique-index", "local-transaction", "durable-outbox"]) {
                      const row = rowOf(fact);
                      if (!row || row.provided !== "unknown" || row.evidenceSource !== "attachment-declaration") {
                        fail("native-file-origin-promotion", `${fact}: ${JSON.stringify(row)}`);
                      }
                    }
                    const cas = rowOf("compare-and-swap");
                    if (!cas || cas.provided !== "unknown") {
                      fail("native-cas-overclaimed", JSON.stringify(cas));
                    }
                    if (report.evidenceCoverage.nativeEvidenceIds.length !== 0) {
                      fail("native-evidence-coverage", JSON.stringify(report.evidenceCoverage));
                    }
                  }
                  const unexecuted = portability("tests/fixtures/task-external-links/profiles/node.json");
                  if (unexecuted.status !== 0) {
                    fail("native-portability-unexecuted-exit", String(unexecuted.stderr).slice(0, 300));
                  } else {
                    const report = JSON.parse(unexecuted.stdout).taskLinks;
                    // Only the execution-bound facts stay unknown for a
                    // target that never ran; `delivery-uncertainty` is a
                    // declared delivery-rule row, not an executed
                    // guarantee, and may stay supported on both sides.
                    const executionBound = new Set([
                      "unique-index",
                      "compare-and-swap",
                      "local-transaction",
                      "durable-outbox",
                    ]);
                    for (const row of report.targetGuarantees) {
                      if (executionBound.has(row.fact) && row.provided !== "unknown") {
                        fail("native-guarantee-crossed-targets", JSON.stringify(row));
                      }
                    }
                  }
                } finally {
                  rmSync(scratch, { recursive: true, force: true });
                }
              }
            }
          }
        }
      }
    }
  }
}

// Required maintained-port leg: the CLI owner launches fresh managed Laravel
// processes, synchronizes competing workers, kills at uncommitted/accepted
// boundaries and independently verifies full durable state. No copied native
// sidecar or success-labelled evidence can enter this channel.
if (provisioned && failures.length === 0) {
  const binary = ["lekalo", "lekalo.exe"].map(name => join(root, "target/debug", name)).find(existsSync);
  if (!binary) fail("binary-missing", "cargo build --workspace --locked");
  else {
    const run = (profile) => spawnSync(binary, ["--no-cache", "--json", "task-links", "portability",
      "tests/fixtures/task-external-links/contracts/integration.json", "--project", "tests/fixtures/task-external-links/project",
      "--from-profile", "tests/fixtures/task-external-links/profiles/node.json", "--to-profile", profile,
      "--profile", "permissive", "--execute-mock", "--execute-native"], { cwd: root, encoding: "utf8", timeout: 180000 });
    for (const profile of ["laravel", "node"]) {
      const result = run(`tests/fixtures/task-external-links/profiles/${profile}.json`);
      if (result.error || result.status !== 0) { fail("maintained-process-battery-failed", `${profile}: ${String(result.error ?? result.stderr).slice(0,300)}`); continue; }
      const report = JSON.parse(result.stdout).taskLinks;
      const native = report.executionProvenance.find(row => row.origin === "native");
      if (!native || native.profileId !== "php-laravel-maintained"
          || !native.implementation.includes("laravel-13.33.0-pdo-sqlite")) fail("native-execution-provenance", native);
      for (const obligation of ["process-interruption-rollback", "process-restart-full-context", "synchronized-competing-workers", "acceptance-before-ack-no-resend", "durable-attempt-bound"]) {
        if (!native?.assertions.includes(obligation)) fail("native-assertion-missing", obligation);
      }
      for (const row of report.targetGuarantees.filter(row => ["unique-index", "compare-and-swap", "local-transaction", "durable-outbox"].includes(row.fact))) {
        const expected = profile === "laravel" ? "supported" : "unknown";
        if (row.provided !== expected) fail("native-target-binding", row);
        if (expected === "supported" && (row.evidenceSource !== "native-evidence" || row.pin !== native.resultDigest || native.profileDigest !== report.toProfile.digest)) fail("native-result-binding", row);
      }
      if (profile === "node" && report.evidenceCoverage.nativeEvidenceIds.length !== 0) fail("unexecuted-target-coverage", report.evidenceCoverage);
    }
    if (failures.length === 0) nativeLeg = "maintained-port-process-battery-executed";
  }
}

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate,
    nativeLeg,
    note: nativeLeg === "maintained-port-process-battery-executed"
      ? "Rust SQLite and maintained Laravel 13.33.0/PDO SQLite executed: synchronized identity/lease/version races, ungraceful transaction and ack-window termination, full-context restart, durable retries; only the actual maintained profile earns native rows"
      : "mock evidence never satisfies the durable/race assertions; native coverage stays unclaimed until the qualified environment runs this gate with LEKALO_TASKLINKS_NATIVE=1",
  })}\n`,
);
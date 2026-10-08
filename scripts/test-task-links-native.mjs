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
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { resolve as resolvePath } from "node:path";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const gate = "task-links-native";
const failures = [];
const fail = (reason, detail) => failures.push({ reason, detail });
const provisioned = process.env.LEKALO_TASKLINKS_NATIVE === "1";

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
          // The absolute path keeps the battery, this gate, and the
          // CLI admission aligned no matter their working directories.
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
      const ran = rust.stdout.match(/test native_leg\S+ \.\.\. ok/g) ?? [];
      if (ran.length < 3) {
        fail("native-battery-empty", "the native_leg_* battery did not fully execute");
      } else {
        // The qualification receipt is the admission artifact: the CLI
        // admits native-origin evidence only against it. A battery run
        // that did not execute and pass leaves no (complete) receipt.
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
            ];
            const legs = Array.isArray(receipt.legs) ? receipt.legs : [];
            for (const required of requiredLegs) {
              if (!legs.includes(required)) {
                fail("native-receipt-leg-missing", required);
              }
            }
            if (receipt.identity !== "dev.lekalo.task-links-native-receipt@0.8.0") {
              fail("native-receipt-identity", String(receipt.identity));
            }
            if (failures.length === 0) {
              nativeLeg = "durable-restart-and-db-race-executed";
            }
          }
        }
      }
    }
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
    note: "mock evidence never satisfies the durable/race assertions; native coverage stays unclaimed until the qualified environment runs this gate with LEKALO_TASKLINKS_NATIVE=1",
  })}\n`,
);

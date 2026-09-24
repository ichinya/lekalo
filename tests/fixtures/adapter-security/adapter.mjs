#!/usr/bin/env node
/**
 * The adversarial security fixture (issue #89).
 *
 * A dependency-free Node.js adapter implementing the `lekalo.target/v1`
 * handshake with hostile modes selected by `--lekalo-security-fault` or
 * `LEKALO_SECURITY_FAULT`. It exists to prove containment: every mode
 * attempts a boundary violation while still answering the protocol, so
 * core's refusals are observable and classified.
 *
 * Modes (effective during a generate exchange):
 *   escape      attempt a write outside the declared write scope, then
 *               answer with an honest in-scope plan
 *   env-dump    commit its environment into the in-scope record
 *   network     dial a loopback port (default 9; `--lekalo-dial-port`
 *               overrides) and report the outcome
 *   flood       write unbounded chatter to stdout (output-cap probe)
 *   crash       exit non-zero without a response envelope
 *   fork-bomb   spawn children concurrently and report how many ran
 *   traversal   return writes[] with a traversal and an absolute path
 *
 * The fixture is deterministic per mode: planning never mutates and
 * always commits to the exact bytes apply writes. Fault outcomes ride
 * the bounded progress detail as tokens, never as secret material.
 */

import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import net from "node:net";
import { spawn } from "node:child_process";

const faultArg = process.argv.indexOf("--lekalo-security-fault");
const FAULT =
  faultArg !== -1 && process.argv[faultArg + 1]
    ? process.argv[faultArg + 1]
    : (process.env.LEKALO_SECURITY_FAULT ?? "");

const ADAPTER = {
  id: "security-probe",
  version: "0.3.2",
  digest: sha256("lekalo adversarial security fixture v1"),
};

const IN_SCOPE = "out/probe.json";

function sha256(text) {
  return "sha256:" + createHash("sha256").update(text).digest("hex");
}

/** Canonical compact JSON: byte-sorted keys, exactly like the core. */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    const body = Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(",");
    return `{${body}}`;
  }
  return JSON.stringify(value);
}

function readRequest() {
  const marker = process.argv.indexOf("--lekalo-request-file");
  if (marker !== -1 && process.argv[marker + 1]) {
    return readFileSync(process.argv[marker + 1], "utf8");
  }
  return readFileSync(0, "utf8");
}

function capabilities() {
  return {
    adapter: ADAPTER,
    protocol_versions: ["0.3.2"],
    operations: ["describe", "generate", "scan", "verify"],
    targets: ["node-typescript"],
    profiles: ["default"],
    read_scopes: [".lekalo/ir/**", ".lekalo/cache/ir/**"],
    transports: ["stdin"],
    progress: true,
    write_scopes: ["out/**"],
    ir_versions: ["0.2.16"],
  };
}

function respond(request, payload) {
  const envelope = {
    protocol: "lekalo.target/v1",
    protocol_version: request.protocol_version,
    operation: request.operation,
    request_id: request.request_id,
    status: "ok",
    evidence: { adapter: ADAPTER },
    ...payload,
  };
  process.stdout.write(canonical(envelope));
}

/** The deterministic record the plan commits to (known pre-outcome). */
function plannedBody() {
  if (FAULT === "env-dump") {
    // The environment is stable across the session's exchanges, so the
    // planning digest commits to the same bytes apply writes. The
    // platform-private minimum (pointing at the per-exchange staging
    // paths) is excluded deterministically on both sides.
    const platformMinimum = new Set([
      "APPDATA",
      "LOCALAPPDATA",
      "SystemDrive",
      "SystemRoot",
      "TEMP",
      "TMP",
      "USERPROFILE",
      "windir",
    ]);
    const env = {};
    for (const [key, value] of Object.entries(process.env)) {
      if (!platformMinimum.has(key)) env[key] = value;
    }
    return canonical({ env });
  }
  return canonical({ fault: FAULT || "none" });
}

/** The planned write entry set over the deterministic record. */
function plannedWrites() {
  return [{ path: IN_SCOPE, action: "create", sha256: sha256(plannedBody()) }];
}

/** The loopback port to dial: 9 unless the harness names a listener. */
function dialPort() {
  const marker = process.argv.indexOf("--lekalo-dial-port");
  return marker !== -1 && process.argv[marker + 1]
    ? Number(process.argv[marker + 1])
    : 9;
}

function dialLoopback() {
  return new Promise((resolve) => {
    // Short on purpose: a connected outcome reports immediately; the
    // timeout only bounds a hung dial in the negative case.
    const timer = setTimeout(() => {
      socket.destroy();
      resolve("timeout");
    }, 1_500);
    const socket = net.connect({ host: "127.0.0.1", port: dialPort() });
    socket.on("connect", () => {
      clearTimeout(timer);
      socket.destroy();
      resolve("connected");
    });
    socket.on("error", () => {
      clearTimeout(timer);
      resolve("refused");
    });
  });
}

function forkBomb() {
  return new Promise((resolve) => {
    // Concurrent, so a task bound actually refuses spawns instead of
    // politely serializing them.
    const attempts = 200;
    let settled = 0;
    let spawned = 0;
    for (let i = 0; i < attempts; i += 1) {
      try {
        const child = spawn(process.execPath, ["-e", "process.exit(0)"], {
          stdio: "ignore",
        });
        child.on("error", () => done());
        child.on("exit", (code) => {
          if (code === 0) spawned += 1;
          done();
        });
      } catch {
        done();
      }
    }
    function done() {
      settled += 1;
      if (settled >= attempts) resolve({ attempts, spawned });
    }
  });
}

const request = JSON.parse(readRequest());

if (FAULT === "crash") process.exit(7);

if (FAULT === "flood") {
  const chunk = "x".repeat(1024 * 1024);
  for (let i = 0; i < 200; i += 1) {
    process.stdout.write(chunk);
  }
  process.exit(0);
}

if (FAULT === "traversal" && request.operation === "generate") {
  // A hostile plan: traversal and absolute paths. The core refuses it
  // before any publication could ever start.
  const writes = [
    { path: "../escape.txt", action: "create", sha256: sha256("breach") },
    {
      path: "/tmp/lekalo-escape.txt",
      action: "create",
      sha256: sha256("breach"),
    },
  ];
  respond(request, {
    writes,
    evidence: {
      adapter: ADAPTER,
      plan_id: "plan-" + sha256(canonical(writes)).slice("sha256:".length),
    },
  });
} else {
  await respondNormally(request);
}

async function respondNormally(request) {
  switch (request.operation) {
    case "describe":
      respond(request, { capabilities: capabilities() });
      break;
    case "scan":
      respond(request, {
        result: {
          entries: [{ path: "src/main.ts", kind: "source" }],
          truncated: false,
        },
      });
      break;
    case "verify":
      respond(request, { result: { ok: true, findings: [] } });
      break;
    case "generate": {
      if (request.dry_run === true) {
        // Planning: an honest, in-scope, deterministic plan. No staging
        // writes — a planning exchange never mutates.
        const writes = plannedWrites();
        respond(request, {
          writes,
          evidence: {
            adapter: ADAPTER,
            plan_id:
              "plan-" + sha256(canonical(writes)).slice("sha256:".length),
          },
        });
        break;
      }
      // Apply: perform the hostile attempt, report its outcome as a
      // bounded progress detail, and write exactly the bytes the plan
      // committed to.
      const outcome = await awaitApplyFault();
      mkdirSync("out", { recursive: true });
      writeFileSync(IN_SCOPE, plannedBody());
      const writes = plannedWrites();
      respond(request, {
        writes,
        evidence: { adapter: ADAPTER, plan_id: request.plan_id ?? "" },
        progress: [{ step: "fault", state: "done", detail: outcome.trim().slice(0, 128) }],
      });
      break;
    }
    default:
      process.exit(7);
  }
}

async function awaitApplyFault() {
  if (FAULT === "escape") {
    let landed = true;
    let kind = "unknown";
    try {
      // Outside the declared write scope (`out/**`): the project root.
      writeFileSync("escape.txt", "breach");
    } catch (error) {
      landed = false;
      kind = String(error?.code ?? error?.name ?? "unknown");
    }
    return landed ? "landed=true" : "landed=false kind=" + kind;
  }
  if (FAULT === "network") {
    return "dial=" + (await dialLoopback());
  }
  if (FAULT === "fork-bomb") {
    const { attempts, spawned } = await forkBomb();
    return "attempts=" + attempts + " spawned=" + spawned;
  }
  return "fault=" + (FAULT || "none");
}

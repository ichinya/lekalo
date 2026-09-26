#!/usr/bin/env node
// Issue #72 client-SDK runtime gate: the generated TypeScript client
// passes wire-behavior contract tests against a scripted fake
// transport, and the Vue consumer fixture typechecks/compiles against
// the generated module. Node built-ins only; the Vue/type tooling is
// provisioned outside the checkout exactly like Ajv (CI does the
// same) and reached through NODE_PATH / LEKALO_AJV_NODE_PATH.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const harnessPath = join(repoRoot, "tests", "fixtures", "client-sdk", "consumer-harness.mjs");

// ---------------------------------------------------------------------------
// 1. Wire-behavior contract tests over the generated client.
// ---------------------------------------------------------------------------
const contractTest = spawnSync(
  process.execPath,
  [
    "--experimental-strip-types",
    "--no-warnings",
    "--test",
    join(repoRoot, "tests", "fixtures", "client-sdk", "client-contract.test.mts"),
  ],
  { stdio: "pipe" },
);
if (contractTest.status !== 0) {
  process.stderr.write(contractTest.stdout ?? Buffer.alloc(0));
  process.stderr.write(contractTest.stderr ?? Buffer.alloc(0));
  process.stderr.write(`${JSON.stringify({ ok: false, reason: "wire-contract-tests" }, null, 2)}\n`);
  process.exit(1);
}

// ---------------------------------------------------------------------------
// 2. The Vue consumer fixture typechecks (pinned TypeScript via NODE_PATH)
//    and compiles (pinned @vue/compiler-sfc via the harness).
// ---------------------------------------------------------------------------
const harness = await import(pathToFileURL(harnessPath).href);

const typecheck = await harness.typecheck();
if (typecheck.tooling === "typescript-unavailable") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "typescript-unavailable", detail: "provision typescript@5.9.3 through NODE_PATH" }, null, 2)}\n`,
  );
  process.exit(1);
}
if (!typecheck.ok) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "consumer-typecheck", detail: typecheck.diagnostics.slice(0, 8) }, null, 2)}\n`,
  );
  process.exit(1);
}

const vue = await harness.compileVueSfcs();
if (vue.tooling === "vue-compiler-unavailable") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "vue-compiler-unavailable", detail: "provision @vue/compiler-sfc@3.4.38 through NODE_PATH" }, null, 2)}\n`,
  );
  process.exit(1);
}
if (!vue.ok) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "consumer-vue-compile", detail: vue.diagnostics.slice(0, 8) }, null, 2)}\n`,
  );
  process.exit(1);
}

process.stdout.write(
  `${JSON.stringify(
    {
      ok: true,
      gate: "client-sdk-runtime",
      typecheck: typecheck.tooling,
      vue: vue.tooling,
      wireTests: "client-contract.test.mts",
    },
    null,
    2,
  )}\n`,
);

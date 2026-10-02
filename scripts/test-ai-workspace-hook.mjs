#!/usr/bin/env node
/**
 * Issue #37 integration gate: prove the lekalo-side AI Workspace
 * boundaries against the pinned upstream binary
 * (lee-to/ai-workspace@8fdf818fee757d24e723d657fc5d38614995e557,
 * package 1.5.0) over isolated temporary databases and synthetic
 * role-role fixtures. Dependency-free; requires the upstream binary
 * (AI_WORKSPACE_BIN or --upstream); run from the repo root.
 *
 * The binary is intentionally NOT built or downloaded by this gate:
 * CI runs the dependency-free phases (AI_WORKSPACE_BIN unset and
 * upstream absent skips the binary phases with an explicit recorded
 * reason — never a silent pass), while a developer with the pinned
 * checkout built (cargo build) runs the full proof locally.
 *
 * Phases:
 *   A. contracts gate re-run (dependency-free, always).
 *   B. optionality (dependency-free): the hook reports disabled /
 *      unavailable accurately with no upstream, no network, and no
 *      Lekalo state change; the binary probe with a fake executable
 *      fails closed to unknown-delivery.
 *   C. leaked-output quarantine (dependency-free): a fake upstream
 *      whose stderr carries private markers leaves none of them in the
 *      hook's result.
 *   D–H (pinned binary): config-first registration without auto-share
 *      (sentinel README/package/private docs stay unshared), group-
 *      scoped MCP reads exact approved schema bytes from a consumer
 *      while wrong-group and single-project scoping deny, the full
 *      hook send pipeline delivers a real service_changed event and
 *      verifies the target readback, unknown-delivery on a
 *      stderr-poisoned/unavailable upstream, and hostile inherited
 *      widening flags being forced off in every child.
 */
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const HOOK = join(REPO_ROOT, "scripts", "ai-workspace-hook.mjs");
const CONTRACTS_GATE = join(REPO_ROOT, "scripts", "test-ai-workspace-contracts.mjs");
const MANIFEST = join(REPO_ROOT, "tests", "fixtures", "ai-workspace", "routing-manifest.json");
const PROTOCOL_SCHEMA = "contracts/target-protocol.schema.v0.3.2.json";

const sha256Ref = (buffer) => `sha256:${createHash("sha256").update(buffer).digest("hex")}`;

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "ai-workspace-hook", reason, detail: String(detail ?? "").slice(0, 2000) }, null, 2)}\n`);
  process.exit(1);
};

const argOf = (name) => {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : null;
};
const UPSTREAM = argOf("--upstream") ?? process.env.AI_WORKSPACE_BIN ?? null;

const run = (command, argv, options = {}) => {
  const result = spawnSync(command, argv, {
    encoding: "utf8",
    timeout: options.timeoutMs ?? 120_000,
    cwd: options.cwd ?? REPO_ROOT,
    input: options.input,
    env: options.env,
    windowsHide: true,
  });
  return {
    status: result.status,
    stdout: typeof result.stdout === "string" ? result.stdout : "",
    stderr: typeof result.stderr === "string" ? result.stderr : "",
    error: result.error,
  };
};

const runHook = (argv, options = {}) => run(process.execPath, [HOOK, ...argv], options);

// Closed hook state vocabulary (issue-37-implementation.md): every
// `state` the hook prints must be a member. Observed states are
// collected across the gate and asserted at the end.
const HOOK_STATES = new Set([
  "planned", "delivered", "unknown-delivery", "refused", "disabled", "unavailable", "no-change",
]);
const observedHookStates = new Set();
const observeStates = (output) => {
  try {
    const parsed = JSON.parse(output);
    if (parsed && typeof parsed.state === "string") observedHookStates.add(parsed.state);
  } catch {
    // Non-JSON output (quiet mode); the state asserts already checked it.
  }
};
const runHookObserved = (argv, options = {}) => {
  const out = runHook(argv, options);
  observeStates(out.stdout);
  return out;
};

const mcpCall = (upstream, dbPath, cwd, calls, options = {}) => {
  const requests = calls.map((call, index) => ({
    jsonrpc: "2.0",
    id: index + 1,
    method: "tools/call",
    params: { name: call.name, arguments: call.arguments ?? {} },
  }));
  const result = run(upstream, ["serve", "--group", options.group ?? "lekalo-dev"], {
    cwd,
    input: `${requests.map((request) => JSON.stringify(request)).join("\n")}\n`,
    timeoutMs: 30_000,
    env: {
      ...process.env,
      AI_WORKSPACE_DB: dbPath,
      AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0",
      AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0",
    },
  });
  if (result.status !== 0 && !result.stdout.trim()) {
    fail("mcp-server-died", `${result.stderr.slice(0, 400)}`);
  }
  const responses = result.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
  return calls.map((call, index) => {
    const response = responses.find((message) => message.id === index + 1);
    assert.ok(response, `no MCP response for ${call.name}`);
    if (response.error) {
      return { denied: true, error: response.error, text: null, json: null };
    }
    const text = response.result?.content?.find((entry) => entry.type === "text")?.text ?? "";
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {
      json = null;
    }
    return { denied: false, error: null, text, json };
  });
};

const upstream = (dbPath, argv, options = {}) =>
  run(UPSTREAM, argv, {
    ...options,
    env: {
      ...process.env,
      AI_WORKSPACE_DB: dbPath,
      AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0",
      AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0",
      ...(options.env ?? {}),
    },
  });

// One synthetic role checkout: empty share config BEFORE init (the
// auto-share suppressor), sentinel private files that must never leak.
const roleCheckout = (parent, role, extraFiles = {}) => {
  const root = join(parent, role);
  mkdirSync(root, { recursive: true });
  // Config first: its presence is what suppresses upstream auto-share.
  writeFileSync(join(root, ".ai-workspace.local.json"), `${JSON.stringify({
    ai_workspace_config_version: 1,
    name: role,
    slug: role,
    groups: ["lekalo-dev"],
    share: [],
    notes: [],
  }, null, 2)}\n`);
  // Sentinels: public-looking and private files that must never be
  // auto-shared or readable through MCP.
  writeFileSync(join(root, "README.md"), `${role} AUTO-SENTINEL must-stay-unshared\n`);
  writeFileSync(join(root, "package.json"), `${JSON.stringify({ name: `@sentinel/${role}`, private: true })}\n`);
  mkdirSync(join(root, "private"), { recursive: true });
  writeFileSync(join(root, "private", "secrets.md"), `${role} PRIVATE-SENTINEL token=deadbeef\n`);
  for (const [name, content] of Object.entries(extraFiles)) {
    const path = join(root, name);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, content);
  }
  return root;
};

const cleanup = [];
const tempRoot = () => {
  const dir = mkdtempSync(join(tmpdir(), "lekalo-i37-"));
  cleanup.push(dir);
  return dir;
};

try {
  // ------------------------------------------------------------------
  // Phase A: contracts gate (dependency-free; always runs).
  // ------------------------------------------------------------------
  const contracts = run(process.execPath, [CONTRACTS_GATE], {
    env: { ...process.env, NODE_PATH: process.env.LEKALO_AJV_NODE_PATH ?? process.env.NODE_PATH ?? "" },
  });
  assert.equal(contracts.status, 0, `contracts gate failed: ${contracts.stderr.slice(0, 600)}`);

  // ------------------------------------------------------------------
  // Phase B: optionality without any upstream (dependency-free).
  // ------------------------------------------------------------------
  {
    const dir = tempRoot();
    // A no-op range closes as `no-change` — a real upstream is never
    // invoked for an event with nothing to announce (T3 unrelated-
    // file/identical-range cases).
    const noop = runHookObserved(["--base", "HEAD", "--manifest", MANIFEST], { cwd: REPO_ROOT });
    assert.equal(noop.status, 0, "hook no-change plan must not fail");
    const noChange = JSON.parse(noop.stdout);
    assert.equal(noChange.state, "no-change");
    assert.equal(noChange.reason, "no-approved-artifact-changed");

    // A range with an approved-path change produces a planned envelope.
    const out = runHookObserved(["--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST], { cwd: REPO_ROOT });
    assert.equal(out.status, 0, "hook plan must not fail without upstream");
    const planned = JSON.parse(out.stdout);
    assert.equal(planned.state, "planned");
    assert.match(planned.eventKey, /^sha256:[0-9a-f]{64}$/);
    assert.ok(existsSync(HOOK), "hook exists");

    // Explicit send without the operator override: refused at the
    // policy gate (upstream is never invoked unadmitted), and the
    // private outbox records the attempt.
    const outbox = join(dir, "outbox");
    const db = join(dir, "unused.db");
    const refused = runHookObserved(["--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST, "--send", "--outbox", outbox, "--db", db, "--upstream", join(dir, "definitely-not-here.exe")], { cwd: REPO_ROOT });
    assert.equal(refused.status, 3, "unadmitted send must refuse");
    const refusal = JSON.parse(refused.stdout);
    assert.equal(refusal.state, "refused");
    assert.equal(refusal.reason, "policy-not-admitted");
    const recorded = JSON.parse(readFileSync(join(outbox, "outbox.json"), "utf8"));
    const entry = recorded.entries.find((candidate) => candidate.eventKey === refusal.eventKey);
    assert.ok(entry, "refusal recorded in the outbox");
    assert.ok(entry.states.some((state) => state.state === "planned"));
    assert.ok(entry.states.some((state) => state.state === "refused" && state.reason === "policy-not-admitted"));

    // With the explicit override, a missing upstream reports
    // `unavailable` accurately — no crash, no partial delivery.
    const outbox2 = join(dir, "outbox2");
    const missing = runHookObserved(["--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST, "--send", "--outbox", outbox2, "--db", db, "--upstream", join(dir, "definitely-not-here.exe"), "--allow-unadmitted-send"], { cwd: REPO_ROOT });
    assert.equal(missing.status, 0, "missing upstream is unavailable, not a crash");
    const unavailable = JSON.parse(missing.stdout);
    assert.equal(unavailable.state, "unavailable");
    assert.equal(unavailable.reason, "upstream-binary-missing");
    const recorded2 = JSON.parse(readFileSync(join(outbox2, "outbox.json"), "utf8"));
    const entry2 = recorded2.entries.find((candidate) => candidate.eventKey === unavailable.eventKey);
    assert.ok(entry2.states.some((state) => state.state === "unavailable"), "unavailable recorded in the outbox");

    // A PRESENT-but-unhealthy upstream (probe exits nonzero) reports
    // the precise `upstream-probe-nonzero` reason on the PUBLIC result —
    // not the collapsed missing-binary literal (round-2 M7).
    const outbox3 = join(dir, "outbox3");
    const unhealthyJs = join(dir, "unhealthy-fake.js");
    writeFileSync(unhealthyJs, "process.exit(7);\n");
    const unhealthyLauncher = process.platform === "win32" ? join(dir, "unhealthy.cmd") : join(dir, "unhealthy.sh");
    const exePath = process.execPath.split("\\").join("/");
    const jsPath = unhealthyJs.split("\\").join("/");
    writeFileSync(unhealthyLauncher, process.platform === "win32"
      ? `@"${exePath}" "${jsPath}" %*\r\n`
      : `#!/bin/sh\nexec "${exePath}" "${jsPath}" "$@"\n`);
    if (process.platform !== "win32") chmodSync(unhealthyLauncher, 0o755);
    const unhealthy = runHookObserved(["--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST, "--send", "--outbox", outbox3, "--db", db, "--upstream", unhealthyLauncher, "--allow-unadmitted-send"], { cwd: REPO_ROOT });
    assert.equal(unhealthy.status, 0, "unhealthy upstream is unavailable, not a crash");
    const probeNonzero = JSON.parse(unhealthy.stdout);
    assert.equal(probeNonzero.state, "unavailable");
    assert.equal(probeNonzero.reason, "upstream-probe-nonzero", "emitted reason must distinguish a nonzero probe from a missing binary");
    const recorded3 = JSON.parse(readFileSync(join(outbox3, "outbox.json"), "utf8"));
    const entry3 = recorded3.entries.find((candidate) => candidate.eventKey === probeNonzero.eventKey);
    assert.ok(entry3.states.some((state) => state.state === "unavailable" && state.reason === "upstream-probe-nonzero"), "outbox records the same precise reason");
    assert.equal(unavailable.reason === probeNonzero.reason, false, "the two failure shapes must stay distinguishable on the wire");

    // Staged (index) modifications to a tracked file taint the digest
    // just like unstaged ones: the hook must refuse worktree-dirty
    // (round-2 devin minor 1).
    const dirtyStaged = runHookObserved(["--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST], { cwd: REPO_ROOT });
    // (control first: the tree is clean at this point in the gate)
    assert.equal(dirtyStaged.status, 0);
    const sentinelDoc = join(REPO_ROOT, "docs/target-protocol.md");
    const originalDoc = readFileSync(sentinelDoc, "utf8");
    try {
      writeFileSync(sentinelDoc, `${originalDoc}\n<!-- staged-taint probe -->\n`);
      const stagedAdd = run("git", ["add", "docs/target-protocol.md"], { cwd: REPO_ROOT });
      assert.equal(stagedAdd.status, 0);
      const stagedTaint = runHookObserved(["--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST], { cwd: REPO_ROOT });
      assert.equal(stagedTaint.status, 3, "a staged modification of a tracked file must refuse worktree-dirty");
      assert.equal(JSON.parse(stagedTaint.stdout).reason, "worktree-dirty");
    } finally {
      writeFileSync(sentinelDoc, originalDoc);
      run("git", ["reset", "--quiet", "--", "docs/target-protocol.md"], { cwd: REPO_ROOT });
      // The worktree byte content is restored; if the gate itself is
      // mid-run on a dirty tree the earlier control already handled it.
    }
    const recovered = runHookObserved(["--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST], { cwd: REPO_ROOT });
    assert.equal(recovered.status, 0, "restoring the staged file must restore the clean state");

    // Usage errors never reflect arbitrary non-flag tokens
    // (round-2 devin minor 2): a stray positional is reduced to a
    // bounded marker.
    const stray = runHookObserved(["C:\\Users\\alice\\topsecret-positional"], { cwd: REPO_ROOT });
    assert.equal(stray.status, 2, "a stray positional is a usage error");
    const strayResult = JSON.parse(stray.stdout);
    assert.equal(strayResult.reason, "usage");
    assert.equal(strayResult.detail, "unexpected-positional", "a stray positional must not be echoed");
    assert.equal(stray.stdout.includes("alice"), false, "the positional value leaked into usage output");
  }

  // ------------------------------------------------------------------
  // Phase C: leaked-output quarantine (dependency-free). A hostile
  // upstream whose create fails with private-marker stderr — while its
  // version probe succeeds — must not have any of those markers
  // re-enter the hook result.
  // ------------------------------------------------------------------
  {
    const dir = tempRoot();
    const poison = "C:\\Users\\alice\\topsecret\\consumer-checkout token=hunter2";
    // Cross-platform fake upstream: probe exits 0; MCP reads (serve
    // with stdin JSON-RPC) get valid empty responses; the create fails
    // with poisoned stderr. The hook invokes it as `[node] fake.js ...`
    // by pointing --upstream at a launcher (the hook execs one binary),
    // so on Windows a .cmd launcher forwards to node.
    const fakeJs = join(dir, "poison-fake.js");
    writeFileSync(fakeJs, `const poison = ${JSON.stringify(poison)};
const argv = process.argv.slice(2);
if (argv[0] === "--version") { console.log("ai-workspace 1.5.0"); process.exit(0); }
if (argv[0] === "serve") {
  let input = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => { input += chunk; });
  process.stdin.on("end", () => {
    // The graph shape is injected by the gate through the inherited
    // LEKALO_FAKE_GRAPH environment variable (the hook passes its
    // environment through to children unchanged).
    let injected = { links: [] };
    try { injected = JSON.parse(process.env.LEKALO_FAKE_GRAPH ?? ""); } catch {}
    for (const line of input.split("\\n").filter(Boolean)) {
      let id = null;
      let name = "";
      try { const parsed = JSON.parse(line); id = parsed.id; name = parsed.params?.name ?? ""; } catch {}
      const payload = name === "workspace_events" ? (process.env.LEKALO_FAKE_EVENTS ? JSON.parse(process.env.LEKALO_FAKE_EVENTS) : []) : injected;
      process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id, result: { content: [{ type: "text", text: JSON.stringify(payload) }] } }) + "\\n");
    }
  });
  return;
}
process.stderr.write(poison + "\\n");
process.exit(1);
`);
    const launcher = process.platform === "win32" ? join(dir, "poison.cmd") : join(dir, "poison.sh");
    writeFileSync(launcher, process.platform === "win32"
      ? `@"${process.execPath}" "${fakeJs}" %*
`
      : `#!/bin/sh\nexec "${process.execPath}" "${fakeJs}" "$@"\n`);
    const fake = launcher;
    if (process.platform !== "win32") chmodSync(fake, 0o755);
    const outbox = join(dir, "outbox");
    // The fake graph links exactly the two declared consumers, so the
    // recipient preflight passes and the poisoned create is reached.
    const linkedGraph = {
      links: [
        { from: "greenfield-consumer", to: "lekalo-core" },
        { from: "brownfield-consumer", to: "lekalo-core" },
      ],
    };
    const out = runHookObserved([
      "--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST, "--send", "--outbox", outbox, "--db", join(dir, "x.db"),
      "--upstream", fake, "--allow-unadmitted-send",
    ], { cwd: REPO_ROOT, env: { ...process.env, LEKALO_FAKE_GRAPH: JSON.stringify(linkedGraph) } });
    assert.equal(out.status, 4, "failed send is unknown-delivery");
    const result = JSON.parse(out.stdout);
    assert.equal(result.state, "unknown-delivery");
    assert.equal(result.reason, "upstream-nonzero", "closed reason present");
    const flat = `${out.stdout}${out.stderr}`.toLowerCase();
    for (const marker of ["alice", "topsecret", "hunter2", "consumer-checkout"]) {
      assert.equal(flat.includes(marker), false, `poison leaked: ${marker}`);
    }
    const recorded = JSON.parse(readFileSync(join(outbox, "outbox.json"), "utf8"));
    const entry = recorded.entries.find((candidate) => candidate.eventKey === result.eventKey);
    assert.ok(entry.states.some((state) => state.state === "unknown-delivery"));
    assert.equal(result.reason, "upstream-nonzero", "closed failure reason present on unknown-delivery");
  }

  // Dependency-free phases are complete; the binary phases need the
  // pinned upstream executable.
  for (const state of observedHookStates) {
    assert.ok(HOOK_STATES.has(state), `hook printed an undocumented state: ${state}`);
  }
  if (!UPSTREAM || !existsSync(UPSTREAM)) {
    const reason = UPSTREAM ? "upstream-binary-path-missing" : "upstream-binary-not-configured (set AI_WORKSPACE_BIN to run the full proof)";
    process.stdout.write(`${JSON.stringify({ ok: true, gate: "ai-workspace-hook", binaryPhases: "skipped", reason })}\n`);
    for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  } else {
  // ------------------------------------------------------------------
  // Binary setup: one shared temp workspace over synthetic role
  // checkouts. The "core" role is a synthetic copy of the real
  // checkout's public contract files: upstream resolves projects by
  // cwd, so registering the real checkout would write operator state
  // into it (its .ai-workspace.local.json must stay operator-owned).
  // All registration is config-first (auto-share off).
  // ------------------------------------------------------------------
  const root = tempRoot();
  const dbPath = join(root, "workspace.db");
  const coreRoot = roleCheckout(root, "lekalo-core", {
    [PROTOCOL_SCHEMA]: readFileSync(join(REPO_ROOT, PROTOCOL_SCHEMA)),
    "docs/target-protocol.md": readFileSync(join(REPO_ROOT, "docs/target-protocol.md")),
    "docs/adr/0025-target-protocol.md": readFileSync(join(REPO_ROOT, "docs/adr/0025-target-protocol.md")),
    "contracts/trace-manifest.schema.v0.2.16.json": readFileSync(join(REPO_ROOT, "contracts/trace-manifest.schema.v0.2.16.json")),
  });
  const consumerRoot = roleCheckout(root, "greenfield-consumer");
  const otherConsumerRoot = roleCheckout(root, "brownfield-consumer");
  const strangerRoot = roleCheckout(root, "unrelated-project");

  const initIn = (dir) => {
    const role = dir.split(/[\\/]/).pop();
    const result = upstream(dbPath, ["--config", ".ai-workspace.local.json", "init", "--name", role, "--slug", role, "--group", "lekalo-dev"], { cwd: dir });
    assert.equal(result.status, 0, `init failed in ${role}: ${result.stderr.slice(0, 300)}`);
    return result;
  };

  // Phase D: config-first registration suppresses auto-share (B3).
  {
    initIn(consumerRoot);
    initIn(otherConsumerRoot);
    initIn(strangerRoot);
    initIn(coreRoot);

    // Sentinel probe via group-scoped MCP from the consumer: none of
    // the sentinels may be visible before any explicit share.
    const [context, search] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_context" },
      { name: "workspace_search", arguments: { query: "AUTO-SENTINEL" } },
    ]);
    assert.equal(context.denied, false);
    const contextText = context.text ?? "";
    assert.equal(contextText.includes("README"), false, "sentinel README auto-shared");
    assert.equal(contextText.includes("package.json"), false, "sentinel package.json auto-shared");
    assert.equal(contextText.includes("secrets.md"), false, "sentinel secret visible");
    const searchList = Array.isArray(search.json) ? search.json : [];
    assert.equal(searchList.length, 0, `auto-share sentinel found in search: ${contextText.slice(0, 200)}`);
  }

  // Phase E: explicit shares; exact bytes over group-scoped MCP (AC3).
  let schemaItem = null;
  let consumerReadText = null;
  {
    const share = upstream(dbPath, ["--config", ".ai-workspace.local.json", "share", PROTOCOL_SCHEMA, "--label", "Lekalo target protocol 0.3.2"], { cwd: coreRoot });
    assert.equal(share.status, 0, `share failed: ${share.stderr.slice(0, 300)}`);

    // Consumer MCP: context finds the share; read returns exact bytes.
    const [context] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_context" },
    ]);
    assert.equal(context.denied, false);
    assert.ok((context.text ?? "").includes("target-protocol.schema"), "share not visible in consumer context");
    assert.equal((context.text ?? "").includes("secrets.md"), false, "private sentinel leaked into context");

    // The schema is real JSON: extract the shared item id from the
    // context listing (upstream nests shared_items under each
    // project), then read it and compare exact bytes.
    const items = (context.json?.projects ?? []).flatMap((project) => project.shared_items ?? []);
    schemaItem = items.find((item) => (item.path ?? "").endsWith(PROTOCOL_SCHEMA));
    assert.ok(schemaItem, "schema item missing from shared_items");
    const [read] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_read", arguments: { item_id: schemaItem.id } },
    ]);
    assert.equal(read.denied, false, "consumer read denied");
    const committed = readFileSync(join(coreRoot, PROTOCOL_SCHEMA));
    consumerReadText = read.text;
    assert.equal(sha256Ref(Buffer.from(read.text, "utf8")), sha256Ref(committed), "shared bytes diverge from committed bytes");
  }

  // Wrong-group and single-project denials (B4/AC5 companion).
  {
    // Group-scoped visibility (access ≠ impact): the share is readable
    // from any group member's cwd via the same item id.
    const [readStranger] = mcpCall(UPSTREAM, dbPath, strangerRoot, [
      { name: "workspace_read", arguments: { item_id: schemaItem.id } },
    ], { group: "lekalo-dev" });
    assert.equal(readStranger.denied, false, "group share must be readable group-wide");
    assert.equal(readStranger.text, consumerReadText, "group-wide read diverges");

    // Single-project scope pinned to the CONSUMER must NOT read the
    // core share even by explicit item id (scope denies the project).
    const result = run(UPSTREAM, ["serve", "--project", "greenfield-consumer"], {
      cwd: consumerRoot,
      input: `${JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name: "workspace_read", arguments: { item_id: schemaItem.id } } })}\n`,
      env: { ...process.env, AI_WORKSPACE_DB: dbPath, AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0", AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0" },
    });
    const responses = result.stdout.split("\n").map((line) => { try { return JSON.parse(line); } catch { return null; } }).filter(Boolean);
    const response = responses.find((message) => message.id === 1);
    assert.ok(response, "no response from single-project server");
    const deniedOrError = response.error || response.result?.isError
      || /Access denied|not shared|requires explicit opt-in|Invalid shared item|not found/i.test(response.result?.content?.[0]?.text ?? "");
    assert.ok(deniedOrError, "single-project scope read the core share");
  }

  // Project-wide confinement with the flags off (AC5).
  {
    // Resolve the core project id from the workspace listing instead
    // of assuming registration order.
    const [contextForIds] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_context" },
    ]);
    const coreProjectId = (contextForIds.json?.projects ?? []).find((project) => project.slug === "lekalo-core")?.id;
    assert.ok(Number.isInteger(coreProjectId), "core project id unresolved");
    // Positive control: the tree/grep tools work over the shared scope
    // (the approved schema path is visible) — an empty/denied response
    // must not pass the confinement assertions below.
    const requests = [
      { id: 1, name: "project_tree", arguments: { project_id: coreProjectId } },
      { id: 2, name: "project_file_write", arguments: { project_id: coreProjectId, path: "pwned.txt", content: "no" } },
      { id: 3, name: "project_grep", arguments: { project_id: coreProjectId, pattern: "lekalo/target" } },
      { id: 4, name: "project_grep", arguments: { project_id: coreProjectId, pattern: "AUTO-SENTINEL" } },
    ];
    const result = run(UPSTREAM, ["serve", "--group", "lekalo-dev"], {
      cwd: consumerRoot,
      input: `${requests.map((request) => JSON.stringify({ jsonrpc: "2.0", id: request.id, method: "tools/call", params: { name: request.name, arguments: request.arguments } })).join("\n")}\n`,
      env: { ...process.env, AI_WORKSPACE_DB: dbPath, AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0", AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0" },
    });
    const responses = result.stdout.split("\n").map((line) => { try { return JSON.parse(line); } catch { return null; } }).filter(Boolean);
    const responseFor = (id) => responses.find((message) => message.id === id);

    const tree = responseFor(1);
    assert.ok(tree, "no project_tree response");
    const treeText = tree.result?.content?.[0]?.text ?? "";
    assert.equal(Boolean(tree.error || tree.result?.isError), false, "project_tree denied (positive control failed)");
    assert.ok(treeText.includes("target-protocol.schema"), "project_tree positive control failed: shared schema not in tree");
    assert.equal(treeText.includes("README"), false, "project_tree leaked the sentinel README with flags off");
    assert.equal(treeText.includes("package.json"), false, "project_tree leaked the sentinel package.json");
    assert.equal(treeText.includes("secrets.md"), false, "project_tree leaked the private directory");

    const write = responseFor(2);
    assert.ok(write, "no project_file_write response");
    const writeDenied = write.error || write.result?.isError
      || /denied|disabled|not (found|available)|unknown tool|Access denied/i.test(write.result?.content?.[0]?.text ?? "");
    assert.ok(writeDenied, "project_file_write was accepted with the write flag off");

    const grepPositive = responseFor(3);
    assert.ok(grepPositive, "no project_grep positive response");
    const grepPositiveText = grepPositive.result?.content?.[0]?.text ?? "";
    assert.equal(Boolean(grepPositive.error || grepPositive.result?.isError), false, "project_grep denied over shared scope (positive control failed)");
    assert.ok(grepPositiveText.length > 0, "project_grep positive control returned nothing over the shared scope");

    const grep = responseFor(4);
    assert.ok(grep, "no project_grep response");
    const grepText = grep.result?.content?.[0]?.text ?? "";
    assert.equal(grepText.includes("AUTO-SENTINEL"), false, "project_grep matched unshared sentinel content");
  }

  // Genuine wrong-group denial (B4/AC5): a project in a different
  // group cannot read the core share even by explicit item id.
  {
    const outsideRoot = roleCheckout(root, "outside-project");
    const outsideInit = upstream(dbPath, ["--config", ".ai-workspace.local.json", "init", "--name", "outside-project", "--slug", "outside-project", "--group", "other-dev"], { cwd: outsideRoot });
    assert.equal(outsideInit.status, 0, `outside init failed: ${outsideInit.stderr.slice(0, 200)}`);
    const [outsideRead] = mcpCall(UPSTREAM, dbPath, outsideRoot, [
      { name: "workspace_read", arguments: { item_id: schemaItem.id } },
    ], { group: "other-dev" });
    const outsideDenied = outsideRead.denied || outsideRead.text === ""
      || /Access denied|not shared|requires explicit opt-in|Invalid shared item|not found/i.test(outsideRead.text ?? "");
    assert.ok(outsideDenied, "wrong-group project read the core share");
  }

  // ------------------------------------------------------------------
  // Phase F: the full hook pipeline end to end (AC2).
  // ------------------------------------------------------------------
  {
    // Service links consumer -> core; the upstream event impact set.
    for (const dir of [consumerRoot, otherConsumerRoot]) {
      const role = dir.split(/[\\/]/).pop();
      const link = upstream(dbPath, ["link", "add", role, "lekalo-core", "--kind", "depends_on", "--label", "target-protocol"], { cwd: dir });
      assert.equal(link.status, 0, `link add failed: ${link.stderr.slice(0, 300)}`);
    }
    // The stranger stays unlinked and must not appear as a target.

    // A real protocol change exists in history: 4a084aab modified
    // docs/target-protocol.md. The hook computes the envelope from the
    // real repository's committed history (its git operations bind to
    // the script's own repository root, not the cwd); the upstream
    // spawn runs from a registered project directory.
    const outbox = join(root, "outbox");
    const out = runHookObserved([
      "--base", "4a084aab~1", "--head", "4a084aab",
      "--manifest", MANIFEST,
      "--send", "--outbox", outbox, "--db", dbPath,
      "--upstream", UPSTREAM,
      "--allow-unadmitted-send",
    ], { cwd: consumerRoot });
    assert.equal(out.status, 0, `hook send failed: ${out.stderr.slice(0, 400)}${out.stdout.slice(0, 400)}`);
    const result = JSON.parse(out.stdout);
    assert.equal(result.state, "delivered", `hook state: ${result.state}`);
    assert.equal(result.eventKind, "protocol-change");
    assert.deepEqual([...result.affectedRoles].sort(), ["brownfield-consumer", "greenfield-consumer"]);
    assert.equal(result.policyAdmitted, false);

    // The event is real: read it back independently through MCP and
    // verify the target rows name exactly the two linked consumers.
    const [events] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_events", arguments: {} },
    ]);
    assert.equal(events.denied, false);
    const list = Array.isArray(events.json) ? events.json : [];
    assert.ok(list.length >= 1, "no event in the workspace");
    const event = list.find((candidate) => candidate.kind === "service_changed" && (candidate.title ?? "").includes("Lekalo"));
    assert.ok(event, "the service_changed event is missing");
    const [details] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_event_details", arguments: { event_id: event.id } },
    ]);
    assert.equal(details.denied, false);
    const targets = (details.json?.affected_services ?? [])
      .filter((target) => target.relation_kind === "linked_service")
      .map((target) => target.project)
      .sort();
    assert.deepEqual(targets, ["brownfield-consumer", "greenfield-consumer"],
      `event targets diverge from declared subscribers: ${JSON.stringify(targets)}`);
    assert.equal((details.json?.affected_services ?? []).some((target) => target.project === "unrelated-project"), false,
      "unlinked project appeared as a target");

    // Repeat after verified delivery is a no-op (no second event).
    const before = list.length;
    const repeat = runHookObserved([
      "--base", "4a084aab~1", "--head", "4a084aab",
      "--manifest", MANIFEST,
      "--send", "--outbox", outbox, "--db", dbPath,
      "--upstream", UPSTREAM,
      "--allow-unadmitted-send",
    ], { cwd: consumerRoot });
    assert.equal(repeat.status, 0);
    assert.equal(JSON.parse(repeat.stdout).state, "delivered");
    const [eventsAfter] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_events", arguments: {} },
    ]);
    assert.equal((Array.isArray(eventsAfter.json) ? eventsAfter.json : []).length, before,
      "verified-delivery repeat created a duplicate event");
  }

  // ------------------------------------------------------------------
  // Phase G: unknown-delivery on a failing upstream (T4 shape).
  // ------------------------------------------------------------------
  {
    const dir = tempRoot();
    const outbox = join(dir, "outbox");
    const poison = "PRIVATE-MARKER-α";
    // Same cross-platform JS fake as phase C: probe OK, reads OK (the
    // graph links the declared consumers), create fails with poison.
    const fakeJs = join(dir, "fail-fake.js");
    writeFileSync(fakeJs, `const poison = ${JSON.stringify(poison)};
const argv = process.argv.slice(2);
if (argv[0] === "--version") { console.log("ai-workspace 1.5.0"); process.exit(0); }
if (argv[0] === "serve") {
  let input = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk) => { input += chunk; });
  process.stdin.on("end", () => {
    let injected = { links: [] };
    try { injected = JSON.parse(process.env.LEKALO_FAKE_GRAPH ?? ""); } catch {}
    for (const line of input.split("\\n").filter(Boolean)) {
      let id = null;
      let name = "";
      try { const parsed = JSON.parse(line); id = parsed.id; name = parsed.params?.name ?? ""; } catch {}
      const payload = name === "workspace_events" ? [] : injected;
      process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id, result: { content: [{ type: "text", text: JSON.stringify(payload) }] } }) + "\\n");
    }
  });
  return;
}
process.stderr.write(poison + "\\n");
process.exit(3);
`);
    const launcher = process.platform === "win32" ? join(dir, "fail.cmd") : join(dir, "fail.sh");
    writeFileSync(launcher, process.platform === "win32"
      ? `@"${process.execPath}" "${fakeJs}" %*
`
      : `#!/bin/sh\nexec "${process.execPath}" "${fakeJs}" "$@"\n`);
    const fake = launcher;
    if (process.platform !== "win32") chmodSync(fake, 0o755);
    const linkedGraph = {
      links: [
        { from: "greenfield-consumer", to: "lekalo-core" },
        { from: "brownfield-consumer", to: "lekalo-core" },
      ],
    };
    const fakeEnv = { ...process.env, LEKALO_FAKE_GRAPH: JSON.stringify(linkedGraph) };
    const out = runHookObserved([
      "--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST, "--send", "--outbox", outbox, "--db", join(dir, "x.db"),
      "--upstream", fake, "--allow-unadmitted-send",
    ], { cwd: REPO_ROOT, env: fakeEnv });
    assert.equal(out.status, 4, "nonzero upstream is unknown-delivery");
    const failed = JSON.parse(out.stdout);
    assert.equal(failed.state, "unknown-delivery");
    assert.equal(failed.reason, "upstream-nonzero", "closed reason on the result");
    assert.equal(out.stdout.includes(poison), false, "poison reached the result");

    // Reconciliation: a repeat under the same key must reconcile by
    // key through the read surface BEFORE any second create. With the
    // failing fake (create always fails, never creates), the readback
    // finds nothing and the hook records the reconciled checkpoint.
    const before = JSON.parse(readFileSync(join(outbox, "outbox.json"), "utf8"));
    const firstEntry = before.entries.find((candidate) => candidate.eventKey === failed.eventKey);
    const statesBefore = firstEntry.states.length;
    const repeat = runHookObserved([
      "--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST, "--send", "--outbox", outbox, "--db", join(dir, "x.db"),
      "--upstream", fake, "--allow-unadmitted-send",
    ], { cwd: REPO_ROOT, env: fakeEnv });
    assert.equal(repeat.status, 4, "repeat over an unverified attempt is still unknown-delivery");
    const after = JSON.parse(readFileSync(join(outbox, "outbox.json"), "utf8"));
    const entryAfter = after.entries.find((candidate) => candidate.eventKey === failed.eventKey);
    assert.ok(
      entryAfter.states.some((state, index) => index >= statesBefore && state.state === "reconciled" && state.outcome === "not-found"),
      "the repeat recorded the keyed reconciliation checkpoint",
    );
    assert.ok(entryAfter.states.slice(0, statesBefore).every((state) => state.state !== "delivered"), "no silent delivery appeared");
  }

  // ------------------------------------------------------------------
  // Phase H: hostile inherited widening flags (T5 shape).
  // ------------------------------------------------------------------
  {
    const dir = tempRoot();
    const out = runHookObserved([
      "--base", "4a084aab~1", "--head", "4a084aab", "--manifest", MANIFEST, "--send", "--outbox", join(dir, "outbox"), "--db", join(dir, "x.db"),
      "--upstream", UPSTREAM, "--allow-unadmitted-send",
    ], {
      cwd: REPO_ROOT,
      env: { ...process.env, AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "1", AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "1" },
    });
    assert.equal(out.status, 3, "hostile widening flags must refuse the send");
    assert.equal(JSON.parse(out.stdout).reason, "widening-flags-enabled");
  }

  for (const state of observedHookStates) {
    assert.ok(HOOK_STATES.has(state), `hook printed an undocumented state: ${state}`);
  }
  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "ai-workspace-hook", binaryPhases: "executed", upstream: "configured", hookStates: [...observedHookStates].sort() })}\n`);
  }
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "ai-workspace-hook", reason: String(error?.message ?? error).slice(0, 600), stack: String(error?.stack ?? "").split("\n").slice(0, 4).join(" | ") }, null, 2)}\n`);
  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.exit(1);
}

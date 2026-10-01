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
 * CI runs the dependency-free phases only (AI_WORKSPACE_BIN unset and
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
    const out = runHook(["--base", "HEAD", "--manifest", MANIFEST], { cwd: REPO_ROOT });
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
    const refused = runHook(["--base", "HEAD", "--manifest", MANIFEST, "--send", "--outbox", outbox, "--db", db, "--upstream", join(dir, "definitely-not-here.exe")], { cwd: REPO_ROOT });
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
    const missing = runHook(["--base", "HEAD", "--manifest", MANIFEST, "--send", "--outbox", outbox2, "--db", db, "--upstream", join(dir, "definitely-not-here.exe"), "--allow-unadmitted-send"], { cwd: REPO_ROOT });
    assert.equal(missing.status, 0, "missing upstream is unavailable, not a crash");
    const unavailable = JSON.parse(missing.stdout);
    assert.equal(unavailable.state, "unavailable");
    assert.equal(unavailable.reason, "upstream-binary-missing");
    const recorded2 = JSON.parse(readFileSync(join(outbox2, "outbox.json"), "utf8"));
    const entry2 = recorded2.entries.find((candidate) => candidate.eventKey === unavailable.eventKey);
    assert.ok(entry2.states.some((state) => state.state === "unavailable"), "unavailable recorded in the outbox");
  }

  // ------------------------------------------------------------------
  // Phase C: leaked-output quarantine (dependency-free). A hostile
  // upstream whose stderr carries private markers must not have any of
  // them re-enter the hook result.
  // ------------------------------------------------------------------
  {
    const dir = tempRoot();
    const poison = "C:\\Users\\alice\\topsecret\\consumer-checkout token=hunter2";
    const poisonSh = `#!/bin/sh\necho "${poison}" >&2\nexit 1\n`;
    const poisonCmd = `@echo ${poison} >&2\r\n@exit /b 1\r\n`;
    const fake = process.platform === "win32" ? join(dir, "poison.cmd") : join(dir, "poison.sh");
    writeFileSync(fake, process.platform === "win32" ? poisonCmd : poisonSh);
    if (process.platform !== "win32") chmodSync(fake, 0o755);
    const outbox = join(dir, "outbox");
    const out = runHook([
      "--base", "HEAD", "--manifest", MANIFEST, "--send", "--outbox", outbox, "--db", join(dir, "x.db"),
      "--upstream", fake, "--allow-unadmitted-send",
    ], { cwd: REPO_ROOT });
    assert.equal(out.status, 4, "failed send is unknown-delivery");
    const result = JSON.parse(out.stdout);
    assert.equal(result.state, "unknown-delivery");
    const flat = `${out.stdout}${out.stderr}`.toLowerCase();
    for (const marker of ["alice", "topsecret", "hunter2", "consumer-checkout"]) {
      assert.equal(flat.includes(marker), false, `poison leaked: ${marker}`);
    }
    const recorded = JSON.parse(readFileSync(join(outbox, "outbox.json"), "utf8"));
    const entry = recorded.entries.find((candidate) => candidate.eventKey === result.eventKey);
    assert.ok(entry.states.some((state) => state.state === "unknown-delivery"));
  }

  // Dependency-free phases are complete; the binary phases need the
  // pinned upstream executable.
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
    // context listing, then read it and compare exact bytes.
    const items = context.json?.shared_items ?? [];
    const schemaItem = items.find((item) => (item.path ?? "").endsWith(PROTOCOL_SCHEMA));
    assert.ok(schemaItem, "schema item missing from shared_items");
    const [read] = mcpCall(UPSTREAM, dbPath, consumerRoot, [
      { name: "workspace_read", arguments: { item_id: schemaItem.id } },
    ]);
    assert.equal(read.denied, false, "consumer read denied");
    const committed = readFileSync(join(coreRoot, PROTOCOL_SCHEMA));
    assert.equal(sha256Ref(Buffer.from(read.text, "utf8")), sha256Ref(committed), "shared bytes diverge from committed bytes");
  }

  // Wrong-group and single-project denials (B4/AC5 companion).
  {
    // Same-group share is readable group-wide (the stranger has no
    // link, but the share is visible context; access ≠ impact).
    const [read] = mcpCall(UPSTREAM, dbPath, strangerRoot, [
      { name: "workspace_read", arguments: { rel_path: PROTOCOL_SCHEMA } },
    ], { group: "lekalo-dev" });
    assert.equal(read.denied, false, "group share must be readable group-wide");

    // Single-project scope on the consumer must NOT see core shares.
    const result = run(UPSTREAM, ["serve", "--scope", "current-project"], {
      cwd: consumerRoot,
      input: `${JSON.stringify({ jsonrpc: "2.0", id: 1, method: "tools/call", params: { name: "workspace_read", arguments: { rel_path: PROTOCOL_SCHEMA } } })}\n`,
      env: { ...process.env, AI_WORKSPACE_DB: dbPath, AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "0", AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "0" },
    });
    const responses = result.stdout.split("\n").map((line) => { try { return JSON.parse(line); } catch { return null; } }).filter(Boolean);
    const response = responses.find((message) => message.id === 1);
    assert.ok(response, "no response from single-project server");
    assert.ok(response.error || !(response.result?.content?.[0]?.text ?? "").includes("target-protocol.schema"),
      "single-project scope leaked a core share");
  }

  // Project-wide/write tools must not exist with the flags off (AC5).
  {
    const requests = [1, 2].map((id) => ({
      jsonrpc: "2.0",
      id,
      method: "tools/call",
      params: { name: id === 1 ? "project_tree" : "project_file_write", arguments: id === 1 ? { project_id: 1 } : { path: "x.txt", content: "no" } },
    }));
    const result = run(UPSTREAM, ["serve", "--group", "lekalo-dev"], {
      cwd: consumerRoot,
      input: `${requests.map((request) => JSON.stringify(request)).join("\n")}\n`,
      env: { ...process.env, AI_WORKSPACE_DB: dbPath },
    });
    const responses = result.stdout.split("\n").map((line) => { try { return JSON.parse(line); } catch { return null; } }).filter(Boolean);
    for (const id of [1, 2]) {
      const response = responses.find((message) => message.id === id);
      assert.ok(response, `no response for direct tool ${id}`);
      const denied = response.error
        || /denied|disabled|not (found|available)|unknown tool|Access denied/i.test(response.result?.content?.[0]?.text ?? "")
        || (response.result?.isError ?? false);
      assert.ok(denied, `project-wide tool ${id === 1 ? "project_tree" : "project_file_write"} was callable with flags off`);
    }
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
    const out = runHook([
      "--base", "4a084aab~1", "--head", "4a084aab",
      "--manifest", MANIFEST,
      "--send", "--outbox", outbox, "--db", dbPath,
      "--upstream", UPSTREAM,
      "--allow-unadmitted-send",
      "--quiet",
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
    const repeat = runHook([
      "--base", "4a084aab~1", "--head", "4a084aab",
      "--manifest", MANIFEST,
      "--send", "--outbox", outbox, "--db", dbPath,
      "--upstream", UPSTREAM,
      "--allow-unadmitted-send",
      "--quiet",
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
    const fake = process.platform === "win32" ? join(dir, "fail.cmd") : join(dir, "fail.sh");
    writeFileSync(fake, process.platform === "win32" ? `@echo ${poison} >&2\r\n@exit /b 3\r\n` : `#!/bin/sh\necho "${poison}" >&2\nexit 3\n`);
    if (process.platform !== "win32") chmodSync(fake, 0o755);
    const out = runHook([
      "--base", "HEAD", "--manifest", MANIFEST, "--send", "--outbox", outbox, "--db", join(dir, "x.db"),
      "--upstream", fake, "--allow-unadmitted-send", "--quiet",
    ], { cwd: REPO_ROOT });
    assert.equal(out.status, 4, "nonzero upstream is unknown-delivery");
    assert.equal(JSON.parse(out.stdout).state, "unknown-delivery");
    assert.equal(out.stdout.includes(poison), false, "poison reached the result");
  }

  // ------------------------------------------------------------------
  // Phase H: hostile inherited widening flags (T5 shape).
  // ------------------------------------------------------------------
  {
    const dir = tempRoot();
    const out = runHook([
      "--base", "HEAD", "--manifest", MANIFEST, "--send", "--outbox", join(dir, "outbox"), "--db", join(dir, "x.db"),
      "--upstream", UPSTREAM, "--allow-unadmitted-send", "--quiet",
    ], {
      cwd: REPO_ROOT,
      env: { ...process.env, AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS: "1", AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE: "1" },
    });
    assert.equal(out.status, 3, "hostile widening flags must refuse the send");
    assert.equal(JSON.parse(out.stdout).reason, "widening-flags-enabled");
  }

  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.stdout.write(`${JSON.stringify({ ok: true, gate: "ai-workspace-hook", binaryPhases: "executed", upstream: "configured" })}\n`);
  }
} catch (error) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "ai-workspace-hook", reason: String(error?.message ?? error).slice(0, 600) }, null, 2)}\n`);
  for (const dir of cleanup) rmSync(dir, { recursive: true, force: true });
  process.exit(1);
}

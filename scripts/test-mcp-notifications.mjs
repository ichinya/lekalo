#!/usr/bin/env node
// Live stdio proof: subscriptions authorize, observe edits while stdin is idle,
// invalidate on failed reads, recover, and stop after unsubscribe. All source
// mutations are confined to a disposable synthetic planner copy.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = process.env.LEKALO_MCP_BIN ?? join(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
assert.ok(existsSync(binary), "build the checkout CLI first");
const stage = mkdtempSync(join(root, "target/mcp-notifications-"));
cpSync(join(root, "tests/fixtures/inspect/planner"), join(stage, "planner"), { recursive: true });
const path = join(stage, "planner/lekalo/modules/planner/commands.yaml");
const original = readFileSync(path, "utf8");
const uri = "lekalo://project/summary";
const child = spawn(binary, ["mcp", "serve", "--stdio", "--project", "planner"], {
  cwd: stage, env: { ...process.env, LEKALO_PROJECT: "" }, stdio: ["pipe", "pipe", "pipe"],
});
const frames = [];
let stderr = "";
let closed = false;
const completion = new Promise((resolveExit, rejectExit) => {
  child.once("error", rejectExit);
  child.once("close", code => { closed = true; resolveExit(code); });
});
child.stderr.on("data", chunk => { stderr += chunk; });
createInterface({ input: child.stdout }).on("line", line => { frames.push(JSON.parse(line)); });
const delay = ms => new Promise(resolveDelay => setTimeout(resolveDelay, ms));
async function waitFor(predicate, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const found = frames.find(predicate);
    if (found) return found;
    assert.equal(closed, false, `server exited early: ${stderr}`);
    await delay(25);
  }
  assert.fail(`notification/response deadline: ${stderr}`);
}
let nextId = 0;
async function request(method, params) {
  const id = ++nextId;
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  return waitFor(frame => frame.id === id);
}
const updates = () => frames.filter(frame => frame.method === "notifications/resources/updated");
const mutate = text => writeFileSync(path, original.replace(/description: [^\r\n]+/, `description: ${JSON.stringify(text)}`));
try {
  const initialized = await request("initialize", { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "notification-gate", version: "1" } });
  assert.equal(initialized.result.capabilities.resources.subscribe, true);
  assert.equal(initialized.result.capabilities.resources.listChanged, false, "catalog identity list is fixed at startup");
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
  const denied = await request("resources/subscribe", { uri: "lekalo://foreign/summary" });
  assert.equal(denied.error.code, -32602);
  const before = await request("resources/read", { uri });
  const beforeRevision = JSON.parse(before.result.contents[0].text).revision;
  assert.deepEqual((await request("resources/subscribe", { uri, _meta: {} })).result, {});
  assert.deepEqual((await request("resources/subscribe", { uri })).result, {}, "subscribe is idempotent");
  mutate("Notification checkpoint one");
  await waitFor(frame => frame.method === "notifications/resources/updated");
  const after = await request("resources/read", { uri });
  assert.notEqual(JSON.parse(after.result.contents[0].text).revision, beforeRevision);
  assert(updates().every(frame => !Object.hasOwn(frame, "id") && frame.params.uri === uri));
  const offset = frames.length;
  writeFileSync(path, "not a valid Model document\n");
  await waitFor(frame => frames.indexOf(frame) >= offset && frame.method === "notifications/resources/updated");
  assert((await request("resources/read", { uri })).error, "invalidated resource refuses rather than serving cached content");
  const recoveryOffset = frames.length;
  mutate("Notification checkpoint recovered");
  await waitFor(frame => frames.indexOf(frame) >= recoveryOffset && frame.method === "notifications/resources/updated");
  assert((await request("resources/read", { uri })).result);
  assert.deepEqual((await request("resources/unsubscribe", { uri })).result, {});
  const count = updates().length;
  mutate("Notification checkpoint unsubscribed");
  await delay(750);
  assert.deepEqual((await request("ping", {})).result, {});
  assert.equal(updates().length, count, "unsubscribe stops updates");
  assert.equal(existsSync(join(stage, "planner/.lekalo")), false, "watching creates no project state");
  child.stdin.end();
  assert.equal(await completion, 0, stderr);
  console.log(JSON.stringify({ ok: true, gate: "mcp-notifications", updates: count, scopeDenials: 1, recovered: true, unsubscribe: true }));
} finally {
  if (!closed) child.kill();
  await completion.catch(() => {});
  rmSync(stage, { recursive: true, force: true });
}

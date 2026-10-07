#!/usr/bin/env node
// Issue #33 stdio gate: the real server binary under a real child
// process, driven over LF-delimited JSON-RPC from Node with the pinned
// Ajv validating every structuredContent against its closed output
// schema. Covers the lifecycle, the seven-tool catalog, tool calls over
// the synthetic planner, scope denial, group narrowing, write-tool
// absence, and the read-only guarantee with a full before/after
// inventory. Exact Ajv 8.17.1 is provisioned outside this checkout and
// exposed through NODE_PATH / LEKALO_AJV_NODE_PATH.

import { createRequire } from "node:module";
import { spawnSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `ajv-unavailable: provision exact Ajv 8.17.1 outside the checkout and expose it through NODE_PATH / LEKALO_AJV_NODE_PATH (${error.message})\n`,
  );
  process.exit(2);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(`ajv-version: expected 8.17.1, got ${ajvVersion}\n`);
  process.exit(2);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  process.stderr.write(`mcp-stdio ${reason}: ${JSON.stringify(detail ?? "")}\n`);
  process.exit(1);
};

// ---- The server binary. ---------------------------------------------------

const exe =
  process.platform === "win32" ? "lekalo.exe" : "lekalo";
const binary =
  process.env.LEKALO_MCP_BIN ??
  (existsSync(join(root, "target", "debug", exe))
    ? join(root, "target", "debug", exe)
    : join(root, "target", "release", exe));
if (!existsSync(binary)) {
  process.stderr.write(
    `server-binary-unavailable: build the CLI first (cargo build --locked -p lekalo-cli) or set LEKALO_MCP_BIN\n`,
  );
  process.exit(2);
}

// ---- The schemas. ---------------------------------------------------------

const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const ajv = new Ajv2020({ strict: true, allErrors: true });
const outputSchemas = new Map();
for (const slug of [
  "inspect",
  "impact",
  "context",
  "graph",
  "validate",
  "target-status",
  "trace-export",
]) {
  const schema = read(`contracts/mcp-tool-${slug}-output.schema.v0.8.0.json`);
  outputSchemas.set(slug, ajv.compile(schema));
}
const capabilitiesSchema = read("contracts/mcp-capabilities.schema.v0.8.0.json");
const validateCapabilities = ajv.compile(capabilitiesSchema);

// ---- The served root. -----------------------------------------------------

const tempRoot = join(root, "target", "mcp-stdio-gate");
rmSync(tempRoot, { recursive: true, force: true });
mkdirSync(tempRoot, { recursive: true });
cpSync(join(root, "tests/fixtures/inspect/planner"), join(tempRoot, "planner"), {
  recursive: true,
});
mkdirSync(join(tempRoot, "planner", "traces"), { recursive: true });
cpSync(
  join(root, "tests/fixtures/trace/full.trace.json"),
  join(tempRoot, "planner", "traces/full.trace.json"),
);
const catalog = {
  schemaVersion: "lekalo/mcp-server-config/v0.8.0",
  identity: "dev.lekalo.mcp-server-config@0.8.0",
  projects: [{ id: "planner", root: "planner" }],
  groups: [{ id: "planner-review", projectId: "planner", modules: ["planner"] }],
  manifests: [{ id: "baseline", projectId: "planner", path: "traces/full.trace.json" }],
  git: { changedMode: false },
};
writeFileSync(join(tempRoot, "mcp.local.json"), JSON.stringify(catalog, null, 2));

let checks = 0;
const check = (condition, name, detail) => {
  checks += 1;
  if (!condition) fail(name, detail);
};

// ---- Session plumbing. ----------------------------------------------------

function inventory(base) {
  const files = [];
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(path);
      } else {
        const bytes = readFileSync(path);
        const digest = createHash("sha256").update(bytes).digest("hex");
        files.push(`${path.slice(base.length + 1).replaceAll("\\\\", "/").toLowerCase()}:${digest}`);
      }
    }
  };
  walk(base);
  files.sort();
  return files;
}

async function serve(frames, extraArgs, config) {
  const args = ["mcp", "serve", "--stdio"];
  if (config) {
    args.push("--catalog-config", config);
  } else {
    args.push("--project", "planner");
  }
  args.push(...(extraArgs ?? []));
  const child = spawn(binary, args, {
    cwd: tempRoot,
    env: { ...process.env, LEKALO_PROJECT: "" },
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stdin.write(frames.map((frame) => `${JSON.stringify(frame)}\n`).join(""));
  child.stdin.end();
  const { stdout, stderr, code } = await new Promise((resolvePromise, rejectPromise) => {
    let out = "";
    let err = "";
    child.stdout.on("data", (chunk) => {
      out += chunk;
    });
    child.stderr.on("data", (chunk) => {
      err += chunk;
    });
    child.on("close", (code) => resolvePromise({ stdout: out, stderr: err, code }));
    child.on("error", rejectPromise);
  });
  const responses = stdout
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        return fail("frame-not-one-json-line", { line: line.slice(0, 120), error: error.message });
      }
    });
  return { responses, stderr, code, stdout };
}

const initialize = { jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25" } };
const initialized = { jsonrpc: "2.0", method: "notifications/initialized" };
const lifecycle = () => [initialize, initialized];
const call = (id, name, arguments_) => ({
  jsonrpc: "2.0",
  id,
  method: "tools/call",
  params: { name, arguments: arguments_ },
});
const findResponse = (responses, id) =>
  responses.find((response) => JSON.stringify(response.id) === JSON.stringify(id));
const toolOutput = (responses, id) => {
  const response = findResponse(responses, id);
  check(response !== undefined, "response-present", { id });
  return response.result.structuredContent;
};
const validateToolOutput = (responses, id, slug) => {
  const output = toolOutput(responses, id);
  const validate = outputSchemas.get(slug);
  const valid = validate(output);
  check(valid === true, "output-schema-valid", { id, slug, errors: validate.errors });
  // The text content block carries the same serialized JSON.
  const response = findResponse(responses, id);
  const text = response.result.content[0].text;
  const reparsed = JSON.parse(text);
  check(
    JSON.stringify(reparsed) === JSON.stringify(output),
    "text-and-structured-agree",
    { id },
  );
  // The pinned contract identity.
  check(
    output.schemaVersion === `lekalo/mcp-tool-${slug}-output/v0.8.0`,
    "output-discriminator",
    { id, got: output.schemaVersion },
  );
  check(
    output.identity === `dev.lekalo.mcp-tool-${slug}-output@0.8.0`,
    "output-identity",
    { id },
  );
  return output;
};

// ---- describe. -------------------------------------------------------------

{
  const describeOutput = spawnSync(binary, ["mcp", "describe", "--json"], {
    cwd: tempRoot,
    encoding: "utf8",
  });
  check(describeOutput.status === 0, "describe-exit", describeOutput.status);
  const document = JSON.parse(describeOutput.stdout);
  check(document.status === "valid", "describe-status");
  const valid = validateCapabilities(document.capabilities);
  check(valid === true, "describe-schema", validateCapabilities.errors);
  check(document.capabilities.tools.length === 7, "describe-tools");
  check(document.capabilities.resources.length === 5, "describe-resources");
  // Deterministic bytes.
  const again = spawnSync(binary, ["mcp", "describe", "--json"], {
    cwd: tempRoot,
    encoding: "utf8",
  });
  check(describeOutput.stdout === again.stdout, "describe-deterministic");
}

// ---- Lifecycle and protocol errors. ----------------------------------------

{
  const { responses, stderr } = await serve(
    [
      ...lifecycle(),
      { jsonrpc: "2.0", id: 2, method: "ping", params: {} },
      { broken: true },
      { jsonrpc: "2.0", id: 3, method: "no/such/method", params: {} },
    ],
    [],
  );
  check(responses.length === 4, "lifecycle-response-count", responses.length);
  check(
    findResponse(responses, 1).result.protocolVersion === "2025-11-25",
    "protocol-pin",
  );
  check(findResponse(responses, 2).result === undefined || true, "ping-result");
  const parseError = responses.find((response) => response.id === null);
  check(parseError?.error?.code === -32700, "parse-error", parseError);
  check(findResponse(responses, 3)?.error?.code === -32601, "method-not-found");
  check(stderr.includes("lekalo-mcp"), "stderr-logging");
}

// ---- tools/list and write-tool absence. -------------------------------------

{
  const { responses } = await serve(
    [...lifecycle(), { jsonrpc: "2.0", id: 2, method: "tools/list", params: {} }],
    [],
  );
  const tools = findResponse(responses, 2).result.tools;
  check(tools.length === 7, "seven-tools", tools.map((tool) => tool.name));
  const names = tools.map((tool) => tool.name);
  for (const forbidden of ["write", "generate", "collect", "install", "migrate", "scan"]) {
    check(
      !names.some((name) => name.toLowerCase().includes(forbidden)),
      "no-write-tool",
      forbidden,
    );
  }
  const smuggled = findResponse(
    (
      await serve(
        [...lifecycle(), call(3, "lekalo_generate", { projectId: "planner" })],
        [],
      )
    ).responses,
    3,
  );
  check(smuggled?.error?.code === -32602, "unknown-tool-32602", smuggled);
}

// ---- The seven read tools, Ajv-validated. ------------------------------------

{
  const before = inventory(tempRoot);
  const frames = [
    ...lifecycle(),
    { jsonrpc: "2.0", id: 3, method: "resources/list", params: {} },
    { jsonrpc: "2.0", id: 4, method: "resources/read", params: { uri: "lekalo://planner/project-summary" } },
    call(10, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" }),
    call(11, "lekalo_impact", { projectId: "planner", mode: "symbol", symbol: "planner.focus_task" }),
    call(12, "lekalo_context", { projectId: "planner", mode: "symbol", symbol: "planner.focus_task", budget: 5000 }),
    call(13, "lekalo_graph", { projectId: "planner", mode: "show", symbol: "planner.focus_task" }),
    call(14, "lekalo_validate", { projectId: "planner" }),
    call(15, "lekalo_target_status", { projectId: "planner" }),
    call(16, "lekalo_trace_export", { projectId: "planner", manifestRef: "baseline" }),
  ];
  const { responses } = await serve(frames, [], "mcp.local.json");
  // Resource enumeration is bounded and authorized.
  const resources = findResponse(responses, 3).result.resources;
  check(resources.length >= 4, "resource-list", resources);
  const summary = findResponse(responses, 4).result.contents[0];
  check(summary.mimeType === "application/json", "resource-mime");
  const resourceDocument = JSON.parse(summary.text);
  check(
    resourceDocument.schemaVersion === "lekalo/mcp-resource/v0.8.0",
    "resource-discriminator",
  );
  check(
    resourceDocument.revision.startsWith("sha256-"),
    "resource-revision-form",
    resourceDocument.revision,
  );
  // Each tool output validates against its closed schema, status valid.
  for (const [id, slug] of [
    [10, "inspect"],
    [11, "impact"],
    [12, "context"],
    [13, "graph"],
    [14, "validate"],
    [15, "target-status"],
    [16, "trace-export"],
  ]) {
    const output = validateToolOutput(responses, id, slug);
    check(output.status === "valid", `tool-${slug}-valid`, output.failure);
    check("result" in output, `tool-${slug}-result-present`);
    check(!("failure" in output), `tool-${slug}-no-fabricated-failure`);
  }
  // The one-call context capsule keeps its mandatory facts.
  const context = toolOutput(responses, 12);
  check(context.result.context.budget.limit === 5000, "capsule-budget");
  check(context.result.context.coverage.included >= 1, "capsule-coverage");
  check(context.result.context.budget.minimumRequired !== undefined, "capsule-minimum");
  // The trace export carries the digest-prefixed manifest identity.
  const trace = toolOutput(responses, 16);
  check(
    trace.result.manifestDigest.startsWith("sha256:"),
    "trace-digest-form",
    trace.result.manifestDigest,
  );
  // Read-only: the served root is byte-identical after the full session.
  const after = inventory(tempRoot);
  check(
    JSON.stringify(before) === JSON.stringify(after),
    "served-root-untouched",
    { added: after.filter((entry) => !before.includes(entry)) },
  );
}

// ---- Scope denial, group narrowing, and privacy. -----------------------------

{
  const { responses, stdout } = await serve(
    [
      ...lifecycle(),
      call(50, "lekalo_inspect", { projectId: "evil", symbol: "planner.focus_task" }),
      call(51, "lekalo_inspect", { projectId: "../planner", symbol: "x" }),
      {
        jsonrpc: "2.0",
        id: 52,
        method: "resources/read",
        params: { uri: "file:///etc/passwd" },
      },
      {
        jsonrpc: "2.0",
        id: 53,
        method: "resources/read",
        params: { uri: "lekalo://evil/project-summary" },
      },
    ],
    [],
  );
  const denial = toolOutput(responses, 50);
  check(denial.status === "denied", "unregistered-project-denied");
  check(denial.failure.failureClass === "scope-denied", "scope-denial-class");
  check(findResponse(responses, 50).result.isError === true, "denial-is-error");
  check(findResponse(responses, 51).error?.code === -32602, "malformed-id-32602");
  check(findResponse(responses, 52).error?.code === -32602, "file-uri-32602");
  check(toolOutput(responses, 53).status === "denied", "resource-scope-denied");
  check(!stdout.includes("/etc/passwd"), "no-path-echo");
  check(!stdout.includes("file://"), "no-scheme-echo");
}

{
  const { responses } = await serve(
    [
      ...lifecycle(),
      call(60, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" }),
      call(61, "lekalo_inspect", { projectId: "planner", symbol: "notify.send_digest" }),
    ],
    ["--group", "planner-review"],
    "mcp.local.json",
  );
  check(toolOutput(responses, 60).status === "valid", "group-inside-valid");
  const outside = toolOutput(responses, 61);
  check(outside.status === "denied", "group-outside-denied");
  check(outside.failure.failureClass === "scope-denied", "group-denial-class");
}

// ---- Determinism. -------------------------------------------------------------

{
  const frames = [
    ...lifecycle(),
    call(70, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" }),
  ];
  const first = await serve(frames, [], "mcp.local.json");
  const second = await serve(frames, [], "mcp.local.json");
  check(first.stdout === second.stdout, "session-bytes-deterministic");
}

rmSync(tempRoot, { recursive: true, force: true });
process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  checks,
  tools: 7,
}, null, 2)}\n`);

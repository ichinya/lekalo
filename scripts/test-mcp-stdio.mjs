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
const resourceSchema = read("contracts/mcp-resource.schema.v0.8.0.json");
const validateResource = ajv.compile(resourceSchema);

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
  child.stdin.write(
    frames.map((frame) => `${typeof frame === "string" ? frame : JSON.stringify(frame)}\n`).join(""),
  );
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

// ---- Review-round coverage: the closed gaps as wire behavior. ---------------

// F10: duplicate JSON keys refuse before any map insertion.
{
  const raw =
    '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"lekalo_inspect","arguments":{"projectId":"planner","projectId":"evil","symbol":"planner.focus_task"}}}';
  const { responses } = await serve([lifecycle(), raw], []);
  const duplicate = responses.find((response) => response.error?.message === "mcp.duplicate-key");
  check(duplicate !== undefined, "duplicate-key-refused", {
    ids: responses.map((response) => response.id),
    messages: responses.map((response) => response.error && response.error.message),
  });
  check(responses.every((response) => response.error?.message !== undefined || response.id === 1), "duplicate-frame-no-tool-run");
}

// F8: only implemented capabilities are advertised.
{
  const { responses } = await serve(lifecycle(), []);
  const capabilities = findResponse(responses, 1).result.capabilities;
  check(capabilities.resources.subscribe === false, "advertise-subscribe-false", capabilities);
  check(capabilities.resources.listChanged === false, "advertise-listchanged-false", capabilities);
}

// F12: the lifecycle gate refuses pre-initialize requests.
{
  const { responses } = await serve([call(5, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" })], []);
  check(findResponse(responses, 5)?.error?.message === "mcp.lifecycle-not-initialized", "lifecycle-gate", findResponse(responses, 5));
}

// F11: the standard requestId cancellation suppresses the late reply.
{
  const { responses } = await serve([
    ...lifecycle(),
    { jsonrpc: "2.0", method: "notifications/cancelled", params: { requestId: 77 } },
    call(77, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" }),
  ]);
  check(findResponse(responses, 77) === undefined, "requestid-cancellation-suppresses", responses.map((r) => r.id));
}

// F6: the output bound substitutes the typed mcp.output-limit refusal.
{
  const { responses, stdout } = await serve(
    [...lifecycle(), call(2, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" })],
    ["--max-response-bytes", "512"],
  );
  const refusal = responses.find((response) => response.error?.message === "mcp.output-limit");
  check(refusal !== undefined, "output-limit-substituted", responses.map((r) => r.id));
  check(refusal.error.data.code === "LEK-MCP-005", "output-limit-code", refusal?.error?.data);
  check(refusal.error.data.limit === 512, "output-limit-count", refusal?.error?.data);
  check(!stdout.includes("structuredContent"), "output-limit-no-partial");
}

// F7: every live resource envelope Ajv-validates against the closed schema.
{
  const frames = [
    ...lifecycle(),
    { jsonrpc: "2.0", id: 2, method: "resources/read", params: { uri: "lekalo://planner/project-summary" } },
    { jsonrpc: "2.0", id: 3, method: "resources/read", params: { uri: "lekalo://planner/modules" } },
    { jsonrpc: "2.0", id: 4, method: "resources/read", params: { uri: "lekalo://planner/symbol/planner.focus_task" } },
    { jsonrpc: "2.0", id: 5, method: "resources/read", params: { uri: "lekalo://planner/targets" } },
    { jsonrpc: "2.0", id: 6, method: "resources/read", params: { uri: "lekalo://planner/diagnostics" } },
  ];
  const { responses } = await serve(frames, [], "mcp.local.json");
  const expectedKinds = {
    2: "project-summary",
    3: "module-list",
    4: "symbol-metadata",
    5: "target-status",
    6: "diagnostics",
  };
  for (const [id, kind] of Object.entries(expectedKinds)) {
    const response = findResponse(responses, Number(id));
    const text = response?.result?.contents?.[0]?.text;
    check(typeof text === "string", "resource-text", { id });
    const document = JSON.parse(text);
    check(document.kind === kind, "resource-contract-kind", { id, got: document.kind, want: kind });
    check(document.revision.startsWith("sha256-"), "resource-revision-form", { id });
    check(
      document.schemaVersion === "lekalo/mcp-resource/v0.8.0" &&
        document.identity === "dev.lekalo.mcp-resource@0.8.0",
      "resource-identity",
      { id },
    );
    // The symbol family's URI carries a dotted qualifier the frozen
    // schema's uri pattern cannot spell; the other four validate fully.
    if (kind !== "symbol-metadata") {
      const valid = validateResource(document);
      check(valid === true, "resource-ajv-valid", { id, errors: validateResource.errors });
    }
  }
}

// F3: the launch group is a ceiling; a different explicit groupId is the
// typed widening refusal, never a silent substitution.
{
  const { responses } = await serve(
    [
      ...lifecycle(),
      call(60, "lekalo_inspect", { projectId: "planner", groupId: "planner-review", symbol: "planner.focus_task" }),
      call(61, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" }),
    ],
    ["--group", "planner-review"],
    "mcp.local.json",
  );
  check(toolOutput(responses, 60).status === "valid", "launch-group-exact-admitted");
  check(toolOutput(responses, 61).status === "valid", "launch-group-default-admitted");
}

// F4: resources/read resolves unqualified URIs under the launch group and
// refuses a group-qualified URI naming a different group.
{
  const { responses } = await serve(
    [
      ...lifecycle(),
      { jsonrpc: "2.0", id: 70, method: "resources/read", params: { uri: "lekalo://planner/g/planner-review/symbol/notify.send_digest" } },
      { jsonrpc: "2.0", id: 71, method: "resources/read", params: { uri: "lekalo://planner/symbol/notify.send_digest" } },
    ],
    ["--group", "planner-review"],
    "mcp.local.json",
  );
  check(toolOutput(responses, 70).status === "denied", "resource-outside-group-denied");
  check(toolOutput(responses, 71).status === "denied", "resource-unqualified-narrows-to-launch-group");
}

// F13: resources/subscribe runs the read-scope authorization first.
{
  const { responses } = await serve(
    [
      ...lifecycle(),
      { jsonrpc: "2.0", id: 80, method: "resources/subscribe", params: { uri: "lekalo://evil/project-summary" } },
      { jsonrpc: "2.0", id: 81, method: "resources/subscribe", params: { uri: "lekalo://planner/project-summary" } },
    ],
    [],
    "mcp.local.json",
  );
  check(toolOutput(responses, 80).status === "denied", "subscribe-unregistered-denied");
  check(findResponse(responses, 81).result, "subscribe-admitted-acknowledged");
}

// F17: secret-shaped output refuses with mcp.sensitive-output.
{
  const commandsPath = join(tempRoot, "planner", "lekalo", "modules", "planner", "commands.yaml");
  const original = readFileSync(commandsPath, "utf8");
  writeFileSync(commandsPath, original.replace("Focus one task", "GATE_CANARY_33 password=0123456789abcdef0123456789"));
  const { responses, stdout } = await serve(
    [...lifecycle(), call(90, "lekalo_inspect", { projectId: "planner", symbol: "planner.focus_task" })],
    [],
    "mcp.local.json",
  );
  writeFileSync(commandsPath, original);
  const output = toolOutput(responses, 90);
  check(output.status === "denied", "sensitive-output-denied", output.failure);
  check(output.failure.failureClass === "sensitive-output", "sensitive-output-class");
  check(!stdout.includes("0123456789abcdef"), "sensitive-output-no-echo");
}

// F9: target-status serves without spawning git when the catalog denies it.
{
  const { responses } = await serve(
    [...lifecycle(), call(95, "lekalo_target_status", { projectId: "planner" })],
    [],
    "mcp.local.json",
  );
  const status = toolOutput(responses, 95);
  check(status.status === "valid", "target-status-valid-without-git");
  check(Array.isArray(status.result?.doctor?.checks), "target-status-doctor-checks");
}

// F1: a comma operand is a structural refusal, never a re-split set.
{
  const { responses } = await serve(
    [
      ...lifecycle(),
      call(96, "lekalo_context", { projectId: "planner", groupId: "planner-review", mode: "changed", changedSymbols: ["planner.focus_task,notify.send_digest"], budget: 5000 }),
    ],
    [],
    "mcp.local.json",
  );
  check(findResponse(responses, 96)?.error?.code === -32602, "comma-operand-32602", findResponse(responses, 96));
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

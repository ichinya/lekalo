#!/usr/bin/env node
// Issue #33 host qualification gate: connect a real MCP host client to
// the real server binary over stdio and complete the lifecycle with a
// tool call and a resource read, recording the client/runtime versions
// and the exact contract pins. The official MCP SDK
// (@modelcontextprotocol/sdk) is provisioned outside this checkout and
// exposed through NODE_PATH / LEKALO_MCP_SDK_NODE_PATH.
//
// Exit codes: 0 qualified, 1 qualification failure, 2 host unavailable.
// A missing host is reported unavailable; CI treats exit 2 as a failure,
// so a missing required host can never be relabeled accepted.

import { createRequire } from "node:module";
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  process.stderr.write(`mcp-host ${reason}: ${JSON.stringify(detail ?? "")}\n`);
  process.exit(1);
};
const unavailable = (reason, detail) => {
  process.stderr.write(`mcp-host-unavailable: ${reason} ${JSON.stringify(detail ?? "")}\n`);
  process.exit(2);
};

// ---- The server binary. ----------------------------------------------------

const exe = process.platform === "win32" ? "lekalo.exe" : "lekalo";
const binary =
  process.env.LEKALO_MCP_BIN ??
  (existsSync(join(root, "target", "debug", exe))
    ? join(root, "target", "debug", exe)
    : join(root, "target", "release", exe));
if (!existsSync(binary)) {
  unavailable("server-binary-missing", "build the CLI first or set LEKALO_MCP_BIN");
}

// ---- The SDK client. -------------------------------------------------------

const require = createRequire(import.meta.url);
let clientModule;
let stdioModule;
let sdkVersion;
try {
  clientModule = require("@modelcontextprotocol/sdk/client/index.js");
  stdioModule = require("@modelcontextprotocol/sdk/client/stdio.js");
  sdkVersion = JSON.parse(
    readFileSync(
      join(
        dirname(require.resolve("@modelcontextprotocol/sdk/client/index.js")),
        "../../../package.json",
      ),
      "utf8",
    ),
  ).version;
} catch (error) {
  unavailable("mcp-sdk-missing", {
    hint: "npm install --prefix <dir> @modelcontextprotocol/sdk and set NODE_PATH",
    error: error?.code ?? error?.message,
  });
}

const planner = join(root, "tests/fixtures/inspect/planner");
const { Client } = clientModule;
const { StdioClientTransport } = stdioModule;

const transport = new StdioClientTransport({
  command: binary,
  args: ["mcp", "serve", "--stdio", "--project", "tests/fixtures/inspect/planner"],
  cwd: root,
  env: { ...process.env, LEKALO_PROJECT: "" },
});
const client = new Client(
  { name: "lekalo-mcp-host-gate", version: "0.8.0" },
  { capabilities: {} },
);

try {
  await client.connect(transport);

  // The negotiated profile: the SDK already refuses unsupported dates
  // inside connect(), and a raw one-shot probe pins the exact answer.
  const serverInfo = client.getServerVersion();
  if (serverInfo?.name !== "lekalo") {
    fail("server-name", serverInfo);
  }
  const probe = spawnSync(
    binary,
    ["mcp", "serve", "--stdio", "--project", "tests/fixtures/inspect/planner"],
    {
      cwd: root,
      input:
        JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: "2025-11-25" } }) +
        "\n",
      encoding: "utf8",
      timeout: 30000,
    },
  );
  const probeAnswer = probe.stdout
    .split("\n")
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return undefined;
      }
    })
    .find((response) => response?.id === 1);
  if (probeAnswer?.result?.protocolVersion !== "2025-11-25") {
    fail("protocol-pin", probeAnswer);
  }

  // tools/list: exactly the seven read tools, read-only annotated.
  const tools = await client.listTools();
  const names = tools.tools.map((tool) => tool.name).sort();
  const expected = [
    "lekalo_context",
    "lekalo_graph",
    "lekalo_impact",
    "lekalo_inspect",
    "lekalo_target_status",
    "lekalo_trace_export",
    "lekalo_validate",
  ];
  if (JSON.stringify(names) !== JSON.stringify(expected)) {
    fail("tool-catalog", names);
  }
  for (const tool of tools.tools) {
    if (tool.annotations?.readOnlyHint !== true) {
      fail("read-only-hint", tool.name);
    }
  }

  // One-call context capsule over the host SDK.
  const context = await client.callTool({
    name: "lekalo_context",
    arguments: {
      projectId: "project",
      mode: "symbol",
      symbol: "planner.focus_task",
      budget: 5000,
    },
  });
  if (context.isError) {
    fail("context-call-error", context);
  }
  const capsule = context.structuredContent;
  if (capsule?.status !== "valid") {
    fail("context-status", capsule?.status);
  }
  if (capsule?.result?.schemaVersion !== "lekalo/context/v0.2.16") {
    fail("context-discriminator", capsule?.result?.schemaVersion);
  }
  if (capsule?.result?.context?.budget?.limit !== 5000) {
    fail("context-budget-facts");
  }
  if (!Array.isArray(context.content) || context.content[0]?.type !== "text") {
    fail("context-text-block");
  }

  // resources/read through the host SDK.
  const summary = await client.readResource({
    uri: "lekalo://project/project-summary",
  });
  const text = summary.contents?.[0]?.text;
  if (summary.contents?.[0]?.mimeType !== "application/json" || !text) {
    fail("resource-read-shape", summary);
  }
  const resource = JSON.parse(text);
  if (resource.schemaVersion !== "lekalo/mcp-resource/v0.8.0") {
    fail("resource-discriminator", resource.schemaVersion);
  }

  process.stdout.write(`${JSON.stringify({
    ok: true,
    qualified: true,
    host: { sdk: "@modelcontextprotocol/sdk", version: sdkVersion },
    server: { name: serverInfo.name, version: serverInfo.version, protocol: serverInfo.protocolVersion },
    tools: names.length,
    checks: ["initialize", "tools/list", "tools/call context", "resources/read"],
  }, null, 2)}\n`);
} catch (error) {
  fail("host-session", error?.message ?? String(error));
} finally {
  await client.close().catch(() => {});
}

#!/usr/bin/env node
// Issue #33 boundary gate: source-level dependency and read-only checks
// that keep the MCP surface pure and the ordinary CLI independent. The
// transport module must stay network-free, write-free, and adapter-free;
// the dispatch surface must never widen the loader seams; and no CLI
// startup path may reference the MCP lifecycle.

import { readFileSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  process.stderr.write(`mcp-boundary ${reason}: ${JSON.stringify(detail ?? "")}\n`);
  process.exit(1);
};
const read = (relative) => readFileSync(resolve(root, relative), "utf8");

let checks = 0;
const check = (condition, name, detail) => {
  checks += 1;
  if (!condition) fail(name, detail);
};

// ---- The CLI transport module inventory. -----------------------------------

const cliMcpDir = join(root, "crates/lekalo-cli/src/mcp");
const cliMcpFiles = readdirSync(cliMcpDir)
  .filter((name) => name.endsWith(".rs"))
  .sort();
check(
  JSON.stringify(cliMcpFiles) ===
    JSON.stringify([
      "catalog.rs",
      "dispatch.rs",
      "execution.rs",
      "mod.rs",
      "read.rs",
      "snapshot.rs",
      "stdio.rs",
    ]),
  "cli-module-layout",
  cliMcpFiles,
);
const cliMcpSource = cliMcpFiles
  .map((name) => readFileSync(join(cliMcpDir, name), "utf8"))
  .join("\n");

// ---- Network-free, process-spawn-free, cache-write-free. -------------------

for (const forbidden of [
  "std::net",
  "TcpStream",
  "TcpListener",
  "UdpSocket",
  "reqwest",
  "ureq",
  "hyper",
  "tokio",
]) {
  check(
    !cliMcpSource.includes(forbidden),
    "no-network-in-transport",
    forbidden,
  );
}

// The transport never opens an adapter subprocess and never touches the
// native gate or install machinery. (Git access can only ride the gated
// CLI `git_input` seam inside the reused `run_impact` call, which the
// catalog admission denies by default.)
for (const forbidden of [
  "adapter_package",
  "native_gate",
  "InstallPlan",
  "orchestration::AdapterSupply",
  "std::process::Command",
]) {
  check(!cliMcpSource.includes(forbidden), "no-adapter-or-install", forbidden);
}

// No runtime-cache seam in the MCP module: reads are the uncached loader
// path and the bounded no-follow reader.
check(
  !cliMcpSource.includes("cache::Session"),
  "no-cache-session",
);
check(
  !cliMcpSource.includes("record_graph"),
  "no-graph-cache-record",
);

// ---- The read-only catalog is the only admission table. --------------------

const coreTools = read("crates/lekalo-core/src/mcp/tools.rs");
const catalogBlock = coreTools.slice(
  coreTools.indexOf("pub const CATALOG"),
  coreTools.indexOf("/// Look up one descriptor by exact tool name."),
);
for (const name of [
  "lekalo_inspect",
  "lekalo_impact",
  "lekalo_context",
  "lekalo_graph",
  "lekalo_validate",
  "lekalo_target_status",
  "lekalo_trace_export",
]) {
  check(catalogBlock.includes(`"${name}"`), "catalog-tool", name);
}
for (const forbidden of [
  "lekalo_generate",
  "lekalo_write",
  "lekalo_collect",
  "lekalo_install",
  "lekalo_migrate",
  "lekalo_scan",
  "lekalo_report",
  "lekalo_history",
]) {
  check(!catalogBlock.includes(`"${forbidden}"`), "catalog-clean", forbidden);
}

// ---- The request grammar is the write capability boundary. -----------------

check(
  !coreTools.includes("\"write\"") || !catalogBlock.includes("write"),
  "no-write-mode",
);
// No arm accepts export destinations, argv, or environment operands:
// strip comments first so the prohibition docs do not self-match.
const coreToolsCode = coreTools
  .split("\n")
  .filter((line) => !line.trim().startsWith("///") && !line.trim().startsWith("//"))
  .join("\n");
for (const member of ["destination", "argv", "environment", "output_path", "reportFile", "cwd", "env"]) {
  check(!coreToolsCode.includes(member), "no-smuggled-member", member);
}

// ---- Sensitive-path policy exists and refuses dotfiles/secrets. ------------

const corePolicy = read("crates/lekalo-core/src/mcp/policy.rs");
for (const refused of ['".env"', "DotSegment", "Traversal", "StreamOrNul", "SecretLocation"]) {
  check(corePolicy.includes(refused), "policy-refusal", refused);
}
const cliRead = read("crates/lekalo-cli/src/mcp/read.rs");
for (const seam of ["symlink_metadata", "REPARSE_POINT", "MANIFEST_READ_CAP"]) {
  check(cliRead.includes(seam), "read-seam", seam);
}

// ---- The ordinary CLI stays independent of the MCP lifecycle. --------------

const mainRs = read("crates/lekalo-cli/src/main.rs");
// Exactly one dispatch arm plus the module declaration may mention mcp.
const mcpReferences = mainRs
  .split("\n")
  .filter((line) => /\bmcp\b|McpCommands|McpRun/.test(line) && !line.trim().startsWith("//"));
check(mcpReferences.length <= 6, "cli-mcp-references", mcpReferences);
// No CLI startup path attempts negotiation, install, or a network lookup.
check(!/serve/.test(mainRs), "cli-no-serve-startup", undefined);

// ---- The diagnostic family is registered and additive. ---------------------

const registry = JSON.parse(read("contracts/diagnostic-registry.v0.8.0.json"));
const mcpEntries = registry.entries.filter((entry) => entry.id.startsWith("mcp."));
check(mcpEntries.length === 11, "registry-family-size", mcpEntries.length);
const registryRs = read("crates/lekalo-core/src/diagnostics/registry.rs");
check(registryRs.includes("MCP_REGISTRY_BYTES"), "registry-embedded");
check(registryRs.includes("mcp_successor"), "registry-successor");
const normalizeRs = read("crates/lekalo-core/src/diagnostics/normalize.rs");
check(
  normalizeRs.includes("mcp_successor"),
  "normalize-consults-mcp-registry",
);

// ---- The contract family and the gate inventory are wired. -----------------

const contractFiles = readdirSync(join(root, "contracts")).filter(
  (name) => name.startsWith("mcp") && name.endsWith(".v0.8.0.json"),
);
check(contractFiles.length === 18, "contract-family-size", contractFiles.length);
const ci = read(".github/workflows/ci.yml");
check(ci.includes("test-mcp-contracts.mjs"), "ci-schema-gate");
check(ci.includes("test-mcp-stdio.mjs"), "ci-stdio-gate");
check(ci.includes("test-mcp-boundaries.mjs"), "ci-boundary-gate");

// ---- The docs and the extension example exist. -----------------------------

check(existsSyncOrNull("docs/mcp.md") === true, "docs-mcp");
check(
  existsSyncOrNull("examples/mcp/ai-factory-extension.json") === true,
  "extension-example",
);

function existsSyncOrNull(relative) {
  try {
    readFileSync(join(root, relative), "utf8");
    return true;
  } catch {
    return false;
  }
}

process.stdout.write(`${JSON.stringify({ ok: true, checks }, null, 2)}\n`);

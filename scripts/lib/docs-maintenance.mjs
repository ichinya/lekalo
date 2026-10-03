// Issue #105: bounded public-document maintenance, never application authority.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, dirname, join, relative, isAbsolute, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
export const P0 = ["README.md", ...["architecture", "authority", "model", "project-layout", "target-protocol", "diagnostics", "adoption", "security", "integrations", "tutorial-greenfield-planner", "tutorial-brownfield-typescript", "roadmap"].map(x => `docs/${x}.md`)];
export const sha = bytes => `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
export const text = path => readFileSync(safePath(path), "utf8");
export function safePath(path, base = ROOT) {
  assert.equal(typeof path, "string");
  assert.ok(path && !isAbsolute(path) && !path.includes("\\") && !path.split("/").some(x => !x || x === ".." || x === "."), `unsafe documentation path: ${path}`);
  const result = resolve(base, path);
  assert.ok(result.startsWith(resolve(base) + sep), "path escapes root");
  return result;
}
export function filesUnder(path) {
  return readdirSync(path).sort().flatMap(name => {
    const next = join(path, name);
    return statSync(next).isDirectory() ? filesUnder(next) : [next];
  });
}
export function binary() {
  const result = join(ROOT, "target", "debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
  assert.ok(existsSync(result), "binary-missing: cargo build -p lekalo-cli --locked");
  return result;
}
export function discoverCommands(bin) {
  const rows = [];
  const visit = args => {
    assert.ok(args.length < 5, "unexpected help recursion");
    const result = spawnSync(bin, [...args, "--help"], { cwd: ROOT, encoding: "utf8", timeout: 10000, maxBuffer: 1024 * 1024 });
    assert.equal(result.status, 0, `help failed: ${args.join(" ")}`);
    assert.equal(result.stderr, "");
    const help = result.stdout.replaceAll("lekalo.exe", "lekalo");
    const children = [...(help.match(/Commands:\n([\s\S]*?)(?:\n\n|$)/)?.[1] ?? "").matchAll(/^  ([a-z][a-z-]*)\s{2,}/gm)].map(x => x[1]);
    if (args.length) rows.push({ id: `cli:${args.join(" ")}`, kind: "command", command: args.join(" "), usage: help.match(/^Usage: (.+)$/m)?.[1], helpDigest: sha(help), summary: help.split("\n")[0], leaf: children.length === 0 });
    children.forEach(child => visit([...args, child]));
  };
  visit([]);
  return rows.sort((a,b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}
const commandOwners = {
  load:"loader", migrate:"versioning", compatibility:"versioning", provider:"provider-contract", validate:"validation", inspect:"inspect", impact:"impact", context:"context", "context-budget":"context-budget", diff:"semantic-diff", graph:"graph", effects:"effect-graph", lock:"lockfile", update:"lockfile", trace:"trace-manifest", requirements:"requirements", transport:"transport-http", openapi:"openapi", "query-model":"query-model", expressions:"expressions", "storage-profile":"storage-engine-profile", classification:"classification", dataflow:"classification", privacy:"privacy-runtime", generate:"orchestration", verify:"orchestration", init:"bootstrap", module:"bootstrap", adapter:"adapter-install", cache:"cache", history:"run-history", doctor:"doctor", status:"doctor", readiness:"doctor", observe:"observed-mode", scan:"observed-mode", bindings:"bindings", contract:"contracted-mode", native:"native-gates", nfr:"nfr",
};
export function commandOwner(command) {
  const [top, sub] = command.split(" ");
  if (top === "adapter" && sub === "test") return "docs/adapter-conformance.md";
  if (top === "storage") {
    const owner = sub === "laravel-plan" ? "laravel-migrations" : sub === "introspect-check" ? "storage-introspection" : ["validate","project","diff","plan"].includes(sub) ? "storage-projection" : "storage-engine";
    return `docs/${owner}.md`;
  }
  assert.ok(commandOwners[top], `unowned command: ${command}`);
  return `docs/${commandOwners[top]}.md`;
}
const families = {
  "authority-contracts":"authority", "authority-matrix":"authority", "adapter-install-plan":"adapter-install", "adapter-inventory":"adapter-install", "adapter-manifest":"adapter-manifest", "ai-workspace-event":"integrations/ai-workspace", "artifact-manifest":"artifact-manifest", authorization:"authorization", cache:"cache", "ci-report":"ci-reports", "classification-policy":"classification", "client-sdk":"client-sdk", "context-budget-comparison":"context-budget", "context-budget-policy":"context-budget", "context-budget-profile":"context-budget", "context-budget-report":"context-budget", "context-capsule":"context", "contracted-declaration":"contracted-mode", "data-classification":"classification", "data-flow-report":"classification", diagnostic:"diagnostics", "diagnostic-registry":"diagnostics", doctor:"doctor", "effect-graph":"effect-graph", "error-contract":"error-contracts", "error-registry":"error-contracts", expressions:"expressions", "expressions-builtin-support":"expressions", "expressions-vectors":"expressions", "extended-effects":"extended-effects", "generate-check-receipt":"orchestration", graph:"graph", impact:"impact", implementation:"implementation", inspect:"inspect", "invariant-transition":"invariant-transition", "laravel-migration-input":"laravel-migrations", lock:"lockfile", model:"model", "native-gate-plan":"native-gates", "native-gate-policy":"native-gates", "native-gate-run":"native-gates", "native-gate-view":"native-gates", nfr:"nfr", "nfr-evidence":"nfr", "nfr-report":"nfr", "observed-index":"observed-mode", "observed-scan":"observed-mode", "orchestration-report":"orchestration", "php-operations-evidence":"php-laravel-operations", "php-operations-input":"php-laravel-operations", "php-operations-map":"php-laravel-operations", "php-routes-evidence":"@php", "php-routes-input":"@php", "php-routes-map":"@php", "php-types-evidence":"@php", "php-types-input":"@php", "php-types-map":"@php", "privacy-authorization-subject-profile":"privacy", "privacy-authorizing-evidence":"privacy", "privacy-cli-error":"privacy-runtime", "privacy-export":"privacy", "privacy-policy":"privacy", "provider-capabilities":"provider-contract", "query-model":"query-model", "reference-evaluation":"reference-evaluation", requirements:"requirements", "requirements-report":"requirements", "run-assertions":"run-history", "run-history-store":"run-history", "run-observation":"run-history", "run-record":"run-history", "scenario-ir":"scenario-ir", "scenario-run":"native-gates", "semantic-diff":"semantic-diff", "semantic-ids":"semantic-ids", "storage-engine":"storage-engine", "storage-engine-profile":"storage-engine-profile", "storage-introspection":"storage-introspection", "storage-migration-plan":"storage-engine", "storage-observation":"storage-introspection", "storage-projection":"storage-projection", "storage-rename-history":"storage-projection", "target-profile":"target-profile", "target-protocol":"target-protocol", "test-port":"native-gates", "trace-manifest":"trace-manifest", "transaction-concurrency":"transaction-concurrency", "transport-http":"transport-http", "validation-profile":"validation", "validation-profile.default":"validation", "validation-profile.strict":"validation", "validation-report":"validation",
};
export function contractRecords() {
  return readdirSync(join(ROOT,"contracts")).filter(x => x.endsWith(".json")).sort().map(name => {
    const family = name.replace(/(?:\.schema)?\.v\d.*$/, "").replace(/\.manifest\.json$/, "");
    let owner = families[family];
    assert.ok(owner, `unowned contract: ${name} (${family})`);
    if (family === "privacy-export" && name.includes(".output.")) owner = "privacy-runtime";
    const source = `contracts/${name}`;
    return { id:`contract:${source}`, kind:"contract", source, sourceDigest:sha(text(source)), owner:owner === "@php" ? "adapters/php-laravel/README.md" : `docs/${owner}.md`, status:"implemented", selection:"exact producer or accepted manifest reference; file presence is not current-version admission" };
  });
}
export function protocolRecords() {
  const result = [{ id:"protocol:compiled-ir", kind:"protocol", source:"crates/lekalo-core/src/ir/version.rs", owner:"docs/ir.md", status:"implemented" }];
  // Public contributor formats are maintenance protocols, not Model contracts.
  for(const name of readdirSync(join(ROOT,"tests/fixtures/suite/schema")).filter(x=>x.endsWith(".json")).sort()) {
    const source=`tests/fixtures/suite/schema/${name}`;
    result.push({id:`protocol:suite:${name}`,kind:"protocol",source,sourceDigest:sha(text(source)),owner:"docs/architecture.md",status:"implemented"});
  }
  for(const [id,source] of [["owners","scripts/test-docs-ownership.mjs"],["examples","scripts/test-docs-examples.mjs"]]) result.push({id:`protocol:docs:${id}`,kind:"protocol",source,owner:"docs/architecture.md",status:"implemented"});
  for (const operation of ["describe","scan","bind","validate","generate","verify","plan-clean","clean","plan-native"]) result.push({id:`protocol:target:${operation}`,kind:"protocol",source:"contracts/target-protocol.schema.v0.3.2.json",owner:"docs/target-protocol.md",status:"implemented"});
  for (const adapter of ["node-typescript","php-laravel"]) {
    const roots = filesUnder(join(ROOT,"adapters",adapter,"src"));
    const identities = new Set(roots.flatMap(path => [...readFileSync(path,"utf8").matchAll(/["'](lekalo\/[a-z0-9-]+\/v[0-9.]+)["']/g)].map(x=>x[1])));
    for (const identity of [...identities].sort()) result.push({id:`protocol:${adapter}:${identity}`,kind:"protocol",source:`adapters/${adapter}/src`,owner:`adapters/${adapter}/README.md`,status:"implemented"});
  }
  return result.sort((a,b)=>a.id.localeCompare(b.id,"en"));
}
export function assertPublicText(value) {
  assert.ok(!/https?:\/\/(?:private|internal|corp)[.-]|consumer-private-canary|credential-canary-105|tenant-private-canary/i.test(value), "private-content-canary");
  assert.ok(!/\b(?:postgres|mysql):\/\/[^\s]+:[^\s]+@/i.test(value), "connection-URL-in-public-docs");
}
export function fences(value) {
  return [...value.matchAll(/^```([^\n]*)\n([\s\S]*?)^```\s*$/gm)].map(m=>({info:m[1].trim(),body:m[2].trimEnd()}));
}
export function validateMetadata(records) {
  assert.equal(new Set(records.map(x=>x.id)).size,records.length,"duplicate surface owner");
  for (const row of records) {
    assert.ok(Object.keys(row).every(x=>["id","kind","command","usage","helpDigest","summary","leaf","source","sourceDigest","owner","status","selection"].includes(x)),"unknown ownership field");
    assert.ok(["command","global","contract","protocol"].includes(row.kind), "unknown surface kind");
    assert.ok(["implemented","planned","experimental"].includes(row.status), "unknown feature status");
    assert.ok(existsSync(safePath(row.owner)), `missing owner ${row.owner}`);
    assert.ok(text(row.owner).trim().length > 50, "empty documentation owner");
  }
}

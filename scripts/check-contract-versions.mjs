#!/usr/bin/env node
// Changed contracts take the product version of the commit changing them.
// Exception (frozen restore): re-adding a contract whose exact bytes
// already exist in that path's own git history is an archival restore of a
// historical artifact — its version is pinned by its content, so the
// product-version rule does not apply. Anything else (new contracts,
// edited content, version renames) still requires the product version.
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8", ...(args.length && typeof args[args.length - 1] === "object" ? args.pop() : {}) });
const product = readFileSync(resolve(root, "Cargo.toml"), "utf8")
  .match(/\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m)?.[1];
if (!product) throw new Error("workspace.package.version is missing");
const args = process.argv.slice(2);
if (args.length && (args.length !== 2 || args[0] !== "--base")) {
  throw new Error("usage: node scripts/check-contract-versions.mjs [--base REF]");
}
const base = args[1] ?? "HEAD";
const changed = new Set([
  ...git("diff", "--name-only", "--diff-filter=ACMRT", base, "--", "contracts").trim().split("\n"),
  ...git("ls-files", "--others", "--exclude-standard", "--", "contracts").trim().split("\n"),
]);
const restoreCache = new Map();
// Whether `name`'s exact current bytes already exist in the path's own
// commit history: a content-addressed archival restore never drifts a
// version, because the bytes and the version in the name travel together.
function isFrozenRestore(name) {
  if (restoreCache.has(name)) return restoreCache.get(name);
  let result = false;
  let current;
  try {
    current = readFileSync(resolve(root, name));
  } catch {
    restoreCache.set(name, result);
    return result;
  }
  let commits = [];
  try {
    commits = git("log", "--format=%H", "--", name).trim().split("\n").filter(Boolean);
  } catch {
    restoreCache.set(name, result);
    return result;
  }
  for (const commit of commits) {
    try {
      const historical = git("show", `${commit}:${name}`, { stdio: ["ignore", "pipe", "ignore"] });
      if (Buffer.compare(Buffer.from(historical, "utf8"), current) === 0) {
        result = true;
        break;
      }
    } catch {
      // The path does not exist at this commit; keep walking.
    }
  }
  restoreCache.set(name, result);
  return result;
}
const problems = [];
const families = new Map();
for (const name of readdirSync(resolve(root, "contracts"))) {
  const match = name.match(/^(.*)\.v(\d+\.\d+\.\d+)(\..*)$/);
  if (!match) continue;
  const [, family, version, suffix] = match;
  const key = family + suffix;
  families.set(key, version);
  if (changed.has(`contracts/${name}`) && version !== product && !isFrozenRestore(`contracts/${name}`)) {
    problems.push(`${name}: changed contract must use product ${product}`);
  }
}
if (problems.length) throw new Error(problems.join("\n"));
console.log(JSON.stringify({ ok: true, product, contractArtifacts: families.size, base }));

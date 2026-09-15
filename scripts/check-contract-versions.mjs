#!/usr/bin/env node
// Changed contracts take the product version of the commit changing them.
import { execFileSync } from "node:child_process";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" });
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
const problems = [];
const families = new Map();
for (const name of readdirSync(resolve(root, "contracts"))) {
  const match = name.match(/^(.*)\.v(\d+\.\d+\.\d+)(\..*)$/);
  if (!match) continue;
  const [, family, version, suffix] = match;
  const key = family + suffix;
  families.set(key, version);
  if (changed.has(`contracts/${name}`) && version !== product) {
    problems.push(`${name}: changed contract must use product ${product}`);
  }
}
if (problems.length) throw new Error(problems.join("\n"));
console.log(JSON.stringify({ ok: true, product, contractArtifacts: families.size, base }));

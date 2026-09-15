#!/usr/bin/env node
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const root = mkdtempSync(join(tmpdir(), "lekalo-contract-version-"));
try {
  mkdirSync(join(root, "scripts"));
  mkdirSync(join(root, "contracts"));
  cpSync(new URL("./check-contract-versions.mjs", import.meta.url), join(root, "scripts/check-contract-versions.mjs"));
  const product = version => writeFileSync(join(root, "Cargo.toml"), `[workspace.package]\nversion = "${version}"\n`);
  const contract = "contracts/example.schema.v0.2.16.json";
  product("0.2.16");
  writeFileSync(join(root, contract), '{}\n');
  const git = (...args) => execFileSync("git", args, { cwd: root, stdio: "pipe", windowsHide: true });
  git("init", "--quiet");
  git("add", "Cargo.toml", "contracts", "scripts");
  git("-c", "user.name=Contract test", "-c", "user.email=contract-test@example.invalid", "commit", "--quiet", "-m", "baseline");
  const check = () => spawnSync(process.execPath, ["scripts/check-contract-versions.mjs"], { cwd: root, encoding: "utf8", windowsHide: true });
  assert.equal(check().status, 0);
  product("0.2.17");
  assert.equal(check().status, 0, "unchanged contract keeps its version");
  writeFileSync(join(root, contract), '{"changed":true}\n');
  assert.notEqual(check().status, 0, "changed contract requires the product version");
  renameSync(join(root, contract), join(root, "contracts/example.schema.v0.2.17.json"));
  assert.equal(check().status, 0, "renamed current contract passes");
  console.log(JSON.stringify({ ok: true, cases: 4 }));
} finally {
  rmSync(root, { recursive: true, force: true });
}

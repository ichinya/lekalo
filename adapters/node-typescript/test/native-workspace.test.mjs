/**
 * Issue #48 workspace inventory tests: bounded pnpm membership parsing,
 * package graph construction, ambiguity refusal, and the standalone
 * fallback — all read-only, in-process, over the committed fixtures.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

import {
  classifyWorkspacePattern,
  parseWorkspaceYaml,
  patternMatchesDirectory,
  WorkspaceRefusal,
} from "../src/workspace.mjs";
import { join } from "node:path";

import { fileURLToPath } from "node:url";
const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const fixtureRoot = join(repoRoot, "tests/fixtures/node-native-gates");

test("workspace YAML subset: packages list parses; tags/anchors/duplicates refuse", () => {
  const parsed = parseWorkspaceYaml('packages:\n  - "packages/*"\n');
  assert.deepEqual(parsed.packages, ["packages/*"]);
  assert.throws(() => parseWorkspaceYaml("packages:\n  - *alias\n"), WorkspaceRefusal);
  assert.throws(() => parseWorkspaceYaml("packages: [a, b]\n"), WorkspaceRefusal);
  assert.throws(() => parseWorkspaceYaml("packages:\n  - a\npackages:\n  - b\n"), WorkspaceRefusal);
  assert.throws(() => parseWorkspaceYaml("packages:\n\t- a\n"), WorkspaceRefusal);
});

test("workspace pattern classification accepts the closed subset only", () => {
  assert.equal(classifyWorkspacePattern("packages/*").supported, true);
  assert.equal(classifyWorkspacePattern("packages/**").supported, true);
  assert.equal(classifyWorkspacePattern("apps/?pp").supported, true);
  assert.equal(classifyWorkspacePattern("packages/extras").supported, true);
  assert.equal(classifyWorkspacePattern("PKG/*").supported, false, "uppercase refused");
  assert.equal(classifyWorkspacePattern("../outside").supported, false, "traversal refused");
  assert.equal(classifyWorkspacePattern(".hidden/*").supported, false, "dot dirs refused");
  assert.equal(classifyWorkspacePattern("a\\b").supported, false, "backslash refused");
  assert.equal(classifyWorkspacePattern("a{b}").supported, false, "brace expansion refused");
});

test("pattern matching: one-level `*`, recursive `**`, no dot traversal", () => {
  assert.equal(patternMatchesDirectory("packages/*", "packages/a"), true);
  assert.equal(patternMatchesDirectory("packages/*", "packages/a/b"), false);
  assert.equal(patternMatchesDirectory("packages/**", "packages/a/b"), true);
  assert.equal(patternMatchesDirectory("packages/**", "packages"), false, "never the head itself");
  assert.equal(patternMatchesDirectory("packages/*", "packages/.hidden"), false);
  assert.equal(patternMatchesDirectory("other/*", "packages/a"), false);
});

test("the committed monorepo fixture carries the expected membership", () => {
  const text = readFileSync(join(fixtureRoot, "pnpm-monorepo/pnpm-workspace.yaml"), "utf8");
  const parsed = parseWorkspaceYaml(text);
  assert.deepEqual(parsed.packages, ["packages/*"]);
  assert.equal(classifyWorkspacePattern(parsed.packages[0]).supported, true);
});

test("the standalone fixture declares no workspace manifest by design", () => {
  // npm-standalone layout: the inventory layer falls back to the
  // standalone capability path when no in-scope workspace doc exists.
  assert.throws(
    () => readFileSync(join(fixtureRoot, "npm-standalone/pnpm-workspace.yaml")),
    /ENOENT/,
    "no workspace manifest exists",
  );
});

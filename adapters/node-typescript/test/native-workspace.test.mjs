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
  buildWorkspaceInventory,
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

test("workspace YAML: quoted `!` values are patterns; bare tags still refuse", () => {
  const parsed = parseWorkspaceYaml(
    'packages:\n  - "packages/*"\n  - "!packages/web"\n',
  );
  assert.deepEqual(parsed.packages, ["packages/*", "!packages/web"]);
  assert.throws(
    () => parseWorkspaceYaml("packages:\n  - !packages/web\n"),
    WorkspaceRefusal,
    "unquoted `!` is a YAML tag, not a pattern",
  );
  assert.throws(
    () => parseWorkspaceYaml('packages:\n  - "a"\nother: !tag\n'),
    WorkspaceRefusal,
    "key-position tag refuses",
  );
});

test("negated workspace patterns exclude members and record uncertainty", () => {
  const files = {
    "package.json": JSON.stringify({ name: "monorepo", private: true }),
    "pnpm-workspace.yaml":
      'packages:\n  - "packages/*"\n  - "!packages/web"\n',
    "packages/api/package.json": JSON.stringify({ name: "api" }),
  };
  const readView = {
    canRead: (path) => Object.prototype.hasOwnProperty.call(files, path),
    readFile: (path) => Buffer.from(files[path], "utf8"),
  };
  const inventory = buildWorkspaceInventory({
    readView,
    directories: ["packages/api", "packages/web"],
  });
  assert.deepEqual(
    inventory.packages.map((record) => record.root).sort(),
    [".", "packages/api"],
    "packages/web is excluded by the negated pattern",
  );
  assert.equal(inventory.completeness, "incomplete");
  assert.ok(
    inventory.uncertainties.some(
      (entry) => entry.kind === "pattern-partial"
        && entry.detail.includes("!packages/web"),
    ),
    "the exclusion is recorded as a pattern-partial uncertainty",
  );
});

test("unsupported `!` bodies refuse through classification, never match", () => {
  const negated = classifyWorkspacePattern("!packages/web");
  assert.equal(negated.supported, true);
  assert.equal(negated.negated, true);
  assert.equal(negated.body, "packages/web");
  assert.equal(classifyWorkspacePattern("!../escape").supported, false);
  assert.equal(classifyWorkspacePattern("!").supported, false);
});

test("workspace YAML: quoted `&`/`*` are pattern values; bare anchors/aliases refuse", () => {
  const parsed = parseWorkspaceYaml('packages:\n  - "&x"\n  - "*x"\n');
  assert.deepEqual(parsed.packages, ["&x", "*x"]);
  assert.throws(
    () => parseWorkspaceYaml("packages:\n  - &x\n"),
    WorkspaceRefusal,
    "unquoted `&` is a YAML anchor, not a pattern",
  );
  assert.throws(
    () => parseWorkspaceYaml("packages:\n  - *x\n"),
    WorkspaceRefusal,
    "unquoted `*` is a YAML alias, not a pattern",
  );
});

test("a quoted `&x` pattern classifies unsupported and degrades the inventory", () => {
  const files = {
    "package.json": JSON.stringify({ name: "monorepo", private: true }),
    "pnpm-workspace.yaml": 'packages:\n  - "&x"\n  - "packages/*"\n',
    "packages/api/package.json": JSON.stringify({ name: "api" }),
  };
  const readView = {
    canRead: (path) => Object.prototype.hasOwnProperty.call(files, path),
    readFile: (path) => Buffer.from(files[path], "utf8"),
  };
  const inventory = buildWorkspaceInventory({
    readView,
    directories: ["packages/api"],
  });
  assert.deepEqual(
    inventory.packages.map((record) => record.root).sort(),
    [".", "packages/api"],
  );
  assert.equal(inventory.completeness, "incomplete");
  assert.ok(
    inventory.uncertainties.some(
      (entry) => entry.kind === "pattern-partial"
        && entry.detail.includes("&x"),
    ),
    "the quoted anchor-shaped value is an unsupported pattern, not YAML",
  );
});

test("a quoted `*x` value is a legal one-level glob", () => {
  const classification = classifyWorkspacePattern("*x");
  assert.equal(classification.supported, true);
  assert.equal(patternMatchesDirectory("*x", "ax"), true);
  assert.equal(patternMatchesDirectory("*x", "bx"), true);
  assert.equal(patternMatchesDirectory("*x", "ab"), false);
  assert.equal(patternMatchesDirectory("*x", "a/bx"), false, "one level only");
});

test("truncated bounded discovery degrades completeness", () => {
  const files = {
    "package.json": JSON.stringify({ name: "monorepo", private: true }),
    "pnpm-workspace.yaml": 'packages:\n  - "packages/*"\n',
    "packages/api/package.json": JSON.stringify({ name: "api" }),
  };
  const readView = {
    canRead: (path) => Object.prototype.hasOwnProperty.call(files, path),
    readFile: (path) => Buffer.from(files[path], "utf8"),
  };
  const inventory = buildWorkspaceInventory({
    readView,
    directories: ["packages/api"],
    discoveryTruncated: true,
  });
  assert.equal(inventory.completeness, "incomplete");
  assert.ok(
    inventory.uncertainties.some(
      (entry) => entry.kind === "pattern-partial"
        && entry.detail.includes("truncated"),
    ),
    "a cut-off candidate expansion records pattern-partial",
  );
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

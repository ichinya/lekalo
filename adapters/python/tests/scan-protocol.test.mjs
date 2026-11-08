import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";

const executable = process.env.LEKALO_PYTHON_ADAPTER_BIN
  ?? fileURLToPath(new URL(`../target/debug/lekalo-target-python${process.platform === "win32" ? ".exe" : ""}`, import.meta.url));
const temporaryParent = realpathSync(tmpdir());
const scratch = mkdtempSync(join(temporaryParent, "lekalo-python-regressions-"));
after(() => {
  const checked = realpathSync(scratch);
  assert.equal(dirname(checked), temporaryParent);
  assert(basename(checked).startsWith("lekalo-python-regressions-"));
  rmSync(checked, { recursive: true, force: true, maxRetries: 3 });
});

const canonical = (value) => value && typeof value === "object"
  ? Array.isArray(value) ? `[${value.map(canonical).join(",")}]`
    : `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(",")}}`
  : JSON.stringify(value);
const digest = (value) => `sha256:${createHash("sha256").update(canonical(value)).digest("hex")}`;
const profile = (path = "src") => ({
  id: "review", mode: "observed", target: "python",
  readRoots: [{ kind: "tree", path }], exclusions: [],
  provenance: { origin: "declared", revision: "review-regressions", disposition: "public-fixture" },
});
const resolution = {
  digest: digest("trusted target-profile resolution"),
  capabilities: [{ id: "scan.schema", support: "partial" }, { id: "scan.symbols", support: "full" }],
};
const fixture = (files) => {
  const cwd = mkdtempSync(join(scratch, "case-"));
  for (const [path, source] of Object.entries(files)) {
    const full = join(cwd, path);
    mkdirSync(dirname(full), { recursive: true });
    writeFileSync(full, source);
  }
  return cwd;
};
const run = (cwd, launch, extra = {}) => {
  const request = JSON.parse(JSON.stringify({
    protocol: "lekalo.target/v1", protocol_version: "1.1.0", operation: "scan",
    project_root: ".", target: "python", profile: launch.id, ...extra,
  }));
  request.request_id = digest(request).replace("sha256:", "req-");
  const result = spawnSync(executable, ["--lekalo-project-profile-json", JSON.stringify(launch)], {
    cwd, input: JSON.stringify(request), encoding: "utf8", timeout: 20_000,
    maxBuffer: 16 * 1024 * 1024,
  });
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
};
const symbols = (result) => result.result.entries.flatMap((entry) => entry.evidence.native_symbols ?? []);
const names = (result) => symbols(result).map((symbol) => symbol.qualified_name).sort();
const references = (result) => result.result.entries.flatMap((entry) => entry.evidence.references ?? []).map((row) => row.target);
const assertForests = (result) => {
  for (const entry of result.result.entries) {
    const forest = entry.evidence.native_symbols;
    const ids = new Set(forest.map((symbol) => symbol.native));
    assert.equal(forest.filter((symbol) => symbol.member_of === undefined).length, 1);
    for (const symbol of forest) {
      assert([...symbol.qualified_name].length <= 256);
      if (symbol.member_of !== undefined) assert(ids.has(symbol.member_of));
    }
  }
};

test("resolved requests bind the target snapshot and preserve launch-profile custody", () => {
  const cwd = fixture({ "src/sample.py": "def short():\n    pass\n" });
  const launch = { ...profile(), targetResolution: resolution };
  const result = run(cwd, launch, {
    profile_digest: resolution.digest,
    // JSON object member order is not part of a capability claim.
    profile_capabilities: resolution.capabilities.map(({ id, support }) => ({ support, id })),
  });
  assert.equal(result.status, "ok");
  const normalized = { ...launch, readRoots: launch.readRoots.map((root) => ({ ...root, scope: `${root.path}/**` })) };
  assert.equal(result.evidence.scan_profile_digest, digest(normalized));
  assert.notEqual(result.evidence.scan_profile_digest, resolution.digest);
});

test("missing, altered, reordered and extra resolution claims are refused", () => {
  const cwd = fixture({ "src/sample.py": "def short():\n    pass\n" });
  const launch = { ...profile(), targetResolution: resolution };
  const bound = { profile_digest: resolution.digest, profile_capabilities: resolution.capabilities };
  for (const changes of [
    { profile_digest: undefined }, { profile_capabilities: undefined },
    { profile_digest: digest("different resolution") },
    { profile_capabilities: [] },
    { profile_capabilities: resolution.capabilities.slice(1) },
    { profile_capabilities: [...resolution.capabilities].reverse() },
    { profile_capabilities: resolution.capabilities.map((row) => ({ ...row, support: "unsupported" })) },
    { profile_capabilities: [...resolution.capabilities, { id: "verify.schema", support: "full" }] },
  ]) {
    assert.equal(run(cwd, launch, { ...bound, ...changes }).status, "error");
  }
  assert.equal(run(cwd, launch).error.code, "profile-resolution-required");
  assert.equal(run(cwd, profile(), bound).error.code, "profile-resolution-missing");
});

test("malformed injected resolution snapshots are refused", () => {
  const cwd = fixture({ "src/sample.py": "pass\n" });
  for (const targetResolution of [
    "not-an-object", { ...resolution, extra: true }, { ...resolution, digest: "invalid" },
    { ...resolution, capabilities: [] },
    { ...resolution, capabilities: [...resolution.capabilities].reverse() },
    { ...resolution, capabilities: [resolution.capabilities[0], resolution.capabilities[0]] },
    { ...resolution, capabilities: [{ id: "bad capability", support: "full" }] },
    { ...resolution, capabilities: [{ id: "scan.symbols", support: "invented" }] },
    { ...resolution, capabilities: [{ id: "scan.symbols", support: "full", extra: true }] },
    { ...resolution, capabilities: Array.from({ length: 65 }, (_, i) => ({ id: `scan.c${String(i).padStart(2, "0")}`, support: "full" })) },
  ]) {
    const result = run(cwd, { ...profile(), targetResolution });
    assert.equal(result.status, "error");
    assert.equal(result.error.message, "profile-invalid");
  }
});

test("unresolved profiles retain predecessor and custody behavior", () => {
  const cwd = fixture({ "src/sample.py": "def short():\n    pass\n" });
  for (const version of ["0.3.2", "0.6.4", "0.9.0", "0.9.1", "1.1.0"]) {
    const result = run(cwd, profile(), { protocol_version: version });
    assert.equal(result.status, "ok");
    assert.equal(result.result.truncated, false);
    if (["0.3.2", "0.6.4"].includes(version)) assert.equal(result.evidence.scan_profile_digest, undefined);
    else assert.equal(result.evidence.scan_profile_digest, digest({ ...profile(), readRoots: [{ kind: "tree", path: "src", scope: "src/**" }] }));
  }
});

for (const extension of ["py", "pyi"]) {
  test(`package read roots agree with their children and relative imports (${extension})`, () => {
    const cwd = fixture({
      [`src/shop/__init__.${extension}`]: "from .orders import Order\n",
      "src/shop/orders.py": "from .models import Model\nclass Order:\n    pass\n",
      "src/shop/models.py": "class Model:\n    pass\n",
    });
    const packageResult = run(cwd, profile("src/shop"));
    const parentResult = run(cwd, profile("src"));
    assert.equal(packageResult.status, "ok");
    assert.deepEqual(names(packageResult), names(parentResult));
    assert(names(packageResult).includes("shop.orders.Order"));
    assert(references(packageResult).includes("shop.orders.Order"));
    assert(references(packageResult).includes("shop.models.Model"));
    assertForests(packageResult);
  });
}

test("excluded or ungranted package markers cannot change module names", () => {
  const cwd = fixture({
    "src/shop/__init__.py": "from .orders import Order\n",
    "src/shop/orders.py": "class Order:\n    pass\n",
  });
  const excluded = { ...profile("src/shop"), exclusions: ["src/shop/__init__.py"] };
  const fileScope = { ...profile(), readRoots: [{ kind: "file", path: "src/shop/orders.py" }] };
  for (const launch of [excluded, fileScope]) {
    const result = run(cwd, launch);
    assert.equal(result.status, "ok");
    assert.deepEqual(names(result), ["orders.Order"]);
    assert(result.result.entries.every((entry) => entry.path === "src/shop/orders.py"));
  }
});

test("oversized qualified names preserve healthy inventory and report uncertainty", () => {
  const cwd = fixture({ "src/sample.py": `def ${"a".repeat(257)}():\n    pass\ndef short():\n    pass\n` });
  const result = run(cwd, profile());
  assert.equal(result.status, "ok");
  assert.deepEqual(names(result), ["sample.short"]);
  assert.equal(result.result.truncated, true);
  assert(result.evidence.native_uncertainty.some((row) => row.detail === "native-inventory-bound"));
  assertForests(result);
});

#!/usr/bin/env node
// Issue #96 contract gate: the versioned explanation catalog, the closed
// explain-result wire, the additive explain.* registry family, live CLI
// behavior, and the coverage rule that every registered diagnostic code
// has a stable explanation. A new stable diagnostic without catalog
// documentation fails here (CI-fails-when-missing), and the golden
// explain documents must match the live binary byte-for-byte.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { registryUnion, assertRegistryUnion, ACCEPTED_M8_REGISTRY_FAMILIES } from "./lib/registry-union.mjs";

const require = createRequire(import.meta.url);
const Ajv2020 = require("ajv/dist/2020").default;
assert.equal(require("ajv/package.json").version, "8.20.0", "exact gate dependency required");
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => JSON.parse(readFileSync(resolve(root, path), "utf8"));
const ajv = new Ajv2020({ strict: true, allErrors: true });
ajv.addSchema(read("contracts/diagnostic.schema.v0.2.16.json"), "https://dev.lekalo/diagnostic.schema.v0.2.16.json");

for (const family of ["explain-catalog", "explain-result"]) {
  const schema = read(`contracts/${family}.schema.v0.8.0.json`);
  assert.equal(schema.title, `lekalo/${family}/v0.8.0`, `${family} title`);
  assert.equal(schema.$id, `https://dev.lekalo/${family}.schema.v0.8.0.json`, `${family} $id`);
  assert.equal(schema.properties.schemaVersion.const, `lekalo/${family}/v0.8.0`);
  assert.equal(schema.properties.identity.const, `dev.lekalo.${family}@0.8.0`);
  ajv.addSchema(schema);
}
const catalogSchema = ajv.getSchema("https://dev.lekalo/explain-catalog.schema.v0.8.0.json");
const resultSchema = ajv.getSchema("https://dev.lekalo/explain-result.schema.v0.8.0.json");
const valid = (validator, value, name) => assert(validator(value), `${name}: ${JSON.stringify(validator.errors)}`);

// ---------------------------------------------------------------------------
// Gate mode: default runs everything; --schema-only skips the live-binary
// and golden replay sections (the contracts CI job builds no binary).
// ---------------------------------------------------------------------------
const schemaOnly = process.argv.includes("--schema-only");
const providerCatalogSchema = ajv.compile(read("contracts/explain-provider-catalog.schema.v0.8.0.json"));
const providerCatalog = {
  schemaVersion: "lekalo/explain-provider-catalog/v0.8.0",
  identity: "dev.lekalo.explain-provider-catalog@0.8.0",
  entries: [
    { namespace: "com.example.alpha:1.0.0", originalCode: "A009", explanationRef: "urn:alpha:explain:1.0.0:A009" },
    { namespace: "com.example.beta:2.0.0", originalCode: "B009", explanationRef: "urn:beta:explain:2.0.0:B009" },
  ],
};
valid(providerCatalogSchema, providerCatalog, "provider catalog");
for (const patch of [{ explanationRef: null }, { namespace: "core" }, { originalCode: "unsafe\ncode" }]) {
  const bad = structuredClone(providerCatalog);
  Object.assign(bad.entries[0], patch);
  assert.equal(providerCatalogSchema(bad), false, "provider reference grammar stays closed");
}

// ---------------------------------------------------------------------------
// 1. The embedded catalog: schema-valid, closed, registry-pinned, complete.
// ---------------------------------------------------------------------------
const registry = read("contracts/diagnostic-registry.v0.8.0.json");
const catalog = read("contracts/explain-catalog.v0.8.0.json");
valid(catalogSchema, catalog, "explain-catalog");
assert.equal(catalog.closed, true, "catalog is closed");
assert.equal(catalog.registryIdentity, "dev.lekalo.diagnostic-registry@0.8.0", "catalog pins the registry union");
const { createHash } = await import("node:crypto");
assert.equal(
  catalog.registryDigest,
  `sha256:${createHash("sha256").update(readFileSync(resolve(root, "contracts/diagnostic-registry.v0.8.0.json"))).digest("hex")}`,
  "catalog registry digest pin",
);

const registryByCode = new Map(registry.entries.map((entry) => [entry.code, entry]));
const catalogCodes = new Set(catalog.entries.map((entry) => entry.code));
assert.equal(catalog.entries.length, registry.entries.length, "one explanation per registered code");
for (const entry of catalog.entries) {
  const registered = registryByCode.get(entry.code);
  assert.ok(registered, `catalog code ${entry.code} is not registered`);
  assert.equal(entry.ruleId, registered.id, `rule-id drift for ${entry.code}`);
  assert.equal(entry.category, registered.category, `category drift for ${entry.code}`);
  assert.equal(entry.severityProfile.defaultSeverity, registered.default_severity, `severity drift for ${entry.code}`);
  assert.deepEqual(
    [...entry.severityProfile.allowedStatuses].sort(),
    [...registered.allowed_statuses].sort(),
    `allowed-status drift for ${entry.code}`,
  );
  assert.equal(entry.severityProfile.blocking, registered.allowed_statuses.includes("invalid"), `blocking drift for ${entry.code}`);
  assert.ok(entry.title.length > 0 && entry.checks.length > 0 && entry.rationale.length > 0, `prose completeness for ${entry.code}`);
  assert.ok(entry.rootCauses.length > 0 && entry.fixOptions.length > 0 && entry.examples.length > 0, `content completeness for ${entry.code}`);
  assert.ok(entry.relatedCodes.every((code) => catalogCodes.has(code)), `related codes resolve for ${entry.code}`);
  assert.ok(entry.docsLinks.length > 0 && entry.docsLinks.every((link) => link.version === "0.8.0"), `versioned docs links for ${entry.code}`);
  for (const fix of entry.fixOptions) {
    assert.ok(["safe", "unsafe", "breaking", "manual-only"].includes(fix.class), `closed fix class for ${entry.code}`);
  }
}

// Coverage rules: every blocking diagnostic explained with an example;
// safe-fix recipe IDs only ever link the admitted executable recipe.
for (const entry of registry.entries.filter((e) => e.allowed_statuses.includes("invalid"))) {
  const explanation = catalog.entries.find((candidate) => candidate.code === entry.code);
  assert.ok(explanation, `blocking diagnostic ${entry.code} (${entry.id}) has no explanation`);
  assert.ok(explanation.examples.length > 0, `blocking diagnostic ${entry.code} has no example`);
}
for (const entry of catalog.entries) {
  if (entry.executableRecipeId !== null) {
    assert.equal(entry.executableRecipeId, "core.canonical-format", "only the admitted executable recipe is linked");
    assert.ok(entry.code.startsWith("LEK-FORMAT-"), "the executable recipe link belongs to the formatting family");
  }
}

// ---------------------------------------------------------------------------
// 2. The explain.* registry family: additive, closed, sorted, admitted.
// ---------------------------------------------------------------------------
const predecessor = read("contracts/diagnostic-registry.v0.6.5.json");
registryUnion(predecessor, registry);
assertRegistryUnion(predecessor, registry, { families: ACCEPTED_M8_REGISTRY_FAMILIES, ownPrefix: "explain.", ownCount: 3 });
valid(ajv.compile(read("contracts/diagnostic-registry.schema.v0.8.0.json")), registry, "registry");
const own = registry.entries.filter((entry) => entry.id.startsWith("explain."));
assert.deepEqual(own.map((entry) => [entry.id, entry.code]), [
  ["explain.catalog-invalid", "LEK-EXPLAIN-002"],
  ["explain.code-unexplained", "LEK-EXPLAIN-001"],
  ["explain.report-invalid", "LEK-EXPLAIN-003"],
]);
// The unknown-code outcome may only surface under `unsupported`; report
// refusals are invalid; catalog-integrity failures are unavailable.
assert.deepEqual(own.find((entry) => entry.id === "explain.code-unexplained").allowed_statuses, ["unsupported"]);
assert.deepEqual(own.find((entry) => entry.id === "explain.report-invalid").allowed_statuses, ["invalid"]);
assert.deepEqual(own.find((entry) => entry.id === "explain.catalog-invalid").allowed_statuses, ["unavailable"]);
// Every explain.* diagnostic is itself explained (the catalog covers its own family).
for (const entry of own) assert.ok(catalogCodes.has(entry.code), `own family code ${entry.code} is explained`);

// ---------------------------------------------------------------------------
// 3. The generator is authoritative: committed bytes match regeneration.
// ---------------------------------------------------------------------------
const { execFileSync } = await import("node:child_process");
const before = readFileSync(resolve(root, "contracts/explain-catalog.v0.8.0.json"));
execFileSync(process.execPath, [resolve(root, "scripts/gen-explain-catalog.mjs"), "--check"], { cwd: root, stdio: "pipe" });
assert.equal(readFileSync(resolve(root, "contracts/explain-catalog.v0.8.0.json")).equals(before), true, "generator --check mutated nothing");
const drifted = structuredClone(catalog);
drifted.entries[0].title = "drifted";
const driftedBytes = Buffer.from(JSON.stringify(drifted, null, 2) + "\n");
const scratch = resolve(root, "target/explain-catalog-drift-probe.json");
const { writeFileSync, rmSync } = await import("node:fs");
writeFileSync(resolve(root, "contracts/explain-catalog.v0.8.0.json"), driftedBytes);
let checkFailed = false;
try {
  execFileSync(process.execPath, [resolve(root, "scripts/gen-explain-catalog.mjs"), "--check"], { cwd: root, stdio: "pipe" });
} catch {
  checkFailed = true;
} finally {
  writeFileSync(resolve(root, "contracts/explain-catalog.v0.8.0.json"), before);
}
assert.equal(checkFailed, true, "generator --check must fail on catalog drift");
rmSync(scratch, { force: true });

// ---------------------------------------------------------------------------
// 4. Live CLI behavior against the real binary (when present).
// ---------------------------------------------------------------------------
const binary = process.env.LEKALO_EXPLAIN_BIN ?? resolve(root, "target/debug", process.platform === "win32" ? "lekalo.exe" : "lekalo");
if (!schemaOnly) assert.ok(existsSync(binary), "live CLI required; build the checkout first");
let live = false;
if (!schemaOnly && existsSync(binary)) {
  live = true;
  const run = (args) => {
    const child = spawnSync(binary, args, { cwd: root, encoding: "utf8", timeout: 30000, maxBuffer: 8 * 1024 * 1024 });
    return child;
  };
  // Known code: valid receipt on stdout, schema-valid document.
  const known = run(["explain", "LEK-HIDDEN-009", "--json"]);
  assert.equal(known.status, 0, `known code exit\n${known.stderr}`);
  assert.equal(known.stderr, "", "status-owned stream");
  const knownDoc = JSON.parse(known.stdout);
  valid(resultSchema, knownDoc, "known-code result");
  assert.equal(knownDoc.state, "explained");
  assert.equal(knownDoc.explanations.length, 1);
  assert.equal(knownDoc.explanations[0].code, "LEK-HIDDEN-009");
  assert.equal(knownDoc.explanations[0].source.provider, "core");
  assert.ok(!("reasonCodes" in knownDoc) || knownDoc.reasonCodes.length === 0, "a valid explain carries no refusal diagnostics");
  // Rule-id selector resolves to the same document.
  const byRule = run(["explain", "hidden.observer-write", "--json"]);
  assert.equal(byRule.status, 0);
  assert.equal(JSON.parse(byRule.stdout).explanations[0].code, "LEK-HIDDEN-009", "rule-id and code agree");
  // Safe-fix recipe link reaches the executable recipe id.
  const safeRecipe = JSON.parse(run(["explain", "LEK-FORMAT-001", "--json"]).stdout);
  assert.equal(safeRecipe.explanations[0].executableRecipeId, "core.canonical-format");
  const recipeOption = safeRecipe.explanations[0].fixOptions.find((fix) => fix.recipeId === "core.canonical-format");
  assert.ok(recipeOption, "recipe-linked fix option present");
  assert.equal(recipeOption.class, "safe");
  // Distinct unknown-code outcome: exit 4, unsupported, typed diagnostic.
  const unknown = run(["explain", "LEK-TYPE-014", "--json"]);
  assert.equal(unknown.status, 4, "unknown code is the distinct unsupported outcome");
  const unknownDoc = JSON.parse(unknown.stdout);
  assert.equal(unknownDoc.status, "unsupported");
  const unknownInner = unknownDoc.payload;
  assert.equal(unknownInner.state, "unexplained");
  assert.deepEqual(unknownDoc.reasonCodes, ["explain.code-unexplained"]);
  assert.equal(unknownInner.explanations.length, 0);
  // --diagnostic-file attaches structural report context, resolved and valid.
  const reportPath = resolve(root, "tests/fixtures/diagnostics/usage-envelope.json");
  const fromReport = run(["explain", "--diagnostic-file", reportPath, "--json"]);
  assert.equal(fromReport.status, 0, `diagnostic-file exit\n${fromReport.stderr}`);
  const reportDoc = JSON.parse(fromReport.stdout);
  valid(resultSchema, reportDoc, "diagnostic-file result");
  assert.equal(reportDoc.state, "explained");
  for (const explanation of reportDoc.explanations) {
    assert.ok(explanation.source.report, "report context attached");
    assert.ok(explanation.source.report.message.length > 0);
  }
  // Malformed report refuses invalid with the typed diagnostic.
  const malformed = run(["explain", "--diagnostic-file", resolve(root, "Cargo.toml"), "--json"]);
  assert.equal(malformed.status, 1, "malformed report is invalid");
  assert.deepEqual(JSON.parse(malformed.stderr).reasonCodes, ["explain.report-invalid"]);
  // Human output stays a projection of the same object (stable codes).
  const human = run(["explain", "LEK-HIDDEN-009"]);
  assert.equal(human.status, 0);
  assert.match(human.stdout, /LEK-HIDDEN-009/);
  assert.match(human.stdout, /Checks:/);
  const english = run(["explain", "LEK-HIDDEN-009", "--locale", "en", "--json"]);
  const russian = run(["explain", "LEK-HIDDEN-009", "--locale", "ru", "--json"]);
  assert.equal(english.status, 0);
  assert.equal(russian.status, 0);
  assert.equal(english.stdout, russian.stdout, "locale cannot change machine bytes");
  assert.match(run(["explain", "LEK-HIDDEN-009", "--locale", "ru"]).stdout, /Проверка:/);
  const { mkdtempSync } = await import("node:fs");
  const stage = mkdtempSync(resolve(root, "target/explain-providers-"));
  try {
    const report = resolve(stage, "report.json");
    const providerPath = resolve(stage, "providers.json");
    writeFileSync(providerPath, JSON.stringify(providerCatalog));
    writeFileSync(report, JSON.stringify({ diagnostics: providerCatalog.entries.map((entry) => ({
      code: "LEK-HIDDEN-009", message: "synthetic provider finding",
      metadata: { [entry.namespace]: { original_code: entry.originalCode } },
    })) }));
    const resolved = run(["explain", "--diagnostic-file", report, "--provider-catalog", providerPath, "--json"]);
    assert.equal(resolved.status, 0, resolved.stderr);
    const document = JSON.parse(resolved.stdout);
    valid(resultSchema, document, "provider references result");
    assert.deepEqual(document.explanations[0].source.providerExplanations, providerCatalog.entries);
    const missing = run(["explain", "--diagnostic-file", report, "--json"]);
    assert.equal(missing.status, 0);
    assert.equal(JSON.parse(missing.stdout).explanations[0].source.providerExplanations[0].explanationRef, null);
    const bad = structuredClone(providerCatalog);
    bad.entries.push(bad.entries[0]);
    writeFileSync(providerPath, JSON.stringify(bad));
    assert.equal(run(["explain", "--diagnostic-file", report, "--provider-catalog", providerPath, "--json"]).status, 1);
    assert.equal(run(["explain", ...Array(65).fill("LEK-HIDDEN-009"), "--json"]).status, 1, "selector limit is a typed refusal");
    writeFileSync(report, '{"diagnostics":[],"diagnostics":[]}');
    assert.equal(run(["explain", "--diagnostic-file", report, "--json"]).status, 1, "duplicate report keys refuse");
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
  // Catalog-integrity refusals are typed; no silent fallback exists.
  assert.ok(!knownDoc.explanations[0].checks.includes("undefined"), "no placeholder prose leaks");
}

// ---------------------------------------------------------------------------
// 5. Golden explain documents: byte-identical to the live binary.
// ---------------------------------------------------------------------------
for (const [name, args] of [
  ["known-code", ["explain", "LEK-HIDDEN-009", "--json"]],
  ["safe-recipe", ["explain", "LEK-FORMAT-001", "--json"]],
  ["unknown-code", ["explain", "LEK-TYPE-014", "--json"]],
  ["diagnostic-file", ["explain", "--diagnostic-file", resolve(root, "tests/fixtures/explain/report.json"), "--json"]],
]) {
  const goldenPath = resolve(root, "tests/fixtures/explain/golden", `${name}.json`);
  assert.ok(existsSync(goldenPath), `golden ${name} exists`);
  if (!live) continue;
  const child = spawnSync(binary, args, { cwd: root, encoding: "utf8", timeout: 30000, maxBuffer: 8 * 1024 * 1024 });
  const golden = readFileSync(goldenPath, "utf8").replaceAll("\r\n", "\n");
  assert.equal(child.stdout.replaceAll("\r\n", "\n"), golden, `golden ${name} matches the live binary`);
}

console.log(JSON.stringify({
  ok: true,
  gate: "test-explain-contracts",
  registryEntries: registry.entries.length,
  explainedCodes: catalog.entries.length,
  ownFamilyRules: own.length,
  golden: 4,
  live,
}));

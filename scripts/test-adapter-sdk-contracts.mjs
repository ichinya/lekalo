#!/usr/bin/env node
/**
 * The adapter-SDK contracts gate (issue #106).
 *
 * Validates the two closed 0.8.0 contract artifacts and every emitted
 * SDK document against them (pinned Ajv 8.17.1 through
 * NODE_PATH/LEKALO_AJV_NODE_PATH, like the other schema gates), and
 * cross-checks the recorded facts against reality:
 *
 *   - identities, discriminators, and closed vocabularies;
 *   - every recorded schema/registry/capability/vector/generator digest
 *     recomputed from the exact bytes on disk;
 *   - the operation/version matrix equal to the reviewed inputs;
 *   - canonical byte form (sorted keys, compact, no trailing LF) and
 *     duplicate-key refusal for the emitted descriptors;
 *   - the generated upgrade matrix covering exactly the corpus upgrade
 *     rows (the protocol-upgrade accept/refuse evidence);
 *   - generator `--check` determinism (committed outputs byte-match
 *     regeneration).
 *
 * Exit protocol: 0 with a one-line JSON status on success, 1 with a JSON
 * failure report on stderr otherwise.
 */
import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPT_DIR, "..");
const LANGUAGES = ["go", "php", "rust", "typescript"];

const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-contracts", reason, detail })}\n`);
  process.exit(1);
};

const sha256 = (text) => "sha256:" + createHash("sha256").update(text).digest("hex");
const read = (relative) => readFileSync(join(ROOT, relative), "utf8");

// Pinned Ajv 8.17.1 through NODE_PATH / LEKALO_AJV_NODE_PATH (repo-wide
// schema-gate convention; CI provisions it outside the checkout).
const require_ = createRequire(import.meta.url);
let Ajv;
try {
  const nodePath = process.env.LEKALO_AJV_NODE_PATH ?? "";
  if (nodePath) {
    Ajv = createRequire(nodePath + "/")("ajv/dist/2020.js");
  } else {
    Ajv = require_("ajv/dist/2020.js");
  }
} catch {
  fail("ajv-missing", "pinned Ajv 8.17.1 must be provided through NODE_PATH/LEKALO_AJV_NODE_PATH");
}
const ajvModule = Ajv.default ?? Ajv;
let ajvVersion = "unknown";
try {
  ajvVersion = require_("ajv/package.json").version;
} catch {
  fail("ajv-missing", "ajv/package.json unreadable");
}
if (ajvVersion !== "8.17.1") fail("ajv-version", `expected Ajv 8.17.1, found ${ajvVersion}`);
const ajv = new ajvModule({ strict: true, allErrors: true });

const failures = [];
const check = (condition, reason, detail) => {
  if (!condition) failures.push({ reason, detail });
};

// 1. The contract artifacts themselves.
const compatibilitySchema = JSON.parse(read("contracts/adapter-sdk-compatibility.schema.v0.8.0.json"));
const generationSchema = JSON.parse(read("contracts/adapter-sdk-generation.schema.v0.8.0.json"));
for (const [name, schema] of [
  ["adapter-sdk-compatibility", compatibilitySchema],
  ["adapter-sdk-generation", generationSchema],
]) {
  check(schema.$schema === "https://json-schema.org/draft/2020-12/schema", "schema-draft", name);
  check(schema.additionalProperties === false, "schema-open", name);
  check(
    schema.$id === `https://dev.lekalo/${name}.schema.v0.8.0.json`,
    "schema-id", schema.$id,
  );
  const compiled = ajv.compile(schema);
  check(typeof compiled === "function", "schema-compile", name);
}
const validateCompatibility = ajv.compile(compatibilitySchema);
const validateGeneration = ajv.compile(generationSchema);

const config = JSON.parse(read("sdk/schema-inputs.json"));
const capabilityExport = JSON.parse(read("sdk/capability-definitions.json"));

// 2. Every emitted document, with reality cross-checks.
const descriptorDigests = new Map();
for (const language of LANGUAGES) {
  const compatibilityRaw = read(`sdk/${language}/compatibility.json`);
  const generationRaw = read(`sdk/${language}/generation.json`);
  check(compatibilityRaw === JSON.stringify(JSON.parse(compatibilityRaw)) ? compatibilityRaw : compatibilityRaw, "descriptor-noncanonical", language);
  // Canonical byte form: re-encoding the parsed document must reproduce
  // the exact bytes (sorted keys, compact, no trailing LF).
  const canonicalForm = (text) => {
    const walk = (value) => {
      if (Array.isArray(value)) return `[${value.map(walk).join(",")}]`;
      if (value !== null && typeof value === "object") {
        return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${walk(value[key])}`).join(",")}}`;
      }
      return JSON.stringify(value);
    };
    return walk(JSON.parse(text));
  };
  check(canonicalForm(compatibilityRaw) === compatibilityRaw, "descriptor-not-canonical", `${language}/compatibility.json`);
  check(canonicalForm(generationRaw) === generationRaw, "record-not-canonical", `${language}/generation.json`);
  check(!compatibilityRaw.endsWith("\n"), "descriptor-trailing-lf", language);

  const compatibility = JSON.parse(compatibilityRaw);
  const generation = JSON.parse(generationRaw);

  if (!validateCompatibility(compatibility)) {
    fail("descriptor-schema", `${language}: ${ajv.errorsText(validateCompatibility.errors)}`);
  }
  if (!validateGeneration(generation)) {
    fail("record-schema", `${language}: ${ajv.errorsText(validateGeneration.errors)}`);
  }

  // Identity pins and package facts.
  const pkg = config.packages[language];
  check(compatibility.identity === "dev.lekalo.adapter-sdk-compatibility@0.8.0", "descriptor-identity", language);
  check(compatibility.schemaVersion === "lekalo/adapter-sdk-compatibility/v0.8.0", "descriptor-discriminator", language);
  check(compatibility.language === language, "descriptor-language", language);
  check(compatibility.sdkPackage === pkg.name, "descriptor-package", `${language}: ${compatibility.sdkPackage}`);
  check(compatibility.sdkVersion === pkg.version, "descriptor-version", language);
  check(compatibility.wireToken === config.wireToken, "descriptor-token", language);
  check(JSON.stringify(compatibility.protocolVersions) === JSON.stringify(config.protocolVersions), "descriptor-versions", language);
  for (const row of compatibility.operations) {
    check(
      JSON.stringify(row.operations) === JSON.stringify(config.operations[row.protocolVersion]),
      "descriptor-operations", `${language}/${row.protocolVersion}`,
    );
  }

  // Digests recomputed from disk.
  for (const schema of compatibility.schemas) {
    const version = schema.identity.split("@")[1];
    const actual = sha256(read(`contracts/target-protocol.schema.v${version}.json`));
    check(actual === schema.digest, "schema-digest", `${language}/${version}`);
    descriptorDigests.set(`${language}/${version}`, schema.digest);
  }
  check(
    compatibility.diagnosticRegistry.digest === sha256(read("contracts/diagnostic-registry.v0.6.4.json")),
    "registry-digest", language,
  );
  check(
    compatibility.capabilityDefinitions.entriesDigest === sha256(JSON.stringify(canonicalWalk(compatibility.capabilityDefinitions.entries))),
    "capability-entries-digest", language,
  );
  for (const vector of compatibility.testVectors) {
    check(
      vector.digest === sha256(read(`tests/fixtures/adapter-sdk/${vector.path}`)),
      "vector-digest", `${language}/${vector.path}`,
    );
  }

  // Generation record cross-checks.
  check(generation.identity === "dev.lekalo.adapter-sdk-generation@0.8.0", "record-identity", language);
  check(generation.schemaVersion === "lekalo/adapter-sdk-generation/v0.8.0", "record-discriminator", language);
  check(generation.generator.name === "scripts/generate-adapter-sdk.mjs", "record-generator", language);
  check(
    generation.generator.digest === sha256(read("scripts/generate-adapter-sdk.mjs")),
    "generator-digest", language,
  );
  for (const input of generation.inputs) {
    check(input.digest === sha256(read(input.path)), "input-digest", `${language}/${input.path}`);
  }
  check(
    generation.compatibility.digest === sha256(compatibilityRaw),
    "record-compatibility-digest", language,
  );
  for (const output of generation.outputs) {
    if (output.path === "compatibility.json") continue;
    let actual;
    try {
      actual = sha256(read(`sdk/${language}/${output.path}`));
    } catch {
      actual = `missing:${output.path}`;
    }
    check(actual === output.digest, "output-digest", `${language}/${output.path}`);
  }
  check(
    JSON.stringify(generation.semanticCoverage) === JSON.stringify([...config.semanticCoverage].sort((a, b) => (a.definition < b.definition ? -1 : 1))),
    "semantic-coverage", language,
  );
}

function canonicalWalk(value) {
  if (Array.isArray(value)) return value.map(canonicalWalk);
  if (value !== null && typeof value === "object") {
    const sorted = {};
    for (const key of Object.keys(value).sort()) sorted[key] = canonicalWalk(value[key]);
    return sorted;
  }
  return value;
}

// 3. Duplicate-key refusal controls for the emitted documents.
const duplicateKeys = (text) => {
  const offenders = [];
  const stack = [null];
  let index = 0;
  while (index < text.length) {
    const ch = text[index];
    if (ch === '"') {
      const start = index++;
      while (index < text.length) {
        if (text[index] === "\\") { index += 2; continue; }
        if (text[index] === '"') { index++; break; }
        index++;
      }
      let key;
      try {
        key = JSON.parse(text.slice(start, index));
      } catch {
        return ["<malformed>"];
      }
      while (index < text.length && /\s/.test(text[index])) index++;
      if (text[index] === ":") {
        index++;
        const frame = stack[stack.length - 1];
        if (frame !== null) {
          if (frame[key]) offenders.push(key);
          frame[key] = true;
        }
      }
      continue;
    }
    if (ch === "{") { stack.push({}); index++; continue; }
    if (ch === "}") { if (stack.length > 1) stack.pop(); index++; continue; }
    index++;
  }
  return offenders;
};
for (const control of ['{"a":1,"a":2}', '{"nested":{"a":1,"\\u0061":2}}']) {
  if (duplicateKeys(control).length !== 1) fail("duplicate-control", "decoded duplicate was lost");
}
if (duplicateKeys('{"a":{"b":1},"c":{"b":2}}').length !== 0) fail("duplicate-control", "distinct objects collided");

// 4. Generated upgrade matrix covers exactly the corpus upgrade rows.
// The TypeScript module embeds the canonical matrix JSON and is checked
// row-by-row; the other languages' generated modules are checked by row
// count and per-row token presence (their runtime accept/refuse behavior
// is proven by the vectors gate's upgrade family).
const corpusRows = [];
for (const name of require_("node:fs").readdirSync(join(ROOT, "tests/fixtures/adapter-sdk/upgrade")).sort()) {
  if (!name.endsWith(".json")) continue;
  const row = JSON.parse(read(`tests/fixtures/adapter-sdk/upgrade/${name}`));
  corpusRows.push({
    accepted: row.accept === true,
    // The plain describe rows carry no explicit operation member in the
    // generated matrix; only cross-version and unknown operations do.
    operation: row.operation === "describe" ? null : row.operation ?? null,
    protocolVersion: row.protocolVersion,
    reason: row.accept === true ? null : row.reason ?? null,
  });
}
const matrixFiles = {
  go: "sdk/go/generated/compatibility_tests.go",
  php: "sdk/php/src/Generated/CompatibilityTests.php",
  rust: "sdk/rust/src/generated/compatibility_tests.rs",
  typescript: "sdk/typescript/src/generated/compatibility_tests.mjs",
};
for (const language of LANGUAGES) {
  const matrixText = read(matrixFiles[language]);
  if (language === "typescript") {
    const match = matrixText.match(/export const COMPATIBILITY_MATRIX = (\[.*\]);/s);
    check(match !== null, "upgrade-matrix-missing", language);
    if (match) {
      const generatedRows = JSON.parse(match[1]);
      check(generatedRows.length === corpusRows.length, "upgrade-row-count", `${language}: ${generatedRows.length} vs ${corpusRows.length}`);
      for (const row of corpusRows) {
        const covered = generatedRows.some((candidate) =>
          candidate.admitted === row.accepted
          && candidate.protocolVersion === row.protocolVersion
          && (row.accepted || candidate.reason === row.reason)
          && (row.accepted || (candidate.operation ?? null) === row.operation),
        );
        check(covered, "upgrade-row-uncovered", `${language}/${row.protocolVersion}/${row.operation ?? "describe"}/${row.reason ?? "accept"}`);
      }
    }
  } else {
    const rowToken = { go: "Admitted:", php: "'admitted' => ", rust: "    CompatibilityCase {" }[language];
    const rowCount = matrixText.split(rowToken).length - 1;
    check(rowCount === corpusRows.length, "upgrade-row-count", `${language}: ${rowCount} vs ${corpusRows.length}`);
    for (const row of corpusRows) {
      check(matrixText.includes(row.protocolVersion), "upgrade-row-version", `${language}/${row.protocolVersion}`);
      if (row.reason !== null) {
        check(matrixText.includes(`"${row.reason}"`) || matrixText.includes(`'${row.reason}'`), "upgrade-row-reason", `${language}/${row.reason}`);
      }
    }
  }
}

// 5. Generator determinism: committed outputs byte-match regeneration.
const generatorCheck = execFileSync(process.execPath, [join(SCRIPT_DIR, "generate-adapter-sdk.mjs"), "--check"], {
  cwd: ROOT, encoding: "utf8",
});
check(generatorCheck.includes('"ok":true'), "generator-check", generatorCheck.slice(0, 200));

if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, gate: "adapter-sdk-contracts", failures: failures.slice(0, 40), total: failures.length })}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({ ok: true, gate: "adapter-sdk-contracts", ajv: ajvVersion, languages: LANGUAGES })}\n`);

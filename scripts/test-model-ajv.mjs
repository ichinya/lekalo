#!/usr/bin/env node

// Release gate for the pinned third-party JSON Schema implementation.
// Install Ajv outside the repository and expose that node_modules directory
// through NODE_PATH; no dependency or lockfile belongs to this contract repo.

import { createRequire } from "node:module";
import { readFile, readdir } from "node:fs/promises";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  MODEL_COMPAT_GOLDEN_PAIR,
  MODEL_DOCUMENT_SCHEMA,
  MODEL_FIXTURE_SETS,
  fixtureManifestParityFailure,
  manifestIntegrityFailure
} from "./model-fixture-manifest.mjs";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  Ajv2020 = require("ajv/dist/2020").default;
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-unavailable",
    detail: error?.code ?? error?.message
  }, null, 2)}\n`);
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const CASES = MODEL_FIXTURE_SETS.map((fixtureSet) => ({
  version: fixtureSet.version,
  schema: fixtureSet.schema,
  fixtures: fixtureSet.fixtures,
  schemaInvalid: new Set(fixtureSet.schemaInvalid)
}));

async function modelDocuments(projectRoot) {
  const output = [];
  async function walk(path) {
    for (const entry of (await readdir(path, { withFileTypes: true })).sort((left, right) =>
      Buffer.compare(Buffer.from(left.name, "utf8"), Buffer.from(right.name, "utf8")))) {
      const child = join(path, entry.name);
      if (entry.isDirectory()) {
        await walk(child);
      } else if (MODEL_DOCUMENT_SCHEMA[entry.name] !== undefined) {
        output.push(child);
      }
    }
  }
  await walk(join(projectRoot, "lekalo"));
  return output;
}

if (ajvVersion !== "8.17.1") {
  process.stderr.write(`${JSON.stringify({
    ok: false,
    reason: "ajv-version",
    expected: "8.17.1",
    actual: ajvVersion
  }, null, 2)}\n`);
  process.exit(1);
}

const warnings = [];
const ajv = new Ajv2020({
  strict: true,
  allErrors: true,
  logger: {
    log() {},
    warn(message) { warnings.push(String(message)); },
    error(message) { warnings.push(String(message)); }
  }
});
const failures = [];
const validators = new Map();
for (const contractCase of CASES) {
  const schema = JSON.parse(await readFile(join(root, contractCase.schema), "utf8"));
  try {
    ajv.addSchema(schema);
    const byDocument = new Map();
    for (const [fileName, definitionName] of Object.entries(MODEL_DOCUMENT_SCHEMA)) {
      byDocument.set(fileName, ajv.compile({ $ref: `${schema.$id}#/$defs/${definitionName}` }));
    }
    validators.set(contractCase.version, {
      umbrella: ajv.getSchema(schema.$id),
      byDocument
    });
  } catch (error) {
    failures.push({
      case: `compile:${contractCase.version}`,
      detail: error?.message
    });
  }
}

let documentCases = 0;
let fixtureCases = 0;
for (const contractCase of CASES) {
  const fixtureRoot = join(root, contractCase.fixtures);
  const versionValidators = validators.get(contractCase.version);
  if (versionValidators === undefined) continue;
  const entries = (await readdir(fixtureRoot, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name);
  // Fail closed on inventory drift: a fixture directory that exists on disk
  // without a manifest entry (or vice versa) must fail this gate, not slip
  // through one suite silently. The shared manifest is the single source.
  const parityFailure = fixtureManifestParityFailure(
    MODEL_FIXTURE_SETS.find((fixtureSet) => fixtureSet.version === contractCase.version),
    entries
  );
  if (parityFailure !== null) {
    failures.push({ case: `fixture-manifest:${contractCase.version}`, detail: parityFailure });
  }
  for (const entry of entries.sort((left, right) =>
    Buffer.compare(Buffer.from(left, "utf8"), Buffer.from(right, "utf8")))) {
    fixtureCases += 1;
    let schemaInvalid = false;
    for (const path of await modelDocuments(join(fixtureRoot, entry))) {
      documentCases += 1;
      let document;
      try {
        document = JSON.parse(await readFile(path, "utf8"));
      } catch {
        schemaInvalid = true;
        continue;
      }
      const specialized = versionValidators.byDocument.get(basename(path));
      const specializedValid = specialized(document);
      const umbrellaValid = versionValidators.umbrella(document);
      if (!specializedValid || !umbrellaValid) schemaInvalid = true;
    }
    const expectedInvalid = contractCase.schemaInvalid.has(entry);
    if (schemaInvalid !== expectedInvalid) {
      failures.push({
        case: `fixture:${contractCase.version}:${entry}`,
        expectedSchemaInvalid: expectedInvalid,
        actualSchemaInvalid: schemaInvalid
      });
    }
  }
}

// The compatibility golden pair must pass third-party validation against its
// exact respective schemas on both sides. Assertions prove both sides were
// exercised: each side requires its own compiled validators and at least one
// document, and both sides are reported individually.
const compatGolden = {};
let compatGoldenCases = 0;
if (manifestIntegrityFailure() !== null) {
  failures.push({ case: "fixture-manifest:integrity", detail: manifestIntegrityFailure() });
}
for (const side of MODEL_COMPAT_GOLDEN_PAIR.sides) {
  const versionValidators = validators.get(side.version);
  const goldenRoot = join(root, MODEL_COMPAT_GOLDEN_PAIR.root, side.directory);
  if (versionValidators === undefined) {
    failures.push({ case: `compat-golden:${side.version}:validators`, detail: "no compiled validators for the golden side schema" });
    continue;
  }
  const paths = await modelDocuments(goldenRoot);
  if (paths.length === 0) {
    failures.push({ case: `compat-golden:${side.version}:empty`, detail: goldenRoot });
    continue;
  }
  let goldenInvalid = false;
  for (const path of paths) {
    compatGoldenCases += 1;
    let document = null;
    try {
      document = JSON.parse(await readFile(path, "utf8"));
    } catch (error) {
      failures.push({ case: `compat-golden:${side.version}:parse`, detail: error?.message });
      goldenInvalid = true;
      continue;
    }
    const specialized = versionValidators.byDocument.get(basename(path));
    if (!specialized(document) || !versionValidators.umbrella(document)) {
      goldenInvalid = true;
    }
  }
  compatGolden[side.version] = paths.length;
  if (goldenInvalid) {
    failures.push({ case: `compat-golden:${side.version}:schema-valid`, detail: "the golden side must validate against its exact schema" });
  }
}

if (warnings.length > 0) failures.push({ case: "strict-warnings", detail: warnings });
if (failures.length > 0) {
  process.stderr.write(`${JSON.stringify({ ok: false, ajvVersion, failures }, null, 2)}\n`);
  process.exit(1);
}
process.stdout.write(`${JSON.stringify({
  ok: true,
  ajvVersion,
  draft: "2020-12",
  strict: true,
  allErrors: true,
  warnings: 0,
  schemas: CASES.length,
  fixtureCases,
  documentCases,
  compatGoldenSides: MODEL_COMPAT_GOLDEN_PAIR.sides.length,
  compatGoldenCases,
  compatGoldenDocuments: compatGolden
}, null, 2)}\n`);

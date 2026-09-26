#!/usr/bin/env node
// Issue #46 release gate: the rendered OpenAPI documents validate
// against the official OpenAPI Initiative 3.1 meta-schema (pinned
// file plus sha256 sidecar) under the same pinned Draft 2020-12
// implementation as the other contract gates (exact Ajv 8.17.1,
// provisioned outside this checkout, exposed through NODE_PATH /
// LEKALO_AJV_NODE_PATH). The gate also pins the canonical byte form
// of the golden, the closed YAML dialect of the emitted document, the
// ownership-manifest wire shape, the diff pointer view, and the
// trace-chain fixture identity.
import { createRequire } from "node:module";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const require = createRequire(import.meta.url);
let Ajv2020;
let ajvVersion;
try {
  ({ default: Ajv2020 } = require("ajv/dist/2020.js"));
  ajvVersion = require("ajv/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-unavailable", detail: String(error) }, null, 2)}\n`,
  );
  process.exit(1);
}
if (ajvVersion !== "8.17.1") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "ajv-version", detail: ajvVersion }, null, 2)}\n`,
  );
  process.exit(1);
}

// The validator-custody choice (recorded per plan §7): the official
// meta-schema above is the validation contract and stays pinned, but
// its $dynamicRef-based Parameter/Response discrimination is
// unreliable under Ajv 2020-12 processors, so the document pass runs
// through the plan's sanctioned fallback
// @seriousme/openapi-schema-validator (pinned 2.8.0, Ajv 8.x family)
// and the compiled official schema rides as a load-custody check.
const requireFallback = createRequire(import.meta.url);
let Validator;
let validatorVersion;
try {
  // The validator publishes ESM only: require() throws ERR_REQUIRE_ESM
  // on Node <22, so resolve through NODE_PATH and load via import().
  const entry = requireFallback.resolve("@seriousme/openapi-schema-validator");
  ({ Validator } = await import(pathToFileURL(entry).href));
  validatorVersion = requireFallback("@seriousme/openapi-schema-validator/package.json").version;
} catch (error) {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "openapi-validator-unavailable", detail: String(error) }, null, 2)}\n`,
  );
  process.exit(1);
}
// The fallback is the authoritative document pass: an unpinned
// install would silently change what the gate accepts (r1 cline F-7).
if (validatorVersion !== "2.8.0") {
  process.stderr.write(
    `${JSON.stringify({ ok: false, reason: "openapi-validator-version", detail: validatorVersion }, null, 2)}\n`,
  );
  process.exit(1);
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (relative) => JSON.parse(readFileSync(resolve(root, relative), "utf8"));
const readText = (relative) => readFileSync(resolve(root, relative), "utf8");
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

// ---------------------------------------------------------------------------
// 1. The pinned official meta-schemas: exact bytes per the sidecars.
// ---------------------------------------------------------------------------
for (const name of ["openapi.schema.v3.1.0.json", "openapi.schema.v3.0.0.json"]) {
  const bytes = readFileSync(resolve(root, "tests/fixtures/openapi/meta", name));
  const expected = readText(`tests/fixtures/openapi/meta/${name}.sha256`).trim();
  if (sha256(bytes) !== expected) {
    fail("meta-schema-pin", { name, expected, actual: sha256(bytes) });
  }
}
const metaSchema31 = read("tests/fixtures/openapi/meta/openapi.schema.v3.1.0.json");
// The upstream meta-schema predates strictTypes hygiene; the strict
// gates stay with the Lekalo-owned schemas. The compile is a custody
// load check only — document pass/fail belongs to the fallback
// validator below.
const ajv = new Ajv2020({ strict: false, allErrors: true });
try {
  ajv.compile(metaSchema31);
} catch (error) {
  fail("meta-schema-compile", String(error));
}

async function main() {
const validator = new Validator();
const validateDocument = async (document) => (await validator.validate(document)).valid;

// ---------------------------------------------------------------------------
// 2. The canonical golden document validates against the meta-schema
//    and is the exact canonical byte form (compact, byte-sorted keys).
// ---------------------------------------------------------------------------
const goldenText = readText("tests/fixtures/openapi/valid/planner.openapi.json");
const golden = JSON.parse(goldenText);
const canonicalJson = (value) => {
  if (value === null) return "null";
  switch (typeof value) {
    case "boolean":
      return value ? "true" : "false";
    case "number":
      return JSON.stringify(value);
    case "string":
      return JSON.stringify(value);
    case "object": {
      if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
      const keys = Object.keys(value).sort((a, b) => {
        const left = Buffer.from(a, "utf8");
        const right = Buffer.from(b, "utf8");
        const length = Math.min(left.length, right.length);
        for (let index = 0; index < length; index += 1) {
          if (left[index] !== right[index]) return left[index] - right[index];
        }
        return left.length - right.length;
      });
      return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
    }
    default:
      throw new TypeError("unserializable");
  }
};
if (canonicalJson(golden) !== goldenText.trimEnd()) {
  fail("golden-not-canonical", "the golden bytes drift from the canonical form");
}
if (!(await validateDocument(golden))) {
  fail("golden-meta-schema-invalid", validateDocument.errors);
}
// The provenance block pins the generator identity and the exact pins.
const provenance = golden["x-lekalo-provenance"];
if (
  provenance?.generator?.id !== "lekalo-core/openapi"
  || provenance?.generator?.version !== "0.4.0"
  || provenance?.transportRef?.schemaVersion !== "lekalo/transport-http/v0.4.0"
  || !String(provenance?.transportRef?.digest ?? "").startsWith("sha256:")
) {
  fail("golden-provenance", provenance);
}
// Every operation carries its semantic anchors.
for (const [template, item] of Object.entries(golden.paths)) {
  for (const [method, operation] of Object.entries(item)) {
    if (!operation["x-lekalo-endpoint"] || !operation["x-lekalo-operation"]) {
      fail("golden-anchor-missing", `${template} ${method}`);
    }
  }
}
for (const [name, schema] of Object.entries(golden.components?.schemas ?? {})) {
  if (!schema["x-lekalo-symbol"]) fail("golden-symbol-missing", name);
}

// ---------------------------------------------------------------------------
// 3. The emitted YAML dialect: a closed reader for the emitter's own
//    block-style output (quoted scalars, two-space indents, the two
//    empty flow literals). The parsed document validates identically.
// ---------------------------------------------------------------------------
function parseEmitterYaml(text) {
  const lines = text
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => ({
      indent: line.length - line.trimStart().length,
      text: line.trim(),
    }));
  let position = 0;
  const parseScalar = (token) => {
    if (token === "{}") return {};
    if (token === "[]") return [];
    if (token === "null") return null;
    if (token === "true") return true;
    if (token === "false") return false;
    if (/^-?\d+$/.test(token)) return Number(token);
    return JSON.parse(token);
  };
  const splitMember = (text) => {
    // Split on the first `: ` outside the JSON-quoted key; a member
    // may also end at a bare `:` (a nested block header).
    const quoted = text.startsWith('"');
    const keyEnd = quoted ? text.indexOf('"', 1) + 1 : 0;
    const inline = text.indexOf(": ", keyEnd);
    if (inline < 0 && text.endsWith(":")) {
      return [JSON.parse(text.slice(0, text.length - 1)), ""];
    }
    return [JSON.parse(text.slice(0, inline)), text.slice(inline + 2)];
  };
  // Space-based recursive descent over the closed dialect: every
  // child block is exactly two spaces deeper than its parent key.
  const parseBlock = (indent) => {
    if (position >= lines.length || lines[position].indent < indent) return {};
    if (lines[position].text.startsWith("- ")) {
      const sequence = [];
      while (position < lines.length && lines[position].indent === indent
        && lines[position].text.startsWith("- ")) {
        const rest = lines[position].text.slice(2);
        const at = position;
        position += 1;
        if (rest === "{}") { sequence.push({}); continue; }
        if (rest === "[]") { sequence.push([]); continue; }
        if (rest.includes(": ") || rest.endsWith(":")) {
          const [firstKey, firstValue] = splitMember(rest);
          const object = {};
          if (firstValue !== "") {
            object[firstKey] = parseScalar(firstValue);
          } else {
            object[firstKey] = parseBlock(indent + 2);
          }
          while (position < lines.length && lines[position].indent === indent + 2
            && !lines[position].text.startsWith("- ")) {
            const [key, value] = splitMember(lines[position].text);
            position += 1;
            object[key] = value !== "" ? parseScalar(value) : parseBlock(indent + 4);
          }
          sequence.push(object);
        } else {
          sequence.push(parseScalar(rest));
        }
      }
      return sequence;
    }
    const mapping = {};
    while (position < lines.length && lines[position].indent === indent
      && !lines[position].text.startsWith("- ")) {
      const [key, value] = splitMember(lines[position].text);
      position += 1;
      mapping[key] = value !== "" ? parseScalar(value) : parseBlock(indent + 2);
    }
    return mapping;
  };
  return parseBlock(lines[0]?.indent ?? 0);
}
const yamlGolden = readText("tests/fixtures/openapi/valid/planner.openapi.yaml");
const fromYaml = parseEmitterYaml(yamlGolden);
if (canonicalJson(fromYaml) !== canonicalJson(golden)) {
  fail("yaml-golden-drift", "the emitted YAML does not round-trip the golden");
}
if (!(await validateDocument(fromYaml))) {
  fail("yaml-golden-meta-schema-invalid", validateDocument.errors);
}

// ---------------------------------------------------------------------------
// 4. The invalid vectors: the meta-schema refuses a dangling $ref and
//    an unsupported declared version; the YAML vectors document the
//    closed import subset refusals (Rust-owned, pinned as data here).
// ---------------------------------------------------------------------------
const dangling = read("tests/fixtures/openapi/invalid/dangling-ref.json");
if (await validateDocument(dangling)) {
  fail("dangling-ref-not-refused", "the meta-schema must refuse a non-document $ref");
}
const wrongVersion = read("tests/fixtures/openapi/invalid/wrong-openapi-version.json");
if (await validateDocument(wrongVersion)) {
  fail("wrong-version-not-refused", "the meta-schema must refuse openapi 2.0");
}
if (!readText("tests/fixtures/openapi/invalid/bare-version.yaml").includes("openapi: 3.1.0")) {
  fail("bare-version-fixture", "the bare dotted numeral is the refused float-like form");
}
if (!readText("tests/fixtures/openapi/invalid/flow-content.yaml").includes("openapi: {")) {
  fail("flow-content-fixture", "flow content is outside the closed import subset");
}

// ---------------------------------------------------------------------------
// 5. The ownership manifest: closed contract, byte-sorted pointers,
//    every pointer claimed by a bounded owner.
// ---------------------------------------------------------------------------
const ownership = read("tests/fixtures/openapi/merge/planner.openapi.ownership.json");
if (ownership.contract !== "lekalo/openapi-map/v0.4.0") {
  fail("ownership-contract", ownership.contract);
}
if (ownership.generator?.id !== "lekalo-core/openapi") {
  fail("ownership-generator", ownership.generator);
}
const pointerIds = Object.keys(ownership.pointers ?? {});
const sortedIds = [...pointerIds].sort((a, b) => {
  const left = Buffer.from(a, "utf8");
  const right = Buffer.from(b, "utf8");
  const length = Math.min(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return left.length - right.length;
});
if (JSON.stringify(pointerIds) !== JSON.stringify(sortedIds)) {
  fail("ownership-pointers-unsorted", pointerIds);
}
if (pointerIds.some((pointer) => !pointer.startsWith("/"))) {
  fail("ownership-pointer-shape", pointerIds);
}
if (pointerIds.some((pointer) => (ownership.pointers[pointer] ?? "").length === 0)) {
  fail("ownership-owner-missing", pointerIds);
}

// ---------------------------------------------------------------------------
// 6. The diff pointer view: the reused transport classes, non-empty
//    pointer locations, and the pinned blocking verdict.
// ---------------------------------------------------------------------------
const pointerView = read(
  "tests/fixtures/openapi/diff/candidate-add-required-param.pointerview.json",
);
const classes = new Set(["breaking", "non-breaking", "policy-change"]);
const diffPaths = pointerView.openapiDiff?.paths ?? [];
if (diffPaths.length === 0) fail("pointer-view-empty", pointerView.openapiDiff);
for (const path of diffPaths) {
  if (!classes.has(path.class)) fail("pointer-view-class", path);
  if (!Array.isArray(path.pointers) || path.pointers.length === 0) {
    fail("pointer-view-pointers", path);
  }
}
if (pointerView.openapiDiff?.wireConsumerBlocked !== true) {
  fail("pointer-view-blocking", pointerView.openapiDiff?.wireConsumerBlocked);
}

// ---------------------------------------------------------------------------
// 6b. The committed 3.0 golden: the same document class at the
//     declared-3.0 dialect — validated through the same fallback
//     validator (it detects the version from the openapi member) and
//     audited for 3.1-only constructs (r1 F-3/cline F-1).
// ---------------------------------------------------------------------------
const golden30Text = readText("tests/fixtures/openapi/valid/planner.openapi.3_0.json");
const golden30 = JSON.parse(golden30Text);
if (golden30.openapi !== "3.0.0") {
  fail("golden30-version", golden30.openapi);
}
if (golden30Text.includes('"const"')) {
  fail("golden30-dialect-const", "3.0 spells single-value enums, never const");
}
if (/"type":\[[^\]]*"null"/.test(golden30Text)) {
  fail("golden30-dialect-type-array", "3.0 never widens a type array with null");
}
if (!(await validateDocument(golden30))) {
  fail("golden30-invalid", "the 3.0 golden fails the 3.0 meta-schema");
}

// ---------------------------------------------------------------------------
// 7. The trace-chain fixture: the artifact node pins the exact golden
//    bytes and the generator identity.
// ---------------------------------------------------------------------------
const trace = read("tests/fixtures/openapi/trace/planner-openapi.trace.json");
const artifact = (trace.nodes ?? []).find((node) => node.nodeKind === "artifact");
if (artifact?.generatorRef !== "dev.lekalo/lekalo-core") {
  fail("trace-generator", artifact);
}
const goldenDigest = sha256(readFileSync(resolve(root, "tests/fixtures/openapi/valid/planner.openapi.json")));
if (artifact?.contentDigest !== `sha256:${goldenDigest}`) {
  fail("trace-artifact-digest", artifact?.contentDigest);
}
const exportGolden = read(
  "tests/fixtures/openapi/trace/planner-openapi.export.golden.json",
);
if (exportGolden.trace?.manifestId !== trace.manifestId) {
  fail("trace-export-manifest", exportGolden.trace?.manifestId);
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  ajv: ajvVersion,
  openapiValidator: validatorVersion,
  metaSchema: metaSchema31.$id,
  goldenValid: true,
  golden30Valid: true,
  yamlGoldenValid: true,
  invalidRefused: 2,
  ownershipPointers: pointerIds.length,
  diffPaths: diffPaths.length,
  traceNodes: (trace.nodes ?? []).length,
}, null, 2)}\n`);

}

main().catch((error) => fail("gate-crash", String(error)));

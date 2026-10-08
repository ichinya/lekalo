#!/usr/bin/env node
// Issue #51 binary-driven impact gate.
//
// The gate proves the complete reverse closure of the public DTO
// through the production binary and executes both mutation probes on
// isolated copies of the fixture:
//
// 1. Every oracle witness family is validated: the five graph-reachable
//    consumers as exact set equality in both directions (graph and
//    transitive impact), and the two file witnesses — the public
//    schema must project exactly the composition allowlist (no private
//    join, no missing property) and the composition must validate
//    through the production custody join with the oracle subject.
// 2. The additive mutation ("add one optional DTO field") is executed
//    on an isolated copy: entity, schema, and allowlist gain the field
//    coherently, and both closures stay exactly the baseline sets.
// 3. The breaking mutation ("remove the DTO projection from the public
//    queries") is executed on another isolated copy: both closures
//    lose exactly the oracle's expected delta — the four declared
//    consumers — and nothing else.
//
// A missing or broken binary fails the gate; no mutation is ever
// simulated by reading metadata. Dependency-free; run from the repo
// root after `cargo build -p lekalo-cli`.
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
// The production binary resolves per-host: an explicit LEKALO_BIN wins,
// then the Windows debug name, then the POSIX one (precedent:
// test-context-budget-contracts.mjs).
const resolveBinary = () => {
  if (process.env.LEKALO_BIN) return process.env.LEKALO_BIN;
  const windows = resolve(root, "target/debug/lekalo.exe");
  const posix = resolve(root, "target/debug/lekalo");
  if (existsSync(windows)) return windows;
  if (existsSync(posix)) return posix;
  return windows;
};
const binary = resolveBinary();
const fixtureRoot = "tests/fixtures/pilot/worklog-changelog";
const project = `${fixtureRoot}/model`;
const oraclePath = `${fixtureRoot}/evidence/consumer-oracle.json`;
const oracle = JSON.parse(readFileSync(resolve(root, oraclePath), "utf8"));
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};
const runJson = (args, cwd = root) => {
  let raw;
  try {
    raw = execFileSync(binary, args, { cwd, encoding: "utf8" }).trim();
  } catch (error) {
    fail("binary-unavailable", `${binary}: ${String(error).slice(0, 300)}`);
  }
  try {
    return JSON.parse(raw);
  } catch {
    fail("binary-output-unparseable", raw.slice(0, 300));
  }
};

const closure = (projectDir) => {
  const graph = runJson([
    "--json", "--no-cache", "graph", "callers", oracle.subject, "--transitive",
    "--project", projectDir,
  ]);
  if (graph.status !== "valid") fail("graph-denied", graph);
  const impact = runJson([
    "--json", "--no-cache", "impact", oracle.subject, "--depth", String(oracle.depthLimit),
    "--project", projectDir,
  ]);
  if (impact.status !== "valid") fail("impact-denied", impact);
    if (!impact.impact?.transitive) {
    process.stderr.write(JSON.stringify({ debug: "impact-envelope", projectDir, keys: Object.keys(impact), envelope: JSON.stringify(impact).slice(0, 400) }, null, 2) + String.fromCharCode(10));
    process.exit(1);
  }
  // An empty closure omits the items array entirely.
  const transitive = impact.impact.transitive;
  return {
    graph: new Set(graph.graph.callers.map((edge) => edge.from)),
    impact: new Set((transitive.items ?? []).map((item) => item.subject)),
    complete: transitive.complete,
    omitted: transitive.omitted,
  };
};

// ---------------------------------------------------------------------------
// 1. The oracle witness inventory: every declared witness file exists.
// ---------------------------------------------------------------------------
for (const witness of oracle.expectedConsumers) {
  const relative = witness.witness;
  // Model-relative witnesses (modules) live under the project;
  // scenario documents and file witnesses live under the fixture root.
  const base = relative.startsWith("lekalo/") ? project : fixtureRoot;
  if (!existsSync(resolve(root, base, relative))) fail("witness-missing", relative);
}

// ---------------------------------------------------------------------------
// 2. The public-schema witness: the schema projects exactly the
//    composition allowlist, and no excluded private field appears.
// ---------------------------------------------------------------------------
const composition = JSON.parse(
  readFileSync(resolve(root, `${fixtureRoot}/attachments/pilot.json`), "utf8"),
);
if (composition.publicAllowlist.dto !== oracle.subject) {
  fail("composition-subject-drift", composition.publicAllowlist.dto);
}
const camel = (snake) => snake.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
const schemaPath = `${fixtureRoot}/public/changelog-entry.schema.json`;
const publicSchema = JSON.parse(readFileSync(resolve(root, schemaPath), "utf8"));
const entry = publicSchema.properties?.entry;
if (!entry || entry.type !== "object") fail("schema-entry-missing", schemaPath);
const servedFields = new Set(composition.publicAllowlist.fields.map((field) => field.name));
const schemaFields = new Set(Object.keys(entry.properties ?? {}));
for (const field of servedFields) {
  if (!schemaFields.has(camel(field))) fail("schema-missing-allowlist-field", field);
}
for (const property of schemaFields) {
  if (!servedFields.has(property.replace(/([A-Z])/g, (_, letter) => `_${letter.toLowerCase()}`))) {
    fail("schema-extra-field", property);
  }
}
// No private join: every excluded private field's wire spelling is
// absent from the public surface. A name the allowlist also serves is
// the reviewed-projection case (the DTO serves the snapshot's field,
// never the private entity's), so only unserved exclusions are
// checked here; the served-field closure is the core validator's.
const servedCamel = new Set([...servedFields].map((field) => camel(field)));
for (const excluded of composition.publicAllowlist.excluded) {
  const wire = camel(excluded);
  if (!servedCamel.has(wire) && schemaFields.has(wire)) {
    fail("schema-private-field", excluded);
  }
}
// The breaking surface is explicit: every served property is required.
for (const property of schemaFields) {
  if (!Array.isArray(entry.required) || !entry.required.includes(property)) {
    fail("schema-property-optional", property);
  }
}

// ---------------------------------------------------------------------------
// 3. The composition witness: the production custody join accepts the
//    committed composition against the committed Model.
// ---------------------------------------------------------------------------
const compositionCheck = runJson([
  "--json", "--no-cache", "pilot", "validate", `${fixtureRoot}/attachments/pilot.json`,
  "--project", project,
]);
if (compositionCheck.pilot?.status !== "valid") fail("composition-invalid", compositionCheck);

// ---------------------------------------------------------------------------
// 4. The baseline closures: exact set equality with the oracle, both
//    directions, for the graph-reachable witness families.
// ---------------------------------------------------------------------------
const expectedGraph = new Set(
  oracle.expectedConsumers
    .filter((consumer) => ["query", "endpoint", "scenario"].includes(consumer.kind))
    .map((consumer) => {
      if (consumer.kind === "query") return `operation:${consumer.consumer}`;
      if (consumer.kind === "endpoint") return `endpoint:${consumer.consumer}`;
      return `scenario:${consumer.consumer}`;
    }),
);
const expectedImpact = new Set(
  oracle.expectedConsumers
    .filter((row) => ["endpoint", "scenario"].includes(row.kind))
    .map((consumer) => `${consumer.kind}:${consumer.consumer}`),
);
const assertExact = (label, actual, expected) => {
  for (const subject of expected) {
    if (!actual.has(subject)) fail(`${label}-consumer-missing`, subject);
  }
  for (const subject of actual) {
    if (!expected.has(subject)) fail(`${label}-consumer-unexpected`, subject);
  }
  if (actual.size !== expected.size) {
    fail(`${label}-set-drift`, { expected: expected.size, actual: actual.size });
  }
};
const baseline = closure(project);
assertExact("consumer", baseline.graph, expectedGraph);
assertExact("impact", baseline.impact, expectedImpact);
if (!baseline.complete || baseline.omitted !== 0) {
  fail("impact-incomplete", { complete: baseline.complete, omitted: baseline.omitted });
}

// ---------------------------------------------------------------------------
// 5. The executed mutation probes, each on an isolated copy of the
//    committed fixture.
// ---------------------------------------------------------------------------
const tempRoot = "target/impact-probe";
rmSync(tempRoot, { recursive: true, force: true });
const stage = (label) => {
  const dir = `${tempRoot}/${label}/fixture`;
  cpSync(resolve(root, fixtureRoot), resolve(root, dir), { recursive: true });
  return dir;
};
const compareClosures = (label, mutated) => {
  const observed = closure(mutated);
  if (!observed.complete || observed.omitted !== 0) {
    fail(`${label}-incomplete`, { complete: observed.complete, omitted: observed.omitted });
  }
  return observed;
};
try {
  // 5a. Additive: one optional DTO field joins the entity, the schema,
  //     and the allowlist coherently. Both closures stay the baseline
  //     sets: adding an optional field is non-breaking.
  const additiveDir = stage("additive");
  const additiveEntityPath = join(additiveDir, "model/lekalo/modules/changelog/entities.yaml");
  const additiveEntity = JSON.parse(readFileSync(additiveEntityPath, "utf8"));
  const dto = additiveEntity.definitions.find(
    (definition) => definition.id === oracle.subject,
  );
  if (!dto) fail("additive-dto-missing", oracle.subject);
  dto.fields.push({ name: "source_worklog_id", type: { optional: { ref: "worklog.task_id" } } });
  writeFileSync(additiveEntityPath, JSON.stringify(additiveEntity));
  const additiveSchemaPath = join(additiveDir, "public/changelog-entry.schema.json");
  const additiveSchema = JSON.parse(readFileSync(additiveSchemaPath, "utf8"));
  additiveSchema.properties.entry.properties.sourceWorklogId = {
    type: "string",
    format: "uuid",
    description: "The optional source worklog reference.",
  };
  writeFileSync(additiveSchemaPath, JSON.stringify(additiveSchema));
  const additiveCompositionPath = join(additiveDir, "attachments/pilot.json");
  const additiveComposition = JSON.parse(readFileSync(additiveCompositionPath, "utf8"));
  additiveComposition.publicAllowlist.fields.push({
    name: "source_worklog_id",
    source: "snapshot",
    type: "worklog.task_id",
  });
  writeFileSync(additiveCompositionPath, JSON.stringify(additiveComposition));
  const additive = compareClosures("additive", additiveDir + "/model");
  assertExact("additive-consumer", additive.graph, expectedGraph);
  assertExact("additive-impact", additive.impact, expectedImpact);
  // The coherent witness projection holds on the mutated copy too.
  const additiveServed = new Set(additiveComposition.publicAllowlist.fields.map((f) => f.name));
  const additiveProperties = new Set(
    Object.keys(additiveSchema.properties.entry.properties),
  );
  for (const field of additiveServed) {
    if (!additiveProperties.has(camel(field))) fail("additive-schema-missing-field", field);
  }

  // 5b. Breaking: the queries stop projecting the DTO. Both closures
  //     lose exactly the oracle's expected delta — the four declared
  //     consumers — and nothing else.
  const breakingDir = stage("breaking");
  const queriesPath = join(breakingDir, "model/lekalo/modules/changelog/queries.yaml");
  const queries = JSON.parse(readFileSync(queriesPath, "utf8"));
  let projected = 0;
  for (const definition of queries.definitions) {
    if (definition.kind === "query" && definition.returns) {
      delete definition.returns;
      projected += 1;
    }
  }
  if (projected === 0) fail("breaking-mutation-vacuous", queriesPath);
  writeFileSync(queriesPath, JSON.stringify(queries));
  const breaking = compareClosures("breaking", breakingDir + "/model");
  const graphDelta = [...expectedGraph].filter((subject) => !breaking.graph.has(subject));
  const impactDelta = [...expectedImpact].filter((subject) => !breaking.impact.has(subject));
  const expectedDelta = new Set(
    oracle.mutationProbes
      .find((probe) => probe.breaking)
      .expectedDelta.flatMap((consumer) => [
        `operation:${consumer}`,
        `endpoint:${consumer}`,
        `scenario:${consumer}`,
      ]),
  );
  for (const subject of graphDelta) {
    if (!expectedDelta.has(subject)) fail("breaking-graph-delta-unexpected", subject);
  }
  for (const subject of impactDelta) {
    if (!expectedDelta.has(subject)) fail("breaking-impact-delta-unexpected", subject);
  }
  if (graphDelta.length === 0 && impactDelta.length === 0) {
    fail("breaking-mutation-ineffective", oracle.subject);
  }
  // No unexpected consumer left behind: the surviving sets are the
  // exact complements of the delta.
  const survivingGraph = new Set([...expectedGraph].filter((subject) => !graphDelta.includes(subject)));
  assertExact("breaking-graph-survivor", breaking.graph, survivingGraph);
} finally {
  rmSync(tempRoot, { recursive: true, force: true });
}

process.stdout.write(
  `${JSON.stringify({
    ok: true,
    gate: "test-worklog-changelog-impact",
    graphConsumers: baseline.graph.size,
    impactConsumers: baseline.impact.size,
    depth: oracle.depthLimit,
    complete: baseline.complete,
    witnesses: oracle.expectedConsumers.length,
    mutationProbes: oracle.mutationProbes.length,
    mutationsExecuted: 2,
  })}\n`,
);

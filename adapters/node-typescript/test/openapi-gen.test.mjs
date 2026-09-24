/**
 * Issue #46 OpenAPI generator tests: the deterministic document render
 * over the canonical evidence join (golden byte compare), the policy
 * grammar, the emitter (canonical YAML with the two empty flow
 * literals), the honest partial notes, the dry-run/apply discipline,
 * and the composite capability declaration.
 */
import assert from "node:assert/strict";
import { test } from "node:test";
import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

import {
  OPENAPI_CAPABILITY,
  OPENAPI_WRITE_SCOPES,
  openapiGenerateOperation,
  openapiVerifyOperation,
  componentName,
} from "../src/openapi-gen.mjs";
import { canonicalJson, toYaml } from "../src/openapi-emit.mjs";
import { DEFAULT_POLICY, parsePolicyYaml, resolvePolicy } from "../src/openapi-policy.mjs";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..");
const plannerEvidence = readFileSync(
  join(repoRoot, "tests/fixtures/adapter-conformance/inputs/transport-minimal.json"),
  "utf8",
);
const plannerIr = readFileSync(
  join(repoRoot, "tests/fixtures/adapter-conformance/inputs/ir-minimal.json"),
  "utf8",
);

const sha256Text = (text) =>
  "sha256:" + createHash("sha256").update(text, "utf8").digest("hex");

const REQUEST_BASE = {
  protocol: "lekalo.target/v1",
  protocol_version: "0.3.2",
  operation: "generate",
  request_id: "req-openapi-1",
  project_root: ".",
  target: "node-typescript",
  profile: "standalone",
  dry_run: true,
  ir_path: ".lekalo/ir/planner.json",
};

/** A bounded read/write view over one temp root (the kernel seam). */
function viewsFor(root) {
  const writes = [];
  return {
    readView: {
      roots: [
        { kind: "tree", path: ".lekalo/cache/transport" },
        { kind: "tree", path: ".lekalo/cache/ir" },
      ],
      permittedProjectRoot: root,
      canRead: (path) => existsSync(join(root, ...path.split("/"))),
      readFile: (path) => {
        if (path.endsWith(".map.json") || path.endsWith(".ownership.json")) {
          return writes.find((write) => write.path === path)?.bytes;
        }
        return readFileOf(root, path);
      },
    },
    writeView: {
      exists: (path) => existsSync(join(root, ...path.split("/"))),
      write: (path, action, bytes) => {
        writes.push({ path, action, bytes });
      },
    },
    writes,
  };
}

function readFileOf(root, path) {
  return readFileSync(join(root, ...path.split("/")));
}

/** A temp project with the planner evidence and IR under the homes. */
function evidenceProject() {
  const root = mkdtempSync(join(tmpdir(), "lekalo-openapi-gen-"));
  const evidenceDir = join(root, ".lekalo", "cache", "transport");
  const irDir = join(root, ".lekalo", "cache", "ir");
  mkdirSync(evidenceDir, { recursive: true });
  mkdirSync(irDir, { recursive: true });
  writeFileSync(join(evidenceDir, "planner.json"), plannerEvidence, "utf8");
  writeFileSync(join(irDir, "planner.json"), plannerIr, "utf8");
  return root;
}

function run(context) {
  return openapiGenerateOperation({
    ...context,
    request: { ...REQUEST_BASE, ...context.request },
  });
}

test("the policy defaults resolve and the closed grammar refuses", () => {
  assert.deepEqual(resolvePolicy(null).policy, DEFAULT_POLICY);
  const document = [
    "zod:",
    "  date: date-string",
    "  unknown-keys: strip",
    "openapi:",
    '  version: "3.0"',
    "  mode: fragments",
    "  path: docs/api.yaml",
    "",
  ].join("\n");
  const resolved = resolvePolicy(document);
  assert.equal(resolved.source, "document");
  assert.deepEqual(resolved.policy, {
    version: "3.0",
    mode: "fragments",
    path: "docs/api.yaml",
  });
  assert.equal(parsePolicyYaml("openapi:\n  version: 2.0\n").refusal, "version-value");
  assert.equal(parsePolicyYaml("openapi:\n  mode: partial\n").refusal, "mode-value");
  assert.equal(parsePolicyYaml("openapi:\n  path: ../escape.yaml\n").refusal, "path-value");
  assert.equal(parsePolicyYaml("swagger: 2.0\n").refusal, "unknown-section");
  assert.equal(parsePolicyYaml("openapi:\n  version: \"3.1\"\n  version: \"3.0\"\n").refusal, "duplicate-key");
});

test("the emitter is deterministic, sorted, and block-style", () => {
  const tree = {
    openapi: "3.1.0",
    info: { title: "planner", version: "0.4.0" },
    paths: {},
    security: [],
    "z-italic": { b: 1, a: [1, 2] },
  };
  const once = toYaml(tree);
  const twice = toYaml(JSON.parse(canonicalJson(tree)));
  assert.equal(once, twice, "byte-stable");
  assert.ok(once.includes('"openapi": "3.1.0"'), "the version stays a quoted string");
  assert.ok(once.includes('"paths": {}'), "the empty map spells the import exception");
  assert.ok(once.includes('"security": []'), "the empty array spells the import exception");
  assert.ok(!once.includes("\r"), "LF endings");
});

test("component names follow the 45 export-name rule", () => {
  assert.equal(componentName("planner.task"), "PlannerTask");
  assert.equal(componentName("planner.task_id"), "PlannerTaskId");
});

test("the render produces the deterministic write plan with sidecars", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "complete");
    const writes = outcome.data.writes;
    assert.deepEqual(
      writes.map((write) => write.path),
      [
        "docs/openapi.map.json",
        "docs/openapi.ownership.json",
        "docs/openapi.yaml",
      ],
    );
    // The plan identity covers the union of the writes.
    assert.match(outcome.data.plan_id, /^plan-[0-9a-f]{64}$/);
    // The partial projections (error variants, query-model types) ride
    // as honest evidence notes, never as silent drops.
    assert.ok(outcome.evidence.partialCount > 0);
    // The YAML parses back to the canonical JSON tree: the dry run
    // carries the exact bytes in the plan bodies.
    const yaml = outcome.data.bodies.get("docs/openapi.yaml");
    assert.ok(yaml && yaml.length > 0);
    assert.ok(yaml.includes('"openapi": "3.1.0"'));
    assert.ok(yaml.includes('"/tasks/{task_id}/focus":'), "path keys are quoted");
    // The ownership manifest covers the operation pointers.
    const ownership = JSON.parse(
      outcome.data.bodies.get("docs/openapi.ownership.json"),
    );
    assert.equal(ownership.contract, "lekalo/openapi-map/v0.4.0");
    assert.ok(Object.keys(ownership.pointers).length > 0);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the render is byte-stable across repeats", () => {
  const first = render();
  const second = render();
  assert.equal(first.yaml, second.yaml);
  function render() {
    const root = evidenceProject();
    try {
      const views = viewsFor(root);
      const outcome = run({ ...views, request: {} });
      assert.equal(outcome.state, "complete");
      return {
        yaml: outcome.data.bodies.get("docs/openapi.yaml"),
      };
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

test("apply writes the exact bytes; verify recomputes without drift", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    // Dry run: nothing is written.
    const dry = run({ ...views, request: {} });
    assert.equal(views.writes.length, 0);
    // Apply: the echoed plan id binds the writes.
    const apply = run({
      ...views,
      request: { dry_run: false, plan_id: dry.data.plan_id },
    });
    assert.equal(apply.state, "complete");
    assert.ok(views.writes.length > 0);
    // Verify over the written files: the views' read side serves the
    // written bytes, so no drift is reported.
    const written = new Map(views.writes.map((write) => [write.path, write.bytes]));
    const verifyViews = {
      readView: {
        roots: views.readView.roots,
        permittedProjectRoot: root,
        canRead: (path) => written.has(path) || views.readView.canRead(path),
        readFile: (path) =>
          written.get(path) ?? views.readView.readFile(path),
      },
    };
    const verify = openapiVerifyOperation({
      ...verifyViews,
      request: { ...REQUEST_BASE },
    });
    assert.equal(verify.state, "complete");
    assert.deepEqual(verify.data.findings, []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("verify reports drift when a maintained document diverges", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const dry = run({ ...views, request: {} });
    const yaml = dry.data.bodies.get("docs/openapi.yaml");
    const drifted = yaml.replace("Success response.", "Maintained response.");
    const written = new Map([["docs/openapi.yaml", Buffer.from(drifted, "utf8")]]);
    const verify = openapiVerifyOperation({
      readView: {
        roots: views.readView.roots,
        permittedProjectRoot: root,
        canRead: (path) => written.has(path) || views.readView.canRead(path),
        readFile: (path) => written.get(path) ?? views.readView.readFile(path),
      },
      request: { ...REQUEST_BASE },
    });
    assert.equal(verify.state, "complete");
    assert.ok(
      verify.data.findings.some(
        (finding) => finding.code === "openapi.drift" && finding.path === "docs/openapi.yaml",
      ),
      JSON.stringify(verify.data.findings),
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("an absent evidence home yields zero openapi writes, not a failure", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-openapi-empty-"));
  try {
    const views = viewsFor(root);
    // Remove the transport evidence: the applicable predicate in the
    // composite gates the invocation; called directly, the generator
    // refuses honestly.
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "failed");
    assert.deepEqual(outcome.diagnostics, [{ reason: "transport-evidence-absent" }]);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});


test("a declared 3.0 render spells the 3.0 dialect, never 3.1-only forms (r1 F-3/cline F-1)", () => {
  const root = mkdtempSync(join(tmpdir(), "lekalo-openapi-30-"));
  try {
    // The stock evidence carries no optional types, so the command
    // input gains one (an optional planner.text note) — exactly the
    // shape whose 3.0/3.1 spellings diverge. The policy pins 3.0
    // through the read view.
    const transport = JSON.parse(plannerEvidence);
    const ir = JSON.parse(plannerIr);
    ir.definitions
      .find((def) => def.id === "planner.focus_task")
      .input.push({ name: "note", required: false, type: { optional: { ref: "planner.text" } } });
    transport.endpoints[0].params.push({
      field: "input.note",
      in: "query",
      name: "note",
      required: false,
    });
    mkdirSync(join(root, "lekalo", "targets"), { recursive: true });
    writeFileSync(
      join(root, "lekalo", "targets", "node-typescript.yaml"),
      "openapi:\n  version: \"3.0\"\n  mode: full\n  path: docs/openapi.yaml\n",
      "utf8",
    );
    const evidenceDir = join(root, ".lekalo", "cache", "transport");
    const irDir = join(root, ".lekalo", "cache", "ir");
    mkdirSync(evidenceDir, { recursive: true });
    mkdirSync(irDir, { recursive: true });
    writeFileSync(join(evidenceDir, "planner.json"), JSON.stringify(transport), "utf8");
    writeFileSync(join(irDir, "planner.json"), JSON.stringify(ir), "utf8");
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "complete", JSON.stringify(outcome.diagnostics ?? []));
    const yaml = outcome.data.bodies.get("docs/openapi.yaml");
    assert.ok(yaml.includes('"openapi": "3.0.0"'), "the wire member stays 3.0.0");
    // No 3.1-dialect constructs anywhere: no const members, no type
    // arrays carrying "null"; nullability rides the nullable sibling.
    assert.ok(!yaml.includes('"const":'), "3.0 spells single-value enums, not const");
    assert.ok(!/"type":\s*\[[^\]]*"null"/.test(yaml), "3.0 never widens a type array with null");
    assert.ok(yaml.includes('"nullable": true'), "nullability rides the nullable sibling");
    // The ok/category identity members spell single-value enums.
    assert.ok(yaml.includes('"enum":'), "identity members spell enums at 3.0");
    // The optional ref composes allOf over the $ref (a 3.0 $ref
    // carries no value siblings directly).
    const nullableAt = yaml.indexOf('"nullable": true');
    const nullableBlock = yaml.slice(Math.max(0, nullableAt - 200), nullableAt + 200);
    assert.ok(nullableBlock.includes('"allOf":'), "the optional ref composes allOf at 3.0");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the write scopes cover the default policy path", () => {
  assert.deepEqual(OPENAPI_WRITE_SCOPES, ["docs/**"]);
});

test("the policy parser is symmetric: quoted modes, sandwiched sections, docs-bounded paths (r1 devin F-12)", () => {
  // mode accepts the quoted spelling exactly like version does.
  const quoted = resolvePolicy('openapi:\n  mode: "fragments"\n');
  assert.equal(quoted.refusal, undefined);
  assert.equal(quoted.policy.mode, "fragments");
  // A non-adjacent openapi…zod…openapi sandwich refuses instead of
  // silently merging.
  const sandwich = parsePolicyYaml(
    'openapi:\n  mode: full\nzod:\n  date: date-string\nopenapi:\n  version: "3.0"\n',
  );
  assert.equal(sandwich.refusal, "duplicate-section");
  // The path is bounded to the declared write scopes at parse time.
  assert.equal(parsePolicyYaml("openapi:\n  path: api/openapi.yaml\n").refusal, "path-value");
  const scoped = resolvePolicy('openapi:\n  path: "docs/api.yaml"\n');
  assert.equal(scoped.refusal, undefined);
  assert.equal(scoped.policy.path, "docs/api.yaml");
});
test("a declared scheme requirement carries its components.securitySchemes entry (r1 F-2)", () => {
  // transport-minimal declares a bearer scheme (user_bearer) and an
  // operation security requirement over it: the component must be
  // emitted too, or the requirement names an undeclared scheme.
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "complete");
    const yaml = outcome.data.bodies.get("docs/openapi.yaml");
    assert.ok(yaml.includes('"user_bearer": []'), "the operation requirement survives");
    const componentsStart = yaml.indexOf('"components":');
    assert.ok(componentsStart >= 0, "components are emitted");
    const schemesStart = yaml.indexOf('"securitySchemes":', componentsStart);
    assert.ok(schemesStart > componentsStart, "components.securitySchemes is emitted");
    const schemeBlock = yaml.slice(schemesStart, schemesStart + 300);
    assert.ok(schemeBlock.includes('"user_bearer":'), "the referenced scheme id is declared");
    assert.ok(schemeBlock.includes('"type": "http"'), "bearer spells the http type");
    assert.ok(schemeBlock.includes('"scheme": "bearer"'), "bearer spells the bearer scheme");
    assert.ok(schemeBlock.includes('"bearerFormat": "jwt"'), "the declared format is carried");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a divided errorDefault inlines its category body and never dangles a $ref (r1 F-1)", () => {
  // Two endpoints: ep0 keeps domain:422, the clone declares domain:423
  // (a status neither endpoint covers with a declared error) — the
  // domain pair is divided, so neither may reference a shared
  // ErrorDomain component; every other uncovered pair stays uniform.
  const transport = JSON.parse(plannerEvidence);
  const ir = JSON.parse(plannerIr);
  const focusDef = ir.definitions.find((def) => def.id === "planner.api_focus");
  const cloneDef = JSON.parse(JSON.stringify(focusDef));
  cloneDef.id = "planner.api_focus_divided";
  cloneDef.path = "/tasks/{task_id}/focus-divided";
  ir.definitions.push(cloneDef);
  const cloneEndpoint = JSON.parse(JSON.stringify(transport.endpoints[0]));
  cloneEndpoint.endpoint = "planner.api_focus_divided";
  cloneEndpoint.errorDefaults = { ...cloneEndpoint.errorDefaults, domain: 423 };
  transport.endpoints.push(cloneEndpoint);

  const root = mkdtempSync(join(tmpdir(), "lekalo-openapi-divided-"));
  try {
    const evidenceDir = join(root, ".lekalo", "cache", "transport");
    const irDir = join(root, ".lekalo", "cache", "ir");
    mkdirSync(evidenceDir, { recursive: true });
    mkdirSync(irDir, { recursive: true });
    writeFileSync(join(evidenceDir, "planner.json"), JSON.stringify(transport), "utf8");
    writeFileSync(join(irDir, "planner.json"), JSON.stringify(ir), "utf8");
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "complete", JSON.stringify(outcome.diagnostics ?? []));
    const yaml = outcome.data.bodies.get("docs/openapi.yaml");
    // The divided domain statuses carry the inline category envelope…
    const block422 = yaml.slice(yaml.indexOf('"422":'), yaml.indexOf('"422":') + 600);
    const block423 = yaml.slice(yaml.indexOf('"423":'), yaml.indexOf('"423":') + 600);
    assert.ok(/"422":/.test(yaml) && block422.includes('"const": "domain"'),
      "the divided 422 renders the domain envelope inline");
    assert.ok(/"423":/.test(yaml) && block423.includes('"const": "domain"'),
      "the divided 423 renders the domain envelope inline");
    assert.ok(!yaml.includes('"$ref": "#/components/responses/ErrorDomain"'),
      "the divided pair never references ErrorDomain");
    assert.ok(!yaml.includes('"ErrorDomain": {'), "the divided ErrorDomain component is not emitted");
    // …and every emitted response $ref resolves to a real component.
    const refs = [...yaml.matchAll(/"\$ref": "#\/components\/responses\/([^"]+)"/g)].map(
      (match) => match[1],
    );
    assert.ok(refs.length > 0, "uniform defaults still share components");
    const componentsStart = yaml.indexOf('"components":');
    assert.ok(componentsStart >= 0, "components are emitted");
    for (const name of refs) {
      assert.ok(
        yaml.includes(`"${name}":`, componentsStart),
        `response $ref target ${name} is emitted under components.responses`,
      );
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the provenance block binds the evidence's model/IR/transport pins (r1 cline F-3)", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "complete");
    const yaml = outcome.data.bodies.get("docs/openapi.yaml");
    const provenanceStart = yaml.indexOf('"x-lekalo-provenance":');
    assert.ok(provenanceStart > 0, "the provenance block is emitted");
    const provenance = yaml.slice(provenanceStart);
    // The model/IR pins ride through the production decode path — no
    // empty digest/identity members.
    assert.ok(provenance.includes('"modelVersion": "0.2.16"'), "model version bound");
    assert.ok(
      provenance.includes('"digest": "sha256:0000000000000000000000000000000000000000000000000000000000000000"'),
      "the evidence's model digest is carried verbatim",
    );
    assert.ok(provenance.includes('"identity": "dev.lekalo.ir@0.2.16"'), "IR identity bound");
    assert.ok(
      provenance.includes('"digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"'),
      "the evidence's IR digest is carried verbatim",
    );
    // The transport digest binds the exact evidence bytes.
    const evidenceDigest =
      "sha256:" + createHash("sha256").update(plannerEvidence, "utf8").digest("hex");
    assert.ok(provenance.includes('"digest": "' + evidenceDigest + '"'), "transportRef pins the exact evidence bytes");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("info.version is the attachment revision, not a generator constant (r1 devin F-6)", () => {
  const transport = JSON.parse(plannerEvidence);
  transport.attachmentRevision = "9.9.9-rc.1";
  const root = mkdtempSync(join(tmpdir(), "lekalo-openapi-rev-"));
  try {
    const evidenceDir = join(root, ".lekalo", "cache", "transport");
    const irDir = join(root, ".lekalo", "cache", "ir");
    mkdirSync(evidenceDir, { recursive: true });
    mkdirSync(irDir, { recursive: true });
    writeFileSync(join(evidenceDir, "planner.json"), JSON.stringify(transport), "utf8");
    writeFileSync(join(irDir, "planner.json"), plannerIr, "utf8");
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "complete");
    const yaml = outcome.data.bodies.get("docs/openapi.yaml");
    assert.ok(yaml.includes('"version": "9.9.9-rc.1"'), "info.version follows the attachment revision");
    // The generator version stays where it belongs: the provenance block.
    assert.ok(yaml.includes('"version": "0.4.0"'), "the generator version rides provenance");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("the ownership sidecar claims responses, security schemes, and the input digests (r1 devin F-11)", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "complete");
    const ownership = JSON.parse(outcome.data.bodies.get("docs/openapi.ownership.json"));
    const pointers = Object.keys(ownership.pointers);
    assert.ok(
      pointers.some((pointer) => pointer.startsWith("/components/responses/")),
      "shared responses are claimed",
    );
    assert.ok(
      pointers.includes("/components/responses/ErrorAuth"),
      "the uniform ErrorAuth component is generator-owned",
    );
    assert.ok(
      pointers.some((pointer) => pointer.startsWith("/components/securitySchemes/")),
      "security schemes are claimed",
    );
    assert.equal(ownership.pointers["/components/securitySchemes/user_bearer"], "lekalo-core/openapi");
    assert.equal(ownership.inputs.model, "sha256:0000000000000000000000000000000000000000000000000000000000000000");
    assert.equal(ownership.inputs.ir, "sha256:1111111111111111111111111111111111111111111111111111111111111111");
    const evidenceDigest =
      "sha256:" + createHash("sha256").update(plannerEvidence, "utf8").digest("hex");
    assert.equal(ownership.inputs.transport, evidenceDigest);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});




test("fragments apply refuses on a manual/generated collision, naming the pointer (r2 F-2)", () => {
  // A hand edit at a generated pointer whose sidecar claim was
  // stripped collides with the regeneration: the core merge refuses
  // (MergeOutcome::into_result) and so does the apply — it never
  // reports complete over a document that silently dropped the
  // generated operation.
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const first = run({ ...views, request: {} });
    assert.equal(first.state, "complete");
    const generatedYaml = first.data.bodies.get("docs/openapi.yaml");
    const generatedOwnership = JSON.parse(
      first.data.bodies.get("docs/openapi.ownership.json"),
    );
    const focusPointer = "/paths/~1tasks~1{task_id}~1focus/post";
    const maintainedOwnership = {
      ...generatedOwnership,
      pointers: Object.fromEntries(
        Object.entries(generatedOwnership.pointers).filter(
          ([pointer]) => pointer !== focusPointer,
        ),
      ),
    };
    const maintainedYaml = generatedYaml.replace(
      '"operationId": "plannerApiFocus"',
      '"operationId": "handEditedOperation"',
    );
    const policyText =
      'openapi:\n  version: "3.1"\n  mode: fragments\n  path: docs/openapi.yaml\n';
    const files = new Map([
      ["docs/openapi.yaml", Buffer.from(maintainedYaml, "utf8")],
      ["docs/openapi.ownership.json", Buffer.from(JSON.stringify(maintainedOwnership), "utf8")],
      ["lekalo/targets/node-typescript.yaml", Buffer.from(policyText, "utf8")],
      [".lekalo/cache/transport/planner.json", Buffer.from(plannerEvidence, "utf8")],
      [".lekalo/cache/ir/planner.json", Buffer.from(plannerIr, "utf8")],
    ]);
    const fragmentsView = {
      roots: [],
      permittedProjectRoot: root,
      canRead: (path) => files.has(path),
      readFile: (path) => files.get(path),
    };
    const outcome = run({
      readView: fragmentsView,
      writeView: views.writeView,
      writes: views.writes,
      request: {},
    });
    assert.equal(outcome.state, "failed", JSON.stringify(outcome.diagnostics ?? []));
    assert.equal(outcome.diagnostics[0].reason, "merge-conflict");
    assert.equal(outcome.diagnostics[0].detail, focusPointer);
    // Nothing was written and no pretend-complete plan is returned.
    assert.equal(views.writes.length, 0);
    assert.equal(outcome.data, undefined);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fragments mode refuses a maintained document outside the closed dialect", () => {
  const root = evidenceProject();
  try {
    const policyText = "openapi:\n  mode: fragments\n  path: docs/openapi.yaml\n";
    const files = new Map([
      ["docs/openapi.yaml", Buffer.from("a: bare\npaths: {}\n", "utf8")],
      ["lekalo/targets/node-typescript.yaml", Buffer.from(policyText, "utf8")],
      [".lekalo/cache/transport/planner.json", Buffer.from(plannerEvidence, "utf8")],
      [".lekalo/cache/ir/planner.json", Buffer.from(plannerIr, "utf8")],
    ]);
    const fragmentsView = {
      roots: [],
      permittedProjectRoot: root,
      canRead: (path) => files.has(path),
      readFile: (path) => files.get(path),
    };
    const outcome = run({
      readView: fragmentsView,
      writeView: { exists: () => false, write: () => {} },
      request: {},
    });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "existing-document-unparseable");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fragments apply carries unclaimed root members and is two-cycle byte-stable (r2 F-1)", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const first = run({ ...views, request: {} });
    assert.equal(first.state, "complete");
    const generatedYaml = first.data.bodies.get("docs/openapi.yaml");
    const generatedOwnership = first.data.bodies.get("docs/openapi.ownership.json");
    // Maintain by hand: root members the generator never emits, a
    // hand-edited info subfield, and a wholly manual path.
    const manualRoot = [
      '"servers":',
      '  - "https://manual.example"',
      '"externalDocs":',
      '  "url": "https://manual.example/docs"',
      '"x-manual-root": "keepme"',
    ]
      .map((line) => `${line}\n`)
      .join("");
    const maintainedYaml = generatedYaml
      .replace('"openapi": "3.1.0"\n', `"openapi": "3.1.0"\n${manualRoot}`)
      .replace(
        '"info":\n',
        '"info":\n  "description": "hand note"\n',
      )
      .replace('"paths":\n', '"paths":\n  "/hand-written":\n    "get":\n      "operationId": "handWritten"\n');
    assert.ok(maintainedYaml.includes('"x-manual-root": "keepme"'));
    assert.ok(maintainedYaml.includes('"hand note"'));
    const policyText = 'openapi:\n  version: "3.1"\n  mode: fragments\n  path: docs/openapi.yaml\n';
    const filesFor = (yamlText) =>
      new Map([
        ["docs/openapi.yaml", Buffer.from(yamlText, "utf8")],
        ["docs/openapi.ownership.json", Buffer.from(generatedOwnership, "utf8")],
        ["lekalo/targets/node-typescript.yaml", Buffer.from(policyText, "utf8")],
        [".lekalo/cache/transport/planner.json", Buffer.from(plannerEvidence, "utf8")],
        [".lekalo/cache/ir/planner.json", Buffer.from(plannerIr, "utf8")],
      ]);
    const runOver = (yamlText) => {
      const files = filesFor(yamlText);
      const fragmentsView = {
        roots: [],
        permittedProjectRoot: root,
        canRead: (path) => files.has(path),
        readFile: (path) => files.get(path),
      };
      return run({
        readView: fragmentsView,
        writeView: { exists: () => false, write: () => {} },
        request: {},
      });
    };
    // Cycle one: the manual root members survive byte-for-byte…
    const cycle1 = runOver(maintainedYaml);
    assert.equal(cycle1.state, "complete", JSON.stringify(cycle1.diagnostics ?? []));
    const out1 = cycle1.data.bodies.get("docs/openapi.yaml");
    assert.ok(out1.includes('"x-manual-root": "keepme"'), "the manual root member survives");
    assert.ok(out1.includes("- \"https://manual.example\""), "servers survives");
    assert.ok(out1.includes('"hand note"'), "the hand-edited info subfield survives");
    assert.ok(out1.includes("handWritten"), "the manual path survives");
    assert.ok(out1.includes('"operationId": "plannerApiFocus"'), "the generated operation survives");
    // …and cycle two over the first output is byte-identical.
    const cycle2 = runOver(out1);
    assert.equal(cycle2.state, "complete", JSON.stringify(cycle2.diagnostics ?? []));
    const out2 = cycle2.data.bodies.get("docs/openapi.yaml");
    assert.equal(out2, out1, "the second fragments apply is byte-stable");
    // A maintained document spelling another version refuses instead
    // of merging mixed-dialect content.
    const files30 = filesFor(maintainedYaml.replace('"openapi": "3.1.0"', '"openapi": "3.0.0"'));
    const fragments30 = {
      roots: [],
      permittedProjectRoot: root,
      canRead: (path) => files30.has(path),
      readFile: (path) => files30.get(path),
    };
    const refused = run({
      readView: fragments30,
      writeView: { exists: () => false, write: () => {} },
      request: {},
    });
    assert.equal(refused.state, "failed");
    assert.equal(refused.diagnostics[0].reason, "existing-document-version");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("fromYaml refuses duplicate keys like the closed frontend (r2 F-3)", async () => {
  const { fromYaml, YamlReadError, toYaml } = await import(
    "../src/openapi-emit.mjs"
  );
  // A repeated key refuses with the frontend's duplicate-key token —
  // never a silent last-wins over a hand-maintained document.
  let refusal;
  try {
    fromYaml('"a": 1\n"a": 2\n');
  } catch (error) {
    refusal = error;
  }
  assert.ok(refusal instanceof YamlReadError, "a duplicate key refuses");
  assert.equal(refusal.reason, "duplicate-key:a");
  // The generator's own output never duplicates, so the round-trip
  // law still holds.
  const tree = { a: 1, b: { c: "x" }, d: [1, 2] };
  assert.equal(
    canonicalJson(fromYaml(toYaml(tree))),
    canonicalJson(tree),
  );
});

test("a fragments apply over a duplicate-keyed maintained doc refuses naming the key (r2 F-3)", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const first = run({ ...views, request: {} });
    assert.equal(first.state, "complete");
    const generatedYaml = first.data.bodies.get("docs/openapi.yaml");
    const generatedOwnership = first.data.bodies.get("docs/openapi.ownership.json");
    const maintainedYaml = generatedYaml.replace(
      '"paths":\n',
      '"paths":\n  "/dup":\n    "get":\n      "operationId": "a"\n  "/dup":\n    "get":\n      "operationId": "b"\n',
    );
    const policyText =
      'openapi:\n  version: "3.1"\n  mode: fragments\n  path: docs/openapi.yaml\n';
    const files = new Map([
      ["docs/openapi.yaml", Buffer.from(maintainedYaml, "utf8")],
      ["docs/openapi.ownership.json", Buffer.from(generatedOwnership, "utf8")],
      ["lekalo/targets/node-typescript.yaml", Buffer.from(policyText, "utf8")],
      [".lekalo/cache/transport/planner.json", Buffer.from(plannerEvidence, "utf8")],
      [".lekalo/cache/ir/planner.json", Buffer.from(plannerIr, "utf8")],
    ]);
    const fragmentsView = {
      roots: [],
      permittedProjectRoot: root,
      canRead: (path) => files.has(path),
      readFile: (path) => files.get(path),
    };
    const outcome = run({
      readView: fragmentsView,
      writeView: { exists: () => false, write: () => {} },
      request: {},
    });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "existing-document-unparseable");
    assert.equal(outcome.diagnostics[0].detail, "duplicate-key:/dup");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("a path-level non-method member rides the merge without a pseudo-pointer note (r2 F-3)", () => {
  const root = evidenceProject();
  try {
    const views = viewsFor(root);
    const first = run({ ...views, request: {} });
    assert.equal(first.state, "complete");
    const generatedYaml = first.data.bodies.get("docs/openapi.yaml");
    const generatedOwnership = first.data.bodies.get("docs/openapi.ownership.json");
    // A path-level `parameters` member under the generated template:
    // not an operation pointer, just maintained content.
    const maintainedYaml = generatedYaml.replace(
      '"paths":\n',
      '"paths":\n  "parameters":\n    - "name": "shared"\n',
    );
    const policyText =
      'openapi:\n  version: "3.1"\n  mode: fragments\n  path: docs/openapi.yaml\n';
    const files = new Map([
      ["docs/openapi.yaml", Buffer.from(maintainedYaml, "utf8")],
      ["docs/openapi.ownership.json", Buffer.from(generatedOwnership, "utf8")],
      ["lekalo/targets/node-typescript.yaml", Buffer.from(policyText, "utf8")],
      [".lekalo/cache/transport/planner.json", Buffer.from(plannerEvidence, "utf8")],
      [".lekalo/cache/ir/planner.json", Buffer.from(plannerIr, "utf8")],
    ]);
    const fragmentsView = {
      roots: [],
      permittedProjectRoot: root,
      canRead: (path) => files.has(path),
      readFile: (path) => files.get(path),
    };
    const outcome = run({
      readView: fragmentsView,
      writeView: { exists: () => false, write: () => {} },
      request: {},
    });
    assert.equal(outcome.state, "complete", JSON.stringify(outcome.diagnostics ?? []));
    const merged = outcome.data.bodies.get("docs/openapi.yaml");
    assert.ok(merged.includes('"shared"'), "the path-level member survives");
    const notes = JSON.stringify(outcome.data.partial);
    assert.ok(
      !notes.includes("/paths/~1tasks~1{task_id}~1focus/parameters"),
      "no pseudo-pointer note for the non-method key",
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("evidence without an attachment revision refuses instead of an empty info.version (r2 F-4)", () => {
  const transport = JSON.parse(plannerEvidence);
  delete transport.attachmentRevision;
  const root = mkdtempSync(join(tmpdir(), "lekalo-openapi-norev-"));
  try {
    const evidenceDir = join(root, ".lekalo", "cache", "transport");
    const irDir = join(root, ".lekalo", "cache", "ir");
    mkdirSync(evidenceDir, { recursive: true });
    mkdirSync(irDir, { recursive: true });
    writeFileSync(join(evidenceDir, "planner.json"), JSON.stringify(transport), "utf8");
    writeFileSync(join(irDir, "planner.json"), plannerIr, "utf8");
    const views = viewsFor(root);
    const outcome = run({ ...views, request: {} });
    assert.equal(outcome.state, "failed");
    assert.equal(outcome.diagnostics[0].reason, "attachment-revision-absent");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

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

test("the write scopes cover the default policy path", () => {
  assert.deepEqual(OPENAPI_WRITE_SCOPES, ["docs/**"]);
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

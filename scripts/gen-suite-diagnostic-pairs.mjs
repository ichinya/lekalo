#!/usr/bin/env node
// Suite-v1 diagnostic pair generator (issue #90).
//
// Writes the paired trigger/non-trigger project fixtures under
// tests/fixtures/suite/v1/diagnostics/<rule>/ for every semantic.*
// validator rule. Each pair is a minimal, self-contained project: the
// trigger project names exactly one registered rule in expect.json and
// the non-trigger control differs by the minimum edit that resolves the
// rule. The loader rejects YAML flow style, so documents are emitted as
// canonical JSON (JSON documents are accepted YAML twins).
//
// The generator is deterministic and refuses to overwrite an existing
// pair unless --force is given. Run it through the reviewed update flow
// (scripts/update-golden-suite.mjs), never ad hoc in CI.
//
// Usage: node scripts/gen-suite-diagnostic-pairs.mjs [--force]

import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const force = process.argv.includes("--force");
const out = join(root, "tests/fixtures/suite/v1/diagnostics");

const schema = "0.2.16";

const doc = (definitions) =>
  `${JSON.stringify({ schema_version: schema, definitions }, null, 2)}\n`;

const scalar = (id, base, extra = {}) => ({
  id,
  kind: "scalar",
  version: 1,
  base,
  ...extra,
});
const entity = (id, fields, identity, extra = {}) => ({
  id,
  kind: "entity",
  version: 1,
  fields: fields.map(([name, ref]) => ({
    name,
    type: { ref },
    required: true,
  })),
  identity,
  ...extra,
});
const command = (id, input, extra = {}) => ({
  id,
  kind: "command",
  version: 1,
  input: input.map(([name, ref]) => ({ name, type: { ref }, required: true })),
  ...extra,
});
const query = (id, reads, returns, extra = {}) => ({
  id,
  kind: "query",
  version: 1,
  ...(reads.length > 0 ? { reads } : {}),
  returns: { ref: returns },
  ...extra,
});
const policy = (id, appliesTo, extra = {}) => ({
  id,
  kind: "policy",
  version: 1,
  applies_to: appliesTo,
  decision: "allow",
  ...extra,
});
const effect = (id, operation, entityRef, emits, extra = {}) => ({
  id,
  kind: "effect",
  version: 1,
  operation,
  entity: entityRef,
  ...(emits ? { emits } : {}),
  ...extra,
});
const event = (id, payload, extra = {}) => ({
  id,
  kind: "event",
  version: 1,
  payload: payload.map(([name, ref]) => ({ name, type: { ref }, required: true })),
  ...extra,
});
const endpoint = (id, invokes, method, path, extra = {}) => ({
  id,
  kind: "endpoint",
  version: 1,
  invokes,
  method,
  path,
  ...extra,
});
const scenario = (id, covers, summary, extra = {}) => ({
  id,
  kind: "scenario",
  version: 1,
  covers,
  summary,
  ...extra,
});
const valueObject = (id, fields, extra = {}) => ({
  id,
  kind: "value-object",
  version: 1,
  fields: fields.map(([name, ref]) => ({
    name,
    type: { ref },
    required: true,
  })),
  ...extra,
});

// The closed pair table: [rule, slug, triggerDocs, nonTriggerDocs].
// Each side is a map of module-relative document names to definitions;
// "planner/entities.yaml" is the default document.
const pairs = [
  {
    rule: "semantic.type-ref-unresolved",
    slug: "type-ref-unresolved",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"], ["title", "planner.ghost"]], ["task_id"]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["task_id", "planner.task_id"], ["title", "planner.text"]], ["task_id"]),
      ],
    },
  },
  {
    rule: "semantic.type-ref-kind-mismatch",
    slug: "type-ref-kind-mismatch",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["task_id", "planner.task_id"], ["title", "planner.create_task"]], ["task_id"]),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["task_id", "planner.task_id"], ["title", "planner.text"]], ["task_id"]),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]]),
      ],
    },
  },
  {
    rule: "semantic.command-effect-unresolved",
    slug: "command-effect-unresolved",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]], { effects: ["planner.ghost_effect"] }),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/events.yaml": [
        event("planner.task_created", [["task_id", "planner.task_id"]]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", ["planner.task_created"]),
        command("planner.create_task", [["title", "planner.text"]], { effects: ["planner.effect_create"] }),
      ],
    },
  },
  {
    rule: "semantic.command-effect-kind-mismatch",
    slug: "command-effect-kind-mismatch",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]], { effects: ["planner.task"] }),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/events.yaml": [
        event("planner.task_created", [["task_id", "planner.task_id"]]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", ["planner.task_created"]),
        command("planner.create_task", [["title", "planner.text"]], { effects: ["planner.effect_create"] }),
      ],
    },
  },
  {
    rule: "semantic.query-read-unresolved",
    slug: "query-read-unresolved",
    trigger: {
      "planner/queries.yaml": [
        query("planner.get_task", ["planner.ghost"], "planner.ghost"),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/queries.yaml": [
        query("planner.get_task", ["planner.task"], "planner.task"),
      ],
    },
  },
  {
    rule: "semantic.query-read-kind-mismatch",
    slug: "query-read-kind-mismatch",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/queries.yaml": [
        query("planner.list_text", ["planner.text"], "planner.text"),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/queries.yaml": [
        query("planner.get_task", ["planner.task"], "planner.task"),
      ],
    },
  },
  {
    rule: "semantic.policy-operation-unresolved",
    slug: "policy-operation-unresolved",
    trigger: {
      "planner/policies.yaml": [
        policy("planner.allow_ghost", ["planner.ghost"]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]]),
      ],
      "planner/policies.yaml": [
        policy("planner.allow_create", ["planner.create_task"]),
      ],
    },
  },
  {
    rule: "semantic.policy-operation-kind-mismatch",
    slug: "policy-operation-kind-mismatch",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/policies.yaml": [
        policy("planner.allow_text", ["planner.text"]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]]),
      ],
      "planner/policies.yaml": [
        policy("planner.allow_create", ["planner.create_task"]),
      ],
    },
  },
  {
    rule: "semantic.effect-resource-unresolved",
    slug: "effect-resource-unresolved",
    trigger: {
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.ghost", null),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", null),
      ],
    },
  },
  {
    rule: "semantic.effect-resource-kind-mismatch",
    slug: "effect-resource-kind-mismatch",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_touch", "create", "planner.text", null),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", null),
      ],
    },
  },
  {
    rule: "semantic.effect-emits-unresolved",
    slug: "effect-emits-unresolved",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", ["planner.ghost_event"]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/events.yaml": [
        event("planner.task_created", [["task_id", "planner.task_id"]]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", ["planner.task_created"]),
      ],
    },
  },
  {
    rule: "semantic.effect-emits-kind-mismatch",
    slug: "effect-emits-kind-mismatch",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", ["planner.task"]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/events.yaml": [
        event("planner.task_created", [["task_id", "planner.task_id"]]),
      ],
      "planner/commands.yaml": [
        effect("planner.effect_create", "create", "planner.task", ["planner.task_created"]),
      ],
    },
  },
  {
    rule: "semantic.endpoint-operation-unresolved",
    slug: "endpoint-operation-unresolved",
    trigger: {
      "planner/bindings.yaml": [
        endpoint("planner.endpoint_create", "planner.ghost", "POST", "/tasks"),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]]),
      ],
      "planner/bindings.yaml": [
        endpoint("planner.endpoint_create", "planner.create_task", "POST", "/tasks"),
      ],
    },
  },
  {
    rule: "semantic.endpoint-operation-kind-mismatch",
    slug: "endpoint-operation-kind-mismatch",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/bindings.yaml": [
        endpoint("planner.endpoint_touch", "planner.text", "POST", "/text"),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]]),
      ],
      "planner/bindings.yaml": [
        endpoint("planner.endpoint_create", "planner.create_task", "POST", "/tasks"),
      ],
    },
  },
  {
    rule: "semantic.scenario-operation-unresolved",
    slug: "scenario-operation-unresolved",
    trigger: {
      "planner/scenarios.yaml": [
        scenario("planner.scenario_create", ["planner.ghost"], "Create one task."),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
      ],
      "planner/commands.yaml": [
        command("planner.create_task", [["title", "planner.text"]]),
      ],
      "planner/scenarios.yaml": [
        scenario("planner.scenario_create", ["planner.create_task"], "Create one task."),
      ],
    },
  },
  {
    rule: "semantic.entity-identity-field-missing",
    slug: "entity-identity-field-missing",
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["ghost_field", "planner.task_id"], ["title", "planner.text"]], ["task_id"]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.text", "string"),
        entity("planner.task", [["task_id", "planner.task_id"], ["title", "planner.text"]], ["task_id"]),
      ],
    },
  },
  {
    rule: "semantic.type-recursion",
    slug: "type-recursion",
    trigger: {
      "planner/entities.yaml": [
        valueObject("planner.loop_a", [["other", "planner.loop_b"]]),
        valueObject("planner.loop_b", [["back", "planner.loop_a"]]),
      ],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.text", "string"),
        valueObject("planner.loop_a", [["other", "planner.loop_b"]]),
        valueObject("planner.loop_b", [["back", "planner.text"]]),
      ],
    },
  },
  {
    rule: "semantic.visibility-boundary-violation",
    slug: "visibility-boundary-violation",
    modules: true,
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        scalar("planner.report", "string"),
      ],
      "planner/queries.yaml": [
        query("planner.get_log", ["audit.log"], "planner.report"),
      ],
      "audit/entities.yaml": [
        entity("audit.log", [["line", "audit.entry"]], ["line"], { visibility: "module" }),
        scalar("audit.entry", "string"),
      ],
      "audit/queries.yaml": [],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/queries.yaml": [
        query("planner.get_task", ["planner.task"], "planner.task"),
      ],
      "audit/entities.yaml": [
        entity("audit.log", [["line", "audit.entry"]], ["line"], { visibility: "module" }),
        scalar("audit.entry", "string"),
      ],
      "audit/queries.yaml": [],
    },
  },
  {
    rule: "semantic.public-output-private-type",
    slug: "public-output-private-type",
    modules: true,
    trigger: {
      "planner/entities.yaml": [
        scalar("planner.report", "string"),
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/queries.yaml": [
        query("planner.get_log", ["planner.task"], "audit.log"),
      ],
      "audit/entities.yaml": [
        entity("audit.log", [["line", "audit.entry"]], ["line"], { visibility: "module" }),
        scalar("audit.entry", "string"),
      ],
      "audit/queries.yaml": [],
    },
    nonTrigger: {
      "planner/entities.yaml": [
        scalar("planner.report", "string"),
        scalar("planner.task_id", "uuid"),
        entity("planner.task", [["task_id", "planner.task_id"]], ["task_id"]),
      ],
      "planner/queries.yaml": [
        query("planner.get_task", ["planner.task"], "planner.report"),
      ],
      "audit/entities.yaml": [
        entity("audit.log", [["line", "audit.entry"]], ["line"], { visibility: "module" }),
        scalar("audit.entry", "string"),
      ],
      "audit/queries.yaml": [],
    },
  },
];

function writeProject(dir, docs) {
  const modules = new Set(
    Object.keys(docs).map((rel) => rel.split("/")[0]),
  );
  mkdirSync(join(dir, "lekalo", "modules"), { recursive: true });
  writeFileSync(
    join(dir, "lekalo", "project.yaml"),
    doc([{ id: "planner", kind: "project", version: 1, description: "Suite v1 diagnostic pair" }]),
  );
  for (const mod of modules) {
    const multiModule = modules.size > 1;
    const imports = multiModule && mod === "planner" ? ["audit"] : undefined;
    mkdirSync(join(dir, "lekalo", "modules", mod), { recursive: true });
    writeFileSync(
      join(dir, "lekalo", "modules", mod, "module.yaml"),
      doc(
        imports
          ? [{ id: mod, kind: "module", version: 1, imports }]
          : [{ id: mod, kind: "module", version: 1 }],
      ),
    );
    // Empty documents stay as empty definition lists; only files named
    // in the pair table are written.
  }
  for (const [rel, definitions] of Object.entries(docs)) {
    if (definitions.length === 0) continue;
    const [mod, name] = rel.split("/");
    writeFileSync(join(dir, "lekalo", "modules", mod, name), doc(definitions));
  }
}

let written = 0;
let skipped = 0;
for (const pair of pairs) {
  const dir = join(out, pair.slug);
  if (existsSync(dir) && !force) {
    skipped += 1;
    continue;
  }
  rmSync(dir, { recursive: true, force: true });
  writeProject(join(dir, "trigger"), pair.trigger);
  writeProject(join(dir, "non-trigger"), pair.nonTrigger);
  writeFileSync(
    join(dir, "expect.json"),
    `${JSON.stringify({ rule: pair.rule, pair: `${pair.slug}.pair` }, null, 2)}\n`,
  );
  writeFileSync(
    join(dir, "fixture.json"),
    `${JSON.stringify({
      fixtureSchema: "dev.lekalo.fixture@1.0.0",
      caseId: `diagnostic.${pair.slug}.pair`,
      revision: 1,
      class: "diagnostic-pair",
      origin: "synthetic",
      issue: "90",
      purpose: `F06 paired witness for ${pair.rule}: trigger names the rule, non-trigger validates clean.`,
      runner: "cli-validate",
      contractPins: [
        { identity: "dev.lekalo.model@0.2.16", role: "model contract" },
        { identity: "dev.lekalo.diagnostic-registry@0.4.0", role: "diagnostics registry" },
      ],
      inputs: [
        { role: "project", path: `tests/fixtures/suite/v1/diagnostics/${pair.slug}/trigger` },
        { role: "project", path: `tests/fixtures/suite/v1/diagnostics/${pair.slug}/non-trigger` },
      ],
      expectation: {
        status: "invalid",
        exit: 1,
        reasonCodes: [pair.rule],
        witnessRule: pair.rule,
        witnessPolarity: "trigger",
      },
      timeoutMs: 60000,
      determinism: {
        clockPolicy: "not-applicable-read-only",
        idPolicy: "not-applicable-read-only",
        locale: "C",
        newlineMode: "lf-only",
        pathMode: "logical-repository-relative",
      },
      limits: { maxOutputBytes: 65536, maxFiles: 32 },
      updateRecipe: "update-golden-case",
    }, null, 2)}\n`,
  );
  written += 1;
}

process.stdout.write(
  `${JSON.stringify({ ok: true, written, skipped, generator: "gen-suite-diagnostic-pairs" })}\n`,
);

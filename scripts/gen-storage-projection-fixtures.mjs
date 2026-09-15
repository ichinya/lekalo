#!/usr/bin/env node
// Issue #65 fixture generator: deterministically emits the committed
// storage-projection fixtures from one declarative description.
//
// The valid golden is emitted in canonical form (compact JSON,
// byte-sorted keys, no trailing LF) so the Rust canonical writer and
// the independent Node canonical-form check prove the same bytes. The
// invalid vectors are emitted as (document, expect) pairs; the expect
// file names the exact registered rule and fixed detail token that the
// typed normalizer must produce. Run from the repository root:
//
//     node scripts/gen-storage-projection-fixtures.mjs
//
// The script never reads the network and writes only into
// tests/fixtures/storage-projection/.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "tests/fixtures/storage-projection");

const digest = (seed) =>
  `sha256:${String(seed).padStart(2, "0").repeat(32).slice(0, 64)}`;

const DIGEST_MODEL = digest(1);
const DIGEST_IR = digest(2);
const DIGEST_SCENARIO_A = digest(3);
const DIGEST_SCENARIO_B = digest(4);

const field = (name, type, visibility, required) => ({
  field: name,
  ...(required === undefined ? {} : { required }),
  type,
  visibility,
});

const uuid = (visibility, required = true) =>
  field("id", { name: "uuid" }, visibility, required);

const entities = [
  {
    description: "A customer-visible note attached to a planner task.",
    entity: "planner.comment",
    entityKey: "comment",
    fields: [
      field("body", { name: "text" }, "internal", true),
      uuid("internal"),
    ],
    visibility: "internal",
  },
  {
    description: "One tracked work session on a planner task.",
    entity: "planner.focus_session",
    entityKey: "focus_session",
    fields: [
      field("minutes", { name: "integer" }, "internal", true),
      uuid("internal"),
      field("started_at", { name: "timestamp" }, "internal", true),
    ],
    invariants: ["planner-invariants/session-minutes-positive"],
    visibility: "internal",
  },
  {
    description: "A remote issue owned by an external provider.",
    entity: "jira.issue",
    entityKey: "jira_issue",
    external: true,
    fields: [field("external_key", { length: 255, name: "string" }, "internal", true)],
    visibility: "internal",
  },
  {
    description: "A reusable label attached to planner tasks.",
    entity: "planner.tag",
    entityKey: "tag",
    fields: [uuid("public"), field("label", { length: 64, name: "string" }, "public", true)],
    visibility: "public",
  },
  {
    aggregateRoot: true,
    description: "The planner work task and aggregate root.",
    entity: "planner.focus_task",
    entityKey: "task",
    fields: [
      field("due_date", { name: "date" }, "public"),
      uuid("public"),
      field("note", { name: "text" }, "internal"),
      field("status", { length: 16, name: "string" }, "internal", true),
      field("title", { length: 200, name: "string" }, "public", true),
    ],
    stateSpaces: ["planner-state/task-lifecycle"],
    visibility: "public",
  },
  {
    aggregateOwner: "task",
    description: "Optional long-form description of one task.",
    entity: "planner.task_detail",
    entityKey: "task_detail",
    fields: [
      field("description", { name: "text" }, "public"),
      uuid("internal"),
    ],
    visibility: "internal",
  },
  {
    aggregateOwner: "task",
    description: "One provider identity linked to a planner task.",
    entity: "planner.task_external_link",
    entityKey: "task_external_link",
    fields: [
      field("external_key", { length: 255, name: "string" }, "public", true),
      uuid("public"),
      field("provider", { length: 64, name: "string" }, "public", true),
      field("url", { name: "text" }, "public"),
    ],
    invariants: ["planner-invariants/provider-link-uniqueness"],
    visibility: "public",
  },
];

const scenario = (id) => ({
  irDigest: DIGEST_SCENARIO_A,
  scenarioId: id,
  scenarioVersion: "1.0.0",
});

const relations = [
  {
    constraints: ["planner-invariants/comment-target-resolvable"],
    deleteBehavior: "cascade",
    description: "A comment attaches to exactly one varying target.",
    kind: "polymorphic",
    max: 1000000,
    min: 1,
    owner: "comment",
    relationId: "planner.relation.comment_target",
    scenarios: [scenario("planner.scenario.comment_thread_moves")],
    target: "task",
  },
  {
    deleteBehavior: "detach",
    description: "The remote identity a linked provider row points at.",
    kind: "external_reference",
    max: 1000000,
    min: 0,
    owner: "task_external_link",
    relationId: "planner.relation.link_provider",
    scenarios: [scenario("planner.scenario.provider_outage_readonly")],
    target: "jira_issue",
  },
  {
    deleteBehavior: "cascade",
    description: "At most one long-form description per task.",
    foreignKey: "task_id",
    kind: "one_to_one",
    max: 1,
    min: 1,
    owner: "task",
    relationId: "planner.relation.task_detail_record",
    constraints: ["planner-invariants/task-description-optional"],
    target: "task_detail",
  },
  {
    deleteBehavior: "cascade",
    description: "Task external links: zero or more provider links per task.",
    foreignKey: "task_id",
    kind: "aggregate_child",
    max: 1000000,
    min: 0,
    owner: "task",
    relationId: "planner.relation.task_external_links",
    scenarios: [
      {
        irDigest: DIGEST_SCENARIO_B,
        scenarioId: "planner.scenario.external_link_roundtrip",
        scenarioVersion: "1.0.0",
      },
    ],
    target: "task_external_link",
  },
  {
    deleteBehavior: "restrict",
    description: "Sessions recorded against one task.",
    foreignKey: "focus_task_id",
    kind: "one_to_many",
    max: 1000000,
    min: 0,
    owner: "task",
    relationId: "planner.relation.task_focus_sessions",
    constraints: ["planner-invariants/session-minutes-positive"],
    target: "focus_session",
  },
  {
    deleteBehavior: "detach",
    description: "An optional parent task within one aggregate.",
    foreignKey: "parent_task_id",
    kind: "optional_reference",
    max: 1,
    min: 0,
    owner: "task",
    relationId: "planner.relation.task_parent",
    scenarios: [scenario("planner.scenario.subtask_reparent")],
    target: "task",
  },
  {
    deleteBehavior: "detach",
    description: "Tags attach to many tasks through one join table.",
    kind: "many_to_many",
    max: 1000000,
    min: 0,
    owner: "task",
    relationId: "planner.relation.task_tags",
    scenarios: [scenario("planner.scenario.tag_retag")],
    target: "tag",
  },
];

const table = (entity, name, rest) => ({
  entity,
  primaryKey: ["id"],
  table: name,
  ...rest,
});

const migrations = (records) => records;

const projections = [
  {
    joins: [
      {
        columns: ["task_id", "tag_id"],
        relation: "planner.relation.task_tags",
        table: "task_tag",
        uniquePair: true,
      },
    ],
    migrationHistory: migrations([
      {
        migrationId: "planner-migrations/baseline",
        risk: "none",
        tables: ["comment", "focus_session", "tag", "task", "task_detail", "task_external_link", "task_tag"],
      },
      {
        migrationId: "planner-migrations/add-soft-deletes",
        risk: "backfill_required",
        tables: ["task"],
      },
    ]),
    namespace: "laravel",
    polymorphics: [
      {
        keyColumn: "target_id",
        relation: "planner.relation.comment_target",
        typeColumn: "target_type",
      },
    ],
    tables: [
      table("comment", "comment", {}),
      table("focus_session", "focus_session", {
        generatedColumns: [{ kind: "sequence", name: "session_no" }],
      }),
      table("tag", "tag", {
        indexes: [{ columns: ["label"], unique: true }],
      }),
      table("task", "task", {
        indexes: [
          { columns: ["due_date"], unique: false },
          { columns: ["tenant_id"], name: "idx_task_tenant", unique: false },
        ],
        softDelete: { column: "deleted_at" },
        technicalColumns: [
          { name: "remember_token", purpose: "session continuity token", type: "string" },
        ],
        tenantKey: { column: "tenant_id", type: "uuid" },
        timestamps: { createdAt: "created_at", updatedAt: "updated_at" },
      }),
      table("task_detail", "task_detail", {}),
      table("task_external_link", "task_external_link", {
        indexes: [
          {
            columns: ["provider", "external_key"],
            name: "uq_external_identity",
            unique: true,
          },
        ],
      }),
    ],
  },
  {
    joins: [
      {
        columns: ["task_id", "tag_id"],
        relation: "planner.relation.task_tags",
        table: "task_tag",
        uniquePair: true,
      },
    ],
    migrationHistory: migrations([
      {
        migrationId: "planner-migrations/baseline",
        risk: "backfill_required",
        tables: ["comment", "focus_session", "tag", "task", "task_detail", "task_external_link", "task_tag"],
      },
      {
        migrationId: "planner-migrations/archive-legacy-notes",
        risk: "destructive",
        tables: ["task"],
      },
    ]),
    namespace: "postgres",
    polymorphics: [
      {
        keyColumn: "target_id",
        relation: "planner.relation.comment_target",
        typeColumn: "target_type",
      },
    ],
    tables: [
      table("comment", "comment", {}),
      table("focus_session", "focus_session", {
        generatedColumns: [{ kind: "identity", name: "session_no" }],
      }),
      table("tag", "tag", {
        indexes: [{ columns: ["label"], unique: true }],
      }),
      table("task", "task", {
        indexes: [
          { columns: ["due_date"], unique: false },
          { columns: ["tenant_id"], name: "idx_task_tenant", unique: false },
        ],
        softDelete: { column: "deleted_at" },
        technicalColumns: [
          { name: "row_etag", nullable: false, purpose: "optimistic concurrency token", type: "bytea" },
        ],
        tenantKey: { column: "tenant_id", type: "uuid" },
        timestamps: { createdAt: "created_at", updatedAt: "updated_at" },
      }),
      table("task_detail", "task_detail", {}),
      table("task_external_link", "task_external_link", {
        indexes: [
          {
            columns: ["provider", "external_key"],
            name: "uq_external_identity",
            unique: true,
          },
        ],
      }),
    ],
  },
];

const validAttachment = () => ({
  attachmentRevision: "1.0.0",
  entities,
  identity: "dev.lekalo.storage-projection@1.0.0",
  irRef: { digest: DIGEST_IR, identity: "dev.lekalo.ir@0.1.0" },
  modelRef: { digest: DIGEST_MODEL, modelVersion: "1.0.0" },
  projectId: "planner",
  projections,
  relations,
  schemaVersion: "lekalo/storage-projection/v1.0.0",
});

// --- canonical form --------------------------------------------------------

// Normalize the array orders the wire contract treats as set-like,
// mirroring the Rust canonical writer exactly. Declared behavioral
// orders (primary keys, join column pairs, the migration history)
// are preserved.
const byKey = (key) => (left, right) =>
  Buffer.compare(Buffer.from(left[key]), Buffer.from(right[key]));
const byName = byKey("name");
const byRelation = byKey("relation");
const byEntity = byKey("entity");
const normalize = (attachment) => {
  const clone = JSON.parse(JSON.stringify(attachment));
  clone.entities.sort(byKey("entityKey"));
  clone.relations.sort(byKey("relationId"));
  clone.projections.sort(byKey("namespace"));
  for (const entity of clone.entities) {
    entity.fields.sort(byKey("field"));
    entity.invariants?.sort();
    entity.stateSpaces?.sort();
  }
  for (const relation of clone.relations) {
    relation.scenarios?.sort(byKey("scenarioId"));
    relation.constraints?.sort();
  }
  for (const projection of clone.projections) {
    projection.tables.sort(byEntity);
    projection.joins?.sort(byRelation);
    projection.polymorphics?.sort(byRelation);
    for (const table of projection.tables) {
      table.technicalColumns?.sort(byName);
      table.generatedColumns?.sort(byName);
      table.indexes?.sort((left, right) => {
        const leftKey = [left.name ?? "", ...left.columns].join("\u0000");
        const rightKey = [right.name ?? "", ...right.columns].join("\u0000");
        return Buffer.compare(Buffer.from(leftKey), Buffer.from(rightKey));
      });
    }
    for (const migration of projection.migrationHistory ?? []) {
      migration.tables.sort();
    }
  }
  return clone;
};

const canonical = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    const body = Object.keys(value)
      .filter((key) => value[key] !== undefined)
      .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)))
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`);
    return `{${body.join(",")}}`;
  }
  return JSON.stringify(value);
};

const write = (relative, document, suffix = "") => {
  if (document) document = normalize(document);
  const target = join(outDir, relative);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, suffix === "" ? canonical(document) : `${suffix}\n`);
  return target;
};

const expect = (relative, rule, detail, subject) =>
  write(
    relative,
    null,
    JSON.stringify(
      subject === undefined ? { detail, rule } : { detail, rule, subject },
    ),
  );

// --- base patching ---------------------------------------------------------

function patch(value, edits) {
  const clone = JSON.parse(JSON.stringify(value));
  for (const [path, update] of Object.entries(edits)) {
    const keys = path.split("/");
    let node = clone;
    while (keys.length > 1) node = node[keys.shift()];
    if (update === undefined) delete node[keys[0]];
    else node[keys[0]] = update;
  }
  return clone;
}

function findEntity(attachment, key) {
  return attachment.entities.find((entity) => entity.entityKey === key);
}

function findRelation(attachment, id) {
  return attachment.relations.find((relation) => relation.relationId === id);
}

function findProjection(attachment, namespace) {
  return attachment.projections.find(
    (projection) => projection.namespace === namespace,
  );
}

// --- valid golden ----------------------------------------------------------

const base = validAttachment();
write("valid/planner-storage.json", base);

// --- diff vectors ----------------------------------------------------------

mkdirSync(join(outDir, "diff"), { recursive: true });
const diffBase = validAttachment();
write("diff/base.json", diffBase);

// The rename candidate: the Model symbol of the task entity rebinds to
// planner.work_item while the entity key, the table names, and every
// storage projection stay exactly as declared.
const rename = patch(diffBase, {
  attachmentRevision: "1.0.1",
});
findEntity(rename, "task").entity = "planner.work_item";
write("diff/candidate-rename.json", rename);

// The storage-only candidate: the Laravel table of task is renamed and
// one technical column is dropped while the domain layer is untouched.
const storage = patch(diffBase, {
  "attachmentRevision": "1.0.1",
});
const laravel = findProjection(storage, "laravel");
laravel.tables.find((entry) => entry.entity === "task").table = "planner_task";
laravel.tables
  .find((entry) => entry.entity === "task")
  .technicalColumns.splice(0, 1);
for (const migration of laravel.migrationHistory) {
  migration.tables = migration.tables.map((name) => (name === "task" ? "planner_task" : name));
}
write("diff/candidate-storage.json", storage);

// The pure permutation: identical content, non-canonical array order.
const permutation = JSON.parse(JSON.stringify(diffBase));
permutation.entities.reverse();
permutation.relations.reverse();
permutation.projections
  .find((projection) => projection.namespace === "laravel")
  .tables.reverse();
write("diff/candidate-permutation.json", permutation);

// --- invalid vectors -------------------------------------------------------

const invalid = [];
const addInvalid = (name, document, rule, detail, subject) => {
  invalid.push({ document, name, rule, subject });
  const target = write(`invalid/${name}.json`, document);
  const expects =
    subject === undefined
      ? { detail, rule }
      : { detail, rule, subject };
  writeFileSync(
    join(dirname(target), `${name}.expect.json`),
    `${JSON.stringify(expects)}\n`,
  );
};

const mutate = (edits, mutateFn) => {
  const clone = patch(base, edits);
  if (mutateFn) mutateFn(clone);
  return clone;
};

// Wire-shape violations (Ajv and the typed normalizer agree).
addInvalid(
  "unknown-top-field",
  mutate({ extras: true }),
  "storage.input-invalid",
  "unknown-field",
);
addInvalid(
  "wrong-schema-version",
  mutate({ "schemaVersion": "lekalo/storage-projection/v0.9.0" }),
  "storage.input-invalid",
  "schema-version",
);
addInvalid(
  "wrong-contract-identity",
  mutate({ "identity": "dev.lekalo.storage-projection@0.9.0" }),
  "storage.input-invalid",
  "contract-identity",
);
addInvalid(
  "bad-attachment-revision",
  mutate({ "attachmentRevision": "1.0" }),
  "storage.input-invalid",
  "attachment-revision",
);
// The raw duplicate-key vector is a synthetic raw-text fixture; the
// typed normalizer never sees it because serde_json Map parsing
// deduplicates, so the Node raw scanner owns it.
write(
  "invalid/duplicate-json-key.json",
  null,
  '{"attachmentRevision":"1.0.0","attachmentRevision":"1.0.0"}',
);
write(
  "invalid/duplicate-json-key.expect.json",
  null,
  JSON.stringify({ detail: "duplicate-json-key", rule: "raw-duplicate-key" }),
);

// Domain-layer violations.
addInvalid(
  "duplicate-entity-key",
  mutate({}, (clone) => {
    const copy = JSON.parse(JSON.stringify(findEntity(clone, "tag")));
    copy.entity = "planner.tag_alias";
    clone.entities.push(copy);
  }),
  "storage.input-invalid",
  "duplicate-entity-key",
);
addInvalid(
  "field-duplicate",
  mutate({}, (clone) => {
    findEntity(clone, "task").fields.push(field("note", { name: "text" }, "internal"));
  }),
  "storage.input-invalid",
  "duplicate-field",
);
addInvalid(
  "string-without-length",
  mutate({}, (clone) => {
    findEntity(clone, "task").fields[4].type = { name: "string" };
  }),
  "storage.input-invalid",
  "type-params",
);
addInvalid(
  "decimal-scale-above-precision",
  mutate({}, (clone) => {
    findEntity(clone, "tag").fields[1] = field("label", { name: "decimal", precision: 4, scale: 9 }, "public", true);
  }),
  "storage.input-invalid",
  "type-params",
);
addInvalid(
  "aggregate-owner-missing",
  mutate({}, (clone) => {
    findEntity(clone, "task_detail").aggregateOwner = "ghost";
  }),
  "storage.domain-invalid",
  "aggregate-owner-missing",
  "task_detail",
);
addInvalid(
  "aggregate-owner-not-root",
  mutate({}, (clone) => {
    findEntity(clone, "task").aggregateRoot = undefined;
  }),
  "storage.domain-invalid",
  "aggregate-owner-not-root",
  "task_detail",
);

addInvalid(
  "aggregate-both",
  mutate({}, (clone) => {
    findEntity(clone, "task_detail").aggregateRoot = true;
  }),
  "storage.domain-invalid",
  "aggregate-both",
  "task_detail",
);
addInvalid(
  "external-aggregate",
  mutate({}, (clone) => {
    findEntity(clone, "jira_issue").aggregateRoot = true;
  }),
  "storage.domain-invalid",
  "external-aggregate",
  "jira_issue",
);

// Relation-layer violations.
addInvalid(
  "duplicate-relation-id",
  mutate({}, (clone) => {
    const copy = JSON.parse(JSON.stringify(findRelation(clone, "planner.relation.task_tags")));
    copy.target = "focus_session";
    copy.owner = "task";
    clone.relations.push(copy);
  }),
  "storage.input-invalid",
  "duplicate-relation-id",
);
addInvalid(
  "relation-unknown-target",
  mutate({}, (clone) => {
    findRelation(clone, "planner.relation.task_tags").target = "ghost";
  }),
  "storage.relation-invalid",
  "entity-absent",
  "planner.relation.task_tags",
);
addInvalid(
  "min-above-max",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_tags");
    relation.min = 5;
    relation.max = 2;
  }),
  "storage.relation-invalid",
  "min-above-max",
  "planner.relation.task_tags",
);
addInvalid(
  "one-to-one-max-two",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_detail_record");
    relation.max = 2;
  }),
  "storage.relation-invalid",
  "one-to-one-max",
  "planner.relation.task_detail_record",
);
addInvalid(
  "one-to-many-max-one",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_focus_sessions");
    relation.max = 1;
  }),
  "storage.relation-invalid",
  "one-to-many-max",
  "planner.relation.task_focus_sessions",
);
addInvalid(
  "optional-reference-min-one",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_parent");
    relation.min = 1;
  }),
  "storage.relation-invalid",
  "optional-reference-min",
  "planner.relation.task_parent",
);
addInvalid(
  "optional-reference-cascade",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_parent");
    relation.deleteBehavior = "cascade";
  }),
  "storage.relation-invalid",
  "optional-reference-delete",
  "planner.relation.task_parent",
);
addInvalid(
  "external-reference-non-external-target",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.link_provider");
    relation.target = "tag";
  }),
  "storage.relation-invalid",
  "external-target-required",
  "planner.relation.link_provider",
);
addInvalid(
  "external-reference-cascade",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.link_provider");
    relation.deleteBehavior = "cascade";
  }),
  "storage.relation-invalid",
  "external-delete-behavior",
  "planner.relation.link_provider",
);
addInvalid(
  "aggregate-child-unowned-target",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_external_links");
    relation.target = "tag";
  }),
  "storage.relation-invalid",
  "aggregate-child-ownership",
  "planner.relation.task_external_links",
);
addInvalid(
  "aggregate-child-detach",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_external_links");
    relation.deleteBehavior = "detach";
  }),
  "storage.relation-invalid",
  "aggregate-child-delete-behavior",
  "planner.relation.task_external_links",
);
addInvalid(
  "detach-minimum",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_detail_record");
    relation.deleteBehavior = "detach";
  }),
  "storage.relation-invalid",
  "detach-minimum",
  "planner.relation.task_detail_record",
);
addInvalid(
  "foreign-key-required",
  mutate({}, (clone) => {
    findRelation(clone, "planner.relation.task_focus_sessions").foreignKey = undefined;
  }),
  "storage.relation-invalid",
  "foreign-key-required",
  "planner.relation.task_focus_sessions",
);
addInvalid(
  "foreign-key-forbidden",
  mutate({}, (clone) => {
    findRelation(clone, "planner.relation.task_tags").foreignKey = "task_id";
  }),
  "storage.relation-invalid",
  "foreign-key-forbidden",
  "planner.relation.task_tags",
);
addInvalid(
  "coverage-missing",
  mutate({}, (clone) => {
    const relation = findRelation(clone, "planner.relation.task_tags");
    relation.scenarios = [];
  }),
  "storage.relation-invalid",
  "coverage-missing",
  "planner.relation.task_tags",
);

// Projection-layer violations.
addInvalid(
  "duplicate-namespace",
  mutate({}, (clone) => {
    const copy = JSON.parse(JSON.stringify(findProjection(clone, "postgres")));
    copy.tables = [];
    copy.joins = [];
    copy.polymorphics = [];
    copy.migrationHistory = [];
    clone.projections.push(copy);
  }),
  "storage.input-invalid",
  "duplicate-namespace",
);
addInvalid(
  "external-entity-projected",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").tables.push({
      entity: "jira_issue",
      primaryKey: ["id"],
      table: "jira_issue",
    });
  }),
  "storage.projection-invalid",
  "external-mapped",
  "jira_issue",
);
addInvalid(
  "unmapped-local-entity",
  mutate({}, (clone) => {
    const projection = findProjection(clone, "postgres");
    projection.tables = projection.tables.filter(
      (entry) => entry.entity !== "comment",
    );
  }),
  "storage.projection-invalid",
  "entity-unmapped",
  "comment",
);
addInvalid(
  "duplicate-table-name",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").tables.find(
      (entry) => entry.entity === "comment",
    ).table = "tag";
  }),
  "storage.projection-invalid",
  "duplicate-table-name",
);
addInvalid(
  "unknown-primary-key-column",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").tables.find(
      (entry) => entry.entity === "tag",
    ).primaryKey = ["ghost"];
  }),
  "storage.projection-invalid",
  "unknown-primary-key",
  "tag",
);
addInvalid(
  "column-collision",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").tables.find(
      (entry) => entry.entity === "tag",
    ).softDelete = { column: "label" };
  }),
  "storage.projection-invalid",
  "column-collision",
  "tag",
);
addInvalid(
  "unknown-index-column",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").tables.find(
      (entry) => entry.entity === "tag",
    ).indexes[0].columns = ["ghost"];
  }),
  "storage.projection-invalid",
  "unknown-index-column",
  "tag",
);
addInvalid(
  "bad-storage-type",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").tables.find(
      (entry) => entry.entity === "task",
    ).technicalColumns[0].type = "biginteger";
  }),
  "storage.input-invalid",
  "storage-type",
);
addInvalid(
  "migration-unknown-table",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").migrationHistory[0].tables.push("ghost");
  }),
  "storage.projection-invalid",
  "migration-unknown-table",
  "ghost",
);
addInvalid(
  "join-kind",
  mutate({}, (clone) => {
    const projection = findProjection(clone, "postgres");
    projection.joins[0].relation = "planner.relation.task_focus_sessions";
  }),
  "storage.projection-invalid",
  "join-kind",
  "planner.relation.task_focus_sessions",
);
addInvalid(
  "join-unknown-relation",
  mutate({}, (clone) => {
    const projection = findProjection(clone, "postgres");
    projection.joins[0].relation = "planner.relation.ghost";
  }),
  "storage.projection-invalid",
  "join-unknown-relation",
  "planner.relation.ghost",
);
addInvalid(
  "missing-join",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").joins = [];
  }),
  "storage.projection-invalid",
  "join-materialization-missing",
  "planner.relation.task_tags",
);
addInvalid(
  "polymorphic-kind",
  mutate({}, (clone) => {
    const projection = findProjection(clone, "postgres");
    projection.polymorphics[0].relation = "planner.relation.task_tags";
  }),
  "storage.projection-invalid",
  "polymorphic-kind",
  "planner.relation.task_tags",
);
addInvalid(
  "missing-polymorphic-materialization",
  mutate({}, (clone) => {
    findProjection(clone, "postgres").polymorphics = [];
  }),
  "storage.projection-invalid",
  "polymorphic-materialization-missing",
  "planner.relation.comment_target",
);



// --- summary ---------------------------------------------------------------

process.stdout.write(
  `${JSON.stringify({
    diffVectors: 4,
    invalidVectors: invalid.length + 1,
    ok: true,
    validGoldens: 1,
  })}\n`,
);

#!/usr/bin/env node
// Issue #117 fixture generator: deterministically emits the committed
// storage-introspection fixtures from one declarative description.
//
// The valid goldens are emitted in canonical form (compact JSON,
// byte-sorted keys, trailing LF) so the Rust canonical writer and an
// independent Node canonical-form check prove the same bytes. The
// invalid vectors and drift pairs are emitted as (document, expect)
// records. Run from the repository root:
//
//     node scripts/gen-storage-introspection-fixtures.mjs
//
// The script never reads the network and writes only into
// tests/fixtures/storage-introspection/.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "tests/fixtures/storage-introspection");

const digest = (seed) =>
  `sha256:${String(seed).padStart(2, "0").repeat(32).slice(0, 64)}`;

const DIGEST_MODEL = digest(21);
const DIGEST_IR = digest(22);
const DIGEST_OBSERVED = digest(23);

const column = (name, type, nullable, rest = {}) => ({
  name,
  nullable,
  type,
  ...rest,
});

const observedTable = (name, rest = {}) => ({
  collation: "utf8mb4_0900_ai_ci",
  engine: "innodb",
  name,
  ...rest,
});

const evidence = () => ({
  schemaVersion: "lekalo/storage-introspection/v0.4.0",
  identity: "dev.lekalo.storage-introspection@0.4.0",
  attachmentRevision: "0.4.0",
  projectId: "planner",
  modelRef: { digest: DIGEST_MODEL, modelVersion: "0.2.16" },
  irRef: { digest: DIGEST_IR, identity: "dev.lekalo.ir@0.2.16" },
  mode: "checked",
  readOnly: true,
  engine: {
    charset: "utf8mb4",
    collation: "utf8mb4_0900_ai_ci",
    defaultStorageEngine: "innodb",
    engine: "mysql",
    engineVersion: "8.0.36",
    sqlMode: [
      "ERROR_FOR_DIVISION_BY_ZERO",
      "NO_ENGINE_SUBSTITUTION",
      "NO_ZERO_DATE",
      "NO_ZERO_IN_DATE",
      "STRICT_TRANS_TABLES",
    ],
    timeZone: "+00:00",
  },
  testSchema: "lekalo_test_planner",
  observedDigest: DIGEST_OBSERVED,
  tables: [
    observedTable("comment", {
      columns: [
        column("body", "text", false),
        column("id", "binary(16)", false),
        column("target_id", "binary(16)", false),
        column("target_type", "varchar(64)", false),
      ],
      foreignKeys: [],
      indexes: [
        { columns: ["target_id", "target_type"], unique: true },
      ],
    }),
    observedTable("tag", {
      columns: [
        column("color", "varchar(64)", true, {
          collation: "utf8mb4_0900_ai_ci",
        }),
        column("id", "binary(16)", false),
        column("label", "varchar(64)", false, {
          collation: "utf8mb4_0900_ai_ci",
        }),
      ],
      foreignKeys: [],
      indexes: [{ columns: ["label"], name: "label", unique: true }],
    }),
    observedTable("task", {
      columns: [
        column("created_at", "datetime(6)", false),
        column("deleted_at", "datetime(6)", true),
        column("due_date", "date", true),
        column("id", "binary(16)", false),
        column("minutes", "bigint", true),
        column("note", "text", true),
        column("parent_task_id", "binary(16)", true),
        column("row_etag", "varbinary", false),
        column("status", "varchar(16)", false),
        column("tenant_id", "binary", false),
        column("title", "varchar(200)", false),
        column("updated_at", "datetime(6)", false),
      ],
      foreignKeys: [
        {
          column: "parent_task_id",
          onDelete: "SET NULL",
          referencesTable: "task",
        },
      ],
      indexes: [
        { columns: ["due_date"], unique: false },
        { columns: ["tenant_id"], name: "idx_task_tenant", unique: false },
      ],
    }),
  ],
  evidence: { kind: "observed-run", ref: "lekalo-test-run-001" },
});

// --- canonical form --------------------------------------------------------

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

const stripEmptyArrays = (value) => {
  if (Array.isArray(value)) return value.map(stripEmptyArrays);
  if (value && typeof value === "object") {
    const out = {};
    for (const [key, member] of Object.entries(value)) {
      if (
        Array.isArray(member) &&
        member.length === 0 &&
        ["foreignKeys", "indexes"].includes(key)
      ) {
        continue;
      }
      out[key] = stripEmptyArrays(member);
    }
    return out;
  }
  return value;
};

const write = (relative, document, suffix = "") => {
  const target = join(outDir, relative);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(
    target,
    suffix === "" ? `${canonical(stripEmptyArrays(document))}\n` : `${suffix}\n`,
  );
  return target;
};

// --- valid golden ----------------------------------------------------------

write("valid/mysql-8.0-planner.json", evidence());

// --- invalid vectors -------------------------------------------------------

const invalid = (name, document, rule, detail) => {
  write(`invalid/${name}.json`, document);
  write(
    `invalid/${name}.expect.json`,
    null,
    JSON.stringify({ detail, rule }),
  );
};

const base = evidence();
const patch = (edits) => {
  const clone = JSON.parse(JSON.stringify(base));
  for (const [path, update] of Object.entries(edits)) {
    const keys = path.split("/");
    let node = clone;
    while (keys.length > 1) node = node[keys.shift()];
    if (update === undefined) delete node[keys[0]];
    else node[keys[0]] = update;
  }
  return clone;
};

// The grammar carries no unchecked form.
invalid(
  "mode-unchecked",
  patch({ mode: "quick" }),
  "storage.introspection-invalid",
  "mode-checked",
);
invalid(
  "read-only-false",
  patch({ readOnly: false }),
  "storage.introspection-invalid",
  "read-only-true",
);
// A credential-shaped member is refused before any other rule.
invalid(
  "credential-member",
  patch({ host: "db.internal.example" }),
  "storage.introspection-invalid",
  "credential-member",
);
// The evidence binds exactly one test schema name.
invalid(
  "host-shaped-schema",
  patch({ testSchema: "db.internal.example:3306" }),
  "storage.introspection-invalid",
  "test-schema",
);

// --- drift pairs (valid documents, different observed facts) ---------------

mkdirSync(join(outDir, "drift"), { recursive: true });
const drift = (name, document) => write(`drift/${name}.json`, document);

// A declared column observed with a different type spelling.
const typeMismatch = patch({
  "tables/1/columns/1/type": "text",
});
drift("type-mismatch", typeMismatch);

// A unique index the evidence lacks.
const missingIndex = JSON.parse(JSON.stringify(base));
missingIndex.tables[1].indexes = [];
drift("missing-index", missingIndex);

// A table the declaration maps and the evidence does not observe.
const missingTable = JSON.parse(JSON.stringify(base));
missingTable.tables = missingTable.tables.filter((table) => table.name !== "comment");
drift("missing-table", missingTable);

// A non-innodb observed engine.
const engineMismatch = patch({ "tables/0/engine": "myisam" });
drift("engine-mismatch", engineMismatch);

// A different observed sql mode.
const sqlModeMismatch = patch({ "engine/sqlMode": ["TRADITIONAL"] });
drift("sql-mode-mismatch", sqlModeMismatch);

// A different observed engine version.
const versionMismatch = patch({ "engine/engineVersion": "8.4.2" });
drift("version-mismatch", versionMismatch);

// --- summary ---------------------------------------------------------------

process.stdout.write(
  `${JSON.stringify({
    driftPairs: 6,
    invalidVectors: 4,
    ok: true,
    validGoldens: 1,
  })}\n`,
);

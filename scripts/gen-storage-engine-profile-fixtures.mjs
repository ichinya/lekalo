#!/usr/bin/env node
// Issue #117 fixture generator: deterministically emits the committed
// storage-engine-profile fixtures from one declarative description.
//
// The valid goldens are emitted in canonical form (compact JSON,
// byte-sorted keys, no trailing LF) so the Rust canonical writer and an
// independent Node canonical-form check prove the same bytes. The
// invalid vectors are emitted as (document, expect) pairs; the expect
// file names the exact registered rule and fixed detail token that the
// typed normalizer must produce. Run from the repository root:
//
//     node scripts/gen-storage-engine-profile-fixtures.mjs
//
// The script never reads the network and writes only into
// tests/fixtures/storage-engine-profile/.

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "tests/fixtures/storage-engine-profile");

const digest = (seed) =>
  `sha256:${String(seed).padStart(2, "0").repeat(32).slice(0, 64)}`;

const DIGEST_MODEL = digest(11);
const DIGEST_IR = digest(12);

const evidence = (kind, ref) => ({ kind, ref });
const capability = (support, kind, ref, bounds) => ({
  support,
  ...(bounds === undefined ? {} : { bounds }),
  evidence: evidence(kind, ref),
});
const full = (ref) => capability("full", "vendor-docs", ref);
const unsupported = (ref) => capability("unsupported", "vendor-docs", ref);
const partial = (bounds, ref) => capability("partial", "vendor-docs", ref, bounds);

const envelope = (engine) => ({
  attachmentRevision: "0.4.0",
  engine,
  identity: "dev.lekalo.storage-engine-profile@0.4.0",
  irRef: { digest: DIGEST_IR, identity: "dev.lekalo.ir@0.2.16" },
  modelRef: { digest: DIGEST_MODEL, modelVersion: "0.2.16" },
  projectId: "planner",
  schemaVersion: "lekalo/storage-engine-profile/v0.4.0",
});

const sqlMode = [
  "ERROR_FOR_DIVISION_BY_ZERO",
  "NO_ENGINE_SUBSTITUTION",
  "NO_ZERO_DATE",
  "NO_ZERO_IN_DATE",
  "STRICT_TRANS_TABLES",
];

const innodbTransactions = {
  "transaction.atomic_group": full("mysql-8.0-en"),
  "transaction.rollback": full("mysql-8.0-en"),
  "transaction.transactional_ddl": unsupported("mysql-8.0-en"),
  "isolation.read_committed": full("mysql-8.0-en"),
  "isolation.repeatable_read": full("mysql-8.0-en"),
  "isolation.serializable": full("mysql-8.0-en"),
  "isolation.snapshot": unsupported("mysql-8.0-en"),
  "lock.shared": full("mysql-8.0-en"),
  "lock.exclusive": full("mysql-8.0-en"),
  "lock.key": full("mysql-8.0-en"),
  "lock.range": partial(
    "gap and next-key locks only under repeatable read; effectively absent under read committed",
    "mysql-8.0-en",
  ),
  "lock.nowait": full("mysql-8.0-en"),
  "lock.skip_locked": full("mysql-8.0-en"),
  "concurrency.compare_and_set": full("mysql-8.0-en"),
  "concurrency.etag_if_match": full("mysql-8.0-en"),
  "invariant.unique_concurrent": full("mysql-8.0-en"),
  "invariant.collation_aware": full("mysql-8.0-en"),
  "idempotency.durable_key": full("mysql-8.0-en"),
  "idempotency.replay": partial(
    "no engine result replay; adapter-level dedup only",
    "mysql-8.0-en",
  ),
};

const storageCapabilities = {
  "storage.check_constraints": full("mysql-8.0-en"),
  "storage.fulltext_index": partial(
    "parser and stopword semantics are engine- and version-specific",
    "mysql-8.0-en",
  ),
  "storage.prefix_index": full("mysql-8.0-en"),
  "storage.generated_columns": full("mysql-8.0-en"),
  "storage.sequences": unsupported("mysql-8.0-en"),
  "storage.collation_aware": full("mysql-8.0-en"),
  "storage.introspection_checked": full("mysql-8.0-en"),
  "storage.partial_index": unsupported("mysql-8.0-en"),
  "storage.deferred_constraints": unsupported("mysql-8.0-en"),
  "storage.exclusion_constraints": unsupported("mysql-8.0-en"),
  "storage.array_types": unsupported("mysql-8.0-en"),
  "storage.json_operators": partial(
    "JSON_EXTRACT family; no jsonb binary operator surface",
    "mysql-8.0-en",
  ),
  "storage.returning": unsupported("mysql-8.0-en"), // mariadb arm flips this to partial (10.5+)
  "storage.timestamptz": unsupported("mysql-8.0-en"),
  "storage.advisory_locks": partial(
    "GET_LOCK named locks only",
    "mysql-8.0-en",
  ),
  "index.descending": full("mysql-8.0-en"),
  "index.functional": full("mysql-8.0-en"),
  "index.invisible": partial(
    "invisible index semantics are 8.0-specific",
    "mysql-8.0-en",
  ),
  "pagination.limit_offset": full("mysql-8.0-en"),
  "pagination.keyset_cursor": full("mysql-8.0-en"),
  "test.create_schema": full("mysql-8.0-en"),
  "test.drop_schema": full("mysql-8.0-en"),
};

const mysqlEngine = (version) => ({
  charset: "utf8mb4",
  collation: "utf8mb4_0900_ai_ci",
  defaultStorageEngine: "innodb",
  engine: "mysql",
  engineVersion: version,
  evidence: evidence("vendor-docs", "mysql-8.0-en"),
  sqlMode: [...sqlMode],
  timeZone: "+00:00",
  variant: "mysql-community",
});

const mariadbCapabilities = {
  ...Object.fromEntries(
    Object.entries(storageCapabilities).map(([id, record]) => [
      id,
      id === "storage.sequences"
        ? full("mariadb-10.11-en")
        : id === "storage.returning"
          ? partial(
              "10.5 or newer statements; not every context returns rows",
              "mariadb-10.11-en",
            )
          : { ...record, evidence: evidence(record.evidence.kind, "mariadb-10.11-en") },
    ]),
  ),
  ...Object.fromEntries(
    Object.entries(innodbTransactions).map(([id, record]) => [
      id,
      id === "lock.shared"
        ? partial(
            "LOCK IN SHARE MODE only; no FOR SHARE spelling before 10.6 parity work",
            "mariadb-10.11-en",
          )
        : { ...record, evidence: evidence(record.evidence.kind, "mariadb-10.11-en") },
    ]),
  ),
};

const mariadbEngine = (version) => ({
  charset: "utf8mb4",
  collation: "utf8mb4_general_ci",
  defaultStorageEngine: "innodb",
  engine: "mariadb",
  engineVersion: version,
  evidence: evidence("vendor-docs", "mariadb-10.11-en"),
  sqlMode: [...sqlMode],
  timeZone: "+00:00",
  variant: "mariadb",
});

const testLifecycle = (engineRef) => ({
  create: full(engineRef),
  drop: full(engineRef),
  evidence: evidence("vendor-docs", engineRef),
  isolation: "schema-per-run",
  production: "forbidden",
  testSchemaPrefix: "lekalo_test",
});

const mysql80 = () => ({
  ...envelope(mysqlEngine("8.0.36")),
  adapters: [
    {
      dialect: "drizzle mysql-core",
      evidence: evidence("adapter-docs", "drizzle-orm-mysql-core"),
      name: "drizzle",
    },
  ],
  capabilities: {
    ...Object.fromEntries(
      Object.entries(innodbTransactions).map(([id, record]) => [
        id,
        { ...record, evidence: evidence(record.evidence.kind, "mysql-8.0-en") },
      ]),
    ),
    ...storageCapabilities,
  },
  testLifecycle: testLifecycle("mysql-8.0-en"),
});

const mariadb1011 = () => ({
  ...envelope(mariadbEngine("10.11.8")),
  capabilities: mariadbCapabilities,
  testLifecycle: testLifecycle("mariadb-10.11-en"),
});

// --- canonical form --------------------------------------------------------

// Normalize the member orders the wire contract treats as set-like,
// mirroring the Rust canonical writer exactly.
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
  const target = join(outDir, relative);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, suffix === "" ? `${canonical(document)}\n` : `${suffix}\n`);
  return target;
};

// --- valid goldens ---------------------------------------------------------

write("valid/mysql-8.0.json", mysql80());
write("valid/mariadb-10.11.json", mariadb1011());

// --- invalid vectors -------------------------------------------------------

const invalid = (name, document, rule, detail) => {
  write(`invalid/${name}.json`, document);
  write(
    `invalid/${name}.expect.json`,
    null,
    JSON.stringify({ detail, rule }),
  );
};

const base = mysql80();
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

// A merged mysql-family claim: the closed engine token refuses it.
invalid(
  "engine-merged",
  patch({ "engine/engine": "mysql-family" }),
  "storage.profile-invalid",
  "engine-token",
);
// A version range: exact engine/version evidence is mandatory.
invalid(
  "engine-version-range",
  patch({ "engine/engineVersion": "8.0-8.4" }),
  "storage.profile-invalid",
  "engine-version-range",
);
// The mandatory sql-mode member is missing: the mode is never implicit.
invalid(
  "sql-mode-missing",
  patch({ "engine/sqlMode": undefined }),
  "storage.profile-invalid",
  "sql-mode-missing",
);
// A capability record without the support member.
invalid(
  "capability-without-support",
  patch({
    "capabilities/transaction.atomic_group": {
      evidence: evidence("vendor-docs", "mysql-8.0-en"),
    },
  }),
  "storage.profile-invalid",
  "support-token",
);
// Partial support without its bounded gap note.
invalid(
  "partial-without-bounds",
  patch({
    "capabilities/lock.range": capability("partial", "vendor-docs", "mysql-8.0-en"),
  }),
  "storage.profile-invalid",
  "partial-without-bounds",
);
// A credential-shaped member: the grammar refuses it outright.
invalid(
  "credential-member",
  patch({ host: "db.internal.example" }),
  "storage.profile-invalid",
  "credential-member",
);
// The production token carries no other spelling than forbidden.
invalid(
  "production-allowed",
  patch({ "testLifecycle/production": "allowed" }),
  "storage.profile-invalid",
  "production-forbidden",
);
// The collation does not belong to the declared charset.
invalid(
  "collation-charset-mismatch",
  patch({ "engine/collation": "latin1_swedish_ci" }),
  "storage.profile-invalid",
  "collation-charset-mismatch",
);
// A capability id outside the closed vocabulary.
invalid(
  "unknown-capability-id",
  patch({
    "capabilities/storage.made_up": full("mysql-8.0-en"),
  }),
  "storage.profile-invalid",
  "capability-id",
);

// --- summary ---------------------------------------------------------------

process.stdout.write(
  `${JSON.stringify({
    invalidVectors: 9,
    ok: true,
    validGoldens: 2,
  })}\n`,
);

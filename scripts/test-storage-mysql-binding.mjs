#!/usr/bin/env node
// Issue #117 release gate: the hermetic Drizzle/MySQL fixture binds to
// the Lekalo storage projection (AC-2). The TS scanner is #116; this
// gate proves the contract surface the binding validates against:
//
//  1. the fixture files exist and the binding document is canonical
//     JSON;
//  2. every table the binding expects exists in the committed mysql
//     derived golden, and every column expectation matches the derived
//     storage type exactly;
//  3. the dialect divergences (serial alias, json storage class, uuid
//     version gating) are recorded in the binding and the mysql
//     namespace really refuses the mariadb-only uuid token.
//
// The gate never parses TypeScript and never reads the network.

import { readFileSync, existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fail = (reason, detail) => {
  process.stderr.write(`${JSON.stringify({ ok: false, reason, detail }, null, 2)}\n`);
  process.exit(1);
};

const schemaPath = "tests/fixtures/storage-mysql/drizzle/schema.ts";
const bindingPath = "tests/fixtures/storage-mysql/drizzle/binding.expect.json";
const derivedPath = "tests/fixtures/storage-projection/derived/mysql.json";
const validPath = "tests/fixtures/storage-projection/valid/planner-storage.json";

for (const relative of [schemaPath, bindingPath, derivedPath, validPath]) {
  if (!existsSync(resolve(root, relative))) fail("fixture-missing", relative);
}

// 1. The fixture carries the drizzle mysql-core import and the
//    declared table spellings.
const schema = readFileSync(resolve(root, schemaPath), "utf8");
if (!schema.includes("drizzle-orm/mysql-core")) fail("fixture-import", "mysql-core");
for (const table of ["task", "tag", "task_tag", "focus_session", "comment"]) {
  if (!schema.includes(`"${table}"`)) fail("fixture-table", table);
}

// 2. The binding document and the derived golden agree.
const binding = JSON.parse(readFileSync(resolve(root, bindingPath), "utf8"));
if (binding.binding.namespace !== "mysql") fail("binding-namespace", binding.binding.namespace);
if (binding.binding.adapter !== "drizzle") fail("binding-adapter", binding.binding.adapter);
const derived = JSON.parse(readFileSync(resolve(root, derivedPath), "utf8"));
const tableByName = new Map(derived.tables.map((table) => [table.table, table]));
// The join tables are materialized separately; bind them through the
// same column-expectation contract.
const joinByName = new Map(derived.joins.map((join) => [join.table, join]));

const expectedTables = Object.keys(binding.expectations.tables);
if (expectedTables.length === 0) fail("binding-empty", "no table expectations");
for (const [tableName, expectation] of Object.entries(binding.expectations.tables)) {
  const table = tableByName.get(tableName);
  const join = joinByName.get(tableName);
  if (!table && !join) fail("binding-table-unknown", tableName);
  const columns = table ? table.columns : join.columns;
  for (const [columnName, columnExpectation] of Object.entries(expectation.columns)) {
    const column = columns.find((entry) => entry.name === columnName);
    if (!column) fail("binding-column-unknown", `${tableName}.${columnName}`);
    if (column.type !== columnExpectation.storageType) {
      fail("binding-type-mismatch", {
        column: `${tableName}.${columnName}`,
        derived: column.type,
        expected: columnExpectation.storageType,
      });
    }
    if (column.nullable !== (columnExpectation.nullable ?? true)) {
      fail("binding-nullability-mismatch", `${tableName}.${columnName}`);
    }
  }
  if (expectation.indexes && table) {
    for (const indexExpectation of expectation.indexes) {
      const match = table.indexes.some(
        (index) =>
          (index.name ?? null) === (indexExpectation.name ?? null) &&
          index.columns.join(",") === indexExpectation.columns.join(","),
      );
      if (!match) fail("binding-index-mismatch", `${tableName}:${indexExpectation.name}`);
    }
  }
}

// 3. The dialect divergences are recorded.
const divergences = binding.expectations.dialectDivergences ?? {};
for (const key of ["serial", "json", "uuid"]) {
  if (typeof divergences[key] !== "string" || divergences[key].length < 8) {
    fail("divergence-missing", key);
  }
}
// The mysql namespace vocabulary really refuses the mariadb uuid.
const valid = JSON.parse(readFileSync(resolve(root, validPath), "utf8"));
const mysqlProjection = valid.projections.find(
  (projection) => projection.namespace === "mysql",
);
if (!mysqlProjection) fail("projection-missing", "mysql");
const declaredTypes = new Set();
for (const table of mysqlProjection.tables) {
  for (const technical of table.technicalColumns ?? []) declaredTypes.add(technical.type);
  for (const generated of table.generatedColumns ?? []) {
    if (generated.type) declaredTypes.add(generated.type);
  }
  if (table.tenantKey) declaredTypes.add(table.tenantKey.type);
}
if (declaredTypes.has("uuid")) fail("mysql-uuid-leak", "uuid must stay mariadb-only");

process.stdout.write(
  `${JSON.stringify({
    bindingTables: expectedTables.length,
    divergences: Object.keys(divergences).length,
    ok: true,
  })}\n`,
);

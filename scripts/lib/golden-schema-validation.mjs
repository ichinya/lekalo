// Shared Ajv validation for the closed golden-update plan schema.
// Ajv 8.17.1 resolves from the repo checkout when present or from
// LEKALO_AJV_NODE_PATH (the same provisioning as the contract gates).

import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(here, "../..");

let cached = null;

function loadAjv() {
  if (cached) return cached;
  const require = createRequire(import.meta.url);
  let Ajv2020;
  let version;
  try {
    Ajv2020 = require("ajv/dist/2020").default;
    version = require("ajv/package.json").version;
  } catch {
    const fallback = join(process.env.LEKALO_AJV_NODE_PATH ?? "", "ajv");
    Ajv2020 = require(join(fallback, "dist/2020.js")).default;
    version = JSON.parse(readFileSync(join(fallback, "package.json"), "utf8")).version;
  }
  if (version !== "8.17.1") {
    throw new Error(`ajv-version-mismatch: ${version}`);
  }
  cached = Ajv2020;
  return Ajv2020;
}

/** Validate one plan object; returns the Ajv error list (empty = valid). */
export function validateGoldenUpdatePlan(plan) {
  const Ajv2020 = loadAjv();
  const schema = JSON.parse(
    readFileSync(join(repoRoot, "tests/fixtures/suite/schema/golden-update.schema.v1.0.0.json"), "utf8"),
  );
  const ajv = new Ajv2020({ strict: true, allErrors: true });
  const validate = ajv.compile(schema);
  if (validate(plan)) return [];
  return validate.errors ?? [{ message: "unknown validation failure" }];
}

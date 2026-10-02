import { readFileSync, writeFileSync } from "node:fs";
const p = "contracts/context-budget-comparison.schema.v0.6.3.json";
let t = readFileSync(p, "utf8");
const q = String.fromCharCode(34);
const old1 = [
  "          " + q + "then" + q + ": {",
  "            " + q + "not" + q + ": { " + q + "required" + q + ": [" + q + "reason" + q + "] },",
  "            " + q + "required" + q + ": [" + q + "deltas" + q + "],",
  "          },",
].join("\n");
const new1 = [
  "          " + q + "then" + q + ": {",
  "            " + q + "properties" + q + ": { " + q + "reason" + q + ": { " + q + "not" + q + ": {} } },",
  "            " + q + "required" + q + ": [" + q + "deltas" + q + "],",
  "          },",
].join("\n");
if (!t.includes(old1)) { console.error("r1"); process.exit(1); }
t = t.replace(old1, new1);
const old2 = [
  "          " + q + "then" + q + ": {",
  "            " + q + "required" + q + ": [" + q + "reason" + q + "],",
  "            " + q + "not" + q + ": { " + q + "required" + q + ": [" + q + "deltas" + q + "] },",
  "          },",
].join("\n");
const new2 = [
  "          " + q + "then" + q + ": {",
  "            " + q + "required" + q + ": [" + q + "reason" + q + "],",
  "            " + q + "properties" + q + ": { " + q + "deltas" + q + ": { " + q + "not" + q + ": {} } },",
  "          },",
].join("\n");
if (!t.includes(old2)) { console.error("r2"); process.exit(1); }
t = t.replace(old2, new2);
writeFileSync(p, t);
console.log("strict-safe absence");

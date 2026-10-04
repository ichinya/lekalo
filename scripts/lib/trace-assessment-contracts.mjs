// Independent canonical/digest implementation for the issue #35 gates.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

export const root = fileURLToPath(new URL("../../", import.meta.url));
export const family = "tests/fixtures/trace-assessment";
export const read = (path) => JSON.parse(readFileSync(resolve(root, path), "utf8"));
export const stable = (value) => {
  if (Array.isArray(value)) return `[${value.map(stable).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.keys(value).sort().map((k) => `${JSON.stringify(k)}:${stable(value[k])}`).join(",")}}`;
  return JSON.stringify(value);
};
export const digest = (value) => `sha256:${createHash("sha256").update(stable(value)).digest("hex")}`;
export function normalized(input) {
  const doc = structuredClone(input);
  for (const ids of Object.values(doc.scope)) ids.sort();
  doc.requiredProviders.sort();
  doc.providerPins.sort((a, b) => a.provider < b.provider ? -1 : a.provider > b.provider ? 1 : 0);
  for (const chain of doc.chains) { chain.relationRefs.sort(); chain.evidenceRefs.sort(); }
  const byId = (a, b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  doc.chains.sort(byId); doc.evidence.sort(byId);
  const rank = { error: 0, warning: 1, info: 2 };
  for (const receipt of doc.evidence) receipt.diagnostics.sort((a, b) => {
    for (const key of ["code", "subject"]) { if (a[key] < b[key]) return -1; if (a[key] > b[key]) return 1; }
    return rank[a.severity] - rank[b.severity];
  });
  return doc;
}
export const mappingDigest = (input) => {
  const doc = normalized(input);
  return digest(["lekalo/trace-validation-evidence/v0.6.4/mapping", doc.scope, doc.chains]);
};

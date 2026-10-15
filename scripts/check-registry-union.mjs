#!/usr/bin/env node
// The union-completeness guard for the 0.8.0 diagnostic registry
// (issue #113, r8 review finding 1): the merged registry is a union
// successor of every predecessor line it integrates. A stale union that
// misses a whole landed family (r6 portability, r7 managed, r8 tasklink)
// was invisible to the family gates because those only police additions
// on this branch. This gate polices the other direction:
//
//   1. monotonicity — against every accessible parent of HEAD (merge
//      commit or not), the registry keeps every parent entry id and
//      every parent family prefix; a registry may never shrink.
//   2. live-line completeness — against --base REF (default: the local
//      ichinya/M8 integration ref when it exists), the registry keeps
//      every base entry id and family, so an unmerged live-line family
//      is a failure, not a review finding.
//   3. registry invariants — ids sorted, ids unique, message_id == id.
//
// --selftest exercises every failure mode on synthetic registries;
// --ref REF reads the child registry from a git ref instead of the
// working tree (used to reproduce the historical stale states).
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const REGISTRY = "contracts/diagnostic-registry.v0.8.0.json";
const git = (...args) =>
  execFileSync("git", args, { cwd: ROOT, encoding: "utf8", maxBuffer: 1 << 26 });

const args = process.argv.slice(2);
const flag = (name) => {
  const at = args.indexOf(name);
  if (at === -1) return null;
  return args[at + 1];
};

/// Parse one registry document into { ids, families }.
function parseRegistry(text) {
  const doc = JSON.parse(text);
  const ids = doc.entries.map((entry) => entry.id);
  const families = new Set(ids.map((id) => `${id.split(".")[0]}.`));
  return { ids, families, doc };
}

/// Structural invariants of the union registry itself.
function registryInvariants({ ids, doc }) {
  const problems = [];
  const sorted = [...ids].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  if (JSON.stringify(ids) !== JSON.stringify(sorted)) {
    problems.push("registry-not-id-sorted");
  }
  if (new Set(ids).size !== ids.length) problems.push("registry-ids-not-unique");
  if (new Set(doc.entries.map((entry) => entry.code)).size !== doc.entries.length) {
    problems.push("registry-codes-not-unique");
  }
  if (!doc.entries.every((entry) => entry.message_id === entry.id)) {
    problems.push("registry-message-id-drift");
  }
  return problems;
}

/// Entries and families of `ref`'s registry, or null when the ref (or the
/// path at that ref) is unavailable — shallow clones may legitimately not
/// have the parent.
function registryAt(ref) {
  try {
    return parseRegistry(git("show", `${ref}:${REGISTRY}`));
  } catch {
    return null;
  }
}

/// The stale-base failure core: every id and family of `parent` must
/// survive in `child`. Returns the names of the problems found.
function unionGaps(child, parent, label) {
  const gaps = [];
  const have = new Set(child.ids);
  const missingIds = parent.ids.filter((id) => !have.has(id));
  if (missingIds.length) {
    gaps.push(`${label}: registry dropped ${missingIds.length} entries (${missingIds.slice(0, 5).join(", ")}${missingIds.length > 5 ? ", …" : ""})`);
  }
  const missingFamilies = [...parent.families].filter((family) => !child.families.has(family));
  if (missingFamilies.length) {
    gaps.push(`${label}: registry dropped landed families (${missingFamilies.join(", ")})`);
  }
  return gaps;
}

function selftest() {
  const base = {
    schema_version: "0.8.0",
    entries: [
      { id: "format.a", code: "LEK-FORMAT-001", message_id: "format.a" },
      { id: "module-api.b", code: "LEK-MODULE-001", message_id: "module-api.b" },
    ],
  };
  const ok = () => ({ doc: base, ids: base.entries.map((e) => e.id),
    families: new Set(["format.", "module-api."]) });
  const clone = (o) => JSON.parse(JSON.stringify(o));

  // 1. A superset child passes.
  const grown = clone(base);
  grown.entries.push({ id: "tasklink.c", code: "LEK-TASKLINK-001", message_id: "tasklink.c" });
  const grownReg = parseRegistry(JSON.stringify(grown));
  if (unionGaps(grownReg, ok(), "t1").length) throw new Error("selftest 1: superset rejected");
  if (registryInvariants(grownReg).length) throw new Error("selftest 1: invariants rejected a clean registry");

  // 2. The r8 recurrence shape: the parent (live line) carries a family
  // the child (this branch) never merged.
  const live = clone(base);
  for (let i = 1; i <= 8; i += 1) {
    live.entries.push({ id: `tasklink.rule-${i}`, code: `LEK-TASKLINK-00${i}`, message_id: `tasklink.rule-${i}` });
  }
  const gaps = unionGaps(parseRegistry(JSON.stringify(base)), parseRegistry(JSON.stringify(live)), "t2");
  if (!gaps.some((gap) => gap.includes("dropped 8 entries")) || !gaps.some((gap) => gap.includes("dropped landed families (tasklink.)"))) {
    throw new Error(`selftest 2: stale union not caught: ${gaps.join(" | ")}`);
  }

  // 3. Structural invariants: unsorted, duplicate ids, code reuse, and
  // message_id drift are all failures.
  const broken = { schema_version: "0.8.0", entries: [
    { id: "z.last", code: "LEK-A-001", message_id: "z.last" },
    { id: "a.first", code: "LEK-A-001", message_id: "other" },
  ] };
  const problems = registryInvariants(parseRegistry(JSON.stringify(broken)));
  for (const want of ["registry-not-id-sorted", "registry-codes-not-unique", "registry-message-id-drift"]) {
    if (!problems.includes(want)) throw new Error(`selftest 3: ${want} not caught`);
  }
  return true;
}

if (args.includes("--selftest")) {
  selftest();
  console.log(JSON.stringify({ ok: true, gate: "registry-union", mode: "selftest" }));
  process.exit(0);
}

const problems = [];
// The child registry under review: the working tree copy (what a commit
// would carry), or --ref REF for a historical ref.
const ref = flag("--ref");
const child = ref
  ? registryAt(ref)
  : parseRegistry(readFileSync(resolve(ROOT, REGISTRY), "utf8"));
if (!child) throw new Error(`registry not readable at ${ref ?? REGISTRY}`);
problems.push(...registryInvariants(child).map((p) => `${ref ?? "worktree"}: ${p}`));

// 1. Monotonicity against every accessible parent of the checked ref.
const head = ref ?? "HEAD";
let parents = [];
try {
  parents = git("rev-list", "--parents", "-n", "1", head).trim().split(" ").slice(1);
} catch {
  parents = [];
}
let parentEdges = 0;
for (const parent of parents) {
  const parentRegistry = registryAt(parent);
  if (!parentRegistry) continue; // unavailable in a shallow clone
  parentEdges += 1;
  problems.push(...unionGaps(child, parentRegistry, `parent ${parent.slice(0, 9)}`));
}

// 2. Live-line completeness against the integration base.
const baseArg = flag("--base");
const baseRef = baseArg ?? "ichinya/M8";
const baseExplicit = baseArg !== null;
let baseRegistry = null;
let baseChecked = false;
try {
  git("rev-parse", "--verify", "--quiet", `${baseRef}^{commit}`);
  baseRegistry = registryAt(baseRef);
} catch {
  baseRegistry = baseExplicit ? registryAt(baseRef) : null;
}
if (baseRegistry) {
  baseChecked = true;
  problems.push(...unionGaps(child, baseRegistry, `base ${baseRef}`));
} else if (baseExplicit) {
  problems.push(`base ${baseRef}: registry not readable`);
}

if (problems.length) throw new Error(`registry union guard:\n${problems.join("\n")}`);
console.log(JSON.stringify({
  ok: true,
  gate: "registry-union",
  ref: ref ?? "worktree",
  entries: child.ids.length,
  families: [...child.families].length,
  parentEdges,
  base: baseChecked ? baseRef : "skipped",
}));

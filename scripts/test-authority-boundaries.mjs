#!/usr/bin/env node

import { loadAcceptedAuthority, evaluate } from "./check-authority.mjs";

const { contract, context } = await loadAcceptedAuthority("1.3.1");
const actors = contract.actors;
const kinds = contract.artifactKinds;
const broadPredecessorKinds = new Set([
  "source-native.source-code",
  "source-native.native-test",
  "generated.code",
  "native.source-map"
]);

function fail(message) {
  throw new Error(message);
}

function globToRegExp(glob) {
  let output = "^";
  for (let index = 0; index < glob.length; index += 1) {
    const character = glob[index];
    if (character === "*") {
      if (glob[index + 1] === "*") {
        index += 1;
        if (glob[index + 1] === "/") {
          index += 1;
          output += "(?:.*/)?";
        } else {
          output += ".*";
        }
      } else {
        output += "[^/]*";
      }
    } else if (character === "?") {
      output += "[^/]";
    } else {
      output += character.replace(/[\\^$+?.()|{}\[\]]/g, "\\$&");
    }
  }
  return new RegExp(`${output}$`, "iu");
}

function matches(path, pattern) {
  return globToRegExp(pattern).test(path);
}

function sampleFor(pattern) {
  return pattern.split("/").map((segment) =>
    segment === "**" ? "sample" : segment.replace(/\*+/gu, "sample").replace(/\?/gu, "q")
  ).join("/");
}

function specificity(pattern) {
  const segments = pattern.split("/");
  let prefix = 0;
  for (const segment of segments) {
    if (/[*?]/u.test(segment)) break;
    prefix += 1;
  }
  return [
    prefix,
    segments.filter((segment) => !/[*?]/u.test(segment)).length,
    [...pattern].filter((character) => !["*", "?", "/"].includes(character)).length,
    segments.length,
    -(pattern.match(/\*\*|\*|\?/gu)?.length ?? 0)
  ];
}

function compare(left, right) {
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return left[index] - right[index];
  }
  return 0;
}

function identity(boundary) {
  return JSON.stringify({
    owner: boundary.owner,
    artifactKinds: boundary.artifactKinds,
    readers: boundary.readers,
    writers: boundary.writers
  });
}

function selectBoundary(path, includeConditional) {
  const candidates = contract.pathBoundaries.filter((boundary) => matches(path, boundary.pattern));
  if (includeConditional) {
    candidates.push(...contract.conditionalPathBoundaries.filter((boundary) => matches(path, boundary.pattern)));
  }
  if (candidates.length === 0) fail(`No boundary matched generated sample ${path}`);
  let bestScore = specificity(candidates[0].pattern);
  for (const candidate of candidates.slice(1)) {
    const score = specificity(candidate.pattern);
    if (compare(score, bestScore) > 0) bestScore = score;
  }
  const best = candidates.filter((candidate) => compare(specificity(candidate.pattern), bestScore) === 0);
  if (new Set(best.map(identity)).size !== 1) fail(`Independent oracle found ambiguous best policy for ${path}`);
  return best[0];
}

let decisions = 0;
let allowed = 0;
let denied = 0;
let mismatches = 0;
let broadRelabelDenials = 0;
const boundaryCases = [
  ...contract.pathBoundaries.map((boundary) => ({ boundary, conditional: false })),
  ...contract.conditionalPathBoundaries.map((boundary) => ({ boundary, conditional: true }))
];

for (const { boundary: declaredBoundary, conditional } of boundaryCases) {
  const path = sampleFor(declaredBoundary.pattern);
  const selected = selectBoundary(path, conditional);
  if (selected.pattern !== declaredBoundary.pattern) {
    fail(`Sample ${path} for ${declaredBoundary.pattern} selected more-specific ${selected.pattern}`);
  }

  for (const kind of kinds) {
    const pathAllowed = kind.allowedPaths.some((pattern) => matches(path, pattern));
    const kindAllowed = selected.artifactKinds.includes(kind.id);
    for (const actor of actors) {
      const operationContext = conditional ? { hlvLayoutConfirmed: true } : undefined;
      const readOperation = {
        action: "read",
        actor,
        source: { artifactKind: kind.id, path },
        ...(operationContext ? { context: operationContext } : {})
      };
      const writeOperation = {
        action: "write",
        actor,
        target: { artifactKind: kind.id, path },
        ...(operationContext ? { context: operationContext } : {})
      };
      const expectedRead = pathAllowed && kindAllowed
        && kind.allowedReaders.includes(actor) && selected.readers.includes(actor);
      const expectedWrite = pathAllowed && kindAllowed
        && kind.allowedWriters.includes(actor) && selected.writers.includes(actor);
      const actualRead = evaluate(readOperation, contract, context);
      const actualWrite = evaluate(writeOperation, contract, context);
      for (const [mode, expected, actual] of [
        ["read/source", expectedRead, actualRead],
        ["write/target", expectedWrite, actualWrite]
      ]) {
        decisions += 1;
        if (actual.allowed) allowed += 1;
        else denied += 1;
        if (actual.allowed !== expected) {
          mismatches += 1;
          fail(`${declaredBoundary.pattern} ${path} ${kind.id} ${actor} ${mode}: expected ${expected}, got ${JSON.stringify(actual)}`);
        }
      }
      if (broadPredecessorKinds.has(kind.id) && !kindAllowed) {
        if (actualRead.allowed || actualWrite.allowed) {
          fail(`Broad predecessor relabel escaped ${selected.pattern}: ${kind.id} / ${actor}`);
        }
        broadRelabelDenials += 2;
      }
    }
  }
}

process.stdout.write(`${JSON.stringify({
  ok: true,
  contractVersion: contract.version,
  boundaries: boundaryCases.length,
  kinds: kinds.length,
  actors: actors.length,
  modes: ["read/source", "write/target"],
  decisions,
  allowed,
  denied,
  broadRelabelDenials,
  mismatches
})}\n`);

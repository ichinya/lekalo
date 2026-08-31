#!/usr/bin/env node

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { evaluateDecision, loadTrustedContext } from "./check-privacy.mjs";

const root = fileURLToPath(new URL("../", import.meta.url));
const context = await loadTrustedContext();
const base = JSON.parse(await readFile(`${root}/tests/fixtures/privacy/allowed.json`, "utf8"))[0].decision;
const clone = (value) => structuredClone(value);
const REPO_ONE = `repo-sha256:${"1".repeat(64)}`;
const REPO_TWO = `repo-sha256:${"2".repeat(64)}`;
const roles = context.policy.vocabularies.repositoryRole;
const boundaries = context.policy.vocabularies.trustBoundary;
const relations = context.policy.vocabularies.repositoryRelation;
const tenantRelations = context.policy.vocabularies.tenantRelation;
const audiences = context.policy.vocabularies.audience;
const operations = context.policy.vocabularies.operation;

function repositoryBacked(role) {
  return !["local-workspace", "public-channel"].includes(role);
}

function oracleRepositoryContext(source, destination, audience) {
  if (source.repositoryRole === "public-channel") return false;
  if (repositoryBacked(source.repositoryRole) !== (source.repositoryRef !== null)) return false;
  if (repositoryBacked(destination.repositoryRole) !== (destination.repositoryRef !== null)) return false;
  if (destination.trustBoundary === "same-local-workspace") {
    return destination.repositoryRole === "local-workspace" && destination.repositoryRef === null
      && destination.repositoryRelation === "not-applicable" && destination.tenantRelation === "same-tenant" && audience === "operator-only";
  }
  if (destination.trustBoundary === "same-repository") {
    return repositoryBacked(source.repositoryRole) && source.repositoryRef !== null
      && destination.repositoryRole === source.repositoryRole && destination.repositoryRef === source.repositoryRef
      && destination.repositoryRelation === "same-origin" && destination.tenantRelation === "same-tenant";
  }
  if (destination.trustBoundary === "same-tenant" || destination.trustBoundary === "cross-repository") {
    return source.repositoryRef !== null && destination.repositoryRef !== null && source.repositoryRef !== destination.repositoryRef
      && destination.repositoryRole === "external-repository" && destination.repositoryRelation === "different-repository"
      && destination.tenantRelation === "same-tenant";
  }
  if (destination.trustBoundary === "cross-tenant") {
    return source.repositoryRef !== null && destination.repositoryRef !== null && source.repositoryRef !== destination.repositoryRef
      && destination.repositoryRole === "external-repository" && destination.repositoryRelation === "different-repository"
      && destination.tenantRelation === "cross-tenant";
  }
  return destination.trustBoundary === "public" && destination.repositoryRole === "public-channel"
    && destination.repositoryRef === null && destination.repositoryRelation === "not-applicable"
    && destination.tenantRelation === "not-applicable" && audience === "public";
}

function oracleProvenance(source, provenance) {
  if (provenance.repositoryRole !== source.repositoryRole || provenance.repositoryRef !== source.repositoryRef) return false;
  if (provenance.origin === "synthetic") {
    return provenance.synthetic === true && provenance.derived === false
      && source.repositoryRole === "local-workspace" && source.repositoryRef === null;
  }
  if (provenance.origin === "derived") return provenance.derived === true && provenance.synthetic === false;
  if (provenance.synthetic !== false || provenance.derived !== false) return false;
  if (provenance.origin === "consumer-repository") return source.repositoryRole === "consumer-repository";
  if (provenance.origin === "lekalo-repository") return source.repositoryRole === "lekalo-repository";
  if (provenance.origin === "external") return source.repositoryRole === "external-repository";
  return provenance.origin === "tool-runtime";
}

let destinationCases = 0;
let provenanceCases = 0;
let contradictionCases = 0;
let mismatches = 0;

for (const operation of operations) {
  for (const trustBoundary of boundaries) {
    for (const repositoryRole of roles) {
      for (const repositoryRef of [null, REPO_ONE, REPO_TWO]) {
        for (const repositoryRelation of relations) {
          for (const tenantRelation of tenantRelations) {
            for (const audience of audiences) {
              const input = clone(base);
              input.artifactKind = "fixture";
              input.exportDisposition = "public-fixture";
              input.operation.id = operation;
              input.source = { repositoryRole: "consumer-repository", repositoryRef: REPO_ONE };
              input.provenance.origin = "consumer-repository";
              input.provenance.repositoryRole = "consumer-repository";
              input.provenance.repositoryRef = REPO_ONE;
              input.provenance.synthetic = false;
              input.destination = { repositoryRole, repositoryRef, trustBoundary, repositoryRelation, tenantRelation };
              input.audience = audience;
              const coherent = oracleRepositoryContext(input.source, input.destination, audience);
              const result = evaluateDecision(input, context);
              if (!coherent) {
                contradictionCases += 1;
                if (result.output.decision !== "deny" || result.output.sourceTransferAllowed !== false) mismatches += 1;
              }
              destinationCases += 1;
            }
          }
        }
      }
    }
  }
}

for (const sourceRole of roles) {
  for (const sourceRef of [null, REPO_ONE]) {
    for (const provenanceRole of roles) {
      for (const provenanceRef of [null, REPO_ONE]) {
        for (const origin of ["synthetic", "consumer-repository", "lekalo-repository", "tool-runtime", "derived", "external"]) {
          for (const synthetic of [false, true]) {
            for (const derived of [false, true]) {
              const input = clone(base);
              input.source = { repositoryRole: sourceRole, repositoryRef: sourceRef };
              input.provenance.repositoryRole = provenanceRole;
              input.provenance.repositoryRef = provenanceRef;
              input.provenance.origin = origin;
              input.provenance.synthetic = synthetic;
              input.provenance.derived = derived;
              const coherent = oracleProvenance(input.source, input.provenance);
              const result = evaluateDecision(input, context);
              if (!coherent) {
                contradictionCases += 1;
                if (result.output.decision !== "deny" || result.output.sourceTransferAllowed !== false) mismatches += 1;
              }
              provenanceCases += 1;
            }
          }
        }
      }
    }
  }
}

assert.equal(mismatches, 0);
console.log(JSON.stringify({ ok: true, destinationCases, provenanceCases, contradictionCases, mismatches }));

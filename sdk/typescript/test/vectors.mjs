/**
 * TypeScript codec runner for the shared adapter-SDK vector corpus
 * (issue #106). Executed by `scripts/test-adapter-sdk-vectors.mjs`; it
 * proves this language's runtime against every corpus family and prints
 * one JSON verdict line. No network; no side effects beyond stdout.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import * as sdk from "../src/index.mjs";

const CORPUS = resolveCorpus();
function resolveCorpus() {
  // The repo gate runs from the checkout; a packed consumer runs from the
  // package root with `--corpus <path>`.
  const flag = process.argv.indexOf("--corpus");
  if (flag !== -1) return process.argv[flag + 1];
  return new URL("../../../tests/fixtures/adapter-sdk/", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
}

const read = (name) => readFileSync(join(CORPUS, name), "utf8");
const list = (family) => readdirSync(join(CORPUS, family)).filter((name) => name.endsWith(".json") && !name.endsWith(".expect.json")).sort();

const results = [];
const verdict = (name, ok, extra) => {
  results.push({ name, ok, ...extra });
};

for (const name of list("valid")) {
  const text = read(`valid/${name}`);
  const envelope = JSON.parse(text);
  const isResponse = envelope.status !== undefined;
  if (isResponse) {
    const decoded = sdk.decodeResponse(text);
    verdict(`valid/${name}`, decoded.ok, decoded.ok ? {} : { reason: decoded.reason });
  } else {
    const decoded = sdk.decodeRequest(text);
    if (!decoded.ok) {
      verdict(`valid/${name}`, false, { reason: decoded.reason });
      continue;
    }
    const validated = sdk.validateRequest(decoded.value);
    verdict(`valid/${name}`, validated.ok, validated.ok ? {} : { reason: validated.reason });
  }
}

for (const name of list("invalid")) {
  const text = read(`invalid/${name}`);
  const expect = JSON.parse(read(`invalid/${name.replace(/\.json$/, ".expect.json")}`));
  // The wire families are distinguishable without parsing: response
  // envelopes always carry the closed status/evidence pair.
  const isResponse = text.includes('"status"') && text.includes('"evidence"');
  const decoded = expect.stage === "validate" || !isResponse
    ? (() => {
        const request = sdk.decodeRequest(text);
        if (!request.ok) return { stage: "decode", reason: request.reason };
        const validated = sdk.validateRequest(request.value);
        if (!validated.ok) return { stage: "validate", reason: validated.reason };
        return { stage: "none" };
      })()
    : (() => {
        const decode = sdk.decodeResponse(text);
        if (!decode.ok) return { stage: "decode", reason: decode.reason };
        return { stage: "none" };
      })();
  const ok = decoded.stage === expect.stage && decoded.reason === expect.reason;
  verdict(`invalid/${name}`, ok, ok ? {} : { expect, actual: decoded });
}

for (const name of list("semantic")) {
  const vector = JSON.parse(read(`semantic/${name}`));
  const kind = vector.kind;
  try {
    let ok = false;
    const [expectKind, expectReason] = vector.expect.split(":");
    if (kind === "validate-before-write") {
      const request = sdk.decodeRequest(JSON.stringify(vector.request));
      const response = sdk.decodeResponse(JSON.stringify(vector.response));
      if (!request.ok || !response.ok) {
        ok = expectKind === "reject" && expectReason === (request.reason ?? response.reason);
      } else {
        const identity = sdk.validateResponseIdentity(response.value, {
          operation: request.value.operation,
          protocolVersion: request.value.protocol_version,
          requestId: request.value.request_id,
        });
        const payload = identity.ok ? sdk.validateResponsePayload(request.value, response.value) : identity;
        const reason = !identity.ok ? identity.reason : !payload.ok ? payload.reason : null;
        ok = reason === null ? expectKind === "accept" : expectKind === "reject" && expectReason === reason;
      }
    } else if (kind === "build-success") {
      const request = sdk.decodeRequest(JSON.stringify(vector.request));
      if (!request.ok) {
        ok = false;
      } else {
        const envelope = sdk.buildSuccess({
          adapter: sdk.buildAdapterIdentity(vector.builder.identity),
          capabilities: vector.builder.capabilities,
          ...(vector.builder.fragments ?? {}),
          request: request.value,
        });
        ok = expectKind === "accept";
        if (ok && vector.builder.fragments?.writes) {
          ok = JSON.stringify(envelope.writes) === JSON.stringify([...vector.builder.fragments.writes].sort((a, b) => (a.path < b.path ? -1 : 1)));
        }
      }
    } else if (kind === "build-error") {
      const request = sdk.decodeRequest(JSON.stringify(vector.request));
      if (!request.ok) {
        ok = false;
      } else {
        try {
          const envelope = sdk.buildError({
            adapter: sdk.buildAdapterIdentity(vector.builder.identity),
            ...(vector.builder.error ?? {}),
            request: request.value,
          });
          ok = expectKind === "accept" && envelope.status === "error";
        } catch (error) {
          ok = expectKind === "reject" && expectReason === error.reason;
        }
      }
    } else if (kind === "decode-oversize") {
      const decoded = sdk.decodeRequest(vector.text, vector.maxBytes);
      ok = !decoded.ok && expectKind === "reject" && decoded.reason === expectReason;
    } else if (kind === "canonical-plan") {
      const sorted = sdk.sortWritePlan(vector.writes);
      ok = sorted !== null && sdk.canonicalPlanId(sorted) === vector.planId;
    } else if (kind === "artifact-manifest") {
      const manifest = sdk.buildArtifactManifest(vector.entries);
      if (expectKind === "reject") ok = manifest === null;
      else ok = manifest !== null && JSON.stringify(manifest.entries) === JSON.stringify(vector.expectSorted);
    } else if (kind === "scenario-result") {
      const normalized = sdk.normalizeScenarioResult(vector.result);
      if (expectKind === "reject") ok = normalized === null;
      else ok = normalized !== null && JSON.stringify(normalized.findings) === JSON.stringify(vector.normalizedFindings);
    }
    verdict(`semantic/${name}`, ok, ok ? {} : { expect: vector.expect, kind });
  } catch (error) {
    const [expectKind, expectReason] = vector.expect.split(":");
    const ok = expectKind === "reject" && expectReason === error.reason;
    verdict(`semantic/${name}`, ok, ok ? {} : { thrown: error.reason ?? String(error) });
  }
}

const canonicalOutputs = {};
for (const name of list("canonical")) {
  const vector = JSON.parse(read(`canonical/${name}`));
  const bytes = sdk.canonicalJson(vector.value);
  const roundTrip = sdk.strictParse(bytes);
  const ok =
    bytes === vector.canonical &&
    roundTrip.ok &&
    sdk.canonicalJson(roundTrip.value) === sdk.canonicalJson(vector.value);
  canonicalOutputs[`canonical/${name}`] = bytes;
  verdict(`canonical/${name}`, ok, ok ? {} : { bytes });
}

let nfcSupported = true;
for (const name of list("security")) {
  const vector = JSON.parse(read(`security/${name}`));
  if (vector.requiresNfc === true && !nfcSupported) {
    verdict(`security/${name}`, true, { skipped: "nfc-unsupported" });
    continue;
  }
  let ok = false;
  if (vector.scopeGrammar === true) {
    const value = vector.value ?? vector.path;
    ok = sdk.isScope(value) === vector.scopeOk;
    if (ok && vector.touches !== undefined) ok = sdk.scopeTouchesProtectedHome(value) === vector.touches;
    if (ok && vector.logical !== undefined) ok = sdk.isLogicalPath(value) === vector.logical;
  } else if (vector.token !== undefined) {
    ok = sdk.isToken(vector.token) === vector.tokenOk;
  } else if (vector.capability !== undefined) {
    ok = sdk.isCapabilityId(vector.capability) === vector.tokenOk;
  } else {
    const pathValue = vector.path ?? vector.value;
    ok = sdk.isLogicalPath(pathValue) === vector.logical;
    if (ok && vector.home !== undefined) ok = sdk.protectedHome(pathValue) === vector.home;
    if (ok && vector.scope !== undefined) ok = sdk.scopeCovers(vector.scope, pathValue) === vector.covers;
  }
  verdict(`security/${name}`, ok, ok ? {} : { vector });
}

for (const name of list("upgrade")) {
  const vector = JSON.parse(read(`upgrade/${name}`));
  const decoded = sdk.decodeRequest(JSON.stringify(vector.request));
  let accepted = decoded.ok;
  let reason = decoded.ok ? null : decoded.reason;
  if (decoded.ok) {
    const validated = sdk.validateRequest(decoded.value);
    accepted = validated.ok;
    reason = validated.ok ? null : validated.reason;
  }
  const ok = vector.accept === accepted && (vector.accept || vector.reason === reason);
  verdict(`upgrade/${name}`, ok, ok ? {} : { accept: accepted, reason, expect: vector });
}

const failed = results.filter((entry) => !entry.ok);
const summary = {
  ok: failed.length === 0,
  language: "typescript",
  total: results.length,
  failed: failed.length,
  canonicalOutputs,
};
if (failed.length > 0) summary.failures = failed;
process.stdout.write(`${JSON.stringify(summary)}\n`);
if (failed.length > 0) process.exit(1);

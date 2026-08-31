# Privacy policy migration: 1.0.3 to 1.0.4

Policy `1.0.3` is frozen byte-for-byte and yanked after the fourth independent
review. Its exact semantic identity is
`sha256:868ced73748caa4ad80a74df0e5d6ad8d3c5463a6593caa4ead09d872160280f`;
its raw policy hash is
`9179ced3d5d9c07f2fb9ed5bb7c40c5c5c1e072bb65eb077229fb040c55cac1c`;
and its accepted-manifest raw hash is
`fe58cfc9fe3323b5fdb0d9be9f1610c50681e376e7fe9529812f117e80b5f112`.
Those old files and sidecars are immutable history, not selectable current
policy.

Policy `1.0.4` is a new accepted corrective successor. Product release remains
`0.0.1`; only an independent PASS and closure of issue #120 can advance the
product to `0.0.2`. Policy, schema and decision versions are independent of the
product release.

## Exact successor identities

| Component | Exact identity or raw hash |
|---|---|
| Policy | `dev.lekalo.privacy-export-policy@1.0.4@sha256:259cf596fcdc38423fa45d1df0937e85591569621c481197f3940f894bbc4ce5` |
| Policy raw bytes | `76702466ddd1d54f1c542e73b63995de7dcc81861bc641914e4bf862623bbed1` |
| Decision contract | `dev.lekalo.privacy-export-decision@1.3.0` |
| Input schema | `dev.lekalo.privacy-export-input-schema@2.3.0`, raw `8466cf55b6494e318c3daf5006ddc2d3a0e4f4a79b2bfa805ce1fe651b0f6b06` |
| Output schema | `dev.lekalo.privacy-export-output-schema@1.3.0`, raw `8518695095fb41eba7baf45ca948e48f59e8afb179f1c6a98ea3fa5906f10085` |
| Classification contract | Unchanged exact `dev.lekalo.privacy-classification-decision@1.0.0` |
| Authorizing-evidence contract | `dev.lekalo.privacy-authorizing-evidence@1.0.0`, raw `5fc226843be9913327586ec02603eddec5dc7ea8dfdc066dc6bfb132c09187b6` |
| Accepted manifest | Format `1.3.0`, raw/trust anchor `82ad6a16080dbe7fe8555d6726312f7123a6e0682d50d4ed30d0ce95d4fdc026` |

## Wire changes

The input now requires exact `authorizingEvidenceContractRef` and opaque
`artifactRef`. Generic permission/license/consent and decision audit references
are removed from authorizing positions. They are replaced by separate closed
records for public-fixture permission, public-fixture license, public-fixture
consent, consumer-repository ACL permission, consumer-repository consent,
general export transfer consent, conflict resolution, declassification and
aggregation.

Each record carries the exact evidence-contract triple, constant field-specific
kind and purpose, an allowed outcome, opaque evidence identity,
verification/freshness state and exact declared decision-context binding. One
identity cannot satisfy multiple purposes. Wrong contract, version, shape or
purpose is malformed. Missing required evidence, stale/expired/unverified
status, wrong outcome, repeated identity or binding mismatch deterministically
denies.

The output adds the exact authorizing-evidence contract to `effectiveRefs`.
Decision/input/output versions advance because the wire shapes and semantics
changed. The classification contract remains byte-exact `1.0.0`.

## Trust and compatibility boundary

The default checker trusts only the exact accepted `1.0.4` manifest anchor.
Selecting the old exact `1.0.3` manifest, changing old or current bytes,
recomputing sidecars, or substituting an evidence registry fails before
decision evaluation.

Trusted #119 runtime envelopes and #89 integration adapters must mint opaque
evidence and repository tokens, bind them to physically resolved authorized
context, verify authoritative sources and freshness, and fail closed when that
verification is unavailable. The #120 evaluator remains offline and checks
only declared exact metadata and coherence; it does not prove physical or
cryptographic authenticity.

Consumers must migrate atomically to policy `1.0.4`, decision `1.3.0`, input
schema `2.3.0`, output schema `1.3.0`, and the exact evidence registry. No local
vocabulary override or compatibility alias is accepted.

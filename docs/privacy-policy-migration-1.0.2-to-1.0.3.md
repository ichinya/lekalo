# Privacy policy migration: 1.0.2 to 1.0.3

Policy `1.0.2` was accepted and then frozen with
`silentMutationAllowed:false`. A third independent review found three semantic
false-allows, schema/runtime well-formedness divergence, and an undocumented
repository-token evidence boundary. Its exact bytes were not edited. Manifest
`1.2.0` records it once as `yanked-after-third-independent-review`.

Policy `1.0.3` is a new accepted corrective successor. This is a privacy
contract release, not a product release: the accepted product remains `0.0.1`.
Only a later PASS and closure of issue #120 can produce product `0.0.2`.

## Exact successor identities

| Contract | Identity or raw SHA-256 |
|---|---|
| Policy | `dev.lekalo.privacy-export-policy@1.0.3@sha256:868ced73748caa4ad80a74df0e5d6ad8d3c5463a6593caa4ead09d872160280f` |
| Policy file | `9179ced3d5d9c07f2fb9ed5bb7c40c5c5c1e072bb65eb077229fb040c55cac1c` |
| Decision contract | `dev.lekalo.privacy-export-decision@1.2.0` |
| Input schema | `dev.lekalo.privacy-export-input-schema@2.2.0`, raw `61a8aed3294a19a3e416903e08a1a85c32f6d5f1bfd3ef433a128b670b8d0432` |
| Output schema | `dev.lekalo.privacy-export-output-schema@1.2.0`, raw `163343801477c4b3515493a7a85fa53a0f5e40ee02e532b00d37afe5bf817492` |
| Classification contract | Reused byte-exact `dev.lekalo.privacy-classification-decision@1.0.0@sha256:58626d1889990bf6120874fcd194c9fc68f2d81f05af1f3b8f7d04336fb3aa9e` |
| Accepted manifest | Format `1.2.0`, raw/trust anchor `fe58cfc9fe3323b5fdb0d9be9f1610c50681e376e7fe9529812f117e80b5f112` |

The policy digest remains the SHA-256 of the deterministic canonical policy
projection with only `policyRef.digest` omitted. The manifest separately pins
exact file bytes; a recomputed successor or caller-selected predecessor is not
trusted.

## Wire and behavior changes

- Provenance modes are exhaustive and exclusive. Derived means
  `derived:true, synthetic:false`; synthetic means
  `synthetic:true, derived:false`; other origins require both false.
- Every non-synthetic public fixture requires exact permission, license and
  consent for `repository-store`, `transfer`, and `publish`.
- Every consumer-repository-only operation requires an originating consumer
  source/provenance role, coherent non-null repository ref and ordinary
  consumer origin. Repository storage additionally requires same
  repository/origin/tenant, ACL permission and consent.
- Nested authority, policy, classification, evidence, grant,
  declassification and aggregation refs now have identical strict runtime and
  schema shapes. Conflict state and decision-ref coupling is exact.
- Exact duplicates in set-like input arrays are malformed. Same opaque source
  or transform identity with different content is a semantic conflict and
  denies after shape acceptance.
- Repository-ref equality remains declared metadata coherence only. Trusted
  #119 runtime envelopes and integration adapters such as #89 must mint, bind
  and freshness-check tokens from physically resolved repository context.
  Missing, stale or unverified binding fails closed before invocation or
  decision acceptance.

Consumers must emit decision contract `1.2.0`, policy ref `1.0.3`, and input
schema `2.2.0`. Old `1.0.2` refs and manifest selection fail during custody or
shape validation; no compatibility alias is provided.

## Frozen predecessor custody

The following predecessor identities remain byte-exact:

- policy ref `dev.lekalo.privacy-export-policy@1.0.2@sha256:207117a6a064c2341d95087b208b8dbc7f0953be08eb8c59b5da7eb905e25be1`;
- policy raw `de8f7495087d8b2890ed00efddc448f99563f32c68e753b43dc73646ab7e5719`;
- manifest raw `5739d80bde351b85c6ba8b7eedff1ef25f562c6649a7a1364027d4a308b4d530`;
- input schema `2.1.0` raw `7f1eb0baa64e9197dc78f433b325f7eb584f47b37416cbb0e0970306aefc631c`;
- output schema `1.1.0` raw `c6accaff73dd29592103eefe3a1494ea3111e7a35ee7eafaae0ecb7ab2c7cc7b`.

Policy `1.0.1` remains yanked and `1.0.0` remains rejected-unaccepted. The
accepted manifest contains exactly one current accepted policy: `1.0.3`.

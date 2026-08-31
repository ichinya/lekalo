# Privacy policy migration: 1.0.1 to 1.0.2

Status: normative lifecycle and wire-contract migration for issue #120.

Product release remains `0.0.1`. This migration changes privacy-contract
identities only; product `0.0.2` is conditional on cold-review PASS and issue
acceptance.

Superseded: `1.0.2` was later yanked after the third independent review; the
ladder continues through `1.0.6`, which is the accepted policy. Issue #120
closed on 2026-08-30 and the accepted product release is `0.0.2`.

## Preserved predecessor custody

Policy `1.0.1` is frozen and rejected after corrective review:

- policy ref `dev.lekalo.privacy-export-policy@1.0.1@sha256:f8faab0908fb1bc2da8c173969921000d6424a1f900ab0b04f2b57fa058d0ffb`;
- policy raw SHA-256 `462df7a5c92b676deda7d315e9304695f3563295ffa41775246341fb99970622` at `contracts/privacy-policy.v1.json`;
- manifest raw SHA-256 `f74df7390b44c56489ead001e146c4cde43f7c2d44e2b0163c5c6d1dba43eeed` at `contracts/privacy-policy.v1.manifest.json`;
- input schema `2.0.0`, raw SHA-256 `ba8239dac1ffc203993bb4564e9f9a6dfa16b2b92cab9e7952aefeb74a4e0e4c`;
- output schema `1.0.0`, raw SHA-256 `43b9c5e1ed6a4847c53a13223376a959c999c21b1c9f2de71ab20b33b8f4f2f1`.

Those files and sidecars remain byte-for-byte unchanged. They are not accepted
startup inputs. The accepted manifest records them once under
`yankedCandidates`, with `accepted:false`.

## Accepted successor

- policy `dev.lekalo.privacy-export-policy@1.0.2`;
- decision contract `dev.lekalo.privacy-export-decision@1.1.0`;
- input schema `dev.lekalo.privacy-export-input-schema@2.1.0`;
- output schema `dev.lekalo.privacy-export-output-schema@1.1.0`;
- classification contract and schema `1.0.0`.

Accepted exact raw custody:

| File | SHA-256 |
|---|---|
| `contracts/privacy-policy.v1.0.2.json` | `de8f7495087d8b2890ed00efddc448f99563f32c68e753b43dc73646ab7e5719` |
| `contracts/privacy-policy.v1.0.2.classification.json` | `58626d1889990bf6120874fcd194c9fc68f2d81f05af1f3b8f7d04336fb3aa9e` |
| `contracts/privacy-export.schema.v2.json` | `7f1eb0baa64e9197dc78f433b325f7eb584f47b37416cbb0e0970306aefc631c` |
| `contracts/privacy-export.schema.v2.output.json` | `c6accaff73dd29592103eefe3a1494ea3111e7a35ee7eafaae0ecb7ab2c7cc7b` |
| `contracts/privacy-export.schema.v2.classification.json` | `b996f62f23eb3341518ba3c4d917757814d27200c2ab410f2c127b4f161d50cb` |
| `contracts/privacy-policy.v1.0.2.manifest.json` | `5739d80bde351b85c6ba8b7eedff1ef25f562c6649a7a1364027d4a308b4d530` |

The wire changes are intentionally versioned:

1. source, destination and provenance gain an opaque nullable
   `repositoryRef`; repository-backed roles use `repo-sha256:<64 hex>`;
2. provenance classification and every derived source use an exact
   classification contract triple plus opaque decision identity;
3. every derived source gains `sourceRef: source-sha256:<64 hex>`;
4. nested source sensitivity/disposition vocabularies become closed enums;
5. output effective refs gain the classification contract;
6. repository/origin cross-field contradictions, duplicate/conflicting source
   identities and Win32 compatibility aliases fail closed.

Consumers must not translate stale `1.0.1` inputs silently. They must construct
new `2.1.0` inputs with exact current refs and reevaluate them. There is no
accepted broadening grant and no local vocabulary override.

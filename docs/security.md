# Security and privacy boundaries

Status: **Implemented** exact-custody privacy/authority and bounded confinement; public aggregate metrics export is **Planned**. Owner: privacy and adapter-host maintainers. [#119](https://github.com/ichinya/lekalo/issues/119), [#120](https://github.com/ichinya/lekalo/issues/120), [privacy ADR](adr/0002-privacy-export-policy.md), [classification ADR](adr/0043-data-classification-security-gates.md).

The selected authority and privacy policy versions are both `0.3.2`; accepted exact refs live in [authority manifest](../contracts/authority-contracts.manifest.json) and [privacy manifest](../contracts/privacy-policy.v0.3.2.manifest.json). A caller-recomputed digest does not admit modified policy. [Privacy policy](privacy.md), [runtime enforcement](privacy-runtime.md), [classification](classification.md), [authority](authority.md).

## Threats and controls

| Input or action | Control and limit |
| --- | --- |
| Model/import/root selectors | Closed shapes, bounded reads, physical path containment and link/alias refusal. |
| Adapter or native execution | Explicit trust/profile capabilities and bounded process scopes; no generic shell execution from returned manifest text. |
| Generate/apply/clean | Digest-bound plan, before-state checks, staged outputs and eligible artifact lifecycle; maintained source is not blanket write scope. |
| Source/prompts/raw logs/credentials/PII | Local custody and explicit classification/export admission; redaction and a hash alone are not authorization. |
| Public tutorials/results | Synthetic provenance, consumer role aliases, no real tenant records, private URLs, source inventory or raw history uploads. |

Validation/provider discovery and scanning are not network publication. Scanning never installs packages or executes the consumer. Native dependency provisioning and synthetic migrations are visible separate setup/test steps. A live database test is allowed only against its isolated tutorial service, never a user-supplied production connection URL.

## Check selected policy and fixture provenance

**Implemented.** From the repository root:

```sh docs-example=security-checks
node scripts/check-authority.mjs
node scripts/check-privacy.mjs
node scripts/test-fixture-provenance.mjs
```

Expected: exit 0, accepted authority/privacy `0.3.2` and complete synthetic family coverage. These are Node checker protocols, separate from the Rust CLI envelope. A policy refusal or missing provenance is a failed admission, not a reason to recompute accepted refs.

[Confinement](adapter-confinement.md) describes OS prerequisites and refusals; [adapter installation](adapter-install.md) describes trust, quarantine and lifecycle. Missing confinement/toolchains remain unavailable. Successful example replay is local verification and does not establish production acceptance.

Public documentation uses only `greenfield-consumer`, `brownfield-consumer`, `workflow-consumer` and `validation-consumer` roles for applications. Public library/vendor links and fictional fixture package names are reference identities, not private applications. The ownership gate scans public prose for credential/private-domain canaries; manual review is still required to detect unknown private identities.

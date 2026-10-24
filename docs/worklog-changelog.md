# Worklog and opt-in Changelog pilot

Status: **Implemented** bounded pilot with contract family `0.8.0`. Owner: pilot contract and admission maintainers. The pilot composes independent task completion, private worklog, provider outbox/delivery and opt-in public Changelog operations. It does not publish to a real provider.

## Commands

```text
lekalo pilot validate <PATH> --project DIR
lekalo pilot admit <PATH> <COMPOSITION> --profile strict|permissive --project DIR
```

`validate` reads a composition document and validates its bindings against the project's compiled Model and consumed attachments. `admit` reads an evidence report plus the exact composition document, checks their binding and derives admission. Both honor the global `--json` and `--no-cache` options. The [CLI index](cli.md#command-index) records the complete synopsis.

The CLI maps valid results to exit `0`, invalid input to `1`, denied results to `3` and unsupported results to `4`. An unsupported admission retains its evidence in the failure envelope's `payload`; it must not be treated as a successful strict admission. See [diagnostics](diagnostics.md) for result streams and envelopes.

## Contracts and ownership

| Document | Purpose |
| --- | --- |
| [Pilot composition](../contracts/worklog-changelog-pilot.schema.v0.8.0.json) | Operation roles, declared effects, commit boundaries, independence, approval/snapshot bindings, public allowlist, replay scopes and target capability requirements. |
| [Evidence](../contracts/worklog-changelog-evidence.schema.v0.8.0.json) | Composition and toolchain pins, target execution, capabilities, scenarios, consumer witnesses, completeness and verdicts. |
| [Consumer oracle fixture](../tests/fixtures/pilot/worklog-changelog/evidence/consumer-oracle.json) | Expected public-schema consumers and impact witnesses. |
| [Port fixture](../tests/fixtures/pilot/worklog-changelog/php/port-manifest.json) | Bounded target backend declaration. |
| [Refusal fixture](../tests/fixtures/pilot/worklog-changelog/go/refusal.json) | Explicit unsupported backend evidence. |

Core admission is owned by `crates/lekalo-core/src/worklog_changelog/`; CLI loading and execution are owned by `crates/lekalo-cli/src/main.rs`. The `LEK-WC-001` through `LEK-WC-007` diagnostics resolve through the `0.8.0` registry successor. Unchanged predecessor contracts retain their exact family pins.

## Validation and admission

Composition validation checks closed documents, duplicate keys, project and IR custody, declared operation/effect bindings and consumed attachment pins. Independent concerns must not share effect identities or infrastructure effects. Correlation and replay keys resolve against actual operation inputs; replay scopes remain payload-bound. The public DTO allowlist excludes private fields.

Completion, worklog, outbox, delivery and publication have separate declared commit boundaries. External calls do not acquire local atomicity by declaration. Public publication requires preview-bound approval and a revisioned snapshot. Scenarios cover stale previews, repeated correlation keys, unsafe content, partial failure, withdrawal and republishing.

Admission re-derives capability obligations, scenario/assertion rows, target membership, consumer closure and verdicts from the pinned composition and compilation. Missing targets and invented witnesses refuse. Strict admission cannot pass when required evidence is unsupported, unknown or unexecuted; permissive admission retains degraded or unsupported rows and blockers.

The eligible PHP lane is bound to the reviewed implementation embedded in the CLI. Admission executes a private copy of that implementation; a neighboring file's self-reported digest or a fabricated passing report cannot grant eligibility. Changing the backend requires a reviewed engine rebuild. The engine, host and interpreter remain the trusted execution boundary; this is not remote attestation.

The PHP journal serializes durable request/claim transitions, arbitrates replay keys against current state and refuses ambiguous delivery, malformed recovery records and unavailable recovery sources. A torn unterminated tail is handled before the next acknowledged commit. These mechanisms qualify the modeled local journal and scripted transport profile, not real provider-side idempotency, deployed simultaneous processes or physical power-loss recovery.

## Reproduction

Synthetic inputs live under [tests/fixtures/pilot/worklog-changelog](../tests/fixtures/pilot/worklog-changelog): the neutral Model, consumed attachments, Scenario IR corpus, evidence, public DTO and PHP/Go target declarations. The maintained journal regression is [tests/pilot/worklog-changelog-journal.php](../tests/pilot/worklog-changelog-journal.php).

Build the matching locked CLI and provision the dependencies selected by [CI](../.github/workflows/ci.yml) before running the gates:

- [Contracts](../scripts/test-worklog-changelog-contracts.mjs) checks closed schemas, registry union, fixture pins and corpus parity.
- [Pilot](../scripts/test-worklog-changelog-pilot.mjs) exercises target/profile admission and retains expected denials.
- [Admission](../scripts/test-worklog-changelog-admission.mjs) checks real backend execution, provenance and hostile report controls.
- [Impact](../scripts/test-worklog-changelog-impact.mjs) checks exact consumer closure and mutations against the oracle.
- [Neutrality](../scripts/test-worklog-changelog-neutrality.mjs) checks the public synthetic artifact inventory.

The `gen-worklog-changelog-*.mjs` scripts are explicit fixture/contract writers. Review generated bytes and pins; a generated report is not an execution receipt. See [script maintenance](../scripts/README.md).

Real provider integration, live publication, private requirements and native Go runtime acceptance remain outside this pilot's qualified profile. Unsupported Go evidence remains visible; PHP success never supplies missing evidence for another target.

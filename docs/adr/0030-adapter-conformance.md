# ADR-0030: the target adapter conformance suite

## Status

Accepted (issue #31). Reserved number, consumed by this issue's owner
decisions.

## Context

Issues #27/#28/#29 publish the target process protocol, capability
discovery, and composable profiles, but nothing verifies that an
arbitrary adapter actually honors the contract. Acceptance for issue #31
requires one shared battery: any target adapter must pass the same
contract tests before it is recognized as compatible with an exact
protocol/IR version pair, locally and in CI, with JSON/JUnit results,
determinism detection, uncompensable hard failures, and an
exact-version badge.

## Decisions

1. **The suite drives the production client.** Every check runs through
   `TargetClient` and the confined sandbox, never through a parallel
   reimplementation or an unconfined fallback. What the suite verifies
   is what generation will execute.
2. **One closed catalog with fixed order.** Eighteen checks, one
   inherent class each, stable dotted ids. Reports may carry only these
   rows in this order; the verdict is a pure fold, so two runs of the
   same adapter produce the same verdict.
3. **Failure class travels with the outcome, not the check.** A crash
   during a feature check is a process-class failure; a refusal during
   any exchange is security-class. This is what makes hard failures
   uncompensable: the verdict fold ranks security > protocol > process
   > feature/determinism, and passing feature tests never offset a hard
   class.
4. **Fail-closed fixture custody.** The suite runs only against its own
   hermetic fixture project, materialized into a fresh private root.
   The scenario fixture is decoded through the production Scenario IR
   module before any adapter launches; a custody failure is an
   infrastructure error, never a silent pass and never a user-project
   read.
5. **Determinism by repetition, byte-exactly.** Repeated describes,
   dry-runs, and read operations must produce byte-identical canonical
   responses; the repetition count and deadline are bounded caller
   options so a nondeterministic or hung adapter is classified in
   seconds.
6. **The badge names exact versions only.** A passing run with no
   skipped core rows earns `verified` for the negotiated protocol
   version and the core IR version, spelled exactly. No badge covers a
   declared range, a wildcard, or a session the battery did not run.
7. **Reports are deterministic artifacts.** The JSON envelope and the
   JUnit projection carry no timestamps, durations, host paths, or raw
   child output; JUnit exists for CI ingestion and prints on stdout for
   every completed run, with the exit code still owned by the verdict.
8. **Suite-level diagnostics join the registry as 1.15.0.** Four closed
   `adapter.*` rules (`LEK-ADP-002..005`) project the failure classes
   onto the accepted diagnostic wire; every predecessor entry survives
   verbatim and the registry ladder stays additive.

## Consequences

`lekalo adapter test` is the first production consumer of the #27
client surface. The suite defines qualification, not distribution: a
badge is evidence recorded in a report, and wiring it into the lock or
an adapter catalog belongs to their own issues. Scenario execution
backends (#107/#47/#56) remain separate; the suite owns fixtures,
normalization assertions, and process conformance only.

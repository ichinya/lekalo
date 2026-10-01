# CI reports: the closed report surface (issue #103)

> Версионирование обновлено: контракт при изменении получает текущую версию
> проекта. Исходная точка — 0.2.16; старые схемы и миграции удалены.

Lekalo makes headless CI a first-class consumer: the four CI-surfaced
commands emit one closed, versioned, deterministic machine-readable report
per run, plus pure projections to the formats CI systems read. The report
is a side channel; the status-owned stream and the documented exit policy
of every command stay byte-stable.

## Contract

One report document per run:
[`contracts/ci-report.schema.v0.6.3.json`](../contracts/ci-report.schema.v0.6.3.json)
(discriminator `lekalo/ci-report/v0.6.3`, identity
`dev.lekalo.ci-report@0.6.3`). Canonical form is compact JSON with
byte-sorted object keys and exactly one trailing LF. Report bytes are
deterministic: no timestamps, durations, wall-clock values, absolute
paths, drive spellings, host or user identity, environment values, or raw
child output are representable. The document binds:

- `producer` — the exact product version;
- `invocation` — the closed command name, mode, targets, modules, and the
  locked demand;
- `provenance` — the exact git/model/IR/lock/profile/adapter pins, each a
  value state (`known` carries the complete pin; `unknown` carries only a
  closed reason and never a fabricated value; a missing lock is
  `unknown/absent`, never the hash of an empty file);
- `commandResult` — the underlying domain status and its stable exit,
  preserved verbatim;
- `evaluation` — the authoritative gated outcome (status, exit, verdict
  `ready|degraded|blocked`, coverage, completeness);
- `checks`/`suites` — every check and case row with the closed
  source-outcome, failure-class, and effective-outcome vocabularies;
- `diagnostics` — the source diagnostics in normalized order (the closed
  `lekalo/diagnostic/v0.2.16` items), referenced by index;
- `publication` — the `ci-derived` classification and the publication
  decision.

## Commands and flags

```sh
lekalo validate [--strict] --report-file out/report.json [--report-format json|junit|sarif|md]
lekalo generate --check [--locked] --report-file out/drift.json
lekalo verify --locked --report-file out/verify.json
lekalo readiness --phase release --check --report-file out/readiness.json
```

`--report-file PATH` requests the side channel; `--report-format` selects
the projection (`json` is the default for a named file). A format without
a path is the stable usage failure. Report writes are confined: the
destination must be a regular file in an existing directory, links are
refused, and only the granted report path itself is ever touched. A
refused write is the typed `ci.report-write-failed` unavailable envelope
(exit 4): a report failure after a passing run surfaces as exit 4; after a
failing run the command's own failure stays on its status-owned stream and
the report failure is not silently swallowed.

## Exit policy

The producing command exits with the **evaluation**, never blindly with
the legacy envelope:

- required failure (or a genuine optional failure, or a denial, or a
  cancellation) → the classified nonzero domain status;
- optional unavailable/unsupported rows are policy-driven — under the
  default policy they warn (exit 0, incomplete coverage); a future
  versioned CI policy may promote them (`strict`) or skip them
  (`lenient`);
- the underlying command result is preserved in `commandResult` so no
  consumer ever loses the original observation.

`lekalo readiness` keeps the doctor contract by default: the report is the
product and exits 0 whenever produced. `readiness --check` is the gate:
a blocked required panel fails the run with the classified
`unavailable` status (exit 4) and `ci.required-check-missing`.

`lekalo verify` retains its typed evidence on every outcome: the blocked
verdict no longer discards the component receipt; the CI report projects
the captured component rows (with their failure classes) and the scenario
suite even when the aggregate envelope is what the user sees.

## Projections

- **JSON** — the canonical report itself.
- **JUnit XML** — one `<testsuite>` per report suite plus a synthetic
  gate suite for the check rows; scenario and case counts are assertion
  counts; `<failure>` carries evaluated assertion classes, `<error>`
  carries infrastructure/required-unavailable/denied classes, policy
  allowed absences are `<skipped>`. No `<system-out>`/`<system-err>`, no
  source snippets, no timing.
- **SARIF 2.1.0** — one run with a Lekalo tool driver; rules are derived
  from the diagnostic registry (sorted by the immutable `LEK-*` code),
  results carry repository-relative safe paths under the `%SRCROOT%` base
  id with one-based Unicode-scalar positions, and the closed Lekalo
  property projection binds the report digest, status, exit, and verdict.
  The absolute checkout URI is deliberately omitted (permitted by the
  SARIF specification).
- **Markdown** — the concise job summary: verdict/exit, counts, the exact
  revision pins, and a bounded diagnostic table; all borrowed text is
  escaped and truncation is explicit.

All projections are pure functions of the report; the same report bytes
always render the same projection on every OS.

## Verification and reporting are different facts

A successful report write never means the check passed, and a rendered
document is never itself a published summary: appending bytes to
`$GITHUB_STEP_SUMMARY`, uploading artifacts, or opening a merge check
belongs to the calling workflow. The in-repository examples of that
caller surface live in [the action design](adr/0048-lekalo-action.md).

## Privacy

Report content reuses the diagnostic allow-lists: logical paths only,
bounded structured data, registry-approved text. Secret material never
enters a report because it never enters a diagnostic. The document is
classified `ci-derived`; publication decisions belong to the caller, and
local run history stays export-ineligible. CI gating per concern stays
with `validate`, `lock --check`, `generate --check`, and
`readiness --check`.

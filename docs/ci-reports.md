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
lekalo validate [--strict] --report-file out/report.json [--report-format json|junit|sarif|md] [--ci-policy default|strict|lenient]
lekalo generate --check [--locked] --report-file out/drift.json [--ci-policy POLICY]
lekalo verify --locked --report-file out/verify.json [--ci-policy POLICY]
lekalo readiness --phase release --check --report-file out/readiness.json [--ci-policy POLICY]
```

`--report-file PATH` requests the side channel; `--report-format` selects
the projection (`json` is the default for a named file; a format without
a path is the stable usage failure). Report writes are confined: the
destination must be a new or exactly-empty regular file in an existing
directory; traversal spellings (`out/../…`), case-variant spellings of
protected homes, and links (symbolic links and Windows junctions alike)
are refused after the destination is resolved through its deepest
existing ancestor; and the protected project homes (`lekalo/`,
`.lekalo/`, `apps/`) are never written. Only the granted report path
itself is ever touched: a refused write is the typed
`ci.report-write-failed` diagnostic — after a passing run as the
unavailable envelope (exit 4), after a failing run appended to the
command's own envelope under its status, so the refusal is observable
on every command class.

## Exit policy

Under a gate — `validate`, `generate --check`, `verify`, and
`readiness --check` — the producing command exits with the
**evaluation**, never blindly with the legacy envelope:

- required failure (or a genuine optional failure, or a denial, or a
  cancellation) → the classified nonzero domain status;
- optional unavailable/unsupported rows are policy-driven — under the
  default policy they warn (exit 0, incomplete coverage); `--ci-policy
  strict` promotes them to the unavailable class (exit 4) and
  `--ci-policy lenient` skips them (exit 0); a future versioned
  CI policy file may replace the closed three-level built-in;
- the underlying command result is preserved in `commandResult` so no
  consumer ever loses the original observation.

`lekalo readiness` keeps the doctor contract by default: the report is the
product and exits 0 whenever produced. Such an informational artifact
records the evaluation the gated run would exit with — a consumer that
gates on the report reads the same verdict the `--check` form would
have enforced; the informational process exit stays 0 by this
documented contract, and it is the only case where the process exit
and the recorded evaluation differ. `readiness --check` is the gate:
a blocked evaluation — a required blocked **or degraded** row — fails
the run with the classified `unavailable` status (exit 4) and
`ci.required-check-missing`, exactly as the artifact records.

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
  results carry safe paths relative to the resolved project root (the
  invocation-relative project, which in a monorepo may be a workspace
  under the repository) under the `%SRCROOT%` base id with one-based
  Unicode-scalar positions, and the closed Lekalo property projection
  binds the exact report digest (`lekaloReportDigest` is the SHA-256 of
  the emitted JSON bytes of the same run), status, exit, and verdict.
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
bounded structured data, registry-approved text — secret material never
enters a diagnostic at construction. Defense in depth: every rendered
projection is scanned again at the sink (`scan_rendered`) before any
byte is written, and a secret-shaped token that survives redaction
refuses the report write (`ci.report-write-failed` with the
`secret-token` detail) instead of publishing. The document is
classified `ci-derived`; publication decisions belong to the caller, and
local run history stays export-ineligible. CI gating per concern stays
with `validate`, `lock --check`, `generate --check`, and
`readiness --check`.

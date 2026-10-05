# ADR-0048: The official `ichinya/lekalo-action` composite design

Date: 2026-10-01
Status: accepted for issue #103

## Context

Issue #103 asks for the official GitHub Actions workflow entry:

```yaml
- uses: ichinya/lekalo-action@v1
  with:
    command: verify
    locked: true
```

The research (docs/m7/issue-103-research.md) recommends a separate
public JavaScript repository. This delivery consciously lands the
**composite-action design** in this repository instead: the CLI already
carries every decision (the closed report contract, the exit policy, the
provenance capture), so the action is a thin, reviewed launcher, and a
composite keeps the dogfooding loop inside the compiler repository until
the public `@v1` tag ships. No second launcher is maintained.

## Decision

### 1. Shape: composite action, source-of-truth in this repository

The action is a composite (`action.yml` with `runs.using: composite`)
that invokes the `lekalo` binary the calling workflow provides or pins.
The design (this ADR) is normative for the future public
`ichinya/lekalo-action` repository: when it is extracted, the same
input/output contract, command allow-list, and finalization rules apply,
and the minimal public entry above works unchanged.

### 2. Closed inputs, argv handoff, no shell

| Input | Meaning | Default |
| --- | --- | --- |
| `command` | allow-list: `validate`, `generate-check`, `verify`, `readiness` | (required) |
| `locked` | `--locked` where applicable | `true` |
| `strict` | `validate --strict` | `false` |
| `phase` | readiness phase | `release` |
| `report-formats` | comma subset of `json,junit,sarif,md` | `json` |
| `report-dir` | the granted report output directory (fresh runner-temp recommended) | `$RUNNER_TEMP/lekalo-reports` |
| `check` | readiness gate mode (`--check`) | `true` |
| `timeout-minutes` | step timeout | `15` |
| `lekalo-path` | the confined dev input: a same-repository just-built binary | — |

`generate-check` expands to `generate --check`. Every input becomes one
argv element (or a validated flag), never shell-interpolated; the
composite runs each command with `shell: bash` and explicit argument
arrays only. No input reaches `update`, `lock` (creation), `migrate`,
`clean`, `init`, `doctor --fix`, or any other mutating surface: the
action is check-only by construction.

### 3. Outputs and finalization

Outputs: `exit-code`, `status`, `verdict`, `coverage`,
`report-json`, `report-sarif`, `report-junit`, `report-md`,
`report-digest`. The step emits bounded `::error`/`::warning`/`::notice`
annotations from the report's normalized diagnostics with
repository-relative `file` values, appends the Markdown projection to
`$GITHUB_STEP_SUMMARY`, and only then fails the step with the evaluated
exit code. A failing check still produces its report, annotations, and
summary (the finalization path runs on both outcomes); an upload or
summary failure never resets the check failure.

### 4. Security posture

- Default jobs run on `pull_request` with `contents: read`,
  `persist-credentials: false`, and no secrets; forks and Dependabot
  never execute `pull_request_target` with candidate code.
- SARIF file generation is permission-independent; SARIF **upload** is an
  explicit workflow choice (`security-events: write`), pinned to a
  reviewed full commit SHA, with a distinct analysis category per
  matrix leg.
- Cancellation forwards to child processes; a hard kill cannot fabricate
  a completed report — a missing terminal report is never success.
- No cache restore is ever accepted as check completion; caches key on
  exact tool/lock/profile hashes, never selectors.
- The `lekalo-path` dev input is confined to same-repository dogfood
  workflows (the public example never uses it; consumers get a pinned
  released binary through the future launcher repository).

### 5. In-repository examples

The `examples/ci/*.yml` workflows (added with the public action) cover
the three fixtures: the core gates, the Node consumer scenario suite,
and the Laravel fixture. Every example pins immutable action SHAs and
carries a negative control (an intentionally failing fixture that must
fail the consumer gate).

## Consequences

- The CLI owns all semantics; the action stays reviewable shell.
- The public `@v1` remains a separate reviewed release; until then the
  composite in this repository is the working reference and the
  dogfood path for the repository's own CI (issue #103 report uploads).
- The `--report-file`/`--report-format` flags, the closed report
  contract, and the readiness `--check` gate are the complete action
  contract; adding one is a versioned contract change, not an action
  upgrade.

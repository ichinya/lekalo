# Laravel storage migrations

Issue #57: `lekalo` generates safe Laravel migrations from the
language-independent storage model and its semantic diff. The pipeline
is **core plans, PHP emits, nothing executes automatically**.

## Pipeline

```text
storage-projection (base, candidate)        # declared facts
storage-engine profile                      # postgres pin + policies
        │
        ▼
core: plan_with_history / plan              # deterministic SQL steps
        │                                   # gated: destructive ∪ backfill
        ▼
core: laravel-migration-input               # canonical bounded document
        │
        ▼
php-laravel adapter: generate               # .php migration artifacts
        │                                   # + append-only ledger
        ▼
you / CI harness: `php artisan migrate`     # explicit human step
```

## CLI

```sh
# The gated engine plan (destructive steps block until --confirm).
lekalo storage migrate-plan BASE CANDIDATE --profile PROFILE

# The Laravel generation input: SQL plan + effective gate + rename map
# + rollback classification, bound to every input digest.
lekalo storage laravel-plan BASE CANDIDATE --profile PROFILE \
  [--history HISTORY.json] [--timestamp-base 20260927000000] [--json]
```

`laravel-plan` is a dry run: it prints the bounded input document the
PHP adapter consumes. It never writes artifacts and never connects to
a database.

## Gates

An additive plan (risks `none` only) generates without confirmation.
A plan with a destructive **or** backfill step is blocked: the CLI
prints the exact `planId` to state with `--confirm PLAN_ID`. Changed
inputs invalidate a previous confirmation.

## Renames

A rename of a Model symbol never renames a table. Physical renames
travel in a validated history document:

```json
{
  "schemaVersion": "lekalo/storage-rename-history/v0.4.0",
  "identity": "dev.lekalo.storage-rename-history@0.4.0",
  "projectId": "planner",
  "baseDigest": "sha256:…",
  "candidateDigest": "sha256:…",
  "renames": [
    {
      "entity": "focus_session",
      "kind": "table",
      "from": "focus_session",
      "to": "session",
      "historyRef": "planner.focus_session"
    }
  ]
}
```

The planner validates every entry (old exists, new unoccupied, no
cycles) and emits `ALTER TABLE … RENAME` / `ALTER TABLE … RENAME
COLUMN` instead of drop+add. Ambiguous or unvalidated history blocks
generation.

## Rollback classification

| Class | Meaning | `down()` |
|---|---|---|
| `reversible` | inverse restores the schema exactly | emitted from the typed reverse plan |
| `data-loss-on-rollback` | inverse drops data created since | emitted only with explicit rollback permission |
| `irreversible` | no faithful inverse exists | refuses before the first statement |

## Adapter output

`generate` writes `.lekalo/generated/php-laravel/<target>/migrations/`
only: one anonymous-class migration per plan plus `ledger.json`. The
ledger is append-only — published migrations are never rewritten,
byte-identical regeneration is a no-op, and `plan-clean` refuses to
delete them.

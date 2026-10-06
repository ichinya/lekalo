# Formatting (`lekalo format`)

Status: **Implemented** (issue #94)

`lekalo format` rewrites the admitted Model sources into one pinned
canonical text form. The formatter is a pure planning service over the
accepted loader surface: every candidate is reparsed, reloaded, and
compared through the real Model/IR oracle before anything is written, and
formatting is proven semantics-preserving or refused.

## Command surface

```text
lekalo format [PATH] [--project DIR] [--check] [--allow-lossy-comments] [--json]
lekalo format --stdin --stdin-filepath PATH [--project DIR] [--json]
```

- `PATH` selects an exact admitted document (`lekalo/modules/planner/entities.yaml`)
  or one module directory (`lekalo/modules/planner`); omitted, it covers
  the whole admitted Model source set. Scope limits writes, never
  dependency reads.
- `--check` plans only: valid source is a `valid` result (exit 0), source
  that needs formatting is `denied` (exit 3) with the changed-file report,
  a malformed source is `invalid` (exit 1) with the ordinary loader parse
  diagnostic, and an unknown `schema_version` is `unsupported-version`
  (exit 5). Check is read-only on every outcome.
- `--allow-lossy-comments` is the only way a plan with enumerated comment
  losses applies. No hidden configuration or prompt authorizes loss.
- `--stdin --stdin-filepath` is the editor variant: it reads one bounded
  unsaved buffer, verifies it against the project overlay in memory, and
  emits the full-document replacement text (the human projection is the
  formatted buffer; `--json` carries it in the report row). It never
  writes, and positional paths, `--check`, and `--allow-lossy-comments`
  are refused in this mode.

## Canonical policy

The built-in `dev.lekalo.format-policy@0.8.0` contract is the only style
authority: UTF-8 without BOM, LF line endings, one final newline,
two-space indentation, double-quoted strings and keys, lowercase
`true`/`false`/`null`, base-10 integers. Mapping keys sort by UTF-8 bytes
and `definitions` sort by full semantic ID, each within retained
blank-line groups; set-like `modules[].imports` sort; every
behaviorally ordered array is preserved. Comments ride their owners;
a comment without an unambiguous owner blocks until the explicit lossy
opt-in. A source file that is already canonical formats to itself.

## Reports and safety

The command payload is the closed `dev.lekalo.format-report@0.8.0`
document: byte-sorted file rows with before/after digests, semantic
equality, enumerated losses, and the summary counts
(`selected = changed + unchanged`). Apply runs through a confined
transaction under `.lekalo/cache/format/`: digest-bound preimages,
immutable backups, a durable journal, verified rollback, and fail-closed
recovery — while a journal or lock exists every reader refuses with
`format.recovery-required` until the next explicit `format` run restores
the pinned bytes. The nine `format.*` diagnostics (`LEK-FORMAT-001` …
`LEK-FORMAT-009`) live in the union diagnostic registry at `0.8.0`.

Diagnostics: `format.diff`, `format.comments-loss`,
`format.semantic-change`, `format.selection-invalid`,
`format.source-changed`, `format.write-failed`,
`format.recovery-required`, `format.limit-exceeded`,
`format.unsupported-surface`.

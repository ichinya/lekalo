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
  unsaved buffer, verifies it against the project overlay built from the
  buffer itself (the oracle judges edits against the unsaved state, not
  the stale saved file), and emits the full-document replacement text
  (the human projection is the formatted buffer; `--json` carries it in
  the report row). It never writes. A buffer whose plan enumerates any
  comment loss is refused with the registered editor-mode diagnostic
  `format.stdin-loss`, which carries the exact loss span — the
  replacement text is never returned while a comment would drop — and
  positional paths, `--check`, and `--allow-lossy-comments` are refused
  in this mode. A well-formed selector outside the admitted Model source
  family (lockfiles, docs, runtime trees) is the registered
  `format.unsupported-surface` refusal (exit 4) in every mode.

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
recovery — while a journal exists every reader refuses with
`format.recovery-required` until the next explicit `format` run restores
the pinned bytes. Recovery is fully validated before any restore: the
journal and manifest are typed closed records pinned to their plan
identity and admitted Model paths only, every current source digest is
preflighted (a post-crash user edit or deletion surfaces as the explicit
`format.source-changed` conflict and is never erased), and the whole
recover-then-apply window runs under one process-death-released
ownership lock — a live second formatter is refused with the registered
`format.locked` denial instead of being rolled back as crashed. The
mutating run also reclaims crash residue under the same lock: a
pre-journal attempt home and an orphaned rename temp never poison an
identical retry or leave the governed tree unloadable (rename temps live
inside the runtime home, never as source-tree siblings). Retries of an
identical plan get a fresh attempt home and never collide with completed
material; a bounded attempt chain reclaims its oldest completed home
instead of permanently denying a repeated change set. The eleven `format.*` diagnostics (`LEK-FORMAT-001`
… `LEK-FORMAT-011`) live in the union diagnostic registry at `0.8.0`.

Diagnostics: `format.diff`, `format.comments-loss`,
`format.semantic-change`, `format.selection-invalid`,
`format.source-changed`, `format.write-failed`,
`format.recovery-required`, `format.limit-exceeded`,
`format.unsupported-surface`, `format.locked`, `format.stdin-loss`.

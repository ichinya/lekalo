# Lekalo greenfield bootstrap (`lekalo init`, `lekalo module new`)

Issue #97 creates a minimal greenfield Lekalo project: the canonical
skeleton and nothing else — no full application generation, no target
framework until an explicit selection, no package or tool installation,
no adapter execution, no lock. The lock appears only after explicit
resolution through `lekalo lock` (issue #91 catalog seam); application
code only through the separate generation commands.

Greenfield bootstrap shares the adoption configuration semantics of
issue #38 ([adopt.md](adopt.md), [ADR-0028](adr/0028-init-adopt.md))
verbatim: the same closed grammars, the same atomic no-clobber writer
with journal and rollback, the same identical-bytes skip and
any-other-content conflict denial, the same target document bytes, and
the same in-process load-and-validate gate. [ADR-0039](adr/0039-greenfield-init-bootstrap.md)
records the greenfield decisions.

Product 0.2.14, Model 1.0.0, IR 0.1.0 and diagnostic registry 1.24.0
remain independent version lines.

## Commands

```text
lekalo init
lekalo init --target node-typescript
lekalo init --target node-typescript --profile default
lekalo init --profile <token>            # refused: a profile selects within --target
lekalo init --project-id ID
lekalo init --module planner
lekalo init --frontend yaml|json
lekalo init --editor-hints
lekalo init --dry-run
lekalo module new planner
lekalo module new planner --frontend json --dry-run
```

`init --adopt` keeps the issue #38 adoption behavior unchanged; the
two modes never mix in one invocation.

## Root selection

The greenfield root is the invocation directory itself, or an explicit
`--project DIR` / `LEKALO_PROJECT` selector (usual selection grammar
and alias refusal; the marker is not required because it may not exist
yet). Bootstrap never walks the ancestors for workspace or manifest
evidence — that discovery belongs to adoption; a new project is created
exactly where the user stands. The directory must already exist;
bootstrap never creates project roots. The receipt records the closed
`rootBasis` (`explicit` or `invocation-directory`).

## Artifacts

| Path | Written | Content |
| --- | --- | --- |
| `lekalo/project.yaml` | always | one `project` definition, Model 1.0.0 |
| `lekalo/modules/<module>/module.yaml` | always | one empty `module` definition (the first module) |
| `lekalo/targets/<target>.yaml` | only for `--target` | the shared opaque selection document (byte-identical to adoption) |
| `.gitignore` | always | the managed `/.lekalo/` ignore line |
| `.vscode/settings.json` | only for `--editor-hints` | editor/schema hints for the model homes |
| `lekalo.lock` | never | created later by explicit `lekalo lock` resolution |

No application code is generated; that belongs to the separate
`generate` command. Nothing outside the planned artifact set is read
for content or modified.

### The `.gitignore` addition

`.gitignore` is user-owned, so bootstrap treats it as a merge, never a
replace: when the file is absent the managed line is created; when it
exists without the line, exactly one `/.lekalo/` line is appended
(with one newline separator when the file does not end in one); when
either line spelling (`/.lekalo/` or `.lekalo/`) is already present
the file is reported unchanged. User bytes always stay a prefix of the
result, the append is journaled with the exact original length, and a
later failure restores the file by truncation — never by rewriting its
bytes. Every other artifact uses the plain no-clobber rule: identical
bytes skip, any other content is a conflict.

### Editor/schema hints

`--editor-hints` opts into a fixed `.vscode/settings.json` mapping
`lekalo/project.yaml` and `lekalo/modules/**/*.yaml` to the published
Model 1.0.0 schema `$id` (`https://lekalo.dev/schemas/model/1.0.0/schema.json`).
A differing pre-existing settings file is an ordinary no-overwrite
conflict; bootstrap is re-runnable without the flag.

## Wizard decisions and their flags

| Decision | Flag | Persisted |
| --- | --- | --- |
| project id/name | `--project-id` (else the sanitized directory name) | `lekalo/project.yaml` |
| canonical model frontend | `--frontend yaml\|json` (default `yaml`) | document syntax |
| initial module | `--module` (default `app`); `lekalo module new ID` for later modules | `lekalo/modules/<id>/module.yaml` |
| target/profile | `--target`, `--profile` | `lekalo/targets/<id>.yaml` |
| ownership mode | — (greenfield is model-owned by construction; observed mode enters through `init --adopt` / `observe`) | — |
| OpenSpec/AI Factory/HLV integration | — (evidence enters later via `doctor --trace`) | — |
| strictness profile | — (chosen per validation: `lekalo validate --strict`) | — |
| adapter/tool availability | — (resolved only through the explicit `lekalo lock -- PROGRAM` handshake) | — |

Decisions without a contract home are never persisted as unpublished
fields; the Model contract stays closed.

## Semantics

- **Non-interactive and machine-readable.** Every decision is a flag;
  `--dry-run` prints the full plan as the receipt envelope
  (`writes[]` with per-artifact `action`, `sha256`, `disposition`)
  without writing.
- **Idempotent re-run.** A re-run over identical artifacts reports
  every artifact `unchanged` (`changed: false`, no gate); a partially
  deleted tree re-adds exactly the missing artifacts.
- **No silent overwrite.** Any planned path existing with different
  content denies the whole run (`init.bootstrap-conflict`, exit 3) and
  the conflicting bytes survive.
- **Target stays optional.** A project without a target validates the
  Model normally; `--target` records the selection only and resolves
  nothing — adapters are resolved exclusively through the explicit
  lock handshake, never probed or installed by bootstrap.
- **Safe relative paths.** Every artifact path is a fixed
  project-relative POSIX spelling; the closed id grammars keep module
  homes inside `lekalo/modules/`.
- **Stable semantic IDs.** The generated documents use the closed
  one-segment id grammar; no randomized or path-derived identity
  enters the Model.
- **Template versions recorded.** The receipt carries one
  `templates[]` record per planned artifact
  (`lekalo/init/<artifact>@<product version>`); every generated
  document also carries its own `schema_version`.
- **Journaled rollback.** Creates are journaled in order and the
  `.gitignore` append with its original length; any write or gate
  failure rolls the exact journal back in reverse (truncation for the
  append), and an incomplete rollback is reported
  (`init.bootstrap-recovery-required`), never silently repaired.

## `lekalo module new`

`module new ID` creates one additional empty module
(`lekalo/modules/<ID>/module.yaml`, the module manifest and nothing
else) in an existing project, resolved through the normal marker
discovery. It shares the bootstrap semantics verbatim: the closed
one-segment id grammar (`cli.usage` on violation), `--dry-run`, the
no-overwrite conflict denial, the identical-bytes skip, the journaled
writer, and the post-write load-and-validate gate. Outside a Lekalo
project the run is the normal `structure.root-not-found` failure and
nothing is written.

## Receipts and diagnostics

The success envelope is the closed `BootstrapReceipt` / `ModuleReceipt`
wire (camelCase fields, normative order, no timestamps, no absolute
paths): `status`, `operation`, `mode`, `changed`, `rootBasis`,
`projectId`, `projectIdSource`, `module`, `frontend`, `target`,
`adapterProfile`, `editorHints`, `added`, `unchanged`, `conflicting`,
`writes[]`, `gate`, `templates[]`. Dispositions spell the acceptance
vocabulary: `create`, `append`, `unchanged`, `conflict`.

Failures are registered rules of the `init.*` family (diagnostic
registry 1.24.0, `LEK-INIT-006..009`): `init.bootstrap-conflict`
(denied), `init.bootstrap-id-required` (denied),
`init.bootstrap-write-failed` (invalid), and
`init.bootstrap-recovery-required` (invalid). Request grammar
violations are the stable `cli.usage` failure; gate failures surface
the exact loader/validator envelopes. Adoption keeps its own
`init.adopt-*` spellings unchanged.

## Exit protocol

| Outcome | Stream | Exit |
| --- | --- | --- |
| valid receipt (dry-run, apply, or idempotent re-run) | stdout | `0` |
| no-overwrite / id-derivation policy denial | stdout | `3` |
| usage, write, rollback, or gate failure | stderr | `1` |

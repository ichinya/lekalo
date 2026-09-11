# Lekalo adoption (`lekalo init --adopt`)

Issue #38 connects Lekalo to an existing repository: adoption never moves
sources, never generates beyond the minimal canonical skeleton, and never
overwrites. Detection is evidence — every proposal carries its evidence
path and a closed confidence — and never declares unconfirmed framework
semantics. Nothing here executes a discovered script, package manager, or
adapter; the issue #27 target protocol keeps that boundary.

Product 0.2.3, Model 1.0.0, IR 0.1.0 and diagnostic registry 1.13.0 remain
independent version lines.

## Commands

```text
lekalo init --adopt
lekalo init --adopt --target node-typescript
lekalo init --adopt --target node-typescript --profile default
lekalo init --adopt --dry-run
lekalo init --adopt --project DIR
lekalo init --adopt --project-id ID
```

`init` without `--adopt` is the stable usage failure (exit 1, stderr,
`cli.usage`): greenfield creation is a later issue with its own template
semantics and shares the bootstrap configuration seam recorded in
[ADR-0028](adr/0028-init-adopt.md).

## Root selection

The adoption root is chosen deterministically:

1. an explicit `--project DIR` (or `LEKALO_PROJECT`) selector wins; the
   usual selection grammar and alias refusal apply, and the marker file is
   not required because it may not exist yet;
2. otherwise the invocation ancestors are scanned for workspace roots
   (`package.json` with `workspaces`, `pnpm-workspace.yaml`, `go.work`,
   `Cargo.toml` with `[workspace]`, `settings.gradle(.kts)`); exactly one
   workspace root is adopted, more than one is ambiguous and demands
   explicit resolution (`init.adopt-ambiguous-root`, exit 3);
3. otherwise the nearest ancestor with any recognized manifest is adopted;
4. otherwise the invocation directory itself is adopted (an empty
   repository).

Candidate spellings are invocation-relative (`.`/`..`/`../..`); no
absolute filesystem path ever enters a receipt.

## Detection

Detection is a bounded, no-follow walk (depth 4, 10 000 entries, closed
skip list for dependency/build trees and for the externally owned
`openspec/`, `.ai-factory/`, `.hlv/` homes and the canonical `lekalo/`
trees). Every observation is a proposal with provenance and one of the
closed confidences `high` (declared by a manifest field), `medium` (file
presence or ecosystem convention), or `low`:

- manifests and package managers (lockfiles) with their ecosystem;
- language and framework hints from declared dependencies
  (`typescript`, `react`, `laravel/framework`, ...), each with its
  declaring field;
- monorepo/workspace roots with their verbatim member globs, plus
  monorepo tooling evidence (`lerna.json`, `turbo.json`, `nx.json`);
- source and test directories (existence only, never semantics);
- OpenAPI/schema files;
- native gate command proposals: workflow-declared scripts verbatim
  (`npm run build` with `package.json#scripts.build` as evidence) or
  derived ecosystem commands (`go test ./...` with `go.mod` as evidence);
- installed target adapters: a bounded `PATH` lookup of
  `lekalo-target-<target-id>[.exe]`. A missing adapter is reported, never
  installed, and never blocks the skeleton;
- observed modules: workspace members carrying a manifest, recorded with
  `mode: "observed"`.

Observed modules are exactly what the name says: observations. The Model
contract has no module-mode field, so adoption emits none — the canonical
skeleton stays a zero-module project and the observations live only in
the receipt. Authoring canonical modules from observed code is explicit
later work.

## The minimal canonical skeleton

Adoption writes only what the accepted #4 structure contract requires:

- `lekalo/project.yaml` — the single project definition
  (`{"schema_version":"1.0.0","definitions":[{"id":...,"kind":"project","version":1,"description":"Adopted existing project."}]}`),
  always written;
- `lekalo/targets/<id>.yaml` — the explicit selection record, written
  only when `--target` selected one. Target documents stay opaque to the
  loader.

The explicit adapter profile (`--profile PROFILE`) completes the issue's
explicit target/profile selection: it requires `--target`, uses the #28
token grammar (lowercase ASCII, digits, `-`, at most 64 bytes), and is
recorded verbatim in the target document (`"profile"`) and in the
receipt's `adapterProfile`. Adoption never executes an adapter, so a
profile is never checked against an adapter's declared profiles — it is
a persisted selection, and the wire protocol governs its use later. An
orphan `--profile` without `--target` or a malformed token is the stable
usage failure (`cli.usage`, exit 1) before any plan or write; the same
target id re-adopted with a different profile plans different bytes and
therefore denies with `init.adopt-conflict` instead of overwriting. The
core adoption entry (`lekalo_core::init::adopt`) enforces the same
refusal for every library caller, before any root discovery, detection
walk, read probe, or write — and the skeleton writer refuses values
outside its closed grammars as well, so no consumer of the bootstrap
seam can plan a path escape or a structurally broken document.

No `.lekalo/**` runtime state is written, `lekalo.lock` stays the `lekalo
lock` seam, and `.gitignore` is user-owned and never edited.

## The project id

The canonical project id comes from, in order: `--project-id` (exact
match against the closed grammar), the root `package.json` name,
`composer.json` name, `Cargo.toml` `[package] name`, the `go.mod` module
path's last segment, or the root directory name. A name that is not
already grammar-clean is sanitized (`-` → `_`, scope separators folded)
and the receipt records the derivation (`projectIdSource.source`,
`.original`, `.confidence`). When nothing derivable remains, adoption
refuses with `init.adopt-id-required` until `--project-id` names the
project.

## Dry-run, no-overwrite, rollback, idempotence

`--dry-run` prints the full plan — every planned write with its action
and exact-bytes SHA-256, plus the complete detection — and writes
nothing.

Applying enforces the no-overwrite policy at the operating-system level:
planned files are created with atomic `create_new` semantics, so a path
that appears between plan and apply fails the write instead of being
overwritten. Paths already present with byte-identical content are
skipped and reported (`already-present`); any other existing path denies
the whole adoption (`init.adopt-conflict`, exit 3) before anything is
written.

Every created path is journaled. A failed write rolls the journal back in
reverse; a rollback that cannot remove a created path is reported
explicitly (`init.adopt-recovery-required`, exit 1) with the logical
paths that remain.

Adoption is idempotent: a re-run over an adopted repository reports the
identical bytes, writes nothing, and exits 0.

## The post-write gate

After writing, adoption loads the skeleton through the normal loader and
validates it under the default profile, in process. The receipt carries
the gate outcome (`gate.modelVersion`). If the gate ever fails, adoption
first rolls every created file back — using the exact mutation journal
the apply produced, so only genuinely created paths are removed and
pre-existing directories are never inferred into ownership — then
returns the gate's failure envelope unchanged — the same failure
`lekalo load`/`lekalo validate` would print. A successful gate never
rolls anything back. An idempotent re-run with nothing to write skips
the gate: the repository may have legitimately grown beyond the
skeleton.

## Exit and output protocol

| Outcome | Stream | Exit | Envelope |
|---|---|---|---|
| plan or applied adoption | stdout | `0` | `{"status":"valid","operation":"init",...}` |
| usage (no `--adopt`, bad ids) | stderr | `1` | `cli.usage` |
| write/rollback failure | stderr | `1` | `init.adopt-write-failed` / `init.adopt-recovery-required` |
| conflict / ambiguity / id required | stdout | `3` | `init.adopt-conflict` / `init.adopt-ambiguous-root` / `init.adopt-id-required` |

The receipt is deterministic: fixed field order, sorted collections, no
timestamps, no absolute paths, no raw argv.

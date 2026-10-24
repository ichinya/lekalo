# Lekalo adoption (`lekalo init --adopt`)

`lekalo init --adopt` connects Lekalo to an existing repository: adoption never moves
sources, never generates beyond the minimal canonical skeleton, and never
overwrites. Detection is evidence — every proposal carries its evidence
path and a closed confidence — and never declares unconfirmed framework
semantics. Nothing here executes a discovered script, package manager, or
adapter; the issue #27 target protocol keeps that boundary.

This page owns existing-repository detection and skeleton writes. Use
[adoption](adoption.md) to choose an ownership workflow and
[bootstrap](bootstrap.md) for a new empty project.

## Commands

```text
lekalo init --adopt
lekalo init --adopt --target node-typescript
lekalo init --adopt --target node-typescript --profile default
lekalo init --adopt --dry-run
lekalo init --adopt --project DIR
lekalo init --adopt --project-id ID
lekalo init --adopt --discovery-scope discovery.json --dry-run
```

`init` without `--adopt` creates a new minimal project through
[bootstrap](bootstrap.md); it does not perform repository detection. The
two modes cannot be combined. [ADR-0028](adr/0028-init-adopt.md) records the
shared configuration seam.

## Discovery scope

`--discovery-scope FILE` is an explicit, repository-relative JSON input for
adoption only. There is no automatic config search. `--profile` continues
to select an adapter profile; it is not a discovery config filename.

For a synthetic PHP backend and TypeScript frontend beside archived copies,
create `discovery.json` at the selected root:

```json
{
  "schemaVersion": "lekalo/discovery-scope/v0.9.0",
  "identity": "dev.lekalo.discovery-scope@0.9.0",
  "includes": ["apps/api", "apps/web"],
  "excludes": [".orca", ".bench", "debug", "archives"],
  "gitIgnore": "respect"
}
```

Then preview and apply through the same public command:

```bash
lekalo init --adopt --project . --project-id mixed --discovery-scope discovery.json --dry-run
lekalo init --adopt --project . --project-id mixed --discovery-scope discovery.json
```

Includes and exclusions are **literal paths**, not globs. `.` includes the
selected root. Overlapping/duplicate roots are sorted and deduplicated;
an exclusion removes that path and its entire subtree. Hidden repository
directories and actual source-directory capitalization are permitted as scope paths, while the canonical Model path
grammar remains unchanged. Absolute paths, traversal, Windows aliases,
symlinks/junctions and protected include roots receive registered
`structure.*` refusals before any skeleton write. Missing exclusion paths
are valid policy; selected include roots must exist as physical directories.

The precedence is fixed: protected dependency/build/runtime/sensitive paths
cannot be included; explicit exclusions prune first; repository Git ignore
rules then apply; includes never override either decision. Git ignore
negation can restore a path only while its parent remains traversable and
the path is neither protected nor explicitly excluded.

Both default and explicit discovery respect `.gitignore` at the selected
root and in traversed subdirectories. Matching is case-sensitive and does
not consult the Git index, global excludes or `.git/info/exclude`; a tracked
file matching these repository rules is still omitted from discovery.
The explicitly supplied scope file and the applicable ancestor `.gitignore`
files are policy inputs, read through the no-follow filesystem and pinned
separately. An excluded subtree is pruned before reading its own ignore
file, manifests or workspace members. The fixed owner-layout hints below
remain metadata-only; they never grant a protected source read.

Without a new input, the effective policy is `includes: ["."]`,
`excludes: []`, `gitIgnore: "respect"`. With a scope input and no explicit
`--project`/`LEKALO_PROJECT`, the invocation directory is the root: scoped
discovery does not inspect ancestor manifests to choose a different root.
The scope filename is resolved from that selected root.

`detection.scope` is a versioned
[receipt](../contracts/discovery-scope-receipt.schema.v0.9.0.json), paired with
the [input schema](../contracts/discovery-scope.schema.v0.9.0.json). It records
the normalized roots, builtin/file provenance, input and ignore-file SHA-256
pins, an effective-policy digest, bounded skipped-path reasons and coverage
limits. Filter contents and absolute paths are not copied into it.
Protected entries are grouped under path `.` with reason `protected`;
sensitive filenames and names inside protected homes are not published.
Changing the scope or an applicable ignore file changes its digest. The
same unchanged policy has the same digest in preview, apply and replay;
these invocations re-read the selected input rather than retaining a hidden
plan. Discovery filters do not bypass canonical post-write validation or
runtime recovery checks.

Depth, entry and collection limits mark `complete: false` with a reason.
Manifest reads are capped at 8 MiB each and 32 MiB per invocation. Scope
and ignore inputs are capped at 64 KiB; at most 128 ignore files and 1024
lines per ignore file are accepted. Unreadable, malformed or oversized
policy inputs refuse instead of silently broadening discovery. A partial
receipt is not exhaustive evidence about the repository.

Discovery still executes no package scripts, gates or adapters and edits
neither source nor ignore files. Workspace member globs are expanded only
against the already admitted catalog, so they cannot reintroduce excluded
or ignored copies. A PATH adapter directory inside the repository is also
subject to that catalog.

## Root selection

The adoption root is chosen deterministically:

These ancestor-search rules apply without `--discovery-scope`; the explicit
scoped-input anchoring is described above.

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
trees), filtered by the discovery scope above. Every observation is a proposal with provenance and one of the
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
  (`{"schema_version":"0.2.16","definitions":[{"id":...,"kind":"project","version":1,"description":"Adopted existing project."}]}`),
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
path's last segment, or the root directory name. Only root manifests admitted
by discovery may supply that identity; ignored or out-of-scope manifests do
not. A name that is not
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
| usage (bad ids, malformed scope, orphan scope flag) | stderr | `1` | `cli.usage` |
| scope/path policy refusal | stdout | `3` | registered `structure.*` reason |
| write/rollback failure | stderr | `1` | `init.adopt-write-failed` / `init.adopt-recovery-required` |
| conflict / ambiguity / id required | stdout | `3` | `init.adopt-conflict` / `init.adopt-ambiguous-root` / `init.adopt-id-required` |

The receipt is deterministic: fixed field order, sorted collections, no
timestamps, no absolute paths, no raw argv.

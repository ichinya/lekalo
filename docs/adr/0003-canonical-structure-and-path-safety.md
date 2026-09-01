# ADR-0003: Canonical project structure and path safety

Status: accepted structure contract for issue #4; the product remains at the
accepted M0 release `0.0.2`. This decision does not advance Model, loader,
lockfile, IR or product lifecycles.

Date: 2026-08-31

## Context

Issue #4 must fix portable file homes and safe root/path behavior before the
Rust CLI (#3), Model v0.1 (#5), loader (#7) and lock implementation (#10). The
published authority matrix `dev.lekalo.authority-matrix` `1.3.1` already owns
`lekalo/**` and a closed set of `.lekalo/**` paths. #4 must fit that accepted
contract, not replace it.

The live downstream issues also establish a hard seam: #5 owns model fields and
schemas, #7 owns parsing/import semantics, and #10 owns the lock envelope.
Structure validation cannot pre-empt those contracts.

## Decision

Adopt [`docs/canonical-structure.md`](../canonical-structure.md) as the
structure contract, with
[`scripts/check-structure.mjs`](../../scripts/check-structure.mjs) as the
dependency-free reference checker and `tests/fixtures/structure/**` as the
portable fixtures.

The closed decisions are:

1. **Kind-file physical layout.** `module.yaml` marks a module home; definition
   homes are the seven named kind files. Their contents and definition kinds
   remain Model/loader concerns.
2. **Directory scan is physical truth.** `modules/` may be absent for a
   zero-module project. Once present, every directory directly below it needs
   `module.yaml`; no project-level module list is introduced by #4.
3. **Imports and references are deferred.** #4 fixes their future file homes
   only. Syntax, resolution, cycles, collisions and canonical normalization are
   owned by #6/#7.
4. **Nearest-root-wins with safe relative selection.** Discovery has no redirect
   files. `--project`, `--from` and `LEKALO_PROJECT` accept only invocation-
   relative, traversal-free, non-aliased selections.
5. **Portable lexical deny-set plus physical fail-closed scanning.** Governed
   paths are lowercase NFC; absolute/encoded/traversal/device/NFKC-alias/8.3-
   like spellings are malformed. Symlinks, junctions, special entries and real-
   path aliases are denied before content access.
6. **Authority-compatible canonical/runtime split.** Canonical inputs live in
   `lekalo/**` and `lekalo.lock`. Runtime homes exactly cover every Lekalo-owned
   `.lekalo/**` path registered by authority 1.3.1: `import`, `cache`,
   `generated`, `consumer/{model,bindings}` and the closed privacy subtree.
7. **Nested roots are nearest-root projects, not governed-tree content.** Child
   roots in ordinary source subtrees are allowed and independent. Any nested
   `lekalo` directory inside `lekalo/**` or `.lekalo/**` is denied.
8. **File contents are opaque at #4.** Marker, module, target and lock files are
   checked for physical placement/type only. Invalid YAML and future/richer
   downstream envelopes must still pass structure validation.
9. **Deterministic bounded diagnostics.** Validation is fail-closed on unreadable
   trees and scan limits. Errors use logical paths; successful discovery alone
   returns an absolute operational root.

## Consequences

- #3 can implement discovery and selector/exit behavior without inventing model
  parsing.
- #5 may define `schema_version`, project/module identities and definition
  schemas without a structure successor, provided file homes remain stable.
- #6/#7 may define references, YAML/JSON parsing and import semantics without
  fighting an earlier flow-YAML parser.
- #10 may publish the rich lock envelope described by its live issue; #4 fixes
  only the `lekalo.lock` home and regular-file requirement.
- Stable-tree symlink/junction escapes are rejected and adversarially tested.
  Long-running consumers remain responsible for handle-relative I/O and TOCTOU
  resistance after validation.

# ADR-0006: Loader frontends, imports, and canonical normalization

Status: accepted for issue #7 (product candidate 0.1.5).
Consumes: ADR-0003 (canonical structure), ADR-0004 (Model v0.1),
ADR-0005 (semantic IDs). Excludes: IR (#8), migration (#9).

## Context

Issue #7 requires loading Lekalo source files from YAML/JSON, resolving
explicit imports, and normalizing equivalent entries into one canonical
representation before semantic validation. The loader is the first Rust
capability that reads project content, so it also inherits the physical
safety story that #4 established in Node.

## Decisions

1. **Two exact Model versions, finite dispatch.** The loader recognizes
   exactly `0.1.0` and `1.0.0`. There is no version range, no prerelease
   grammar, no migrator, and no auto-conversion. An unknown literal fails
   `versioning.unsupported-version` (exit 5); supported-but-mixed input
   fails `versioning.mixed-versions` (exit 1). The active version selects
   the module-ID grammar (0.1.0 allows hyphens; 1.0.0 does not) and is
   preserved verbatim in the output.

2. **Strict frontends over one spanned tree.** JSON and YAML both build the
   same byte-spanned value tree. Frontend selection is the first
   non-whitespace byte (`{`/`[` = JSON); JSON never falls back to YAML. The
   YAML surface is closed (no anchors, aliases, tags, merge keys, flow,
   multi-document streams, non-string keys, or non-core numeric forms) so
   that "accepted input" has no grey zone. saphyr-parser 0.0.8 (MIT OR
   Apache-2.0, MSRV 1.65, no unsafe) provides YAML 1.2 events; strictness
   and byte spans are enforced in this codebase because byte-precise spans
   and duplicate-key reporting are contract requirements the event layer
   alone does not deliver.

3. **Module-ID imports with direct visibility only.** Imports are semantic
   edges between module IDs, never textual includes and never directory
   names. Fully qualified references to another module require a direct
   import; short references resolve only inside the declaring module.
   Transitive visibility does not exist. Old IDs from rename history are
   traceability, never aliases (#6).

4. **Normalization is Sugar-free canonical form.** Compact type sugar
   (`list<T>`, `T?`) and structured one-key objects normalize to identical
   canonical bytes: `{"ref": …}`, `{"list": …}`, `{"optional": …}` with a
   four-level ceiling. Key order is raw UTF-8 byte order; `modules`,
   `definitions`, and `imports` are the only reordered arrays because #6
   declares them set-like; all other arrays keep source order so semantics
   are never invented.

5. **Structure safety is re-implemented at the read path.** The Node #4
   checker remains the reference validator, but a preflight cannot close a
   TOCTOU window. The loader therefore re-validates with capability-safe
   primitives: descriptor-relative, no-follow opens on Unix; component-wise
   reparse-point rejection on Windows; identity re-checks after open. It
   never writes and never reads `.lekalo/**` as input.

6. **Typed exits reuse the accepted classes and reserve one.** Valid 0
   stdout; invalid 1 stderr; denied 3 stdout (including `loader.path-escape`
   import denials); unsupported Model version 5 stderr. Exit 4 remains the
   not-yet-implemented capability class and is not produced by `load`.

## Consequences

- Consumers get byte-stable canonical models with exact source locations;
  the semantic validator (#5/#6 checker) stays the sole owner of closed
  fields, kind placement, typed reference checks, and registry invariants.
- The accepted-YAML surface is deliberately narrower than YAML 1.2; wider
  surfaces require a reviewed contract revision, not a loader change.
- IR (#8) consumes `LoadedProject` without reparsing; no IR concepts exist
  here. A future explicit converter (#9 adjacent) must reuse this
  filesystem capability and live outside `load`.

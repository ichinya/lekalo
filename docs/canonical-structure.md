# Canonical Lekalo project structure

Status: accepted structure contract for issue #4. This layer fixes physical
layout, ownership homes, root discovery and path safety. Model schemas (#5),
symbol/reference syntax (#6), YAML/JSON parsing and import semantics (#7), and
the lockfile envelope (#10) are deliberately downstream.

The normative reference checker is
[`scripts/check-structure.mjs`](../scripts/check-structure.mjs), and the
conformance fixtures are [`tests/fixtures/structure`](../tests/fixtures/structure).
[ADR-0003](adr/0003-canonical-structure-and-path-safety.md) records the closed
decisions. If prose and checker disagree, consumers stop; prose cannot broaden
the checker.

## Layout and ownership

```text
<project-root>/
  lekalo/
    project.yaml
    modules/
      <module>/
        module.yaml
        entities.yaml
        commands.yaml
        queries.yaml
        policies.yaml
        events.yaml
        scenarios.yaml
        bindings.yaml
    targets/
      <target-id>.yaml
  lekalo.lock
  .lekalo/
    import/
    cache/
      cache.sqlite
    generated/
      ir/
      manifests/
      reports/
    consumer/
      model/
      bindings/
    privacy/
      exports/
      redacted/
      aggregates/
      decisions/
        export/
        redaction/
        aggregate/
```

Only `lekalo/project.yaml` is required. `lekalo/modules/` may be absent while a
project has no modules; once present, every child directory is a module home.
All other shown files and directories are optional legal homes. The leaf names beneath
`.lekalo/generated/` are examples, not a closed list.

| Path | Class | Committed | Owner |
|---|---|---|---|
| `lekalo/project.yaml` | canonical marker/model document | yes | Lekalo |
| `lekalo/modules/<module>/**` | canonical semantic model | yes | Lekalo |
| `lekalo/targets/<target-id>.yaml` | canonical target configuration | yes | Lekalo |
| `lekalo.lock` | canonical resolved versions | yes | Lekalo (#10) |
| `.lekalo/**` | derived/cached/direct evidence | no | Lekalo runtime |
| everything else | outside this structure contract | by its owner | user toolchain/registered owner |

`lekalo/**` plus `lekalo.lock` is the committed Lekalo project input.
`.lekalo/**` is regenerated or direct runtime evidence and is never the only
copy of a canonical semantic decision. Canonical model files are never stored
in `.ai-factory/qa`, generated rules or OpenSpec changes.

### Authority 1.3.1 compatibility

The published `dev.lekalo.authority-matrix` `1.3.1` contract is upstream and
non-negotiable. Every Lekalo-owned registered `.lekalo/**` kind has exactly one
legal structure home:

| Artifact kind | Legal path |
|---|---|
| `lekalo.observed-model-draft` | `.lekalo/import/**` |
| `lekalo.cache` | `.lekalo/cache/**` |
| `lekalo.generated-intermediate` | `.lekalo/generated/**` |
| `consumer.model` | `.lekalo/consumer/model/**` |
| `consumer.target-bindings` | `.lekalo/consumer/bindings/**` |
| `export.artifact` | `.lekalo/privacy/exports/**` |
| `export.decision` | `.lekalo/privacy/decisions/export/**` |
| `redaction.artifact` | `.lekalo/privacy/redacted/**` |
| `redaction.decision` | `.lekalo/privacy/decisions/redaction/**` |
| `aggregate.artifact` | `.lekalo/privacy/aggregates/**` |
| `aggregate.decision` | `.lekalo/privacy/decisions/aggregate/**` |

The conformance suite derives this set from the accepted contract and requires
exact equality with the checker allowlist. It also materializes one real file
in every home and denies historical/wrong placements such as
`.lekalo/cache.sqlite`, `.lekalo/ir/**` and misspelled consumer/privacy homes.
This is placement validation only; privacy disposition and authority actions
remain governed by their published contracts.

## Root discovery and selection

A project root is the nearest ancestor directory containing a physical regular
file at `lekalo/project.yaml`. Discovery starts at the current directory (or
`--from <relative-dir>`) and walks upward one directory at a time. The first
marker wins. Reaching the filesystem root without a marker is
`structure.root-not-found`.

Explicit selection is `--project <relative-dir>` or `LEKALO_PROJECT`. Selectors
are relative to the invocation directory; `.` is allowed. Absolute, drive,
UNC, URI, `..`, backslash, percent-encoded, DOS-device and short-name-like
selectors are malformed. A selector resolving through a symlink, junction,
reparse alias or different real/8.3 path is denied. The selected directory must
still contain the marker. There are no redirect files.

Successful `--find-root` output contains the canonical absolute filesystem
root because callers need an unambiguous operational location. This is the one
intentional absolute-path envelope. Failures never echo selectors or absolute
filesystem paths.

## Portable path and physical-containment rules

Every entry below the governed `lekalo/` and `.lekalo/` trees uses a
project-relative POSIX spelling with `/` separators. The reserved roots
`.lekalo` and `lekalo.lock` are fixed exceptions to the ordinary segment
grammar.

A declared path is well formed when:

- it is non-empty and relative: no leading `/`, drive, UNC, `~` or URI scheme;
- it contains no backslash, colon or percent-encoded octet;
- every segment matches `[a-z0-9][a-z0-9._-]{0,63}`: lowercase, no empty,
  `.`/`..`, trailing dot/space or control-character segment;
- no segment is a DOS device after Unicode NFKC compatibility normalization:
  `CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, `CONIN$`,
  `CONOUT$`, `CLOCK$`, with any extension. This includes fullwidth, circled,
  superscript and subscript compatibility aliases;
- no segment contains short-name-like `~<digit>`;
- its raw spelling is Unicode NFC.

Rejections are classified in fixed order: absolute prefixes
(`structure.path-absolute`), escapes (`structure.path-escape`), empty/long
segments (`structure.path-segment`), traversal/dot-space tails
(`structure.path-traversal`), compatibility device aliases
(`structure.path-device`), short-name aliases (`structure.path-short-name`),
uppercase (`structure.path-case`), then remaining segment violations.

The checker uses `lstat`/directory-entry metadata and fails closed on every
symlink, junction or special entry beneath `lekalo/**` and `.lekalo/**`, on a
linked `lekalo.lock`, and on an aliased selected root. It never follows a link
to validate escaped content. Real junction probes are part of the test suite.
Consumers that keep operating after validation must preserve this invariant
with descriptor/handle-relative I/O and recheck after mutation; no preflight
checker can eliminate a later TOCTOU replacement.

## Structure files and downstream seams

At #4, file contents are opaque:

- `lekalo/project.yaml` is a required marker and future project-model document;
- `lekalo/modules/` is optional for a zero-module project; each directory
  directly below it is a module home and must contain `module.yaml`;
- only `module.yaml` and the seven definition-kind files listed in the layout
  may appear directly inside a module; nested module directories are denied;
- `lekalo/targets/`, when present, contains only `<target-id>.yaml` files;
- `lekalo.lock`, when present, must be a physical regular file.

The checker intentionally does not parse any of them. Project/module fields and
`schema_version` belong to Model v0.1 (#5); stable references belong to #6;
YAML/JSON syntax, imports, cycles, collisions and source locations belong to
the loader (#7); exact lock contents, digests and deterministic serialization
belong to #10. Opaque-content fixtures prevent #4 from accidentally freezing
those contracts.

## Monorepos, nested projects, fixtures and ignore policy

- Standalone repositories and monorepos use the same layout.
- Sibling roots are independent. A project root may also exist in an ordinary
  child source subtree; nearest-root-wins makes the child independent.
- A directory named `lekalo` anywhere inside a project's governed
  `lekalo/**` or `.lekalo/**` tree is denied as `structure.nested-root`.
  Governed ownership trees cannot embed another root.
- Repository fixtures live outside canonical/runtime trees (this repository
  uses `tests/fixtures/structure/**`). Generated manifests and reports use
  `.lekalo/generated/**`.
- User repositories commit `lekalo/**` and `lekalo.lock`, and normally ignore
  `.lekalo/`. `.gitignore` is user-owned and this implementation never edits it.

## Exit and output protocol

| Outcome | Stream | Exit | Envelope |
|---|---|---|---|
| valid | stdout | `0` | `{"status":"valid",...}` |
| well-formed but policy-denied | stdout | `3` | `{"status":"denied","reasonCodes":[...]}` |
| malformed/usage | stderr | `1` | `{"status":"invalid","reasonCodes":[...]}` |

Reason codes are stable non-empty `structure.*` identifiers. Collections are
ordered and output has no timestamps. Repeated evaluation of the same input and
filesystem state is byte-stable. Except for the successful discovery root,
envelopes contain only logical project-relative paths and never echo raw root
selectors.

# Issue #94 formatter fixture family. All sources are synthetic: authored
# noncanonical seeds whose canonical outputs are reviewed committed bytes.
# No private or executed application content.

## Layout

- `planner-canonical/` — the committed canonical golden tree of the
  synthetic planner project. Every byte is reviewed; the CLI gates compare
  fresh `lekalo format` output against it on every OS.
- `seeds/noncanonical/` — the same model, deliberately noncanonical
  (unsorted keys and definitions, unsorted imports, mixed quoting, blank
  groups, comments in every owned position).
- `seeds/noncanonical-json/` — JSON-syntax twin of one module file. JSON
  source in a governed `.yaml` path stays JSON when formatted.
- `seeds/invalid-parse/` — malformed YAML. Formatting refuses with the
  existing loader parse diagnostic (invalid, exit 1), never a format diff.
- `seeds/unsupported-version/` — unknown Model `schema_version`. Refused
  with `versioning.unsupported-version` (exit 5), never rewritten.
- `seeds/comment-loss/` — a valid document whose trailing comment sits on
  a multi-line plain-scalar continuation line (no owner). Default mode
  denies before any write; `--allow-lossy-comments` applies with the loss
  enumerated.
- `seeds/ordered-swap/` — deliberate swaps inside behaviorally ordered
  arrays (enum `values`, scenario `covers`). The formatter preserves the
  swapped source order byte-for-byte (it never silently reorders content);
  the accepted #18 diff owner treats both members as set-like, so the
  semantic diff stays empty. Order preservation is additionally enforced
  by renderer unit tests.
- `portability/` — CRLF and Unicode seed projects for the cross-platform
  byte-equality gate, with their reviewed canonical expected outputs.

## Gates

- `scripts/test-format-contracts.mjs` — schema/instance/registry validation.
- `scripts/test-format-cli.mjs` — real-binary check/apply/idempotence/diff.
- `scripts/test-format-atomic.mjs` — transaction, recovery, and fail-closed.
- `scripts/test-format-portability.mjs` — byte-equality across platforms.

# Issue #84: Devin review round 2 (fix verification)

**Verdict: ACCEPT.** Both P2 findings from Codex r1 are fixed in `96cf8535`
and independently re-verified live. See `issue-84-fix1.md` for the author's
record; this document is the second Devin opinion.

## F1 — UTF-8 crash → structured refusal: VERIFIED

Independent reproduction on the rebuilt binary:
- bare U+00E9 profile document → `status:"invalid"`, exit **1**,
  `architecture-profile.input-invalid` (LEK-APR-001) — previously exit 101 panic.
- UTF-8 BOM + otherwise-valid published profile collection → same structured
  refusal, exit 1 — previously panic at `source.rs:89`.
- Mechanism confirmed in diff: `LineIndex::position` clamps/rounds to char
  boundary; `span` rounds end up; no ASCII-only restriction added (positive
  Unicode controls pass through the same decoder, exit 0).
- Bonus fix verified in source: `parse_hex4` cursor off-by-one for escaped
  supplementary characters was real and is corrected.
- Gate now runs 24 multibyte refusals (3 vectors × 8 admission routes) +
  2 Unicode successes; `fixProbes` reports them explicitly.

## F2 — advisory rule as sole blocker: VERIFIED

Independent reproduction: child of `contracted-standard` with
`justified-abstractions.required=true` → admission refusal
`architecture-profile.input-invalid`, exit 1 (before: admitted, then denied
`--check` as evidence-gap). Guard requires catalog `blockingBasis`
semantic|measured for `required:true`, shared across resolve/lock/diff/assess.

The `deterministic-generation` reclassification to `semantic` is legitimate —
repeated-output equality for pinned inputs is a behavior obligation, not a
style opinion; its coverage stays `unsupported`, and required-missing receipts
still produce evidence-gap/exit 3 on the managed profile.

## Regression surface

- Architecture gate: 124 live probes (up from 62), 7 families, 9 goldens,
  508/8 registry, read-only, exact Ajv 8.17.1 — green.
- `cargo test --locked -p lekalo-core --lib`: **1063 passed**, 0 failed.
- `cargo fmt --all -- --check`: clean.
- Author's byte-diff claim spot-checked: only the catalog `blockingBasis` +
  derived digest refs changed; all predecessor schemas/registries untouched.

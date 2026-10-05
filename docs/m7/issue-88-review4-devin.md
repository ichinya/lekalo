# Issue #88: Devin review round 4

**Verdict: ACCEPT** — the round-3 residual P2 (reconstructed Model-domain
identity admitting a native-shaped fact) is closed by emitted-shape
verification, and no new defect was found in the delta.

Reviewed 2026-10-05 in `ichinya/m7-issue-88`, worktree
`C:/Users/User/orca/workspaces/lekalo/m7-issue-88`. Fix under review:
`471589dab00cdb96dea21a0f5f54210bdd26a19c` on top of codex round-3 doc
`dfa685ee`. Starting worktree/index clean; review is local-only, no push.

## What was verified

Read [codex review round 3](issue-88-review3-codex.md),
[fix 3 report](issue-88-fix3.md), and the full source delta of
`471589da` (`waivers/policy.rs`, `waivers/mod.rs`, `waivers/tests.rs`,
shared gate, `docs/waivers.md`).

### Source inspection

- `admitted_producer_domain` now requires the emitted Model shape, not just a
  caller-recomputed identity: symbol must be `known` with the 2–3 segment
  semantic-ID grammar; `subject` must equal that symbol (the Model dependency
  witness binds subject to its semantic symbol, while the native depth producer
  binds an opaque native-witness subject); `module` must equal the symbol's
  first segment; `source_confidence` must be `known` `"exact"`; then the
  existing checks — finding ID recipe over the admitted subject/symbol,
  canonical Model/IR revision hash, unsupported adapter/capabilities, unknown
  path — still apply. A recomputed hash over retained native occurrence fields
  therefore fails at `subject == symbol` and `module`/`confidence` before any
  identity comparison.
- `ProfileState::producer_domain_admitted` default is now
  `fact.target != "model"`: an owner implementing only `reference`/`rule` can
  no longer inherit Model-only applicability. The three owners that recognize
  the producer (`ValidationState`, `LintState`, `TargetState`) opt in
  explicitly, each retaining its own selector/obligation check.
- `fingerprint_requirements` requires both the shared shape check and the
  owner admission before granting Model-only requirements.
- `waivers/mod.rs::audit` rechecks `admitted_producer_domain` at effectiveness,
  so even an owner that opts in cannot make an invalid reserved shape
  effective; pre-existing approval-bound stores keep their bytes and are
  classified `unverifiable` rather than rewritten.

### Live reproduction on this worktree

- `cargo build --locked -p lekalo-cli`: fresh binary.
- `cargo test --locked -p lekalo-core waivers:: --lib`: **15 passed, 0
  failed**, including the three new regression tests
  (`waivers_recomputed_model_identity_still_requires_the_emitted_shape`,
  `waivers_profile_defaults_never_opt_into_model_inapplicability`,
  `waivers_shared_effectiveness_rechecks_model_shape_after_profile_opt_in`).
- `cargo fmt --all -- --check`: clean.
- All three successor gates green with exact Ajv 8.17.1 and the rebuilt
  binary, each reporting `audits:39`, `nativePinControls:7`,
  `producerDomainControls:9`, `modelShapeControls:14`:
  `test-waivers-contracts.mjs`, `test-waiver-input-contracts.mjs`,
  `test-waiver-audit-contracts.mjs` (version 0.6.5). Frozen predecessor gate
  `test-ai-lint-waivers-contracts.mjs` green at schema 0.6.4.

The 14 new `modelShapeControls` cover the round-3 repro class directly: a
newly emitted native depth occurrence carrying the review's full reconstructed
ID/revision/pin/path recipe refuses add preview/apply and stored-waiver audit;
a known project symbol plus exact confidence cannot mask the retained opaque
native subject; optional-capability projections with the invalid source shape
refuse while their advisory gate stays advisory; mixed genuine-Model and
forged facts deny in either order with only the genuine acceptance effective.

The fix-3 report's independent 19-call replay additionally shows the actual
pre-fix applied store now audits unverifiable/waived-0, the old done receipt
is `waivers-done-stale`, and real native lint keeps the original blocking
finding — matching the requirement that old stores replay safely without
silent migration.

## Boundaries

Not re-verified here: the full workspace test suite (worktree reports a
paging-file flake under default concurrency; the focused waiver suite and all
contract gates pass), hosted CI, non-Windows platforms, and authenticated
external approval issuers. The recipe verifies the recognized core producer's
emitted shape; it does not authenticate arbitrary third-party reports, which
remains the documented producer-owned boundary.

# Issue #37 — AI Workspace integration: implementation report

Branch `ichinya/m7-issue-37` (pushed), milestone M7. Lekalo-side
integration of the public
[`lee-to/ai-workspace`](https://github.com/lee-to/ai-workspace) project
as an **external, optional, cross-project context and service-graph
layer**, per the research authority
[`docs/m7/issue-37-research.md`](issue-37-research.md). Companion
documents: the integration guide
[`docs/integrations/ai-workspace.md`](../integrations/ai-workspace.md)
(setup, agent flow, boundaries, upstream gaps) and the benchmark
evidence [`docs/m7/issue-37-benchmark.md`](issue-37-benchmark.md) with
its machine-readable result
[`issue-37-benchmark.json`](issue-37-benchmark.json).

Everything executed here was re-verified against the pinned upstream
revision `8fdf818fee757d24e723d657fc5d38614995e557` (package 1.5.0),
built from a shallow clone outside the checkout; nothing upstream is
vendored, and no Lekalo Rust code links to, shells out to, or reads AI
Workspace state.

## What was built

1. **Integration guide** — `docs/integrations/ai-workspace.md`: the
   recommended `lekalo-dev` group and six role aliases with
   config-before-init registration (the exact upstream mechanism that
   suppresses auto-sharing, verified in upstream source:
   auto-share runs only when no config exists), explicit share
   allowlists, service links, the single supported server launch shape
   (group scope + `AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS=0` +
   `AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE=0` overriding any inherited
   value), the ordered agent flow with digest binding, provenance and
   privacy rules, and the honest upstream-gap list.
2. **Versioned closed envelope contract** —
   `contracts/ai-workspace-event.schema.v0.6.3.json`
   (`lekalo/ai-workspace-event/v0.6.3`,
   `dev.lekalo.ai-workspace-event@0.6.3`, product-versioned per
   `docs/versioning.md`): closed Draft 2020-12 wire form of the public
   change-event envelope — identity, event key (sha256 over the
   recursive-lexicographic canonical form bound to the discriminator),
   typed artifact changes with old/new contract identities and
   digests, neutral role aliases with ordered origin-tagged explanation
   chains, a closed limitation vocabulary, bounds (64 artifacts / 64
   roles / 128 chains / 64 KiB body), and the public-safe policy block.
3. **Contract gate** — `scripts/test-ai-workspace-contracts.mjs`
   (pinned Ajv 8.17.1, registered in the Contracts CI job): validates
   the committed example, independently re-derives and pins the event
   key, proves canonicalization determinism under key permutation,
   proves schema closure by mutation, enforces bounds and role/path
   grammars, leak-probes every member, and cross-checks the routing
   manifest fixture digest.
4. **The hook** — `scripts/ai-workspace-hook.mjs` (dependency-free,
   opt-in): plan phase (resolve base/head to full commits, refuse a
   dirty tracked worktree, diff only the reviewed manifest's approved
   public paths over committed bytes, build the envelope, derive the
   key, write nothing) and send phase (record the key in the
   single-writer private outbox before any invocation; refuse
   production sends while the authority matrix admits no workspace
   change-event kind — explicit operator override flag for a designated
   installation; spawn upstream `event create` as an argv array with
   the widening flags forced off in the child; reconcile by keyed MCP
   readback verifying kind/title/body and the linked consumer targets
   read from the live service graph; `unknown-delivery` on any spawn
   failure, unparseable receipt, or target mismatch; verified-delivery
   repeats are no-ops). Closed states:
   `planned|delivered|unknown-delivery|refused|disabled|unavailable`.
5. **Integration gate** — `scripts/test-ai-workspace-hook.mjs`, two
   tiers: always-on phases (contracts re-run; optionality and outbox
   accuracy; poisoned-stderr quarantine — a hostile fake upstream's
   private markers never re-enter the result) and pinned-binary phases
   (run when `AI_WORKSPACE_BIN` names the built pinned binary; skipped
   with an explicit recorded reason otherwise, never silently):
   config-first registration suppressing auto-share over sentinel
   files, group-scoped MCP reading the approved schema bytes exactly
   from a consumer while single-project scope denies the same read,
   forced-off `project_tree`/`project_grep`/`project_file_write`
   confinement, the full send pipeline delivering a real
   `service_changed` event for the historical protocol change
   `4a084aab` with readback-verified targets, duplicate suppression,
   unknown-delivery, and hostile-flag refusal. **Executed end to end on
   this machine against the pinned binary: all phases green.**
6. **Benchmark (AC6)** — `scripts/benchmark-ai-workspace-context.mjs`
   plus `docs/m7/issue-37-benchmark.{md,json}`: pinned protocol over
   the real core change `3d7cfcfb` (the #121 custody fix) against its
   first parent; scratch-copy scope of exactly the three changed
   `run_history` files; three predeclared navigation tasks; 5
   repetitions per timing case; cold/warm/changed/revoked phases;
   closed privacy-probed aggregate. **Run twice; all gates green;
   results reproducible within noise.**
7. **Fixture family** — `tests/fixtures/ai-workspace/` (reviewed
   routing-manifest example, envelope example), declared synthetic in
   `tests/fixtures/fixture-provenance.json` (fail-closed gate green).

## Verification (gates run)

- `cargo fmt --check` — clean.
- `cargo clippy -p lekalo-cli -p lekalo-core -- -D warnings` — clean.
- `cargo test -p lekalo-cli -p lekalo-core` — 1765 passed, 0 failed.
- `node scripts/test-ai-workspace-contracts.mjs` — pass (with pinned
  Ajv 8.17.1).
- `node scripts/test-ai-workspace-hook.mjs` — pass, both tiers
  (binary phases executed against the pinned upstream).
- `node scripts/benchmark-ai-workspace-context.mjs` — pass, twice.
- `node scripts/check-contract-versions.mjs --base HEAD` and
  `--base HEAD^` — pass (96 contract artifacts; the new contract
  carries the product version `0.6.3`).
- `node scripts/check-authority.mjs`, `check-privacy.mjs`,
  `check-structure.mjs`, `check-model.mjs`,
  `test-fixture-provenance.mjs`, and the sibling contract gates
  (`test-target-protocol-contracts`, `test-context-contracts`,
  `test-trace-contracts`, `test-lockfile-contracts`,
  `test-impact-contracts`, `test-inspect-contracts`,
  `test-versioning-contracts`) — pass.
- `test-pilot-brownfield-ts.mjs` not re-run: its harness and fixtures
  are untouched by this branch (the research only borrows its probe
  pattern).
- Environment note: `test-model-contracts.mjs` fails on this machine
  identically on the base `ichinya/M7` worktree — a pre-existing local
  environment dependency (binary + Ajv provisioning), not caused by
  this branch; CI runs it after its own provisioning.

## Upstream gaps (documented, not faked)

All recorded in the integration guide; the load-bearing ones:

1. No role-alias management (no `group create`/`alias` command) — role
   aliases are a reviewed convention plus private local bindings.
2. No protocol triggers or watchers — the Lekalo hook is the trigger.
3. Event creation is neither transactional nor idempotent and has no
   machine-readable receipt (`create_workspace_event` inserts event,
   groups, targets, and artifact impacts through separate statements;
   the CLI prints only a numeric id) — the hook's outbox + keyed
   readback mitigates but cannot fully repair this.
4. Event impact is direct-links-only (both link kinds count, no
   transitive traversal, no schema-version filter).
5. No CLI `--json` and no typed provenance payloads — the envelope is
   the public structure; upstream stores a bounded body text.
6. No artifact/version-targeted events; no impact-JSON importer.
7. CodeGraph carries no Git revision/hash/parser-version provenance in
   its results and `codegraph_status` has no staleness field — the
   benchmark proves staleness behaviorally and the doc mandates the
   wrapper.
8. The accepted authority matrix admits no `workspace.change-event`
   kind — production sends stay refused by default until the successor
   procedure (#119/#120 owners) admits it; the gates prove the pipeline
   behind the explicit operator override.

## Acceptance mapping

| Criterion | Status | Evidence |
| --- | --- | --- |
| AC1 recommended group/setup with role aliases | **lekalo-side verifiable — verified** | guide §setup; gate binary phases executed config-first registration |
| AC2 protocol change → explainable affected-project event | **lekalo-side verifiable — verified** | hook envelope with typed chains; gate delivered a real event for `4a084aab` and verified readback targets + digests |
| AC3 consumer agent reads shared schemas | **lekalo-side verifiable — verified** | gate: group-scoped MCP from consumer reads exact committed bytes; single-project scope denies |
| AC4 fully functional without AI Workspace | **lekalo-side verifiable — verified** | no Rust dependency (zero Rust diff); gate optionality phases; hook `disabled`/`unavailable` accuracy |
| AC5 MCP scope/sensitive policy never silently widened | **lekalo-side verifiable — verified** | gate: forced-off flags override hostile inherited `=1`; tree/grep confined to shares; write tool refuses |
| AC6 CodeGraph context benchmark on core changes | **run** | `docs/m7/issue-37-benchmark.{md,json}`; reproduced twice; staleness + revocation gates green |
| AC7 no canonical model duplicated in notes | **documented boundary + gate inventory** | share allowlist excludes model/IR; notes are pointers; runtime audit of arbitrary future notes remains an authority-owner responsibility |
| AC8 public events/evidence reveal no private identity | **lekalo-side verifiable — verified** | closed schema; leak probes over every public channel including failure paths and hostile stderr; sentinels in gate + benchmark |

Verifiability split: AC1–AC5 and AC8 are enforced by gates committed in
this repository and were executed against the pinned upstream binary.
AC6 is evidenced by the completed, documented benchmark. AC7 combines
the gate inventory (nothing model-shaped can enter through the
manifest-reviewed surface) with the documented authority boundary; its
residual — proving a negative about all future operator notes — is a
documented boundary owned by the authority/privacy review, per the
research's repo split.

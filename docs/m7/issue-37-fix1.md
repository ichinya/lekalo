# Issue #37 — fix round 1 report

Fix round for the three independent reviews committed on this branch:
[`issue-37-review-devin.md`](issue-37-review-devin.md) (2 major, 10
minor), [`issue-37-review-codex.md`](issue-37-review-codex.md) (8 major,
3 minor), and [`issue-37-review-cline.md`](issue-37-review-cline.md)
(1 blocker, 3 major, 3 minor — a review of the mid-fix tree; its
blockers described the uncommitted state and are resolved by the same
commits). Every finding from all three reports is dispositioned below.
No gate, test, contract, or schema was weakened to pass; where the
implementation was wrong the implementation changed, and where a
documented claim overstated reality the claim changed.

## Fix commits

| Commit | Content |
| --- | --- |
| `ca0106c9` | hook rewrite: validated manifests, canonical policy ref, closed failure boundary |
| `0abbec6d` | example regenerated from a real planned run; projection-digest agreement |
| `33747176` | benchmark: repeated per-task retrieval, honest coverage, complete-document probes |
| `06808691` | gate: negative legs, positive controls, cross-platform hostile fakes |
| `4bff9dff` | docs: honest boundary proofs; CI wiring; `.gitignore` |
| `a6297eef` | implementation report semantics updated |
| `faa036c0` | gate: closed hook-state vocabulary assertion |

## Devin findings

| ID | Finding | Disposition | Evidence |
| --- | --- | --- | --- |
| M1 | `privacyPolicyRef.digest` uses the raw file digest; the accepted reference is the canonical policy identity | **fixed** (`ca0106c9`) | the hook now reads `currentAcceptedRef` from `contracts/privacy-policy.v0.3.2.manifest.json`, custody-checks the raw file against `policyFileDigest`, and emits `sha256:5a80966f…` (live plan output verified; matches the committed example) |
| M2 | benchmark stdout prints an absolute `rawRowsPrivate` path outside the probes | **fixed** (`33747176`) | the printed field is now the literal `"private-out-directory"`/`"discarded"` and the probes cover the complete printed document (aggregate + every sibling member) |
| M3 | explanation detail emits "modifiedd"/"addedd" | **fixed** (`ca0106c9`) | detail is now `${entry.path} ${entry.change}` — live plan output: `"docs/target-protocol.md modified"` |
| M4 | committed envelope example diverges from actual hook output | **fixed** (`0abbec6d`) | the example is the exact envelope of a real `planned` run over `4a084aab~1..4a084aab`; role set/order/steps/details are captured output; the contracts gate re-derives its event key and manifest digest independently |
| M5 | hook gate not wired into CI despite its header | **fixed** (`4bff9dff`) | `test-ai-workspace-hook.mjs` runs in the Contracts job with `NODE_PATH="$LEKALO_AJV_NODE_PATH"`; header corrected |
| M6 | zero-change diff still produces a sendable event | **fixed** (`ca0106c9`) | new closed `no-change` state; the send path never reaches upstream for an empty artifact set; gate phase B asserts it |
| M7 | availability probe treats a nonzero `--version` exit as available | **fixed** (`ca0106c9`), **reason-surface corrected in fix round 2** (`4ea3756a`) | the probe requires `kind === "ok"` (no send proceeds); round 1 recorded the precise reason only in the private outbox while the emitted result hard-coded `upstream-binary-missing` — fix round 2 derives one closed code used by BOTH surfaces: `upstream-probe-nonzero` (nonzero exit) vs `upstream-binary-missing` (spawn failure), gate-asserted on stdout and in the outbox with a leg for each shape |
| M8 | `--send` without `--manifest` silently uses the committed fixture; routes/watches/origin unvalidated | **fixed** (`ca0106c9`) | the manifest path is now REQUIRED (no fixture fallback); manifests are validated closed (path/role/version grammars, route enums, ≤64 routes/paths, duplicates); live probes: `manifest-missing`, `manifest-reaction-unknown`, `manifest-too-many-routes` |
| M9 | boundary table names tests that do not exist | **fixed** (`4bff9dff`) | the table now names exactly the committed proofs (share inventory, exact-byte read, sentinel, wrong-group/single-project denials, positive-control confinement) |
| M10 | `.ai-workspace.local.json` not ignored in this repo | **fixed** (`4bff9dff`) | `/.ai-workspace.local.json` added to `.gitignore` |
| M11 | `unknown-delivery` results omit the closed reason | **fixed** (`ca0106c9`) | every `unknown-delivery` result carries `reason` (e.g. `upstream-nonzero`, `readback-targets-mismatch`); gate asserts it |
| M12 | dead `privateRoot`; schema indentation | **fixed** (`ca0106c9`/`33747176`) | both cleaned |

## Codex findings

| ID | Finding | Disposition | Evidence |
| --- | --- | --- | --- |
| F1 | runtime accepts unvalidated manifest data; invalid identity-bearing envelope; digest binds private members | **fixed** (`ca0106c9`, `0abbec6d`) | closed manifest validation (see M8); the envelope is validated against the compiled closed contract shape before print/send (live probe: a poisoned reaction refuses `manifest-reaction-unknown` before any output); the public key binds only the neutral public routing projection — mutating `workspaceSlug` and adding private members leaves the key byte-identical (verified) |
| F2 | blind retry after unknown delivery; no durable sending checkpoint | **fixed** (`ca0106c9`) | unverified attempts reconcile by key through a source event-list readback before any new create (with a `reconciled/not-found` checkpoint); a `sending` state is recorded around invocation. Live crash-shape probe: a fake that inserts then exits 1 → run 1 `unknown-delivery`, run 2 found the inserted event **by key** and verified it instead of re-creating — `creates` stayed at 1 |
| F3 | missing graph evidence becomes verified delivery; reported roles can exceed the verified set | **fixed** (`ca0106c9`) | the graph read is mandatory and fail-closed (`readback-graph-unavailable` ⇒ unknown-delivery, never implicit-empty); zero-target delivery no longer bypasses verification (kind/title/body/key always checked); the result reports `affectedRoles` restricted to verified targets plus `unverifiedDeclaredRoutes` separately |
| F4 | event creation can disclose an unreviewed private recipient outside the group | **fixed** (`ca0106c9`) | recipient preflight before create: the workspace's linked dependents must be exactly the reviewed consumer slugs — `unreviewed-recipients-present` refuses the send (count only, no names); zero linked consumers refuses as misrouting; post-send verification asserts exact target-set equality |
| F5 | privacy policy reference names the file digest | **fixed** — same as devin M1 | |
| F6 | probes omit channels that disclose host paths and private error text; uncaught exceptions leak stacks; usage reflects values | **fixed** (`ca0106c9`, `33747176`) | every exit path is closed: usage errors carry only the flag name, filesystem failures map to `outbox-unavailable`/`outbox-corrupt` (probed with an outbox path that is an existing file: exit 3, 0 stderr bytes), and an `uncaughtException`/`unhandledRejection` boundary emits `refused/internal-error`; benchmark probes cover the complete printed document |
| F7 | gate unwired; missing/ineffective negative legs; hard-coded project id | **fixed** (`06808691`, `4bff9dff`) | CI wiring (see M5); genuine wrong-group denial (a project in `other-dev` cannot read the core share by item id); core project id resolved from the workspace listing; positive controls: `project_tree` must show the shared schema and `project_grep` must match shared content before the absence assertions count |
| F8 | benchmark measures symbol search once per task; no context/budget comparison; coverage is substring-only | **fixed** (`33747176`) | retrieval now runs REPS repetitions per task per strategy (15 samples each at `--reps 5`), adds `codegraph_context` with a bounded budget, records returned context bytes, unresolved-reference counts, and an explicit `tokenEstimate: "unknown"`; coverage checks expected symbols (recorded misses: baseline budget miss on the regression-test task, CodeGraph FTS miss on the quarantine symbol); changed-file sync timings and a deleted-file phase added; staleness and freshness are asserted, not just recorded |
| F9 | delivered event omits provenance claimed as verified | **fixed** (`ca0106c9`, `4bff9dff`) | the upstream body now carries the event key, full source revisions, manifest identity+digest, per-artifact old/new identity+version+digest, roles, and the accepted authority reference; the doc claim narrowed to "verifies the exact target set"; the gate asserts body equality |
| F10 | limitation vocabulary is a pattern, not an enum | **fixed** (`ca0106c9`) | schema `limitation` is a real `enum` (+ `no-declared-subscribers`); the contracts gate adds an Ajv-enforced negative vector (`private-payroll-sentinel` must fail validation) |
| F11 | a range with no approved changes is still a sendable change event | **fixed** — same as devin M6 | |

## Cline findings

| ID | Finding | Disposition | Evidence |
| --- | --- | --- | --- |
| 1 (blocker) | committed gate Phase B fails deterministically (`--base HEAD` → `no-change`) | **fixed** (`06808691`) | Phase B now plans over `4a084aab~1..4a084aab` and separately asserts the honest `no-change` state; both tiers green (outputs below); the implementation doc claims corrected |
| 2 (blocker) | committed benchmark under-samples retrieval vs its evidence claims | **fixed** (`33747176`) | the rewrite is committed and the evidence files were regenerated from it (two complete runs, reproducible); `samples: 15` recorded in the aggregate |
| 3 (blocker) | uncommitted rewrite + stash in the worktree | **fixed** | the rewrite was recovered and committed (`33747176`); the stash was verified fully superseded (`git diff stash@{0} HEAD` shows HEAD strictly ahead) and dropped; `git status` clean |
| 4 (major) | prior reviews lack dispositions | **fixed** | this document is the disposition table for all three reviews |
| 5 (major) | AC1/AC3 verification leg rests on an unreproducible gate | **fixed** | the gate is committed green (both tiers) and CI-wired; the documented-convention nature of role aliases stays honestly labeled |
| 6 (major) | `rawRowsPrivate` absolute-path echo | **fixed** — same as devin M2 | |
| 7 (minor) | uncommitted benchmark shape diverges from the frozen evidence schema version | **rebutted** | the aggregate schema string `lekalo/ai-workspace-context-benchmark/v0.6.3` is not a `contracts/` artifact; per `docs/versioning.md` the version of a change-set is the product version of its commit (still `0.6.3`), and script + evidence doc + evidence JSON were regenerated together in one reviewed commit — no silent edit, no invented version |
| 8 (minor) | diagnostics-registry convention inapplicable | **rebutted (verified negative)** | agreed with the reviewer's own conclusion; no registry entry is introduced |
| 9 (minor) | closed hook-state vocabulary undocumented and unasserted | **fixed** (`faa036c0`, `a6297eef`) | `no-change` is in the documented set; the gate now asserts every printed state is a member (executed run observed: `delivered, no-change, planned, refused, unavailable, unknown-delivery`) |

## Verification outputs (real runs, this machine)

```
$ cargo fmt --check                       → exit 0
$ cargo clippy --all-targets -- -D warnings
    Checking lekalo-cli v0.6.3 … Finished `dev` profile … in 1m 16s   → exit 0
$ cargo test -p lekalo-cli -p lekalo-core
  cargo test passed: 1765 failed: 0

$ NODE_PATH=<ajv-8.17.1> node scripts/test-ai-workspace-contracts.mjs
{"ok":true,"gate":"ai-workspace-contracts","schema":"contracts/ai-workspace-event.schema.v0.6.3.json",
 "example":"tests/fixtures/ai-workspace/event-envelope.example.json"}

$ AI_WORKSPACE_BIN=<pinned 8fdf818 build> LEKALO_AJV_NODE_PATH=<ajv-8.17.1> \
  node scripts/test-ai-workspace-hook.mjs
{"ok":true,"gate":"ai-workspace-hook","binaryPhases":"executed","upstream":"configured",
 "hookStates":["delivered","no-change","planned","refused","unavailable","unknown-delivery"]}
$ node scripts/test-ai-workspace-hook.mjs            (no binary)
{"ok":true,"gate":"ai-workspace-hook","binaryPhases":"skipped",
 "reason":"upstream-binary-not-configured (set AI_WORKSPACE_BIN to run the full proof)"}

$ node scripts/check-contract-versions.mjs --base origin/ichinya/M7
{"ok":true,"product":"0.6.3","contractArtifacts":96,"base":"origin/ichinya/M7"}
$ node scripts/check-authority.mjs   → PASS (12 allowed, 27 forbidden, 15 malformed fixtures)
$ node scripts/check-privacy.mjs     → "accepted": true
$ node scripts/check-structure.mjs   → exit 0
$ node scripts/check-model.mjs       → exit 0
$ node scripts/test-fixture-provenance.mjs → {"ok":true,"families":64,"synthetic":64}

$ node scripts/test-contract-versions.mjs → {"ok":true,"cases":6}
$ test-target-protocol-contracts / test-trace-contracts / test-context-contracts /
  test-lockfile-contracts / test-versioning-contracts   → all exit 0

$ AI_WORKSPACE_BIN=<pinned> node scripts/benchmark-ai-workspace-context.mjs \
    --upstream <pinned> --reps 5      → ok:true, twice; samples 15/15;
  staleness/freshness/deletion/revocation gates all true
```

Adversarial probes (all synthetic, isolated temp state):

- poisoned reaction / 65 routes / unknown watches / duplicate roles →
  closed `manifest-*` refusals, exit 3;
- manifest with mutated `workspaceSlug` + private extra members →
  identical event key (private data outside the digest domain);
- outbox path occupied by a file → `refused/outbox-unavailable`, 0 bytes
  on stderr (was a Node stack before the fix);
- create-then-crash fake → run 1 `unknown-delivery`, run 2 reconciles by
  key and verifies the inserted event; upstream create count stays 1.

`test-model-contracts.mjs` fails on this machine identically on the base
`ichinya/M7` worktree (local binary/Ajv provisioning; CI provisions its
own) — pre-existing, not caused by this branch.

## Acceptance status after the fix round

- AC1/AC3: gate-committed and CI-wired; binary phases executed against
  the pinned binary; role aliases remain an honestly documented
  convention (no upstream registry — unchanged upstream gap).
- AC2: verified with exact target-set verification (no unreviewed
  recipients) and a delivered body that carries identities + digests.
- AC4/AC5: unchanged verdicts (verified); the hostile-flag and widening
  refusals are gate-asserted.
- AC6: benchmark reworked to the research protocol (repeated per-task
  retrieval, context tool, coverage with recorded misses, explicit
  unknowns) and run twice.
- AC7: documented boundary + gate inventory (unchanged, honestly
  scoped).
- AC8: verified across the complete printed/hook/envelope channels
  including failure paths and hostile upstreams.

# Issue #88: Codex independent review round 4

**Verdict: ACCEPT.** Fix `471589da` closes the round-3 P2: reconstructing the
Model ID and revision no longer admits a native-shaped occurrence. The original
CLI and public-profile reproductions now refuse. Actual pre-fix stores are
ineffective without being rewritten, and their old accepted-risk done receipts
are stale. No residual finding was reproduced in the requested module, subject,
profile-override, selector-kind or mixed-input probes.

Reviewed on 2026-10-05 in `ichinya/m7-issue-88`, worktree
`C:/Users/User/orca/workspaces/lekalo/m7-issue-88`. Starting worktree/index was
clean at `949599c777058f2918f84629bf525e3a069815d0`. Fix under review:
`471589dab00cdb96dea21a0f5f54210bdd26a19c`, following Codex round-3 report
`dfa685ee`. I inspected that report, the fix report, Devin round-4 report, source
delta, emitted producer shape, policy owners, matcher, schemas and live gates.
Devin's verdict and the fix report's recorded results are not acceptance evidence;
the observations below come from this round's rebuilt binary and external probes.

The live issue was fetched again with
`gh issue view 88 --repo ichinya/lekalo --json title,body,state,url,updatedAt`.
[Issue #88](https://github.com/ichinya/lekalo/issues/88) remains open with seven
acceptance criteria, dependencies including #84, and body update time
`2026-08-30T10:17:56Z`.

## Source and effectiveness boundary

`crates/lekalo-core/src/waivers/policy.rs:51` now checks the reserved Model
producer's emitted shape: a known two/three-segment semantic symbol, subject
equal to that symbol, known module equal to its owning first segment, and known
exact confidence. It also retains the target-bound Model ID, known Model/IR,
canonical Model/IR revision, unsupported adapter/capabilities and unknown path.
This corresponds to the Model dependency-witness producer at
`ai_lint/mod.rs:478` and the projection in `waivers/lint.rs::facts`. Native depth
at `ai_lint/mod.rs:749` instead emits an opaque subject and unknown symbol.

The trait default at `policy.rs:81` refuses the reserved Model target. The
default fingerprint requirements at `:84` grant Model-only applicability only
after both the shared shape check and the owner's admission. Validation, lint
and target owners explicitly opt in at `:102`, `:141` and `:196`, respectively,
while retaining their rule/capability selection and resolved policy authority.
The CLI's `Loaded` owner delegates both admission and applicability to those
owners. It does not accidentally inherit the new refusing default.

`waivers/mod.rs:421` independently checks the shared producer shape during
effectiveness. This matters beyond CLI admission: an external owner overriding
both domain admission and fingerprint requirements cannot authorize an invalid
reserved shape. Full fact, occurrence, profile and approval bindings still apply;
the tests below deliberately rebind those values so a stale approval does not
mask the producer-domain result.

## Original round-3 reproductions and old applied stores

I copied the original three reproducer sources byte-for-byte outside the checkout
and executed them against the current CLI/core. SHA-256 of the copies:

| Reproducer | SHA-256 |
| --- | --- |
| CLI matrix | `bc33c2b478d4fe0f7375ac0b3e3ec346bac0b457edc69a3c665cfd5d3b89e837` |
| Native-depth reproducer | `615b12834474a35940a434aa9e12b977344b80e163ca6fa90a8db5eec5250bb9` |
| Default-inheriting public owner | `23fe0088131d55831c7a830025e2b276cfa4ec7de890435bc620f71244b0c23c` |

The unchanged matrix completed **56 CLI calls**. All its recipe-forged and
target-only claims now refuse; genuine Model controls still accept. Its capability
branch takes the refusal path, so it makes one fewer call than round 3.

The unchanged depth script freshly emits the real native depth occurrence and
reconstructs the same Model recipe. It now stops at its old acceptance assertion
(`verbatim-depth.mjs:51`): the CLI returns exit 1,
`waivers-ineligible-candidate`, where the old reproducer expected exit 0. The
unchanged API reproducer likewise stops at its old permissive-default assertion
(`api-verbatim/src/main.rs:53`): `producer_domain_admitted` is now false. These
expected failures of old acceptance assertions demonstrate changed behavior;
they are not reported as passing product tests. Separate current-result probes
continue through apply, stored audit, done, live lint and mixed inputs.

A **19-call replay**, executed again in this review, copied the actual old depth
project and actual applied capability candidate from the round-3 captures. It
used their captured facts, commands and approval-bound stores. An additional
current-round probe replayed the old capability done receipt.

| Current operation on actual pre-fix evidence | Observed result |
| --- | --- |
| Old native-depth preview and apply commands | Both exit 1, `waivers-ineligible-candidate`; no store created when absent |
| Matching old depth store, locked audit with `--check` | Exit 3; `unverifiable`, `fingerprint-unverifiable`, effective false, waived 0; fact retained |
| Old depth accepted-risk `--done` | Exit 1, `waivers-done-stale` |
| Actual native lint with that old store | Exit 3, waived 0; native findings unchanged from raw output |
| Original native plus reconstructed depth fact, either order | Exit 3; forged approval unverifiable, waived 0 |
| Actual old optional-capability candidate | Preview/apply exit 1; matching audit unverifiable, waived 0; original advisory gate retained |
| Old capability accepted-risk `--done` | Exit 1, `waivers-done-stale` |

Audit/lint retained the old store bytes exactly; there was no migration. A
matching supplied-fact audit reports `unverifiable`. Actual native lint regenerates
the native target and occurrence ID, so a relabelled entry is nonmatching and
reports `unexamined` for its partial input. It remains ineffective. Claiming
`unverifiable` for that nonmatching live occurrence would misstate the evidence.
Optional capability audit can exit 0 while an entry is unverifiable: its original
gate is advisory. It does not become accepted-risk or contribute to waived counts.

## Residual bypass probes and genuine positives

My separate `boundaries.mjs` harness completed **48 CLI calls**, importing no
repository gate helpers. It creates exact project-scope entries with new fact and
approval bindings, validates the wires, and checks unchanged store bytes on
refusal. The wrong-module case uses **notify, an actually declared module** in
the same project; missing-project membership cannot explain that refusal.

| Probe | Observed result |
| --- | --- |
| Valid `subject == symbol`, exact confidence, but module `notify` for `planner.focus_flow` | Preview/apply exit 1; matching audit exit 3, unverifiable/fingerprint-unverifiable, waived 0 |
| Known project symbol and exact confidence, retaining the opaque native subject; Model ID recomputed | Same refusal and audit result |
| Different known semantic subject with the original known symbol; Model ID recomputed | Same refusal and audit result |
| Otherwise valid Model shape with known high confidence | Same refusal and audit result |
| Model-derived validation rule `semantic.portable-target-reference`, admitted default profile | Preview/apply/audit/done exit 0, waived 1; unchanged fact; profile-owned info/advisory gate becomes accepted-risk |
| Same validation obligation using capability selector kind | Preview/apply exit 1; audit exit 3, unverifiable, waived 0 |
| Valid Model-derived `semantic.public-output-private-type`, supplied warning under that profile's error policy | Preview/apply exit 1; audit exit 3, non-waivable/profile-non-waivable, effective denied, waived 0 |
| Genuine Model-derived optional `transport.streaming` gap under resolved `node-postgres-http` | Preview/apply/audit/done exit 0, waived 1; unsupported outcome remains unsupported |
| Target owner given a registered validation rule selector instead of a capability | Preview/apply exit 1; audit exit 3, unverifiable/fingerprint-unverifiable, waived 0 |
| Target owner given an unknown capability | Preview/apply exit 1; audit exit 3, unverifiable, waived 0; missing policy, not a shape failure |
| `transport.streaming` labelled as a rule | Preview/apply exit 1, `waivers-fact-invalid`; it is not an admitted registry rule |

Fresh genuine Model producer output also completed preview, exact-plan apply,
locked audit and **actual `ai-lint --check`**, all exit 0 with waived 1. The
project-scope live positive reports raw 1, active 0, waived 1 and a known waiver
link. At `2026-11-01T00:00:00Z` it still unblocks; at the following second it
returns exit 3 with expired inventory and waived 0, retaining the original fact.

For scope controls I supplied a different valid known project symbol/subject
with its own recomputed Model occurrence ID. Project- and module-scoped stores
both leave it denied/unwaived; the old entry is unexamined. A known changed
approved revision produces stale inventory and waived 0 in both audit and actual
lint, and invalidates the earlier done receipt.

The capability positive exercises the documented neutral authoring/audit target
profile path. Live lint emits rule facts (`waivers/lint.rs:90`), not target-profile
capability facts. This review does not claim that a capability-selector store
suppresses a rule occurrence, or that a separate target producer was executed.
The actual lint waived-1 positive is the genuine Model rule occurrence.

## Independent public-owner probe

I compiled a new external Cargo program against the reviewed core, offline and
locked, without adding tests or files to the checkout. It produced **19 audits**:

- An owner implementing only `reference` and `rule` refuses both the original
  round-3 native recipe and an otherwise valid Model shape carrying its native
  rule selector. All five native dimensions remain required-known; audits are
  unverifiable, denied, waived 0.
- An owner explicitly returning true from `producer_domain_admitted` still gets
  native requirements for a bad reserved shape. Audit independently refuses it.
- A stronger owner explicitly returning true **and** overriding requirements to
  `model_only()` still cannot accept wrong module, opaque subject, rehashed unknown
  symbol, non-exact confidence, withheld/unsupported module or a capability
  projection retaining an opaque subject. All seven shapes refuse under both
  owners: **14 override controls**, fingerprint-unverifiable, waived 0.
- That stronger owner accepts a genuine complete Model occurrence. Mixed valid
  Model and forged opaque-subject facts, in both orders, remain denied overall;
  only the genuine occurrence becomes accepted-risk, waived 1.

Strict Ajv 8.17.1 independently validated all 19 API audit artifacts. The default
change therefore requires deliberate opt-in for future owners, while the three
existing recognized owners retain their successful Model paths.

## Acceptance-criterion evidence retained

This is a focused correction review, with the seven live-issue criteria checked
against the current source and current executed evidence:

| AC | Source and current evidence |
| --- | --- |
| 1. Visible suppressed finding/link | `waivers/lint.rs::apply`; successor gates compare retained native fields/counts. Independent actual Model lint retains raw 1 and waiver link with waived 1; matching audit retains the exact fact. |
| 2. Expiry restores original gate | `waivers/mod.rs::audit` deadline check; current successor gates plus own actual lint equality/next-second control, exit 0 to exit 3. |
| 3. No unrelated-symbol leakage | Exact matcher and `scope_matches`; current scope gates plus own different-symbol controls under project and module scope, waived 0. Wrong declared module also refuses reserved-domain admission. |
| 4. Model/adapter revision invalidation | Required pin comparison and profile-owned applicability; current five-dimension/native-pin gates, unchanged old matrix unknown/withheld revision controls, own known-revision audit/live-lint stale control. Unsupported native dimensions still refuse. |
| 5. Security-critical non-waivable fixture | Validation owner consumes actual registry/profile severity; current security-critical gate and own warning-supplied/error-profile preview/apply/audit refusal. Required evidence remains enforced by resolved owners. |
| 6. Active/expiring/expired/stale audit | Current status goldens are compared to live output by all successor gates; own active positive, expired live inventory, stale revision, pre-fix and malformed-domain unverifiable controls. |
| 7. Review/done waiver evidence | Audit comparison and full done digest; current changed-decision/base gate plus own actual pre-fix depth/capability and changed-revision done refusals. Store bytes retained on audit/refusal. |

The supplied-report boundary remains explicit: an ID hash is not authenticated
external producer evidence. Completely rewriting an input into another valid
Model-shaped occurrence does not establish that an external tool emitted it.
Current CLI checks project/profile, Model/IR, known symbol/module membership and
requested lock; source-specific report truth remains producer-owned. No signed
reviewer, HLV/AIFHub execution, #84 branch integration or new audit persistence
is inferred from these checks.

## Commands, results and custody

Executed against the freshly built reviewed source:

```powershell
cargo build --locked -p lekalo-cli
$env:NODE_PATH='C:/Users/User/AppData/Local/Temp/lekalo-ajv-8.17.1/node_modules'
$env:LEKALO_AJV_NODE_PATH=$env:NODE_PATH
node scripts/test-waivers-contracts.mjs
node scripts/test-waiver-input-contracts.mjs
node scripts/test-waiver-audit-contracts.mjs
node scripts/test-ai-lint-waivers-contracts.mjs
cargo test --locked -p lekalo-core waivers:: --lib
cargo fmt --all -- --check
node scripts/gen-waivers-contracts.mjs --check
git diff --exit-code dfa685ee HEAD -- contracts tests/fixtures .github/workflows/ci.yml
```

| Check | Result |
| --- | --- |
| Locked CLI rebuild | Passed |
| Three successor gates | Each passed: `live:true`, exact Ajv 8.17.1, `audits:39`, `nativePinControls:7`, `producerDomainControls:9`, `modelShapeControls:14` |
| Frozen predecessor gate | Passed, live binary and schema 0.6.4 |
| Focused core waiver tests | 15 passed, zero failed; 1053 filtered |
| Format and contract generator | Read-only checks passed |
| Own current-result CLI executions | 56-call verbatim matrix, 19-call pre-fix replay, 48-call boundary harness; assertions passed |
| New external public-owner program | Offline locked build/run passed; 19 schema-valid audits and 14 explicit-override shape refusals |
| Frozen boundaries | Schemas, fixture goldens/provenance and CI byte-unchanged relative to `dfa685ee`; no weakening in this fix |
| CI order | Exact Ajv install at `ci.yml:198`, Cargo build at `:204`, predecessor gate at `:273`, all three successor gates at `:280`-`:282` |

External scripts, raw envelopes, copied pre-fix evidence and API artifacts are
under
`C:/Users/User/AppData/Local/Temp/lekalo-issue88-review4-da99d6ce394544e6a43864f69e4c8129`.
The 56 numbered JSON captures belong to the unchanged matrix; `calls.json`
contains the separate pre-fix replay, `boundary-calls.json` the 48-call own
harness, and `api-boundary-results.json` the new API audits. Original round-2
and round-3 evidence directories were read/copied and left untouched. Reproduce
the new API run with:

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR='<evidence>/api-target'
cargo run --offline --locked --quiet --manifest-path '<evidence>/api-boundaries/Cargo.toml' -- '<evidence>/genuine-model-input.json' '<evidence>/native-facts.json' '<evidence>/api-boundary-results.json'
```

Full unfiltered suites, Clippy, hosted CI and non-Windows execution were not run
in this review. No implementation, schema, fixture, gate or other-worktree edits
were made. The sole repository delivery is this review document, staged by its
exact path and committed locally with
`docs(m7): codex review round 4 for issue 88`. No push.

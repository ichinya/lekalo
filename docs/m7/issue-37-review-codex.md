# Issue #37 — independent implementation review (Codex)

**Verdict: ISSUES — 11 findings: 0 blocker, 8 major, 3 minor.**

The documented optional setup and ordinary shared-schema read work. The
implemented send path does not yet establish the claimed delivery and privacy
guarantees: independent probes reproduced blind retries, false delivery,
unreviewed recipients, invalid envelopes, and private output. The default
authority refusal limits exposure but does not make the override path correct.
Do not mark the implementation's acceptance table fully verified yet.

Reviewed pushed HEAD `c4618a780f9360c23de91cf281432787afefd786` on
`ichinya/m7-issue-37`, against `origin/ichinya/M7` =
`9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`. The implementation tip is
`81991d4e`; the later commit adds the existing independent Devin review.
The three-dot diff contains 15 files / 3560 insertions / no deletions and no
Rust changes. This review changes only this document.

## Numbered findings

1. **major — The runtime public boundary accepts unvalidated manifest data
   and can emit an invalid, identity-bearing envelope.**
   Evidence: `scripts/ai-workspace-hook.mjs:152` validates only the manifest
   kind, array presence, and a role character pattern; `:239` and `:240` copy
   unchecked `dependencyKind` / `reaction` into public explanations; `:519`
   emits without validating the envelope against its schema. Changing a
   fixture route's reaction to the synthetic marker
   `token=review-private-sentinel` produced exit 0 / `planned` and printed
   that marker. Independent Ajv validation rejected the result at
   `/impact/affectedRoles/0/explanation/1/reaction`. A manifest with 65
   routes and empty watches also produced exit 0 and `manifest.routes:65`,
   violating the schema's maximum 64. Neither approved-path grammar,
   watches, routing enums, nor an overall envelope byte bound is enforced
   on this path. The fixture-only gate does not validate operator input.
   Additionally, `:167` hashes the complete installation-local manifest,
   including `workspaceSlug` and arbitrary private members, into the public
   manifest digest/event key; it is not a digest of a neutral public routing
   projection. This contradicts the no-private-identity-hashes rule at
   `docs/integrations/ai-workspace.md:56`.
   Required correction: validate a closed, bounded routing input and the
   actual emitted envelope; bind the public key to public routing data and
   keep private slug bindings outside that hash domain.

2. **major — Repeating an unknown delivery blindly sends again.**
   Evidence: `scripts/ai-workspace-hook.mjs:536` only suppresses entries
   containing a `delivered` state; `:546` appends another `planned` state
   for every other entry and `:575` calls create again. The unknown states
   recorded at `:579`, `:590`, `:598`, and `:623` never cause reconciliation
   before the next invocation. Two identical invocations against a fake
   upstream whose create increments a counter and exits 1 both returned
   exit 4 / `unknown-delivery`, and create was called **twice**. A real
   upstream may already have inserted an event before returning failure.
   This directly contradicts `docs/integrations/ai-workspace.md:292`
   (retry only after keyed reconciliation). Readback is by the freshly
   returned numeric ID, not a search for an existing event key. There is
   also no durable `sending` checkpoint or writer lock protecting a crash
   between invocation and recording the result.
   Required correction: persist invocation intent, refuse/reconcile pending
   and unknown attempts by key before any new create, and implement the
   stated single-writer custody or explicitly enforce that prerequisite.

3. **major — Missing graph evidence becomes verified delivery, and reported
   roles can exceed the verified target set.**
   Evidence: `scripts/ai-workspace-hook.mjs:364` accepts details even when
   the graph response is absent/errored; `:606` turns that into `[]`; `:613`
   records `delivered` without calling `verifyDelivery`. A fake upstream
   returning event kind `service_deleted`, wrong title/body, and an error
   for service graph produced exit 0 / `delivered` with no affected roles.
   Thus even event identity is unchecked in the zero-target branch. In the
   nonempty branch, `:612` verifies only manifest slugs intersecting the
   current graph, while `:637` reports **all** manifest-affected roles. A
   pinned-binary probe with only greenfield linked reported both greenfield
   and brownfield as affected after verifying just greenfield. Missing
   subscriptions are silently removed from the delivery obligation.
   Required correction: require a successful, structurally valid graph
   read, verify kind/title/key/body even for zero subscribers, and distinguish
   planned candidates, actual recipients, and missing declared routes.

4. **major — Event creation can disclose an unreviewed private recipient
   outside the chosen group.**
   Evidence: `scripts/ai-workspace-hook.mjs:292` invokes unscoped upstream
   `event create` before any group/recipient privacy preflight; group scope
   is applied only to readback at `:318`. Verification at `:383` tests
   inclusion of expected targets, not absence of unexpected targets or
   unsafe artifact snapshots. Using the pinned real binary, a dedicated
   synthetic database contained neutral core/greenfield projects in
   `lekalo-dev` and an extra synthetic private consumer in `outside-group`
   linked to core. The hook reported `delivered`; querying the other group
   found the event and its details included `private-payroll-sentinel`.
   This is an observable publication/recipient expansion, not a claim that
   the MCP widening flags turned on. Upstream deliberately snapshots all
   source dependents; neutral envelope roles do not sanitize those rows.
   Required correction: fail before create when the full upstream affected
   set, groups, project identities, or artifact snapshots are outside the
   reviewed public-safe mapping. Post-send filtering cannot undo exposure.

5. **major — The emitted privacy policy reference names the file digest
   instead of the accepted policy identity digest.**
   Evidence: `scripts/ai-workspace-hook.mjs:251` / `:257` hashes raw policy
   bytes. Live plan output carries
   `sha256:1fb9047934146e4ec76029b9c2c00b9fac4f605193911147974869b8794ab7dc`.
   `contracts/privacy-policy.v0.3.2.manifest.json:20` identifies the accepted
   policy as
   `sha256:5a80966fa628fd4c9452325d34e7191f40ebb7a9cb49185c91d413861fb18384`;
   `:25` explicitly assigns `1fb90479...` to the separate `policyFileDigest`
   domain. `scripts/check-privacy.mjs:197` and `:287` enforce the canonical
   policy identity with its own digest omitted. The committed envelope
   example uses the accepted identity, so fixture and producer disagree.
   Required correction: emit the accepted canonical policy reference;
   if raw-byte provenance is needed, give it a separately declared field
   and domain, then assert runtime/fixture/policy-reference agreement.

6. **major — Privacy probes omit public output channels that disclose host
   paths and private error text.**
   Evidence: `scripts/benchmark-ai-workspace-context.mjs:421` probes only
   the aggregate, but `:435` prints its sibling `rawRowsPrivate: OUT_DIR`.
   A real run with a temporary `--out` directory returned `ok:true` and
   its parsed stdout field equaled the **absolute** directory path. The
   hook has the same failure-channel gap: `scripts/ai-workspace-hook.mjs:532`
   / `:405` perform filesystem operations without a closed exception
   boundary, and `:642` calls main without a catch. Setting outbox to an
   existing ordinary file produced exit 1, no JSON result, and a Node
   stderr stack containing the full synthetic private outbox path.
   The usage handler at `:423` also reflects arbitrary unknown arguments
   at `:444`. Child-stderr quarantine does not cover these channels.
   Required correction: emit a nonidentifying raw-output marker, translate
   all local failures/usage errors to bounded safe codes, and probe the
   complete stdout/stderr/result envelope before publication.

7. **major — The integration gate is unwired, and its claimed privacy/scope
   proofs have missing or ineffective negative legs.**
   Evidence: `.github/workflows/ci.yml:80` registers only the example
   contract gate; no workflow invokes `test-ai-workspace-hook.mjs`.
   Its header at `scripts/test-ai-workspace-hook.mjs:10` nevertheless says
   CI runs the always-on phases. The purported core confinement test uses
   hard-coded project ID 1 at `:373`, although `:293` registers greenfield
   first and `:296` registers core fourth. The test checks only absence of
   sentinel strings, not a successful tree/grep over the expected shared
   core schema, so an empty response or denial can pass. The
   "wrong-group" block at `:343` reads the stranger in the **same** group
   and tests only a single-project denial; no wrong-group request runs.
   The guide's DB-deletion and digest-mismatch proofs at
   `docs/integrations/ai-workspace.md:363` / `:364` have no such tests.
   Private names, labels, dependency paths, and event metadata are not
   poisoned and checked; the principal adversarial probe is child stderr.
   The contract leak walker at `scripts/test-ai-workspace-contracts.mjs:156`
   also skips primitive array entries and scans only the benign fixture.
   Required correction: wire the cheap integration phases into CI, resolve
   project IDs from the fixture, assert positive and negative controls,
   test the advertised boundaries over hostile data on actual channels,
   and revise claims until those tests exist. Existing gates were not
   weakened by this diff, but these new guarantees are not release-gated.

8. **major — The completed benchmark measures symbol search, not the claimed
   context comparison, and its retrieval repetitions are overstated.**
   Evidence: `scripts/benchmark-ai-workspace-context.mjs:303` executes each
   of three tasks **once**, outside the five-repeat loops for reindex/sync.
   The retrieval medians at `:405` / `:406` combine three different task
   latencies, not five repetitions of each timing case as claimed by
   `docs/m7/issue-37-benchmark.md:36`. Calls at `:317` are workspace metadata
   and `codegraph_search`; no context/node/callers/callees retrieval or
   matched output budget is exercised. Coverage at `:330` is a filename
   substring test, not expected symbol/caller evidence or task success.
   Baseline `maxBytes` does not bound reads (`:205` reads whole files), and
   its byte reduction at `:214` ignores the accumulated total; that metric
   is then unused. Cold retrieval, changed-file timing, deletion/rename,
   returned-context bytes/token estimate, unresolved references, and raw
   public counts required by the research protocol are absent. Finally,
   `:363` computes freshness-after-sync without asserting it, and the
   private-sentinel check at `:425` covers the constructed aggregate,
   not actual retrieval results. The real search benchmark reproduced;
   the missing measurements were not run by doing so.
   Required correction: implement the declared context/budget/quality
   comparison and repeated timing cases, assert freshness/refusal controls,
   and record explicit unknowns for measurements not performed.

9. **minor — The delivered event omits the provenance/explanations claimed
   as readback-verified.**
   Evidence: `scripts/ai-workspace-hook.mjs:272` sends only key, kind,
   shortened revision range, changed paths, role list, completeness, and
   schema identity. It omits old/new identities/digests and typed chains;
   successful send output at `:631` also omits the envelope. The outbox
   retains only key/kind/states at `:543`, so there is no stored public
   envelope location for a consumer to follow. A real delivered event
   confirmed no artifact digest beyond the event key and no typed
   subscription explanation. The gate at
   `scripts/test-ai-workspace-hook.mjs:438` verifies recipient names only,
   although `docs/m7/issue-37-implementation.md:153` claims readback of
   targets **and digests**. Required correction: carry a bounded safe
   envelope or an addressable immutable envelope reference in the event,
   and independently assert received provenance; otherwise narrow the claim.

10. **minor — The claimed closed limitation vocabulary is only a pattern.**
    Evidence: `contracts/ai-workspace-event.schema.v0.6.3.json:298` declares
    a generic string pattern and lists allowed tokens only in prose at
    `:303`. Pinned strict Ajv accepted a mutated envelope containing the
    undeclared limitation `private-payroll-sentinel`. The gate at
    `scripts/test-ai-workspace-contracts.mjs:194` checks the committed
    example against words parsed from the description; it never proves
    the schema rejects an unknown token. Object-member closure is real,
    but this vocabulary is open. Required correction: encode an enum and
    add an unknown-token negative vector using the validator itself.

11. **minor — A range with no approved changes is still a sendable change
    event.**
    Evidence: `scripts/ai-workspace-hook.mjs:475` allows empty artifacts;
    `:485` labels the result `schema-change`; `:575` has no no-change guard.
    Planning `origin/ichinya/M7..HEAD` produced `planned`, zero artifacts,
    zero affected roles, and a change event key. Send with the explicit
    override will still invoke upstream, which fans the service event out
    to all linked dependents. The research's unrelated-file case is absent
    from the gate. Required correction: return a defined no-change result
    without creating an event, and test identical/unrelated commit ranges.

## Acceptance-criteria checklist

| Criterion | Review result | Code/test evidence and remaining limit |
| --- | --- | --- |
| AC1 recommended group/setup with role aliases | PASS | Guide setup documents `lekalo-dev`, all six roles, dedicated DB, empty config before init, reviewed shares and force-disabled flags. Pinned-binary config-first registration passed. |
| AC2 protocol change produces explainable affected-project event | PARTIAL | Historical `4a084aab` produces a real event and the normal gate passes, but F2/F3 invalidate reliable delivery and recipient reporting; received provenance is weaker than claimed (F9). |
| AC3 shared schemas reachable from consumer repository | PASS | Independently rerun binary phase E: group-scoped MCP from consumer reads the approved schema bytes exactly; strict single-project denial passes. Wrong-group proof remains absent (F7). |
| AC4 Lekalo functional without AI Workspace | PASS for the implemented separation | Zero Rust diff and no workspace references under `crates/`; standalone hook reports disabled/unavailable. Ordinary compilation, adapters, and history have no new invocation/dependency. Full reported Cargo suite was not rerun in this review. |
| AC5 scopes and sensitive-path policy never silently widened | PARTIAL | Child MCP flags are literal `0`, hostile `=1` send refuses, and single-project scope denies the peer read. The claimed confinement tests need repair (F7); event publication can reach outside the reviewed group (F4). No hidden/sensitive override was added. |
| AC6 CodeGraph context benchmark run on core changes | PARTIAL | Real pinned core search/index benchmark reproduced with three-file digest and staleness/revocation outcomes; the declared context-quality/budget comparison and five repeated retrieval samples are missing (F8). |
| AC7 no canonical-model duplication in workspace notes | PASS for committed behavior; broader proof unverified | Integration code creates no notes, fixture shares are public contracts/docs, and events contain derived metadata rather than Model/IR bodies. Claimed deletion/inventory negative tests are missing; arbitrary future operator content is not attested (F1/F7). |
| AC8 public events/evidence reveal no private identity | FAIL | Actual manifest marker, outbox-error path, benchmark output path, and unreviewed cross-group event identity disclosures reproduced (F1/F4/F6). Benign fixture and poisoned-child-stderr passes do not establish this invariant. |

Opt-in is real in the normal path: no manifest/no send yields `disabled`;
planning alone writes nothing; send without `--allow-unadmitted-send` refuses
at the documented authority gap, and hostile inherited widening flags refuse.
Both event-create and MCP children force the widening switches off. A send
without an explicit manifest nevertheless falls back to the synthetic fixture
(`scripts/ai-workspace-hook.mjs:153`); a deployment should require its own
reviewed routing configuration. No production-authority admission was supplied
or inferred in this review.

The upstream gaps are mostly stated honestly: role aliases are a convention,
creation is not transactional/idempotent, receipts are untyped, impact is direct
links only, and CodeGraph lacks revision/hash provenance. The findings concern
claims made for the Lekalo mitigation and its tests, not requests to invent
missing upstream capabilities. Optional impact augmentation also remains a
design rule: the hook has no semantic-impact input/decoder, despite the present
tense embedding claim at `docs/integrations/ai-workspace.md:345`.

## Verification and evidence limits

Read the research and implementation reports first, fetched the live issue
with `gh issue view 37 --repo ichinya/lekalo`, then inspected the three-dot
diff/stat, key hunks, complete hook/gates/schema/benchmark, CI, fixture routing,
and versioning/diagnostic/privacy conventions. The existing Devin report was
read after the principal independent failure probes were formed; it was not
used as acceptance authority.

Executed successfully:

- `node scripts/check-contract-versions.mjs --base origin/ichinya/M7`
  (product `0.6.3`, 96 contract families), `test-contract-versions.mjs`.
- `check-authority.mjs`, `check-privacy.mjs`, `check-structure.mjs`,
  `check-model.mjs`, and `test-fixture-provenance.mjs` (64 synthetic families).
- `test-ai-workspace-contracts.mjs` with independently located Ajv **8.17.1**;
  `test-ai-workspace-hook.mjs` both without a binary (explicit tier skip)
  and with the existing upstream debug binary (all binary phases executed).
- Verified that the upstream source checkout HEAD is
  `8fdf818fee757d24e723d657fc5d38614995e557` and binary version is `1.5.0`.
  The binary was reused, not rebuilt or cryptographically build-attested.
- Real `benchmark-ai-workspace-context.mjs --reps 5`, plus a one-repetition
  `--out` probe solely to verify the stdout disclosure. The main run yielded
  the committed scope digest `79f5c64a...`, coverage booleans `[true,true,true]`
  for both searches, and true stale/fresh/revocation booleans. Local median
  cold reindex / warm sync / baseline retrieval / CodeGraph retrieval were
  339 / 155 / 4 / 96 ms; those last two are across tasks, as F8 explains.
- Repeated normal plan output was byte-identical, event keys matched, and
  stdout used LF. Git's tracked new schema/script/benchmark artifacts are
  LF-only. Every schema object closes additional properties. Event-key
  canonicalization independently agrees with the fixture derivation.
- `git diff origin/ichinya/M7...HEAD --check` passed before this report.

All adversarial data used synthetic markers and isolated temporary databases,
fake child executables, or the pinned binary; temporary probe state was removed.
No real consumer was enrolled, no upstream source/database internals were
modified, and no remote service event or GitHub comment was posted. The
three-sentence worker outcome is delivered separately through Orca.

Diagnostic convention check: this change adds no `LEK-*` rule or Rust CLI
command, so no registry allocation is introduced. The standalone Node hook
uses its own `ok`/`state` JSON rather than claiming the Lekalo `--json`
DomainResult contract. Normal state/exit cases are documented, but F3/F6 show
why exit 0 or a parseable result cannot establish terminal success here. New
result/routing/outbox/benchmark objects have no corresponding closed validation
schema beyond the event-envelope schema; a version string alone does not
provide wire validation.

No unrelated artifacts or source changes were found in the initial clean
checkout, and existing gates were preserved. Required follow-up is to fix
F1–F8 and rerun the relevant negative and pinned-binary proofs, then reconcile
F9–F11 and the acceptance/documentation claims. This is a completed review,
not implementation acceptance or production approval.

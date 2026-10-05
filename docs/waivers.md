# Scoped, justified and expiring waivers

Status: **Implemented** at product `0.6.5`. Owner: governance maintainers.
The reviewed successor of [AI-lint waivers](ai-lint.md) is
`contracts/ai-lint-waivers.schema.v0.6.5.json`. The frozen `0.6.4` family
remains supported by `ai-lint --waivers`; its historical semantics are unchanged.
New governance commands accept the successor exclusively. The successor uses
one shared matcher and suppression store, plus derived `waiver-input` and
`waiver-audit` wires. The predecessor retains its original compatibility path;
it is not automatically promoted to an approved successor decision.

## Store and approval

Commit the project-root `lekalo.waivers.json` to version control. It is explicit
user-owned governance configuration outside the closed `lekalo/` Model tree;
it is neither cache nor an authority/privacy grant. This placement avoids changing
the accepted Model/authority contracts to admit a new canonical-root entry.
No scanner discovers it implicitly. `ai-lint --waivers PATH` explicitly selects
one predecessor or successor store; stores are never unioned.

Each successor entry requires a stable ID, registered rule or capability
selector, exact typed scope, occurrence ID and immutable fact digest, exact subject and target, current profile digest,
condition digest, distinct owner and approver, approval reference bound to the
decision fields, reason, creation time, a finite expiry or review deadline,
source issue/decision, accepted risk class, five explicit fingerprint states,
lifecycle, and supersession reference. Reasons cannot be blank. Decision hashes
exclude lifecycle so revocation/supersession does not rewrite the approval.
The local approval digest detects changed decision fields; it does not authenticate
an external reviewer. Repository review supplies organizational approval.

Project/module/symbol scope uses [semantic IDs](semantic-ids.md); path scope uses
the portable project-relative filesystem grammar. Target/profile identifiers
are exact case-sensitive tokens. No wildcards, prefixes, regular expressions,
case folding, short symbol aliases or path normalization are accepted. Even a
named project/module/profile scope binds one exact subject, target and condition.
Occurrence ID and fact digest additionally prevent replacing a supplied symbol
while retaining the same subject, condition or reused occurrence ID.
Ambiguous occurrences and overlapping effective approvals refuse.

## Commands

Global `--json` emits machine-readable output. Use `--no-cache` for independent
evidence generation. Denied results retain their typed evidence under the standard
`payload` member. These examples describe a CI profile from an explicit lint
config; substitute the exact admitted facts and identifiers of your project.

```text
lekalo --json --no-cache ai-lint --all --config lint-config.json --lint-profile ci --waiver-facts
lekalo --json waivers list
lekalo --json waivers audit --facts facts.json --profile-kind ai-lint --lint-config lint-config.json --profile ci --as-of 2026-10-04T12:00:00Z --check
lekalo --json waivers add RULE_ID --facts facts.json --profile-kind ai-lint --lint-config lint-config.json --profile ci --as-of 2026-10-04T12:00:00Z --id WAIVER_ID --symbol MODULE.SYMBOL --target TARGET --subject SUBJECT --owner OWNER --approver APPROVER --approval-ref DECISION_ID --reason REASON --source-issue ISSUE_ID --expires 2026-11-01T12:00:00Z
```

Save the `waiverInput` member of the first envelope as `facts.json`. `list` is an
inventory and explicitly reports effectiveness as `unexamined`; it does not
guess a clock, profile, adapter or current facts. `audit` requires supplied facts,
a resolved current profile and an explicit evaluation time. The CLI validates
the facts' Model/IR digests and known symbols/modules against the selected project.
The neutral input is a supplied producer report, not a re-executed external tool;
its adapter/revision/capability evidence remains producer-owned.
Standalone audit evaluates those supplied facts; it does not replace validation,
privacy, required-coverage or other producer gates. CI owns the evaluation time.

`add` previews a fully populated candidate and `planId`. Repeat the same command
with `--apply sha256:PLAN_ID` to write that exact candidate to `lekalo.waivers.json`.
One of `--symbol`, `--module`, `--path`, `--project-scope`, `--target-scope`, or
`--profile-scope` is required. `--capability` selects a capability ID. Exactly one
of `--source-issue`/`--source-decision` and at least one deadline are required.
`--review-after` supplies a review deadline; `--supersedes ID` retains and marks
the old entry as superseded. A reused ID refuses. Candidate eligibility is
evaluated before writing. Owner/approver values are review metadata, not inferred
identity. Apply checks the original bytes, locks concurrent updates, stages and
syncs the successor, replaces atomically, and verifies the result. It never runs
Git. A small root `waivers-update.guard.json` remains as a local synchronization
file; it is not waiver evidence. Other homes, links/reparse points, hard links,
special files and changed plans refuse.

## Evaluation and profile seam

Full times are whole-second UTC (`YYYY-MM-DDTHH:MM:SSZ`). Date-only CLI sugar
means the end of that UTC day. Equality with the earliest known expiry/review
deadline is effective; the following second restores the original gate. The
default expiring window is seven days, bounded to one year. Audit sorts stable
entry/fact IDs and reports active, expiring, expired, stale, non-waivable,
unverifiable, orphan, unexamined, revoked and superseded inventory states.

Acceptance requires equality of Model, IR, adapter, revision and capability
fingerprints plus profile/condition. Known differences are stale. Unknown or
withheld pins do not grant acceptance. Two explicit `unsupported` states agree
only when the admitted policy marks that dimension inapplicable to the producer.
Model-only lint requires known Model, IR and revision pins, uses unsupported
adapter and capabilities, and pins revision to canonical Model/IR. Its reserved
`model` claim must match the core Model producer's target-bound finding identity,
canonical revision and path/pin states. Lint selectors must also belong to that
producer recipe; derived capability facts retain their Model source identity.
A relabelled native fact is `unverifiable` (`fingerprint-unverifiable`) in audit
and refuses add preview/apply, including when all its native pins are known.
These checks apply to pre-existing stored decisions as well as new candidates.
Native facts require all five dimensions to be known, including source revision
and capability digest; a producer's unsupported state cannot waive that requirement. Native
evidence admission forbids the reserved `model` target. Native lint pins the
admitted producer identity/artifact, source manifest and revision. Git HEAD and waiver-store
bytes are excluded from source revision to avoid self-invalidating commits.

The public `waivers::policy::ProfileState` seam consumes current profile state,
including producer-domain admission through `producer_domain_admitted` and
fingerprint applicability through `fingerprint_requirements`.
Validation profiles use `validator/profile.rs` selections and effective severity:
errors cannot be waived. Lint profiles use configured severity/gate and required
coverage. Target profiles use resolved component capability requirements; required
capabilities cannot be waived. Missing policy is unverifiable and fails closed.
Source invalid/security/data-loss outcomes also remain denied. Issue #84 can
implement the trait over its admitted profile without an invented future wire.
No security-code allow/deny list lives in the matcher. Privacy export enforcement
in [privacy](privacy.md) remains independent; a waiver never authorizes disclosure.

## Facts, audit, review and done

`ai-lint` retains every native finding field, severity, evidence and confidence.
Only disposition and its waiver link change; `summary.raw = active + waived` and
metric counts remain visible. The unchanged `0.6.4` report can carry its existing
waived disposition; `waiverInput` and `waiverAudit` are separate envelope members
with their own schemas. Required coverage, comparable baselines and raw regressions
remain enforced. A capability fact keeps its `unsupported`/`unknown` outcome even
when an eligible gate becomes `accepted-risk`. Reports never claim support or
verification from an exception.

`audit --check` exits 3 only when an effective gate remains denied; ordinary audit
is inspectable with exit 0. Malformed/conflicting input exits 1. These paths reuse
registered waiver/policy diagnostics; the frozen registry remains 500 entries.
`--base base-waivers.json` produces typed added/removed/changed rows with entry
digests. The audit's `doneDigest` binds the entire report: exact store, input,
profile, time, lock reference, comparison base, provenance and dispositions.
`--done sha256:DIGEST` refuses stale completion evidence. `--locked` additionally
requires the source input's exact current valid `lekalo.lock` digest. The frozen
lock `0.3.2` does not gain a waiver pin; the audit companion supplies that binding.
Store edits are visible in Git review and in audit changes/done evidence, while
semantic `lekalo diff` continues to describe Model semantics only.

Archive typed audit bytes as CI artifacts or through a supplied-document HLV/AIFHub
consumer. Fact/entry/store/profile/input/lock/done digests, source references,
approval references and replacement links preserve provenance without claiming
external approval or execution. Run-history contracts remain unchanged; no audit
is silently written to cache or `.lekalo/history`. Store retention is version
control; audit retention is an explicit consumer/CI responsibility.

The implementation and acceptance evidence are mapped in
[issue-88-implementation](m7/issue-88-implementation.md). Each new family has
synthetic goldens and a live Ajv **8.17.1** gate after the workspace build in CI.

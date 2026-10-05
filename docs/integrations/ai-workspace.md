# AI Workspace integration (issue #37)

Status: implemented lekalo-side integration design and gates for the public
[`lee-to/ai-workspace`](https://github.com/lee-to/ai-workspace) project used
as an **external, optional, cross-project context and service-graph layer**
around Lekalo. Research authority:
[`docs/m7/issue-37-research.md`](../m7/issue-37-research.md) (surface
inventory, boundary plan, and test plan; every command below was
re-verified against the pinned upstream revision). Implementation report:
[`docs/m7/issue-37-implementation.md`](../m7/issue-37-implementation.md).

Pinned upstream revision: `8fdf818fee757d24e723d657fc5d38614995e557`,
package `ai-workspace` `1.5.0` (Rust 1.88+). Every upstream CLI/MCP fact in
this document was checked against that revision's source and, where noted,
executed through the integration gates. Nothing from upstream is vendored,
forked, or imported; upstream is a separately installed local executable.

## What this integration is — and is never allowed to become

AI Workspace gives participating repositories (core, consumers, extension,
workflow, PHP consumer) a local shared-context registry: approved file
shares, a service link graph, a human-reviewed event inbox, and a
group-scoped MCP server that agents query before broad scans. Lekalo uses
exactly that surface, and nothing more.

Hard boundaries (each is enforced and tested; research B1–B6):

1. **Not canonical storage.** AI Workspace never stores the Lekalo Model,
   IR, or any authoritative semantic artifact. Workspace notes may hold
   short pointers to committed schemas/ADRs; the committed `contracts/`
   files and Git history remain the only schema authority. The workspace
   SQLite database is private, disposable local state — deleting it must
   lose no Lekalo truth (gate-proven).
2. **Committed schemas over the database.** A shared workspace item is a
   live reference, not an immutable snapshot. Consumers must verify contract
   identity, version, and the `sha256` digest bound to the public source
   revision before trusting shared bytes (see [Agent flow](#agent-flow)).
3. **Opt-in sharing only.** Registration is config-first: a reviewed,
   locally ignored `.ai-workspace.local.json` with `"share": []` exists
   before the first `init`, which suppresses upstream's auto-sharing of
   READMEs and manifests (verified upstream: auto-share runs only when no
   config file exists). Shares are added one reviewed file at a time. The
   `ai-factory` preset, `cloud push`, and directory-wide shares are not
   part of this integration.
4. **No project-wide reads or writes.** The MCP server is launched
   group-scoped with both widening switches force-disabled:
   `AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS=0` and
   `AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE=0` (explicit `0` overrides any
   inherited hostile value; upstream treats the flags as enabled only for
   the exact string `1`). No `codegraph --full-project`, no
   `include_hidden`/`include_sensitive` overrides.
5. **No private identity in public artifacts.** Public event envelopes,
   benchmark aggregates, and hook results speak only neutral role aliases
   (`lekalo-core`, `greenfield-consumer`, …). Absolute paths, private
   repository names, real project IDs, delivery timestamps, foreign trace
   IDs, source snippets, and secrets never enter a public channel. Hashing
   a private path or name does not make it publishable.
6. **CodeGraph is navigation, never semantics.** Upstream CodeGraph is a
   conservative regex-based Rust indexer. Its output may guide *where to
   look*; it is never evidence of semantic impact, never a substitute for
   `lekalo impact`/`inspect`, and public CodeGraph evidence must carry the
   provenance wrapper defined below (parser string, source revision,
   per-file digests, staleness status).

Lekalo is fully functional without AI Workspace. Every existing command —
`inspect`, `impact`, `context`, `trace`, `privacy`, `history`, adapters —
runs with no workspace binary, no workspace database, and no network. No
Lekalo Rust code links to, shells out to, or reads AI Workspace state. The
integration lives entirely in reviewed operator setup plus three
dependency-free Node scripts (`scripts/ai-workspace-hook.mjs`,
`scripts/test-ai-workspace-hook.mjs`,
`scripts/benchmark-ai-workspace-context.mjs`) that are invoked manually or
by an explicitly installed post-change/CI hook — never from Model
compilation, protocol execution, `history`, or default `scan`/`verify`.

## Recommended local group and setup (AC1)

One dedicated workspace group (`lekalo-dev`) and one dedicated database per
machine, selected consistently for **every** CLI and MCP process:

```sh
export AI_WORKSPACE_DB="$HOME/.ai-workspace/lekalo-dev.db"   # operator-local, never committed
```

A dedicated database confines the global portion of upstream `sync`, keeps
this integration out of any other workspace the operator may have, and
makes the whole setup disposable. The host checkout paths below bind role
aliases to real directories **on that machine only**; they are operator
settings and are never committed, reported, or shared.

### Role aliases

Role aliases are the only consumer identity that may appear in public
artifacts. The same role may bind to different checkouts on different
machines; multiple instances of one role need additional local aliases. If
an external adapter checkout participates, give it an optional
`adapter-node-typescript` / `adapter-php-laravel` role. In-tree adapters
stay scopes of `lekalo-core` — never register the same checkout root twice
to manufacture extra projects. Role aliases are not Lekalo semantic rename
aliases and carry no semantics beyond routing.

| Role alias (= registered name and slug) | Local binding (operator-supplied) | Approved sharing from this role |
| --- | --- | --- |
| `lekalo-core` | Lekalo repository root (core + in-tree adapters) | Individually approved committed schemas, target-protocol docs, ADRs |
| `greenfield-consumer` | Greenfield consumer checkout | Nothing initially; optional sanitized `docs/lekalo-integration.md` |
| `brownfield-consumer` | Brownfield consumer checkout | Nothing initially; optional sanitized `docs/lekalo-integration.md` |
| `aifhub-extension` | Extension checkout | Explicit schema/trace integration docs only |
| `aifhub` | Workflow/application checkout | Explicit workflow/protocol docs only |
| `laratesto` | PHP consumer checkout | Explicit approved integration docs only |

There is no upstream role-alias command (documented gap); the mapping above
is the reviewed convention plus each operator's private local binding.

### Config-first registration (mandatory)

Before running `init` in any participating checkout, create a
locally ignored `.ai-workspace.local.json` with an **empty share list**.
The file's presence is what disables upstream auto-sharing (upstream runs
auto-share only `if config.is_none()` — verified at the pinned revision).

Example for the core checkout; other projects substitute their role
name/slug:

```json
{
  "ai_workspace_config_version": 1,
  "name": "lekalo-core",
  "slug": "lekalo-core",
  "groups": ["lekalo-dev"],
  "share": [],
  "notes": []
}
```

Operator preflight, every machine, before first registration:

1. The config file exists, is untracked/ignored, and its share list is
   empty.
2. `AI_WORKSPACE_DB` points at the dedicated database (fresh or previously
   verified); no existing registration under that database uses a different
   alias for the same checkout.
3. No `ai-factory` preset will be applied; no cloud credentials are
   present.

Never silently rename, reset, or migrate an existing workspace database.
Re-running with a changed config can remove old memberships and shares —
inspect before applying (`ai-workspace status`, `ai-workspace list`).

### Explicit shares (core)

After `ai-workspace --config .ai-workspace.local.json init` in the core
checkout, share individually reviewed public files (run only after reading
each; the list is an allowlist, not a default):

```sh
ai-workspace --config .ai-workspace.local.json share contracts/target-protocol.schema.v0.3.2.json --label "Lekalo target protocol 0.3.2"
ai-workspace --config .ai-workspace.local.json share docs/target-protocol.md --label "Lekalo target protocol"
ai-workspace --config .ai-workspace.local.json share docs/adr/0025-target-protocol.md --label "Lekalo protocol ADR"
ai-workspace --config .ai-workspace.local.json share contracts/trace-manifest.schema.v0.2.16.json --label "Lekalo trace manifest 0.2.16"
```

Version notes: derive the contract version from the actual committed file
name (protocol `0.3.2`, several projection schemas `0.2.16`); the product
version (`0.6.3` at this baseline) is not a contract version. Because
shares are live references, a label alone never proves which bytes a
consumer read — bind reads to the digest of the public source revision
(see [Agent flow](#agent-flow)).

Consumers share nothing by default. Optionally, a consumer may share one
new, reviewed, identity-free `docs/lekalo-integration.md` (links and
version requirements only — never Model/IR content, never source trees)
and declare it dependent:

```sh
ai-workspace artifact depends docs/lekalo-integration.md lekalo-core --kind consumes_api --reaction inspect
```

Never share consumer `docs/` trees, repository roots, `.lekalo/`, or raw
pilot/harness output.

### Service links and subscriptions

Links are directional consumer → provider, use slugs, and do not by
themselves grant file access:

```sh
ai-workspace link add greenfield-consumer lekalo-core --kind depends_on --label "target-protocol"
ai-workspace link add brownfield-consumer lekalo-core --kind depends_on --label "target-protocol"
ai-workspace link add aifhub-extension lekalo-core --kind depends_on --label "trace-schema"
```

Add direct links for `aifhub`, `laratesto`, and external adapters only when
their real consumption warrants it. Group membership is context
visibility, not proof of dependence; if a transitive consumer must hear
about core changes today, register its own reviewed direct link (upstream
event impact is direct-links only — a documented gap). Avoid
`related_to` links for presentation: upstream counts them as event impact.

### Server launch (the only supported shape)

```sh
AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS=0 \
AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE=0 \
AI_WORKSPACE_DB="$HOME/.ai-workspace/lekalo-dev.db" \
  ai-workspace serve --group lekalo-dev
```

Group scope keeps peer-project metadata visible while `workspace_read`
still serves only approved shared items. `--scope current-project` and
`--project ROLE` are stricter single-project scopes and are supported
alternatives for consumers that want no peer metadata. The two `=0`
overrides are mandatory and must stay literal in every launcher (they
defeat inherited hostile values; upstream enables the features only for
the exact string `1`). No cloud credentials, ever.

## Agent flow

Consuming agents work in this order; extension clients should schedule and
surface it the same way:

1. `workspace_context` — see which projects, groups, shares, links, and
   events are visible in scope. This comes **before** any broad local
   scan.
2. Locate core's approved schema item and `workspace_read` it.
3. Verify contract identity and version against the consuming adapter's
   expectation, and verify the bytes against the digest published for the
   public source revision (a `workspace_read` returns live file bytes; the
   item ID is installation-local and must never be embedded in committed
   instructions).
4. Inspect `workspace_service_graph` and `workspace_events` for declared
   impact and open review events.
5. Run a bounded local `lekalo context` / `lekalo inspect` /
   `lekalo --json impact` for semantic truth; for opted-in public Rust
   navigation, CodeGraph tools with the provenance checks below.
6. Only for an explicitly unresolved question, broaden the scan — and
   record why.

A notification (event appearing in the inbox) never starts an agent turn
by itself; upstream subscriptions coalesce and are not delivery
acknowledgments. Fallback: without AI Workspace, agents use the committed
`contracts/` files and Lekalo's own commands — every workflow in this
document degrades to that, with no network or database probe.

## Change events: the hook pipeline (AC2)

Target-protocol and schema changes must reach affected projects as
**explainable, review-only** events. Upstream has no protocol trigger, no
typed provenance, and no idempotent creation; Lekalo supplies the missing
seam with an opt-in adapter script:

- `scripts/ai-workspace-hook.mjs` — detects approved public contract
  changes between two commits, builds a bounded deterministic event
  envelope, and (only when explicitly enabled) sends the upstream
  `event create` command as an argv array and verifies delivery by
  readback.
- `contracts/ai-workspace-event.schema.v0.6.3.json` — the closed,
  versioned wire schema of the envelope (`lekalo/ai-workspace-event/v0.6.3`,
  identity `dev.lekalo.ai-workspace-event@0.6.3`). Per
  [`docs/versioning.md`](../versioning.md) the contract takes the product
  version of this change; validated by
  `node scripts/test-ai-workspace-contracts.mjs`.
- A reviewed routing manifest (operator input; committed example:
  `tests/fixtures/ai-workspace/routing-manifest.json`) declaring the
  approved public artifacts, the role routes, and each role's
  installation-local workspace slug.

The hook is invoked manually or by an explicitly installed post-change/CI
hook with explicit `--base`/`--head`. It is **not** called from Model
compilation, target protocol execution, `history`, or default
`scan`/`verify`, and it never posts to GitHub/Slack or any external
service.

What the hook guarantees:

1. **Explainable trigger.** Only approved allowlisted paths are compared
   (bytes and existence: added/modified/deleted are distinct). A
   protocol-doc-only edit still produces a review event with compatibility
   `unknown`; missing Git base is `unknown`, not "no changes". A dirty
   core worktree refuses emission.
2. **Typed explanations.** Each affected role carries an ordered,
   origin-tagged chain — `public artifact changed → core service →
   declared subscription → affected role` — with origins
   `declared-service-link` and `declared-artifact-dependency`. The
   `lekalo-impact` origin is reserved for a future reviewed impact
   input (not implemented — see the provenance rules). "No declared
   subscribers" is stated as such; it never proves "no impact".
3. **Validated inputs and outputs.** The routing manifest is validated
   closed before use (path/role/version grammars, route enums, bounded
   counts), the emitted envelope is validated against its contract
   shape before it is printed or sent, and the public digest/event key
   binds only the neutral public routing projection — private local
   bindings are outside the hash domain.
4. **Honest delivery.** Upstream `event create` is neither transactional
   (separate inserts for groups/targets/artifacts, verified at the pin)
   nor idempotent, and has no machine-readable receipt. Therefore: the
   event key (a `sha256` over the canonical public identity: producer,
   base/head, artifact changes, public routing projection) is recorded
   in a single-writer local outbox **before** sending; any spawn
   failure, timeout, or nonzero exit is recorded `unknown-delivery`
   and a later attempt reconciles by key through the read surface
   before re-sending; ordinary repeats after verified delivery are
   no-ops. Before any create, a recipient preflight requires the
   workspace's actual linked dependents to be exactly the reviewed
   consumer set — an unreviewed linked project refuses the send, and a
   readback verifies the exact target set (no unreviewed recipients,
   every reviewed linked consumer present) plus the projected
   kind/title/body/key. This is at-most-moderate confidence, not
   exactly-once.
5. **No-op ranges are not events.** A revision range with no approved-
   path change closes as `no-change` and never reaches upstream.
6. **Closed results.** The hook prints one closed JSON result (states
   `planned|sending|delivered|unknown-delivery|refused|disabled|
   unavailable|no-change`) and never echoes upstream stdout/stderr,
   which can contain real paths and names; every failure path — usage,
   filesystem, unexpected exception — emits a bounded safe code instead
   of a stack or reflected value. Capture upstream output privately,
   if at all.
7. **No silent scope growth.** The hook refuses to send when the widening
   environment flags are enabled, when the upstream binary is missing
   (`unavailable`), or when the routing manifest has not declared the send
   policy admitted. Production emission additionally requires the privacy
   authority admission of the workspace change-event artifact kind — an
   open, documented prerequisite (see [Upstream and authority gaps](#upstream-and-authority-gaps)).

Upstream receives a bounded serialized projection of the envelope as
`--title`/`--body` (never a raw diff, private filename, or free user
prose; transport-safe length bound enforced) — carrying the event key,
contract identities with old/new digests, affected roles, and the
accepted authority reference, so a consumer can verify what it received
against the envelope schema. Upstream snapshots all direct source
dependents into the event's target rows; the hook verifies the exact
target set (no unreviewed recipients, every reviewed linked consumer
present) through the group-scoped MCP tools before recording
`delivered`. Upstream's affected-set is the conservative declared
candidate set — it is presentation, not a proof of total impact.

## Provenance rules

- **Public envelope.** Contains only: envelope identity and event key;
  `eventKind` (`protocol-change|schema-change`); public source revision
  hashes and clean-state proof; producer versions; approved manifest
  digest; per-artifact allowlisted repository-relative paths with
  old/new contract identity, version, and `sha256` digest (explicit
  `deleted`/`absent`/`unknown` states); affected role aliases with typed
  explanation chains; the accepted policy reference used for the send
  decision. Explicit bounds: 64 artifacts, 64 roles, 128 explanation
  chains, 64 KiB body — over-limit refuses, never truncates away affected
  roles.
- **Private local state.** Upstream numeric project/event/item IDs, real
  local paths, delivery timestamps, raw upstream output, and outbox
  receipts stay on the machine. They are installation-specific routing
  data, not portable identity, and never re-enter a public artifact.
- **CodeGraph evidence (optional family).** If CodeGraph-derived
  navigation evidence is ever published it must carry: the pinned upstream
  commit and package version, the parser provenance string
  (`rust-regex-mvp` / `rust-regex-mvp-resolver` at the pin), the public
  source revision, per-file content digests checked around sync/read, the
  approved scope digest, bounded query settings, and explicit
  stale/unresolved status. `codegraph_context` returns live source
  snippets with no revision/hash guarantees — snippets are therefore
  excluded from public evidence, and a sync timestamp alone never proves
  freshness. A missing schema file, failed sync, or digest mismatch leaves
  an explicit pending/stale result; regex-derived edges never satisfy
  semantic or trace confirmation gates.
- **Semantic evidence (optional family).** `lekalo --json impact`
  output may augment explanations only after explicit review. **Not yet
  implemented:** the hook has no impact input or decoder today; when
  one is added it must embed only reason ids, confidence, and
  completeness from a closed-subset extraction — never raw impact JSON,
  which can name private targets. An unresolved protocol file is never
  reinterpreted as zero semantic impact.
- **Workspace notes.** At most a short pointer to a committed schema/ADR
  plus provenance. Never authoritative entity definitions, compiled IR,
  copied semantic graphs, or a second editable model (B1; gate-checked
  against the share inventory).

## Privacy boundary enforcement

Every boundary has a concrete enforcement; the proofs named below are
exactly the committed gates (no overstated test names):

| Boundary | Enforcement | Proven by |
| --- | --- | --- |
| B1 not canonical storage | Share allowlist excludes model/IR paths; notes are pointers only; zero Rust integration | gate share inventory (fixture manifests declare `share: []` before init; only the approved schema is shared); the external database is disposable by construction — no Lekalo source depends on it (zero `crates/` diff) |
| B2 committed schemas win | Digest binding to public revision; DB bytes never a schema source | hook gate exact-byte read: the group-scoped read must equal the committed bytes by sha256 (a divergent share fails the gate); the DB itself is never read as a schema source |
| B3 opt-in sharing | Config-before-init; sentinel files stay unshared | hook gate sentinel test (config-first registration; README/package/private sentinels invisible) |
| B4 no project-wide access | Forced-off env flags; wrong-group and single-project denials; project-wide tools confined to shares | hook gate: wrong-group read denied, single-project read denied, tree/grep confined with positive controls, write tool refuses, hostile inherited flags refuse the send |
| B5 no private identity out | Closed envelope schema; closed result; leak probes over every public channel, including failure paths | hook gate probes + `test-ai-workspace-contracts.mjs` |
| B6 CodeGraph provenance | Provenance wrapper; stale/refused evidence states | benchmark protocol (asserted staleness + revocation gates) + doc rules |

The leak-probe discipline follows the #118 pattern
(`scripts/test-pilot-brownfield-ts.mjs`): closed member allowlists, the
same probe set run over every emitted artifact, synthetic
private-marker fixtures, and explicit failure-path coverage — extended
here with hostile-environment and upstream-stderr channels. A probe pass
is a property of these gates, not a blanket privacy attestation for
arbitrary future content.

## Upstream and authority gaps (honest list)

The following capabilities do **not** exist at the pinned upstream
revision or in the currently accepted Lekalo authority registry. They are
documented, not faked; none is worked around by writing upstream's
database directly or by widening scopes:

1. **No role-alias management.** No `group create`/`alias` surface; role
   aliases are a reviewed convention plus private local bindings.
2. **No protocol triggers or watchers.** Nothing upstream detects a
   contract change; the Lekalo hook is the trigger.
3. **Non-transactional, non-idempotent event creation.**
   `create_workspace_event` inserts the event, groups, targets, and
   artifact impacts through separate statements with no enclosing
   transaction, no idempotency key, and no machine-readable receipt; the
   CLI prints a numeric id only. The hook's outbox + keyed reconciliation
   + readback mitigates but cannot fully repair this (concurrent writers
   and partial events still require human repair).
4. **Direct-links-only impact.** Event targeting snapshots direct
   `service_links` and `artifact_dependencies` of the source; both link
   kinds count; there is no transitive traversal and no schema-version
   constraint filter.
5. **No `--json` on the CLI and no typed provenance payloads.** Public
   structure is carried by the Lekalo envelope; upstream stores it as
   opaque body text.
6. **No artifact/version-targeted events.** `artifact_changed` still
   selects a source *service*; there is no impact-JSON importer.
7. **CodeGraph provenance is incomplete upstream.** No full Git revision,
   file hash, parser version, or confidence envelope attached to context
   results; the wrapper above restores what it can, navigation-only.
8. **Authority admission is open.** The accepted authority matrix has no
   `workspace.change-event` artifact kind; until the successor procedure
   (#119/#120 owners) admits one, the hook's production send stays
   refused by default (`refused/policy-not-admitted`) and the gates
   demonstrate the pipeline against explicitly admitted local fixtures.
9. **No transitive-subscription ergonomics.** Consumers that must hear
   about changes register explicit direct links today.

## Acceptance mapping (issue #37)

| Criterion | Status | Where proven |
| --- | --- | --- |
| AC1 recommended group/setup with role aliases | **Lekalo-side verified** — documented here; executed against the pinned binary by the integration gate (config-first registration, group scope) | this doc + `scripts/test-ai-workspace-hook.mjs` |
| AC2 protocol change → explainable affected-project event | **Lekalo-side verified** — hook builds the envelope with typed role reasons; gate delivers a real upstream event, verifies the exact readback target set (no unreviewed recipients), and the event body carries the contract identities with old/new digests for consumer verification | hook gate phase F |
| AC3 consumer agent reads shared schemas | **Lekalo-side verified** — group-scoped MCP from a consumer root reads exact approved schema bytes; wrong-group and single-project denials proven | hook gate |
| AC4 fully functional without AI Workspace | **Lekalo-side verified** — no Rust dependency; hook reports `disabled`/`unavailable` without mutating Lekalo behavior | hook gate optionality phase |
| AC5 MCP scope/sensitive policy never silently widened | **Lekalo-side verified** — forced-off flags, direct-call denial, sentinel unreadable, project-wide/write tools absent from tool list | hook gate |
| AC6 CodeGraph context benchmark on core changes | **Run and reported** — pinned protocol over a real core change; results and limitations published; reproducible via `scripts/benchmark-ai-workspace-context.mjs` | `docs/m7/issue-37-benchmark.md` |
| AC7 no canonical model duplicated in notes | **Documented boundary + gate inventory** — share/notes inventory checked; no model/IR content admitted | hook gate + authority rules |
| AC8 public events/evidence reveal no private identity | **Lekalo-side verified** — closed schema, leak probes over all channels incl. failure paths and hostile env | contracts gate + hook gate |

Verifiability split: AC1–AC5 and AC8 are enforced by gates committed in
this repository (runnable with the pinned upstream binary for the
end-to-end phases; the envelope/privacy phases are dependency-free). AC6
is evidenced by a completed, documented benchmark run. AC7 combines the
gate inventory with the documented authority boundary; its
runtime-attribution aspect (proving a negative about all future notes)
remains a documented boundary owned by the authority/privacy review.

## Reference

- Research: [`docs/m7/issue-37-research.md`](../m7/issue-37-research.md)
- Implementation report: [`docs/m7/issue-37-implementation.md`](../m7/issue-37-implementation.md)
- Benchmark: [`docs/m7/issue-37-benchmark.md`](../m7/issue-37-benchmark.md)
- Envelope contract: `contracts/ai-workspace-event.schema.v0.6.3.json`
- Hook: `scripts/ai-workspace-hook.mjs`; gates:
  `scripts/test-ai-workspace-contracts.mjs` (CI),
  `scripts/test-ai-workspace-hook.mjs` (requires pinned upstream binary),
  `scripts/benchmark-ai-workspace-context.mjs` (manual, pinned)
- Privacy runtime the exporter must eventually route through:
  [`docs/privacy-runtime.md`](../privacy-runtime.md)
- Target protocol (the change surface): [`docs/target-protocol.md`](../target-protocol.md)
- Upstream: `lee-to/ai-workspace@8fdf818fee757d24e723d657fc5d38614995e557` (package 1.5.0)

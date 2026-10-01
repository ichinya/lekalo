# Issue #37 — AI Workspace integration research

Research only, 2026-10-01. Authority: [live issue #37](https://github.com/ichinya/lekalo/issues/37), read with `gh issue view 37 --repo ichinya/lekalo`; all seven scenarios, six boundaries, and eight acceptance criteria are mapped below. Lekalo baseline: `9510dd0767a56c0ab34b8d3c8ceb2a14db8de825`, branch `ichinya/m7-issue-37`, product `0.6.3`. External source: public `lee-to/ai-workspace`, shallow-cloned with `gh repo clone` into an isolated directory under the operating-system temporary directory, outside this checkout; inspected commit `8fdf818fee757d24e723d657fc5d38614995e557`, package `1.5.0` (Rust 1.88+). No external source is vendored or committed.

Recommendation: ship a documented, opt-in local setup and a small Lekalo-owned change/export shim using existing AI Workspace commands. Keep workspace storage, MCP authorization, event delivery, and CodeGraph in AI Workspace; keep semantic truth and privacy decisions in Lekalo. The shim must not read recorder databases or make AI Workspace a dependency of normal Lekalo commands. New governed event artifacts require accepted ownership/policy admission before emission.

Evidence level: source, contract, documentation, and existing test inspection. No AI Workspace installation, live group registration, MCP connection, cloud push, protocol-change event, or CodeGraph benchmark was performed. Commands below are source-verified setup examples or explicitly marked future interfaces, not evidence that issue #37 is implemented. Context7 was queried twice for the exact project; it returned the unrelated `/a-tokyo/aiworkspace`, so that documentation was not used. The pinned upstream checkout is the authority for this inventory.

## AI Workspace surface inventory

### Existing commands and missing seams

The actual Clap surface is [src/cli/mod.rs](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/src/cli/mod.rs); compare [docs/cli.md](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/docs/cli.md), [models](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/src/models.rs), and [database operations](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/src/db/crud.rs).

| Surface | Exists at the inspected revision | Integration consequence / missing capability |
| --- | --- | --- |
| Registration and groups | `init --name NAME --slug SLUG --group GROUP`; projects join multiple groups; `list all|projects|groups`, `leave`, `delete-group`, `destroy`, `status` | Project has numeric ID, name, unique normalized slug, and local path. There is no role-alias registry or `group create`/`alias` command. Use explicit neutral names/slugs and a private operator mapping. Reinitialization cannot change an existing slug. |
| Explicit sharing | `share PATH --label LABEL`, `rm TARGET`; file/dir inferred from the path | The issue's `share schemas` is syntactically valid only if that directory exists; Lekalo uses `contracts/`. Prefer individual approved versioned files. There is no `share dir PATH` subcommand despite that spelling in the upstream README. |
| Notes and configuration | `note`, `edit`, `export`; project config selected by global `--config RELATIVE_PATH` or `AI_WORKSPACE_CONFIG`; default `.ai-workspace.json` | Notes are free text, not a schema/model authority. Config includes name, slug, groups, shares, project notes, and artifact dependencies; it does not preserve service links or event history. Group notes remain local. Config paths and share paths are confined; shares are literal paths, not globs. |
| Initialization surprise | Without an existing config, `init` auto-shares `README*`, manifests, and build files; `--preset ai-factory` creates/shares additional context | An empty explicit config must exist **before** first registration. Do not apply the preset by default. Supplying only `--name`/`--slug` does not disable auto-sharing. |
| Synchronization/search | `sync`, `search QUERY --limit N`, `reindex`; Markdown FTS5 index with lazy refresh | `sync` first removes missing shares across the selected database, then reconciles current-project config. It does not detect protocol changes or emit change events. File search indexes `.md`, not JSON schemas; use `workspace_read` for JSON. Shared files are live references, not immutable version snapshots. |
| Service graph | `link add FROM TO --kind depends_on|related_to --label LABEL`, `link list [--project TARGET]`, `link rm ID` | Direction is consumer → provider. Slug, ID, or registered path resolves a target; use slugs in integration commands. Links do not themselves grant file access. |
| Artifact dependencies | `artifact depends ITEM SERVICE --kind references|consumes_api|documents|configures --reaction inspect|update|delete|remove_reference`; `artifact deps`, `artifact undepend` | ITEM must be a shared file/directory in the current owning project. Dependency target is a service, not a versioned schema, semantic symbol, or arbitrary graph edge. Lekalo impact requires an explicit mapping; there is no impact-JSON importer. |
| Events | `event create --kind service_deleted|service_changed|artifact_changed --source SLUG --severity info|warning|error|critical --title TEXT --body TEXT`; `event inbox|list|show|close|rm` | Existing event creation can implement notification. No content watcher, Git/protocol trigger, typed Lekalo provenance payload, idempotency key, or general CLI `--json` exists. `artifact_changed` still selects a source service, not a changed artifact. |
| Affected projects | Event creation snapshots source name/slug, source groups, incoming service links, and artifacts depending on source slug | Incoming links of **both** kinds count; no transitive traversal or schema-version constraint filter. Artifact snapshots include relative path, reaction, and a reason such as dependency-kind → source. Treat these as declared candidate impact. |
| Event delivery | MCP `workspace_events`, `workspace_event_details`; event resources, subscribe/read/unsubscribe | SQLite changes are polled about once per second for subscribed resources. Notifications invalidate resource contents and may coalesce; they are not delivery acknowledgments or automatic agent turns. Consumer/client integration is still needed. |
| CodeGraph | `codegraph reindex|sync|status|search`, optional `--project`; indexing supports explicit `--full-project` | Rust-only conservative regex parser/resolver, not rustc semantics. No watcher, multi-language graph, framework route detector, or Lekalo semantic-ID binding. Default scope is explicitly shared Rust files/directories. |
| Optional cloud/provider surface | `cloud push [--include-markdown]`, `cloud serve`; PostgreSQL snapshots, external OIDC validation, seven read-only hosted MCP tools | No LLM-provider plugin or Lekalo adapter exists. Cloud is a separate deployment choice, unnecessary here. Default cloud snapshots still contain metadata/notes/events; JSON schema bytes and CodeGraph are not available through the hosted content surface. Do not assume a cloud consumer can read local shared JSON. |

Event creation deserves a reliability qualification: `Db::create_workspace_event` inserts the event, groups, service targets, and artifact impacts through separate statements without an enclosing transaction. In contrast, project destruction uses a transaction for its deletion event. A failed ordinary create can therefore leave a partial event; a zero CLI exit is not a substitute for reading back the expected targets. This is an upstream repair, not a reason to write directly into its SQLite database.

### MCP scope, storage, and CodeGraph evidence

[MCP documentation](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/docs/mcp-server.md) and [tool enforcement](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/src/mcp/tools.rs) establish:

- `serve` defaults to **global** metadata scope. `serve --group lekalo-dev` constrains it to that group. `--scope current-project` and `--project ROLE` are strict single-project scopes; they do not grant access to peer projects' schemas just because those peers share a group.
- The 17 default tools cover context/read/note search/Markdown search, service graph/events/details, group/project lists, shared tree/grep, and six CodeGraph tools. `workspace_context` returns metadata, not schema contents; `workspace_read` fetches an approved shared item.
- `AI_WORKSPACE_ALLOW_PROJECT_WIDE_TOOLS=1` separately enables project-wide reads/tree/grep and absolute project-path metadata within server scope. `AI_WORKSPACE_ALLOW_PROJECT_FILE_WRITE=1` separately exposes `project_file_write`, which writes **and shares** a file. Both must be disabled for this integration.
- Hidden and credential-like paths are excluded by default. `workspace_read`, tree, and grep have explicit `include_hidden`/`include_sensitive` options; Markdown full-text search does not honor those overrides. Approved `.ai-factory` context has a narrow exception. This is path policy, not content classification or a guarantee that arbitrary shared prose contains no secrets.
- Initialize instructions encourage shared context and CodeGraph before broad scans, but the client/agent must follow them. They also mention writing durable context; our configuration keeps the write tool disabled.

[SQLite schema](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/src/db/schema.rs): schema version 7, WAL and foreign keys; default home `~/.ai-workspace/workspace.db`, override `AI_WORKSPACE_DB`. Tables include `projects` (real local paths), `groups`, `project_groups`, `shared_items`, `service_links`, `artifact_dependencies`, `workspace_events`, `event_groups`, `event_targets`, `event_artifacts`, `indexed_files`, notes/files FTS, `code_files`, `code_nodes`, `code_edges`, `code_unresolved_refs`, CodeGraph FTS, and `cloud_sync_state`. Files are referenced by relative path, notes/FTS contain text, and CodeGraph holds derived source structure. This database and WAL are private local state, not a shareable artifact, committed schema store, or Lekalo history store.

[CodeGraph implementation](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/src/codegraph.rs) stores file hashes/times, node locations/signatures/docstrings, unresolved references, and edge provenance `rust-regex-mvp` / `rust-regex-mvp-resolver`. MCP rechecks current share visibility, including stale indexed rows that are no longer shared. However, `codegraph_context` returns task text, symbol locations, signatures, counts, and **live source snippets**; it does not attach a complete Git revision, file hash, parser version, or semantic confidence envelope. A sync timestamp alone cannot prove source freshness. An integration provenance wrapper and upstream richer metadata are required before these facts can be retained as evidence.

Existing upstream tests provide useful future regression targets, not newly executed evidence: `tests/cli_tests.rs` covers auto-share suppression with config, events/inbox, CodeGraph incremental updates and sensitive paths; `tests/mcp_tests.rs` covers scopes, direct-call denial, shared-path boundaries, and stale CodeGraph visibility; `tests/mcp_subscriptions.rs` covers scoped resource updates. [Cloud documentation](https://github.com/lee-to/ai-workspace/blob/8fdf818fee757d24e723d657fc5d38614995e557/docs/cloud.md) is separate from local MCP behavior.

### Recommended local group/setup

Use a dedicated, private `AI_WORKSPACE_DB` for `lekalo-dev`, selected consistently for **every** CLI/MCP process; this also confines the global portion of `sync`. Configure its host path only in local operator settings, never committed configuration or reports. Local aliases resolve to real checkout roots only on that machine:

| Role alias / registered name and slug | Local binding supplied by operator | Approved sharing |
| --- | --- | --- |
| `lekalo-core` | Lekalo repository root, including core and in-tree adapters | Individually approved committed schemas, target-protocol docs, ADRs |
| `greenfield-consumer` | Greenfield consumer checkout | Nothing initially; optional sanitized contract-consumption declaration |
| `brownfield-consumer` | Brownfield consumer checkout | Nothing initially; optional sanitized contract-consumption declaration |
| `aifhub-extension` | Extension checkout | Explicit schema/trace integration docs only |
| `aifhub` | Workflow/application checkout | Explicit workflow/protocol docs only |
| `laratesto` | PHP consumer checkout | Explicit approved integration docs only |

These are role labels, not a request to discover or publish actual consumer repositories. The same role may bind to different checkouts on different machines; multiple instances need separate local aliases with no private basename. An external adapter checkout may receive an optional `adapter-node-typescript` or `adapter-php-laravel` role. In-tree adapters remain scopes of `lekalo-core`; do not register the same root repeatedly to manufacture separate projects. Role aliases are not Lekalo semantic rename aliases.

Before `init`, create a locally excluded `.ai-workspace.local.json` in each participating checkout. Example for core (other projects substitute the role name/slug):

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

An operator must first confirm that this local config is untracked/ignored, the selected database is dedicated, and any existing registration already uses the expected alias and share set. Do not silently rename, reset, or migrate an existing global database. Configuration import can remove old memberships/shares: inspect before applying to an existing project.

Source-verified examples, to run only after reviewing each share; repeat `init` inside every opted-in repository with its matching config:

```text
ai-workspace --config .ai-workspace.local.json init
ai-workspace --config .ai-workspace.local.json share contracts/target-protocol.schema.v0.3.2.json --label "Lekalo target protocol 0.3.2"
ai-workspace --config .ai-workspace.local.json share docs/target-protocol.md --label "Lekalo target protocol"
ai-workspace --config .ai-workspace.local.json share docs/adr/0025-target-protocol.md --label "Lekalo protocol ADR"
ai-workspace --config .ai-workspace.local.json share contracts/trace-manifest.schema.v0.2.16.json --label "Lekalo trace manifest 0.2.16"
ai-workspace link add greenfield-consumer lekalo-core --kind depends_on --label "target-protocol"
ai-workspace link add brownfield-consumer lekalo-core --kind depends_on --label "target-protocol"
ai-workspace link add aifhub-extension lekalo-core --kind depends_on --label "trace-schema"
ai-workspace serve --group lekalo-dev
```

Add direct subscriptions for `aifhub`, `laratesto`, and external adapters only when their actual consumption warrants it; group membership is context visibility, not proof of dependence. If transitive consumers must receive core changes, explicitly register their reviewed direct core subscription for now. Avoid `related_to` merely for presentation because upstream currently counts it as event impact too.

In a consumer, an optional **new, reviewed, identity-free** `docs/lekalo-integration.md` can be shared and declared dependent with `artifact depends docs/lekalo-integration.md lekalo-core --kind consumes_api --reaction inspect`. Keep this declaration as links/version requirements, not a copy of Model/IR. Consumer source files and physical filenames are not needed to demonstrate affected-project delivery. Never share entire consumer `docs/`, root directories, `.lekalo/`, or raw pilot output by default.

The client launches the group-scoped server with both widening flags explicitly `0`, overriding inherited values, and no cloud credentials. Agent flow: `workspace_context` → locate core's approved schema item ID → `workspace_read` → check schema identity/version against the consuming adapter → inspect `workspace_service_graph` and `workspace_events` → use a bounded local `lekalo context`/`inspect` or, for opted-in public Rust navigation, CodeGraph → broaden a scan only for an explicit unresolved question. Do not embed transient database item IDs in committed instructions.

For the public-core benchmark only, explicitly add `share crates/lekalo-core/src --label "Lekalo public Rust core"`, then `codegraph sync --project lekalo-core`; no `--full-project` or project-wide MCP flag is needed. This deliberately exposes that source scope and snippets to the chosen local group. Keeping CodeGraph in a separate benchmark database/group is preferable to expanding the normal schema-only setup.

## Lekalo integration points

| Current surface and source | What can be reused | What must not be inferred |
| --- | --- | --- |
| [CLI](../../crates/lekalo-cli/src/main.rs), [Git input](../../crates/lekalo-cli/src/git_input.rs) | Read-only command dispatch and typed Git change handoff; `lekalo --json inspect SYMBOL`, `lekalo --json impact SYMBOL`, `lekalo --json impact --changed --base BASE --head HEAD`, `lekalo --json context SYMBOL --budget N` | No existing workspace hook/subcommand/event bus was found. Semantic model events/effect edges describe application behavior, not filesystem or cross-project change notifications. |
| [Inspect](../../crates/lekalo-core/src/inspect/mod.rs), [impact](../../crates/lekalo-core/src/impact/mod.rs), [context contract](../../contracts/context-capsule.schema.v0.2.16.json) | Versioned deterministic projections, impact explanation paths/reasons/confidence/completeness, bounded context and explicit gaps | Model symbols, descriptions, logical paths, and target names can still identify a private consumer. Deterministic/path-safe JSON is not automatically public. Context `--spans` is opt-in; omit it for public evidence. |
| [Protocol](../target-protocol.md), [protocol schema](../../contracts/target-protocol.schema.v0.3.2.json), [ADR](../adr/0025-target-protocol.md), `contracts/` | Exact committed public contract paths/identities/digests are the first change trigger and shared context | Product `0.6.3` does not mean every contract is `0.6.3`: protocol is `0.3.2`, several projection schemas are `0.2.16`. Derive version from the actual contract. A Markdown/schema edit may have no Model symbol; semantic impact alone cannot detect it. |
| [Trace manifest](../trace-manifest.md), `trace collect|validate|export|query` in CLI | Neutral versioned links from requirements through symbols/artifacts/tests/gates, source revision/digest, evidence status and confidence; AIFHub can consume its schema | Foreign IDs and logical paths can disclose identity. Contract/writer/query are Lekalo-owned; persisted `trace.manifest` remains AI Factory-owned direct evidence under `.ai-factory/traces/**`. Workspace cannot become its writer or canonical owner. |
| [Run history](../../crates/lekalo-core/src/run_history/mod.rs), [implementation limitations](../m6/issue-121-implementation.md) | Offline `.lekalo/history/` SQLite, version `0.4.0` record/assertion/observation contracts, exact pins, scoped local dependent registration/resolution | Implemented but explicitly `local-private` / `ineligible`; no export, upload, network destination, or automatic producer instrumentation. Do not tail its DB or forward `history show` output. Public metric aggregates belong to #102, not #37. |
| [Privacy runtime](../privacy-runtime.md), [frozen refs](../../crates/lekalo-core/src/privacy/refs.rs), [export engine](../../crates/lekalo-core/src/privacy/export.rs) | `privacy subject|evaluate|export|redact`; accepted policy/authority `0.3.2`, class floors, evidence binding, redaction and residual-leak refusal | Destination `workspace` is a policy term, not a connection to the AI Workspace executable. Unknown artifact kind denies; redaction preview is not export authorization. A new hook may not invent a kind or bypass accepted ownership. |
| [Pilot harness](../../scripts/pilot-brownfield-ts.mjs), [privacy probes](../../scripts/test-pilot-brownfield-ts.mjs), [#118 report](../m6/issue-118-implementation.md) | Closed metrics/member allowlists, identical leak probes across `metrics.json` and `report.md`, synthetic fixture, explicit stale/unknown evidence and byte-identical revert | Current probes check selected strings (fixture paths, symbols, Windows drive spelling), not every leak channel. They exercise the successful fixture flow, and the harness retains a canonical-path digest and failure text. Neither is a suitable public event payload. Extend the discipline, do not blindly reuse its raw output. |

### Proposed deliverable and repository split

All paths in this subsection are **future implementation proposals**; only this research file is changed now.

| Owner | Deliverable / responsibility |
| --- | --- |
| **Lekalo (#37)** | `docs/integrations/ai-workspace.md`: reviewed role/setup template, explicit shares, agent order, independent-operation fallback, provenance/privacy rules. `scripts/ai-workspace-hook.mjs`: small optional change/export adapter outside Rust core; dependency-free Node style follows existing scripts. A synthetic fixture family, `scripts/test-ai-workspace-hook.mjs`, and a separate pinned-upstream MCP integration gate prove the boundaries. |
| **Lekalo authority/privacy owners (#2/#119/#120)** | Review artifact ownership and exact export policy for the new event envelope, share manifest, local outbox/receipt, and public benchmark evidence. Proposed kinds such as `workspace.change-event` are **not accepted registry entries today**. Use the successor procedure, not hand-edits to frozen references or the HLV metrics kind. No production emission before admission. |
| **AI Workspace** | Existing group/share/service/event/MCP/SQLite/CodeGraph implementation; future transactionally complete event creation, typed metadata/idempotent create with machine-readable receipts, optional artifact/version targeting, transitive impact semantics, full CodeGraph revision/hash/parser provenance, and optional role-alias ergonomics. Maintain authorization/path rules and tests upstream; Lekalo must not import private DB schemas. |
| **AIFHub Extension / agent clients** | Configure the local group-scoped MCP process, consume schema/trace documents and event inbox, implement subscriptions/polling and context refresh, show candidate impact with explanations, and avoid assuming a notification starts an agent turn. No extension implementation belongs in this research commit. |
| **Consumer repositories / operator** | Own local role-to-checkout mapping, participation consent, contract-consumption declarations, exclusions, and intentional links. No discovery of private names or auto-enrollment of repositories. |

The shim should accept explicit base/head commits and an approved share/subscription manifest, produce a bounded deterministic dry-run change envelope, and send only when explicitly enabled. It is invoked manually or by an explicitly installed post-change/CI hook; it is not called from Model compilation, target protocol execution, `history`, or default `scan`/`verify`. An absent binary/config produces an integration-only `unavailable`/`disabled` result without changing normal Lekalo output or exit behavior. An explicitly requested send that fails must return failure/unknown delivery, not pretend success.

## Event/provenance design draft

### Contract-change detection and explainability

1. Resolve base/head to immutable public Lekalo commits through argv-only read operations. Enumerate only approved public protocol/schema/ADR paths (not all repository contents). Compare path existence plus bytes: add, modify, and delete are distinct. A missing Git base is `unknown`, not “no changes.” Dirty-worktree event emission is refused in the first version; public hashes are over approved public bytes only.
2. Trigger directly on `docs/target-protocol.md` and approved versioned protocol/schema changes. Docs-only changes still produce a review event with compatibility `unknown` unless a reviewed contract diff proves otherwise. Record old/new contract identity and digest, including version-preserving content changes; deletion keeps the old identity and explicit deleted state.
3. Resolve affected **role aliases** through the reviewed dependency manifest and current workspace service/artifact declarations. Preserve the explanation `public artifact changed → core service → declared incoming subscription → affected role`, plus dependency kind/reaction. No-match means “no declared subscribers”; it does not prove no impact.
4. Optional `lekalo impact` input augments this explanation with validated semantic reason chains, confidence, and completeness, only after authorization and role mapping. Never reinterpret an unresolved protocol file as a zero semantic impact. Missing, stale, truncated, or unsupported evidence remains explicit. Do not turn CodeGraph guesses into semantic edges or automatically rewrite consumer artifact dependencies.
5. Use the existing `service_changed` source event for a target-protocol change. AI Workspace snapshots all direct source dependencies; report this conservative set as candidates. A schema-specific/transitive exact affected set needs upstream support or explicit direct subscriptions, not a fictitious `--affected-projects` flag.

Example using existing upstream syntax, after policy and preflight checks:

```text
ai-workspace event create --kind service_changed --source lekalo-core --severity warning --title "Lekalo target protocol changed" --body "Review the approved protocol change envelope before updating consumers."
```

The real shim supplies bounded serialized approved metadata as `--body` using an argument array, never a shell-assembled command, raw Git diff, private filename, or arbitrary user prose. It does not post GitHub/Slack messages. Upstream CLI stdout/stderr can include real paths, snapshot names, and artifact paths: capture privately and emit only a closed safe result. Read back the event and verify target/reason rows through scoped MCP before reporting `delivered`.

### Proposed envelope and provenance rules

This is a draft schema contract, not a currently accepted wire format. Close all objects/arrays, validate bounds and unknown fields, canonicalize ordering, and bind content digests. Suggested initial limits: 64 changed public artifacts, 64 roles, 128 explanation chains, 64 KiB event body; refuse over-limit without silently truncating away affected roles.

| Field family | Required meaning |
| --- | --- |
| Identity | Versioned envelope identity, `eventKind: protocol-change|schema-change`, source role `lekalo-core`, deterministic event key |
| Source | Public Lekalo base/head revision, clean-state proof, producer build/version and shim version, approved manifest digest |
| Artifacts | Allowlisted public repository-relative path, change kind, old/new schema identity/version/digest; explicit absent/deleted/unknown states |
| Impact | Affected role aliases; ordered typed explanation steps; `declared-service-link`, `declared-artifact-dependency`, and `lekalo-impact` remain distinguishable evidence origins; completeness and limitations |
| Policy | Exact accepted policy/authority refs and export decision/consent binding appropriate to the destination; public-safe decision reference only |
| Optional semantic evidence | Impact contract/algorithm/digest, source/model/IR/profile pins where actually available, reason IDs and confidence after export approval; no Model or IR bodies |
| Optional CodeGraph evidence | AI Workspace commit/package version, parser provenance string, public source revision, per-file content digest checked around sync/read, approved scope digest, node locator and bounded query settings, stale/unresolved status; navigation evidence only |

Keep upstream numeric project/event/item IDs, real local paths, delivery timestamps, and detailed receipts in private local state. They are installation-specific routing data, not portable semantic identity. The public projection contains only public source facts and neutral role/explanation data; omit private consumer digests, private commits, source signatures/snippets, foreign trace IDs, and private artifact names. Hashing a private path/name does not make it safe.

Workspace JSON shares point to live files. A consuming agent must verify contract identity/version and the digest associated with the public source revision; a label such as “0.3.2” is not proof of immutable bytes. Workspace notes may hold a short pointer to a committed schema/ADR plus provenance, never authoritative entity definitions, compiled IR, copied semantic graphs, or a second editable model.

### Retry, staleness, and independence

The event key is a hash of the canonical **approved public** producer identity, base/head, artifact changes, and reviewed routing-manifest version. Maintain a local single-writer outbox with `planned|sending|delivered|unknown-delivery|refused` states once its custody is admitted. Record the intended key before invoking the CLI; retain only private bounded receipts. Ordinary repeats after verified delivery are no-ops.

Because upstream create has neither an idempotency key nor an atomic target snapshot, a timeout/crash/nonzero result after invocation is `unknown-delivery`. Reconcile by key via scoped event reads and verify expected targets; do not blindly resend. This is not an exactly-once guarantee. Concurrent writers, partial events, or inconsistent snapshots require repair/review until upstream transactional keyed creation lands. A routing change needs a new manifest/key; old event snapshots do not silently acquire newly linked consumers.

No auto-close based on compilation success: an affected consumer owns its review. A removed schema, failed CodeGraph sync, or hash mismatch leaves an explicit pending/stale result. Without AI Workspace, users keep using the existing committed contracts and `inspect`/`impact`/`context`/`trace` commands; no network/SQLite probe, global config mutation, or integration receipt is needed for those commands.

## Privacy boundary + test plan

### Boundary mapping

| Issue boundary | Design enforcement | Proof required before implementation acceptance |
| --- | --- | --- |
| B1 — workspace is not canonical Model/IR storage | Read-only pointers/derived evidence; exclude `lekalo/**`, compiled IR/model bodies, and model-sized notes from share manifest | Reject model/IR payloads and unsolicited note creation; remove workspace DB and show ordinary compilation/projections unchanged |
| B2 — SQLite does not replace committed schemas | Git remains authoritative; pin public revision + contract identity + digest; no DB reads as schema source | Rebuild fresh workspace registration from approved committed files; tampered/stale shared bytes fail the digest check |
| B3 — content sharing is opt-in | Explicit empty config before init, individually reviewed shares, no preset/cloud, private role mapping | Register with sentinel README/package/private docs and assert none is shared; dry-run produces no side effects |
| B4 — no automatic project-wide reads/writes | Dedicated DB/group, both widening env flags forced off, no `--full-project`, no hidden/sensitive overrides | MCP tool-list and direct invocation checks; sentinel unshared paths remain inaccessible even with inherited hostile env |
| B5 — no private names, absolute paths, or secrets exported | Closed role-based projection plus accepted privacy export pipeline and final leak scan; private log quarantine | Probe every output channel, metadata/name snapshots and failure paths; no bare identity/path hashes |
| B6 — CodeGraph provenance, no semantic substitution | Explicit parser/source/hash/scope wrapper, stale/unresolved status, snippets excluded from public events | Stale index/current source mismatch refuses evidence; regex-derived edges never satisfy semantic/trace confirmation gates |

Use #119's accepted pipeline where an artifact is admitted: classification plus destination-specific evaluation, bound evidence, required transformations, and verification scan. Do not bypass denial by relabeling a workspace event as `context.capsule`, `trace.manifest`, or HLV metrics. Current accepted authority lists no workspace change-event kind; its admission is a concrete implementation prerequisite. Sharing already public schemas must not imply permission to transfer consumer-derived data or publish a local group.

The current #118 gate is a useful allowlist/probe pattern, **not** a blanket privacy attestation. Its `consumer.pathDigest` is a stable hash of a physical path, and error text is allowed in failed step records; both stay out of the proposed public artifact. Raw `inspect`/`context`/`trace` and CodeGraph JSON must never be forwarded on the assumption that absence of absolute paths makes them safe.

### Planned tests and validation gates

| Test | Fixture/action and observable pass condition | Owner |
| --- | --- | --- |
| T1 — optionality | Run existing core/CLI fixtures with binary missing, empty integration config, corrupt external DB, and network unavailable; baseline bytes/exits unchanged. Explicit shim invocation reports disabled/unavailable/refused accurately. | Lekalo |
| T2 — shared schema reachability | Isolated temporary DB, public core fixture plus synthetic greenfield and brownfield roots; strict group MCP initialized from consumer. Context finds only approved shares; read returns exact schema bytes/identity. Wrong-group and strict single-project requests cannot read core shares. | Joint MCP integration gate |
| T3 — explained protocol change | Two pinned public contract revisions; protocol-only, schema-only, deletion, unrelated-file, and same-input repeats. Event reaches each declared dependent, excludes unrelated projects, and has typed artifact→source→subscription reasons and matching old/new digests. No semantic source-map match still triggers protocol review. | Lekalo + upstream event readback |
| T4 — delivery failures | Fake executable/transport for missing process, stderr with secrets, timeout before/after create, partial targets, concurrent send and restart. Capture raw logs privately; no duplicate blind retry; partial/unknown delivery never reported as success. Repeat at pinned upstream version. | Lekalo; upstream transaction/key fix |
| T5 — no implicit scope growth | Sentinel secrets in README, package metadata, `.env`, hidden dirs, key files, unshared Rust, sibling projects, links/junctions outside root, and Windows case/drive/UNC variants. Config-before-init prevents auto-share; shared reads confined; absent write tool/direct write call refuses; malicious inherited flags overridden. | Joint |
| T6 — complete output privacy | Synthetic private repo/name/path/URL/token/PII markers in consumer metadata, labels, dependency paths, note text, CodeGraph task/snippet, trace external IDs and injected child errors. Check JSON, Markdown, event title/body/details, stdout/stderr, receipts selected for publication, and artifact names. Reject unknown fields and identity/path hashes. Include failed runs, not just successful fixtures. | Lekalo |
| T7 — ownership/policy | Deny unknown kinds, missing/expired/mismatched consent, unclassified data, unauthorized scopes, residual leaks, recorder payloads and canonical Model/IR bodies. Verify unchanged accepted custody refs and no implicit policy exception. | Lekalo privacy owners |
| T8 — evidence/provenance | Missing/stale/truncated impact; changed file after sync; removed share with old indexed rows; unknown parser version; wrong contract digest and moved aliases. States remain explicit, no guessed zero-impact/safe outcome, no elevated CodeGraph confidence. | Joint |
| T9 — neutral consumer parity | Same synthetic schema/event flow from greenfield, brownfield, extension, workflow, and PHP role fixtures; role binding differs only in private config. Foreign names never needed to pass. | Lekalo fixtures + downstream clients |
| T10 — no duplicated authority | Inventory shared items and event payload members; only approved committed files/pointers and derived event data. Delete/recreate external DB without losing any Model/schema/trace source. No direct writes to AI Workspace tables or Lekalo recorder storage. | Joint |

Future Lekalo gates: `node scripts/test-ai-workspace-hook.mjs` (proposed), a pinned-binary MCP gate over isolated temp databases (proposed), `node scripts/check-authority.mjs`, `node scripts/check-privacy.mjs`, `node scripts/check-contract-versions.mjs`, and `node scripts/test-fixture-provenance.mjs`. Run `node scripts/test-pilot-brownfield-ts.mjs` only if its harness/probes are changed; retain the accepted privacy-runtime/leak corpus gates when connecting the exporter. Keep cloud and real private consumers outside routine CI. For new contracts, use the product version of the implementing change, not an invented fixed successor version.

### Rust CodeGraph context benchmark protocol

AC6 remains **unmeasured** in this research. Implement `scripts/benchmark-ai-workspace-context.mjs` plus an approved result schema later, with raw measurements local and public-core/synthetic aggregate evidence publishable only through the reviewed policy. Proposed reproducible benchmark:

1. Pin upstream `8fdf818fee757d24e723d657fc5d38614995e557`, compiler/build, OS, Lekalo revisions, exact shared scope, queries, and budget. Use public scratch copies, dedicated database, no private group or user config. One concrete core change is `3d7cfcfb87576147ec0b43dcc2380ddfeb677193` against its resolved first parent (history custody/recovery); add reviewed impact/context and cross-file caller changes. Record full resolved hashes, never an unpinned moving branch.
2. Predeclare at least three navigation tasks and expected relevant files/symbols/callers from reviewed source diffs. Compare baseline bounded file/grep retrieval against `workspace_context` + `codegraph_context`/node/callers/callees with the same scope, question, and output budget. Keep semantic `lekalo context` as a separate measure: Model context and Rust code navigation answer different questions.
3. Measure cold reindex, unchanged warm sync, changed-file sync, deleted/renamed file handling, cold/warm retrieval latency, files/read bytes, returned context size/token estimate, expected-evidence coverage and misses, unresolved references, and stale-result rate. Run at least five repetitions per timing case; report raw counts plus median/range, not just an improvement percentage. Real model token/cost fields remain unknown unless measured.
4. Force an edit after sync and a revoked share before querying. Public provenance must identify stale/unknown evidence or cause rejection; revoked scopes remain inaccessible. Record bounded retrieval limits, parser limitations, and task success under the same rubric. Agent order logs must show shared context before any broad scan.
5. Acceptance evidence is a reproducible completed comparison with explicit findings/limitations, including regressions; no preclaimed speedup threshold. Privacy, no scope widening, and no semantic substitution are hard gates. Running upstream unit tests or counting indexed nodes alone does not satisfy AC6.

## Acceptance-criteria mapping

### Scenarios

| Issue scenario | Plan coverage / implementation owner |
| --- | --- |
| S1 — group core, both consumer modes, extension, AIFHub, Laratesto with local role aliases | Recommended setup table/config above; six explicit roles; adapters handled as in-tree scopes or optional external roles. Operator mapping is private; no upstream alias command assumed. |
| S2 — share versioned schemas, protocol docs and ADRs | Individual `share` examples, Git/version/digest verification, local consumer `workspace_read` flow; docs and shim owned by Lekalo, access enforcement upstream. |
| S3 — register service links | Consumer→provider `link add`, reviewed direct subscriptions and optional artifact dependency; actual dependencies confirmed by each participant. |
| S4 — protocol/schema change creates event for affected projects | Explicit contract-path trigger, existing `service_changed`, role-based explanations, readback and uncertain-delivery handling; shim in Lekalo, durable event fixes upstream. |
| S5 — agent gets shared context before broad scan | Ordered agent workflow and T2/benchmark call-order evidence; agent/extension owns scheduling and fallback. |
| S6 — Rust CodeGraph navigates core | Explicit source share + sync and benchmark protocol; parser provenance/staleness retained, public core only. Upstream owns graph extraction. |
| S7 — impact export augments artifact dependencies | Optional validated semantic explanation input and reviewed semantic-to-shared-artifact/service mapping; no automated graph promotion, transitive assumption, or raw consumer impact upload. |

All boundaries B1–B6 are individually mapped in the preceding section.

| Acceptance criterion | Concrete completion evidence for the eventual implementation | Research status |
| --- | --- | --- |
| AC1 — recommended group/setup with aliases documented | Setup table, explicit config-before-init, scoped MCP and fallback instructions; validate on T2/T9 fixtures | Recommended setup documented here; runnable setup not executed |
| AC2 — target-protocol change creates explainable affected-project event | T3 produced event + readback identifies old/new contract digests and each declared affected role/reason; T4 proves honest partial/retry handling | Design complete; hook/event demonstration pending |
| AC3 — agent in consumer can read shared schemas | T2 group-scoped MCP from consumer reads exact approved JSON schema without private/unshared reads | Source support confirmed; end-to-end proof pending |
| AC4 — Lekalo fully functional without AI Workspace | T1 unchanged command results offline/without binary/with broken external state; no unconditional dependency or hook | Existing separation confirmed; regression proof for future shim pending |
| AC5 — MCP scope and sensitive-path policy never silently widened | T5 plus reviewed launch/config diffs, denied direct tool calls, two flags forced off | Upstream controls inspected; integration tests pending |
| AC6 — Rust CodeGraph context benchmark on core changes run | Pinned, repeated cold/warm comparison and stale/revocation cases with coverage/cost evidence under the protocol above | **Not run; no performance or task-success claim** |
| AC7 — no canonical model duplicated in notes | T7/T10 share/payload inventory, no model/IR body, disposable workspace DB | Ownership design complete; runtime audit pending |
| AC8 — public events/evidence reveal no private identity | T6 success/failure-channel leak probes, neutral role projection, no private source/path/name hashes or recorder outputs | Privacy plan complete; adversarial end-to-end proof pending |

This research dispatch is complete when this one file is validated and committed. It does not close the implementation acceptance criteria or authorize changes in other repositories.

## Risks

- **Auto-sharing and broad defaults:** plain `init`, global `serve`, inherited flags, AI Factory preset, directory shares, or accidental `cloud push` can expose content outside the agreed set. Config-first registration, dedicated DB/group, explicit launch environment, and positive share allowlists are required.
- **Identity survives aliases elsewhere:** upstream events snapshot names and artifact paths; FTS, snippets, free-form bodies, labels, errors, and old registrations can retain identity. Renaming one slug or hashing a path is insufficient; validate the whole surface and keep original database/logs private.
- **Event completeness and retry:** ordinary create is not transactional, lacks keyed idempotency and JSON receipts, and notifies direct source dependents irrespective of link kind/schema relevance. Expose candidate impact and unknown delivery; prioritize upstream fixes before unattended retries or stronger guarantees.
- **Scope is not a confidentiality boundary within a group:** every participating agent can read intentionally shared group material. A server's ability to support sensitive-path override flags is not permission for this integration to use them. No consumer source sharing is required for the initial setup.
- **CodeGraph is approximate and can be stale:** regex resolution omits Rust semantics; context mixes indexed locations with current snippets. Require pinned source/scope and freshness checks, keep unresolved facts visible, and never let it override Lekalo IR or confirmed trace evidence.
- **Authority admission remains open:** current accepted policy does not contain the proposed event kinds. Consent and role labels cannot mint canonical ownership. Treat admission of storage/export artifacts as an explicit dependency, including any future local outbox path.
- **Multiple evolving version families:** product, protocol, projection schema, accepted privacy policy, upstream package and SQLite schema versions differ. Pin actual identities and upstream commit; avoid documentation examples or package version as compatibility proof.
- **Cloud is not local MCP parity:** hosted snapshots omit JSON file contents and CodeGraph; resource support does not guarantee agent wakeups. Remote access requires a separately scoped design and policy review, not an implicit fallback.
- **Historical privacy probes are limited:** #118's success-fixture allowlists do not establish privacy on arbitrary errors or identity hashes. Do not copy old reports/configs wholesale into shared context; test every publication channel independently.
- **Benchmark evidence outstanding:** source inspection validates surface availability, not performance, Windows execution, end-to-end event delivery, or correctness of a running multi-project setup. The implementation must supply those results before claiming issue acceptance.

Research validation: `node scripts/check-authority.mjs`, `node scripts/check-privacy.mjs`, and `node scripts/check-contract-versions.mjs` passed on the stated Lekalo baseline. A document check verified all six required sections, all 21 scenario/boundary/acceptance mappings, and every relative file link. Whitespace and the staged single-file boundary are checked before committing; no proposed integration tests or benchmark were run.

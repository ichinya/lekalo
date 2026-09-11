# Lekalo diagnostics

Issue #11 defines the stable, machine-readable diagnostic contract: one
closed wire item (`lekalo/diagnostic/v1.0.0`), one embedded rule registry
(`dev.lekalo.diagnostic-registry@1.21.0`, registry version `1.21.0` — issue #12 added the `semantic.*`/`validate.*` families and issue #13 added the `graph.*` family, each as a wire-shape-preserving minor increment; issue #14's effect graph reuses the `graph.*` infrastructure rules with bounded tokens and keeps its comparison states as result data; issue #15 added the `inspect.*` family for the single-symbol inspect projection as the same kind of minor increment; issue #16 adds the `impact.*` family plus the one `denied` rule its strict bounded denial requires as the same kind of wire-shape-preserving additive minor increment; issue #18 adds the `diff.*` family for the semantic diff as the same kind of wire-shape-preserving additive minor increment; issue #25 adds the `authorization.*` family for the authorization contract as the same kind of wire-shape-preserving additive minor increment; issue #62 adds the `error.*` family for the explicit error contracts as the same kind of wire-shape-preserving additive minor increment; issue #26 adds the `extended.*`, `event.*`, `job.*`, `call.*`, `cache.*`, `publication.*`, `contract.*`, and `case.*` families for the extended effect contracts as the same kind of wire-shape-preserving additive minor increment; issue #63 adds the `invariant.*` family for first-class invariants and state transitions as the same kind of wire-shape-preserving additive minor increment; issue #27 adds the `target.*` family for the target process protocol as the same kind of wire-shape-preserving additive minor increment; issue #36 adds the `requirements.*` family for the OpenSpec requirement traceability integration as the same kind of wire-shape-preserving additive minor increment; Issue #31 adds the four `adapter.*` rules (`LEK-ADP-002..005`) — the closed conformance-suite failure classes `check-failed` (invalid), `adapter.process-failure` (unavailable), `protocol-failure` (unsupported), and `security-failure` (denied) — and publishes 1.15.0 additively over the accepted frozen 1.14.0. Issue #39 adds the eleven `observed.*` rules (`LEK-OBS-001..011`) — the closed refusals and findings of the observed mode: scan normalization and bounds, unknown modules and symbols, the confirmation rule, the staleness gate, the observed-graph incompleteness report, and the promotion refusal and plan-pinning rules — publishing 1.16.0 additively over the accepted frozen 1.15.0. Issue #30 adds the seven `implementation.*` rules (`LEK-IMPL-001..007`) — the closed refusals and portability warning of the foreign/custom implementation attachment: document shape, effect-surface mismatch, unresolved operation references, ambiguous and unknown selections, refused symbol spellings, and missing target implementations — publishing the reserved 1.18.0 additively over the accepted frozen 1.16.0 (1.17.0 stays reserved by its parallel owner), a typed
Rust API, deterministic normalization, and the human and JSON projections
through the shared `DomainResult` envelope. Issue #42 adds the three `bindings.*` rules (`LEK-BND-001..003`: `bindings.proposal-unknown`, `bindings.ambiguous`, `bindings.plan-mismatch`) — the closed refusals of the issue #42 binding workflow: an unknown proposal id, an ambiguous proposal confirmed without naming one candidate of its set, and a batch applied against drifted state — publishing 1.20.0 additively over the accepted frozen 1.19.0 (issue #40's `contracted.*` family, itself published additively over the accepted frozen 1.18.0). Issue #28 adds the single `target.ir-unsupported` rule — the capability-discovery refusal that keeps an adapter which never declared the core IR contract version from receiving project IR — as the same kind of wire-shape-preserving additive minor increment. Issue #29 adds the six `target-profile.*` rules (`LEK-TPF-001..006`) — the closed refusals of the composable target profile contract: document shape, unknown component, invalid inheritance reference, incompatible combination, unsatisfied capability requirement, and unacknowledged inheritance weakening — as the same kind of additive minor increment. Issue #38 (integrated first as registry 1.13.0) adds the five `init.adopt-*` rules (`LEK-INIT-001..005`) for the `init --adopt` boundary; this integrated line publishes 1.14.0 additively over the accepted frozen 1.13.0. Issue #31 adds the four `adapter.*` rules (`LEK-ADP-002..005`) — the closed conformance-suite failure classes `check-failed` (invalid), `adapter.process-failure` (unavailable), `protocol-failure` (unsupported), and `security-failure` (denied) — and publishes 1.15.0 additively over the accepted frozen 1.14.0. Issue #39 adds the eleven `observed.*` rules (`LEK-OBS-001..011`) — the closed refusals and findings of the observed mode: scan normalization and bounds, unknown modules and symbols, the confirmation rule, the staleness gate, the observed-graph incompleteness report, and the promotion refusal and plan-pinning rules — publishing 1.16.0 additively over the accepted frozen 1.15.0. The contracts are published as
Issue #64 adds the ten `query.*` rules (`LEK-QRY-001..010`: `query.input-invalid`, `query.contract-invalid`, `query.source-invalid`, `query.filter-invalid`, `query.sort-invalid`, `query.pagination-invalid`, `query.tenant-filter-missing`, `query.visibility-boundary`, `query.reference-invalid`, `query.export-limit`) — the closed refusals of the issue #64 declarative query model: malformed wire input, incoherent declarations, write surfaces in read slots, filters or sorts or pagination that violate the closed grammar or the bound field types, strict-profile tenant filter omissions, visibility boundary crossings, unresolved references, and the canonical payload bound — publishing 1.21.0 additively over the accepted frozen 1.20.0 (issue #42's `bindings.*` family).


- [`contracts/diagnostic.schema.v1.0.0.json`](../contracts/diagnostic.schema.v1.0.0.json) — one diagnostic item,
- [`contracts/diagnostic-registry.schema.v1.0.0.json`](../contracts/diagnostic-registry.schema.v1.0.0.json) — the registry schema,
- [`contracts/diagnostic-registry.v1.20.0.json`](../contracts/diagnostic-registry.v1.20.0.json) — the current registry instance,
- [`contracts/diagnostic-registry.v1.19.0.json`](../contracts/diagnostic-registry.v1.19.0.json) — the accepted predecessor instance (issue #40),
- [`contracts/diagnostic-registry.v1.18.0.json`](../contracts/diagnostic-registry.v1.18.0.json) — the accepted predecessor instance (issue #30),
- [`contracts/diagnostic-registry.v1.16.0.json`](../contracts/diagnostic-registry.v1.16.0.json) — the accepted predecessor instance (issue #39),
- [`contracts/diagnostic-registry.v1.15.0.json`](../contracts/diagnostic-registry.v1.15.0.json) — the accepted predecessor instance (issue #31),
- [`contracts/diagnostic-registry.v1.14.0.json`](../contracts/diagnostic-registry.v1.14.0.json) — the accepted predecessor instance (issue #29),
- [`contracts/diagnostic-registry.v1.13.0.json`](../contracts/diagnostic-registry.v1.13.0.json) — the accepted predecessor instance (issue #38),
- [`contracts/diagnostic-registry.v1.12.0.json`](../contracts/diagnostic-registry.v1.12.0.json) — an earlier accepted predecessor instance.
- [`contracts/diagnostic-registry.v1.10.0.json`](../contracts/diagnostic-registry.v1.10.0.json) — an earlier accepted predecessor instance.
- [`contracts/diagnostic-registry.v1.9.0.json`](../contracts/diagnostic-registry.v1.9.0.json) — an earlier accepted predecessor instance.

Versions are independent of the product release, the Model/IR/protocol
contract versions, the lock wire, and the resolver algorithm version.

## The diagnostic item

Field order is normative: `schema_version`, `registry_version`, `id`, `code`,
`severity`, `category`, `message_id`, `message`, optional `symbol`, optional
`source`, then the always-present `data`, `related_locations`, `causes`,
`fixes`, and `metadata`.

- `id` is the stable dotted rule id (`loader.json-parse`,
  `versioning.unsupported-version`). `code` is the immutable second label
  `LEK-SUBSYSTEM-NNN` (`^LEK-[A-Z][A-Z0-9]{1,11}-[0-9]{3}$`). Neither is
  locale-dependent or computed from sort order; retired codes are never
  reassigned.
- `message_id` equals the rule id in v1. `message` is the registry's
  deterministic default English text: JSON output never changes with the
  host locale.
- `severity` is `info`, `warning`, or `error`; `category` is exactly one of
  `model`, `semantic`, `compatibility`, `adapter`, `infrastructure`,
  `security`, registered per rule and never inferred from the prefix.
- `source` carries a project-relative POSIX logical path and/or a nested
  `range` with 0-based half-open byte offsets and 1-based line/column
  Unicode-scalar positions. Absolute, drive, UNC, backslash, URI, and
  traversal spellings are unrepresentable.
- `data` is closed by the registry entry: every key is declared with one
  type (`token`, `count`, `flag`, `list`, `records`, `record`), at most 16
  fields, tokens at most 256 bytes, lists at most 64 members.
- `related_locations` (at most 32) are a sorted set; `causes` (at most 8)
  keep the immediate-to-root order; `fixes` (at most 16) are inert
  suggestions with applicability `safe`, `unsafe`, or `breaking` — `safe`
  means eligible to offer, never permission to mutate. Execution belongs to
  #73/#96.
- `metadata` is provider namespacing: versioned reverse-DNS keys whose
  values carry a bounded `original_code` only. Provider raw messages,
  stacks, stdout/stderr, paths, argv, and URLs are unrepresentable.

## Registry and identity

The embedded registry is parsed and validated once per process and is the
only source of rule identity: code, category, default severity, allowed
statuses, default message, location requirement (`none`/`path`/`span`), and
closed data fields. Producers construct diagnostics only through the
registry-backed constructor; a rule the registry does not know fails closed
into the single `diagnostics.registry-invalid` invariant diagnostic. Code
prefixes are allocated by owner/subsystem, not by category:

```text
STR structure path/layout policy   LOAD loader pipeline        IR typed IR
VER versioning/migrations          LOCK committed lock         CLI usage
DIAG core capability surface       ADP adapter/provider seam
```

## Envelope integration

`DomainResult` alone owns the status, the protocol stream, and the exit:
`valid` 0/stdout, `invalid` 1/stderr, `denied` 3/stdout, `unsupported`
4/stdout, `unavailable` 4/stdout, `unsupported-version` 5/stderr. Severity
and category never compute an exit. Failure envelopes carry the
authoritative `diagnostics` array plus the derived `reasonCodes` (unique ids
in normalized order); successes omit both while empty and keep their
accepted payload bytes. Human output renders one item per line:
`status severity [LEK-CODE] id path:start-line:start-column: message`, with
related locations, causes, and fixes as fixed-indent children. No ANSI.

Deterministic normalization: related locations sort by relation, logical
path, and span bytes; fixes by id, applicability rank, then target; data and
metadata keys are byte-sorted; exact machine duplicates collapse (message
text never affects identity); the total order is global-before-located,
then path, span bytes, line/column, code, id, symbol, severity rank, and
canonical payload bytes. A set over the 256-diagnostic bound is an
invariant failure, never a silent truncation.

## Privacy and localization

A diagnostic carries only logical project-relative paths, bounded typed
data, and registry-approved text. Raw operating-system messages, absolute
paths, provider output, argv, URLs, credentials, and timestamps never
enter. A diagnostic is a safe structured payload candidate — not durable
evidence; provenance/revision wrapping and export enforcement stay with
their owning issues. Machine output is stable English from the registry; a
human catalog may be injected explicitly later, but the v1 CLI never reads
the ambient locale.

## SARIF, adapters, and evidence

SARIF 2.1.0 export, report files, and CI annotations belong to #103. The
adapter process wire, handshake, and transport belong to #27; the
executable reference adapter and conformance suite belong to #31; #11
ships only the in-process, hermetic provider normalization. Durable
evidence wrapping belongs to #22/#35/#103 and privacy enforcement to #119.

## Allocated codes

Every active rule with its immutable code is listed in the embedded
registry; adding an entry is a deliberate, reviewed registry change and a
golden update never renumbers existing codes.

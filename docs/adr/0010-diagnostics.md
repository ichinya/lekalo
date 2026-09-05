# ADR-0010: The stable machine-readable diagnostic contract

Date: 2026-09-04
Status: accepted for issue #11

Custody: issue #15 now carries the **prospective product candidate 0.1.21**
in every accepted path (issue #23 published product 0.1.20 (annotated tag
`v0.1.20` on `eef1863`); issue #22 published product 0.1.19 (annotated
tag `v0.1.19` on `31468e9`); issue #14 published product 0.1.12 at
`81666da`; issue #13 published product 0.1.11 at `007c01d`; issue #12
published product 0.1.10 at `fdfbcb5`); this issue carried prospective
product 0.1.9 in
every accepted path (workspace `Cargo.toml`, both `lekalo` packages in
`Cargo.lock`, the `--version` behavior and its pinning tests, `README.md`,
`docs/cli.md`); issue #10 published product 0.1.8 (annotated tag on
`8ddbbf0`). The diagnostic schema version (`lekalo/diagnostic/v1.0.0`) and
the diagnostic registry version (`1.0.0`) are independent of the product
release, of the Model/IR/protocol contract versions, and of the lock wire by
design.

## Context

Issues #3–#10 shipped the result envelope, the structure rules, semantic
IDs, the loader, the typed IR, versioning, and the committed lock. Every
surface produced typed failures, but the wire had no stable severity,
category, message, related-location, cause, or fix shape, and reason codes
carried no shared classification. Issue #11 freezes that contract without
revising any published producer logic: the closed wire schema and registry
live in `contracts/`, the typed API and normalization live in
`lekalo_core::diagnostics`, and every command projects human and JSON
output through the one `DomainResult`.

## The eight recorded owner decisions

1. **Discriminator and ranges.** The wire uses the exact discriminator
   `lekalo/diagnostic/v1.0.0` with snake_case item fields and the nested
   #7/#8 start/end range shape; the issue's illustrative snippet and its
   `start_line` shorthand are non-normative.
2. **Identity.** The dotted rule id stays the primary identity and each
   rule receives an immutable `LEK-SUBSYSTEM-NNN` code
   (`^LEK-[A-Z][A-Z0-9]{1,11}-[0-9]{3}$`). The issue's `LEK-TYPE-014`
   sample is non-normative; no reserved slot is frozen for it.
3. **reasonCodes compatibility.** `reasonCodes` is retained as the derived
   array of unique diagnostic ids in normalized order; removal requires a
   separately versioned envelope major/migration.
4. **Localization.** JSON and default messages are stable English from the
   registry; human localization is explicit-catalog-only and the v1 CLI
   never reads the ambient locale.
5. **Inert fixes.** Suggested fixes are registered, inert advice with
   `safe`/`unsafe`/`breaking` applicability; no apply API, command, or
   digest/CAS-bound edit contract ships here (#73/#96 own execution).
6. **Scope splits.** SARIF/report persistence belongs to #103, the process
   fake adapter to #27/#31, and evidence/export enforcement to #119; #11
   ships only the in-process provider normalization and proves its payload
   omits raw/private runtime data.
7. **Independent registry.** Diagnostic schema and registry versions are
   their own contracts (`DiagnosticSchemaVersion`/
   `DiagnosticRegistry`); only the strict SemVer mechanics of #9 are
   reused, and the Model/IR/protocol registry is not widened.
8. **Bounded provider metadata.** Defensive limits are fixed (256
   diagnostics/result, 32 related, 8 causes, 16 fixes, 8 provider
   namespaces, 16 data fields, 64 list members, 128-byte original codes,
   256-byte tokens) and provider metadata is `original_code`-only;
   arbitrary metadata needs its own namespaced schema plus #119 review.

## Decision highlights

### Closed wire or refusal

Every object is closed (`additionalProperties: false`, and
`unevaluatedProperties: false` at composed boundaries). Core diagnostics
are `Serialize` without a general-purpose `Deserialize`; untrusted
providers propose only the bounded `ProviderDiagnosticWire`, whose
unknown fields, over-limit values, malformed namespaces, and unsafe paths
fail closed into one core-owned `adapter.diagnostic-invalid`
infrastructure diagnostic — never truncation by arrival order. Unknown
schema or registry discriminators fail closed before item contents are
trusted.

### Registry custody

The embedded registry is validated once per process (sorted unique ids,
unique codes, `message_id == id`, closed data fields) and is the only
source of rule identity. Adding a rule or fix without a wire-shape change
is a registry minor; a non-semantic text correction is a registry patch
and remains attributable through `registry_version`; changing an existing
identity/code/category/data/status meaning in place is forbidden. Retired
entries stay as tombstones and are never reassigned. `LEK-ADP` is
allocated from now for the in-process provider seam; process-transport
adapters (#27) extend it.

### Envelope migration

`DomainResult` gains the `unavailable` (exit 4, stdout) and
`unsupported-version` (exit 5, stderr) status classes that the accepted
#7/#9/#10 surfaces already used, and becomes the single envelope every
command renders: the CLI has exactly one emit path, failures carry the
authoritative `diagnostics` plus derived `reasonCodes`, and successes
keep their accepted payload bytes with both fields omitted while empty.
Human output changes to one diagnostic per line
(`status severity [LEK-CODE] id [path:line:column]: message`); the old
one-line comma-joined code list is superseded. Load-failure envelopes
also normalize to one final LF.

## Consequences

- Every emitted core rule is registered, parity-tested, and stable across
  locales; consumers can classify by code/category/severity without
  parsing prose.
- The #10 lock seam, the #9 migration seam, and future #12 rules map onto
  the same registry without schema changes; the registry minor increments
  as rules arrive.
- SARIF, report files, adapter transport, executable fixes, and durable
  evidence remain with #103/#27/#31/#73/#96/#119 and are intentionally
  absent here.

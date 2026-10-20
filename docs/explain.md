# Diagnostic explanations

Status: **Implemented** in product 0.8.0 (issue #96). Owner: result/reporting maintainers.

`lekalo explain` gives humans and AI a stable explanation of every diagnostic
code: what the rule checks, why it matters, common root causes, fix options
with closed risk classes, before/after examples, related rules, profile and
severity behavior, target notes, versioned doc links and the waiver policy.
The explanation lives in the embedded
[explain-catalog](../contracts/explain-catalog.v0.8.0.json), versioned
together with the [diagnostic registry](diagnostics.md) union it explains —
one stable entry per registered code, resolved completely offline.

## Command surface

```sh
lekalo explain LEK-HIDDEN-009 --json
lekalo explain hidden.observer-write
lekalo explain --diagnostic-file report.json --json
lekalo explain --diagnostic-file report.json --provider-catalog adapter-explanations.json --json
lekalo explain LEK-HIDDEN-009 --locale ru
```

Selectors are diagnostic codes (`LEK-HIDDEN-009`) or dotted rule ids
(`hidden.observer-write`); up to 64 distinct selectors resolve per
invocation. The command never loads a project and never writes.

`--locale en|ru` selects human headings. The versioned catalog prose stays
English; codes, risk classes and the complete JSON bytes are locale-independent.
Human output includes causes, fix classes (including visible `BREAKING`),
before/after examples and any available provider references.

`--provider-catalog` accepts the closed, bounded
[offline provider catalog](../contracts/explain-provider-catalog.schema.v0.8.0.json):
`schemaVersion: lekalo/explain-provider-catalog/v0.8.0`,
`identity: dev.lekalo.explain-provider-catalog@0.8.0`, and `entries` containing
`namespace`, `originalCode` and the exact `explanationRef` supplied by the
adapter owner. Namespaces and original codes come from each diagnostic's
frozen `metadata` map. All provider identities for a code survive deduplication
in `source.providerExplanations`; a missing offline catalog entry keeps the
identity and an explicit null reference. A provider pointer never replaces a
core explanation or changes a recipe's applicability. Duplicate entries,
unknown versions and malformed metadata refuse; references are never fetched.

## The stable machine contract

`--json` emits the closed
[explain-result](../contracts/explain-result.schema.v0.8.0.json) document:
`state: "explained"` with one entry per requested code, or the distinct
refusal below. Human output may vary by locale; the code and the structured
document do not. Every entry carries:

- **Stable identity**: code, dotted rule id, title, category (both pinned by
  the registry).
- **Checks and rationale**: what the rule verifies and why it matters for
  correctness, portability, or AI readability.
- **Root causes**: the common ways this diagnostic comes to exist.
- **Fix options**: each with the closed risk class (`safe`, `unsafe`,
  `breaking`, `manual-only`, mirroring the versioned
  [fix recipes](fixes.md)) and, where one exists, the `recipeId` of the
  executable recipe. A recipe link is a pointer, never a claim that the
  generic fix applies to the current source; `lekalo fix` still owns
  admission, preview and the exact plan confirmation. Breaking alternatives
  are visibly marked `breaking` in the class field.
- **Examples**: at least one before/after pair.
- **Related codes** and **severity profile**: default severity, allowed
  envelope statuses, whether the code blocks (`invalid` is possible), and
  the closed waiver policy.
- **Target notes** and **versioned doc links**: target-specific behavior is
  separated from target-neutral semantics; links pin their docs version.
- **Source provenance**: `source.provider: "core"`; adapters may publish
  namespaced explanations with their own provider identities. With
  `--diagnostic-file`, the structural context of the originating report row
  (index, location, message, severity) rides `source.report`, separate from
  the stable entry. The explanation never claims a fix applies to the
  current source, and report context is never merged into rule semantics.

## Unknown codes are distinct outcomes

An unregistered code such as the illustrative `LEK-TYPE-014` returns the
typed `explain.code-unexplained` diagnostic (exit 4, `unsupported`) with
`state: "unexplained"` — never an empty success and never a guessed
explanation. Malformed `--diagnostic-file` inputs refuse with
`explain.report-invalid` (exit 1); a drifted embedded catalog would refuse
with `explain.catalog-invalid` (exit 4, a developer fault).

## Catalog governance

The catalog is generated data (`scripts/gen-explain-catalog.mjs --check` in
CI): it pins the registry identity and digest it explains, carries exactly
one entry per registered code, and every core blocking diagnostic has an
explanation with an example. The registry-union gate admits the additive
`explain.*` diagnostic family (`LEK-EXPLAIN-001..003`); a new stable
diagnostic without a catalog entry fails the explain contract gate, so CI
fails when documentation goes missing.

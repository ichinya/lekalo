# Module public API and dependency boundaries (issue #113)

`module api` validates the explicit public boundary of a versioned module,
exports its standalone public contract, inventories its consumers, pins and
checks consumer locks, diffs two public revisions with exact affected
consumers, and plans a bounded consumer capsule from the public bundle alone.
It is read-only: no file, lock, report, baseline, or cache writes, no adapter
execution, no network, and no Git state change. Capture stdout explicitly when
a reviewed artifact is needed.

## Attachment

The boundary is explicit. An attachment (`module-api.schema.v0.8.0.json`)
binds one project and keys policy by exact semantic module id: an export
allowlist (project visibility alone never exports), optional independent
`apiVersion` (`null` selects digest-only), consumer declarations with the
closed `exact-digest` / `exact-version` / `version-range` policy vocabulary,
service ports, capability obligations, shared-aggregate write grants, and
deprecation windows. Attachment imports must agree with the declared Model
imports; the attachment adds policy and use detail, never hidden imports.

```sh
lekalo --json --no-cache module api check --project DIR --api PATH [--module ID] [--strict]
lekalo --json --no-cache module api export tasks --project DIR --api PATH
lekalo --json --no-cache module api consumers tasks --project DIR --api PATH
lekalo --json --no-cache module api lock --project DIR --api PATH --module tasks --consumer planner
lekalo --json --no-cache module api lock --project DIR --api PATH --module tasks --consumer planner --check --lock PATH
lekalo --json module api diff BASE_PUBLIC CANDIDATE_PUBLIC --consumers-before SNAP --consumers-after SNAP [--strict]
lekalo --json module api context PUBLIC --consumer planner --uses a.b,c.d --budget 4000
lekalo --json validate --project DIR --strict --module-api PATH [--module-api-lock PATH]
```

## Boundary rules

- Cross-module references resolve against the provider's export allowlist; a
  project-visible-but-unexported symbol is `LEK-MODULE-003` (direct
  module-private references keep the existing `LEK-SEM-020` rule).
- Every resolved cross-module dependency needs its declared import
  (`LEK-MODULE-004`); unused declared imports stay visible, never an error.
- The exported public signature closure (field/return/payload types only) may
  reach only exported types (`LEK-MODULE-005`) and portable types
  (`LEK-MODULE-006`); query reads and command effect obligations are not
  public shapes.
- Actual declared cross-owner writes need an exact shared-aggregate grant
  with operation and field scope (`LEK-MODULE-007`); an entity-wide write is
  never covered by a field-only grant, and neither an import nor a shared
  transaction is a grant.
- Declared import cycles fail at load with the existing `LEK-LOAD-008` rule
  before any graph exists; the boundary SCC projection (`LEK-MODULE-008`)
  covers projected public dependency cycles.

Status policy: malformed input is invalid (exit 1); unknown family versions
are unsupported-version (exit 5, stderr); exact strict boundary violations
are invalid (exit 1); strict compatibility/deprecation/lock policy failures
deny (exit 3) with the full report retained; required missing evidence is
unavailable (exit 4). Advisory default keeps exit 0 with an explicit
`assessment` field. `module api export` and `lock` refuse an uncertified
closure regardless of mode.

## Public bundles, consumers, locks

`export` materializes the standalone `module-public-contract` bundle:
exported signatures with their portable type closure, typed public errors from
the #62 registry (public payload fields only), observable effect obligations,
ports, capabilities, deprecation metadata, exact public dependency pins, and
a canonical SHA-256 digest over public semantics only (layout, descriptions,
spans, and physical locations are excluded; the digest member is excluded
from its own domain). The same digest backs the consumer snapshot
(`module-api-consumers`), the consumer lock (`module-api-lock`), and the
public-only context capsule (`module-consumer-context`), so an in-process
consumer and an extracted-service port conform to the identical portable
contract.

The diff classifies changes (additive, behavioral, source-breaking,
wire-breaking, policy) with exact reasons, joins the union before/after
consumer inventory, partitions affected/compatible/unaffected consumers, and
evaluates the independent version policy: stable majors need a major bump for
breaking changes, pre-1.0 modules pin minor-breaking behavior explicitly, and
digest-only modules never claim SemVer compatibility. Coverage stays honest:
unknown external callers are reported, never merged into a purported exact
total.

The consumer context (`module api context`) reads the bundle alone — the
provider checkout may be absent. Required facts (inputs, results, errors,
effect obligations, capability/version/deprecation obligations) are protected
against budget drops: an over-budget capsule keeps `fits:false`, the minimum
required cost, and the omission reasons, with the pinned offline estimator
(`module-context-estimator`, `public-facts-chars4/1`).

## Diagnostics and registry

The `module-api.*` rules are `LEK-MODULE-001`–`014` in the
`diagnostic-registry.v0.8.0.json` union successor (the accepted 0.6.5 union
plus this family, additive end to end). The boundary profiles
(`validation-profile.{default,strict}.v0.8.0.json`) pin that registry and
differ only in the portability override, exactly like their 0.6.4
predecessors. All new contract families take the 0.8.0 product generation;
frozen contracts keep their exact bytes.

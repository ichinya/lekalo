# ADR-0011: Semantic validation over the Lekalo IR

Date: 2026-09-04
Status: accepted for issue #12

Custody: issue #15 now carries the **prospective product candidate 0.1.21**
in every accepted path; issue #23 published product 0.1.20 (annotated tag
`v0.1.20` on `eef1863`); issue #22 published product 0.1.19 (annotated
tag `v0.1.19` on `31468e9`); issue #14 published product 0.1.12
(annotated tag `v0.1.12` on `81666da`); this issue published product
0.1.10 (annotated
tag `v0.1.10` on `fdfbcb5`, followed by the CI-parity fix `47b2ec8`),
with the same accepted custody paths as every issue since #3. The validation-profile contract version
(`lekalo/validation-profile/v1.0.0`) and the diagnostic registry increment
(`1.0.0` → `1.1.0`) are independent of the product release, of the
Model/IR/protocol contract versions, and of the lock wire by design.

## Context

Issues #3–#11 shipped the result envelope, structure rules, semantic IDs,
the loader, the typed IR, versioning, the committed lock, and the stable
diagnostic contract. Every layer produced typed failures, but nothing
checked meaning: references expanded by the loader were never resolved
against the compiled aggregate, no kind expectation existed at the IR
layer, and visibility, output leakage, and recursion were unchecked.
Issue #12 adds that semantic layer without revising any published producer:
the diagnostic registry gains a successor per its own governance, the
closed profile contract and its two built-ins live in `contracts/`, and the
pure validator lives in `lekalo_core::validator` behind `lekalo validate`.

## The recorded owner decisions

1. **Code and rule prefixes.** Rule ids use the `semantic.*` family;
   immutable codes use `LEK-SEM-NNN`. Invocation rules use `validate.*`
   with `LEK-VAL-NNN`. The registry successor is a minor increment
   (`1.0.0` → `1.1.0`, wire shape unchanged, schema discriminator
   `lekalo/diagnostic-registry/v1.0.0` retained).
2. **Reference-kind matrix.** The expectations mirror the published model
   contract exactly (`model.ref-kind-mismatch`): type leaves resolve to
   `enum`/`entity`/`scalar`/`value-object`; `command.effects` to `effect`;
   `query.reads` to `entity`; `policy.applies_to` to `command` only;
   `effect.entity` to `entity`; `effect.emits` to `event`;
   `endpoint.invokes` to `command`/`query`; `scenario.covers` is
   existence-only. Aligning with the published checker keeps every
   model-valid project valid and avoids double-jeopardy diagnostics.
3. **Query write effects.** The accepted `QueryDef` cannot represent a
   write effect (`reads`/`returns` only), so the P0 check is an invariant
   of the closed type system, pinned by tests, not a runtime rule. A Model
   successor adding a write surface must revisit this.
4. **Private-field/public-output semantics.** The IR exposes
   definition-level `visibility` only. P0 leakage is therefore two
   definition-level security rules — cross-module references into a
   module-private symbol, and project-visible queries returning a
   module-private named type. Field-level privacy classification stays
   with #25/#87/#119.
5. **Target bindings.** `target-binding.target` names a target config with
   no in-Model registry (`lekalo/targets/` is outside the IR), and the
   model layer already owns `model.target-unresolved`. #12 adds no target
   rule; symbol-binding semantics belong to the adapters issue (#27).
6. **Duplicate declarations.** The loader owns duplicate/conflicting
   declarations and the closed IR types make duplicate identities
   unrepresentable through the accepted producer, so no runtime rule
   exists; the aggregate invariant remains documented here.
7. **Profiles.** One closed, independently versioned profile contract with
   `profile_id`, `version`, `diagnostic_registry_version`, a closed
   `scope`, and a sorted, duplicate-rejecting `rules` array of
   `{id, enabled, severity_override?}`. Overrides are legal only for
   non-error registry defaults and only downward. Built-ins are embedded
   from `contracts/` and parsed by the same closed parser; `default`
   downgrades `semantic.portable-target-reference` to info, `strict` keeps
   the warning. Persistent profile-file custody is not assigned to #12, so
   `--profile <path>` is deferred.
8. **Module scoping.** `--module` validates the whole project and filters
   only the report: module-owned diagnostics at any severity plus every
   error anywhere, never hiding mandatory cross-module failures; an
   unknown scope fails closed with `validate.module-unresolved`.

## Consequences

- The registry successor adds 20 `semantic.*` and 3 `validate.*` rules;
  retired codes are never reassigned.
- Every emitted diagnostic carries an exact source span derived from the
  accepted #8 source map after grammar validation; the bounded-token
  discipline of #11 applies to every echo (`validate.span-unavailable`
  covers an impossible lookup miss as a fail-closed invariant).
- Determinism is fixed by phase order (resolution → semantic →
  portability), registry-ordered rules, canonical definition order, fixed
  reference-site order, exact deduplication, and the #11 total sort.
- `lekalo validate` composes accepted load → IR → semantic validation and
  renders both projections through the one `DomainResult`; the JSON
  success envelope embeds the fixed-order `validation` report object.
- No targets/adapters (#27+), no policy/authorization semantics (#25), no
  autofix (#74), and no reporting (#103) behavior is included.

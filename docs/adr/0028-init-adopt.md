# ADR-0028: Adoption (`init --adopt`) and the bootstrap configuration seam

Status: accepted for issue #38. Extends the #4 canonical structure
contract and the #11 diagnostic contract; deliberately excludes greenfield
`init` and module authoring.

## Context

`lekalo init --adopt` must connect Lekalo to an existing repository
without moving sources, generating extra code, or demanding a full model
on day one. The seam matters twice: greenfield `init` (a later issue)
must share the bootstrap configuration semantics, and adoption must
honestly represent what it knows — detection is evidence, not framework
semantics.

## Decisions

1. **The skeleton is exactly the #4 requirement.** `lekalo/project.yaml`
   alone is a complete legal project (zero modules). Adoption writes it
   with deterministic compact JSON, one trailing LF, fixed key order.
   `lekalo/targets/<id>.yaml` is written only for an explicit `--target`
   and records an explicit `--profile` (requires `--target`, the #28
   token grammar) verbatim; target documents stay opaque, and a profile
   is a persisted selection — never executed or checked against an
   adapter, because adoption runs nothing. No lock (`lekalo lock` owns
   it), no `.lekalo/**` runtime writes, no `.gitignore` edits.
2. **Observed modules are receipt data, never canonical documents.** The
   Model contract has no module-mode field and the registry/policy forbid
   unpublished fields, so `mode: "observed"` lives in the adoption
   receipt next to evidence and confidence. Emitting a `mode` field into
   `project.yaml` would be silently ignored by every loader — refused on
   principle.
3. **No-overwrite is enforced by the OS, not by preflight only.** Planned
   writes use atomic `create_new`; byte-identical paths are skipped
   (idempotence); any other existing path denies the adoption
   (`init.adopt-conflict`, exit 3) before the first write. Created paths
   are journaled; failures roll back in reverse; an incomplete rollback
   is an explicit `init.adopt-recovery-required`, never a silent pass.
4. **The post-write gate is the normal load/validate path.** Adoption
   proves its output by running the loader and the default validation
   profile in process; a gate failure rolls everything back first, then
   passes the gate's exact failure envelope through.
5. **Ambiguity demands explicit resolution.** Multiple workspace roots on
   the ancestor chain refuse with `init.adopt-ambiguous-root` (exit 3)
   until `--project` names the root. `--project-id` is the same kind of
   explicit resolution for project ids that sanitize to ambiguity; the
   derivation is always recorded (`projectIdSource`).
6. **Detection never executes.** Gates are command proposals with
   evidence; adapters are `PATH` existence probes (`lekalo-target-<id>`);
   the walk is bounded, no-follow, and skips the externally owned
   OpenSpec/AI Factory/HLV homes entirely.
7. **Bootstrap configuration seam (for greenfield `init`).** The
   skeleton writer, the create-new journal with rollback, the
   preflight/disposition vocabulary (`create`/`already-present`/
   `conflict`), and the closed `init.*` rules are core services; the
   later issue reuses them for its template writes instead of inventing a
   second writer.

## Consequences

- Adoption failures have registered identities: `init.adopt-conflict`
  (LEK-INIT-001), `init.adopt-ambiguous-root` (LEK-INIT-002),
  `init.adopt-id-required` (LEK-INIT-003), `init.adopt-write-failed`
  (LEK-INIT-004), `init.adopt-recovery-required` (LEK-INIT-005) in
  diagnostic registry 1.13.0.
- Idempotence is byte-compare based, so the skeleton content must stay
  deterministic; changing it requires a deliberate migration decision,
  not a re-run.
- Observed modules waiting for canonicalization are visible in the
  receipt, which is the audit trail for the later authoring step.

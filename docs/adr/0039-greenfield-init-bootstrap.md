# ADR-0039: greenfield init bootstrap

Status: accepted (issue #97). Builds on ADR-0003 (canonical structure),
ADR-0004/0005 (Model and semantic IDs), ADR-0011 (diagnostics),
ADR-0021 (artifact ownership), ADR-0028 (init adoption), ADR-0029
(target profiles), and ADR-0032 (doctor readiness). Consumers: #109
(reference projects), #105 (docs/quickstart), #114 (greenfield pilot).

## Context

Issue #38 connected Lekalo to existing repositories
(`init --adopt`, ADR-0028) and deliberately left `init` without
`--adopt` as the stable usage failure. Issue #97 asks for the
greenfield counterpart: a minimal new Lekalo project — the project
document, a first empty module, the optional target selection, the
`.gitignore` addition for the derived `.lekalo/` tree, and optional
editor/schema hints — without generating an application, imposing a
target framework before an explicit choice, installing anything, or
creating the lock before explicit resolution.

## Decision

1. **One more entry point on the #38 seam, not a new writer.**
   Greenfield `init` and `module new` reuse the adoption plan surface
   verbatim: the closed id/target/profile grammars, the atomic
   no-clobber `create_new` writer, the exact mutation journal, the
   identical-bytes skip, the whole-run conflict denial, and the
   in-process load-and-validate gate. "Greenfield and `init --adopt`
   share common config semantics" is satisfied by shared code and
   shared bytes: the target document is byte-identical to adoption's.

2. **Bootstrap root is where the user stands.** No ancestor walk for
   workspace or manifest evidence — that is adoption's discovery. An
   explicit `--project` selector (or `LEKALO_PROJECT`) wins; otherwise
   the invocation directory. The directory must exist; bootstrap never
   creates project roots. The receipt records the closed `rootBasis`.

3. **`.gitignore` is a merge, everything else is no-clobber.** The
   ignore file is user-owned, so the managed `/.lekalo/` line is
   appended to existing content (one newline separator when needed)
   instead of denying a tree that already has an ignore file. Both
   line spellings (`/.lekalo/`, `.lekalo/`) count as present, so a
   user-authored ignore is never duplicated. The append is journaled
   with the exact original length; rollback is truncation, never a
   rewrite. The `#4` prose "this implementation never edits it"
   governs the structure checker, not an explicit bootstrap command
   whose issue lists the addition as an artifact.

4. **The first module is an empty module, always created.** The
   bootstrap artifact list names the first empty/example module, so
   greenfield writes `lekalo/modules/<id>/module.yaml` (module
   manifest only) with `--module` (default `app`).
   `lekalo module new ID` creates later modules through the same
   seam and the normal marker root discovery.

5. **Frontends, not formats.** `--frontend yaml|json` selects the
   document syntax inside the fixed `.yaml` homes: human-friendly
   block YAML (default) or the compact JSON spelling adoption writes.
   Both parse through the same strict frontends; the choice is
   recorded in the receipt, never persisted as an unpublished field.

6. **Editor hints are opt-in.** `--editor-hints` writes a fixed
   `.vscode/settings.json` mapping the model homes to the published
   Model 1.0.0 schema `$id`. A differing pre-existing settings file is
   an ordinary conflict (re-run without the flag); an opt-in
   convenience never silently overrides user editor configuration.

7. **Wizard decisions without contract homes are not persisted.**
   Ownership (greenfield is model-owned by construction), integration
   evidence (enters via `doctor --trace`), strictness (per
   `validate --strict`), and adapter availability (only through the
   explicit `lekalo lock -- PROGRAM` handshake) stay per-invocation
   choices of the commands that own them. The Model contract stays
   closed; bootstrap emits no unpublished field.

8. **Template versions are the receipt's custody.** The in-code
   templates version with the product, so each planned artifact
   carries a `lekalo/init/<artifact>@<product version>` record in the
   receipt; every generated document separately carries its own
   `schema_version`, and each `writes[]` entry carries the sha256 of
   the exact bytes.

9. **Diagnostics: a parallel spelling, not a reuse.** Adoption's
   `init.adopt-*` messages name adoption; reusing them for greenfield
   would mislead. The registry publishes 1.24.0 (reserved for this
   issue) with the mode-neutral `init.bootstrap-*` family
   (`LEK-INIT-006..009`: conflict, id-required, write-failed,
   recovery-required), used by both greenfield `init` and
   `module new`. 1.23.0 stays reserved by its parallel owner. The
   embedded validation profiles' registry pin advances to 1.24.0 with
   no profile-content change.

10. **The lock stays out of bootstrap.** `lekalo.lock` appears only
    after explicit resolution (`lekalo lock`, issue #10/#91); bootstrap
    never probes, executes, or installs an adapter, so target
    selection resolves only installed/explicit adapters by
    construction.

## Consequences

- A greenfield re-run after an adoption (or the reverse) can conflict
  on the project document's mode-specific description; that denial is
  the honest no-silent-overwrite answer, not a bug.
- Windows/Unix append rollback is exact (truncate to the journaled
  length); a concurrently replaced shorter file fails closed instead
  of being zero-extended.
- `module new` outside a project pays a write-then-gate-then-rollback
  cycle before the `structure.root-not-found` failure; the failure
  surface stays identical to every other project-rooted command and
  nothing is left behind.

# Work with Lekalo in this project

Use the installed Lekalo CLI to read and validate the project's declared model.
Treat source observations as evidence, not permission to rewrite project intent.
Keep this instruction subordinate to the user's authorization and project policy.

## Start from the current project

1. Read applicable project instructions once; reuse instructions already in context.
2. Find `lekalo/project.yaml`; establish the project identity and exact contract pin.
3. Check `lekalo --version` once; use command help when its flags are unknown.
4. Select the affected semantic IDs and their current evidence before broad reads.
5. Read module declarations when editing them or resolving a specific evidence gap.
6. Load full canonical IR only when the task needs it; prefer a bounded projection.
7. Run strict validation for Model work; repeat when its inputs or required gates change.
8. Keep failed, unsupported and unavailable checks visible; do not repeat unchanged failures.

## Recognize the layout

- `lekalo/project.yaml` owns the declared project and its contract pin.
- `lekalo/modules/<module>/module.yaml` declares a semantic module.
- Other files under that module declare entities, commands, queries and policies.
- Module files may also declare events, scenarios and explicit source bindings.
- `lekalo/targets/` contains declared target profiles and configuration.
- `lekalo.lock` pins the resolved contract/tool inputs; inspect changes to it.
- `.lekalo/` contains derived runtime state, caches and local evidence.
- Keep `.lekalo/` out of source control according to the project policy.
- `.agents/lekalo.md` is the managed copy of this instruction.
- `openspec/` contains requirement documents when OpenSpec is in use.
- `.ai-factory/` contains workflow integration state when AI Factory is in use.
- `.hlv/` contains HLV integration material when HLV is in use.
- Do not create an absent integration home merely because it appears here.

## Read before editing

Use semantic IDs from the loaded model rather than guessing IDs from filenames.
Start with `lekalo context ID --budget 1600 --project .` when semantic context is useful.
Use inspect or impact for details or obligations the capsule does not supply.
Use status when project state matters; do not run every projection as a checklist.
Prefer compact human output for agent reading and JSON for programmatic consumers.
Reuse current results for the same inputs instead of dumping the Model or full IR.
Read diagnostics by their registered IDs and actual command exit status.
Do not turn missing evidence into a successful check.
Do not treat an empty result as proof that no dependency or obligation exists.

## Read source once per version and needed range

Keep a working record of paths, source digests and ranges already present in context.
Use names, mappings and search locations to select the necessary implementation.
Read uncovered ranges; do not reprint an unchanged file already available to you.
If an editor requires its own Read, use that as the first read of an intended edit.
Avoid reading through a helper and then repeating the same content for that editor.
Respect the editor's requirements; choose a compatible edit method when available.
Changed bytes, missing ranges, truncated output and lost context justify another read.
State the reason and request only the needed range; a new digest invalidates old evidence.
Verify changes with a scoped diff, fingerprints and native checks rather than routine dumps.
An incomplete contract or absent semantic/native join still requires relevant source.
Native names and signature digests alone do not establish implementation behavior.

## Keep authority boundaries explicit

Lekalo owns declared semantics, model structure, bindings and derived projections.
OpenSpec owns requirements and their lifecycle; a model edit does not approve them.
AI Factory owns workflow state; a model edit does not complete a workflow task.
HLV owns validation results; semantic validation does not invent runtime outcomes.
Read the project's authority and privacy policy before crossing those boundaries.
Use existing integration documents as references rather than duplicating them.
Do not overwrite one authority's records with another authority's interpretation.
Do not claim that a passing semantic check proves application behavior.
Do not claim deployment, production acceptance or user approval from local tests.

## Edit the intended source

Change authored declarations in their canonical `lekalo/` homes when authorized.
Change application source only within the user's requested implementation scope.
Preserve unrelated edits and preserve project-owned configuration.
Keep uncertain source mappings observed until explicitly confirmed or promoted.
Use explicit confirmation workflows; never auto-promote inferred semantics.
Do not refresh a source fingerprint merely to hide an unresolved semantic change.
Do not hand-edit `.lekalo/` caches, indexes or other derived output.
Do not remove or rewrite Lekalo-managed `.gitignore` entries by hand.
Use the owning CLI operation for managed state and inspect its planned changes.
Keep canonical model paths portable and relative to the selected project.
Refuse traversal, symlink/junction escapes and ambiguous ownership.

## Respect independent versions

The product version and contract versions are independent.
Preserve the project's explicit model, IR, protocol and profile pins.
Do not replace every version string with the installed product version.
Do not silently negotiate away an unsupported required capability.
Treat a migration as a separate explicit operation with its own review.
Use the installed binary's diagnostics and published schema for the exact pin.
Keep unknown, incomplete, unsupported and failed evidence distinct.

## Run only authorized checks

Start with read-only load, inspect, impact, context, status and validation.
Inspect dry-run/planned changes before a command writes managed project state.
Do not run package managers, project scripts or deployments just to discover them.
Native planning is read-only and requires explicit policy/profile/catalog inputs.
Native execution additionally requires the exact externally approved outer digest.
Honor the installed binary's declared platform/runtime support and confinement.
Run confirmed commands through the supported production entrypoint when authorized.
Keep original-source, mutation, exit/outcome and cleanup evidence together.
Do not describe a plan-only refusal or a synthetic harness result as execution.
Do not install dependencies, call paid inference or retry external work implicitly.

## Finish with evidence

Re-run checks appropriate to the changed definitions and their affected scope.
Report the exact commands and meaningful results without exposing credentials.
Separate semantic validation, native tests, CI and production verification.
State what was not run and why when the distinction affects acceptance.
Summarize remaining diagnostics and unresolved behavior rather than hiding them.
Do not commit, push, release, deploy or send messages without authorization.

## Canonical references

- Authority: https://github.com/ichinya/lekalo/blob/main/docs/authority.md
- CLI: https://github.com/ichinya/lekalo/blob/main/docs/cli.md
- Diagnostics: https://github.com/ichinya/lekalo/blob/main/docs/diagnostics.md
- Versions: https://github.com/ichinya/lekalo/blob/main/docs/versioning.md
- Native gates: https://github.com/ichinya/lekalo/blob/main/docs/native-gates.md
- Roadmap: https://github.com/ichinya/lekalo/blob/main/docs/roadmap.md
- Decisions: https://github.com/ichinya/lekalo/tree/main/docs/adr

Prefer the installed version's help and contract pins if newer reference pages differ.

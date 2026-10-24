# Product, contract and migration versions

This page owns the repository's version policy. Feature references specify exact accepted contracts; they do not define separate release-numbering rules.

## Product and contract versions

The product version is `workspace.package.version` in [Cargo.toml](../Cargo.toml), currently `0.8.0`. Each contract has an exact identity and wire spelling, such as Model `0.2.16`, Model `0.8.0` or target protocol `0.3.2`.

When a contract changes, its new version is the product version of the implementing commit. Update the filename, identity/discriminator, embedded constants, consumer pins and digests together. Contracts whose bytes do not change retain their accepted versions. A product release does not rewrite every contract to the product number.

An archival exception permits restoring the exact bytes previously committed at the same contract path, preserving that historical version. [The version checker](../scripts/check-contract-versions.mjs) verifies the path's Git history. Editing a restored artifact is a change, not an archival restore.

Acceptance is exact. A higher filename, matching major version or newer product does not grant compatibility. Use the producing component, admitted manifest and matching schema. The [ownership inventory](documentation-owners.json) identifies the reference for each public contract.

## Source profiles and protocols

The loader admits the explicit Model/IR profiles described in [Model](model.md) and [IR](ir.md). The `0.2.16` profile does not gain successor fields because the CLI is `0.8.0`. Unsupported shapes still refuse; documents must agree with the compilation's source-profile rules.

The adapter bootstrap protocol remains `0.3.2`. Lint (`0.6.4`) and watch-selection (`0.8.0`) seams require explicit negotiation. Protocol support does not establish support for every Model/IR pair. [Target protocol](target-protocol.md) owns that negotiation; [provider discovery](provider-contract.md) owns its separate process contract.

Diagnostic item format, registry, validation profile and result format have separate pins. Producers select an admitted registry rather than the largest version found on disk. [Diagnostics](diagnostics.md) owns result and registry selection.

## Compatibility and migrations

Use `lekalo compatibility` to inspect the binary's registered families. The registry and migration planner decide whether a source version and transition are supported; old filenames and numeric ranges do not.

Migrations are explicit through `lekalo migrate --to model/<version>`. Inspect `--dry-run` before applying a registered transition. Journaling, preimage checks, bounded paths and backup/rollback do not create a missing migration path. Unknown versions and absent paths refuse. There is no automatic old-data or downgrade guarantee; rebuild derived IR from the admitted Model.

When adding a transition, register its source/target versions and add transfer, refusal and recovery tests. Historical transitions are not restored automatically. A compatibility view is separate from each consumer's admission rules.

## Contributor checks

`node scripts/check-contract-versions.mjs` compares the working tree with `HEAD`; `--base REF` selects another base. CI uses its workflow's declared base. This checks product-version naming and exact-byte archival restoration. Schema, generator, digest and runtime gates still verify content and behavior.

[Contributing](contributing.md) owns build/check setup. [Release maintainers](releasing.md) verify tag, Cargo, title and binary parity. Installed users should follow [installation](releases.md).

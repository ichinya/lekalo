# Native lexical and application vectors

`native-lexical-vectors.json` adds 118 cases to the existing 50 fence/section
vectors. It executes original OpenSpec `extractRequirementsSection`,
`parseDeltaSpec` and `buildUpdatedSpec` at commit
`e062b9572be933564ba3899d059377dfa1393e32`. Provenance includes SHA-256 receipts
for the 13-module application closure. Mechanical TypeScript 5.9.3 transpilation
changes syntax only; no replacement native parser or application is used.

SPACE/TAB/NBSP/NEL/FEFF cross accepted H2/H3, operation headings, title edges,
rename/removal directives, section termination, and body tails/interior lines.
Case, exact wrappers, BOM multiplicity, empty headings, Unicode line separators
and rename-plus-modification controls complete the matrix. `unsupported` marks
the explicit fail-closed subset in `docs/requirements.md`, independently of
whether native parsing ignores or accepts a form.

`expectedEntries` comes from original native blocks and plans, with the
documented ECMAScript body canonicalization and bounded public title slugs.
`archiveAccepted` is the actual upstream `buildUpdatedSpec` result, not a
hand-built archive projection. Its extracted catalog is also checked against
the plan projection during generation. `nativeError` records the nine original
application refusals; those cases do not claim successful native archival.
This includes accepted one/two-BOM documents and out-of-section controls.

The Rust CLI regression asserts native catalogs/revisions, pinned fresh gates,
closed trace validation, no partial results for unsupported input, no source
writes, and identical confirmed traces after each supported actual native
rebuild. The runtime OpenSpec package is not a Lekalo dependency. This does
not execute the full OpenSpec archive CLI lifecycle or replace its preflight.

Fresh original sources, generator, full native outputs, actual before/after
CLI streams, snapshots and exact Ajv 8.17.1 export checks are retained in:
`C:/Users/User/orca/artifacts/lekalo-m2-20260908/issue-36-fix-4-evidence/`.
The generator is `generate-boundaries.mjs`; it refuses existing output paths.

# `tests/fixtures/node-typescript-scanner/` — synthetic scanner fixtures

`public-fixture`. Every file here is authored, synthetic fixture
vocabulary created exclusively for the #44 scanner suites. Nothing in
these projects is a claim of conformance with any real framework,
package, or DSL; names such as `router`, `test-dsl`, or `@fixture/*` are
fixture vocabulary only.

## Executions never run here

Every committed `package.json` file carries poison sentinel scripts
(preinstall/postinstall/prepare/test) that fail loudly if ever executed —
and nothing in the kernel, scanner, or suites ever executes them. The fixture
runner executes only the current Node interpreter and the committed
lekalo test binaries. No package manager, no `tsc`, no framework runner,
and no project hook is ever launched, during tests or during scans.

## Layout

| Directory | Purpose |
| --- | --- |
| `esm/` | ESM project: named/default/type exports, export aliases and star re-exports, overloads, merged interface members, class members, static members. |
| `cjs/` | CommonJS project: `export =` / `import equals` and static value exports. |
| `project-references/` | Two-package workspace with a tsconfig project reference and a `paths` mapping; clean source-only state (no dist). |
| `path-aliases/` | `baseUrl`/`paths` aliases resolved through the compiler. |
| `pnpm-workspace/` | Workspace-shaped monorepo (inert `pnpm-workspace.yaml`, `workspace:*` dependency) resolved via explicit source mappings, not a store. |
| `framework-evidence/` | Authored router/test-DSL declarations with static routes, dynamic routes (uncertainty), and static test definitions. |
| `uncertainty/` | `any`/`unknown` surfaces, unresolved imports, computed keys, dynamic calls. |
| `excluded/` | Poison canaries under `vendor/`, `node_modules/`, and `dist/` that must never be read or indexed. |
| `incremental/` | Single subject module for body/signature edit and cold==warm parity probes (edits are applied by tests in disposable copies). |
| `protocol/` | The committed conformance launch profile and protocol vectors. |

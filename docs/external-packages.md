# External package ownership

The host resolves external `lekalo/runtime/v1` packages or installed executables and validates portable contract rules. See [runtime configuration](external-runtimes.md). A new runtime does not require adding its language to the host.

The following implementations are maintained outside this repository:

| Repository | Owns |
| --- | --- |
| [lekalo-adapter-sdk](https://github.com/ichinya/lekalo-adapter-sdk) | Four SDKs, generator, templates, codec corpus and SDK conformance tools |
| [lekalo-typescript](https://github.com/ichinya/lekalo-typescript) | TypeScript compiler client, runtime distribution and runtime tests |
| [lekalo-typescript-hono](https://github.com/ichinya/lekalo-typescript-hono) | Hono analysis, evidence, compatibility vocabulary and fixtures |
| [lekalo-typescript-drizzle](https://github.com/ichinya/lekalo-typescript-drizzle) | Drizzle analysis and declaration-closure tooling |
| [lekalo-typescript-zod](https://github.com/ichinya/lekalo-typescript-zod) | Zod mapping, generation, policy and fixtures |
| [lekalo-postgres](https://github.com/ichinya/lekalo-postgres) | PostgreSQL type policy, capability matrix, DDL renderer and target extension |

No SDK or TypeScript runtime implementation is tracked under `sdk/` or `adapters/node-typescript` in the host. The TypeScript runtime builds its distribution from pinned external npm/Git packages. Hono receives runtime provenance explicitly; Drizzle receives the compiler API through the call context. Their modules do not import private runtime source files. These existing target integrations are not yet portable `lekalo/contract-rules/v1` rule packs; the runtime-v1 migration remains a separate acceptance gate.

Python and FastAPI consume the Rust SDK from its owner repository through an immutable Cargo Git revision. Python also consumes FastAPI from its owner repository by immutable Git revision. They no longer vendor SDK or framework source.

Go, Rust, TypeScript and Python discovery are standalone executables in their language repositories. HLV mapping policy and OpenSpec requirements parsing are standalone executables in `lekalo-hlv` and `lekalo-openspec`. All seven remaining snapshots, including the Python runtime, have been removed from core; there is no `packages/`, `adapters/python/`, or source snapshot sync script. See [service configuration and installation](external-services.md).

The published [PHP types, operations, and routes schemas](https://github.com/ichinya/lekalo-php/tree/f22d578d432a16a256127349486f90333c8a0ddb/contracts) live in `lekalo-php`. The
[`laravel-migration-input.schema.v0.4.0.json`](https://github.com/ichinya/lekalo-php-laravel/blob/10e491e26899acb56d1dfd59bb644924e6b41f0c/contracts/laravel-migration-input.schema.v0.4.0.json) lives in `lekalo-php-laravel`. The development lock selects exact
package revisions for the contract gates.

## Development and integration tests

`external-tools.lock.json` pins integration dependencies. Run `node scripts/external-tool.mjs --provision` before compiling tests that embed external artifact fixtures. Tests that exercise discovery/requirements/trace also need the separate service build and explicit selections described in [external services](external-services.md#development-and-ci). Checkouts are cached outside the host and exposed through ignored `tests/external/<name>` links. There are no package installation or lifecycle hooks in this step. The host production build does not fetch or compile those JavaScript packages.

For a development checkout, `LEKALO_EXTERNAL_TOOLS` may explicitly map package names to absolute local roots. This override is for testing uncommitted package changes; release and CI runs use the committed lock.

The small SDK/framework scripts remaining in `scripts/` delegate to their owning repository. Host/schema fixtures remain in core where they test admission. Public schema snapshots in the SDK have an exact origin/digest receipt; they contain protocol data, not a copy of host implementation.

Legacy storage commands use the external PostgreSQL provider through `LEKALO_STORAGE_PROVIDER`, encoded as a JSON Source, for example `"path:/absolute/path/lekalo-postgres"` or `{"command":["node","/absolute/path/lekalo-postgres/bin/lekalo-postgres.mjs"]}`. The default installed command is `lekalo-postgres`. Missing providers fail closed; there is no embedded DDL fallback. The provider runs in the same empty strict sandbox as an external runtime, with JSON inputs and no database authority.

## Remaining legacy boundaries

The legacy init detector still contains PHP/Composer, Ruby, Java/Gradle and Deno handling and HLV/OpenSpec layout observations. Requirements identifiers and trace system names retain their published closed wire contracts. PostgreSQL-specific migration/drift policy and generic projection tables also remain in the legacy storage pipeline; the extracted renderer and type/matrix answers are external, while existing typed facades preserve callers. Replacing these remaining registrations and moving the parent/child contract graph are still tracked by the open separation issues.

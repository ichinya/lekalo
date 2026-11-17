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

Python and FastAPI consume the Rust SDK from its owner repository through an immutable Cargo Git revision. They no longer vendor SDK source.

## Development and integration tests

`external-tools.lock.json` pins integration dependencies. Run `node scripts/external-tool.mjs --provision` before compiling tests that embed external artifact fixtures. Checkouts are cached outside the host and exposed through ignored `tests/external/<name>` links. There are no package installation or lifecycle hooks in this step. The host production build does not fetch or compile those JavaScript packages.

For a development checkout, `LEKALO_EXTERNAL_TOOLS` may explicitly map package names to absolute local roots. This override is for testing uncommitted package changes; release and CI runs use the committed lock.

The small SDK/framework scripts remaining in `scripts/` delegate to their owning repository. Host/schema fixtures remain in core where they test admission. Public schema snapshots in the SDK have an exact origin/digest receipt; they contain protocol data, not a copy of host implementation.

Legacy storage commands use the external PostgreSQL provider through `LEKALO_STORAGE_PROVIDER`, encoded as a JSON Source, for example `"path:/absolute/path/lekalo-postgres"` or `{"command":["node","/absolute/path/lekalo-postgres/bin/lekalo-postgres.mjs"]}`. The default installed command is `lekalo-postgres`. Missing providers fail closed; there is no embedded DDL fallback. The provider runs in the same empty strict sandbox as an external runtime, with JSON inputs and no database authority.

## Remaining legacy boundaries

`external-packages.lock.json` still covers seven older source snapshots: Go/Rust/TypeScript/Python discovery, Python runtime, HLV and OpenSpec. They are not the final plugin architecture. PostgreSQL-specific migration/drift policy and generic projection tables also remain in the legacy storage pipeline; the extracted renderer and type/matrix answers are external, while existing typed facades preserve callers. Replacing these remaining registrations and moving the parent/child contract graph are still tracked by the open separation issues.

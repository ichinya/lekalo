# TypeScript 7 runtime bundle

The maintained Node adapter uses TypeScript **7.0.2**. Its native compiler
is a separately packaged, core-supervised process. The JS artifact embeds
the matching API client, AST helpers and standard declarations. It does not
resolve project dependencies, search `PATH`, install packages or spawn a
compiler during a scan.

## Provisioning and distribution

From the repository root, prepare the adapter on the machine that will run it:

```sh
npm ci --prefix adapters/node-typescript --ignore-scripts --no-audit --no-fund
node adapters/node-typescript/provision-compiler.mjs
node adapters/node-typescript/build.mjs --check
```

Provisioning checks the installed platform package against the committed
compiler SHA-256 and all 108 standard-library pins. It copies these resources
into the ignored `adapters/node-typescript/runtime/` directory. It needs no
network access after `npm ci` and refuses a modified existing compiler.

Distribute `adapter.mjs`, `adapter.manifest.json`, `runtime-bundle.json`, the
selected `runtime/<platform>/tsc[.exe]`, `runtime/libraries/`,
`THIRD_PARTY_NOTICES.md` and `typescript-native-notices.txt` together. The
runtime contract lists five exact native resources: Windows x64, Linux x64
and arm64, and macOS x64 and arm64. Only the selected compiler must be present.
No `node_modules` directory is needed in the deployed package.

Use the [public scan command](public-typescript-scan.md). A current Lekalo
core reads the entry's literal runtime-bundle header, validates its closure
and stages the verified bytes before starting either process. Direct
`node adapter.mjs --version-json` remains a metadata probe. A compiler-backed
operation requires the supervised bundle; older cores cannot run it.

## Sandbox and transport

Core creates two independent private views and supervises both processes.
The producer receives only the admitted application scopes. The native
compiler receives an empty application view, the pinned compiler and pinned
standard declarations. Both use the existing child and network restrictions;
on Windows each has a separate AppContainer and job with a one-process cap.
Each component retains its own memory bound. Session evidence reports the
sum of the two component bounds, rather than claiming one process for the
whole bundle. Platform-specific enforcement qualifications remain applicable.

Core relays anonymous stdin/stdout pipes between the peers using fixed-size
buffers. The producer receives its public request through the existing
bounded file transport; its fd2 is the private response channel mapped back
to logical protocol stdout. Native stderr retains a separate diagnostic cap.
The common deadline, cancellation and output bounds terminate both peers.
Core checks native liveness before accepting successful producer completion,
then deliberately stops the compiler and joins the relay workers.

All five native filesystem callbacks are sealed. A missing read returns
`null`, never `undefined` (which TypeScript interprets as permission to use
the real filesystem). File and directory misses return false; enumeration
returns a bounded admitted list. Application sources are compiler context,
never executable imports.

## Provenance and compatibility

The new [runtime contract](../contracts/compiler-runtime.schema.v1.3.0.json)
pins entry, compiler and declaration bytes. Protocol
[`1.3.0`](../contracts/target-protocol.schema.v1.3.0.json) adds the selected
compiler kind, version, platform, binary digest and standard-library digest.
The host checks this provenance independently against the staged resources.
Runtime-bundle responses require it; ordinary adapters can omit the additive
member when they have no supervised compiler component.
It is included in the observed scan revision, so a changed compiler or
platform cannot reuse a previous proposal or binding confirmation unnoticed.

The adapter's existing `digest` and lint `artifactDigest` continue to mean
SHA-256 of the exact JS entry bytes. Native inventory keeps the bounded
`1.1.0` shape. Frozen predecessor schemas, receipts and indexes keep their
published bytes and meanings. A new scan does not infer or confirm business
semantics automatically.

The compatibility bridge resolves actual native API handles, ASTs, types,
symbols and diagnostics. It rebases configured `paths` before combining
workspace options. Removed or unrepresentable compiler settings must surface
as diagnostics or refusal; they must not silently become a successful scan.

Source-only tests and the brownfield authoring harness explicitly inject the original pinned native API
outside confinement. Public CLI and sandbox gates use the production paired
transport and are the evidence for host confinement.

# Issue #43: observed Node/TypeScript adapter kernel implementation plan

## Research basis and conclusion

Research date: 2026-09-15. Checkout: `ichinya/M3`, HEAD `ce539c53c60f6c2456ad97d9a824d2cbc580555a`; product version `0.2.16`. The checkout was clean when research started. The first research command was `gh issue view 43 --repo ichinya/lekalo`; the issue body, its comment, and the current bodies of #44 and #48 were read. This task creates only this plan; no implementation, version reserve, build, native command execution, commit, or branch change was performed.

Implement a dependency-free, read-only process kernel first. Keep the production launch artifact self-contained, advertise only implemented operations, validate all configured roots before invoking an extension, and retain complete extension evidence inside a separate internal envelope. Do not copy the reference scanner's extraction/package discovery into the kernel. Sources: [issue #43](https://github.com/ichinya/lekalo/issues/43), [issue #44](https://github.com/ichinya/lekalo/issues/44), [issue #48](https://github.com/ichinya/lekalo/issues/48), [`tests/fixtures/bindings/ts-scanner.mjs`](../tests/fixtures/bindings/ts-scanner.mjs).

**Four boundaries require explicit acceptance decisions.** A complete plan can be delivered now, but these must not be disguised as already-supported protocol features:

1. The resolved **target profile** transported by v0.2.16 is a profile token plus digest/capability pairs. It contains no project read-root list. Even the full core `ResolvedProfile` contains no read roots. A kernel-internal resolved **project profile** must therefore have a separately specified input authority; it cannot be an invented request member. See [`target_profile/resolution.rs`](../crates/lekalo-core/src/target_profile/resolution.rs), `ResolvedProfile`/`wire_resolution`, and [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `RequestEnvelope`/`ProfileResolution`.
2. The closed process `Evidence` contains only adapter identity and optional plan id. It cannot carry arbitrary revision/provenance/freshness/full source spans/local references, and `describe` cannot carry `result` or `progress`. Full **external** preservation and runtime-version reporting in the handshake require a reviewed transport/consumer extension. Internal preservation plus a separate local metadata probe is feasible without changing contracts. See [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `Evidence`/`AdapterIdentity`, and [`target_protocol/mod.rs`](../crates/lekalo-core/src/target_protocol/mod.rs), describe response validation.
3. Strict shared conformance requires all eight operations. A truthful describe-only kernel fails `capability.surface`; its applicable default-profile checks can pass, with explicit skips. Do not advertise fake generation or implement fake empty successes to obtain a badge. See [`docs/adapter-conformance.md`](../docs/adapter-conformance.md), “Profiles”, and [`adapter_conformance/mod.rs`](../crates/lekalo-core/src/adapter_conformance/mod.rs), `ALL_OPERATIONS`/`capability_phase`.
4. The current `scan_service` projects a small, closed `detail` object and derives its own revision/fingerprints. It does not preserve an arbitrary extension revision or full evidence object, and it does not inspect `result.truncated` before merging. Rich or partial scanner output must not be enabled through that consumer until #44 and the core owner resolve this boundary. See [`observed/scan_service.rs`](../crates/lekalo-core/src/observed/scan_service.rs), `run`, `DETAIL_KEYS`, `parse_detail`, `build_document`.

Recommended acceptance scope for #43: a functioning production handshake; standalone synthetic/internal profile, dispatch and evidence tests; explicit unsupported wire responses for absent extensions; unchanged published contracts; and a documented integration decision for the missing external projections. If acceptance means full evidence must traverse the public process boundary immediately, the “no contract changes” expectation is not feasible as written. Resolve that interpretation before implementing a substitute transport.

## 1. Repository layout, complete file inventory, dependencies and launch

### Proposed layout

Use `adapters/node-typescript/`. The root Cargo workspace currently contains only `crates/lekalo-cli` and `crates/lekalo-core`; it has no Node package workspace convention to extend. An adapter-local directory is already used inside the orchestration synthetic project. The canonical structure contract explicitly leaves user-toolchain paths outside `lekalo/**` and `.lekalo/**` to their owner. Sources: [`Cargo.toml`](../Cargo.toml), [`tests/fixtures/orchestration/project/adapters/node-typescript/node-adapter.mjs`](../tests/fixtures/orchestration/project/adapters/node-typescript/node-adapter.mjs), [`docs/canonical-structure.md`](../docs/canonical-structure.md).

Proposed new implementation files (all paths below are proposals, not files created by this research):

| Path | Responsibility |
|---|---|
| `adapters/node-typescript/adapter.mjs` | Single physical runtime artifact; logical modules listed below; guarded main so tests can import pure APIs without starting stdin processing. |
| `adapters/node-typescript/README.md` | Exact invocation, kernel-only operation table, profile authority, standalone synthetic limitations, errors and extension interface. |
| `adapters/node-typescript/test/kernel.test.mjs` | Pure request/response validation, canonicalization, profile, dispatch and evidence tests. |
| `adapters/node-typescript/test/process.test.mjs` | Real one-shot child stdin/file transport, runtime metadata, malformed bytes, refusal and no-side-effect tests. |
| `adapters/node-typescript/test/roots.test.mjs` | Lexical/physical roots, link/junction, all-roots-before-dispatch tests. |
| `adapters/node-typescript/test/fixtures.test.mjs` | Closed synthetic fixture custody and repeated-output assertions. |
| `scripts/test-node-typescript-kernel.mjs` | Explicit test-file runner; uses Node directly, no package manager or package scripts. |
| `tests/fixtures/node-typescript-kernel/README.md` | `public-fixture`, exclusively synthetic provenance, bounded fixture inventory, no real-consumer derivation. |
| `tests/fixtures/node-typescript-kernel/project/src/example.ts` | Inert synthetic text; the kernel never parses it. |
| `tests/fixtures/node-typescript-kernel/project/test/example.test.ts` | Inert native-test text; never executed. |
| `tests/fixtures/node-typescript-kernel/project/package.json` | Optional poison/tripwire package script declaration; never read for discovery and never executed. |
| `tests/fixtures/node-typescript-kernel/profiles/standalone.valid.json` | Explicit internal resolved profile with only `src/**` and `test/**` source permissions. |
| `tests/fixtures/node-typescript-kernel/profiles/invalid.json` | Table of named invalid profiles, including traversal, absolute/UNC/drive, sibling-prefix and digest/capability mismatches. |
| `tests/fixtures/node-typescript-kernel/requests/describe.json` | Valid protocol 0.2.16 request and deterministic request id. |
| `tests/fixtures/node-typescript-kernel/requests/invalid.json` | Raw request strings/byte vectors; duplicate keys and invalid UTF-8 must not be pre-parsed away. |
| `tests/fixtures/node-typescript-kernel/extensions/results.json` | Synthetic scanner/runner results: complete, partial, unknown, ambiguous, malformed, error and local-only canary cases. |
| `tests/fixtures/node-typescript-kernel/expected/normalization.json` | Expected normalized internal evidence and allowed wire projection/refusal outcomes. |
| `crates/lekalo-core/tests/node_typescript_kernel.rs` | Production `TargetClient` tests of the actual adapter artifact and public failure classification. |

Existing files to change during implementation: `.github/workflows/ci.yml` (run the new suite and default-profile adapter conformance on existing OS/Node matrices), `docs/target-protocol.md` (link the concrete adapter and accurately state its limits), plus the version-custody files in section 8. Add a documentation page only if the README becomes insufficient; no additional build/bundle/package files are required by this design. Do not edit the existing fake adapter or reference scanner to make the new kernel appear compatible.

### Logical module breakdown within `adapter.mjs`

1. Identity and supported-version constants; metadata probe.
2. Bounded input transport and strict JSON/UTF-8 decoder.
3. Closed request and response validators and canonical serializer.
4. Capability descriptor and operation table.
5. Resolved project-profile validator and pure normalization.
6. Root grammar/physical resolution and restricted read facade.
7. Extension registration/dispatch with preconditions.
8. Evidence/diagnostic normalization and wire projection.
9. One-shot main: read one request, dispatch once, emit one response, finish.

This is deliberately one physical file: core copies the executable and **only its first-argument script** into its private runtime. Relative imports of sibling files and ambient `node_modules` are not copied. A future split into source modules requires a deterministic single-file build artifact and custody check, or an explicitly supported runtime bundle; simply adding `.mjs` imports will break confinement. Sources: [`target_protocol/confinement.rs`](../crates/lekalo-core/src/target_protocol/confinement.rs), `command` script-copy logic around lines 225–268; [`docs/target-protocol.md`](../docs/target-protocol.md), “Platform boundary”.

Runtime dependencies: Node built-ins only (`node:fs`, `node:path`, `node:crypto`, `node:url`, `node:util` as needed). No npm installation, transpilation, `typescript`, Ajv, network client, `node:child_process`, worker launcher or shell in runtime code. JSDoc describes internal types. Tests may launch **only** Node/lekalo under an explicit argv allowlist. Existing Ajv 8.17.1 is a repository test dependency provisioned outside the checkout, not an adapter runtime dependency. Match the repository's existing Node 18/24 test matrix initially; do not use newly introduced APIs without checking their minimum version. Sources: [`.github/workflows/ci.yml`](../.github/workflows/ci.yml), [`tests/fixtures/bindings/ts-scanner.mjs`](../tests/fixtures/bindings/ts-scanner.mjs).

Core execution is `AdapterCommand { program: node, args: [absolute_adapter_script] }`, then `TargetClient::describe` / `call`; the script must be the first argument, ahead of adapter options. Use `node adapters/node-typescript/adapter.mjs` from the repo for local examples. Launch is direct argv; no `npx`, npm command, shell wrapper, native Rust dependency on this adapter, or raw `transport::run` fallback. Sources: [`target_protocol/transport.rs`](../crates/lekalo-core/src/target_protocol/transport.rs), [`target_protocol/mod.rs`](../crates/lekalo-core/src/target_protocol/mod.rs), [`docs/target-protocol.md`](../docs/target-protocol.md).

## 2. Exact M3 protocol surface and version negotiation

### Implemented operations versus future attachments

| Operation | #43 production posture | Later owner and boundary |
|---|---|---|
| `describe` | Mandatory and implemented. No project/source/config reads or writes. | Kernel owns permanently. |
| `scan` | Recognize and validate framing; absent scanner is unsupported and omitted from advertised operations. Test-only scanner injection exercises dispatch. | #44 performs actual scanning. |
| `bind` | Unsupported/undeclared. Do not confuse profile validation with semantic binding. | #44/#42 binding integration. |
| `validate` | Unsupported/undeclared. This operation requires IR; it is **not** a project-profile validation RPC. | Appropriate later semantic/target validator owner. |
| `verify` | Unsupported/undeclared; test-only runner injection may exercise framing without spawning commands. | #48 native gates and separately #47 scenario support, with honest capability semantics. |
| `generate` | Unsupported/undeclared for both dry-run and apply. No fake plan. | #45–#47 after #40; existing planned-write protocol. |
| `plan-clean` | Unsupported/undeclared. No inferred deletions. | Generation/ownership lifecycle owner. |
| `clean` | Unsupported/undeclared, even if a caller supplies a plausible plan id. | Generation/ownership lifecycle owner. |

On direct valid requests to an unavailable operation, return one valid `status: "error"` response with `error.class: "unsupported"`, fixed code/message, `partial: false`, no result/writes/progress. Core normally refuses an undeclared operation **before launch** with `target.capability-unsupported` (exit 4); a child-reported unsupported error instead maps through `target.operation-failed` to invalid (exit 1). Do not promise those two paths have identical CLI status. Sources: [`target_protocol/diagnostic.rs`](../crates/lekalo-core/src/target_protocol/diagnostic.rs), `rule_for`; [`target_protocol/mod.rs`](../crates/lekalo-core/src/target_protocol/mod.rs), `check_error` and capability checks.

Proposed descriptor:

```json
{
  "adapter": {"id":"lekalo-target-node-typescript","version":"0.3.0","digest":"sha256:<actual-entry-bytes-hash>"},
  "protocol_versions":["0.2.16"],
  "ir_versions":[],
  "operations":["describe"],
  "transports":["stdin","file"],
  "targets":["node-typescript"],
  "profiles":["standalone"],
  "read_scopes":[],
  "write_scopes":[],
  "progress":false,
  "capabilities":{
    "generate.openapi":"unsupported",
    "generate.ui":"unsupported",
    "generate.zod":"unsupported",
    "scan.symbols":"unsupported",
    "verify.scenarios":"unsupported"
  }
}
```

The displayed digest placeholder is illustrative; implementation must hash exact launch-script bytes and echo the same identity on every response. `0.3.0` is the proposed adapter release version after product reservation; it is not a new protocol version. Empty `ir_versions` truthfully states that this kernel has no IR consumer. When an extension actually validates/accepts IR 0.2.16, declare exactly `["0.2.16"]`; do not predeclare compatibility to satisfy `lekalo scan` selection. Core's strict scanner selection currently requires both the IR version and `scan.symbols: full`, so the bare kernel is intentionally not a usable scanner. Sources: [`target_protocol/capability.rs`](../crates/lekalo-core/src/target_protocol/capability.rs) (the five registered ids), [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `Capabilities`, and [`observed/scan_service.rs`](../crates/lekalo-core/src/observed/scan_service.rs), `run`.

Do not invent `kernel.*`, `validate.profile`, `verify.native`, `runtime.node`, or `workspace.pnpm` named capability ids: unknown ids are rejected even if their state is unsupported. A target-profile component capability such as `runtime.async` is not automatically an adapter-capability definition. #48 must resolve the native-gate capability naming with that registry's owner instead of claiming `verify.scenarios` means every native test.

### Handshake and input rules

- Wire token: `lekalo.target/v1`. `BASE_VERSION`, `VERSION` and the sole `SUPPORTED_VERSIONS` member are all `0.2.16`. Probe once at 0.2.16; the advertised exact set must include it. No SemVer range inference, aliases, old-version fallback or automatic 0.3.0 wire upgrade. Historical “base/additive/legacy” paragraphs in `docs/target-protocol.md` were collapsed during the baseline reset and do not establish three live versions. Executable truth: [`target_protocol/version.rs`](../crates/lekalo-core/src/target_protocol/version.rs), `SUPPORTED_VERSIONS`/`negotiate`; current policy: [`docs/versioning.md`](../docs/versioning.md).
- Every response echoes the supplied operation, request id and negotiated protocol version. `project_root` must equal `.`; it denotes the core's private project view. Do not recompute a production request id from visible fields: operational ids additionally bind input bytes/project identity. [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `validate_request`/`request_id`; [`docs/target-protocol.md`](../docs/target-protocol.md), “Requests and identities”.
- Enforce all current operation-specific combinations: describe forbids IR/target/profile; scan allows them optionally; bind requires target/profile; validate/verify require IR; generate requires IR/target/dry_run and a plan id only for apply; clean requires a plan id; plan-clean has no plan id. Reject duplicate decoded keys, unknown members, explicit null optionals, invalid tokens and unsupported versions before dispatch. Mirror both schema and Rust semantic checks. [`contracts/target-protocol.schema.v0.2.16.json`](../contracts/target-protocol.schema.v0.2.16.json), [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs).
- Read at most 1 MiB request bytes, enforce fatal UTF-8 decoding, parse exactly one JSON document, write exactly one bounded JSON response. Support stdin and `--lekalo-request-file PATH`; request-file argv is a **transport resource** furnished by core, not a project read-root grant. Reject duplicate/missing/ambiguous transport flags. No logs on stdout; bounded fixed diagnostics only on stderr. A malformed request lacking a valid echo identity must fail with bounded stderr/nonzero exit, not fabricate a valid request id. Default core limits are 600 s, 8 MiB stdout and 64 KiB stderr. [`target_protocol/version.rs`](../crates/lekalo-core/src/target_protocol/version.rs), [`target_protocol/transport.rs`](../crates/lekalo-core/src/target_protocol/transport.rs).
- `JSON.parse` alone silently collapses duplicate keys: add a bounded duplicate-aware lexer/parser and test decoded-key aliases, nesting/depth, malformed UTF-8 and trailing JSON. Canonical keys use UTF-8 byte order, not locale ordering; deterministic arrays need explicit identity order. All generated JSON is LF-only. Follow the reference serializer's intent but improve validation; never copy its permissive input parser as the acceptance gate. [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), decoder comments; [`scripts/test-lockfile-contracts.mjs`](../scripts/test-lockfile-contracts.mjs), `canonicalText`.

Exact Node version reporting: expose a **separate local adapter metadata probe**, e.g. proposed `node adapter.mjs --version-json`, reporting the exact `process.versions.node`, adapter version and entry digest. Keep protocol stdout unchanged. The same values belong in internal evidence. This is a new adapter CLI option, not an existing Lekalo flag. If #43 requires those values specifically inside `describe`, this alternative needs owner acceptance; the present closed handshake has no runtime slot and rejects both result/progress there.

## 3. Resolved project-profile authority and root resolution

### Input seam without changing contracts

Define an internal `ResolvedProjectProfile` supplied to `createKernel({resolvedProjectProfile, extensions, localEvidenceSink})`; its fields are a proposed in-process interface, **not** a JSON protocol extension or canonical target YAML:

```text
id, mode=observed, target=node-typescript,
readRoots=[{path, kind=file|tree}],
exclusions=[explicit bounded logical scopes],
targetResolution?={digest, capabilities},
provenance={origin, revision, disposition},
localReference? [never serialized into public output]
```

Keep permitted project root as an injected trusted execution context (production: the canonical current private view), never a caller-controlled absolute grant. The internal profile's own digest, if used, hashes its complete effective permissions; it is a different domain from core `profiles.digest`. Never claim the existing target profile digest binds roots it does not contain. A caller supplying `profile_digest` and `profile_capabilities` must provide both, with a profile token, valid SHA-256 and sorted/unique bounded capability pairs; compare them against the trusted target-resolution snapshot, do not just check syntax. Sources: [`target_profile/resolution.rs`](../crates/lekalo-core/src/target_profile/resolution.rs), digest domain and `wire_resolution`; [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `validate_request`.

For #43 standalone tests, inject the explicit committed synthetic profile directly into the imported kernel and into a self-contained generated test entry when process framing is needed. Generate that entry in disposable test storage using the same production code and immutable fixture data, with no dynamic import from the project. Production describes the kernel without roots or installed extensions. Do not enable ambient `src/**`, package.json discovery, environment-derived roots, a user-supplied profile-file path, or module paths on the wire as a convenience fallback.

For later real projects, the upstream resolver/build integration must supply a validated root-bearing snapshot through an accepted mechanism (for example, a trusted pre-resolved launch artifact whose bytes are covered by the entry digest). This remains an integration decision with #44/#48/#29; a shipped universal arbitrary-project root-selection mechanism does not exist in the current request contract. If required inside #43, prefer a formally reviewed contract change to an undocumented out-of-band authority channel.

### Validation pipeline (before invoking any extension)

1. Validate the entire internal profile as a closed bounded object: required identity/mode/target, own-property types, no null/unknown keys, unique explicit roots, allowed root kinds, valid evidence vocabulary and coherent optional target resolution. Freeze a normalized copy; preserve provenance/local references separately without weakening them.
2. Validate **all** root spellings before filesystem I/O: no absolute/drive/UNC/device namespace/URI paths, `..` or `.` segments, empty segments, backslashes, percent-encoded escapes, colon/control characters, trailing dots/spaces, DOS device/short-name aliases, overlong paths, uppercase/nonportable segments. `.` is permitted only as protocol `project_root`, not as an all-files read root. Root records name exact files or trees; convert tree `src` to `src/**` for scope comparison, never accept general globbing. Use target scope grammar rather than inventing a looser host-path grammar. [`target_protocol/scopes.rs`](../crates/lekalo-core/src/target_protocol/scopes.rs), `segment_ok`, `is_logical_path`, `is_scope`.
3. Require each root to fit the trusted profile permissions and, for an enabled process operation, the handshake's declared read scopes. Scope containment is by path segments: `src/**` does not cover `src-other/x.ts` or exact `src`; an exact-file scope grants no siblings. Deduplicate/order explicit roots deterministically or reject overlapping ambiguous declarations; never widen to an ancestor as normalization. [`target_protocol/scopes.rs`](../crates/lekalo-core/src/target_protocol/scopes.rs), `scope_covers` and tests.
4. Resolve against the permitted private root, walk components with `lstat`, reject links/junctions/special files, and check canonical physical containment and spelling. A root missing or uninspectable at invocation is an explicit failure/unknown input, not an empty successful project. On Windows, lexical drive comparison alone is insufficient. No source read or extension invocation occurs until every root passes. [`docs/canonical-structure.md`](../docs/canonical-structure.md), “Portable path and physical-containment rules”; [`target_protocol/confinement.rs`](../crates/lekalo-core/src/target_protocol/confinement.rs).
5. Provide extensions a read facade restricted to the resolved roots; revalidate physical path/descriptor state at actual reads, cap file count/bytes and reject excluded/vendor/generated inputs. No raw absolute-root authority in the public DTO. Counters in tests prove that one invalid late root prevents **all** scanner/runner callbacks.

Node `lstat` inspects the link itself while `stat` follows it; `realpath` resolves physical spelling. These help validation but cannot establish full cross-platform descriptor-relative confinement or eliminate ancestor-swap races. The production host's confined private view remains essential, with #89 owning the broader private/untrusted guarantee. Current Node documentation was fetched via Context7 `/nodejs/node`: [official fs API](https://github.com/nodejs/node/blob/main/doc/api/fs.md). Do not treat recent Node implementation examples as a promise of API availability on the repository's Node 18 baseline.

## 4. Stable internal scanner/runner dispatch boundary

Proposed internal API:

```text
createKernel({identity, resolvedProjectProfile, extensionRegistry, localEvidenceSink})
  describe() -> current closed process descriptor
  dispatch(validatedRequest, trustedExecutionContext) -> InternalOperationOutcome

ExtensionDescriptor = {
  id, version, operations, namedCapabilities, acceptedIrVersions,
  invoke({operation, request, profile, readView, cancellation, limits})
}
InternalOperationOutcome = {
  state: complete|partial|unknown|unsupported|failed,
  data, evidence, diagnostics
}
```

The API is proposed; current protocol operation names and capability states remain unchanged. Register built-in extensions explicitly, never discover plugins or load code paths from a project/profile. Validate descriptor compatibility before advertising it. Compute the capability map from installed, enabled implementations; a registered function alone does not prove full semantic coverage. Validate every profile/root before **invoke**, normalize every returned value before serialization, and catch rejection/throw/malformed output at one boundary.

#44 attaches `scan` and later binding semantics; #48 attaches its plan/run lifecycle to an appropriate existing operation once its wire projection and capability meaning are agreed. Test spies for both attach independently to the same dispatcher and verify call arguments without executing scripts. #43 has no runner subprocess implementation. The stable callback boundary is an architectural extension seam, not an isolation boundary against malicious imported JavaScript. Sources: [#43](https://github.com/ichinya/lekalo/issues/43), [#44](https://github.com/ichinya/lekalo/issues/44), [#48](https://github.com/ichinya/lekalo/issues/48), [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `Operation`.

Keep future write operations behind separate extension eligibility and core plan authority. Existing generate/plan-clean/clean framing already supports the future process boundary; this does **not** mean all future Zod/OpenAPI/scenario/native-gate payloads fit today's closed result shape. #44's TypeScript compiler/package runtime and #48's runner tooling also need explicit bundling/confinement decisions; zero-dependency #43 does not resolve those packages' runtime access. [`docs/target-protocol.md`](../docs/target-protocol.md), “Plans”/“Platform boundary”; [`target_protocol/confinement.rs`](../crates/lekalo-core/src/target_protocol/confinement.rs).

## 5. Evidence envelope, partial states and diagnostics

### Preserve internally; project only what the consumer can represent

| Evidence | Internal normalization requirement | Existing public representation and limitation |
|---|---|---|
| Adapter/runtime identity | Exact adapter id/version/content digest plus exact Node version. No placeholder zero digest in production. | Process evidence has adapter identity only; Node version has no handshake field. |
| Revision | Preserve extension revision byte-for-byte, bind normalized outcome to request/profile/input identities separately. | Core scan derives revision from serialized `[adapter.id, adapter.version, adapter.digest, response.request_id]`; it does not accept an extension revision through scan entries. |
| Provenance | Preserve origin, adapter and source revision; distinguish declared/observed/inferred and user-confirmed state. Never upgrade inferred evidence. | Observed `Provenance` has origin/confidence/adapter/revision; limited scan projection constructs these downstream. |
| Confidence | Closed `exact/high/medium/low/unknown`; no numeric score or optimistic default. | Entry detail `q`; core currently defaults missing `q` to medium, so an extension must supply `unknown` explicitly when appropriate. |
| Freshness | Preserve `current/stale/unknown` and its evidence; missing fingerprint stays unknown. | Core fingerprints source bytes and audits; adapter current claims are not authority. |
| Source spans | Keep original start/end line/column and coordinate convention plus source identity; validate ranges. Do not invent missing end/column values. | `ScanEntry.path` plus detail `l` supports a 1-based declaration line; observed `SourceLocation` has only path/line. Full spans cannot round-trip. |
| Original local reference | Keep original value in a local-only side of the internal result/sink; derive portable logical paths separately. Never overwrite it with a redacted string and call that preservation. | No process/observed slot for an arbitrary local-only reference. Do not place it in detail, error, stderr, golden, or source path. |
| Dynamic/ambiguous result | Preserve candidate sets and reasons; state partial/unknown, never first-match or empty-success. | Current scan grouping supports multiple native candidates, but not full arbitrary uncertainty envelopes. |

Sources: [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `Evidence`/`ScanEntry`; [`observed/types.rs`](../crates/lekalo-core/src/observed/types.rs), `BindingState`, `Confidence`, `SourceLocation`, `Provenance`; [`observed/scan_service.rs`](../crates/lekalo-core/src/observed/scan_service.rs), `parse_detail`/`build_document`; [`docs/bindings.md`](../docs/bindings.md).

The scan `detail` boundary is at most **128 Unicode code points** at process decoding and a closed JSON object with keys `s,n,l,q,t` in the core consumer; `s` and `n` are actually required by `parse_detail` (despite a comment describing `n` as optional). Reject an unrepresentable value instead of truncating native identities or smuggling metadata into extra keys. A generic JSON-object-in-detail escape hatch is not compatible with the existing consumer. [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `validate_bounds`; [`observed/scan_service.rs`](../crates/lekalo-core/src/observed/scan_service.rs), `DETAIL_KEYS`/`string_member`.

For #43 unsupported/dynamic synthetic extension outcomes, test full internal evidence preservation, then use an honest operation error on the public wire if the information cannot be represented safely. `status:"error"` cannot also carry `result`, `capabilities`, or `progress`. For partial failure set `error.partial:true`; do not add invented `status:"partial"`. For future scan truncation, `truncated:true` is syntactically available but the current merging caller ignores it; keep that integration disabled until the owner ensures partial data cannot become a valid complete scan receipt. [`target_protocol/mod.rs`](../crates/lekalo-core/src/target_protocol/mod.rs), error/result validation; [`observed/scan_service.rs`](../crates/lekalo-core/src/observed/scan_service.rs), `run`.

### Error normalization

- Profile/request invalidity: fixed error class `invalid`, stable bounded adapter code, fixed human text, `retryable:false`.
- Uninstalled/unsupported extension: class `unsupported`; do not reinterpret a capability gap as a successful no-op.
- An extension's controlled conflict: class `conflict`; explicit partial state if any partial work/evidence was produced.
- Exception/uninspectable dependency: class `infrastructure`, fixed code/message; omit raw JS stacks, OS paths, stdout/stderr and untrusted error strings.
- Core transport cancellation/crash/timeout/output cap remains core's `target.*` unavailable classification. No raw error class guesses at the adapter boundary.

Wire code is 1–128 code points, message 1–256, detail at most 16 entries of at most 128. Public core diagnostics retain fixed `adapter-error` or `adapter-error-partial` and opaque path subjects; they do not preserve arbitrary child codes/messages. Keep richer internal diagnostics in local-only evidence when authorized, and never promise new `target-profile.*`/`observed.*` rules are emitted by an arbitrary child error. [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `OperationError`/`validate_bounds`; [`target_protocol/diagnostic.rs`](../crates/lekalo-core/src/target_protocol/diagnostic.rs).

Local-only original references are not an export authorization. Public fixture/evidence is exclusively synthetic and explicitly `public-fixture`; exclude real consumer names, URLs, identifiers, absolute paths, credentials and copied source. Persistence/export for real private consumers needs the #120 policy/#119 enforcement/pilot path, not a kernel “redaction passed” flag. [`docs/privacy.md`](../docs/privacy.md), “Dispositions”; [issue #43 comment](https://github.com/ichinya/lekalo/issues/43).

## 6. Deterministic standalone synthetic framing

Use exactly the fixture files listed in section 1. Materialize fresh disposable copies and explicit fixture profiles; do not search for a project, walk parents, resolve nearest package.json, read npm/pnpm/yarn/bun configuration, parse tsconfig or inspect workspaces. The fixture's source text is inert data. Build test-only callback results directly from committed synthetic result documents; a fake scanner does not claim actual TypeScript support. This differs intentionally from the existing #42 reference scanner, whose `scan` reads package.json and extracts symbols. [`tests/fixtures/bindings/ts-scanner.mjs`](../tests/fixtures/bindings/ts-scanner.mjs), `scan`; [#43](https://github.com/ichinya/lekalo/issues/43).

Use fixed logical ids, explicit revisions/digests and injected runtime metadata in pure goldens. Real metadata probes must match the actual Node runtime; assert their field values rather than freezing one Node patch across CI versions. No timestamps, random identifiers or absolute temp paths enter normalized public output. For the same request identity and bytes, repeat process requests and compare bytes. Across distinct confined projects, request/plan identities deliberately differ; assert identity-aware equivalence and never normalize away evidence/capability drift. [`target_protocol/wire.rs`](../crates/lekalo-core/src/target_protocol/wire.rs), `request_id`; [`crates/lekalo-cli/tests/generate_orchestrate.rs`](../crates/lekalo-cli/tests/generate_orchestrate.rs), golden normalization example.

Each process test snapshots fixture source/config bytes before and after success, invalid profile, invalid root, extension rejection and unavailable operation. Include poison package scripts and a PATH directory of synthetic package-manager tripwires, but never intentionally invoke them. Static/runtime import guards plus direct-child argv auditing complement sentinel files; file preservation alone does not prove no command executed. Link/junction adversarial fixtures are created only inside disposable test roots with a synthetic outside sentinel, never the private consumer.

## 7. Contract-change decision

**For the bounded kernel plan above: no files under `contracts/` change.** Existing schemas represent the handshake, unsupported operation errors and future existing operation framing. Internal JS APIs, local test fixtures, adapter metadata CLI output and adapter implementation do not require renaming every contract. Do not add a schema or capability merely to describe internal function parameters.

The checker compares changed/added contract filenames against the product version; unchanged 0.2.16 files may remain after a 0.3.0 product bump. It scans `contracts/`, not every contract-bearing Rust constant, and does not establish semantic compatibility by itself. Baseline check executed during research:

```text
node scripts/check-contract-versions.mjs
{"ok":true,"product":"0.2.16","contractArtifacts":58,"base":"HEAD"}
```

Sources: [`scripts/check-contract-versions.mjs`](../scripts/check-contract-versions.mjs), lines 9–38; [`docs/versioning.md`](../docs/versioning.md). The later reset policy supersedes d7cf992's historical independent-contract wording: a **changed** contract takes the product version of that change; an **unchanged** contract keeps its version.

Conditional exception: adding runtime fields to describe, root-bearing request fields, full external evidence fields, changed conformance contract semantics or new closed operation names is a contract change. Then use 0.3.0 for the affected contract(s), update discriminator/constants/embedded copies/registry/digests/consumers and compatibility tests, and explicitly revise #43's scope. Do not edit a `.v0.2.16` published file in place while product is 0.3.0. Internal evidence preservation alone must not be presented as proof of full external evidence preservation.

## 8. Exact product-version reserve and golden regeneration

This section is for the implementation coordinator; **none of these mutations were performed in this research task**. Historical precedent inspected: [`d7cf9928f95314eeda1a4da673a9ac04ca7e0e56`](https://github.com/ichinya/lekalo/commit/d7cf9928f95314eeda1a4da673a9ac04ca7e0e56), which reserved 0.2.15 and changed 18 files. Reproduce its custody categories against the current tree, not its old hash values or obsolete contract versions.

### Files and precise edit scope

| Surface | Required action for 0.2.16 → 0.3.0 |
|---|---|
| `Cargo.toml` | Change only `[workspace.package].version`. |
| `Cargo.lock` | Change the local `lekalo-cli` and `lekalo-core` package record versions; preserve third-party packages/checksums. |
| `crates/lekalo-cli/tests/cli.rs` | Update literal root-manifest version/inheritance custody checks. Runtime output probes already use `env!("CARGO_PKG_VERSION")`. |
| `crates/lekalo-cli/tests/{authorization,context,diff,effects,graph,impact,inspect,requirements}.rs` | Update human/JSON **binary-version** expectations and corresponding comments only; leave contract identities and schema versions at 0.2.16. |
| `tests/fixtures/lockfile/valid/contract-only.lock.json` | Regenerate through the real `lekalo lock`; this updates `core.version` and the derived `resolver.request_digest`. Keep contract and resolver versions unchanged. |
| `tests/fixtures/lockfile/valid/contract-only.expect.json` | Recompute both `lockDigest` and `payloadSha256` from canonical lock payload without the final LF. |
| `crates/lekalo-core/tests/lockfile.rs` | Update `GOLDEN_DIGEST`, expected core version and version-tampering selectors. Prefer a `core.version`-specific byte selector so identical 0.2.16 contract fields are not accidentally changed. |
| `crates/lekalo-cli/tests/lock.rs` | Update `GOLDEN_DIGEST`. |
| `tests/fixtures/orchestration/generate.dry-run.golden.json` | Regenerate actual receipt; changed lock digest comes from real new lock. |
| `tests/fixtures/orchestration/generate.apply.golden.json` | Regenerate actual receipt, including derived manifest digest. |
| `tests/fixtures/orchestration/verify.full.golden.json` | Regenerate after the same lock/generate sequence. |

Sources: current files above; [`lockfile/resolution.rs`](../crates/lekalo-core/src/lockfile/resolution.rs), `ResolutionRequest::canonical_bytes`/`request_digest`; [`scripts/test-lockfile-contracts.mjs`](../scripts/test-lockfile-contracts.mjs), `recomputeDigest`; [`crates/lekalo-cli/tests/generate_orchestrate.rs`](../crates/lekalo-cli/tests/generate_orchestrate.rs), `the_full_sequence_matches_the_golden_receipts`.

Do not blanket-replace `0.2.16`. The current reset makes product, adapter-fixture, resolver and contract versions textually identical while their meanings differ. Do not regenerate `multi-adapter.lock.json` merely because it has a version literal: it is synthetic wire/resolver data, not the binary-generated contract-only custody fixture. d7cf992 changed `lockDigest` but left its sidecar's old `payloadSha256`; the current sidecar is coherent, and the new reserve should recompute **both**, not copy that historical oversight.

### Regeneration commands and recipe

1. Recheck branch/dirty state and reserve ownership; edit the two Cargo surfaces and version-custody assertions precisely. Build the reserved binary without dependency updates:

```powershell
cargo build --workspace --locked
cargo run --locked -p lekalo-cli -- --version
cargo run --locked -p lekalo-cli -- --version --json
```

2. Run the following one-off Node recipe from the repository root through PowerShell (`@' … '@ | node --input-type=module`). It uses only the existing trusted synthetic adapters. It writes the named goldens and two digest constants, and puts fixture execution in new external disposable directories. It is proposed regeneration code, not a currently committed script; run it only during authorized implementation. If `CARGO_TARGET_DIR` redirects the binary, set `LEKALO_BIN` to its absolute built path first. Do not use an older PATH-installed lekalo.

```javascript
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, mkdtempSync, readFileSync, realpathSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const repo = realpathSync('.');
const bin = process.env.LEKALO_BIN ?? join(repo, 'target', 'debug',
  process.platform === 'win32' ? 'lekalo.exe' : 'lekalo');
assert.equal(JSON.parse(execFileSync(bin, ['--version', '--json'],
  { encoding: 'utf8', cwd: repo })).version, '0.3.0');
const work = realpathSync(mkdtempSync(join(tmpdir(), 'lekalo-reserve-030-')));
const run = (cwd, args) => execFileSync(bin, args,
  { cwd, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024 });
const copyFixture = (name, relative) => {
  const dest = join(work, name);
  cpSync(resolve(repo, relative), dest, { recursive: true });
  return realpathSync(dest);
};
const lockRoot = copyFixture('lock', 'tests/fixtures/lockfile/project');
run(lockRoot, ['lock']);
const bytes = readFileSync(join(lockRoot, 'lekalo.lock'));
assert.equal(bytes.at(-1), 10);
assert.notEqual(bytes.at(-2), 10);
assert.equal(bytes.includes(13), false);
const lock = JSON.parse(bytes.toString('utf8'));
assert.equal(lock.core.version, '0.3.0');
assert.equal(lock.contracts.target_protocol.version, '0.2.16');
assert.equal(lock.resolver.version, '0.2.16');
const hash = createHash('sha256').update(bytes.subarray(0, -1)).digest('hex');
const digest = `sha256:${hash}`;
writeFileSync(join(repo, 'tests/fixtures/lockfile/valid/contract-only.lock.json'), bytes);
writeFileSync(join(repo, 'tests/fixtures/lockfile/valid/contract-only.expect.json'),
  JSON.stringify({ lockDigest: digest, payloadSha256: hash }) + '\n');
for (const relative of ['crates/lekalo-core/tests/lockfile.rs',
  'crates/lekalo-cli/tests/lock.rs']) {
  const path = join(repo, relative);
  const text = readFileSync(path, 'utf8');
  const pattern = /(const GOLDEN_DIGEST: &str =\s*\n\s*")[^"]+(";)/;
  assert.equal((text.match(new RegExp(pattern.source, 'g')) ?? []).length, 1);
  writeFileSync(path, text.replace(pattern, (_, a, b) => a + digest + b));
}
const orchRoot = copyFixture('orchestration', 'tests/fixtures/orchestration/project');
const adapter = ['--', 'node', 'adapters/node-typescript/node-adapter.mjs'];
run(orchRoot, ['lock', ...adapter]);
const receipts = [
  ['generate.dry-run.golden.json',
   run(orchRoot, ['--json', 'generate', '--target', 'node-typescript', '--dry-run', ...adapter])],
  ['generate.apply.golden.json',
   run(orchRoot, ['--json', 'generate', '--target', 'node-typescript', ...adapter])],
  ['verify.full.golden.json',
   run(orchRoot, ['--json', 'verify', '--target', 'node-typescript', ...adapter])]
];
for (const [name, text] of receipts) {
  const receipt = JSON.parse(text);
  assert.equal(receipt.status, 'valid');
  for (const target of receipt.targets ?? []) {
    if (Object.hasOwn(target, 'planId')) target.planId = 'plan-normalized';
  }
  writeFileSync(join(repo, 'tests/fixtures/orchestration', name),
    JSON.stringify(receipt, null, 2) + '\n');
}
console.log(JSON.stringify({ work, lockDigest: digest,
  requestDigest: lock.resolver.request_digest }));
```

The recipe deliberately reproduces the real `lock → generate --dry-run → generate → verify` fixture sequence. Only location-dependent `targets[].planId` is normalized, matching the existing test; do not zero other digests, timestamps, verdicts or evidence. It does not clean its external temporary directory automatically; any later cleanup must verify that exact directory first. The reference adapter executes within core confinement and generates only the copied synthetic fixture, never package-manager scripts.

3. Run fresh custody gates after adjusting the remaining literal assertions, then inspect the full diff:

```powershell
cargo test --locked -p lekalo-core --test lockfile
cargo test --locked -p lekalo-cli --test lock --test cli --test authorization --test context --test diff --test effects --test graph --test impact --test inspect --test requirements --test generate_orchestrate
node scripts/test-lockfile-contracts.mjs
node scripts/test-orchestration-contracts.mjs
node scripts/check-contract-versions.mjs
git diff --check
git diff -- Cargo.toml Cargo.lock contracts crates/lekalo-cli/tests crates/lekalo-core/tests/lockfile.rs tests/fixtures/lockfile/valid tests/fixtures/orchestration
```

The orchestration schema test needs the existing pinned Ajv environment described below. `check-contract-versions --base HEAD^` is the hosted check **after** a commit; while preparing changes, default HEAD or the coordinator's actual integration base is the meaningful comparison. No commit is requested by this research deliverable.

## 9. Test plan and exact commands

### Existing gates to preserve

Use the existing externally provisioned Ajv 8.17.1 via `LEKALO_AJV_NODE_PATH` / `NODE_PATH` (CI's exact provisioning is in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml)). The new #43 suite must not install it or call a package manager. Baseline suites remain independent of the new adapter:

```powershell
node scripts/check-contract-versions.mjs
node scripts/test-contract-versions.mjs
node scripts/test-versioning-contracts.mjs
node scripts/test-lockfile-contracts.mjs
# For the schema gates, NODE_PATH must name the pre-provisioned Ajv 8.17.1 modules.
$env:NODE_PATH = $env:LEKALO_AJV_NODE_PATH
node scripts/test-target-protocol-contracts.mjs
node scripts/test-target-profile-contracts.mjs
node scripts/test-bindings-contracts.mjs
node scripts/test-observed-contracts.mjs
node scripts/test-orchestration-contracts.mjs
cargo test -p lekalo-core target_protocol_conformance --locked
cargo test --locked -p lekalo-core --test target_protocol --test target_protocol_boundaries --test target_discovery --test target_profiles --test adapter_conformance --test observed
cargo test --locked -p lekalo-cli --test observed --test generate_orchestrate --test lock
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked --no-fail-fast
```

Retain all other existing workflow contract/privacy/structure/schema gates, MSRV 1.80 checks and Windows/Linux/macOS confined runtime tests. The commands above are the affected subset plus workspace validation, not permission to remove other CI steps. Linux bwrap/AppArmor and macOS sandbox prerequisites remain exactly as current CI specifies; an unavailable backend is an infrastructure result, never a successful adapter test.

### New tests

1. Handshake identity/token/version/echo; raw input duplicate keys (including escaped aliases), explicit nulls, unknown keys, two JSON documents, invalid UTF-8, boundary-sized data, wrong transport arguments; two separate process invocations prove no stale handshake state is assumed in the child.
2. Every operation's required/forbidden field matrix, both absent and injected extension registry; unsupported operations cannot produce empty success. Core refuses undeclared operations before launch. Confirm `describe` reads no project/source/config.
3. Full profile validation including paired resolution fields, digest-domain separation, mode/target mismatch, sorted capability pairs, all read roots validated before callbacks, zero-root policy, no inferred roots or package-manager discovery.
4. Root lexical and physical traversal, sibling prefixes, encoded/device names, symlink/junction escape, missing/changed root, special file and late-invalid-root cases. Counters prove scanner and runner never start on refusal.
5. Complete evidence normalization and local-only/public separation, revision/freshness/confidence unchanged, full spans intact internally, error mapping, original local reference retained locally, public canary absent. Projection refuses information loss it cannot safely represent.
6. Partial/unknown/dynamic/ambiguous outcomes never become success; `truncated` scan cannot enter the current core merge as a silently complete scan. Validate error bounds and no error+result pairing.
7. Deterministic fixture repeats, request/output caps, cancellation/timeout behavior under the production client; malformed extension result cannot bypass response validation.
8. Before/after source/config hashes and write sentinels for every outcome; no forbidden runtime imports or child/package command execution; standalone fixture tests succeed with package managers absent from PATH.
9. Actual single-file entry under production confinement, proving no hidden sibling imports/package/runtime assets are required. Inject future scanner/runner fakes using the same kernel functions; do not change external wire or test-only branches in core.

Proposed new commands after files exist:

```powershell
node --check adapters/node-typescript/adapter.mjs
node scripts/test-node-typescript-kernel.mjs
cargo test --locked -p lekalo-core --test node_typescript_kernel
cargo run --locked -p lekalo-cli -- adapter test --profile default --report junit -- node adapters/node-typescript/adapter.mjs
cargo run --locked -p lekalo-cli -- adapter test --profile strict --report junit -- node adapters/node-typescript/adapter.mjs
```

**Expected strict outcome for the proposed bare kernel:** protocol failure/exit 4 from `capability.surface`, with unsupported feature cases skipped. The exact requested strict command is included and must be run/report retained, but it is not a promised green gate. Default-profile rows applicable to the implemented handshake must pass; a skipped `no-ir-operations`, `operation-undeclared`, or `no-repeatable-probe` is not evidence of feature correctness. New kernel tests own profile/dispatch/error-normalization checks because the existing suite does not exercise all these for a describe-only adapter. No full compatibility badge is claimed from those skips. [`adapter_conformance/mod.rs`](../crates/lekalo-core/src/adapter_conformance/mod.rs), `capability_phase`, `input_phase`, determinism aggregation; [`docs/adapter-conformance.md`](../docs/adapter-conformance.md).

Keep the existing full-surface reference test green independently:

```powershell
cargo run --locked -p lekalo-cli -- adapter test --profile strict --report junit --timeout-ms 60000 -- node tests/fixtures/target-protocol/fake-adapter.mjs --lekalo-adapter-variant fluent
```

Do not broaden/rewrite shared strict semantics for one incomplete adapter. If maintainers want a named observed-kernel conformance profile, treat that as separately owned conformance-contract work and reconsider section 7.

## 10. Implementation sequence mapped to issue acceptance

The checkbox text below follows the ten current acceptance items in issue #43; these are planned proofs, not completed claims.

| Order | Concrete work | Acceptance item(s) and evidence |
|---|---|---|
| 1 | Resolve four boundary decisions in the conclusion: internal versus external evidence acceptance, root-bearing profile authority, exact-runtime metadata location, applicable versus strict conformance. Freeze the resulting scope in the adapter README before production code. | Prerequisite to AC1, AC2, AC6–AC9; prevents claiming unsupported contract behavior. |
| 2 | Reserve product 0.3.0 using section 8, regenerate every custody surface, run its focused gates. Keep contract baseline 0.2.16. | Versioned lifecycle prerequisite; not itself an issue checkbox. |
| 3 | Add the standalone runtime artifact with identity, strict bounded request parsing, canonical envelope and mandatory describe; add truthful capability/error matrix and metadata probe. | **AC1:** applicable handshake/capability/error conformance, plus exact identities and versions. |
| 4 | Add internal profile validator and synthetic fixture/profile custody, without filesystem/package discovery. | **AC2:** valid standalone synthetic profile accepted deterministically; **AC10:** no private consumer identity/path in public fixtures. |
| 5 | Add all-root lexical/physical validation and restricted read facade; inject callback spies only after success. | **AC3:** traversal/out-of-scope root rejected before scanner/runner dispatch. |
| 6 | Add side-effect tests and deny executable/project-discovery dependencies in the kernel and test harness. | **AC4:** framing never changes source/config; **AC5:** #43 suite never launches package managers/native package scripts. |
| 7 | Attach independent fake scanner and runner via explicit internal descriptors; exercise existing operation wire projection or explicit refusal. | **AC6:** #44/#48 attach without changing the external process boundary. Real scanning/execution remains their work. |
| 8 | Add evidence normalization, internal full-value retention, local-only sink/public projection, malformed-result/error bounds and ambiguity/partial refusals. | **AC7:** revision/provenance/confidence/freshness/original local reference preserved at the accepted envelope boundary; **AC8:** dynamic/ambiguous remains partial/unknown. External round-trip is still conditional on the decision in step 1. |
| 9 | Document extension eligibility for planned writes and existing generation operation framing, without enabling it. | **AC9:** #45–#47 can attach at the same process boundary; additional payload/bundle contracts remain explicitly owned follow-up work. |
| 10 | Run all new tests, applicable default conformance, exact strict command with expected limitation, existing strict fake suite and full regression gates; inspect changed-path allowlist and fixture privacy. | **AC1/AC4/AC5/AC10:** executable evidence, separate pass/skip/expected strict failure and unresolved decisions. |

### Completion report required from the eventual implementer

Report the exact candidate SHA and reserved product/contract/adapter versions, changed paths, actual commands with exit statuses, applicable conformance rows versus skips, the strict verdict, source/config preservation and zero-script-launch evidence, and unresolved external profile/evidence integration. If the owner has not accepted the internal-envelope interpretation, leave AC7 explicitly incomplete; never replace full source spans or local references with line-only/redacted projections and mark it done.

### Research validation performed

- Live issue #43 body/comments, #44 and #48 bodies fetched with `gh`.
- Current branch/SHA/status and historical reserve commit inspected read-only.
- Protocol/profile/observed/conformance/lock code, fixtures, schema and CI commands inspected; Node filesystem documentation checked through Context7.
- `node scripts/check-contract-versions.mjs` passed for product 0.2.16 and 58 contract artifact families.
- All 64 local file links resolved, the proposed regeneration JavaScript passed a syntax-only Node check, and final Git status showed only `.m3/issue-43-plan.md` as new; no tracked file changed.
- The proposed implementation and regeneration snippets were not executed; no test pass or implementation acceptance is claimed by this plan.

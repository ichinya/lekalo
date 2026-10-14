# Adapter SDKs

Issue #106 ships four optional, independently packaged SDKs for target
adapter authors, plus one shared vector corpus and a set of minimal
templates. An SDK is never required: any executable that speaks the
`lekalo.target/v1` process protocol ([target protocol](target-protocol.md))
stays fully compatible — the dependency-free fake adapter is the
independently maintained control and passes the same strict host battery
the SDK references pass.

| Package | Language | Layout | Compatibility |
| --- | --- | --- | --- |
| `lekalo-adapter-sdk-rust` | Rust (MSRV 1.80.0) | `sdk/rust` | `sdk/rust/compatibility.json` |
| `@lekalo/adapter-sdk` | TypeScript / Node ≥ 24 | `sdk/typescript` | `sdk/typescript/compatibility.json` |
| `lekalo-adapter-sdk-go` | Go ≥ 1.23 | `sdk/go` | `sdk/go/compatibility.json` |
| `lekalo/adapter-sdk-php` | PHP ≥ 8.3 | `sdk/php` | `sdk/php/compatibility.json` |

The PHP runtime has no required extensions: UTF-8 character counting
falls back to a byte-exact pure-PHP implementation when `ext-mbstring`
is absent (`ext-mbstring` is declared as an optional `suggest` in
`composer.json`; presence is used when available, never required).

Package versions are independent of the product version. A package
version can never widen the exact accepted wire set: the emitted
`compatibility.json` pins the closed supported spellings (`0.3.2`,
`0.6.4`), the per-version operation vocabulary, the upstream schema
digests, the diagnostic-registry and capability pins, the host-policy
prerequisites, and the qualifying vector digests. The contract family is
`dev.lekalo.adapter-sdk-compatibility@0.8.0` (descriptor) and
`dev.lekalo.adapter-sdk-generation@0.8.0` (reproducibility record); both
are closed Draft 2020-12 artifacts under `contracts/`.

## What an SDK gives an author

- **Generated wire vocabulary** — protocol token, exact version set,
  per-version operations, bounds, closed enum spellings, and type
  shapes, rendered from the pinned canonical contracts
  (`scripts/generate-adapter-sdk.mjs`; `--check` in CI proves the
  committed outputs byte-match regeneration).
- **Strict decoding** — one bounded request document; struct-position
  duplicate keys (including escaped spellings), forbidden nulls, unknown
  members, wrong types/tokens, unsupported exact versions, and
  out-of-bounds values are refused before callbacks with safe reason
  tokens. Admission follows the production decoder's defaults and plain
  `Option` nullability. The response `capabilities.capabilities` map alone
  permits duplicate keys with last-wins semantics; constraints may be
  absent/null, while non-null values must be in `1..=1073741824`.
- **Semantic validation** — cross-member request rules and response
  echo/identity/pairing rules, including equality of describe evidence
  and declared adapter identities, `describe` support, and the negotiated
  version's presence in the declaration.
- **Honest builders with validate-before-write** — success is
  operation-specific; a builder output that violates the closed
  contract is refused before a single stdout byte, and an undecodable
  request never becomes a fabricated success.
- **Deterministic ids and canonical bytes** — `req-`/`plan-`
  identifiers and byte-sorted compact serialization matching the core
  exactly.
- **Path/scope helpers** — the closed logical-path grammar, scope
  coverage, protected homes; defense in depth only, never a grant.
- **Bounded stderr logging** — tagged, redacted, single-line, capped;
  stdout stays the response channel.
- **The serve loop** — the whole per-exchange lifetime: read one
  request (stdin or the transport-owned `--lekalo-request-file`
  coordinate), decode, validate, dispatch, emit exactly one canonical
  response.
- **Conformance client** — a thin wrapper over the one real host
  battery (`lekalo adapter test`); native verdicts, reports, and exit
  codes pass through unchanged, and no wrapper ever awards a badge.

## Minimal adapter

```js
import { buildAdapterIdentity, buildCapabilities, serve, sha256 } from "@lekalo/adapter-sdk";

const identity = buildAdapterIdentity({
  digest: sha256("my adapter"),
  id: "my-adapter",
  version: "0.1.0",
});

// The bounded serve loop returns the process exit code; propagation is
// required so refusal classes surface as nonzero process status.
process.exitCode = await serve({
  identity,
  handlers: {
    describe: () =>
      buildCapabilities({
        adapter: identity,
        operations: ["describe", "scan"],
        protocolVersions: ["0.3.2"],
        transports: ["stdin", "file"],
        targets: ["my-adapter"],
        progress: false,
      }),
    scan: () => ({ payload: { entries: [], truncated: false } }),
  },
});
```

```sh
lekalo adapter test --profile default --report json --repeats 3 --timeout-ms 60000 node adapter.mjs
```

Everything after the program path is passed to the adapter verbatim (no
shell). The same shape works in every language: a Rust author builds a
small crate depending on `lekalo-adapter-sdk-rust` and implements one
`Adapter` trait; a Go author implements the `Adapter` interface; a PHP
author passes `identity` + `handlers` to `Serve::serve`. Full runnable
projects live in `sdk/templates/<language>/{reference,scan,generate,verify}`:
the reference implements the complete strict base surface and passes
`--profile strict` with the badge issued; the scan/generate/verify
examples are capability-limited and pass `--profile default` with their
feature rows asserted individually.

## Lifecycle and security boundaries

- **Read-only default.** No write scopes, no network, no subprocess, no
  shell. Generation adapters opt into one narrow output scope and plan
  before apply; dry-run writes nothing, apply echoes the host's opaque
  `plan_id`, and the host — never the adapter — owns publication and
  rollback.
- **The host decides.** The SDK's checks are honest-adapter aids.
  Third-party executables can bypass helpers, so the host re-validates
  everything: confinement, private view staging, scope coverage,
  protected homes, write-plan digests, and apply authority.
- **No hidden behavior.** No telemetry, no network calls, no ambient
  reads, no handshake caches. Logs are bounded, redacted, single-line
  stderr; stdout carries exactly one canonical response.
- **Errors are classified, never swallowed.** Refused builder output
  becomes a valid closed error envelope when possible, otherwise bounded
  stderr and nonzero exit; a builder refusal object is never stdout.
  Deliberate callback error returns are tested separately from actual
  exceptions/panics. TypeScript recovers thrown handler exceptions to a
  closed error envelope; Go/PHP refuse with nonzero exit and no stdout,
  and Rust handler panics terminate the process. The host classifies real
  crashes and owns cleanup/publication; the SDK does not promise Rust
  panic recovery.
- **Exact compatibility.** Unknown and future protocol versions fail
  before IR disclosure or callback execution; new operations under an
  old version fail; the generated matrix re-runs on every protocol
  upgrade (`upgrade/` vectors and the per-language
  `generated/compatibility_tests` modules).

## Versioning, provenance, and release custody

- The generator runs offline over reviewed local inputs only. Each
  `generation.json` records the generator bytes, ordered input digests,
  toolchain pin, output digests, compatibility-descriptor digest, and
  the semantic-coverage mapping (generated type or explicit
  hand-maintained codec obligation, with the vectors that prove it).
- Package releases are separately authorized: the records prove build
  inputs, not publisher signatures. Ordinary hashes are not signatures,
  and SDK provenance never bypasses the adapter trust/quarantine gates
  ([package trust](adapter-manifest.md)).
- The context-adapter API is **Planned**: reusable layers ship today,
  but no target-protocol wrapper is presented as a verified context
  provider until its owning contract lands.

## Gates

```sh
node scripts/test-adapter-sdk-contracts.mjs      # schemas + digests + matrix + determinism
node scripts/test-adapter-sdk-vectors.mjs --language all   # corpus-derived shared vectors x 4 codecs
node scripts/test-adapter-sdk-conformance.mjs --language all  # real confined battery + control parity
node scripts/test-adapter-sdk-packages.mjs --verify           # packed integrity + detached consumer
```

The vector gate replays the full wire corpus byte-for-byte. Annotated
`productionClientExpect` describe rows and emitted error envelopes also
run through the real `TargetClient::describe` using a disposable replay
process: the child verifies the actual request and emits the original
response bytes unchanged. This qualifies base-session echo, identity,
negotiation and describe semantics, not filesystem apply authorization;
the confined host battery remains the authority for planning/publication.

CI runs the schema/generator gate in the contracts job, builds the
templates in the same job that runs the conformance wrappers, and
provisions pinned Ajv 8.17.1 externally through `NODE_PATH` /
`LEKALO_AJV_NODE_PATH`. Missing required runtimes are failed setups,
never skipped acceptance.

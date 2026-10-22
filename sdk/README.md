# Lekalo adapter SDKs (issue #106)

Four optional, independently packaged SDKs over the canonical
`lekalo.target/v1` JSON wire contracts, plus one shared vector corpus:

| Package | Language | Root |
| --- | --- | --- |
| `lekalo-adapter-sdk-rust` | Rust 1.80+ (MSRV) | [`sdk/rust`](rust/) |
| `@lekalo/adapter-sdk` | TypeScript (Node ≥ 24) | [`sdk/typescript`](typescript/) |
| `lekalo-adapter-sdk-go` | Go 1.23+ | [`sdk/go`](go/) |
| `lekalo/adapter-sdk-php` | PHP 8.3+ | [`sdk/php`](php/) |

SDK adoption never affects executable eligibility, host policy, protocol
semantics, or conformance verdicts. A standalone executable that speaks
the protocol stays fully compatible — the dependency-free fake adapter
([`tests/fixtures/target-protocol/fake-adapter.mjs`](../tests/fixtures/target-protocol/fake-adapter.mjs))
is the independently maintained control and passes the same strict host
battery the SDK references pass.

## Layout

- `schema-inputs.json` — the reviewed generator configuration: ordered
  local inputs, per-version operation vocabulary, package identities,
  toolchain pins, and the semantic-coverage obligations.
- `capability-definitions.json` — the generated export of the reviewed
  embedded capability table (never a second registry).
- `<language>/generated/` — generator-owned protocol/capability/
  diagnostic vocabulary and the exact-version compatibility matrix.
- `<language>/compatibility.json` — the emitted descriptor validated by
  `contracts/adapter-sdk-compatibility.schema.v0.8.0.json`.
- `<language>/generation.json` — the reproducibility record validated by
  `contracts/adapter-sdk-generation.schema.v0.8.0.json`.
- `templates/<language>/{reference,scan,generate,verify}/` — minimal
  public-author projects; the reference implements the complete strict
  base surface, the examples are capability-limited.
- `spec.md` — the language-neutral decode/build/write contract and its
  shared vector semantics.

## Commands

```text
node scripts/generate-adapter-sdk.mjs --check
node scripts/generate-adapter-sdk.mjs --write --language all
node scripts/test-adapter-sdk-contracts.mjs
node scripts/test-adapter-sdk-vectors.mjs --language all
node scripts/test-adapter-sdk-conformance.mjs --language rust --profile strict --repeats 3 --timeout-ms 60000
node scripts/test-adapter-sdk-packages.mjs --verify
```

`--write` is an explicit development-only generator action bounded to
the generated SDK files and their records; CI permits only `--check`.
No generator, SDK runtime, harness, or adapter operation downloads
dependencies, phones home, installs runtimes, or updates templates.

## Boundaries

- The SDK validates before it writes: a builder output that violates
  the closed contract is refused before a single stdout byte, and an
  undecodable request never becomes a fabricated success.
- SDK path/scope helpers are defense in depth; only the host's private
  view, confinement, and policy enforcement are real.
- Package versions are independent of the product version and can never
  widen the exact accepted wire set; the generated matrix and host
  conformance re-run for every declared version.

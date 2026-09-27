# Typed client SDKs (issue #72)

Issue #72 adds the closed, versioned **client-SDK projection family**
([`contracts/client-sdk.schema.v0.4.0.json`](../contracts/client-sdk.schema.v0.4.0.json),
`lekalo/client-sdk/v0.4.0`, identity `dev.lekalo.client-sdk@0.4.0`):
one deterministic, language-neutral projection that joins the
validated transport-http attachment (#70), the compiled project IR
(#8), the #62 error registry, and the #64 query-model attachment into
the typed client contract every language backend renders from.
Multiple language clients derive from **one** contract — never from
independent interpretations.

The projection is pure declaration data: client generation never
defines server behavior, never reads server source, never infers
safety from the HTTP method, and never synthesizes a field the bound
contracts do not declare. No base URL, credential, or environment
value ever enters the projection; transports are injected at
construction time in every generated language.

## Family identity

| Constant | Value |
| --- | --- |
| `FAMILY` | `dev.lekalo.client-sdk` |
| `VERSION` / schema | `0.4.0` / `lekalo/client-sdk/v0.4.0` |
| Compatibility metadata | `dev.lekalo.client-sdk-compatibility@0.4.0` |
| Consumer index | `dev.lekalo.client-sdk-index@0.4.0` |

Diagnostic rules register additively as the closed `client.*` family
(`LEK-SDK-001..008`): `input-invalid`, `contract-invalid`,
`symbol-unresolved`, `mapping-unsupported`, `retry-unsafe`, `drift`,
`consumer-invalid`, `limit-exceeded`.

## What one operation carries

Every projected `ClientOperation` binds the full wire surface:

- the stable **effective operation id** (the method-name binding);
  endpoint and invoked-operation semantic ids ride as metadata;
- method and path come from the Model endpoint symbol (never restated
  by the client);
- parameters and body fields carry the complete structured data —
  location, style AND explode, requiredness, nullability, the resolved
  type id, and the closed wire `shape` (`value` or `list`), so a
  `list<T>` output never collapses to its element (issue #72 round 2;
- the request/success body projections (whole or explicit subset);
- the error union: every variant preserves the exact #62 semantic
  identity — error id, immutable code (`LEK-ERR-…`), closed category,
  projected status, **public** payload fields only — plus the derived
  retry authorization;
- auth scheme references, the idempotency-key binding with its
  requirement, and the correlation headers;
- the pagination helper binding (style, limit/offset/cursor params,
  cursor field and type) so iteration termination is explicit, never
  guessed.

Named types project with explicit presence and nullability axes:
`required` is presence, `Optional(T)` is nullability; the four
combinations stay distinct. Scalars map explicitly (`string`,
`number`, `boolean`, `date`, `datetime`, `uuid`, `uri`);
**decimals are a policy, not a default** — a scalar maps to
`decimal-string` only through explicit configuration
(`ClientConfig::decimal_scalars`) and only when the declared scalar is
string-backed; a numeric wire value is never coerced. Dates stay
validated ISO strings — never a runtime date type.

## Retry safety (the conservative matrix)

The default is **no automatic retry**; a positive retry budget alone
never authorizes one. Each declared error grants exactly one
authorization, derived from its own #62 contract (preserving
`check_retry_consistency`):

| Declared error contract | Authorization |
| --- | --- |
| `retry: never` | `never` |
| `retry: safe` on a read / `guaranteed` write | `safe` |
| `retry: safe` otherwise | `never` (refused derivation) |
| `conditional(idempotency-key)` with `key-required`/`guaranteed` | `key-required` |
| `conditional(reconciliation)` | `reconciliation-only` |

`reconciliation-only` never retries automatically: the caller
reconciles. A transport disconnect after send, a timeout, or any
failure without a recognized declared error makes **one attempt** —
there is no operation-level declaration proving retry safety for
unknown execution state, and HTTP 429/503 alone is insufficient.
Attempt budgets are bounded (`MAX_ATTEMPTS = 5`) and a zero or
over-bound request is a refusal, never a silent clamp.

## Generated clients and the Vue consumer

The node-typescript adapter claims `generate.client-sdk` (partial) and
reads only `.lekalo/cache/client-sdk/<project>.json` (written by
`lekalo generate`'s preflight when the transport home plus the
`lekalo/query-model.yaml` home validate). From that one evidence
document it renders two backends under
`src/generated/node-typescript/clients/**`: the TypeScript module
(`.client.ts`) and the Go package (`.client.go`), plus the
compatibility metadata sidecar (`.compatibility.json`, including the
rendered languages) and the ownership map (`.map.json`). Both backends
derive their path substitution, query serialization, request bodies,
and response decoding from the same operation set, so one wire request
spelled by the TypeScript client is the wire request spelled by the Go
client. The TypeScript module takes its base URL, transport, and
credentials only from constructor arguments; the Go client injects a
`Transport` interface and a `context.Context` credential function per
call.

The emitted code has no hidden analytics or telemetry and performs no
I/O by itself. The Vue consumer fixture
(`tests/fixtures/client-sdk/vue-consumer/`) imports the generated
module, typechecks strict with the pinned TypeScript, and compiles its
SFC with the pinned `@vue/compiler-sfc`; the positive usage drives a
fake transport, and the negative fixture is typechecked to prove the
consumer contract can fail. The Go client is compiled with the pinned
toolchain (`go build`) in the parity scope of the runtime gate.

## Consumer impact

`ClientArtifactIndex` relates each client artifact to its safe logical
path, language, contract digest, endpoint coverage, and **declared**
consumer ids — consumers are explicit maintained declarations, never
inferred from traffic. `client_sdk::affected_clients` joins a
base/candidate transport diff with the before index, so a **removed**
endpoint still resolves the artifact (and consumers) that covered it.
Unrelated artifacts stay excluded, and an empty inventory reports
incomplete, never clean.

## Verification

```sh
cargo test --locked -p lekalo-core --lib client_sdk
cargo test --locked -p lekalo-core --test client_sdk
cargo test --locked -p lekalo-cli --test client_sdk
NODE_PATH=… node scripts/test-client-sdk-contracts.mjs   # Ajv 8.17.1
NODE_PATH=… node scripts/test-client-sdk-runtime.mjs     # pinned TS + @vue/compiler-sfc
node scripts/test-node-client-sdk.mjs                    # adapter gate
cargo run -p lekalo-core --example regen-client-sdk -- . # golden regen (explicit only)
```

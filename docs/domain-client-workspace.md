# Domain and client workspace without a backend

Status: **Implemented** adoption, strict semantic validation and inspection
for the synthetic npm workspace below. Owner: Node evidence maintainers.
[#153](https://github.com/ichinya/lekalo/issues/153).

An entity represents domain state. A command declares an action and its
effects. None of these require HTTP routes, a database, Hono, Drizzle or
a running server. Transport and storage are separate projections; an
`update` effect alone does not mean a database update.

The example reuses the
[`core` + `web` workspace](../tests/fixtures/public-typescript-scan/workspace)
and its explicitly authored
[entity](../tests/fixtures/public-typescript-scan/model/entities.yaml),
[command/effect](../tests/fixtures/public-typescript-scan/domain/commands.yaml)
and [authorization policy](../tests/fixtures/public-typescript-scan/domain/authorization.json).
All names and state are fictional. There are no installed project packages
or backend services in this lane.

## Reproduce the example

From a Lekalo checkout with its CLI built, run:

```sh
node scripts/test-domain-workspace.mjs
```

The gate creates a fresh disposable copy, checks adoption preview purity,
adopts it, declares missing `public_core`/`public_web` modules, installs the
authored semantic example, runs `validate --strict`, inspects the command
and effect, and requests impact/context. It proves original source/config
bytes unchanged and removes its copy. CI runs it on Windows, Linux and macOS.
It does not execute the domain command or claim native/runtime correctness.

For a persistent user copy, the same public command path starts with:

```text
lekalo init --adopt --project-id domain_workspace --project .
lekalo module new public_core --project .
lekalo module new public_web --project .
```

Create only modules that adoption did not already declare. Put the example
`entities.yaml` and `commands.yaml` in `lekalo/modules/public_core/`.
`public_core.set_value` accepts a `public_core.value` and declares
`public_core.write_score`, an update of the domain entity
`public_core.score`. These are authored semantics; discovery does not invent
them or bind them automatically to source functions.

## Strict authorization still applies

Without an authorization declaration, strict validation of the update
command must deny with `authorization.effect-unprotected`. The example
keeps that negative check. A client-only application still needs an explicit
decision about its protected effects.

The example policy deliberately allows a public local caller to read/write
the single `value` field. Review that policy before using it for another
application. Copy its contents into `lekalo/authorization.yaml`, replacing
the all-zero `model_ref.ir_digest` with the SHA-256 of the exact canonical
IR object returned by `lekalo load --ir --json --project .` (the `ir` member,
not the enclosing result). The gate shows the exact public-CLI pinning
recipe. A stale model pin must deny; changing a Model requires reviewing
and pinning its policy again. [Authorization contract](authorization.md).

Then the semantic flow is:

```text
lekalo validate --strict --project .
lekalo inspect public_core.set_value --project .
lekalo inspect public_core.write_score --project .
lekalo impact public_core.score --project .
lekalo context public_core.set_value --budget 4000 --project .
```

The [public TypeScript scan](public-typescript-scan.md) is a separate,
explicitly scoped evidence step for supported top-level source mappings.
It needs no backend. It does not infer command semantics, approve an
authorization policy or promote implementation ownership.

## Which features need additional contracts or services?

| Capability | Additional requirement |
|---|---|
| Adopt, Model entities/commands/effects, strict validation, inspect/impact/context | No HTTP or database binding. Protected effects need an explicit, current authorization declaration. |
| Supported TypeScript observed scan | Explicit project profile and the manifested Node adapter; no backend or installed application packages. |
| Client/browser runtime and pure domain logic | User-owned implementation; a server is optional. The example does not claim verified runtime behavior. |
| Vitest, typecheck and Vite build evidence | Declared native commands and execution approval; production qualification is tracked separately in [#151](https://github.com/ichinya/lekalo/issues/151) and [#156](https://github.com/ichinya/lekalo/issues/156). No backend is intrinsically required. |
| OpenAPI, generated API clients and HTTP scenario projections | Declared HTTP contracts and a compatible target profile. Contract generation alone does not require a running server or database. |
| Hono/Laravel routes and framework-specific verification | Matching framework source/runtime and bindings; optional for a domain/client workspace. |
| Storage mappings, migrations and real persistence conformance | Declared storage contracts plus the chosen provider. Actual persistence verification additionally needs its isolated database service. |

An absent optional transport/storage surface is valid. An operation that
requests one still has to satisfy that operation's own contracts and gates.

# Public FastAPI route evidence

Status: **Implemented**, explicitly enabled static provider over the Python
scanner. Owner: Python adapter and observed-host maintainers. This records
registration declarations, not a running ASGI application's behavior.

First build the CLI and separate adapter workspace using the locked offline
recipe in [public Python scan](public-python-scan.md). No Python interpreter,
FastAPI installation, application import, server, network or database is used.

## Enable the provider

The existing resolved Python project profile still controls read roots and
exclusions. Pass this additional launch argument as two argv elements:

```text
--lekalo-framework-policy-json {"schema":"lekalo/framework-policy","version":1,"providers":[{"id":"fastapi","state":"enabled"}]}
```

For example, build an argv array rather than relying on shell JSON escaping:

```javascript
const args = ['scan', '--target', 'python', '--profile', 'standalone',
  '--project', '.', '--', adapterPath,
  '--lekalo-project-profile-json', JSON.stringify(resolvedProfile),
  '--lekalo-framework-policy-json', JSON.stringify({
    schema: 'lekalo/framework-policy', version: 1,
    providers: [{ id: 'fastapi', state: 'enabled' }],
  })];
```

Omitting the policy or disabling `fastapi` preserves the generic scanner and
its highest protocol `1.1.0`. Enabling it explicitly advertises `1.2.0` and
requires that version for scan. An older host cannot silently drop HTTP facts.
An already accepted HTTP index cannot be downgraded by a generic scan.

## Supported static registrations

The provider resolves imported `FastAPI`/`APIRouter` identities, local import
aliases and function aliases across admitted `.py` modules. Local lookalikes,
rebinding, ambiguous module names and local `fastapi` packages do not establish
the external framework identity. `.pyi` declarations cannot prove runtime
registrations. Package identity is a static import claim within the admitted
source inventory, not authentication of the installed runtime dependency.

It handles method decorators (`get`, `post`, `put`, `patch`, `delete`, `head`,
`options`, `trace`), `api_route`, `add_api_route`, and nested/repeated
`include_router` with literal or statically resolved string prefixes. Static
string constants/concatenation and literal method lists are supported. Router
paths must begin with `/`; other shapes remain unknown. Router
inclusion is live: later declarations remain visible. `FastAPI.root_path` is
not a registration prefix. Explicit literal `operation_id` is retained;
generated IDs are not guessed.

Each relation retains HTTP method/path, actual application and function
native IDs, registration location, `fastapi-static-v1` rule revision, trusted
policy digest, sorted exact source attachments and `high` confidence. The
handler carrier becomes an observed Endpoint proposal. Multiple paths may
point to one handler; this does not create multiple canonical definitions.
Conflicting handlers/operation IDs for one application/method/path are unknown;
the same method/path on distinct applications is valid.

Unknown paths, methods, handlers, prefixes, wrappers, control flow, import
cycles, object escape/mutation and unmounted routers produce source-bound
`framework-route` uncertainty. Some cases conservatively suppress the affected
route graph. Overflow is explicitly truncated and the public host refuses a
truncated scan. The provider bounds analysis to 100,000 work steps, 1,024
objects, 2,000 routes, 32 levels, 64 routes per handler and 64 source attachments
per route, inside the generic scanner's existing source/output bounds.

Function defaults, annotation expressions and decorator arguments are checked
conservatively for definition-time mutations. Uncalled lambda bodies are not
treated as immediate execution. Method-list aliases share an identity; a
mutation or escape to an unknown call invalidates that cached value before
later registration, including aliases imported from another module.

## Confirm author intent and review changes

Canonical Endpoint definitions remain authored user intent. The synthetic
fixture under `tests/fixtures/python-scan/fastapi` supplies a manual Endpoint
whose semantic ID matches the handler proposal, plus its invoked command.
The scanner never creates or edits these definitions. Use the existing
`bindings propose`, `bindings confirm`, `bindings audit`, `observe inspect`,
`observe impact`, `inspect`, `impact` and `context` workflow from the Python
guide with that Endpoint ID.

The observed index uses `lekalo/observed-index/v1.2.0`. Endpoint attachments
include `http_evidence`; the handler signature is qualified by the full route
evidence/source closure. Changing a registration or prefix in another file
fails audit before rescan and requires explicit reconfirmation after rescan.
Confirmation rechecks every attached source; an edit after proposal review
refuses before writing. Removing all registrations leaves a confirmed Endpoint
stale rather than silently reconfirming its generic function as an Endpoint.

Inspect exposes framework uncertainty and marks Endpoint completeness
incomplete even when the unknown registration lives in another source file.
Observed impact is always an incomplete recorded graph. A successful scan
means accepted static evidence, not route coverage or runtime correctness.
Dependency injection, middleware, Pydantic/storage extraction, OpenAPI
generation, factories and executing Python remain outside this provider.

## Acceptance

```text
node --test adapters/python/tests/fastapi.test.mjs
node scripts/gen-http-scan-protocol.mjs --check
node scripts/test-public-fastapi-scan.mjs
```

The public gate uses the compiled CLI and confined adapter on a disposable
source copy, confirms a manually authored Endpoint, exercises existing views,
checks deterministic index bytes, cross-file staleness, stale approval refusal,
route removal and source/Model preservation. CI runs these checks on Linux,
Windows and macOS; local success does not substitute for those hosted lanes.
CI additionally passes `--validate-contracts` with pinned Ajv 8.20.0 provided
outside the checkout through `LEKALO_AJV_NODE_PATH`. This independently checks
all three successor schemas and predecessor/closed-field refusal controls.

The static call rules follow the official [APIRouter reference](https://fastapi.tiangolo.com/reference/apirouter/)
and [router composition guide](https://fastapi.tiangolo.com/tutorial/bigger-applications/).

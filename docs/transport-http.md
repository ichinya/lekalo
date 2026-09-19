# Lekalo HTTP/JSON transport attachment

Issue #70 makes the HTTP/JSON wire surface of every Model endpoint a
first-class, machine-checkable contract. One closed, versioned
attachment — [`contracts/transport-http.schema.v0.4.0.json`](../contracts/transport-http.schema.v0.4.0.json)
(`lekalo/transport-http/v0.4.0`, identity
`dev.lekalo.transport-http@0.4.0`) — binds each Model `endpoint`
symbol to its full transport surface, and one deterministic projection
derives the canonical route surface every runtime (Node, Laravel, Go,
Rust) and every OpenAPI rendering consume. See
[ADR-0042](adr/0042-http-transport.md) for the owner decisions.

The attachment is pure declaration and validation data: it never
executes, never carries source text, runtime principals, tokens,
middleware names, or secrets, and has no write surface. Method, path,
and `invokes` stay single-sourced in the Model `endpoint` symbol —
the attachment references the symbol and owns everything else.

## Document shape

```json
{
  "schemaVersion": "lekalo/transport-http/v0.4.0",
  "identity": "dev.lekalo.transport-http@0.4.0",
  "attachmentRevision": "0.4.0",
  "projectId": "planner",
  "modelRef": {"modelVersion": "0.2.16", "digest": "sha256:…"},
  "irRef": {"identity": "dev.lekalo.ir@0.2.16", "digest": "sha256:…"},
  "wire": {"dialect": "lekalo-http-wire/v1", "contentType": "application/json"},
  "defaults": {"errorEnvelope": "canonical-v1", "idempotencyHeader": "Idempotency-Key",
                "correlationHeaders": ["X-Correlation-Id", "X-Request-Id"]},
  "securitySchemes": [
    {"id": "user_bearer", "kind": "bearer", "format": "jwt"}
  ],
  "endpoints": [
    {
      "endpoint": "planner.endpoint_focus_task",
      "operationId": "plannerFocusTask",
      "params": [{"name": "task_id", "in": "path", "field": "input.task_id", "required": true}],
      "body": {"contentType": "application/json", "mode": "whole-input"},
      "success": {"status": 200, "body": {"mode": "whole-output"}},
      "errors": [{"error": "planner.task_not_found", "status": 404}],
      "errorDefaults": {"validation": 400, "auth": 403, "conflict": 409,
                         "not-found": 404, "domain": 422, "infrastructure": 500},
      "auth": {"actor": "identity.user", "schemes": ["user_bearer"],
                "policyRef": "planner.focus_owner_only"},
      "idempotency": {"header": "Idempotency-Key", "required": true},
      "scenarios": ["planner.focus_one_task"]
    }
  ]
}
```

## Members

- **Endpoint identity.** `endpoint` is the semantic-id reference to
  the Model `endpoint` symbol; a missing symbol or wrong kind refuses
  (`transport.endpoint-unresolved`). `operationId` defaults to the
  deterministic camel-case transform of the endpoint id
  (`planner.endpoint_focus_task` → `plannerEndpointFocusTask`); an
  explicit override is declaration data and duplicate effective ids
  refuse.
- **Parameters.** `params` bind wire names to declared operation
  inputs: `path` parameters must bind exactly the templated segments
  of the Model path in both directions
  (`transport.param-invalid`), `query`/`header`/`cookie` parameters
  resolve to command input members (`input.<name>`) or declared
  query-model parameters (bare names). No transport-invented field
  can reach a domain entity: synthetic wire members live here only.
- **Bodies.** `body.mode` is `whole-input` (the JSON body is exactly
  the operation input) or `explicit` (a declared field subset).
  Bodyless methods (GET/DELETE) refuse `body` unless `mode:
  explicit` opts in. `success.body.mode` mirrors the output side;
  `success.status` is one of 200/201/202/204.
- **Error projection.** `errors` key on the immutable #62 error
  identity and carry one status each; the wire body is always the
  canonical quadruple `{"ok":false,"error":{"id","code","category",
  "payload"}}` with public payload fields only. Entries outside the
  operation's declared error union refuse; unmapped union members
  refuse under the strict profile (`transport.mapping-missing`).
  `errorDefaults` fixes one status per category so validation, auth,
  domain, and infrastructure failures stay distinguishable on the
  wire; undeclared infrastructure failures render the fixed generic
  body and never carry a declared id.
- **Security.** Schemes are explicit, declared, secret-free
  (`bearer`, `api-key`, `basic`, `oauth2`, `mutual-tls`, `custom`).
  Per endpoint, `auth.actor` projects the closed #25 vocabulary
  (`public`, `identity.user`, `identity.service`, `system.job`,
  `internal`); `public` carries no scheme and every authenticated
  actor carries at least one (`transport.security-invalid`);
  `policyRef` resolves to a Model policy symbol.
- **Idempotency and correlation.** `idempotency.required` is forced
  when the operation's #62 error metadata declares `key-required`
  and forbidden when every declared error is `not-applicable`;
  `correlation` declares the header names runtimes echo.
- **Pagination.** `pagination` projects the #64 contract onto
  `limitParam`/`offsetParam`/`cursorParam`/`cursorField`; only
  `page`/`stream` queries admit it, the cursor parameter type must
  match the trailing identity sort key, and the declared parameters
  must resolve in the bound query model
  (`transport.pagination-invalid`).
- **Rate limit, cache, versioning.** `rateLimit`, `cache`, and
  `apiVersion` are declarations only — header rendering belongs to
  the runtime. `wire.dialect` (`lekalo-http-wire/v1`) versions the
  wire format itself; a dialect bump is a breaking change by policy.
- **Capabilities.** `capabilities` is the closed per-endpoint set
  `streaming`/`upload`/`download` with a closed detail vocabulary
  (`sse|chunked|ws`, `multipart`, `binary`) and a `minimumSupport`
  checked against the resolved profile's capability map
  (`transport.streaming`, `transport.upload`, `transport.download`).
  An unsatisfied declaration refuses
  (`transport.capability-unsatisfied`) — never a silent downgrade.
- **Scenarios.** `scenarios` are black-box coverage references that
  must resolve to Model scenario symbols; the conformance check
  `transport.blackbox-scenarios` drives them.

## Projection and compatibility

`transport_http::project(attachment, namespace)` derives the
canonical route surface for the closed namespaces `node`, `laravel`,
`go`, and `rust` from the one attachment — byte-stable declaration
data (route table, operation identity, decode/encode plans, error
map, middleware/scheme requirements). The same canonical surface is
the single input every runtime route layer and every OpenAPI
rendering (issue #46) consume, which is what makes cross-runtime
parity checkable by byte comparison.

`transport_http::compare` classifies every changed path of two
same-family attachments: **breaking** (endpoint, parameter, error
member/status or security removal, required parameter added,
capability removed, wire dialect bump), **non-breaking** (additions
under the evolution policy, metadata), **policy-change**
(rate-limit/cache/idempotency/security-strengthening changes,
operationId override). A `wire-consumer` strict profile turns any
breaking classification into a blocked diff.

## Canonical home and evidence

One project-level canonical document `lekalo/transport.yaml`
([canonical structure](canonical-structure.md)). `lekalo transport
validate|inspect|project|diff` operate on any document path;
`lekalo validate` includes the transport semantic pass when the home
document exists, and `lekalo generate` validates it as a preflight
and writes canonical evidence to
`.lekalo/cache/transport/<project>.json` inside the `lekalo.cache`
authority home — the only transport bytes an adapter may read as
input, covered by its declared read scopes.

## Diagnostics

The family routes through the accepted #11 contract:

| Rule | Code | Meaning |
| --- | --- | --- |
| `transport.input-invalid` | LEK-TRN-001 | fatal wire input violation |
| `transport.contract-invalid` | LEK-TRN-002 | a declaration violates its closed contract |
| `transport.endpoint-unresolved` | LEK-TRN-003 | endpoint ref missing or wrong symbol kind |
| `transport.param-invalid` | LEK-TRN-004 | parameter binding violates the closed contract |
| `transport.mapping-missing` | LEK-TRN-005 | unmapped union member under the strict profile |
| `transport.security-invalid` | LEK-TRN-006 | actor/scheme combination violates the closed contract |
| `transport.capability-unsatisfied` | LEK-TRN-007 | declared capability unsupported by the resolved profile |
| `transport.pagination-invalid` | LEK-TRN-008 | pagination conflicts with the bound query model |
| `transport.export-limit` | LEK-TRN-009 | the canonical payload exceeds its bound |

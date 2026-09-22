# ADR-0042: The HTTP/JSON transport attachment

Date: 2026-09-19
Status: accepted for issue #70

## Context

Issue #70 asks for a reusable HTTP transport mapping for Node,
Laravel, Go, and Rust targets that never mixes HTTP semantics with
domain commands/queries. The Model's `endpoint` definition kind
deliberately carries only `invokes`, `method`, and `path` — the wire
surface behind each endpoint (parameters, bodies, error statuses,
security, idempotency, pagination, capabilities) had no home, so
every runtime improvised it and the transport axis of the target
profiles (`transport.http`) had no semantics to resolve against. The
`generate.openapi` capability is defined in the embedded capability
registry but has no input contract to consume.

## Decision

### 1. One independent attachment family, not a Model change

Transport semantics live in one closed, versioned attachment
(`lekalo/transport-http/v0.4.0`, identity
`dev.lekalo.transport-http@0.4.0`) following the established
attachment pattern (query model, storage projection, error contract):
bound to one project and one exact Model/IR pin, closed under
`additionalProperties: false`, canonical bytes compact with
byte-sorted keys. The family name is `transport-http` — not
`transport` — because `transport` already names the target-profile
axis and the confined process primitive (`target_protocol::transport`).
The name leaves `transport-grpc` room for the `grpc-proto` component.

### 2. The Model stays single-sourced; the attachment owns the rest

The Model `endpoint` symbol keeps `method`, `path`, and `invokes`;
the attachment references the symbol by semantic id and owns
everything else: parameter bindings, request/response schemas, the
error-to-status projection, security schemes, idempotency/correlation
headers, pagination projection, rate-limit/cache metadata, content
versioning, and declared capabilities. Validation joins the two:
path-template segments must match the declared path parameters
exactly in both directions, parameter/body fields must resolve to
declared operation inputs (command input members or query-model
parameters), and endpoint refs must resolve to `endpoint` symbols.

### 3. Errors project the #62 identity; statuses are projections

Every error entry keys on the error's immutable semantic identity and
carries one status. The wire body is always the canonical quadruple
`{"ok":false,"error":{"id","code","category","payload"}}` with public
payload fields only — a domain error ID survives independently of the
HTTP status. Category defaults (`validation`/`auth`/`conflict`/
`not-found`/`domain`/`infrastructure`) are declared per endpoint so
the four error classes stay distinguishable on the wire; undeclared
infrastructure failures render the fixed generic body (category
`infrastructure`, no declared id/code) and never masquerade as a
declared error. There is no catch-all: entries outside the
operation's declared #62 union refuse, and unmapped union members
refuse under the strict profile (`transport.mapping-missing`).

### 4. Security is explicit declaration, never ambient

Schemes are declared once (`bearer`, `api-key`, `basic`, `oauth2`,
`mutual-tls`, `custom`) with no secrets, URLs, or middleware names.
Per endpoint, `auth` projects the #25 actor vocabulary: `public`
carries no scheme, every authenticated actor carries at least one,
and the optional `policyRef` resolves to a Model policy symbol.

### 5. Capabilities are closed per-endpoint declarations

Streaming/upload/download are declared per endpoint with a closed
detail vocabulary (`sse|chunked|ws`, `multipart`, `binary`) and a
`minimumSupport` checked against the resolved target profile's
capability map (`transport.streaming`, `transport.upload`,
`transport.download`). An unsatisfied declaration is an explicit
refusal (`transport.capability-unsatisfied`) — never a silent
downgrade and never a surprise at runtime.

### 6. One canonical projection feeds every runtime

`transport_http::project(attachment, namespace)` derives one
canonical route surface per closed namespace (`node`, `laravel`,
`go`, `rust`) from the single attachment — pure declaration data,
byte-stable. The same canonical endpoint surface is the input every
OpenAPI rendering consumes (issue #46): parity across runtimes is
provable by byte comparison because every runtime renders the same
canonical surface.

### 7. Wire compatibility is classified, and breaking blocks

`transport_http::compare` classifies every changed path of two
same-family attachments as breaking (endpoint/param/error/status/
security removal, required param added, capability removed, wire
dialect bump), non-breaking (additions under the evolution policy),
or policy-change (rate-limit/cache/idempotency/security-strengthening
changes). The verdict stays data; a `wire-consumer` strict profile
turns any breaking classification into a blocked diff.

### 8. Canonical home and adapter evidence

One project-level canonical document `lekalo/transport.yaml` (the
`authorization.yaml` precedent — transport spans modules and is
target-agnostic). `lekalo generate` validates it as a preflight and
writes canonical evidence under `.lekalo/cache/transport/<project>.json`
inside the existing `lekalo.cache` authority home — the same
treatment the IR evidence gets — and adapters read it through their
declared read scopes. HTTP implementations may be generated (the
Node adapter's transport extension claims `generate` with
`generate.transport-http`) or maintained (observed `exposes` bindings
verify method/path today).

## Consequences

- The diagnostic registry gains the `transport.*` family
  (`LEK-TRN-001..009`); the embedded target-capability registry gains
  `generate.transport-http` and `verify.transport-http`; the
  component registry's `http-json` gains `transport.upload` and
  `transport.download` capability ids so it can state partial support
  honestly.
- The `rust` projection namespace ships now without a Rust runtime
  component: contract work is target-agnostic (the `laravel` storage
  namespace already exists with no PHP adapter); a `rust-*` runtime
  component is a separate reviewed registry increment.
- OpenAPI rendering stays with issue #46; this issue ships the
  canonical endpoint surface OpenAPI needs (operationId, params with
  styles, request/response schemas, error map, security, tags).

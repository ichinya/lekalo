# PHP Laravel operations (issue #59)

The operations family generates or checks explicit PHP Laravel use-case
classes — command/query handlers with constructor-injected typed
dependencies, one public `handle` entrypoint, narrow per-operation ports,
and typed error classes. No controller, model, or observer carries the
use-case algorithm; no facade or service locator appears in portable core.

## Input

Author `lekalo/operations/<project>.operations.json`
(`dev.lekalo.php-operations-input@0.4.0`; schema
`contracts/php-operations-input.schema.v0.4.0.json`). The document pins
the exact staged IR evidence digest and the digest of the bound
`lekalo/types/<project>.types.json`; the #58 types family is a required
dependency and the composed generate run plans missing managed types and
operations in one authorized plan.

## Modes

Per operation record:

- `managed` — generated regenerable bytes under
  `.lekalo/generated/php-laravel/operations/` (namespace
  `Lekalo\Generated\Operations`). Requires a closed recipe:
  `port-delegation` (one typed port call carries the maintained body) or
  `single-entity-update` (typed read-by-key, typed preconditions, full
  explicit entity rebuild, save, declared event emissions, inside the
  declared transaction). Operands are closed: `fromInput`, `fromEntity`,
  `enumCase`, `literal`.
- `scaffold-once` — the same signature emitted once under
  `app/lekalo-operations` (namespace `App\LekaloOperations`), body the
  explicit unimplemented failure; the sidecar `operations.map.json` is
  the marker and the bytes are user-owned afterwards.
- `checked` — no writes; the declared `entry` (FQN/method/path) joins
  against `.lekalo/import/observed/operations-evidence.json`
  (`dev.lekalo.php-operations-evidence@0.4.0`).
- `custom` — maintained body, same shape join, no business-body
  rewriting.

Queries accept only `port-delegation`; a write recipe on a query is the
typed `operations.query-write` finding with a zero-write plan.

## Errors and policy

Declared `errors` must equal the #62 error-registry binding of the
operation exactly; every declared id gets one typed class
(`<Module>\Errors\<Name>Error`) extending the shared `OperationError`.
The optional `policy` binding must name an IR policy applying to the
operation; it becomes one narrow `authorize(input, actor)` port.
`transaction` is `required` (one `TransactionPort` run wraps the body)
or `forbidden` (queries are forbidden by default).

## Verification

`lekalo generate` runs the core join first (IR + registry + digests +
recipes; see `crates/lekalo-core/src/php_operations/`), then the adapter
re-validates and emits. `verify` reports managed `operations.drift`,
scaffold `operations.scaffold-missing`, and the checked/custom
`operations.binding-*` findings. The emitted classes syntax-check and
load under `php -n` through the deterministic `classmap.php` — the
loading authority, with no runtime registration magic.

The Laravel boundary (connections, queue visibility, auth principal
binding, PostgreSQL behavior) is maintained application code and is not
claimed by this family.

# External runtimes and portable rules

Status: **Implemented** opt-in component contract checks for issue #241.
Owner: runtime protocol maintainers.

A runtime is an external process. A rules package is an independent contract
artifact. The component configuration selects both; core does not resolve a
language name, framework name or built-in target profile on this path. Adding
an unknown runtime does not require rebuilding Lekalo.

## Configure and run

```sh
lekalo init --runtime gh/example/lekalo-zen --rules gh/example/service-contracts --component-path src
lekalo runtime lock
lekalo runtime check
```

The coordinates above are placeholders for repositories you control, not
published example packages. Plain `init` creates the starter Model and editable
`lekalo.yaml`. With `--runtime` it creates just the component configuration. `runtime lock`
fetches declared packages and executes their confined `describe` operation.
An existing config is never overwritten. `lekalo runtime init` creates an empty
configuration to edit; lock/check refuse it until a component or service is declared.

```yaml
schema: lekalo/runtime-config/v1
components:
  api:
    path: api/src
    runtime: gh/example/lekalo-zen#v1.0.0
    rules:
      - gh/example/http-contracts#v1.0.0
  web:
    path: web/src
    runtime:
      command: [lekalo-custom-runtime]
    rules:
      - "path:../http-contracts"
```

Coordinates are strings: `gh/OWNER/REPO[#REF]`, `git+https://HOST/REPO.git[#REF]`,
`git+file:///ABSOLUTE/REPO[#REF]`, or `path:DIRECTORY`. A command is an argv array,
never shell text. `--runtime exec:PROGRAM` is the one-program shorthand.
Git references are pinned to a commit; uncommitted worktree files are ignored.
Set `target_version: "8.0"` on a component (or pass `init --target-version 8.0`)
to request the target language version. The selected runtime must advertise
`target.version` and answer with the requested version and `supported: true`.
The PHP and Python readers compare the request with their interpreter version.

Component paths are project-relative and must not overlap or escape the root.
The current scanner sends bounded UTF-8 files below each component, with
relative paths and SHA-256 digests. Use source directories rather than dependency
or build directories. Dot entries are excluded. Symbolic links, binary files,
excess depth, entry count, file size or total size cause refusal.

`lekalo.runtime.lock` pins the configuration, package contents, runtime executable,
entry script, description and selected rules. Packages live outside the project,
under `LEKALO_PACKAGE_HOME` or the user's cache. `runtime check`, `validate`, and
`runtime lock --offline` never fetch or update packages. Edited executable/package
bytes or a changed configuration refuse validation. Copy the config and v2 lock
to a new checkout and run `lekalo runtime restore` to populate its cache from
the locked Git commits. The executable and script digests must still match.
Absolute paths explicitly declared by the project retain their meaning. Older
v1 locks can still be checked on their original machine. An explicit online
`runtime lock` writes the new format.

`lekalo validate --strict` runs component checks when the selected root has
`lekalo.yaml`. If `lekalo/project.yaml` also exists, its existing Model validation
runs first; failure cannot be masked by runtime results. Model-specific module,
module-API and CI-report flags currently refuse this combined mode. Other legacy
commands retain their existing target protocol. `runtime check --component ID`
selects one component, after checking integrity of the whole composition.

Named [external services](external-services.md) use the same resolver and lock for discovery, requirements and trace mapping, independently of component runtimes and rules.

## Package contract

Each Git/local package root contains `lekalo.package.json`:

```json
{
  "schema": "lekalo/package/v1",
  "id": "example-runtime",
  "version": "1.0.0",
  "kind": "runtime",
  "files": ["main.mjs", "lib/parser.mjs"],
  "command": ["node", "main.mjs"]
}
```

For a rules package use `kind: "rules"`, omit `command`, set `rules: "rules.json"`,
and list `rules.json` in `files`. Package versions are semver. Runtime identity
and version in its handshake must match the manifest. Declared files are read
without following links, hashed and copied into the external cache. Git admission
reads committed blobs and does not check out or execute package build hooks.

Runtime package files are staged together in a private, read-only runtime tree,
so relative imports and data files work without granting access to the original
repository. A native executable can itself be a declared file. An interpreter
must be installed locally; its first argument can be the declared script entry.
A directly configured command supports an installed executable and an optional
first-argument entry script; use a package for additional code/data resources.
There is no automatic package-manager install, compilation or interpreter install.

## Process protocol

The schema is [external-runtime.schema.v1.json](../contracts/external-runtime.schema.v1.json).
Each process receives one JSON object on stdin and must write exactly one JSON
object on stdout before exiting zero. Stderr is not evidence. The host owns
bounded input/output, deadlines, an empty project view and OS confinement; there
is no fallback to an unconfined launch. Rules and source content arrive as data.

```json
{
  "protocol": "lekalo/runtime/v1",
  "request_id": "sha256:<host-generated-digest>",
  "operation": "describe",
  "input": {}
}
```

The response echoes `protocol` and `request_id` exactly and sets `result`:

```json
{
  "id": "example-runtime",
  "version": "1.0.0",
  "rules_api": ["lekalo/contract-rules/v1"],
  "capabilities": ["http.routes/v1"],
  "operations": ["check"]
}
```

For `operation: "check"`, `input` contains `component`, `files` and `rules`.
Each file has `path`, `digest`, `content`; each rules entry has `id`, `version`,
`digest` and the decoded `rules` document. A successful result has
`state: "complete"` and `facts: {"package-id": {"check-id": <JSON value>}}`.
Every selected package/check must be present exactly once. Missing, extra,
partial or malformed output fails; it is never interpreted as a pass. The
runtime description and source snapshot are checked for changes during a run.

## Portable contract rules

```json
{
  "schema": "lekalo/contract-rules/v1",
  "requires": ["http.routes/v1"],
  "checks": [{
    "id": "health-route",
    "schema": {
      "type": "object",
      "required": ["routes"],
      "properties": {
        "routes": {
          "type": "array",
          "contains": {
            "type": "object",
            "required": ["method", "path"],
            "properties": {
              "method": {"const": "GET"},
              "path": {"const": "/health"}
            }
          }
        }
      }
    }
  }]
}
```

Core compares `requires` with advertised runtime capabilities and validates each
returned fact against its check's JSON Schema Draft 2020-12. References must be
local (`#` or `#/...`); external references and `$id` are rejected. Unknown APIs,
missing capabilities and invalid schemas fail at lock time. Invalid facts fail
at check time with registered `runtime.rule-failed` diagnostics. Process/config
failures use `runtime.refused`; incompatible APIs/capabilities use
`runtime.incompatible` (exit 5). Other failures exit 1; success exits 0. Standard
Lekalo diagnostic normalization and stdout/stderr selection apply.

Capability names define shared semantics between package authors. A PHP runtime
and a Zen runtime can produce the same neutral HTTP facts and consume identical
rule bytes. Core does not branch on their identities. The host validates the
reported evidence; the runtime remains responsible for deriving honest, complete
facts from its source language. This is not a proof of application behavior.
PHP callbacks or framework syntax alone are not portable contract rules.

## Public HTTP rules

`gh/ichinya/lekalo-php` and `gh/ichinya/lekalo-php-laravel` publish v1 package
manifests. The Laravel package requires `http.routes/v1`. Its `health-route` rule
requires a `GET /health` entry in the returned `routes` array. The optional
`bindings` object supplies reader-specific data. PHP uses its static router
binding to read direct, literal routes in `web.php` or `routes/web.php` relative
to the component. An independent runtime can derive the same facts from its own
language. Changing the selected contract package changes its digest.

## Independent projects in a workspace

Keep each child's `lekalo.yaml` and `lekalo.runtime.lock` with the child.
At the common parent, create `lekalo.workspace.yaml`:

```yaml
schema: lekalo/workspace/v1
children:
  api: pushhard-api
  web: pushhard-web
```

In `pushhard-api/lekalo.workspace.yaml`, select the checked fact to publish:

```yaml
schema: lekalo/workspace/v1
exports:
  routes:
    kind: runtime-fact
    component: app
    package: lekalo-php-laravel
    rule: health-route
```

From the parent, print the candidate contract and review it before accepting it
in the web repository. This example uses an existing `pushhard-web/contracts` directory:

```sh
lekalo --json workspace export routes --project pushhard-api > candidate.json &&
  cp candidate.json pushhard-web/contracts/routes.json
jq -r .digest pushhard-web/contracts/routes.json
```

In `pushhard-web/lekalo.workspace.yaml`, replace the placeholder with that digest:

```yaml
schema: lekalo/workspace/v1
imports:
  routes:
    from: api
    export: routes
    bundle: contracts/routes.json
    digest: "sha256:<64 hex digits from the reviewed bundle>"
```

`lekalo validate --strict --project pushhard-web` verifies the accepted bundle
without a producer checkout. From the parent, `lekalo validate --strict` also
checks each declared child against its own lock and compares consumed exports.
A public route change reports the consumer, import and producer. A private edit
that leaves the canonical route list intact keeps the pin valid. A broken
producer reports an unavailable export. Updating a pin is an explicit consumer
change. This version supports one parent level and up to 32 children.

For a Model module, declare an export with `kind: module-api`, `api:
module-api.json` and `module: tasks`. It uses the same certified public bundle
and digest as `lekalo module api export`. Model validation with
`--module-api` and `--module-api-lock` also checks configured runtimes.

The PHP target-protocol adapter retains its scan/generate/verify interface in
the external packages. Other language providers are at different stages of
migration. Discovery, requirements and external trace mappings use selected
process services. See [external services](external-services.md).

Run the process and workspace examples against a built CLI:

```sh
node scripts/external-tool.mjs --provision-php
node --test scripts/test-external-runtime.mjs scripts/test-portable-laravel.mjs
```


## Official packages

| Runtime | Evidence and independent rules |
| --- | --- |
| `gh/ichinya/lekalo-go`, `gh/ichinya/lekalo-rust` | Tree-sitter source declarations and rule-supplied syntax queries |
| `gh/ichinya/lekalo-typescript` | TypeScript and TSX declarations and queries, Hono routes, exported Drizzle tables and Zod schemas |
| `gh/ichinya/lekalo-python` | Python AST declarations, direct FastAPI routes and FastMCP tools |
| `gh/ichinya/lekalo-php` | Static Laravel route registrations and direct Symfony controller route attributes |

Select framework rules by their repository coordinate, e.g. `gh/ichinya/lekalo-python-fastapi`,
`gh/ichinya/lekalo-php-symfony` or `gh/ichinya/lekalo-typescript-drizzle`.
The PHP and Python runtimes use an installed interpreter. The Go, Rust and
TypeScript syntax readers use Node and packaged WebAssembly grammars. Each
package README documents the syntax covered by its evidence.

A package may mount pure data or code libraries using
`"libraries": {"parser": "gh/owner/library#<40-hex-commit>"}`.
The library declares `"kind": "library"` with its files at its root. The host
pins and verifies the whole graph and mounts the reviewed files under the named
directory in the sandbox. Remote packages can only reference remote libraries.

# External discovery, requirements and trace services

Status: **Implemented** process boundary for issue #241. Owner: runtime protocol maintainers.

Services use the same package resolver, process confinement, request correlation,
limits and lock verification as [external runtimes](external-runtimes.md). Core
has no list of permitted discovery languages or service package repositories.
Adding a Zen discovery executable requires only a configuration entry.

```yaml
schema: lekalo/runtime-config/v1
components: {}
services:
  discovery/go:
    command: [lekalo-go-discovery]
  discovery/rust:
    command: [lekalo-rust-discovery]
  discovery/typescript:
    command: [lekalo-typescript-discovery]
  discovery/python:
    command: [lekalo-python-discovery]
  requirements:
    command: [lekalo-openspec]
  trace:
    command: [lekalo-hlv]
```

Run `lekalo runtime lock` after installing the selected executables, then use
`lekalo init --adopt --project . --dry-run`, `lekalo requirements report ...`,
or `lekalo trace assess ... --evidence ...`. Component declarations may coexist
with services. An otherwise empty config requires at least one service to lock.
`runtime check` verifies service integrity; each consuming command validates its
own service results. It does not run every service's domain check automatically.

Service sources accept installed commands, `path:...`, `gh/OWNER/REPO#REF` and the
other runtime coordinates. Repository sources must contain a valid runtime
`lekalo.package.json` and the runnable files it declares. Source-only Rust
repositories are not compiled by the product resolver. Build/install these six
executables separately, or publish a package containing a prebuilt executable.
For example, `cargo install --locked --path crates/discovery --root /your/tools`
from a language repository installs its discovery executable. HLV/OpenSpec use
`--path .`. Add `/your/tools/bin` to PATH before locking. Cargo's installed copy
is a regular file; its hardlinked `target/debug` output is not an installation.

Service keys `discovery` and `discovery/NAME` select discovery providers;
`requirements` and `trace` each select exactly one provider. There may be at most
32 selections. A service-only description can have `rules_api: []`; component
runtimes still have to implement the portable rules/check contract. Unknown
operations, missing services required by a consumer, invalid responses, stale
locks and altered executables fail closed. No package fallback is compiled in.

The requirements command selects services from its project root. Trace assessment
currently selects from the process working directory. Discovery with explicit
`--project` uses that root; ancestor probing uses the invocation directory's
selection. Parent/child service inheritance is not implemented in this change.

## Data contracts

The envelope is `lekalo/runtime/v1`. Operation names are negotiated in `describe`.
Data schemas are published in
[external-services.schema.v1.json](../contracts/external-services.schema.v1.json).
Responses must have the expected schema and no unknown fields.

* **Discovery** advertises filenames and the subset it needs to read through
  `discovery.describe`. `discovery.inspect` receives only admitted relative paths,
  optional UTF-8 content and generic decoded JSON/YAML values. It returns manifest,
  language/framework, workspace, module, name and proposed gate evidence. Paths
  must refer to disclosed files/directories; commands are proposals, never executed.
  Host scope exclusions and no-follow reads remain authoritative. Content is bounded
  to 4 MiB per provider; the combined response is limited to 10,000 rows.
* **Requirements** calls `requirements.snapshot` with the declared source/root and
  a map of supplied reads. A provider can return `read` requests for directory
  entries or files, followed by `complete`, `absent` or `invalid`. The host performs
  only reads inside that declared root, without following links. The replay loop
  permits 1,024 rounds, 4,096 reads and 4 MiB of supplied data; each file is at most
  1 MiB. Catalog identifiers, digests, sorted uniqueness and file provenance are
  checked before the existing report/trace pipeline consumes them. OpenSpec parsing,
  title-to-ID policy, changes, conflicts and rename candidates belong to its package.
* **Trace** calls `trace.mappings` with opaque mapping IDs, required systems,
  revisions and aliases. Every requested ID must receive exactly one accepted
  state. HLV owns its ambiguity/staleness rules. Core retains graph validation,
  evidence admission and diagnostics; the bridge does not execute an HLV tracer.

These are service extensions around existing adoption/requirements/trace formats.
The older requirement ID grammar and trace system vocabulary remain bounded host
contracts. This extraction does not make every legacy format extensible.

## Development and CI

`external-tools.lock.json` pins the owner repositories and service build targets.
It is integration tooling, not a product language registry. Run:

```sh
node scripts/external-tool.mjs --provision
node scripts/external-tool.mjs --build-services
export LEKALO_SERVICES="$(node --input-type=module -e 'import {serviceEnvironment} from "./scripts/lib/external-services.mjs"; console.log(serviceEnvironment("target"))')"
cargo test --locked -p lekalo-core --test requirements --test trace_assessment --test init_adopt_boundary
```

The explicit `LEKALO_SERVICES` JSON Source map is for development/embedding and
bypasses project selection/lock. Production projects should use `lekalo.yaml` and
its lock. CI provisions exact commits, builds separate executables with debug
information and incremental compilation disabled, then includes them in the
existing digest-checked runtime artifact. Consumer jobs reuse that artifact.
Core's Cargo graph contains none of these six packages. Their library tests run
in the owner repositories; host process and confinement tests remain here.

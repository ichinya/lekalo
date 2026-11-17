# External implementation ownership

The source repositories own runtime and integration implementations. The core
owns process supervision, protocol admission, confined filesystem capabilities,
the model, generic trace/requirements reports, and public contract comparison.

| Repository | Maintained implementation | Transitional core supply |
| --- | --- | --- |
| `ichinya/lekalo-php` | PHP language runtime | `adapters/php-laravel/vendor/lekalo-php` |
| `ichinya/lekalo-php-laravel` | Laravel provider | `adapters/php-laravel/vendor/lekalo-php-laravel` |
| `ichinya/lekalo-python` | Python language scanning and process entry | `adapters/python` |
| `ichinya/lekalo-python-fastapi` | FastAPI registration analysis | pinned dependency of `lekalo-python` |
| `ichinya/lekalo-typescript` | TypeScript runtime, compiler client, generators and existing extensions | `adapters/node-typescript` |
| `ichinya/lekalo-openspec` | OpenSpec parsing, catalog projection and conflict semantics | `packages/lekalo-openspec` |
| `ichinya/lekalo-hlv` | HLV revision mapping policy | `packages/lekalo-hlv` |

Source snapshots are generated dependencies, not a second implementation to edit.
`external-packages.lock.json` records source repository, exact commit, source
path and SHA-256 for every supplied file. PHP retains its existing package lock.
The protocol SDK stays in `sdk/rust`; external Rust runtimes vendor it with a
source receipt. Framework packages do not import private runtime symbol types.

## Updating a snapshot

1. Change and test the owning package in its repository. Commit those changes.
2. If changing FastAPI, update the Python package's framework dependency and
   receipt, run its protocol tests, then commit that package too.
3. Run `node scripts/sync-external-packages.mjs --sync`. The source checkouts
   default to `../lekalo-adds`; `LEKALO_ADDS_DIR` can select another directory.
   Only committed Git blobs are copied. Existing modified snapshots refuse sync.
4. Run `node scripts/sync-external-packages.mjs --check` and the affected core
   conformance suites. Commit the generated snapshots and lock together.

CI check mode needs no source checkouts and no network. Source and dependency
commits must be published before the consuming core change is merged.

## PHP composition

PHP runtime and Laravel rules own separate `lekalo.package.json` build manifests.
`lekalo-php/build.php` can compose the runtime alone, or with explicit rules
directories, including third-party packages. The runtime defaults to target `php`;
the transitional core wrapper explicitly selects `php-laravel` to preserve its
published identity, generated paths and wire contract. The core no longer owns
the composition implementation or a second PHP module/resource list.

After committing package changes, run `node scripts/sync-php-packages.mjs --sync`,
then `php -n adapters/php-laravel/build.php`,
`node scripts/regen-php-adapter-manifest.mjs` and
`node scripts/update-reference-projects.mjs`. Sync reads committed Git blobs,
preflights every input before changing the snapshot, and refuses existing vendor
drift. Package manifests supply implementation paths; core adds the package's
build and test tools to the snapshot. Check mode remains offline.

The adapter gate runs package composition tests as well as protocol, analyzer,
framework and process suites. Single-provider families refuse overlapping claims;
the runner merge API refuses duplicate runners and conflicting defaults.
These PHP authoring manifests are separate from the public adapter manifest.
They do not implement dependency fetching, interpreter version selection,
per-component configuration, or parent workspaces.

## Scope of this migration

Published wire identities and the current delivery paths remain compatible.
OpenSpec receives only a confined read capability from core; HLV returns mapping
outcomes that core maps to registered diagnostics. A custom runtime/rules resolver,
component-scoped configuration, configurable requirement/trace processes, and
parent/child coordination are tracked in #197–#201, #239 and #241. This source
migration does not claim to implement those new interfaces.

The parent-workspace design uses explicit members with independent states and
locks. API contracts belong to the producer; consumers pin immutable public API
bundles and compare changes with the existing `module api` machinery. Parent
aggregation must not merge child state by overwriting one shared observed index.

The existing adapter-SDK contract gate checks source snapshots.
`ci/external-packages-workflow.yml` and the packages’ `ci/package-workflow.yml`
are optional, inactive workflow templates; installing them requires a GitHub
credential with workflow-write permission.

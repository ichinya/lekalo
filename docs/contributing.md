# Contributor guide

Run commands on this page from the repository root. Use the source revision and lockfiles you intend to verify. Install release binaries through the separate [installation guide](releases.md).

## Build from source

Install Git, Rust/Cargo and the platform's native C/C++ linker toolchain. The workspace minimum Rust version is `1.89.0`; CI checks it separately from stable Rust. On Windows, use the MSVC Rust target, Visual Studio C++ Build Tools and the Windows SDK. Contributor JavaScript gates use Node **24**.

```sh
cargo build -p lekalo-cli --locked
```

The binary is `target/debug/lekalo` on Linux/macOS or `target/debug/lekalo.exe` on Windows. Add the **absolute** `target/debug` directory to your current process's PATH before entering a fixture. Otherwise use the absolute executable path; a relative build path stops working when you change directories.

`--locked` preserves `Cargo.lock`; it may still download dependencies. Provision the cache before an offline run:

```sh
cargo fetch --locked
cargo build -p lekalo-cli --locked --offline
```

Cargo's [manifest options](https://doc.rust-lang.org/cargo/commands/cargo-build.html#manifest-options) distinguish locking from network isolation. Do not update lockfiles to make a verification command pass.

## Choose checks for the change

| Change | Starting checks | Prerequisites |
| --- | --- | --- |
| Prose or links | Documentation static checks below | Node 24 |
| Commands, schemas or protocols | Matching CLI, live ownership check and affected contract/runtime gates | Exact gate dependencies |
| Rust behavior | Targeted test, then required workspace checks | Rust and native linker |
| Adapter behavior | Adapter build, contract and host-conformance gates | Adapter locks and declared language toolchains |
| Native tutorial or database behavior | Its planner/MySQL/native lane | Fixture locks, PHP/Composer, database or pinned tools for that lane |

[CI](../.github/workflows/ci.yml) owns the executable matrix and toolchain pins. A portable documentation run does not qualify native planner execution, MySQL persistence, macOS behavior or another adapter.

For the Rust workspace:

```sh
cargo test --workspace --locked
```

Add `--offline` only after required dependencies are available. Record the source revision, failures, ignored tests and unavailable prerequisites. A process exiting successfully does not prove its skipped paths ran.

## Documentation changes

For prose and metadata validation without executing examples:

```sh
node scripts/test-docs-ownership.mjs --static
node scripts/test-docs-examples.mjs --static
```

Ownership checks validate reference owners, source digests and required page links. Example checks compare displayed commands, fixtures and outcomes with [the replay registry](../tests/fixtures/docs/examples.json). Static mode does not run those commands.

After building the matching CLI, verify live help and run the affected replay lane:

```sh
node scripts/test-docs-ownership.mjs
node scripts/test-docs-examples.mjs --lane portable
```

Planner and MySQL examples use `--lane planner` and `--lane mysql` after tutorial and CI setup. Missing required dependencies fail the lane. The registry's `sourceProduct` records the original example corpus, not the current product or a new execution result.

When commands, schemas or protocols change, explicitly refresh from the newly built CLI, review the diff and rerun verification:

```sh
node scripts/update-docs-owners.mjs --write
```

Do not run the writer with an old binary. Prose-only edits need no regeneration. [Documentation maintenance](README.md#documentation-maintenance) defines page responsibilities; [script maintenance](../scripts/README.md) distinguishes writers from verifiers.

## Dependencies and generated files

Provision dependencies before gates. The root `package-lock.json` pins Ajv for checks needing only that package:

```sh
npm ci --ignore-scripts --no-audit --no-fund
```

[`npm ci`](https://docs.npmjs.com/cli/commands/npm-ci) requires a matching
lockfile, recreates `node_modules` and does not update package manifests or
the lock. This setup suppresses install scripts; later explicit test/build
commands are separate operations.

Adapter and native fixture locks have separate dependency sets; follow their owner and CI setup. Root dependencies do not satisfy every fixture. If dependencies are provisioned externally, set `NODE_PATH` to the absolute installed `node_modules` directory: `export NODE_PATH=/absolute/path/node_modules` in Bash, or `$env:NODE_PATH = 'C:\absolute\path\node_modules'` in PowerShell. Do not copy Bash assignment syntax into PowerShell.

Keep build output, installed dependencies and temporary execution material under ignored homes such as `target/` and `node_modules/`. Published contracts, reviewed goldens and provenance manifests remain tracked inputs. Run generators as explicit authoring operations and review their bytes, hashes and controls.

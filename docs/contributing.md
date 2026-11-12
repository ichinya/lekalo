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

Automatic branch checks run on pull requests and pushes to `main`. A push to a
feature branch does not start a second copy of its PR checks. To check a branch
before opening a PR, select the workflow and branch under **Actions → Run workflow**.
The workflow must already be available on the default branch for manual dispatch.

| Event | Workflows |
| --- | --- |
| Pull request opened, reopened or updated | CI, Audit, MCP Unit and Reference projects; Linux confinement when its path filters match |
| Push to `main` | The same workflows, retaining Linux confinement's path filters |
| Push of a `v*` tag | CI, Audit, MCP Unit and Linux confinement, including the CI release-version guard |
| Manual dispatch | The selected workflow on the selected branch or tag |
| Weekly schedule | Audit |

Tag pushes do not use path filters. Reference projects has no automatic tag
trigger; release asset builds use the separate Release binaries workflow.
Fork PRs retain their GitHub-hosted runner routing and existing approval rules.

For the Rust workspace:

```sh
cargo test --workspace --locked
```

Add `--offline` only after required dependencies are available. Record the source revision, failures, ignored tests and unavailable prerequisites. A process exiting successfully does not prove its skipped paths ran.

CI runs the root Rust workspace as non-overlapping groups. The dedicated Unix
confinement, Unix transport and adapter containment steps retain their existing
platform conditions, serial execution and time limits. The remaining groups are
selected by `scripts/ci-rust-tests.mjs` from Cargo metadata: CLI defaults, core
unit tests, the other integration targets, doctests and examples. New integration
targets are included automatically; unclassified workspace members, target kinds
or conditional targets fail instead of silently losing coverage.

Use `node scripts/ci-rust-tests.mjs --plan` to inspect group ownership and
`node scripts/ci-rust-tests.mjs --check-coverage` to compare the complete
workspace test listing with the combined groups, including qualification.
The latter compiles test targets but only lists their cases. Running the script
without an option executes only the remaining groups; it does not replace the
three dedicated qualification steps. Local `cargo test --workspace --locked`
still runs the complete default suite directly.

Only failure diagnostics may repeat a root test group. The Unix transport
test's child watchdog processes and tests of SDK packages extracted into fresh
projects are intentional scenarios, not duplicate CI groups. Release-native
gates continue to use the release CLI; direct debug-CLI conformance calls do not
set a global `LEKALO_BIN`. The local Unix/WSL compile gate remains available as
`node scripts/check-unix-compile.mjs`; CI already compiles those paths natively
on its Linux/macOS legs.

CI disables dev/test debug symbols to avoid producing large PDBs for every
Windows test executable. Debug assertions and integer overflow checks remain
enabled explicitly; release settings are unaffected. Local builds keep their
normal debugging information unless the same `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_TEST_DEBUG=0` environment settings are selected. The SDK vector
gate logs its existing phase trace in CI so a long build or lifecycle probe is
visible without changing its fresh-target builds, corpus or time limits.

The release CLI and production native gates run in the independent
`native-execution` job on the same three OS images. That lane provisions its
own Node compiler, Ajv and Linux confinement prerequisites; it retains the
same `native-acceptance-*` artifacts and cleanup. Ordinary debug-CLI gates no
longer wait for the release build. All gate commands and budgets are retained;
this split does not claim that the complete CI now meets a time target.

Contracts builds the workspace CLI once in `contracts-rust`. The four
`target_protocol_conformance` unit tests run in the ordinary `core-lib` group
on Linux, Windows and macOS; the producer does not compile that unit-test
executable a second time. Both Node 18 and Node 24 consumers receive that run's
Linux/x64 CLI artifact. Preparation uses `--strip` to remove unneeded symbols
from the artifact copy only; Cargo's original executable is unchanged. The
manifest records the copied executable after stripping. Before executing
probes, `scripts/ci-contracts-cli.mjs` checks
the actual checkout SHA, runtime, binary size and SHA-256, then restores its
executable permission. A missing or mismatched artifact fails the job; consumers
do not fall back to a second Cargo build. This artifact is never used for SDK
oracle builds or the release-native gates.

On trusted self-hosted workers only, the producer keeps its Cargo target under
the worker's persistent `_work/.lekalo-contracts-target` directory, outside
the checkout cleaned by Actions. Cargo still checks freshness on every run;
no source timestamps or compiler fingerprints are bypassed. Hosted fork jobs
use their normal Rust cache and the same producer/consumer checks. See the
[runner maintenance notes](../.github/runners/videos5/README.md) for resource
limits and the distinction between dependency and build caches.

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

Adapter and native fixture locks have separate dependency sets; follow their owner and CI setup. Before scanner/lint or workspace integration tests, provision the [TypeScript 7 runtime bundle](typescript-native-runtime.md#provisioning-and-distribution). Root dependencies do not satisfy every fixture. If dependencies are provisioned externally, set `NODE_PATH` to the absolute installed `node_modules` directory: `export NODE_PATH=/absolute/path/node_modules` in Bash, or `$env:NODE_PATH = 'C:\absolute\path\node_modules'` in PowerShell. Do not copy Bash assignment syntax into PowerShell.

Keep build output, installed dependencies and temporary execution material under ignored homes such as `target/` and `node_modules/`. Published contracts, reviewed goldens and provenance manifests remain tracked inputs. Run generators as explicit authoring operations and review their bytes, hashes and controls.

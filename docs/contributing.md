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

### Platform CI lanes

The local [platform workflow](../.github/workflows/ci-platform.yml) is called
once per Ubuntu 26.04, Windows and macOS image. Each call starts `build-debug`,
`build-release`, `rust` and `rust-process` independently. Debug consumers need only their own
OS's `build-debug`; `native-execution` needs only `build-release`.

| Owner | Gates and prerequisites |
| --- | --- |
| `cli-core` | Domain/governance, watch/search, privacy, goldens and CI reports; Node, PHP/Composer, planner vendor, compiler bundle and confinement |
| `node-typescript` | Adapters, scanner, framework bindings, generation, TypeScript native example and brownfield pilot; Node, PHP, compiler bundle and confinement |
| `php-laravel` | PHP, Laravel, task-links, planner and Node/PHP scenario parity; Node, PHP/Composer, planner vendor, compiler bundle and confinement |
| `python` | Standalone Rust adapter build/test, protocol, observed/public scan and FastAPI; Rust, Node, Python, compiler bundle and confinement |
| `sdk` | Client/adapter SDK, vectors, Go smoke, conformance and extracted packages; Rust, Go, Node, PHP, compiler bundle and confinement |
| `docs` | Portable and Laravel/Vue replay; Node, PHP/Composer, Go/gofmt, planner vendor, pinned Vue/TypeScript, compiler bundle and confinement |
| `mcp` | Stdio, real host SDK and installed extension; Node and pinned host/extension packages |
| `rust` | Core Rust groups and existing qualifications; Rust, Node, PHP, Go, planner vendor, compiler bundle and confinement |
| `rust-process` | CLI tests and Windows watch/boundary process fixtures, run sequentially by group; same test prerequisites as `rust`, on an independent runner |
| `native-execution` | Native plans, Node workspace tools and MVP workflow; release CLI, Node, compiler bundle and confinement |

The frozen [gate inventory](../.github/ci/platform-gates.json) retains the exact
baseline arguments, env, conditions, budgets and artifact settings.
`node scripts/check-ci-platform.mjs` checks that each gate has one owner and
that event/fork, unrelated jobs and release policies remain intact. Common
setup steps use YAML anchors; each job still provisions its own directories.

Runtime artifacts contain only the CLI and, for debug, the TypeScript native
contract example. The manifest binds the checkout SHA, workflow run, producer
attempt, OS, architecture, runner label, hosted `ImageOS` family, profile,
byte sizes and SHA-256 digests. A rolling label moving to another image family
requires a new producer; patch updates within that family remain compatible.
Runtime uploads use compression level 6 to reduce storage and repeated download
traffic without changing the executable bytes or the seven-day rerun window.
Consumers download by producer output artifact ID and validate every file
before installing executables. A missing or unsuitable artifact fails the job;
privacy, brownfield and SDK gates cannot hide it with a Cargo fallback in CI.
Standalone local invocations retain their build fallback.
Both privacy runtime gates reuse that debug CLI without invoking Cargo; a
missing debug executable fails even if a release binary is present.

Rust builds its own test targets independently. Python retains its standalone
adapter builds; SDK retains production-oracle, fresh-template and extracted
package builds. These test builds are intentionally separate from the shared
runtime producers. Cargo caches have separate lane keys; SDK/Python cache only
outside the target directory, so a cache cannot replace the verified host CLI.
No full Cargo target or Rust test executable is transferred.
The two Rust lanes compile their own workspace/test prerequisites and use
separate Cargo cache keys. This is test compilation, not an additional runtime
publisher. Neither Rust lane waits for producers or for the other Rust lane.

For a consumer-only rerun within seven days, rerun the individual job and retain
the successful producer's artifact ID, manifest digest and producer attempt.
Do not derive an artifact name from the consumer's current `run_attempt`.
Rerunning a producer creates a new attempt-qualified artifact; dependent jobs
use its new outputs. Expired artifacts require rerunning the producer. Check
the actual CI result after either operation: local handoff tests do not attest
GitHub's rerun scheduling. Compatible `lekalo-ci-reports-*` and
`native-acceptance-*` report prefixes and failure cleanup are retained.

Measure Windows CI from workflow creation through the last mandatory Windows
job, including queue, setup, transfer and post steps. Record full workflow
wall-clock and other OS results separately; execution times of parallel jobs
must not be added together or substituted for that measurement.

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

On Windows, the `windows-watch` group runs `watch::targets::tests::` with at most
two test threads;
the parallel core unit group excludes that same prefix. These Node fixture
tests keep their existing deadlines and assertions, without competing with
unrelated unit tests for process startup. Coverage listing rejects an empty
watch group and proves that the split neither omits nor repeats cases.
The `windows-boundaries` group similarly owns all `target_protocol_boundaries`
tests with two threads and their unchanged 10-second request deadlines.
Both Windows fixture groups run after the CLI group in `rust-process`, away
from parallel core unit/integration tests. Other operating systems retain
boundary tests in the ordinary integration group. Hosted stability still needs
CI confirmation; lower concurrency is not evidence of a reproduced root cause.

Use `node scripts/ci-rust-tests.mjs --plan` to inspect group ownership and
`node scripts/ci-rust-tests.mjs --check-coverage` to compare the complete
workspace test listing with the combined groups, including qualification.
The latter compiles test targets but only lists their cases. Running the script
without an option executes only the remaining groups; it does not replace the
three dedicated qualification steps. Local `cargo test --workspace --locked`
still runs the complete default suite directly.
CI uses `--lane=core` in `rust` and `--lane=process` in `rust-process`.
`--plan` may be combined with a lane to inspect its ownership. Coverage checks
must include all lanes: `--check-coverage` refuses a partial lane. Unit controls
check that the two lanes cover each group exactly once and propagate failures.

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

On trusted self-hosted workers, the producer uses the stable toolchain already
installed in the runner image, logs its Rust/Cargo versions and fails if it is
missing. It does not update or auto-install a compiler during artifact creation.
The ordinary three-OS build/test and Clippy jobs still select current stable;
MSRV stays separate. Hosted producers, including fork PRs, retain their stable
installer. Updating the trusted image remains the runner maintenance operation.

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

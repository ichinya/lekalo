# videos5 Linux runner pool

Four registrations, `videos5-lekalo-01` through `videos5-lekalo-04`, share the
custom label `videos5-lekalo`. PHP 8.3 and Composer 2 are installed in the
image, including `php-config` required by setup-php, so the visible setup
steps avoid cold release downloads. Each listener
has a separate Ubuntu 24.04
container, registration directory, workspace, tool cache and private Docker
daemon. No host Docker socket is mounted. The runner and its own daemon share
a network namespace, keeping service-container loopback ports local to that pair.

The pool has an aggregate ceiling of six logical CPUs and 16 GiB RAM. Each
runner/daemon pair has a separate four GiB RAM ceiling and up to two GiB of
the host's existing swap (eight GiB swap maximum for the pool). Rust builds use
one compiler job to keep Rust compilation within that ceiling. MemoryHigh equals
MemoryMax so reclaim throttling cannot stall the runner listener below its hard
budget. The container defaults retain line-table debug information; CI overrides
dev/test symbols to zero on every OS to reduce linking cost. Incremental state
remains disabled on this pool to bound compiler memory. Debug assertions,
overflow checks, tests and Clippy warnings remain enabled. Release profiles
keep their existing settings. See the
[Cargo profile documentation](https://doc.rust-lang.org/cargo/reference/profiles.html#debug).
Each worker retains its own Actions tool cache and Cargo registry beneath
`worker-NN/_work/.cache` across executor recreation. Node 24 is seeded into
the Actions cache from the identical Node binary/npm shipped in the image;
`setup-node` still selects and verifies the requested version normally.
The Mago installer may reuse an archive from the checksum-keyed tool cache;
the committed SHA-256 is always verified before extraction and the real-toolchain
gates always execute. Logs rotate at 10 MB with three files retained.
`lekalo-ci-runtime.service` starts the pool after Docker on boot.

The registry cache alone does not preserve compiled crates. Actions checkout
cleans ignored workspace output, including a default `target/`, before each
job. The Contracts Rust producer therefore sets its job-local `CARGO_TARGET_DIR`
to `worker-NN/_work/.lekalo-contracts-target`, beneath the existing worker mount
and outside the checkout. It is a sibling of the runner's writable `_temp`,
so it does not require write access to the precreated registry/tool-cache parent.
Each worker has its own directory; compiler
jobs and incremental limits are unchanged. Cargo build and protocol conformance
still execute and validate freshness every run. A toolchain or dependency
change can rebuild it; a persistent target does not guarantee a sub-20-second
build. SDK fresh-target builds and release builds do not use this directory.

Only this producer compiles Contracts Rust code. The Node 18/24 matrix downloads
its CLI artifact and verifies checkout SHA, size, digest and Linux/x64 runtime
before executing it. Trusted producer and consumers use the same Ubuntu 24.04
runner image; fork jobs all use the hosted Ubuntu image. No running worker or
Compose configuration needs to be changed for this workflow-local target path.

The pre-job hook restores the image's PHP CLI configuration before every job,
so Mago's extension disabling cannot leak into later contract/pilot jobs.
It is installed in the image, outside the checkout. It rejects
fork pull requests and unexpected repository/event payloads before job or service
containers start. The workflow routes fork PRs to GitHub-hosted runners and
retains Windows/macOS matrix jobs there. The complete `build-test` OS matrix
also remains GitHub-hosted: its Linux leg validates host AppArmor and native
process confinement, which must not change the shared videos5 host policy.
Release binaries retain their original GitHub-hosted OS images and ABI targets.

The post-job hook verifies the private daemon identity before cleaning exited
containers and unused volumes. Images and build cache are retained. DinD is
privileged and shares the host kernel; this is not a VM security boundary.

Runner registrations live under `/srv/github-runners/lekalo/worker-NN`.
Each daemon receives only that registration's `_work` directory, not credentials
or other workers' files. Tokens must be supplied at registration time and are
never stored in this source directory or Compose configuration.

# videos5 Linux runner pool

Four registrations, `videos5-lekalo-01` through `videos5-lekalo-04`, share the
custom label `videos5-lekalo`. Each listener has a separate Ubuntu 24.04
container, registration directory, workspace, tool cache and private Docker
daemon. No host Docker socket is mounted. The runner and its own daemon share
a network namespace, keeping service-container loopback ports local to that pair.

The pool has an aggregate ceiling of six logical CPUs and 16 GiB RAM. Each
runner/daemon pair has a separate four GiB memory ceiling; Rust builds use
one compiler job to keep Rust compilation within that ceiling. MemoryHigh equals
MemoryMax so reclaim throttling cannot stall the runner listener below its hard
budget. Dev/test builds retain line-table backtraces without full type debug
information and disable incremental state to bound compiler memory. This does
not disable debug assertions, overflow checks, tests or Clippy warnings. Release
profiles and GitHub-hosted build-test jobs keep their existing settings. See the
[Cargo profile documentation](https://doc.rust-lang.org/cargo/reference/profiles.html#debug).
Each worker retains its own Actions tool cache and Cargo registry beneath
`worker-NN/_work/.cache` across executor recreation. Node 24 is seeded into
the Actions cache from the identical Node binary/npm shipped in the image;
`setup-node` still selects and verifies the requested version normally.
Logs rotate at 10 MB with three files retained.
`lekalo-ci-runtime.service` starts the pool after Docker on boot.

The pre-job hook is installed in the image, outside the checkout. It rejects
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

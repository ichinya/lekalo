# lekalo-typescript-discovery

Package-owned discovery policy extracted from Lekalo's `init --adopt`.
Inputs are bytes or parsed values supplied by the host; this library has no
filesystem, environment, network, or process access. Manifest recognition and
conventional gate commands are observations, never execution authority.

```sh
cargo test --locked --manifest-path crates/discovery/Cargo.toml
```

The core supplies bounded, no-follow reads, validates member paths, applies
collection limits and maps observations to the existing public receipt.
The compatibility snapshot is pinned by repository, commit and per-file SHA-256.
This is not a new target runtime or a package resolver. Go/Rust service runtime
implementation remains in each repository's issue #1; TypeScript/Python runtimes
remain separate from these host-side adoption helpers.

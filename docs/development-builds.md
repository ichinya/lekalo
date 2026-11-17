# Local Rust builds and disk use

The workspace uses `debug = 0` and `incremental = false` for dev and test builds. This avoids full debug-symbol output and the incremental cache; debug assertions and overflow checks stay enabled. The tradeoff is less debugger/backtrace detail and potentially slower rebuilds after edits.

Use `cargo check -p lekalo-cli --locked` for type checking. Build only the binary or test target you need; avoid repeated `cargo test --workspace --all-targets` during small edits.

Keep the same toolchain, profile settings and target directory during one task. Mixing equivalent settings through different `RUSTFLAGS` values still creates distinct cached build variants. The repository profiles already disable debug information; do not additionally set `RUSTFLAGS=-Cdebuginfo=0` for routine builds.

To recover space, stop active builds before running `cargo clean`. It deletes generated build output, so the next build is cold. Avoid scheduled cleanup while agents or CI jobs are compiling. Different checkout-local `target` directories have independent caches; `CARGO_TARGET_DIR` can explicitly share a cache, but concurrent Cargo operations then contend on its build lock.

When detailed debugging is needed, opt in for that command:

```sh
CARGO_PROFILE_DEV_DEBUG=1 cargo build -p lekalo-cli --locked
```

For another repository that has not adopted these defaults:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test --locked
```

Official reference: [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html).

Runtime integrity acceptance checks that the tested CLI stays byte-for-byte unchanged. Do not rebuild that executable concurrently with the test. For a direct Node acceptance run during other builds, set `LEKALO_BIN` to a separately copied, immutable executable. The check compares SHA-256 digests so a mismatch produces bounded diagnostics.

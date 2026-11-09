# Public Python observed scan

Status: **Implemented** for the bounded, read-only Python source profile below.
Owner: Python adapter and observed-host maintainers. [#146](https://github.com/ichinya/lekalo/issues/146).
The executable acceptance recipe is
[`scripts/test-public-python-scan.mjs`](../scripts/test-public-python-scan.mjs).
It uses the actual checkout CLI and compiled Python adapter in a disposable
copy, including explicit binding confirmation and stale-approval refusal.

## Build and provision

Start in the repository root. Building the CLI requires the native Rust linker
toolchain; the standalone Python adapter requires **Rust 1.97 or newer**.
Node 24 runs the acceptance recipe. No Python interpreter, pip packages,
application dependencies, database or server are required: the adapter uses
the pinned Ruff parser and never imports or executes the scanned application.
It is a separate Cargo workspace, so a root workspace build does not build it.

Provision locked dependencies once while network access is available:

```sh
cargo fetch --locked
cargo fetch --manifest-path adapters/python/Cargo.toml --locked
```

After provisioning, both builds and the example can run offline:

```sh
cargo build -p lekalo-cli --locked --offline
cargo build --manifest-path adapters/python/Cargo.toml --locked --offline
node scripts/test-public-python-scan.mjs
```

The binaries are `target/debug/lekalo` and
`adapters/python/target/debug/lekalo-target-python` (`.exe` on Windows).
Use their absolute paths; do not substitute an older installed release.
The gate accepts `LEKALO_BIN` and `LEKALO_PYTHON_ADAPTER_BIN` overrides for
explicitly built binaries. It performs no provisioning or source generation.

## Run the same workflow manually

Copy only `pyproject.toml`, `profile.json` and `src/` from
[`tests/fixtures/python-scan/standalone`](../tests/fixtures/python-scan/standalone)
into a new disposable project directory. Run subsequent commands there.
Add the built CLI directory to PATH or replace `lekalo` with its absolute path.

```text
lekalo init --adopt --target python --project-id python_shop --project .
lekalo module new shop --project .
```

Create `shop` only if adoption did not already create it. Copy the authored
[`model/entities.yaml`](../tests/fixtures/python-scan/model/entities.yaml) to
`lekalo/modules/shop/entities.yaml`. This small semantic model is an explicit
example-author decision. Scan does not infer or write canonical business
semantics, and the binding exercise does not prove runtime equivalence.

Pass the compact profile as one adapter argument. In PowerShell, substituting
the absolute adapter path:

```powershell
$pythonScanProfile = Get-Content -LiteralPath profile.json -Raw | ConvertFrom-Json | ConvertTo-Json -Depth 20 -Compress
lekalo scan --target python --profile standalone --project . -- C:/path/to/lekalo/adapters/python/target/debug/lekalo-target-python.exe --lekalo-project-profile-json $pythonScanProfile
```

In a POSIX shell:

```sh
python_scan_profile=$(node -e 'process.stdout.write(JSON.stringify(JSON.parse(require("node:fs").readFileSync("profile.json", "utf8"))))')
lekalo scan --target python --profile standalone --project . -- /path/to/lekalo/adapters/python/target/debug/lekalo-target-python --lekalo-project-profile-json "$python_scan_profile"
```

The profile grants only `pyproject.toml` and `src/**`. It is a trusted launch
input; the adapter cannot expand its own read roots. The scanner indexes
`.py`/`.pyi` modules, functions, async functions, classes, methods and argument/
return annotations. Stable native IDs, source digests and signatures are
evidence, with explicit partial/unknown states for unresolved or dynamic code.
Framework routes, application execution and source generation are outside this
generic Python profile. Protocol `1.1.0` preserves bounded native declaration
forests and type uncertainty; a successful scan does not make unknowns complete.

Review proposals, then confirm the actual proposal ID for `shop.orders.total`:

```text
lekalo bindings propose --project .
lekalo bindings confirm prop-<actual-id> --project .
lekalo bindings audit --project .
lekalo validate --strict --project .
lekalo observe inspect shop.orders.total --project .
lekalo inspect shop.orders.total --project .
lekalo impact shop.orders.total --project .
lekalo context shop.orders.total --budget 4000 --project .
```

All commands exit 0 in the unchanged example. `--json` emits machine-readable
receipts. Confirmation changes observed binding evidence, not source ownership
or the authored Model. Repeated unchanged scans preserve revision and index
bytes; source/config bytes remain unchanged throughout this positive workflow.

Change `tax: bool = True` to `tax: bool = True, precision: int = 2` in
the `total` parameter list in `src/shop/orders.py`.
`bindings audit` now exits 1. Rescan and review the fresh proposal; scan never
silently reconfirms the old fact. Editing source again after that review makes
confirmation refuse with `bindings.plan-mismatch` and preserves the accepted
index. Rescan, review and explicitly confirm again. These refusal and recovery
steps are exercised by the acceptance recipe.

A body-only edit also fails audit before rescan, but unchanged signature
evidence can become current again after rescan. A rename produces a new native
identity and proposal; it never silently redirects the old confirmed binding.

## Conformance and evidence limits

For explicitly enabled HTTP registration evidence, see the separate
[FastAPI provider guide](public-fastapi-scan.md). Generic scanning keeps its
existing protocol and declaration-only behavior.

From the disposable project, using the same adapter/profile arguments as above:

```text
lekalo adapter test --report json --repeats 2 -- /absolute/path/to/lekalo-target-python --lekalo-project-profile-json <compact-profile-json>
```

Use the `.exe` filename on Windows. The JSON report must have `verdict: pass`
and zero failed checks. Generation, storage and other undeclared capabilities
remain explicit skips; a default-profile pass is not an all-capabilities badge.
The acceptance gate separately proves scan determinism, real host admission,
source preservation, explicit binding and stale-approval handling on each OS.

CI runs the same gate after the adapter unit/protocol tests on Linux, Windows
and macOS. A missing or failing platform lane does not qualify that platform.
See [bindings](bindings.md), [observed mode](observed-mode.md) and
[confinement](adapter-confinement.md) for the canonical workflow and boundaries.

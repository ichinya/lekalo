# Lekalo CLI foundation

Issue #3 introduces a target-neutral Rust core and the `lekalo` command-line
front end. The workspace is edition 2021, uses Cargo resolver 2, has an exact
MSRV of Rust 1.80.0, and carries product candidate version 0.1.6. The product
version is independent of every contract or model schema version.

The core crate owns the result contracts, the issue #7 loader
(YAML/JSON frontends, imports, normalization, canonical output), and the
issue #8 typed IR. The CLI crate
owns syntax and presentation through clap and serde_json. The foundation
stubs neither read the filesystem nor build IR, invoke a target, or contact
a provider; the loader and IR are pure: the loader is the one capability
with filesystem access and it never writes.

## Commands

```text
lekalo --version
lekalo load [--project DIR] [--spans] [--ir]
lekalo validate
lekalo inspect SYMBOL
lekalo impact SYMBOL
lekalo context SYMBOL --budget TOKENS
```

`--version` and `load` are implemented. The remaining three subcommands are
recognized stubs: valid syntax reaches the named capability and returns
`unsupported`. `SYMBOL` is an opaque string at this layer, and `TOKENS` is an
unsigned integer. Semantic ID rules, validation, graph construction, and
real inspect, impact, or context behavior belong to later issues.
When those capabilities are implemented they are bound by
dev.lekalo.semantic-ids@0.1.0 to use the validated semantic ID verbatim as
their canonical key; the implemented loader already consumes those IDs
verbatim when it normalizes references.

`--json` is global and may appear before or after a subcommand. Both
`lekalo --json --version` and `lekalo --version --json` select JSON output.
Root and per-command help remain clap help text rather than a domain failure.

### `lekalo load`

`load` parses the fixed `.yaml` homes of a validated project with strict
spanned JSON/YAML frontends, resolves module-ID imports (direct visibility
only, cycles and duplicates rejected), normalizes compact type sugar and
short references, and prints one deterministic canonical model. See
[loader.md](loader.md) for the normative contract, reason codes, and the
hermetic fixture suite under `tests/fixtures/loader/`. `--spans` adds the
sorted `sourceMap` sibling; it never alters the model bytes.

With `--ir`, `load` additionally compiles the loaded model into the typed,
deterministic Lekalo IR (`dev.lekalo.ir@0.1.0`) and prints the canonical IR
bytes in place of the preserved model; `--spans` then appends the IR source
map. IR decode failures are `invalid` (exit 1, stderr) with the closed
`ir.*` reason codes. See [ir.md](ir.md) for the normative IR contract and
the fixture suite under `tests/fixtures/ir/`.

## Exit and stream contract

| Exit | Status | Stream | Meaning |
| ---: | --- | --- | --- |
| 0 | `valid` | stdout | Successful output, help, or version |
| 1 | `invalid` | stderr | Malformed command-line syntax or usage |
| 3 | `denied` | stdout | Well-formed physical/policy denial (loader, structure) |
| 4 | `unsupported` | stdout | Recognized capability unavailable in this build |
| 5 | `unsupported-version` | stderr | Model contract version outside the exact 0.1.0/1.0.0 set |

The stubs emit only 0/1/4. The loader emits 0/1/3/5 as specified in
[loader.md](loader.md).

Loader failures are pretty-printed with two-space indentation and carry
structured reasons: `{"code", "path"?, "span"?, "data"?}` where `path` is a
logical project-relative POSIX path and spans point into the original
document bytes. Success stays one compact line.

clap parsing always uses its non-terminating parse path. Syntax failures are
mapped to exit 1, so clap's default exit 2 is never exposed. The stable failure
does not echo raw arguments or include localized parser detail.

## JSON wire shapes

JSON is UTF-8, pretty-printed with two-space indentation, and followed by
exactly one LF. Fields are emitted in the order shown. Output contains no ANSI
escapes, timestamps, absolute paths, current-directory values, or raw argv.
`reasonCodes` is always an array of strings.

Malformed syntax:

```json
{
  "status": "invalid",
  "reasonCodes": [
    "cli.usage"
  ]
}
```

Recognized unavailable capability (using `inspect` as the example):

```json
{
  "status": "unsupported",
  "capability": "inspect",
  "reasonCodes": [
    "core.capability-unavailable"
  ]
}
```

Version:

```json
{
  "status": "valid",
  "version": "0.1.6"
}
```

The corresponding human lines are `invalid: cli.usage`,
`unsupported: core.capability-unavailable CAPABILITY`, and
`lekalo 0.1.6`. Human and JSON renderers consume the same `DomainResult`.

## Development checks

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --locked
cargo test --workspace --locked
```

CI also checks the exact Rust 1.80.0 toolchain, builds and tests on Linux,
Windows, and macOS, and runs every accepted Node contract checker and suite on
Node.js 18 and 24. On both Node majors the contracts job additionally
provisions exact Ajv 8.17.1 under the runner temp directory, outside the
checkout, exposes it to scripts/test-model-ajv.mjs alone through NODE_PATH,
and fails the job on any install, version, or gate failure.

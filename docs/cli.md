# Lekalo CLI foundation

Issue #3 introduces a target-neutral Rust core and the `lekalo` command-line
front end. The workspace is edition 2021, uses Cargo resolver 2, has an exact
MSRV of Rust 1.80.0, and carries product candidate version 0.1.4. The product
version is independent of every contract or model schema version.

The core crate owns `Request`, `Status`, `DomainResult`, reason codes, and the
exit mapping. It depends only on Serde. The CLI crate owns syntax and
presentation through clap and serde_json. Neither crate reads the filesystem,
loads a model, builds IR, invokes a target, or contacts a provider in this
foundation.

## Commands

```text
lekalo --version
lekalo validate
lekalo inspect SYMBOL
lekalo impact SYMBOL
lekalo context SYMBOL --budget TOKENS
```

`--version` is implemented. The four subcommands are recognized stubs: valid
syntax reaches the named capability and returns `unsupported`. `SYMBOL` is an
opaque string at this layer, and `TOKENS` is an unsigned integer. Semantic ID
rules, project discovery, model loading, validation, graph/IR construction,
and real inspect, impact, or context behavior belong to later issues.
When those capabilities are implemented they are bound by
dev.lekalo.semantic-ids@0.1.0 to use the validated semantic ID verbatim as
their canonical key; the machine contract closes that consumer list, and this
foundation implements none of its members.

`--json` is global and may appear before or after a subcommand. Both
`lekalo --json --version` and `lekalo --version --json` select JSON output.
Root and per-command help remain clap help text rather than a domain failure.

## Exit and stream contract

| Exit | Status | Stream | Meaning |
| ---: | --- | --- | --- |
| 0 | `valid` | stdout | Successful output, help, or version |
| 1 | `invalid` | stderr | Malformed command-line syntax or usage |
| 3 | `denied` | stdout | Reserved for a well-formed policy denial |
| 4 | `unsupported` | stdout | Recognized capability unavailable in this build |

Exit 3 is reserved and is not emitted by the issue #3 stubs. Exit 5 is neither
defined nor emitted here; support for incompatible contract versions belongs
to the versioning work.

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
  "version": "0.1.4"
}
```

The corresponding human lines are `invalid: cli.usage`,
`unsupported: core.capability-unavailable CAPABILITY`, and
`lekalo 0.1.4`. Human and JSON renderers consume the same `DomainResult`.

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

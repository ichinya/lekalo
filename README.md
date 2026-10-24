# Lekalo

Lekalo describes one application model for multiple targets. It compiles language-neutral semantics into typed IR, validates references and effects, and supplies projections and evidence to adapters and workflow tools.

Status: **Implemented** within the profiles described in [feature status](docs/roadmap.md). Native production execution and automatic UI generation remain planned. Owner: contributor-entry maintainers.

## Start here

| Task | Guide |
| --- | --- |
| Install a ready CLI for Windows, Linux or macOS | [Download, verify and install](docs/releases.md) |
| Create a project | [Minimal project](docs/project-layout.md#minimal-project) |
| Add Lekalo to existing code | [Choose an adoption path](docs/adoption.md) |
| Follow Laravel/Vue or TypeScript examples | [Tutorials](docs/README.md#tutorials) |
| Build, test or change Lekalo | [Contributor guide](docs/contributing.md) |
| Look up a command or contract | [CLI index](docs/cli.md#command-index), [reference guide](docs/README.md#reference) |

## Build and first valid example

From this repository's root, with Rust and a native linker installed:

```sh docs-setup=source-build
cargo build -p lekalo-cli --locked
```

The binary is `target/debug/lekalo` (`lekalo.exe` on Windows). Add its absolute directory to PATH before entering a fixture. Dependency setup, Windows linker requirements and offline builds are covered in [contributing](docs/contributing.md#build-from-source).

Copy [the minimal fixture](tests/fixtures/loader/valid-zero-modules) into a disposable directory. Enter that copy's root, containing `lekalo/project.yaml`, then run:

```sh docs-example=read-minimal
lekalo load --ir --json --no-cache
lekalo validate --strict --json --no-cache
```

Both commands exit **0**, write JSON to stdout and leave the fixture unchanged. This fixture uses Model/IR `0.2.16`, with one project and zero modules. Continue with [minimal project](docs/project-layout.md#minimal-project) to create your own project and first module.

## Documentation and boundaries

[The documentation guide](docs/README.md) is the main navigation page. References own behavior and exact contracts, tutorials own walkthroughs, and [ADRs](docs/adr/) preserve design decisions. [Versioning](docs/versioning.md) explains why a product release does not change every contract version.

Lekalo owns application semantics. OpenSpec owns requirements, AI Factory owns workflow state, and HLV owns its validation results. [Authority](docs/authority.md) defines artifact custody; [integrations](docs/integrations.md) defines handoffs.

Documentation on `main` describes this checkout. An installed release uses its tag's source and documentation; see [release snapshots](docs/releases.md#release-snapshots). A local example passing does not establish hosted CI, external delivery or production qualification.

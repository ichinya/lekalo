# Lekalo

Lekalo describes **one application model for multiple targets**. It compiles language-neutral semantics into a typed IR, validates references and effects, and supplies bounded evidence and projections to target adapters and workflow tools.

Готовые исполняемые файлы для Windows, Linux и macOS публикуются в
[GitHub Releases](https://github.com/ichinya/lekalo/releases).
Выбор платформы, проверка SHA-256 и установка описаны в
[инструкции по установке и выпуску](docs/releases.md).

Контракты имеют исходную версию **0.2.16**. При изменении контракта его версия
становится текущей версией проекта; неизменённые контракты сохраняют свою версию.
Старые схемы и история миграций удалены. См. [правила версионирования](docs/versioning.md).

Status: **Implemented** semantic loading, validation, graph/inspect/impact/context, adoption and supported target slices at product **0.6.5**. **Experimental** brownfield scan integration and partial target capabilities have explicit limits. **Planned** full native production execution, automatic Vue screen generation and aggregate public history export are not supplied by this checkout. See [roadmap](docs/roadmap.md). Owner: contributor-entry maintainers; [issue #105](https://github.com/ichinya/lekalo/issues/105).

Lekalo owns application semantics. OpenSpec owns requirements; AI Factory owns workflow state; HLV owns its validation results; source/native tools own execution evidence. [Authority](docs/authority.md) and [adoption modes](docs/adoption.md) explain their boundaries.

## Build and first valid example

Prerequisites: Git, Rust/Cargo (workspace MSRV 1.80; use a current stable toolchain) and Node 24 for contributor gates. Build from this checkout; no release installer is assumed. Network provisioning happens explicitly before offline verification.

```sh docs-setup=source-build
cargo build -p lekalo-cli --locked
```

Put the resulting `target/debug/lekalo` (`lekalo.exe` on Windows) on PATH, or invoke that absolute binary. Create a disposable copy of [the minimal fixture](tests/fixtures/loader/valid-zero-modules) and run these commands from its root:

```sh docs-example=read-minimal
lekalo load --ir --json --no-cache
lekalo validate --strict --json --no-cache
```

Expected: exit **0**, JSON on stdout, empty stderr, IR `contract: dev.lekalo.ir@0.2.16` and a valid strict report. This fixture has one project and zero modules; path A creates the first module. The gate also verifies byte-for-byte read-only preservation. Model/IR `0.2.16`, target/authority/privacy `0.3.2`, diagnostic registry/provider `0.6.4` and CI-report `0.6.3` are exact producer pins, not one interchangeable version. [Diagnostics and exits](docs/diagnostics.md), [version policy](docs/versioning.md).

## Quickstart paths A-F

| Path | Start here | Verified scope and limits |
| --- | --- | --- |
| A. New minimal project | [Project layout and init](docs/project-layout.md) | Empty directory -> project/module -> load and strict validate; creates Model, not an application runtime. |
| B. Adopt an existing Node.js project | [Synthetic Hono + Drizzle + MySQL](docs/tutorial-brownfield-typescript.md) | Disposable adoption, bounded scan and binding; experimental wire path, actual MySQL persistence checked separately. |
| C. Describe one contracted module | [Adoption and contract check](docs/adoption.md) | Maintained implementation + recorded contract declaration; no silent ownership promotion. |
| D. Inspect / impact / context | [Architecture example](docs/architecture.md) | Three read-only projections over the original partial planner corpus; use a separate disposable copy from C. |
| E. Generate / check / verify | [Greenfield Laravel + Vue planner](docs/tutorial-greenfield-planner.md) | Real adapter scenario exchange, Laravel HTTP/scenarios and checked client/maintained Vue screen; native production CLI execution stays planned. |
| F. Export trace; use AIFHub Extension / HLV | [Integration handoffs](docs/integrations.md) | Validated synthetic trace and provider discovery; external tool installation and delivery are separate. |

All command blocks carry example IDs checked by [the documentation gate](scripts/test-docs-examples.mjs). CI replays portable, planner and native MySQL lanes in disposable copies; setup blocks identify explicit provisioning. The [example registry](tests/fixtures/docs/examples.json) binds commands to fixtures, streams, schemas and semantic checks. A missing prerequisite fails its required lane.

## Documentation

| Subject | Canonical entry |
| --- | --- |
| Components and data flow | [Architecture](docs/architecture.md) |
| Artifact authority and three ownership modes | [Authority](docs/authority.md), [adoption](docs/adoption.md) |
| Definition kinds and terminology | [Model and glossary](docs/model.md) |
| Paths, committed inputs and runtime evidence | [Project layout](docs/project-layout.md) |
| Adapter operations, capabilities and confinement | [Target protocol](docs/target-protocol.md) |
| Result status, exits and reports | [Diagnostics](docs/diagnostics.md) |
| Privacy, custody and threats | [Security](docs/security.md) |
| Workflow tools and evidence handoffs | [Integrations](docs/integrations.md) |
| Availability and release snapshots | [Roadmap](docs/roadmap.md) |

[Every public command, protocol and schema has one checked documentation owner](docs/documentation-owners.json); [CLI index](docs/cli.md#command-index). Specialist specifications and full-filename [ADRs](docs/adr) remain the detailed references. Public examples use only synthetic data and roles such as `planner-consumer` and `brownfield-consumer`.

## Contributor checks and release documentation

Run [the documented gates](docs/roadmap.md#contributor-verification) after explicit dependency provisioning. Ownership metadata is refreshed by the explicit maintenance writer, then verified against live help and exact contract bytes; CI never repairs missing records. English is primary; consumers should link to these specifications rather than fork their terminology.

Stable release tags retain immutable README/docs/ADRs, contracts, ownership and example metadata at their release commit. Use the matching tag and its [version policy](docs/versioning.md); docs on a moving branch describe that branch. This work does not rewrite already published tags.

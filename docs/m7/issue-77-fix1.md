# Issue #77 fix round 1

Addresses **R1-1** from [Devin's round-1 review](issue-77-review1-devin.md)
on `ichinya/m7-issue-77`, starting at review commit `5eab4044`.

| Finding | Disposition | Evidence |
| --- | --- | --- |
| R1-1: coupling inherited the #75 description, while context-budget fell back to its args-struct comment. | **Fixed** | `crates/lekalo-cli/src/main.rs`, `Commands::Coupling`, now describes semantic coupling/change-radius evidence, public/internal impact, advisory defaults, strict profiles and optional `--context-budget` planning. The original #75 paragraph is restored immediately above `Commands::ContextBudget`. All three live help commands show the correct descriptions. |

The existing CLI test
`root_and_each_command_help_succeed_without_a_failure_envelope` now covers
both subcommands. It checks their descriptions in root help, coupling's
advisory wording and absence of `--budget`/over-budget claims, and the restored
context-budget description and explicit-budget requirement. It also rejects
the args-struct description as context-budget's help text. The guard failed
on the pre-fix source, printing the misplaced #75 paragraph for the root
`coupling` row, and passed after the correction.

## Live verification

Ran the rebuilt `target/debug/lekalo.exe` directly on Windows:

| Command | Observed result |
| --- | --- |
| `lekalo --help` | Exit 0. `coupling` starts with "Report semantic coupling and change-radius metrics"; `context-budget` starts with "Report the context-budget and local-understandability metrics". |
| `lekalo coupling --help` | Exit 0. Correct #77 summary, advisory default and opt-in strict profile; `--context-budget` is optional planning. No `--budget` or `--budget-profile` claim. |
| `lekalo context-budget --help` | Exit 0. Original #75 paragraph, including "No default budget exists" and the explicit `--budget`/`--budget-profile` requirement. |
| `cargo fmt --all`; `cargo fmt --all -- --check` | Passed. |
| `cargo build -p lekalo-cli --locked` | Passed; live help and the Node gate used this rebuilt binary. |
| `cargo test -p lekalo-cli --locked --test cli` | Six tests passed, including the expanded live help guard. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed. |
| `node scripts/test-coupling-contracts.mjs` | Passed: five schemas, all 36 checks, Ajv **8.17.1**. `NODE_PATH` was set from `LEKALO_AJV_NODE_PATH` to the existing external installation. |

The fix and this evidence document are committed locally on the requested
branch. Nothing was pushed; no other worktree was accessed.

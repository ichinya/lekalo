# Issue #105: independent fix review, round 2 (Codex)

Verdict: **ACCEPT**

Reviewed on 2026-10-04, branch `ichinya/m7-issue-105`, candidate `f5eff7b6395aa71e3b188da9bd40865794c6e820`, delta `6af53b6c..f5eff7b6`. The corrective implementation is `ae7f3b04` plus `cfd15799`; `10a13440` records worker evidence and `f5eff7b6` records the other review. Read the complete fix-review brief, [fix report](issue-105-fix1.md), [round-1 findings](issue-105-review1-codex.md) and [devin round-2 review](issue-105-review2-devin.md). This verdict rests on independent source inspection, actual gates and disposable process/CLI probes. Both prior findings are resolved; no new actionable defect was found in this fix delta.

## Finding dispositions

| Finding | Disposition | Independent evidence |
| --- | --- | --- |
| **R1-1: eight required P0 pages absent from machine metadata; gate did not enforce page coverage** | **FIXED** | `docs/documentation-owners.json:5` explicitly maps all thirteen required pages exactly once. `scripts/lib/docs-maintenance.mjs:108` rejects missing, duplicate, unknown and malformed records and prose disagreement; `scripts/test-docs-ownership.mjs:14` invokes it before either mode can succeed. Both real gate modes pass. An additional copied-gate process probe refused all 40 independently authored mutations, detailed below. |
| **R1-2: greenfield step C selected the original incomplete fixture instead of the contracted tutorial variant** | **FIXED** | `docs/tutorial-greenfield-planner.md:9` selects `tests/fixtures/docs/contracted-module` and the six linked adoption commands. Line 15 explicitly switches D to a separate fresh `tests/fixtures/contracted/planner-slice` copy without transferring C's registry; line 17 states its missing query implementation, support artifact and native test entrypoint. The full displayed C-to-D sequence was independently replayed from fresh copies, including the expected intermediate refusal and final clean conformance. |

## R1-1: exact page census and fail-closed proof

An independent thirteen-path list, compared directly with parsed `p0Pages`, gives 13 records, 13 unique required paths, zero missing paths and zero unknown paths. Every subsystem owner agrees with the page's opening prose declaration.

| Required page | Explicit subsystem owner | Occurrences |
| --- | --- | ---: |
| `README.md` | contributor-entry maintainers | 1 |
| `docs/architecture.md` | core and adapter-host maintainers | 1 |
| `docs/authority.md` | boundary maintainers | 1 |
| `docs/model.md` | semantic-contract maintainers | 1 |
| `docs/project-layout.md` | filesystem/reproducibility maintainers | 1 |
| `docs/target-protocol.md` | adapter-host maintainers | 1 |
| `docs/diagnostics.md` | result/reporting maintainers | 1 |
| `docs/adoption.md` | bootstrap/evidence maintainers | 1 |
| `docs/security.md` | privacy and adapter-host maintainers | 1 |
| `docs/integrations.md` | workflow/evidence-consumer maintainers | 1 |
| `docs/tutorial-greenfield-planner.md` | planner fixture maintainers | 1 |
| `docs/tutorial-brownfield-typescript.md` | Node evidence fixture maintainers | 1 |
| `docs/roadmap.md` | release/documentation maintainers | 1 |

Copied all tracked candidate files into a disposable directory outside the checkout, with no Git worktree registration. Ran the actual copied `node scripts/test-docs-ownership.mjs --static` successfully before and after the mutations. Each negative probe ran that unmodified script in a separate child process, required exit **1**, matched the intended failure reason and required empty stdout:

| Independent mutation | Cases | Required refusal | Result |
| --- | ---: | --- | --- |
| Remove each required page's metadata record | 13 | `missing P0 page owner` | 13/13 refused |
| Duplicate each required page's metadata record | 13 | `duplicate P0 page owner` | 13/13 refused |
| Replace a page with an unknown path | 1 | `unknown P0 page` | Refused |
| Change each copied page's prose `Owner:` while preserving metadata | 13 | `P0 prose owner drift` | 13/13 refused |

These **40 process refusals** supplement the gate's **33 built-in controls** at `scripts/test-docs-ownership.mjs:42`, which also cover absent/empty lists, blank/array owners and extra fields. No checkout metadata or prose was mutated. `scripts/update-docs-owners.mjs:9` validates and preserves the manually maintained map before writing; it cannot supply an omitted owner automatically.

The original `records` array is deeply equal to `git show 6af53b6c:docs/documentation-owners.json` after parsing: **318 unchanged surfaces**, with SHA-256 of `JSON.stringify(records)` equal to `9d0b9dca58a83370e62b67522e4a653822de7ab036d7adb6ad0ca0ad57bd42ea`. Existing CLI-help, source/contract/protocol, status, glossary, link and public-content checks remain enabled.

## R1-2: contributor replay of the linked sequence

Extracted the actual displayed command blocks from `docs/adoption.md:19` and `docs/architecture.md:30`, rather than relying solely on the replay registry. Used fresh tracked-file copies named `contract-planner` and `inspect-planner`, a sibling sentinel and an isolated child temporary directory. Invoked literal argv with the real CLI, capturing exact exits and both streams.

| Step | Working root | Actual result |
| --- | --- | --- |
| `contract update --declaration declarations/initial.json` | Tutorial variant | **0/stdout**, `status: valid`, four symbols recorded |
| `node --test --test-reporter=tap test/native.test.mjs` | Tutorial variant | **0/stdout**, `tutorial.focus` and `tutorial.list` pass; two passed, zero failed |
| First `contract check --module planner` | Tutorial variant | **1/stderr**, `status: invalid`, exactly two `contracted.coverage-missing` errors for `planner.focus_task` and `planner.list_tasks`, both with `data.detail: no-native-test` |
| Attach `tutorial.focus` to `planner.focus_task` | Tutorial variant | **0/stdout**, `status: valid` |
| Attach `tutorial.list` to `planner.list_tasks` | Tutorial variant | **0/stdout**, `status: valid` |
| Final `contract check --module planner` | Tutorial variant | **0/stdout**, `status: valid`, four symbols, no diagnostics |
| `inspect planner.focus_task` | Separate original copy | **0/stdout**, valid, selector resolves exactly |
| `impact planner.focus_task` | Separate original copy | **0/stdout**, valid; three `impact.evidence-unknown` diagnostics remain visible |
| `context planner.focus_task --budget 5000` | Separate original copy | **0/stdout**, valid, budget 5000 and `fits: true` |

Validated both coverage diagnostics and all three projection objects with Ajv 8.17.1 against their unchanged published schemas. The expected first conformance refusal is explicitly documented at `docs/adoption.md:28`, including how to continue after inspecting it under shell fail-fast. Test execution alone does not attach IDs.

SHA-256 snapshots preserved **all 35 pre-existing files** across the two fixture copies and sibling sentinel. The only added file was `contract-planner/.lekalo/import/contracted/registry.json`; D received no registry and its files were unchanged. The designated child temporary directory remained empty. The probe also checked that all **4,604 tracked checkout files** and the existing CLI binary retained their bytes.

The registry at `tests/fixtures/docs/examples.json:68` contains the six C commands followed by D's separate recipe at line 113. `scripts/test-docs-examples.mjs:67` validates visible working-root links, linked command-page anchors and contributor order. Its new refusal controls at line 104 exercise the former wrong fixture, absent root declarations and reversed replay order. The expected coverage check at line 233 verifies exact reason codes, symbol IDs, severity, detail and diagnostic schemas. The existing fingerprint-drift control at line 365 remains enforced after clean conformance.

## Gate results, tooling and custody

| Check | Result |
| --- | --- |
| `node scripts/test-docs-ownership.mjs` | **PASS**, live-help: 318 surfaces, 13 required docs, 13 P0 owners, 33 page-owner controls |
| `node scripts/test-docs-ownership.mjs --static` | **PASS**, identical counts |
| `node scripts/test-docs-examples.mjs --static` | **PASS**, 12 examples, two setup blocks, seven controls |
| Portable `node scripts/test-docs-examples.mjs` | **PASS**, **9 examples / 25 commands / 21 controls**, `sourcePreserved: true` |
| Independent actual-gate ownership mutations | **PASS**, all 40 refused with the intended reason |
| Direct displayed C-to-D contributor replay | **PASS**, all nine commands, exact intermediate refusal, final clean check, schema validation and bounded writes |
| `cargo build -p lekalo-cli --locked` in the disposable tracked-file copy | **PASS**, finished in 2m 42s; no checkout build output was changed |
| Live-help ownership gate and fresh C-to-D replay with that rebuilt binary | **PASS**, identical gate counts and all nine expected command outcomes, schema and custody checks |
| `git diff 6af53b6c..HEAD -- crates contracts` | Empty |
| Additional diff over `Cargo.toml`, `Cargo.lock` and `.github/workflows/ci.yml` | Empty |
| `git diff --check 6af53b6c..HEAD` | **PASS** |

The portable run actually printed successful completion for `contract-planner` and `inspect-planner`, followed by the other portable recipes; they were executed rather than statically counted. Local Node is **24.13.0**, externally provisioned Ajv is **8.17.1**. Both `LEKALO_AJV_NODE_PATH` and `NODE_PATH` were set to `C:/Users/User/AppData/Local/Temp/ajv/node_modules`. With only the former set, the brief's initial invocation refused at dependency resolution (`Cannot find module 'ajv/package.json'`, exit 1), before replay. The gate uses Node's `createRequire` at `scripts/test-docs-examples.mjs:117`; CI explicitly supplies `NODE_PATH` at `.github/workflows/ci.yml:332`. Supplying that existing prerequisite resolved the invocation without installing dependencies or modifying the gate.

The existing binary used for the mandatory gates and first direct replay has independently measured SHA-256 `f2fae0c27aa18ed5a0be963145e6ffca79bfbd2024d4f07269f43119b8817799`. Independently rebuilt the candidate with `cargo build -p lekalo-cli --locked` in the disposable tracked-file copy, leaving the checkout binary untouched. The new binary's SHA-256 is `d1765e4c075b2b4d29ad99ade1d4a24465a9716c2672c56e2d950840765f3bc1`. The actual copied live-help ownership gate passed against it, and a second fresh displayed C-to-D replay reproduced all nine exits/statuses, both schema-valid coverage refusals, clean final conformance, schema-valid projections and the same 35-file preservation/sole-registry-write boundary.

Initial status was clean. All hostile probes and build output were confined to the owned disposable root `C:/Users/User/AppData/Local/Temp/lekalo-fix-review-105-960896b9046d43bc90c13b363c2b9d34`, outside the checkout. Automatic command policy rejected its recursive cleanup (`blocked by policy`), even though the submitted command checked the absolute target, temporary parent, exact created name and reparse-point status before deletion. The command was not executed; that temporary root remains. No other worktree was modified. The forbidden-directory and workflow diffs are empty, and inspection found no gate or schema weakening. Existing portable CI wiring follows its locked CLI build and remains unchanged.

Before the local commit, only `docs/m7/issue-105-review2-codex.md` is staged and the staged diff passes whitespace checks. No push is authorized or performed. Acceptance is limited to this fix delta and the mandatory local checks. No new hosted CI evidence was obtained; native Laravel/Vue, live MySQL, browser and external-consumer acceptance are outside this re-review. The previously disclosed product/schema defects remain outside this unchanged-source fix scope.

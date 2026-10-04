# Issue #105: fix round 1

Implemented on `ichinya/m7-issue-105`, 2026-10-04, against the two findings in [Codex review round 1](issue-105-review1-codex.md), starting from review commit `6af53b6c`. The approved [research](issue-105-research.md) and original [implementation report](issue-105-implementation.md) remain historical evidence. This report records the corrective delta and local replay; it does not assign an independent acceptance verdict.

Fix commits: `ae7f3b04` (documentation and fixture explanations), `cfd15799` (machine ownership mapping and replay checks). This report follows in a separate docs commit. Product/source contracts remain unchanged at the requested base `a56ee578`; no tracked `crates/` or `contracts/` file changed. No push, other worktree or real consumer repository was used.

## Findings and disposition

| Finding | Disposition | Corrective evidence |
| --- | --- | --- |
| **R1-1: eight required P0 pages missing from machine ownership metadata** | **Fixed; locally verified.** | [documentation-owners.json](../documentation-owners.json) now contains an explicit `p0Pages` array of thirteen `{page, owner}` records, including README and all previously absent pages. [Page validator](../../scripts/lib/docs-maintenance.mjs) requires exact P0 coverage, one record per page, closed fields, a nonempty scalar subsystem owner and agreement with the opening prose declaration. [Ownership gate](../../scripts/test-docs-ownership.mjs) invokes it in both modes and exercises 33 refusal controls. [Explicit writer](../../scripts/update-docs-owners.mjs) preserves and validates this manually maintained map instead of inventing missing owners. |
| **R1-2: greenfield step C selects the incomplete original planner fixture** | **Fixed; locally verified.** | [Greenfield C](../tutorial-greenfield-planner.md#contract-the-planner-c) explicitly selects `tests/fixtures/docs/contracted-module`, its disposable working root and prerequisites. [Adoption sequence](../adoption.md#contract-one-module) now shows update, actual tests, expected missing-coverage check, two attachments and clean conformance. [Greenfield D](../tutorial-greenfield-planner.md#inspect-the-planner-d) explicitly switches to a separate fresh original projection copy, without transferring C's registry, and lists its incomplete query/support/test limitations. README and architecture agree with that transition. |

The existing **318 public-surface records are structurally identical** to their pre-fix JSON: 151 commands/groups, 4 globals, 110 contract files and 53 protocols. SHA-256 of `JSON.stringify(records)` is `9d0b9dca58a83370e62b67522e4a653822de7ab036d7adb6ad0ca0ad57bd42ea`. Page accountability is an additional list, not a replacement or reduction of the surface census. All previous live-help, source/contract/protocol, status, glossary, link and public-content checks remain enabled.

The P0 controls remove and duplicate each of the thirteen page records, then reject an absent/empty map, unknown page, blank owner, owner array, extra field and prose disagreement. Separate disposable process probes ran the actual ownership gate with each missing and duplicate page: **26/26 exited 1**, with the intended finding and no success output. No metadata was temporarily changed in the checkout for these probes.

## Linked tutorial replay

[examples.json](../../tests/fixtures/docs/examples.json) keeps the six C commands in one disposable `contract-planner` root, followed by the three D projections in a separate `inspect-planner` root. [Example gate](../../scripts/test-docs-examples.mjs) checks the tutorial's visible working-root links and linked command-page anchors against those recipes, and requires C before D. New mutation controls reject the former wrong fixture, missing working-root declarations and reversed replay order. Literal argv, exact exit/stream checks, published schemas, source preservation and existing drift/refusal controls remain enforced.

An additional direct contributor replay executed the complete linked sequence from fresh copies:

| Step | Actual result |
| --- | --- |
| Declaration update in the tutorial variant | **0/stdout**, valid, four bound symbols |
| Maintained-code tests | **0/stdout**, two passed, zero failed |
| Check before attaching evidence | **1/stderr**, invalid, exactly two `contracted.coverage-missing` diagnostics for `planner.focus_task` and `planner.list_tasks`; both validate against the unchanged diagnostic schema |
| Attach `tutorial.focus` to `planner.focus_task` | **0/stdout**, valid |
| Attach `tutorial.list` to `planner.list_tasks` | **0/stdout**, valid |
| Final module conformance | **0/stdout**, valid, four symbols |
| Switch to the separate original copy; inspect, impact, context at budget 5000 | All three **0/stdout**, valid projections; partial-evidence diagnostics remain visible |

The first conformance refusal is expected: executing tests does not attach their IDs. The docs distinguish that exit from the final clean check and explain continuation with shell fail-fast enabled. The original projection corpus remains deliberately incomplete and is not presented as passing conformance.

All **35 pre-existing files** across the two copies and sibling sentinel retained their SHA-256 bytes. The sole addition was `contract-planner/.lekalo/import/contracted/registry.json`; the sibling sentinel remained unchanged and the designated child temporary home stayed empty. Both direct-replay and ownership-probe roots were removed with checked cleanup boundaries. The portable gate separately retains its controlled fingerprint-drift refusal after clean conformance.

## Verification and delivery boundary

Local Windows verification used Node `24.13.0`, the existing locked CLI binary SHA-256 `f2fae0c27aa18ed5a0be963145e6ffca79bfbd2024d4f07269f43119b8817799`, and externally provisioned Ajv `8.17.1`. No build, dependency install or MySQL service was silently added by the fix.

| Check | Result |
| --- | --- |
| `node scripts/test-docs-ownership.mjs` and `--static` | Pass: 318 surfaces, 13 required pages, **13 explicit page owners**, 33 page-owner controls |
| `node scripts/test-docs-examples.mjs --static` | Pass: 12 examples, 2 setup blocks, **7 static controls**; registry now contains 29 displayed commands across all lanes |
| `node scripts/test-docs-examples.mjs` | Pass: portable lane, **9 examples / 25 commands / 21 controls**, `sourcePreserved: true`; includes security/provenance, six-stage planner chain and MySQL observed harness |
| Actual-gate ownership process probes | Pass: all 13 missing-page and 13 duplicate-page inputs refused |
| Direct linked C-to-D contributor replay | Pass: all 9 commands, expected intermediate refusal, clean final conformance, separate projection root and bounded writes |
| `node scripts/test-contracted-contracts.mjs` | Pass: four declaration symbols, six refusal vectors, 307 registry entries |
| `node scripts/test-fixture-provenance.mjs` | Pass: 69 synthetic families, zero evidence-backed imports; only existing synthetic fixture explanations/metadata changed, so no new provenance family was needed |
| `node scripts/test-golden-hygiene.mjs` | Pass: 259 files, six controls |
| Node syntax checks for the four changed scripts | Pass |
| `git diff --check`, staged-path checks and forbidden-directory diff | Pass; local commits contain only the intended docs, scripts and existing synthetic fixture metadata |

Existing CI wiring requires both ownership verification and portable replay after building the CLI, with separate planner/MySQL lanes. The corrected C/D checks therefore run through the existing portable CI entrypoint; no workflow or gate was weakened. Native Laravel/Vue and live MySQL lanes were not rerun in this fix round, and hosted CI has not run for these unpushed commits. Their earlier evidence remains bounded by the implementation report. The two previously disclosed product/schema defects remain unchanged; this fix does not claim production, browser, release or external-consumer acceptance.

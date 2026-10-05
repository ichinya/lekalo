# Issue #105: independent implementation review, round 1 (Codex)

Verdict: **ISSUES**

Reviewed on 2026-10-04: `a56ee5786f1724f9ee61ef973088c7679684c14d..6d8c5bea46a428c7801cce77e20a3d048b6b6759`, branch `ichinya/m7-issue-105`. Read the complete review brief and all three named research/implementation/devin inputs. The devin ACCEPT is not the basis for this verdict. Independent source inspection and real-binary reproduction found two actionable documentation defects. No implementation, contract, fixture or workflow was edited.

## Findings

### R1-1 — Major: eight required P0 pages are absent from the owner metadata

The brief explicitly requires all thirteen P0 documents to exist **and be owner-mapped in `docs/documentation-owners.json`**. All exist, but eight paths occur in no metadata record: README, project layout, adoption, security, integrations, both tutorials and roadmap. Their prose contains `Owner:` declarations; those declarations do not constitute the required machine mapping.

Evidence: `docs/documentation-owners.json:5` contains the surface records; `scripts/lib/docs-maintenance.mjs:95` validates only command/global/contract/protocol records. `scripts/test-docs-ownership.mjs:19` checks P0 status text and links, without checking a P0 ownership mapping. Its `requiredDocs:13` at line 41 is the constant P0 list length. Both live-help and static gates pass despite this omission.

Independent census: for each required page, filter records for any exact field value equal to its path. The result is below; zero means the page is absent even as a source or primary reference owner.

| Required page | Exists | Referencing metadata records |
| --- | --- | ---: |
| `README.md` | Yes | 0 |
| `docs/architecture.md` | Yes | 7 |
| `docs/authority.md` | Yes | 3 |
| `docs/model.md` | Yes | 1 |
| `docs/project-layout.md` | Yes | 0 |
| `docs/target-protocol.md` | Yes | 10 |
| `docs/diagnostics.md` | Yes | 7 |
| `docs/adoption.md` | Yes | 0 |
| `docs/security.md` | Yes | 0 |
| `docs/integrations.md` | Yes | 0 |
| `docs/tutorial-greenfield-planner.md` | Yes | 0 |
| `docs/tutorial-brownfield-typescript.md` | Yes | 0 |
| `docs/roadmap.md` | Yes | 0 |

Required correction: add an explicit complete P0 page/subsystem-owner mapping and make the ownership gate reject missing or duplicate page owners. Retain the existing public-surface census and checks.

### R1-2 — Major: the greenfield tutorial selects the wrong fixture for step C

`docs/tutorial-greenfield-planner.md:9` instructs the contributor to copy `tests/fixtures/contracted/planner-slice`, then execute **contract one module**. The linked commands in `docs/adoption.md:17` are specifically for `tests/fixtures/docs/contracted-module`, also selected by `tests/fixtures/docs/examples.json:97`. These fixtures have materially different content.

Independent reproduction from a fresh copy containing only tracked original `planner-slice` bytes:

1. `lekalo --json contract update --declaration declarations/initial.json` exits 0 and records four symbols.
2. `node --test --test-reporter=tap test/native.test.mjs` exits **1**: the file does not exist in that fixture.
3. A direct `lekalo --no-cache --json contract check --module planner` exits **1/stderr**, `status: invalid`, including the absent support-artifact finding.

The original also deliberately contains an unimplemented query. The linked adoption page acknowledges those limitations and uses the new variant to avoid claiming they pass. The greenfield page nevertheless sends the reader into the original. The portable gate stays green because its registry replays the correct variant independently of this prose transition.

Required correction: make step C explicitly select the contracted tutorial variant. Identify any subsequent switch to the original projection corpus, its working root and limitations. Verify the linked sequence as a contributor would follow it.

## Mandatory criteria and independent evidence

| Criterion from the brief | Result | Evidence |
| --- | --- | --- |
| 1. All thirteen P0 docs exist and are owner-mapped; run ownership gate | **FAIL** | 13/13 exist; only 5/13 occur in metadata. Both `node scripts/test-docs-ownership.mjs` and `--static` exit 0, reporting 318 surfaces and 13 required docs. R1-1 explains the uncovered requirement. |
| 2. Portable examples gate and local links/anchors | **PASS** | `node scripts/test-docs-examples.mjs` exits 0: 9 examples, 24 commands, 18 controls, `sourcePreserved:true`. Static mode exits 0: 12 examples, 2 setup blocks, 4 controls. Independent link checking across all 20 changed Markdown documents resolves 461 local links and 13 heading anchors, with zero errors. |
| 3. Tutorial replay, matching outputs and bounded writes | **PARTIAL / FAIL for linked greenfield step C** | Independently replayed all five adoption commands on the correct contracted variant: update records four symbols; two real maintained-code tests pass; both attach commands and final conformance check exit 0/stdout. SHA-256 snapshots preserve all 17 original files. The sole added file is `.lekalo/import/contracted/registry.json`, permitted by `docs/contracted-mode.md:19` and line 41. The sibling sentinel and replay-parent inventory remain unchanged; the designated child temporary home remains empty. Controlled source drift produces `contracted.binding-drift` for `planner.focus_task`, exit 1/stderr. R1-2 reproduces the failing linked tutorial transition. |
| 4. Synthetic MySQL labelling and honest inherited defects | **PASS** | `tests/fixtures/pilot/brownfield-mysql/README.md:3` explicitly labels origin synthetic and records/credentials fictional. The tutorial distinguishes authored scanner declarations, fake-repository HTTP checks, experimental wire fallback and real MySQL persistence. Independently reproduced **both** defects documented in the implementation report, with unmodified published schemas; details below. |
| 5. Separate portable/planner/MySQL CI lanes and external provisioning | **PASS for wiring** | `.github/workflows/ci.yml:330` runs portable ownership/examples after the locked build, on Ubuntu/Windows/macOS, with externally provisioned Ajv 8.17.1. The planner step at line 471 follows Composer installation from the committed lock at line 395 and external TypeScript 5.9.3/Vue compiler 3.4.38 provisioning at line 466. `mysql-docs` at line 517 provisions the digest-pinned MySQL 8.4.5 service, builds the CLI, installs external Ajv and runs fixture `npm ci` from its lock before `--lane mysql`. No dependency/vendor paths were added by this delta. Native lane execution and hosted CI were not independently rerun here. |
| 6. No crates/contracts changes; clean status except review file | **PASS** | Initial `git status --porcelain` is empty. `git diff a56ee578..HEAD -- crates contracts` and the corresponding name-only check are empty; `git diff --check a56ee578..HEAD` passes. Before staging, only this new review document is present in status. The authorized local docs commit is restricted to this path; no push. |

## Inherited defects independently reproduced

These defects are real and accurately disclosed in `issue-105-implementation.md`; they are not additional #105 findings. The source and schemas producing them are unchanged from the requested base.

- On a fresh original planner copy, `lekalo --no-cache --json context planner.focus_task --budget 1` exits **0/stdout**, `status: valid`, with `context.sections: {}`. Ajv 8.17.1 rejects `/sections` for `minProperties:1` in `contracts/context-capsule.schema.v0.2.16.json:263`. The read-only invocation creates or changes no fixture file.
- After the original declaration update, module conformance exits **1/stderr** with `contracted.stale-artifact`, `symbol: .lekalo/generated/openapi/planner.json`. Ajv rejects `/symbol` against `#/$defs/symbolId/pattern`, `contracts/diagnostic.schema.v0.2.16.json:88`. `crates/lekalo-core/src/contracted/diagnostic.rs:100` passes the path into the symbol slot, confirming the producer cause. Source bytes remain intact; only the permitted contracted registry is created.

## Verification and custody

`cargo build -p lekalo-cli --locked` passed. The reviewed local binary SHA-256 is `f2fae0c27aa18ed5a0be963145e6ffca79bfbd2024d4f07269f43119b8817799`; Node is 24.13.0, schema validator Ajv 8.17.1. Ajv was reused from its external provisioned temporary dependency directory. All independent fixture probes used fresh tracked-file copies outside the checkout, literal argv, captured exits/streams, file hashes and checked cleanup boundaries. Their disposable directories were removed. No other worktree was read or modified.

The mandatory portable replay actually ran its security/authority/privacy/provenance, six-stage planner-chain and MySQL observed-harness commands, not merely static parsing. Its complete success does not repair the two documentation coverage defects above. This review does not claim independent live MySQL/native Laravel-Vue execution, browser execution, hosted CI, release publication or external-consumer acceptance.

Delivery boundary: only `docs/m7/issue-105-review1-codex.md` is authorized for this local docs commit. Acceptance requires correcting and rechecking R1-1 and R1-2; no source/schema weakening is requested.

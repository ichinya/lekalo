# Issue #84: fixes after Codex review round 1

**Status: F1 and F2 fixed and locally verified.** Work performed on `ichinya/m7-issue-84`, starting from review commit `827b15e5`, on 2026-10-05. Product version remains **0.6.5**. This record addresses the two P2 findings in [Codex review round 1](issue-84-review1-codex.md); it does not claim independent acceptance or completion of the adapter and A/B outcome criteria.

## F1: total UTF-8 error positions and structured admission refusals

The strict JSON frontend can report a one-byte error range beginning at an unexpected multibyte token. Previously, its end position sliced a UTF-8 string inside that character and panicked. `LineIndex::position` now clamps to EOF and rounds an interior byte offset down to the scalar's start (`crates/lekalo-core/src/loader/source.rs:83`). `LineIndex::span` rounds its end up to a character boundary, so a half-open error range includes the full offending scalar (`source.rs:99`). Both operations preserve already valid offsets. No ASCII-only restriction was added.

The shared architecture decoder still performs UTF-8 decoding, strict frontend parsing, version/identity checks and closed typed deserialization (`crates/lekalo-core/src/architecture_profile/mod.rs:72`). Frontend failures now reach its existing bounded `architecture-profile.input-invalid` envelope instead of unwinding. BOM input remains invalid JSON and receives that same structured refusal.

The required positive Unicode control also exposed a separate existing off-by-one in escaped supplementary characters. `parse_hex4` leaves the cursor after its four digits; the high-surrogate path previously looked one byte beyond the following backslash. It now checks the next `\\u` at the current cursor and advances to the low surrogate's `u` before parsing (`crates/lekalo-core/src/loader/frontends/json.rs:288`). Literal Unicode and valid escaped surrogate pairs decode alike. Lone, mismatched, truncated and malformed surrogate/hex escapes remain refused.

Regression evidence:

- `source.rs:189` checks every byte offset, including EOF/one-past-EOF, in a string containing ASCII, two/three/four-byte scalars, CRLF and a BOM, against a character-boundary/line/column oracle. `source.rs:209` verifies complete, sliceable error ranges for the reported tokens and an interior emoji offset.
- `json.rs:522` covers emoji, the minimum and maximum supplementary scalars, and combined escaped text. `json.rs:538` retains six malformed escape refusals; `json.rs:553` checks real frontend error spans for the three malformed token forms.
- `architecture_profile/mod.rs:1125` checks structured core refusals and legal Unicode decoding.
- The live gate at `scripts/test-architecture-profile-contracts.mjs:202` runs **three vectors across eight input routes**, for **24 refusals**. Each vector is bare U+00E9, `{"x":` followed by unquoted U+00E9 and `}`, or U+FEFF followed by an otherwise valid document for that route.

| Document admission route | All three malformed vectors |
| --- | --- |
| `resolve --architecture-profiles` | `invalid`, exit 1, `architecture-profile.input-invalid` |
| `lock --architecture-profiles` | Same |
| `diff --base-profiles` | Same |
| `diff --candidate-profiles` | Same |
| `assess --architecture-profiles` | Same |
| `assess --architecture-lock` | Same |
| `assess --baseline` | Same |
| `assess --adoption`, with valid baseline/date | Same |

The gate parses the refusal as JSON, requires the normal invalid failure channel and exit **1**, and bounds its serialized UTF-8 envelope to **4096 bytes** (`scripts/test-architecture-profile-contracts.mjs:37`, `:219`). None returned 101. Two live adoption controls pass literal and escaped U+00E9, U+6F22 and U+1F600 prose through the same decoder, returning `valid`, exit 0, assessment `adopting` (`:226`).

## F2: catalog authority for required evidence

`selection_valid` now permits `required:true` only for an enabled rule with catalog `blockingBasis` **semantic** or **measured** (`crates/lekalo-core/src/architecture_profile/mod.rs:153`). Advisory rules cannot acquire a blocking completeness obligation through a profile document. The guard is shared by effective-rule admission (`:215`) and baseline/report validation (`:875`, `:907`); typed public document operations re-enter admission (`:177`). Numeric limit eligibility, mandatory error severity and inherited weakening checks remain intact.

For the original exploit, a child of `contracted-standard` requiring `architecture.justified-abstractions` now refuses at admission with `architecture-profile.input-invalid`, status `invalid`, exit **1**, both with and without `--check`. It never reaches assessment as a sole blocking evidence gap. R14 and the other **11 advisory catalog rules** retain their advisory classification.

The existing `managed-generated` profile already requires deterministic generation proof. A blanket guard against its former advisory catalog classification would have invalidated that published built-in. The catalog now explicitly classifies `architecture.deterministic-generation` as **semantic**: equality of repeated outputs for pinned inputs is a behavior obligation, independent of subjective code style (`scripts/gen-architecture-profile-contracts.mjs:29`). Its coverage remains **unsupported**, and its rationale still requires independent repeated receipts. Required missing receipts still produce an `evidence-gap` and deny the managed profile's `--check` with exit **3** (`scripts/test-architecture-profile-contracts.mjs:95`). This metadata correction grants no proof, lowers no severity and removes no required obligation.

Regression evidence:

- `architecture_profile/mod.rs:1070` attempts `required:true` for every advisory rule through serialized parsing and typed `resolve`, `snapshot` and `policy_diff`. All refuse; managed generation remains required, semantic and unsupported.
- `scripts/test-architecture-profile-contracts.mjs:168` exercises all 11 advisory rules as inherited children through `resolve` and `assess --check`, and as complete independent roots through `resolve`. It also tests the original R14 exploit through `lock`, candidate `diff`, and assessment without `--check`: **36 live admission refusals**, each invalid/exit 1 with the expected reason code.
- Those hostile documents still pass the unchanged exact-Ajv structural schema. The test proves that schema shape alone cannot grant catalog authority; runtime admission performs the cross-document invariant check. Existing equality/one-over measurements, semantic errors, weakening refusals, lock tamper refusals and managed-generation denial remain in the full gate.

The current user documentation explains the eligibility rule and the managed proof boundary at `docs/architecture-profile.md:24`. This fixes the AC6 gap identified in review; the original implementation and review reports remain historical records.

## Generated artifacts and compatibility

The generator owns the catalog correction. Ran `node scripts/gen-architecture-profile-contracts.mjs --write`, rebuilt the binary, then regenerated the nine synthetic outputs using `node scripts/test-architecture-profile-contracts.mjs --write-goldens`. The final acceptance run used **read-only mode**, without that flag.

An independent comparison against `827b15e5` verified:

- **115/115 tracked contract schemas are byte-identical**, including every architecture schema. Both diagnostic registries, `v0.6.4` and `v0.6.5`, are byte-identical. The established 500 predecessor entries / eight architecture additions and wire versions receive no change.
- Only two contract instances change: the candidate architecture catalog's generation `blockingBasis`, and the profile collection's `catalogRef.digest`. All profile IDs, versions, rule selections, required flags, limits and inheritance remain identical.
- The new catalog digest is `sha256:88ef69ad4dc3c673a168b26834a7e86ee1068da26412f889322da528a89806fb`. Existing locks, baselines and adoption ledgers pinned to the old candidate catalog must be explicitly regenerated/reviewed by their caller; no automatic write or pin bypass is introduced.
- A recursive before/after comparison of all nine goldens found only that catalog classification and derived SHA-256 references. Report measurements, coverage, severity, dispositions, witnesses, explanations and alternatives remain identical. All nine golden roles and the existing index/provenance declaration remain present.
- `node scripts/update-docs-owners.mjs --write` refreshed only the two changed contract source digests in `docs/documentation-owners.json`. Product version is derived as 0.6.5; the generated CLI index stays byte-identical. No command or family is added, so their ownership maps need no additions.

No predecessor contract, existing neighbor gate, registry, workflow, Model semantics or adapter surface is changed. The architecture gate retains its original checks and adds regression probes. Its existing CI invocation remains after the locked workspace build in `build-test`, with exact Ajv **8.17.1** (`.github/workflows/ci.yml:198`, `:204`, `:300`).

## Final local verification

Commands ran on the final source with the rebuilt `target/debug/lekalo.exe`. Node used the supplied external installation; no dependency installation occurred in this checkout:

```powershell
$env:NODE_PATH = 'C:/Users/User/AppData/Local/Temp/ajv/node_modules'
$env:LEKALO_AJV_NODE_PATH = $env:NODE_PATH
```

Git subprocess checks used a process-scoped `safe.directory` for this worktree. No global configuration or other worktree was changed.

| Command | Observed result |
| --- | --- |
| `cargo build --locked` | Exit 0; product 0.6.5; final dev binary rebuilt. |
| `cargo test --locked -p lekalo-core --lib loader:: -- --test-threads=1` | **39 passed**, 0 failed. Includes source positions, JSON frontend and YAML neighbors. |
| `cargo test --locked -p lekalo-core --lib architecture_profile::tests` | **7 passed**, 0 failed. |
| `cargo test --locked -p lekalo-core --test ir --test diagnostics` | Diagnostics **6 passed**; IR library-suite harness **1 passed**; 0 failed. |
| `node scripts/test-architecture-profile-contracts.mjs` | Exit 0; **Ajv 8.17.1**, 7 families, 9 goldens, 508 entries, 8 diagnostic IDs, **124 live probes**, 4 artifact-fault controls, read-only. |
| `node scripts/test-diagnostic-contracts.mjs` | Exit 0; Ajv 8.17.1; 500 predecessor entries, 5 envelopes / 5 items. |
| `node scripts/test-provider-contracts.mjs` | Exit 0; 15 checks. |
| `node scripts/test-context-budget-contracts.mjs` | Exit 0; Ajv 8.17.1; 500 entries, 8 context rules, live binary checked. |
| `node scripts/test-coupling-contracts.mjs` | Exit 0; Ajv 8.17.1; 5 schemas, 36 checks. |
| `node scripts/test-validation-contracts.mjs` | Exit 0; registry 0.6.4, default/strict profiles, 23 owned rules. |
| Five `scripts/test-ai-lint-{report,evidence,config,waivers,comparison}-contracts.mjs` gates | Each exit 0; schema 0.6.4; live binary checked. |
| `node scripts/gen-architecture-profile-contracts.mjs --check` | Exit 0; all 11 generated artifacts match. |
| `node scripts/test-fixture-provenance.mjs` | Exit 0; 78 families, all synthetic. |
| `node scripts/check-contract-versions.mjs --base 827b15e5` | Exit 0; product 0.6.5, 125 contract artifacts. |
| `node scripts/test-docs-ownership.mjs --static` and `node scripts/test-docs-ownership.mjs` | Both exit 0; 355 surfaces, 13 P0 owners, 33 negative owner controls; live CLI help checked. |
| `node scripts/test-golden-normalization.mjs` | Exit 0; 243 files; UTF-8 ordering, CRLF and path producer controls passed. |
| `node scripts/test-golden-catalog.mjs` | Exit 0; Ajv 8.17.1; 21 cases, 6 imported evidence records, 500 registry rules. |
| `cargo fmt --all -- --check`; `git diff --check` | Both exit 0. |

Final architecture gate output:

```json
{"ok":true,"product":"0.6.5","ajv":"8.17.1","families":7,"goldens":9,"registryEntries":508,"diagnostics":8,"liveProbes":124,"artifactFaults":4,"fixProbes":{"multibyteRefusals":24,"unicodeSuccesses":2,"advisoryRequiredRefusals":36},"mode":"read-only"}
```

The first golden-authoring attempt correctly failed the escaped-emoji positive control; it exposed the surrogate cursor defect described above. After that fix and rebuild, authoring and the separate read-only acceptance run both passed. Acceptance goldens were not edited to accept a refusal or omit that control.

Full workspace Rust tests, Clippy, MSRV, Linux/macOS execution, hosted CI, live adapter/Mago integration and controlled AI-agent A/B outcomes were not run or claimed in this fix round. F1 and F2 are addressed within the measurable core milestone. Delivery is one local conventional fix commit, without a push.

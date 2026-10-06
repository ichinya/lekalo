# Issue #94: canonical YAML/JSON formatter research

Research only, 2026-10-06. Acceptance authority: [live issue #94](https://github.com/ichinya/lekalo/issues/94), read with `gh issue view 94 --repo ichinya/lekalo --json title,body,labels`. Inspected baseline: `15bd13917654dc20ad19b089a5bb8e5e29c2099f`, delivery branch `ichinya/m8-issue-94`; the initial worktree and index were clean. The dispatch names an M8 checkout and product target **0.8.0**, while this baseline's `Cargo.toml` and `docs/versioning.md` still declare **0.6.5**. This brief records that distinction; it does not change the product version.

House format studied: `docs/m7/issue-90-research.md`, `docs/m7/issue-88-research.md`, and `docs/m7/issue-37-research.md`. Their inventory -> proposal -> diagnostics/contracts -> fixtures/gates -> acceptance mapping -> risks structure is reused; historical absence claims and counts are not carried forward as current facts. Context7 resolution for Lekalo returned unrelated projects, so those documents were not used; the inspected repository is the CLI/API authority.

**Decision (proposal):** add one source formatter over the accepted YAML/JSON frontend, retaining a separate comment/trivia representation and using the existing normalized Model/IR as the semantic oracle. A read-only planning service serves CLI check and editor formatting; explicit CLI apply uses a confined transaction. Never serialize the compiled IR back into source: that loses comments, source boundaries and authoring forms. Every candidate is reparsed and compared semantically before mutation.

**Proposal convention:** all sections titled "Proposed", table cells labeled "Proposal", and commands within those sections describe future interfaces or verification, not shipped behavior. New/changed contract artifacts take **0.8.0** in the implementing M8 commit after its product version is set accordingly; unchanged contracts retain their exact accepted versions. This dispatch creates only this document under `docs/m8/` and its requested local commit; no implementation, contract, fixture, CI edit or push is included.

## 1. Current-state inventory

### Authority and dependency context

#94 declares Parent [#1](https://github.com/ichinya/lekalo/issues/1), Depends on [#7](https://github.com/ichinya/lekalo/issues/7), [#8](https://github.com/ichinya/lekalo/issues/8), [#9](https://github.com/ichinya/lekalo/issues/9), [#18](https://github.com/ichinya/lekalo/issues/18), and no Related links. The four dependency issues were read live and are closed; #1 supplies authority/milestone/privacy context. Their bodies are context rather than permission to migrate schemas, change semantic operations, or implement a full IDE. #7 owns strict parsing/normalization and safe imports, #8 separates semantic IR from source locations, #9 owns exact version admission and migration safety, and #18 supplies formatting-only semantic equality.

The issue's goal is to eliminate formatting churn and stabilize textual representation for AI patches, semantic operations, review and golden fixtures. A first useful increment must already distinguish malformed input from noncanonical valid input; source rewriting cannot be built as an unguarded pretty-printer and hardened later.

| Existing surface / searchable owner | Confirmed current behavior | Consequence / gap for #94 |
| --- | --- | --- |
| `crates/lekalo-cli/src/main.rs::{Cli,Commands}` | Global `--json` and `--no-cache`; `load --project DIR --spans --ir`, directory-based `diff OLD NEW --format json`. No `Format` variant at baseline. | Add a command and preserve the existing result/exit contract. Do not present proposed flags as existing syntax. |
| `crates/lekalo-core/src/loader/mod.rs::{LoadSelection,NormalizedModel,ProjectLoad,decode_document_bytes}` | Fixed load phases, original document snapshots, normalized semantic-ID ordering and source-map crumbs. `decode_document_bytes` admits in-memory bytes. | Reuse version/parsing/normalization checks and snapshots; candidate evaluation needs a confined in-memory source overlay, not writes followed by validation. The module's opening historical version comment is stale; use actual decoder constants. |
| `loader/project_docs.rs::{ModelVersion,DocKind}` | `ModelVersion::parse_exact` accepts `0.2.16`; documents carry `schema_version` and `definitions`. | Formatter version `0.8.0` is not Model version `0.8.0`. Unsupported Model versions must refuse without conversion or write. |
| `loader/frontends/mod.rs::{Node,Value,Scalar,parse_document}` | Spanned maps/sequences/scalars; first non-whitespace `{` or `[` selects strict JSON, otherwise YAML. JSON failure never falls back. Depth 64 and node-count 100,000 bounds. | No comment, blank-group or scalar-style storage in `Node`. Syntax is determined by content, not filename. Keep parser limits and dialect selection. |
| `loader/frontends/yaml.rs::{Builder,ByteMap,resolve_plain}` | Restricted YAML 1.2 block forms, quotes, literal/folded strings and comments accepted. Anchors/aliases/tags/merge keys/flow collections/directives/multi-doc streams refused. | A generic YAML emitter can produce syntax the loader rejects. Need comment-aware spans/trivia and a renderer for exactly this accepted surface. No widening of YAML semantics. |
| `loader/frontends/json.rs`, `loader/source.rs::{Source,LineIndex}` | Strict JSON plus encoding, size and source-offset machinery. JSON scalars and YAML scalars have different accepted numeric surfaces. | Preserve exact values/precision, duplicate-key rejection, Unicode byte positions and encoding policy; no JS parse/stringify numeric oracle. |
| `loader/canonical.rs::Canonical`, `ir/canonical.rs::CompiledProject::to_canonical_json` | Byte-sorted object keys; semantic set ordering; ordinary sequence order retained. | Semantic canonical JSON is available as an independent equality oracle, not a source formatter. Sorting every list would change semantics. |
| `crates/lekalo-core/src/project_fs.rs::{ProjectFs,KIND_STEMS,MODULE_STEMS}` and loader source enumeration | Confined reads, no symlink/junction/reparse traversal; loader enumerates `lekalo/project.yaml`, module markers and seven kind files. JSON syntax can be inside these `.yaml` paths. | Reuse the closed source inventory. Do not recursively rewrite every YAML/JSON file, attachment, target profile, expected output or private runtime record. Explicit `.json` attachment formatting needs its own admitted family owner. |
| `crates/lekalo-core/src/versioning/migration/transaction.rs` | No-follow write capability, digest checks, immutable backups, journal, preflight, rollback and fail-closed recovery. Multi-file replacement is explicitly not one filesystem atomic operation. | Strong safety precedent; it is migration-specific and not already a formatter transaction. Extract shared primitives deliberately or implement a narrowly scoped formatter writer. |
| `crates/lekalo-core/src/result.rs::{DomainResult,Status}` | Valid=0/stdout, invalid=1/stderr, denied=3/stdout, unsupported/unavailable=4/stdout, unsupported-version=5/stderr. | A check diff may use denied=3; parse errors remain invalid=1. Do not redefine existing exits or infer exits from severity. |
| `diagnostics/registry.rs::{embedded,successor,for_version}` | Base registry `0.6.4` and independently admitted successor `0.6.5`; successor has 522 entries. No `format.*` or `LEK-FORMAT-*` identities in inspected registry artifacts. | New codes need a union `0.8.0` registry and explicit producer routing. Preserve all accepted identities, schemas, lifecycle/status constraints and legacy family pins. |
| `tests/fixtures/diff/cases/equal-formatting`, core/CLI `tests/diff.rs`, `tests/ir.rs`, CLI `tests/load.rs` | Existing semantic-equality and YAML/JSON/IR seams. `tests/fixtures/model/valid-planner/` supplies a synthetic Planner source corpus. | Reuse as seed/control evidence; no formatter idempotence, comment preservation or mutation safety proof exists merely because diff works. |
| `scripts/run-golden.mjs::ROLE_PRODUCERS`, golden catalog/coverage gates | Current runner actually invokes a closed set of load/IR/validation/graph producers in external sandboxes. | Formatter file outputs require an explicit sandboxed producer role and output inventory; a check-report snapshot alone cannot prove rewritten file bytes. |
| `.gitattributes`, `.github/workflows/ci.yml::{contracts,build-test}` | LF checkout policy for YAML/JSON; build/test matrix includes Linux, Windows and macOS; suite gates already exist. | Preserve exact-byte policy and add real formatter output comparison. Git LF normalization alone does not prove runtime byte stability. |

Source inspection is the evidence level. No formatter binary, editor formatting session, transaction injection test, full Rust suite or hosted cross-platform run was executed for this research.

## 2. Proposed goal decomposition and CLI

### Implementable increments

| Increment | Proposed owners / result | Completion condition |
| --- | --- | --- |
| I1 Policy, admission and pure planning | `crates/lekalo-core/src/format/{mod,policy,syntax,wire,diagnostic}.rs`; enumerate admitted sources, pin originals, refuse unsupported versions and build a bounded plan. | Exact policy/report contracts, check dispositions and source-selection tests; zero runtime/source writes from planning. |
| I2 YAML/JSON rendering with trivia | `format/{trivia,render}.rs`; accepted source AST plus comment/blank-group ownership, stable scalars/maps/lists and loss detection. | Canonical expected byte vectors, reparse equality, comment preservation/refusal, ordered-array controls and idempotence. Default mode never discards comments. |
| I3 Semantic overlay and explicit apply | `format/{semantic,transaction}.rs`; evaluate candidate overlay through existing loader/IR; confined replacements with preimages, journal and recovery. | All candidates pass equality before any write; invalid later document, races, write failures and interruptions covered without partial file bytes or silent mixed-state acceptance. |
| I4 CLI and CI integration | `crates/lekalo-cli/src/format.rs`, `main.rs`; `format`, `--check`, scoped paths, reports/help and suite producers. | Required commands work; check is read-only even on failure; Planner golden source files and check gate run on the real binary. |
| I5 Editor seam, portability and acceptance | Shared in-memory document service; editor subprocess recipe and future LSP adapter contract; three-OS manifest comparison. | Buffer formatting produces version-bound edits with no disk writes, stale edits refuse, all seven issue ACs have independent evidence. Full LSP server and DSL parser remain separate delivery scope. |

I1/I2 can land before source mutation is enabled, but `format` apply and final issue acceptance require I3-I5. Do not mark the issue complete after only a JSON check report or policy publication.

### Exact proposed command surface

```sh
lekalo format
lekalo format --check
lekalo format lekalo/modules/planner
lekalo format --check lekalo/modules/planner --json
lekalo format --project . lekalo/modules/planner/entities.yaml
lekalo format --allow-lossy-comments --json
lekalo format --stdin --stdin-filepath lekalo/modules/planner/entities.yaml --project . --json
```

**Proposal:** `format [PATH] [--project DIR] [--check] [--allow-lossy-comments] [--json]`, plus the exclusive editor variant `--stdin --stdin-filepath PATH`. Global `--no-cache` remains accepted, but the formatter always bypasses disposable caches. No `--write` is needed: ordinary disk mode applies after complete preflight. `--check` only plans. No implicit migration, adapter invocation, network fetch, Git revision resolution or generated-code formatting occurs.

Root precedence matches `LoadSelection`: explicit `--project`, then `LEKALO_PROJECT`, then discovery. Optional PATH is a safe project-relative exact admitted document or module directory, independent of process cwd once the root is selected; no glob, absolute path, URI, traversal, symlink or junction. Omitted PATH selects the closed Model source set. Scope limits writes, not semantic dependency reads: imported modules may be loaded but are not reformatted. Outside-scope originals stay byte-identical. A folder named planner must resolve through its actual module marker; semantic IDs never come from directory enumeration.

Preserve existing source syntax and document boundaries: JSON source in a governed `.yaml` file stays JSON. Filename changes and generic repository `.json` discovery are not required. Treat attempts to select an unowned JSON/YAML attachment as unsupported, with a concrete explanation; future admitted attachment families need typed canonicalization/equality policies and separate gates.

| Proposed outcome | Status, exit and stream | Observable behavior |
| --- | --- | --- |
| Canonical check, successful apply or unchanged apply | `valid`, 0, stdout | Report says unchanged/changed; no write for an unchanged file. |
| Valid source needs formatting under `--check` | `denied`, 3, stdout | `format.diff`; changed-file inventory available, originals and runtime tree untouched. |
| Malformed source, duplicate keys, bad selection | `invalid`, 1, stderr | Existing parser/usage diagnostics or `format.selection-invalid`; not a mere format diff. |
| Comments would be lost in default mode | `denied`, 3, stdout | Exact bounded loss locations/reasons reported before any write. |
| Unknown/newer Model version | `unsupported-version`, 5, stderr | Existing `versioning.unsupported-version`; do not claim the text is safely format-different. |
| Unimplemented syntax/family/editor selection | `unsupported`, 4, stdout | Honest unsupported result; no fallback emitter. |
| Stale preimage or denied physical path | `denied`, 3, stdout | No source mutation, with fixed reason and safe logical path. |
| Replacement/recovery infrastructure failure | `invalid`, 1, stderr | Verified rollback or explicit recovery-required state; never success after partial application. |

The inherited numeric exit for malformed CLI usage is not reassigned. Report outcome separates a valid formatting diff from parse failure even if another consumer displays both as a failed CI gate. `--allow-lossy-comments` is a deliberate opt-in permitting only enumerated comment losses, never semantic loss, unknown schemas or unsafe writes. Its loss report is emitted before mutation; check remains read-only with that flag. No interactive prompt or hidden config authorizes loss.

### Editor/LSP integration boundary

**Proposal:** `--stdin` consumes one bounded unsaved document snapshot at the admitted `--stdin-filepath`, merges it into an in-memory project overlay, and returns the same plan/report with full-document replacement text and before/after digests. It never writes that buffer or project/runtime files. Reject positional PATH, `--check` and `--allow-lossy-comments` in this mode; editors get comment-preserving text or an explicit refusal. A normal stdout text formatter is not mixed with JSON diagnostics: the JSON result owns replacement text and the normal envelope owns status/diagnostics.

Expose `format_document(snapshot, policy) -> edits/report` for future `textDocument/formatting`: the editor owns document version and UTF-16 range conversion, core owns byte spans/normalized semantics. The client must compare its current version/digest before applying edits and reject stale results. Publish a subprocess recipe and synthetic buffer test now; no existing LSP server is assumed. A future DSL frontend implements the same parse/trivia/semantic-overlay/source-map interface, without adding target-specific IR nodes or inventing DSL syntax in #94.

## 3. Proposed canonical text and safety rules

### Canonical policy

**Proposal:** UTF-8 without BOM, LF line endings and exactly one final LF for source documents; two-space indentation, no tabs or trailing layout whitespace. JSON uses expanded objects/arrays with double-quoted strings. YAML uses block mappings/sequences, lowercase `true`, `false`, `null`, canonical base-10 integers and double-quoted ordinary strings/keys. Sequence entries containing mappings use a standalone `-` followed by an indented mapping, subject to real parser acceptance tests. Empty collections use a form proven accepted by the frontend; if no YAML spelling represents a required empty collection, refuse rather than emit unsupported flow YAML or silently change dialect.

Canonical quoting must preserve decoded string values exactly, including Unicode and escapes. JSON finite-number output follows the existing accepted numeric/canonical rules and must round-trip large integers and negative-zero/float controls; YAML does not gain float syntax. Multiline strings use a pinned literal-block/chomping recipe only when it exactly preserves trailing newline semantics; otherwise use an accepted escaped double-quoted form. Never normalize line endings, path separators or Unicode inside string values as a layout operation.

Mapping keys sort by unsigned UTF-8 bytes within a retained comment/group partition; mapping groups retain their original relative order. Top-level definitions sort by full semantic ID within intentional groups, never by file walk order. Sort only schema-declared set-like imports/references whose current normalization establishes order irrelevance. Preserve duplicates for validation rather than deduplicating them during formatting. Preserve ordered fields, parameter lists, enum/value sequences, scenario steps, expressions and any unclassified array. A path move or metadata convention cannot authorize reordering behaviorally ordered content.

Portable paths are canonical only in owner-declared path fields after existing admission rules: preserve safe logical POSIX paths and refuse forbidden drive/UNC/absolute/traversal syntax. Do not globally replace backslashes or "repair" an invalid import into an admitted one. Definitions, semantic IDs, reference spelling, imports' meaning and source file allocation remain unchanged; any optional lexical type shorthand normalization needs its own explicit proof recipe rather than automatic IR-to-source expansion.

### Comment and intentional-group preservation

**Proposal:** a lossless token/trivia sidecar covers every byte not represented by semantic nodes. Do not detect comments by searching for `#`: quoted hashes and literal/folded block content are string data. Classify header/footer, leading-entry/item and inline comments with concrete byte spans; move each comment with its associated key/definition/item. Blank runs that separate groups collapse to one blank line, with group order preserved; leading and trailing decoration has a fixed stable placement. Comment content is preserved, while indentation/EOL follows layout policy.

Every accepted comment must have exactly one output owner. Ambiguous attachment, uncovered trivia, unsupported parser boundary or an emitter incapable of representing a comment causes `format.comments-loss` before any disk mutation. Lossy opt-in lists each omitted comment by safe path, span and fixed reason; raw private comment text does not enter ordinary reports. Both default success and explicit lossy output must be idempotent under their policy. Comments-only or empty optional documents need a byte-preserving/no-op case or a precise unsupported result, never silent deletion.

Grouping is a documented deterministic input, not a universal sort order erasing author intent. Equivalent group-free permutations produce the same canonical text; differently grouped inputs may retain different text while having equal semantic Model/IR. Repeat formatting of either must be stable. Import sorting follows the same comment ownership/group boundaries.

### Semantic admission, atomic writes and recovery

**Proposal pipeline:** confined root/inventory -> pinned byte snapshots -> strict parse and exact version admission -> trivia ownership -> canonical render -> reparse all candidates -> candidate overlay load/compile -> compare normalized Model and canonical IR excluding source map -> build bounded report -> optional mutation preflight/apply. All selected files are planned and verified before the first replacement, including cases where a malformed file sorts last. An unrelated semantic failure that prevents the oracle refuses apply rather than guessing equality.

Require equal canonical Model and IR payloads, stable IDs/references and empty semantic diff through the existing diff owner. Separate source-map expectations are recomputed over output bytes; old offsets are not compared for equality. Formatting changes raw source fingerprints and may invalidate stale source evidence/cache records even when semantic payload digests stay equal; report both domains accurately. Do not rehash or rewrite lockfiles, waivers, manifests, external evidence or generated artifacts to hide that invalidation.

Use digest-bound before/after snapshots and a narrowly derived non-cloneable write capability. Revalidate physical path components and file preimages at every replacement, reject links/reparse points and concurrent source edits, and preserve file permissions where supported. Write a complete candidate to a same-directory temporary regular file, sync it and use a qualified platform replacement operation; original files are never truncated in place. No-op apply avoids a replacement or mtime change.

Multi-file atomicity means all-path preflight, per-file atomic replacement, verified rollback on ordinary failure and durable recoverability after interruption; portable filesystems do not provide a single atomic multi-file commit. **Proposal runtime home:** `.lekalo/cache/format/<planId>/` for immutable backups/manifest/journal, with a formatter active lock distinct from `lekalo.lock`. This new home needs structure/authority admission before implementation. Recovery material is not a disposable success cache and cannot be purged while active. Shared loader entry paths must detect active formatter recovery state as they already do migration recovery; check reads state and refuses, never recovers or creates a lock.

Explicit mutating `format` recovers an interrupted transaction to verified original bytes before planning new work. Crash injection must cover journal creation, first/middle/last replacement, rollback and journal removal. If rollback is incomplete, readers fail closed and report `format.recovery-required`; no command claims the mixed tree is a canonical successful project. Cleanup after success/failure must preserve unrelated runtime state and leave no temp source files. Bounds reuse loader ceilings plus explicit total candidate-byte/file/edit/loss limits; reaching a bound refuses the whole plan, with no truncated clean report.

## 4. Proposed contracts and diagnostics

### Contract artifacts to add or integrate

| Proposed artifact / exact identity | Content and ownership | Version/freeze implication |
| --- | --- | --- |
| `contracts/format-policy.schema.v0.8.0.json`, `contracts/format-policy.v0.8.0.json`; `dev.lekalo.format-policy@0.8.0`, discriminator `lekalo/format-policy/v0.8.0` | Closed deterministic policy: encoding/EOL/final-newline/indent/quotes/scalars, per-schema set-list/path rules, trivia/group policy, limits and policy digest recipe. Built-in pinned policy, no user style config in first increment. | New family at 0.8.0; admitted Model version is a separate exact pin, not the policy version. |
| `contracts/format-report.schema.v0.8.0.json`; `dev.lekalo.format-report@0.8.0`, discriminator `lekalo/format-report/v0.8.0` | Closed command payload with mode/scope/policyRef/modelRef/irRef, byte-sorted file rows, before/after SHA-256, action, equality result, loss rows and summary. Editor rows additionally hold declared replacement text; disk/check rows omit it. | New family at 0.8.0. Refuse unknown members/discriminators; no arbitrary provider extension payload. |
| `contracts/diagnostic-registry.v0.8.0.json` | Union registry containing unchanged existing entries plus the new family below; embedded routing and all producer normalizers select exact version. | Changed registry at 0.8.0; preserve 0.6.4/0.6.5 artifacts and old consumers. Registry-schema successor only if its actual closed shape/identity rules must change, otherwise freeze the schema. |
| Formatter transaction manifest/journal decoder | Closed local runtime record of plan ID, policy digest, safe paths and before/after/backup hashes; never a semantic Model or public evidence document. | If published as a contract, add `contracts/format-transaction.schema.v0.8.0.json`; otherwise explicitly internal and versioned in the runtime reader. No invented reuse of migration wire records. |
| Existing diagnostic envelope and fixture/catalog consumers | Add exact registry/report admission and actual file-output producer roles. Changes to closed wire shapes require reviewed successors. | Diagnostic envelope, Model/IR, diff, lock, protocol, fixture-provenance and suite contracts stay at accepted pins if their bytes/semantics do not change. If changed, publish 0.8.0 successors and update exact constants/refs/tests, not in-place edits under old versions. |

**Proposal report invariants:** scope membership and file paths are exact; `selected = changed + unchanged`, no duplicate rows; all successful planned rows have known before/after digests and semantic equality; applied counts cannot exceed validated changed rows. Check has zero applied files. A loss count can be nonzero on success only for explicit disk lossy mode. Incomplete plans disclose incomplete state and cannot report equality or zero changes as known. Hash domain is exact source bytes including the final LF; policy digest excludes only its explicitly defined self-digest member. Report digests never substitute for inspectable output bytes.

No always-persisted report, repository formatter config or new canonical source manifest is required. Human output lists safe changed paths/reasons; ordinary JSON excludes raw source/comment contents and host paths. The explicit editor response carries authorized selected buffer text, with bounds and no unsolicited external export. Authority/privacy classifiers and structural policies must admit any new runtime or externally consumed artifact kinds before using them; formatting does not widen export permission.

### Proposed diagnostic family and registry implications

Reuse existing `loader.json-parse / LEK-LOAD-012`, `loader.yaml-parse / LEK-LOAD-019`, `loader.duplicate-key / LEK-LOAD-005`, `loader.yaml-unsupported / LEK-LOAD-020`, encoding/limit/path/structure diagnostics, and `versioning.unsupported-version / LEK-VER-018`. Parse failure is never relabeled as `format.diff`.

| Proposed rule / new code | Trigger / status | Closed safe data |
| --- | --- | --- |
| `format.diff / LEK-FORMAT-001` | Valid source requires canonical formatting in check; denied. | Policy digest and changed-file count; file inventory in report, not unbounded diagnostics. |
| `format.comments-loss / LEK-FORMAT-002` | Unrepresentable/ambiguous comment ownership; denied by default, warning under explicit lossy valid apply. | Safe path/span, loss kind/count, policy ref; no raw comment text. |
| `format.semantic-change / LEK-FORMAT-003` | Candidate changes normalized Model/IR or fails round-trip admission; invalid, no writes. | Stage and fixed mismatch reason, logical path where known. |
| `format.selection-invalid / LEK-FORMAT-004` | Invalid/unresolved admitted source selector; invalid. | Selector kind and bounded fixed detail; unsafe paths use security owner diagnostics. |
| `format.source-changed / LEK-FORMAT-005` | Preimage changed before commit or recovery conflicts; denied. | Safe path, expected/current digest refs. |
| `format.write-failed / LEK-FORMAT-006` | Temp/sync/replace/backup failure; invalid. | Safe path, stage, rollback disposition; no OS exception/host-path echo. |
| `format.recovery-required / LEK-FORMAT-007` | Interrupted/incomplete transaction makes normal read/apply unavailable; invalid. | Plan ID and closed recovery reason. |
| `format.limit-exceeded / LEK-FORMAT-008` | Formatter-specific bytes/edits/loss/work limit; invalid. | Bound name and measured/allowed counts. |
| `format.unsupported-surface / LEK-FORMAT-009` | Nonadmitted source family or unavailable frontend/editor context; unsupported. | Family/frontend kind and fixed reason. |

These nine identities are proposals, not reserved/emitted codes. Recheck collisions against the merged M8 registry, retain all prior active/reserved/retired entries, use existing categories (`model`, `semantic`, `infrastructure`, `security`, `compatibility`) rather than adding a category named formatting, and register exact severity/status/message/location/data constraints. Diff is a formatting gate disposition, not a model invalidity or subjective architecture finding. No safe-fix ID or executable action is invented; #74 owns future recipes and #96 consumes the same stable registry explanations.

## 5. Proposed fixture and gate families

### Fixtures and independent oracles

**Proposal family:** `tests/fixtures/format/{yaml,json,comments,groups,ordered,imports,versions,invalid,scope,atomic,editor,planner,golden}/`. Register it as synthetic in `tests/fixtures/fixture-provenance.json`, with explicit seed origins. Do not format the entire existing fixture tree: deliberate malformed bytes and parser inputs are evidence. Host-specific attacks/CRLF/BOM/corrupt UTF-8 are materialized from recipes in fresh external roots rather than rewritten by Git checkout.

| Family | Required positive/negative cases and independently expected facts |
| --- | --- |
| YAML/JSON canonical bytes | Permuted keys/definitions, quotes/escapes, booleans/null/integers, JSON numeric boundaries, multiline/chomping and empty collections. Exact expected bytes must be authored/reviewed, not generated and immediately accepted by the same renderer. |
| Comments / groups | Header/footer/inline/leading-item comments, blank partitions, quoted `#`, literal-block `#`, non-ASCII comments, comments-only optional files, ambiguous ownership and explicit lossy opt-in. Inventory comment tokens/owners independently; assert none disappears silently. |
| Ordered data / imports | Scenario steps, ordered fields/values/parameters, nested expression lists retain order; set-like imports sort with their comments. A deliberate ordered-item swap changes semantic diff and must be refused. Unsafe import/path string is not repaired. |
| Parse / schema / limits | Duplicate keys, late malformed file, JSON parse no-fallback, forbidden YAML feature, invalid encoding, unsupported/newer/mixed versions and exact boundary/one-over limits. All originals and runtime inventory remain untouched on planning failures. |
| Scope / filesystem | One module versus all, explicit root/env/cwd precedence, imported external-to-scope modules, other source siblings and user-owned sentinels. Traversal/absolute/drive/UNC/symlink/junction/case-collision controls; no outside read/write authority. |
| Atomic / concurrency | Stale preimage, parallel mutators, read-only permissions, Windows destination lock, temp/sync/replace/rollback faults, crash checkpoints and conflicting recovery. Check raw bytes plus journal/reader state; an error exit alone is insufficient. |
| Read-only check | Canonical/noncanonical/invalid/unsupported/lossy cases; source/runtime pre/post inventories, byte hashes and relevant file metadata equal, no temp/cache/lock/backup/report creation. Run with unwritable directories. |
| Editor / source maps | Unsaved buffer overlay, multibyte text and CRLF input, exact replacement text, recomputed locations, UTF-16 client conversion and stale version/digest rejection; sentinel disk source stays unchanged. |
| Planner / semantic equality | Synthetic copy from `model/valid-planner` reconciled with accepted IR/diff fixtures, canonical YAML plus JSON-syntax twin. Format -> format -> check; Model/IR semantic payloads equal and real diff `changes` empty. Expected source-map offsets track each output. |
| Portability / diagnostic coverage | Same policy/corpus on three OSes, distinct roots/locales/timezones/enumeration orders. One executed trigger/non-trigger pair per new diagnostic, no exemption from suite coverage. |

### Exact proposed gates and CI wiring

New core and CLI integration files: `crates/lekalo-core/tests/format.rs`, `crates/lekalo-cli/tests/format.rs`. New script gates: `scripts/test-format-contracts.mjs` (schema/native admission and registry/report invariants), `scripts/test-format-cli.mjs` (real command/text/check/scope/editor behavior), `scripts/test-format-atomic.mjs` (transaction/race/fault/recovery), and `scripts/test-format-portability.mjs` (canonical artifact manifest, repeat/no-op and EOL/path recipes). Each fails if its required binary/case/output is missing; no skip-success path.

Proposed future commands, not executed in this research:

```sh
cargo test --locked -p lekalo-core --test format
cargo test --locked -p lekalo-cli --test format
cargo build --workspace --locked
node scripts/test-format-contracts.mjs
node scripts/test-format-cli.mjs
node scripts/test-format-atomic.mjs
node scripts/test-format-portability.mjs
lekalo format --check --project tests/fixtures/format/planner/canonical --json
lekalo load --project before-project --ir --json
lekalo load --project after-project --ir --json
lekalo diff before-project after-project --format json --json
node scripts/test-semantic-diff-contracts.mjs
node scripts/test-diagnostic-contracts.mjs
node scripts/test-fixture-provenance.mjs
node scripts/test-golden-catalog.mjs
node scripts/test-golden-diagnostic-coverage.mjs
node scripts/test-golden-normalization.mjs
node scripts/test-golden-determinism.mjs
node scripts/check-contract-versions.mjs --base HEAD^
```

`before-project`/`after-project` are sandbox roots created by the harness, not committed user paths. Capture output on its actual status-owned stream and inspect semantic payloads, not full load envelopes containing new positions/source fingerprints. Add formatter producer/output roles to the suite catalog/runner with exact per-file inventories, checksums and reviewed update records. Retain existing semantic, loader/IR, diagnostic, project-fs, versioning/migration and golden regressions.

Wire schema-only checks into existing Node 18/24 `contracts`; wire all binary-dependent formatter gates into `build-test` on Linux/Windows/macOS **after a build in the same job**. Use the existing exact Ajv provisioning through `LEKALO_AJV_NODE_PATH` and `NODE_PATH` where required. Upload actual canonical source artifacts plus sorted relative-path/byte-count/SHA-256 manifests; a dependent comparison job requires the same policy/case inventory and byte-identical files across all three OSes. Windows/Linux/macOS green local tests without this comparison do not prove cross-platform equality. Run fmt/clippy/MSRV/workspace gates appropriate to later code changes, with focused/local/hosted evidence reported separately.

## 6. Acceptance criteria mapped verbatim

Each criterion below is copied verbatim from #94. The evidence column describes eventual implementation acceptance, not completed formatter behavior.

| Issue acceptance criterion (verbatim) | Proposed increment / required evidence |
| --- | --- |
| Repeated formatting is idempotent. | I2-I4: `F(F(source)) == F(source)` as exact file bytes on YAML/JSON/comments/groups/Planner; second apply changes no source mtime/inventory and check returns valid. Lossy output also stabilizes under its explicit policy. |
| Linux/Windows/macOS produce canonical committed output. | I5: fresh real-binary runs on the three OS jobs match independently committed canonical source goldens and each other's complete output manifests/bytes. CRLF/Unicode/root/enumeration cases are included; no `trim()` or broad normalizer hides drift. |
| Comments are preserved or loss is explicitly reported before write. | I2-I3: exact token-owner preservation controls and quoted/block hash negatives; default ambiguous/unrepresentable comment fails before source writes. Explicit opt-in exposes loss report first and preserves semantic payloads; check never writes. |
| `format --check` usable in CI. | I1/I4: documented 0/3/1/5/4 status distinctions, JSON/human results and root/scope selectors; required CI step runs without prompts and proves no source/runtime/temp/lock writes on all outcomes. |
| Semantic diff remains empty after formatting. | I2-I3: real loader/IR and `lekalo diff before-project after-project --format json --json` on produced outputs prove equal semantic payloads and zero changes, including moved comments/reordered set-like imports; ordered-step mutation control is detected. |
| Invalid file is not partially rewritten. | I1/I3: malformed/unsupported late source blocks every replacement; atomic per-file write, preimage/race and fault/crash/rollback/recovery probes prove originals intact or explicit fail-closed recovery state. No in-place truncation or falsely successful partial project. |
| Planner fixture has stable formatted representation. | I4-I5: committed synthetic Planner canonical source tree with lineage, reviewed expected bytes, YAML/JSON equality, idempotence/check, empty semantic diff and three-platform exact output receipts. |

Rules/requirements coverage beyond the checklist: deterministic keys/definitions and IDs independent of filesystem order -> canonical/group/permutation controls; indentation/quotes/scalars/list style -> byte vectors plus ordered-list negative; normalized portable paths -> admitted path-field tests without string rewriting; one newline policy -> exact-byte portability; comments/grouping -> trivia owner tests; no semantic changes -> independent Model/IR/diff oracle; import sorting -> set-only/comment-aware controls; future DSL -> neutral frontend/source-map service boundary. Parse error versus diff, read-only check, atomic writes, unknown/newer schema refusal, comment-loss blocking, editor/LSP seam and cross-platform stability each have explicit owners and fixture families above.

## 7. Risks and non-goals

- **Comments are absent from the current AST:** span offsets alone do not prove comment attachment. Require full trivia coverage and refusal before apply when ownership is ambiguous; selecting a generic YAML library does not establish losslessness.
- **Formatting can alter behavior:** sorting all arrays, quoting a scalar incorrectly, mishandling block chomping, replacing separators inside strings or widening YAML syntax can change semantics. The independently loaded candidate and ordered-list controls are hard gates.
- **Grouping and global canonical order conflict:** preserve group partitions explicitly and sort inside them. Do not promise identical text for differently grouped documents; promise idempotence and semantic equality.
- **JSON scope differs from extensions:** the current loader accepts JSON syntax in governed `.yaml` sources. Supporting arbitrary `.json` files or typed attachments requires family admission; filename recognition alone cannot provide semantic safety.
- **Atomicity and recovery are cross-owner work:** formatter journal state must block relevant readers, survive interruption and resist cache cleanup. A lone `rename` call does not make a multi-file operation atomic; reuse migration guarantees only after shared-owner integration tests.
- **Raw evidence can become stale:** unchanged IR does not imply unchanged source fingerprints. Do not silently refresh locks/native evidence or label every full envelope byte equal after formatting.
- **Version/registry integration:** M8 target 0.8.0 is ahead of the inspected 0.6.5 product. Preserve the merged union and exact unchanged family pins; implementation must coordinate shared successors rather than overwrite earlier registry entries.
- **Platform replacement and numeric output:** Windows locked files, Unix modes, Unicode columns, integer precision and float spelling need actual platform probes. Three independently green jobs need a common artifact comparison to establish equal bytes.
- **Editor acceptance is limited:** buffer formatting/edit freshness and documented LSP seam are in scope; a production LSP server, editor marketplace extension and end-user IDE session require their owning work. A subprocess test is not proof of a full LSP deployment.

**Non-goals:** DSL grammar/parser/emitter implementation; arbitrary repository or target-code formatting; configurable style preferences; semantic operation execution (#73), safe-fix recipes (#74), explain-command implementation (#96), schema migrations, generated output rewrites, import repair, semantic ID changes, dependency downloading, external calls and real/private consumer access. Future DSL and semantic-operation callers reuse the pure service under their own authorization. None of these expands this research dispatch beyond `docs/m8/`.

Research validation: `node scripts/check-structure.mjs` passed (15 valid, 1 malformed, 6 denied fixtures), `node scripts/check-authority.mjs` passed (12 allowed, 27 forbidden, 15 malformed), and `node scripts/check-privacy.mjs` passed against the accepted 0.3.2 policy with 57 artifact kinds. `node scripts/check-contract-versions.mjs` passed for product 0.6.5 and 138 contract artifacts against HEAD; no contract changed. Document-specific checks verify existing owner paths, exact seven live AC strings, nine noncolliding proposed codes, seven-section shape and UTF-8/LF bytes.

Delivery: stage only `docs/m8/issue-94-research.md`, verify the staged path set and `git diff --cached --check`, then commit on the current branch with `docs(m8): research issue-94` and confirm the file exists in HEAD. No formatter runtime tests, Rust build, fixture generation or hosted CI are claimed by this document; implementation and its acceptance evidence remain future work.

# Issue #121 — local-only run history and metrics recorder

## 1. Scope and inspected evidence

- Acceptance authority: [issue #121](https://github.com/ichinya/lekalo/issues/121), read in full with `gh issue view 121 --repo ichinya/lekalo` on 2026-09-30; milestone M6, dependencies #10/#11/#17/#22/#85/#87/#119/#120, consumers #100/#102/#118.
- Checkout: branch `ichinya/m6-issue-121`, clean initial tree, baseline `3cf3c701bffac3aa6c7bbf637bb7e0538f776539`; workspace product version `0.4.0`, Rust MSRV `1.80.0`. The issue branch already had the required name.
- Read #119 and #120 live, `docs/privacy.md`, `docs/privacy-runtime.md`, `docs/adr/0002-privacy-export-policy.md`, and M5 examples `docs/m5/issue-57-research.md` / `issue-72-research.md`. Follow their existing-implementation map, acceptance matrix, ordered steps, fixtures, gates and risks convention.
- This is research only. Only this document changes; every path marked **new** is future implementation, not delivered functionality. No private consumer repository, source transcript, paid provider, push or PR is needed.
- Recorder owns local observations, measurement storage and dependent-reference invalidation. #120 owns policy decisions; #119 owns classification/redaction/leak enforcement; #100 executes Framework Lift experiments; #102 alone constructs and reevaluates public aggregate payloads. Recorder must not fetch pricing or infer provider billing.

## 2. Existing seams and concrete gaps

Paths under `core/` below expand to `crates/lekalo-core/src/`.

| Existing file / symbol | Reuse; remaining gap |
| --- | --- |
| `core/lib.rs`, `crates/lekalo-cli/src/main.rs::{Commands,main}` | Core has domain modules; CLI is a Clap dispatch edge. No generic run-history module or command. Add one module rather than overload scenario evidence or cache. |
| `core/result.rs::{DomainResult,Status}`; `core/diagnostics/{registry,normalize,types}.rs`; `contracts/diagnostic-registry.schema.v0.4.0.json` | Preserve established exit/stream semantics and stable codes. Recorded operation verdict is distinct from success/failure of storing the record. |
| `core/scenario_evidence.rs::{RunRecord,RunSummary,INGEST_DIR}`; `contracts/scenario-run.schema.v0.4.0.json` | Per-assertion evidence exists in `.lekalo/import/scenario-runs/`; status vocabulary includes `degraded`. It is not a general metrics/history store. Consume sanitized typed rows without copying test paths or original JSON. |
| `core/native_gate/{types,receipt,runner,evidence}.rs`; `contracts/native-gate-run.schema.v0.4.0.json`; `core/nfr/report.rs` | Gate outcomes and value-state measurements already exist. Explicit mapping must retain blocked/missing/security/unsupported distinctions; no stdout/stderr/commands in history. |
| `core/context/mod.rs::Capsule::{estimated_tokens,candidate_count,included_count,excluded_count,roots}` | Extract capsule counts and estimator identity, never `to_markdown()` or capsule fact bodies. Byte size must identify which rendered representation was measured. |
| `core/lockfile/{types,verify,resolution}.rs`; `core/target_profile/{document,resolution}.rs`; `core/adapter_package/{manifest,integrity}.rs` | Pin actual lock/profile/adapter bytes and versions; directory names and semver alone do not prove exact repeated inputs. |
| `core/privacy/{refs,context,types,vocab,input,evaluate,redact,export}.rs`; `scripts/check-privacy.mjs` | Reuse trusted context, exact refs, value-state semantics, project classification floor and leak primitives. Never call `run_export` to implement recorder output. |
| `core/cache/{path,sqlite}.rs` | Existing bundled `rusqlite = 0.32.1` and transactional persistence; cache uses WAL and is disposable. History needs independent ownership, deletion and provenance; do not put evidence in cache. |
| `core/project_fs.rs`, `core/observed/store.rs::write_confined`, `core/target_protocol/confinement*.rs` | Read confinement and component/link checks are useful precedents. Observed writer's fixed temporary filename/rename is not a concurrency/durability proof for a multi-record ledger. |
| `crates/lekalo-cli/src/git_input.rs`, `doctor_git.rs` | Read-only Git argv precedent, bounded failures without stderr or host paths. New provenance reader must not reuse impact's requirement for changed files. |
| `scripts/test-*.mjs`, `.github/workflows/ci.yml`, `scripts/check-contract-versions.mjs`, `docs/versioning.md` | Strict Draft 2020-12/Ajv 8.17.1, canonical goldens, Rust parity, fixture provenance, Linux/Windows and MSRV gates. No root `package.json`; Ajv is provisioned outside checkout in CI. |

### Privacy custody mismatch and authority prerequisite

The prose in `docs/privacy.md` describes policy `0.2.16`. Actual Rust `privacy/refs.rs`, embedded `TrustedContext`, and JS `check-privacy.mjs` select policy/authority **0.3.2**, with some satellite contracts still **0.2.16**. Do not use the prose's older digests or infer a single version for all satellites. Current exact anchors are:

- `policyRef = {policyId: "dev.lekalo.privacy-export-policy", version: "0.3.2", digest: "sha256:5a80966fa628fd4c9452325d34e7191f40ebb7a9cb49185c91d413861fb18384"}`.
- `authorityRef = {contractId: "dev.lekalo.authority-matrix", version: "0.3.2", digest: "sha256:7ae6454ea20f7b61202d368411ef9bff4e70af96f1f2a408c209d84fe9722f80"}`.
- Manifest bytes: `sha256:760f64bac4f2dd3e62e68b92a97f251316a215278a841d32b9e5b98a21302bf8`; trusted policy raw bytes have a different digest from the semantic `policyRef.digest`.

`contracts/authority-matrix.v0.3.2.json` has no recorder-owned `.lekalo/history/**` kind. `metrics.evaluation-evidence` is HLV-owned, permits only HLV writers and `.hlv/evidence/metrics/**`; its policy default is `shareable-with-redaction`. It cannot authorize Lekalo history. S1 must add accepted successor registry entries for recorder-owned records, assertion sets, local dependents and store metadata, with explicit Lekalo writers/readers and **local-private** defaults. Proposed kind IDs are `history.run-record`, `history.assertion-set`, `history.dependent-reference` and `history.store-metadata`; they are not accepted kinds until that successor passes #2/#120 custody rules. No runtime aliases, taxonomy overrides or bypass of unknown-kind denial.

## 3. Acceptance map

| Issue acceptance criterion | Delivery and required proof |
| --- | --- |
| AC1: fully offline history | S2/S4/S5; record/list/show/delete/recover with empty account/provider settings and outbound networking denied; no remote Git or adapter processes launched. |
| AC2: exact repeated revision/profile provenance | S1/S3; immutable Git/model/IR/lock/core/adapter/profile pins, dirty-state disclosure, repeat links and comparable-input checks; alter each pin independently and reject exact-repeat claims. |
| AC3: missing token/cost stays unknown | S1/S3; required value-state wrappers; absent harness observations normalize to unknown, known zero round-trips, withheld/unsupported remain distinct. |
| AC4: assertions stored separately from metrics | S1/S2/S3; separate schema and SQL table, digest-bound assertion reference, no assertion text in metrics; timings cannot produce a passing assertion. |
| AC5: retention/delete updates indexes/references | S2/S4; one transaction removes row/metrics/assertions and invalidates transitive dependents; crash/concurrency tests, stale reader and generation checks, rebuild cannot resurrect evidence. |
| AC6: no snippets/prompts/secrets/absolute paths by default | S1/S3/S5; closed allowlist, conservative field classes, forbidden-content rejection before persistence, serialized bytes and errors scanned; no raw-record escape hatch in M6. |
| AC7: greenfield/brownfield use one versioned schema | S1/S3/S5; paired synthetic pilot fixtures compiled against the same schema; optional scope coverage represented explicitly, untouched legacy remains observed. |
| AC8: recorder has no export | S1/S4/S5; no export/upload/aggregate command, arbitrary output destination or shareable raw envelope; generic privacy exporter denies recorder kinds; #102 only gets validated local dependent references. |

## 4. Run record and privacy classification of every field

Use **new** `contracts/run-record.schema.v0.4.0.json` with identity `dev.lekalo.run-record@0.4.0` and discriminator `lekalo/run-record/v0.4.0`, if product remains 0.4.0 at implementation. All nested objects are closed (`additionalProperties: false`), arrays bounded and duplicate-free where set-like. Unknown versions refuse; never silently convert old records. Use current product version in all proposed schema filenames/identities if it changes before implementation.

Class codes below expand to #120 labels: **P** public contract metadata, **I** internal, **C** confidential, **T** tenant-scoped, **R** retention-limited, **F** financial. Every persisted runtime object also inherits **T/R** and `exportDisposition: local-private`, even if a leaf is P. Container sensitivity is the union of its children and the project floor; labels cannot lower inferred/project sensitivity. No invented privacy vocabulary. Security-sensitive/PII/health classification propagates restrictively; forbidden content is rejected, not persisted under a stronger label. Empty/unknown classification cannot authorize a write.

| Closed run-record field(s) / exact child set | Shape and origin | Leaf sensitivity / handling |
| --- | --- | --- |
| `schema_version`, `identity`, `artifactKind` | Exact schema/accepted successor kind discriminators | P; `artifactKind` must resolve in trusted registry. |
| `runId`, `timestamp`, `recordedAt` | Random 128-bit opaque run token, bounded UTC RFC3339 occurrence and ingestion timestamps; no username/host-derived ID | I; timestamp may correlate activity, inherits T/R. |
| `scope.{repositoryId,tenantScopeId,repositoryRole}` | Locally generated random IDs and #120 role alias; never repository name, remote URL, raw tenant ID or path hash | IDs C/T, role P. Physical binding is separate local metadata, not a claim that privacy's content-derived repository token proves host identity. |
| `pilot.{mode,scopeState}` | `mode: greenfield \| brownfield`; `scopeState: observed \| contracted \| hybrid` | I; measured scope only, no promotion of untouched legacy to contracted. |
| `provenance.git.{commit,dirty,workingSetDigest}` | Each member is a value-state; exact local commit, explicit dirty flag, aggregate digest of relevant dirty-input bytes | C; no branch name, author, patch, filenames or per-snippet fingerprints. Missing Git stays unknown. |
| `provenance.model.{revision,digest,irDigest}` | Declared model revision as value-state, exact full canonical model and compiled IR digests as value-states | C; whole governed artifacts only, never a hash of small-domain source text. |
| `provenance.lock.{version,digest}` | Actual parsed lock version and exact-byte digest, each value-state | Version I, digest C; missing lock is unknown rather than empty-file hash. |
| `provenance.core.{version,buildRevision,buildDigest}` | Executing core product version, build commit/digest wrappers | Version P, build pins I; reproducibility incomplete when build provenance is unknown. |
| `provenance.adapters[] .{id,version,manifestDigest,bundleDigest}` | Resolved adapter list; stable ID and exact installed manifest/bundle pins, value-states for unobserved pins | ID C, version P, digests C; no package URL, executable path or command line. |
| `provenance.profile.{id,version,digest}` | Effective resolved profile identity/version and canonical digest including policy/target choices; nullable state wrappers | ID C, version I, digest C; no profile absolute path. |
| `provenance.harness.{id,version,modelId,modelRevision}` | Closed, validated identifier/version value-states supplied by harness | I for version, C for IDs/revision; no account, endpoint, credential, model prompt or response. |
| `operation.{kind,affectedSemanticIds}` | Versioned supported-operation enum and bounded sorted stable semantic IDs | Kind I; IDs C; accept validated model IDs only, reject arbitrary free text/paths. |
| `status.{outcome,coverageState}` | `pass \| warn \| fail \| unsupported \| infrastructure`; `complete \| incomplete \| unknown` | I; operation result, independent of recorder exit. |
| `metrics.{durationMs,filesRead,filesChanged,toolCalls,retryCount,replanCount}` | Nonnegative bounded count/duration value-states from actual observation | I; file counts only, no file list; tool-call count only, no arguments/output. |
| `metrics.tokens.{input,output,reasoning,total}` | Independently observed nonnegative integer value-states | I; harness-only, no tokenizer guesses; total remains unknown unless reported. |
| `metrics.cost.{amount,currency,basis}` | Amount is nonnegative decimal-string value-state; currency and `reported \| estimated` basis are value-states | F; no provider pricing lookup. Estimated never silently presented as billed. Missing currency makes cost incomplete. |
| `metrics.context.{bytes,estimatedTokens,includedFacts,candidateFacts,coverageRatio,representation,estimatorVersion}` | Value-states; byte count binds `json \| markdown`; ratio only known with known nonzero denominator | I; capsule metadata only. Zero candidate facts yields unknown ratio, never manufactured 100% coverage. |
| `measurementSources[] .{field,sourceKind,sourceId,sourceVersion}` | Closed field-path enum pointing to metrics leaf, `core \| adapter \| harness \| derived`, safe ID/version wrappers | Field/kind P, source ID C, version I; one source per known metric; no arbitrary attachment metadata. |
| `testGateSummaries[] .{id,kind,sourceOutcome,outcome,passed,failed,unsupported,infrastructure,coverageState,evidenceRef}` | Safe test/gate ID, `test \| gate \| nfr`, versioned source outcome and mapped outcome, independent count wrappers, coverage and optional local evidence ref value-state | ID/ref C; kind/status/counts I. References are store-local opaque IDs, not filenames or URLs. |
| `diagnostics[] .{code,severity,count}` | Registered stable code, registry severity, positive known count; sorted/deduplicated | P for code/severity, I for count. No message, spans, suggestions, interpolated details or tool stderr. |
| `assertionsRef.{setId,digest,count}` | Separate assertion-set token, canonical digest and count; nullable only when no assertions supplied | ID/digest C, count I; absence means no assertion evidence, not pass. |
| `repeat.{parentRunId,inputFingerprint,comparability}` | Parent reference value-state, full pin-tuple digest value-state, `exact \| changed \| incomplete` | C; exact requires live same-scope parent and all required known matching pins. No rerun command/raw prompt retained. |
| `privacy.{dataSensitivity,exportDisposition,policyRef,authorityRef,classificationContractRef,provenance,exportEligibility}` | Nonempty closed labels, local-private, exact typed references; provenance child set `origin,repositoryRole,derived,sourceRefs`; eligibility constant `ineligible` | Labels I; exact ref children `{policyId/contractId,version,digest}` P; provenance source refs C, role/origin/derived I. Source refs are opaque local tokens; no license file contents or personal consent data. |

`value-state` above means the existing #120 shape: `{ "state": "known", "value": 0 }` versus `{ "state": "unknown" }`, `{ "state": "withheld" }`, `{ "state": "unsupported" }`. Only known may carry a value. Reuse `privacy::ValueState` semantics with schema-specific numeric/string restrictions; metric absence is normalized to unknown before serialization. Null, absent, empty string and zero are not alternate unknown spellings. Preserve a known zero and reject negative/nonfinite/range-overflow values, duplicate JSON keys and contradictory wrappers. Distinguish capsule estimated tokens from provider-reported tokens.

**Separate assertions:** **new** `contracts/run-assertions.schema.v0.4.0.json` has closed fields `schema_version,identity,artifactKind,setId,runId,scope,policyRef,authorityRef,dataSensitivity,exportDisposition,rows`. Scope/refs/classes use the rules above. Each row is exactly `{assertionId,subjectSemanticId,kind,outcome,evidenceRef}`: IDs/references C/T/R; fixed kind/outcome I/T/R. No expected/actual values, expressions, failure prose, source excerpts or metric fields. Preserve source `degraded` as recorder warn plus incomplete coverage, unsupported as unsupported, infrastructure as infrastructure, and fail as fail. Assertion summaries are derived from rows; no rows is unknown evidence. Store assertions separately; history's status cannot substitute for their evidence.

**Store-only metadata and dependents:** define **new** `contracts/run-history-store.schema.v0.4.0.json`. Closed root `{schema_version,identity,repositoryId,tenantScopes,generation,retention,dependents}`. `tenantScopes[]` contains `{tenantScopeId,createdAt}`; IDs C, date I. `retention` contains `{maxAgeDays,maxRecords,maxBytes}`, I; `generation` I. Each dependent is `{id,kind,sourceRuns,state,generation}` with kind `index \| claim \| aggregate-input`, state `valid \| invalidated`, ID/sourceRuns C and remaining fields I. Each source reference is `{runId,recordDigest,assertionDigest}` (C, assertion digest nullable). No dependent payload/free text. Physical store location is an in-memory capability, never a persisted absolute path. Index columns are allowlisted projections of these classified fields; no extra field can enter indexes, journal diagnostics or tombstones unclassified.

## 5. Storage, isolation, recovery and deletion

Recommended default: independent SQLite database using already bundled rusqlite, not append-only JSONL plus separately rewritten indexes. Single transactional ownership makes AC5 testable without distributed commit.

```text
<project>/.lekalo/history/
  .gitignore                         # generated '*' protection, no runtime content tracked
  store.sqlite                       # store identity, scopes, retention, run and dependent tables
  store.sqlite-journal               # SQLite-owned transient rollback journal, when present
```

- **New** `core/run_history/{store,path,retention,recovery}.rs`: `HistoryStore::{open,append,list,get,delete,prune,recover,register_dependent,resolve_dependent}`. Tables: `store_meta`, `tenant_scopes`, `runs` (canonical metadata + metrics JSON), `assertion_sets` (separate canonical JSON), `run_index`, `dependents`, `dependent_sources`; foreign keys and composite `(repositoryId,tenantScopeId,runId)` lookup. Insertion binds assertion digest and record digest in one transaction; same run ID+same bytes is idempotent, changed bytes refuses. No update-in-place terminal run record.
- Resolve only current project home, never shared Git-common-dir, home-wide/global DB, remote URL or arbitrary `--store` path. Generate random local repository ID on initialization, tenant-local scope IDs without hashing names. Each worktree/clone has a distinct home; all reads/writes/deletes require explicit current scope. Tenant token selects isolation, not authentication: private owner ACLs and separate OS users are required for mutually untrusted actors; same-user malicious processes are outside this boundary. No automatic cross-scope list or clone import.
- Path checks must reject traversal, symlinks, junctions/reparse points, hard-linked database files, special files and replaced parents; secure owner-only directory/file permissions on Unix and Windows. Extend an actual confined writer/open capability, not just canonicalize-then-open. SQLite journal creation must remain in that protected home; qualify race behavior on both platforms before claiming containment. Refuse unsupported local-filesystem/ACL guarantees; no network-share durability claim.
- Initialize generated local ignore protection before payload write; in Git projects verify history is ignored and no history content already tracked. Refuse tracked/unprotected storage, including caller attempts to use a tracked DB; no automatic modification of consumer tracked `.gitignore`. Runtime ignore file/store initialization is future implementation, not a modification in this research commit. No account, remote, telemetry, transport, provider, shell or adapter process is needed for storage.
- Proposed settings: `foreign_keys=ON`, `journal_mode=DELETE`, `synchronous=EXTRA`, `secure_delete=ON`, `temp_store=MEMORY`, bounded busy timeout (5 s), and bounded rows/bytes; read back effective PRAGMAs and refuse unexpected modes. Use `BEGIN IMMEDIATE` for mutation and one transaction for raw row/assertions/index/dependent changes. EXTRA accounts for rollback-journal directory synchronization; do not copy cache WAL settings blindly. Rollback-journal atomic commit relies on correct filesystem locking/sync, as documented by [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html) and [PRAGMA settings](https://www.sqlite.org/pragma.html).
- Recovery opens through the same scope/path checks, lets SQLite recover hot journals, validates store/schema/refs/digests and foreign keys, and rebuilds secondary indexes only from validated surviving records. Corruption returns stable `history.corrupt`; never silently reset the database, treat truncated input as success, recover from arbitrary backups, or resurrect invalidated claims. Concurrent readers get transaction snapshots; evidence consumers must revalidate live source refs/generation immediately before use.
- Retention proposal: 30 days, 10,000 records and 64 MiB canonical payload total **per tenant scope**, configured locally; enforce the strictest limit. Retention age uses store-authored `recordedAt`, not harness timestamp. Stable prune order is `(recordedAt,runId)`; invalid/out-of-range clocks refuse, clock rollback never accelerates deletion. The bytes bound measures logical payload size, not filesystem SQLite size; compact separately. Retention runs transactionally on append and explicit prune, with no daemon. A record exceeding the bound refuses; pinned claims do not exempt source records from deletion.
- `delete` and `prune` remove raw row, metrics, assertion set, index rows and dependent-source links, invalidate transitive index/claim/aggregate-input dependents and increment generation **in the same transaction**. Keep invalidation state only with opaque dependent ID/kind/generation; remove old source identity lists and cached claim values. A valid dependent must resolve every bound digest to a live same-scope row; detached cached output or deleted parent can never remain valid. Repeated deletion is an explicit absent result, with no cross-scope disclosure.
- #102/#100 consumers register local source dependencies before materializing claims and revalidate through `resolve_dependent`; they cannot accept a cached metric sum without live references. Recorder only returns sanitized local records to these local consumers; aggregate construction/evaluation is #102. Recorder cannot revoke already published artifacts or delete independently owned scenario/gate records. Those consumers must own their invalidation workflow; do not promise cross-issue publication deletion.
- Register repeat-parent references through the same dependency seam; reject dependency cycles. Deleting a parent invalidates its repeat-comparability claim without rewriting the surviving immutable child record: reads expose the historical link separately from its current invalidated resolution. No fresh exact-repeat claim may depend on a deleted parent.
- `secure_delete` reduces recoverable deleted SQLite cell content; `compact` runs `VACUUM` only after delete commits and reports its failure separately. It must not undo committed logical deletion. Confine any temporary files; no `VACUUM INTO` outside the home. [SQLite VACUUM](https://www.sqlite.org/lang_vacuum.html) explains rebuilding and space needs; neither setting proves erasure from SSD wear-leveling, filesystem snapshots or external backups. No raw transcript backups, automatic DB copies or silent reset fallback. Whole-scope clear uses the same transactional delete/invalidation path; store destruction is not a broad filesystem recursive-delete feature.

## 6. CLI and producer integration

All commands below are **proposed**, not available in this checkout. Add **new** `crates/lekalo-cli/src/history.rs` and route `Commands::History` in `main.rs`; keep operations in core, Git/build provenance capture at CLI edge, and `DomainResult` exits (0 valid, 1 invalid, 3 denied, 4 unsupported/unavailable, 5 unsupported-version).

```text
lekalo history init --project DIR
lekalo history scope create --project DIR              # emits random local scope token; no tenant name
lekalo history record --project DIR --scope TOKEN --input -
lekalo history list --project DIR --scope TOKEN --limit 50 --cursor TOKEN
lekalo history show RUN_ID --project DIR --scope TOKEN
lekalo history retention --project DIR --scope TOKEN --max-age-days 30 --max-records 10000 --max-bytes 67108864
lekalo history delete RUN_ID --project DIR --scope TOKEN --dry-run
lekalo history delete RUN_ID --project DIR --scope TOKEN --apply
lekalo history prune --project DIR --scope TOKEN --dry-run
lekalo history prune --project DIR --scope TOKEN --apply
lekalo history clear --project DIR --scope TOKEN --apply
lekalo history recover --project DIR --scope TOKEN
lekalo history compact --project DIR --scope TOKEN
```

- `record --input -` consumes a bounded typed harness observation from stdin, validates/sanitizes in memory and never writes the input bytes. **New** `contracts/run-observation.schema.v0.4.0.json` closes `{schema_version,identity,pilot,operation,timestamp,provenance,metrics,measurementSources,testGateSummaries,diagnostics,assertions,repeatParentRunId}`; children reuse field classifications in section 4, assertions reuse row schema. Scope and accepted privacy refs are store-selected, not harness-overridable; absent metric leaves become unknown. Input has no extension map or raw content fields. Record output is a bounded receipt `{runId,recordDigest,status,reasonCodes,generation}`, with the same scope sensitivity; never echo supplied values in a refusal.
- Default command execution must continue working with history disabled. Add opt-in `--record-run --history-scope TOKEN` to existing scan/verify/context/generate/NFR paths, passing a typed `RunRecorder` sink at the service boundary. Record after terminal operation outcome, using monotonic elapsed time; do not rerun the operation to obtain metrics. Uninstrumented filesystem/tool counts stay unknown. Harness record API supports #100/#118 without invoking a real model. Capture failures as infrastructure with incomplete metrics when the wrapper can observe them; abrupt termination is missing terminal evidence, not pass or zero.
- Recorder I/O failure never rewrites a failed/unsupported operation to pass and cannot silently disappear when recording was explicitly requested. CLI returns failure diagnostic while receipt retains original operation outcome. `history record` may return valid storage success for an operation whose recorded status is fail; that exit proves ingestion only.
- Resolve local Git commit and dirty inputs through argv, no shell/network/remote inspection. Repeated run gets a new ID; exact input fingerprint includes commit/dirty snapshot, model/IR/lock, all adapters, effective profile, core build and harness/model pins. Unknown pins produce incomplete, changed pins produce changed. A repeat link alone cannot reproduce a nonretained prompt or hidden provider state; report comparability, never promise deterministic LLM output.
- Status adapters use exhaustive mapping tests: scenario degraded becomes warn/incomplete; native passed/failed become pass/fail, blocked/security become fail with stable reason, missing becomes infrastructure, unsupported remains unsupported. Keep original source outcome in summary, no numeric-score-to-pass conversion. NFR advisory warnings remain warn; missing mandatory evidence cannot pass.
- List is bounded, sorted by `(recordedAt,runId)`, same-scope only. Cursor binds scope and generation and refuses after mutation. Show can display sanitized metadata, measurements and separately named assertions to the local operator. No public CSV/JSONL dump, `export`, upload, destination argument, raw-debug option or user-selected output file. Stdout is local operator presentation, not a public payload; default records are policy-denied for generic transfer/export too.
- Stable new diagnostic codes (register, do not invent at runtime): `history.scope-mismatch`, `history.input-invalid`, `history.policy-mismatch`, `history.unsafe-field`, `history.path-denied`, `history.tracked-store`, `history.busy`, `history.io`, `history.corrupt`, `history.version-unsupported`, `history.run-conflict`, `history.source-missing`, `history.dependent-invalidated`, `history.cursor-stale`, `history.retention-limit`. Details remain fixed tokens; no rejected value, absolute path, secret, timestamp or raw SQLite/Git error in diagnostic details.

## 7. Ordered executable implementation steps

1. **S1 — contracts and authority (AC2/3/4/6/7/8).** Recheck #121/#119/#120 and current product version. Define the four new schemas above; accepted authority/policy successor is a prerequisite, not a guessed runtime extension. Update the governed authority/policy family, manifests/digests, `core/privacy/{refs,context}.rs`, `scripts/check-privacy.mjs`, strict decision schemas and bound vectors together, preserving unchanged satellite versions. Correct `docs/privacy.md` to the actual custody family. Register schema identities in `core/run_history/version.rs` (new), support/version registry as required by existing `core/versioning/`; register history diagnostic codes. Run authority/privacy/version gates before storage integration. Never mutate an accepted contract under its old identity.
2. **S2 — bounded pure model and store (AC1/4/5/6).** Add `core/run_history/{mod,types,wire,canonical,diagnostic,version,path,store,retention,recovery}.rs` and export from `core/lib.rs`. Implement strict duplicate-aware validation, append/read/delete/invalidation transactions, scoped identity and ignore/ACL checks. Add `crates/lekalo-core/tests/{run_history,run_history_store}.rs` (new) with injection points for clock, random IDs and pre/post-commit failures; injected IDs are test-only, production IDs are random. Do not place history under cache, artifact clean or model loader ownership.
3. **S3 — producer normalization (AC2/3/4/6/7).** Add `core/run_history/{recorder,provenance,status}.rs` and CLI-edge provenance helpers in `crates/lekalo-cli/src/history.rs`. Instrument owned service entry/exit points in `main.rs` and `core/orchestration/{generate,verify}.rs` only as required; consume `Capsule`, scenario/gate/NFR summaries and resolved lock/profile/adapter pins. Separate assertions, sanitize allowlisted IDs and union project floor using #87/#119. Add repeat comparability, live source bindings and unknown defaults; omit capsule/transcript bodies. No harness account or provider pricing dependency.
4. **S4 — local CLI and dependency consumer seam (AC1/5/8).** Implement CLI above, bounded local rendering and opt-in recording, with `crates/lekalo-cli/tests/history.rs` (new). Expose only typed local `register_dependent`/`resolve_dependent` APIs for #100/#102/#118; enforce invalidation and live generation checks. Reject recorder kinds through existing generic privacy export tests. Document local operation/retention and the absent public exporter in `docs/run-history.md` (new future file).
5. **S5 — fixtures and release gates (all ACs).** Commit only synthetic fixture families with provenance, new scripts and CI hooks below. Execute offline CLI, crash/recovery, confinement, schema/type parity and cross-platform tests; validate real bundled SQLite on the Rust 1.80 toolchain. Acceptance is the completed gate evidence, not this research file or a passing storage happy path.

## 8. Fixtures, test gates and commands

**New** `tests/fixtures/run-history/` with entries added to `tests/fixtures/fixture-provenance.json`: synthetic origin, no private repository material. Reuse declared synthetic privacy leak corpus as inputs only; rejected bytes must not survive into committed valid goldens or runtime stores. Generate clocks/random IDs deterministically in tests. Valid JSON goldens use compact byte-sorted keys plus trailing LF and match both Rust canonical writer and Ajv. Store-schema fixtures describe logical state, not committed SQLite binaries.

| New fixture / gate family | Required vectors and observable evidence |
| --- | --- |
| `valid/{greenfield,brownfield,unknown-cost,known-zero,withheld,unsupported,repeat-exact}.json`, `assertions/` | Same run schema for pilots; all states distinct; metric/assertion shape separation; pass/fail/unsupported/infrastructure preserved; no assertion expected/actual payload. |
| `invalid/` | Missing/unknown class and stale refs/kind/version; negative/NaN/overflow counts, invalid cost decimal, total-token inconsistencies when all operands known, extra fields, duplicate keys, unknown enums, wrong scope, malformed refs, duplicate IDs, contradictory assertion summaries. |
| `provenance/` | Same inputs repeat, each pin changed separately, dirty Git, no Git/lock/profile/harness, changed bytes at same version, nonexistent/deleted parent, adapter bundle mismatch; incomplete never exact. |
| `privacy/` | Secrets/JWT, raw prompt/source/response keys, emails/URLs, Windows drive/UNC/home/URI/traversal/relative path fragments, tenant/repository names in free-form IDs; write refusal without echo. Verify serialized DB/journal/CLI bytes, not just in-memory metadata. |
| `storage/` | Two repositories with identical model/version, two scopes, cross-scope run/cursor/dependent access, tracked home, symlink/reparse/hardlink/raced parent, concurrent same/different IDs, denied ACL and read-only disk. |
| `recovery/` | Kill before insert, after raw row, after assertion/index/invalidation updates, before commit, after commit before response; disk-full/busy/truncated/corrupt DB. Outcome is old complete or new complete state, never half-written evidence. Retry uses original run ID. |
| `retention/` | Age/count/bytes thresholds, stable ties, future occurrence timestamp, clock rollback, oversized single row, delete absent, clear scope, transitive dependents, snapshot reader, failed compaction. Deletion removes live raw/metric/assertion/index rows and invalidates dependent claims atomically; recovery cannot restore them. |
| `offline/`, `no-export/` | Run commands with networking actively denied, empty HOME/account config and provider variables; trace no network/client/adapter process calls. CLI export flags/subcommand refuse; generic privacy transfer/publish denies recorder envelope; no CSV/raw dump or generated public payload. |

Add **new** `scripts/test-run-history-contracts.mjs` (strict Ajv 8.17.1, canonicalization and invalid vectors), `scripts/test-run-history-cli.mjs` (end-to-end receipts/bytes), `scripts/test-run-history-offline.mjs` (network/process denial evidence) and `scripts/test-run-history-recovery.mjs` (child-process kill points). Register them in `.github/workflows/ci.yml` contract and Linux/Windows jobs; the actual crash/confinement tests belong in Rust as well. Denied capability to enforce offline/race tests is unsupported evidence, not a skipped pass.

Future implementer commands, after adding the named files (Ajv supplied using CI's external pinned provisioning; set PowerShell `NODE_PATH` to that directory):

```powershell
node scripts/test-run-history-contracts.mjs
cargo test -p lekalo-core --test run_history --test run_history_store --locked
cargo test -p lekalo-cli --test history --locked
cargo build -p lekalo-cli --locked
node scripts/test-run-history-cli.mjs
node scripts/test-run-history-offline.mjs
node scripts/test-run-history-recovery.mjs
node scripts/check-privacy.mjs
node scripts/test-privacy-contracts.mjs
node scripts/test-privacy-coherence.mjs
node scripts/test-privacy-evaluator-parity.mjs
node scripts/test-privacy-runtime-cli.mjs
node scripts/test-privacy-leak-corpus.mjs
node scripts/test-authority-contracts.mjs
node scripts/test-authority-boundaries.mjs
node scripts/test-fixture-provenance.mjs
node scripts/check-contract-versions.mjs --base HEAD^
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

After focused gates pass, run remaining CI privacy/schema parity/authorization evidence gates affected by the authority successor; execute `cargo +1.80.0 check --workspace --all-targets --locked` and the CI matrix, not just the developer's stable compiler. Do not install/update dependencies in this research stage.

## 9. Open risks and recommended decisions

- **Authority and prose drift:** local-private recorder kinds/home are not accepted yet. S1 must resolve machine/prose custody mismatch and accepted successor with #2/#120; no raw record writing under HLV's kind or old refs. Historical run refs are immutable; readers must resolve the exact supported policy family or refuse export/use, never relabel to latest silently.
- **Isolation versus authentication:** random scope IDs avoid reversible hashes but do not protect against a malicious same-user process or copied store. Pin worktree-local homes and owner ACLs; define separate OS trust boundaries before multi-user tenant-security claims. No database transfer/rebind feature in M6.
- **Filesystem race and durability:** existing confined reads/atomic rename snippets are not a proven secure SQLite VFS. Qualify protected-home handle/race behavior, journals and permission inheritance on Windows/Linux; fail closed where that guarantee is unavailable. Logical retention does not imply forensic erasure or deletion of external backups.
- **Evidence authenticity:** hash bindings prove byte identity, not author authenticity or experiment correctness. `docs/privacy-runtime.md` mentions #121 as evidence-store authenticity custody; issue #121 does not define consent issuance/signing. Do not mint consent/declassification records or treat harness assertions as authorized grants; clarify that adjacent seam before claiming it implemented.
- **Instrumentation and reproducibility:** unknown files/tool/token metrics are honest until instrumented. Exact pins cannot replay omitted prompts/provider hidden state; repeat means comparability of declared inputs. Hidden inputs and source-version mapping must be documented by #100/#118.
- **Dependent scope:** same-store claims can be invalidated transactionally; externally cached/published claims require #100/#102/#118 to revalidate. Deleting a recorder row does not delete independent scenario evidence or revoke publication; consumers must not bypass the live-reference API.
- **Retention policy:** 30 days/10k/64 MiB are proposed configurable defaults, not #120 policy amendments. A stricter declared project retention rule dominates. Preserve only bounded minimal invalidation metadata; unknown policy or stricter retention incompatibility refuses rather than silently retains.
- **Cost and outcome mappings:** no pricing source of truth and no combined metric score. Preserve reported/estimated/unknown cost and source gate verdict; metrics never prove functional assertions or turn unsupported into pass.

## 10. Research delivery validation

Research checks run successfully: `git diff --check`, `node scripts/check-privacy.mjs` (accepted policy/authority 0.3.2, 57 registered kinds, satellites 0.2.16), and `node scripts/check-contract-versions.mjs` (product 0.4.0, 91 contract artifacts). These read-only checks confirm the inspected baseline, not recorder implementation acceptance.

Inspect the staged path list and commit exactly `docs(m6): research for issue #121 (issue #121)` on `ichinya/m6-issue-121`. Verify the commit contains only `docs/m6/issue-121-research.md` and the tree is clean. No recorder/schema/source implementation or runtime acceptance tests are claimed by this documentation-only research; the commands in section 8 are the future implementation gate plan.

# Neutral trace assessment

Issue #35 adds a pure Lekalo consumer of an explicit neutral mapping and normalized receipts. The AIFHub Extension owns HLV discovery, version negotiation, native-format interpretation, process execution, layout selection and durable QA custody. `lekalo trace assess TRACE --evidence EVIDENCE --json` reads the two supplied files and emits a receipt. It never executes HLV/tests, reads Git, collects evidence, syncs requirements or writes artifacts.

The existing `lekalo/trace-manifest/v0.2.16` remains unchanged. New contracts are `lekalo/trace-validation-evidence/v0.6.4` / `dev.lekalo.trace-validation-evidence@0.6.4` and `lekalo/trace-assessment/v0.6.4` / `dev.lekalo.trace-assessment@0.6.4`. Their closed schemas are in `contracts/`. Discovery publishes the exact tuples and the read-only `trace.assess` operation in workflow-provider 0.6.4. Model/IR, diagnostic-item, native receipts and target protocol retain their own independent versions.

## Mapping and coverage

The evidence input declares nonempty selected `scope.requirements`, `scope.artifacts` and `scope.scenarios`. An assessment is relative to this scope; a ready result does not assert whole-project coverage. Every selected identity must have a complete chain. A chain names its identity, stable occurrence, requirement, symbol, artifact, scenario, optional test/gate, exact trace `relationRefs` and independent `evidenceRefs`. Omitted test/gate is a visible mapping gap. Unknown or wrong-kind identities produce unresolved findings; no name/path heuristic supplies a missing link.

The required directed trace joins are:

| Relation | Endpoints |
| --- | --- |
| `implements` | symbol → requirement |
| `binds` | symbol → artifact |
| `covers` | scenario → symbol |
| `verifies` | native test → scenario |
| `evidences` | gate → native test |

Exactly one selected relation must establish each join. All must be confirmed under the existing trace provenance rules. The artifact binding is a separate branch: trace reachability alone does not establish it. Extra/unrelated references, repeated semantic mappings with different row IDs, ambiguous receipts and unreferenced receipts produce conflicts. Existing manifest gaps also prevent readiness. The service reports each finding; it never repairs or synchronizes a relation.

The adapter maps the scenario ID to a trace `native_test` node carrying the original HLV test ID in its `externalRefs` (`system: hlv`), and maps the gate likewise. When HLV is required or a chain supplies HLV evidence, test and gate must each have exactly one explicit HLV reference pinned to the current revision. Missing, ambiguous or stale references produce findings. Original HLV IDs are references, not duplicate canonical requirement bodies. OpenSpec and HLV can coexist on existing external references and as independent check providers. Lekalo imposes no HLV naming or native-file grammar.

## Evidence and provenance

Each receipt has an explicit provider, kind (`check` or `execution`), outcome, result digest, source revision, model reference and working-set digest. Completed pass/warn/fail receipts require a tool pin. Both tool and protocol pins have independent namespaced identity, exact version and SHA-256 digest. `providerPins` records the caller's expected negotiated tool/protocol tuples; each selected receipt must match them. No version range, inferred compatibility or automatic update is accepted. Schema and executable/adapter hashes are integrity evidence supplied by the host; they are not signatures or proof that a process ran.

The caller supplies the current project/Git/model/worktree/trace pins. Lekalo compares them with the accepted trace and receipts; it does not independently establish that they describe the live checkout. The Extension must qualify those pins before invocation and compare its input inventory again after the provider run. A changed commit, model digest, worktree digest, trace digest, artifact revision, tool hash or protocol pin cannot reuse a previously ready assessment.

An execution receipt additionally binds gate/test/artifact IDs and artifact content digest. `requireExecution: true` requires exactly one matching qualified receipt per chain. Passing execution also requires the artifact node's content digest and the gate node's evidence digest to match the receipt. A coverage declaration or aggregate HLV check is never converted into a native execution receipt. The report distinguishes passed, failed and unverified execution. `requireExecution: false` explicitly reports not-requested; supplied failed execution still produces a finding. Native evidence remains independently visible when an HLV check fails or is unavailable.

Outcomes remain distinct on the wire: `pass`, `warn`, `fail`, `unavailable`, `unsupported`, `infrastructure`, `configuration`. A completed failed check is `fail`; absence of an HLV executable/result is `unavailable`. The adapter retains original code, original severity and opaque subject for every diagnostic; no native messages, paths, raw streams or command text enter this contract. Codes such as `CTR-030` and `GATE-005` survive verbatim. Registered Lekalo summaries describe the bridge decision separately.

## Policy and reports

`requiredProviders` requires exactly one qualified passing check receipt for each named provider in each chain. Any supplied selected check failure/warning/unavailability prevents ready, including checks beyond that list. Optional policy returns exit 0/stdout with `verdict: degraded`; required policy returns exit 3/stdout with `verdict: blocked`. A clean scoped assessment is ready. Invalid input returns exit 1/stderr using the existing diagnostic envelope. Coverage (`complete`, `partial`, `conflicting`) and execution are separate facts from the aggregate verdict.

Valid receipts carry `assessment`. Denials retain the entire receipt under `payload.assessment`, mirror its registered diagnostics and preserve every normalized provider/native receipt. The report's `findings` contain stable rule, opaque subject and bounded detail tokens. `LEK-TRACE-001..009` are declared through diagnostic registry successor 0.6.4, not emitted as ad-hoc warnings. A report contains at most one summary diagnostic per finding class, preserving every detailed finding without overflowing the core diagnostic limit.

The Extension must aggregate this verdict with the existing Lekalo/native verify receipts and its policy before `/aif-verify` or `/aif-done` completion. Core `verify` and `readiness` do not invoke this service implicitly. Missing HLV is not a dependency of ordinary Lekalo commands; an explicitly supplied assessment can also omit HLV entirely. Durable phase-specific provider QA and any mapping persistence remain Extension responsibilities under their separately published contracts and authority policy.

## Layout and custody

The input and output contain no layout field or native artifact paths. Root greenfield and `.hlv/` adopt selection belong to the Extension. Ambiguous layouts, provider-induced native writes and unavailable/incompatible tools must be reported by that owner as explicit configuration/unsupported/unavailable outcomes. Core accepts qualified neutral evidence from either layout with identical behavior. The fixtures contain HLV-owned sentinels in both homes; the live CLI gate hashes the entire family before/after, proving the Lekalo command leaves them and its inputs unchanged. These are synthetic layout-independence tests, not HLV runtime qualification. No automatic creation/rewriting of HLV-owned artifacts is authorized by assessment.

## Canonicalization and limits

Hashes use SHA-256 prefixed `sha256:` over compact UTF-8 JSON with recursively sorted object keys and no final LF. Scope identity arrays, required providers, expected provider pins, mapping rows, receipt rows and reference lists are normalized lexically. Original diagnostic rows sort by code, subject, severity (error/warning/info); none are dropped. Trace digest uses the existing trace contract's separate canonical-byte recipe.

`mappingDigest` hashes the tuple `["lekalo/trace-validation-evidence/v0.6.4/mapping", normalizedScope, normalizedChains]`. It includes explicit receipt references and excludes policy and receipt outcomes. `evidenceDigest` hashes the complete normalized evidence input, including policy, current pins, expected provider pins, mapping digest and all receipts. The assessment exposes the actual trace digest and separately `expectedTraceDigest`; mismatches become stale findings. Reports contain no clock or host path.

Input and canonical report are each bounded to 8 MiB; mapping rows to 10,000; total selected scope IDs to 10,000; receipts and total original diagnostics to 2,000; diagnostics per receipt to 128; relation references per mapping to five; receipt references per mapping to 16. Exceeding a bound refuses the document; nothing is truncated. The CLI reads at most the limit plus one byte. Duplicate decoded JSON keys (including escape aliases), null optional fields, unknown fields, duplicate identities, contradictory pass/error diagnostics and an invalid mapping digest fail closed. JSON Schema covers shape and per-array limits; the semantic gate additionally enforces cross-array totals, identity uniqueness and digest/join invariants.

## Verification and fixture update

`scripts/test-trace-evidence-contracts.mjs` validates the closed input contract and hostile shapes with exact Ajv 8.17.1. `scripts/test-trace-assessment-contracts.mjs` validates output goldens, independently recomputes hashes, checks semantic expectations and executes the real Lekalo binary. `--require-binary` makes absence a failure in CI build-test after `cargo build`. Both run with `LEKALO_AJV_NODE_PATH`; no additional dependencies are introduced.

`node scripts/gen-trace-assessment-fixtures.mjs` is the explicit reviewed update recipe after building Lekalo. It creates only the declared synthetic fixture family and fails on unexpected statuses. The family is registered in fixture provenance; every new registered diagnostic has a golden witness in suite coverage. Original native/HLV outcomes are injected fixtures; the command does not run HLV or native test programs. External end-to-end HLV execution and verify/done aggregation need the AIFHub Extension adapter successor and its own qualified real-tool gate.

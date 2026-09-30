# Issue #121 — local-only run history and metrics recorder: implementation map

Delivered on `ichinya/m6-issue-121` (milestone M6). The recorder stores
reproducible source measurements fully offline, with repository/tenant
isolation, atomic writes and recovery, configurable retention and deletion,
missing metrics kept unknown, assertions stored separately from metrics,
exact accepted #120 policy references, and no source snippets, raw prompts,
secrets, or absolute paths by default. There is no export surface: public
payloads are built only by #102.

Commits, in order:

1. `feat(contracts): the closed run-history schema family and diagnostic codes (issue #121)`
2. `feat(core): the local-only run history store and metrics recorder (issue #121)`
3. `feat(cli): the history command family with typed dependents seam (issue #121)`
4. `test(contracts): run-history fixture vectors, Ajv and offline CLI gates (issue #121)`
5. `docs(m6): implementation map for issue #121 (issue #121)` (this document,
   plus `docs/run-history.md`)

## Delivered surface

- **Contracts (all at product 0.4.0, closed objects, bounded arrays):**
  `contracts/run-record.schema.v0.4.0.json`,
  `contracts/run-assertions.schema.v0.4.0.json`,
  `contracts/run-observation.schema.v0.4.0.json`,
  `contracts/run-history-store.schema.v0.4.0.json`. Fifteen `history.*`
  diagnostic rules registered in `contracts/diagnostic-registry.v0.4.0.json`
  (`LEK-HST-001..016`, sorted, unique codes, schema-validated).
- **Core** (`crates/lekalo-core/src/run_history/`): `version` (closed
  identities), `clock` (strict RFC 3339 UTC, fixed-millisecond record
  spelling, injection seam for tests), `limits`, `value` (the value-state
  wrapper reusing the #120 `SensitivityState` vocabulary; `known` is the
  only state that carries a value), `types` (closed wire models),
  `validate` (duplicate-key raw scan, typed decode with unknown-field
  denial, safe-field grammars, secret-material scan, registry-bound
  diagnostics, cross-field rules, exhaustive source-outcome mapping,
  fingerprint and repeat resolution, canonical record construction with the
  frozen #120 refs), `path` (the governed `.lekalo/history/` home:
  component-wise symlink/reparse/hard-link checks, generated `*` ignore
  protection, fail-closed untracked verification in Git projects via local
  read-only `git ls-files`), `store` (the SQLite backend: rollback-journal
  mode, `synchronous=EXTRA`, `secure_delete=ON`, `temp_store=MEMORY`,
  `foreign_keys=ON` — read back and verified; `BEGIN IMMEDIATE` with a 5 s
  bounded wait; one transaction binds record + assertions + index +
  dependent invalidation + generation; retention with clock-rollback
  guard; transitive dependent invalidation; digest-verified recovery with
  index rebuild; idempotent retry by content identity; `VACUUM` compaction).
- **CLI** (`lekalo history`): `init`, `scope create`, `record --input -`,
  `list`, `show`, `retention`, `delete --dry-run|--apply`, `prune
  --dry-run|--apply`, `clear --apply`, `recover`, `compact`,
  `dependents register|resolve`. Exits follow the closed envelope
  (valid 0, invalid 1, denied 3, unavailable 4, unsupported-version 5).
- **Gates:** `scripts/test-run-history-contracts.mjs` (pinned Ajv 8.17.1:
  strict schema compile, canonical golden form, adversarial vectors,
  custody-label checks), `scripts/test-run-history-cli.mjs` (end-to-end over
  the real binary in an emptied environment), fixture family
  `tests/fixtures/run-history/` (synthetic, declared in
  `tests/fixtures/fixture-provenance.json`), CI registration in
  `.github/workflows/ci.yml` (contracts job + build-test job).

## Acceptance criteria evidence

| Acceptance criterion | Evidence |
| --- | --- |
| **AC1 — history works fully offline** | No network, account, provider, shell, or adapter code exists on any history path; the only child process is the local read-only `git ls-files` custody probe at init in Git projects, and it fails closed when Git is unavailable (`run_history/path.rs::verify_untracked`). The CLI e2e gate runs the entire init→scope→record→list→show→delete→recover flow with an emptied environment (no HOME/account/provider/proxy/telemetry variables): `scripts/test-run-history-cli.mjs` step 2–4 (`offlineEnv`), `cargo test -p lekalo-cli --test history the_full_offline_history_flow_works_with_an_empty_environment`. |
| **AC2 — repeated run has exact revision/profile provenance** | Every provenance leaf is an exact value-state pin (Git commit/dirty/working-set digest, model revision/digest/IR digest, lock version/digest, adapters' manifest+bundle digests, profile digest, core build, harness pins). The recorder computes the input fingerprint over the full pin tuple and resolves `exact|changed|incomplete` comparability against the live parent: `validate::input_fingerprint`, `validate::resolve_repeat`, `store::parent_fingerprint`. Proof: `run_history::tests::a_repeat_link_needs_a_live_parent_and_exact_needs_equal_pins` (exact, changed, nonexistent parent, deleted parent), `the_input_fingerprint_changes_with_any_changed_pin` (each changed pin moves the fingerprint; unknown pins produce `incomplete`), fixture `tests/fixtures/run-history/valid/repeat-exact.json`. |
| **AC3 — missing token/cost fields stay unknown** | Value-state semantics: absent observation leaves normalize to `unknown` before persistence — never zero, null, or empty (`store::append` → `validate::build_record` → `normalize_metrics`). Known zero round-trips; `withheld`/`unsupported` stay distinct. Proof: `absent_metric_leaves_normalize_to_unknown_never_zero`, `a_known_zero_survives_as_a_known_zero`, fixtures `valid/unknown-cost.json`, `valid/known-zero.json`, `valid/withheld.json`, and the schema `vsU64`/`vsDecimal` oneOf grammar. |
| **AC4 — assertions and metrics stored separately** | Separate contract (`run-assertions.schema.v0.4.0.json`), separate SQLite table (`assertion_sets`), digest-bound reference (`assertionsRef.{setId,digest,count}`), rows carry only identity/subject/kind/outcome — no expected/actual values, prose, or excerpts (schema + `AssertionRow` type). `history show` names record and assertions apart. Proof: `a_full_valid_observation_ingests_with_the_complete_record_shape` (no assertion fields inside `metrics`; digest binds the exact stored bytes), fixture `valid/assertion-set.json`, adversarial `invalid/assertion-expected-actual.json` (schema refuses). |
| **AC5 — retention/delete updates indexes and dependent references** | One `BEGIN IMMEDIATE` transaction removes the raw row, assertion set, and index rows and transitively invalidates dependent `index|claim|aggregate-input` references, then bumps the generation. `resolve` revalidates every bound digest against live same-scope bytes. Proof: `deletion_removes_raw_metrics_assertions_and_indexes_atomically` (dry-run report, apply, repeated-deletion absence, recovery cannot resurrect), `dependent_invalidation_is_transitive_and_never_resurrects` (BFS over dependent→dependent edges; invalidated dependents keep only opaque id/kind/generation), `retention_age_and_count_bounds_prune_with_dependent_invalidation`, `a_clock_rollback_never_accelerates_deletion`, `an_oversized_record_refuses_and_never_persists`, CLI `deletion_invalidates_bound_dependents_and_repeated_deletion_is_absent`. |
| **AC6 — default record carries no snippets/prompts/secrets/absolute paths** | The closed typed model is the allowlist; identifiers accept only a bounded lowercase token grammar that cannot express paths, URLs, or free-form text; a second-layer scan refuses credential/encoded-blob spellings; every refusal carries a fixed detail token and never echoes the rejected value. Proof: `absolute_paths_urls_and_text_cannot_enter_token_fields`, `secret_material_and_encoded_blobs_refuse_even_in_token_grammar`, the schema grammars (`boundedToken`, `semanticId`, `sha256`, `timestampUtc`), adversarial fixtures `invalid/absolute-path-token.json`, `invalid/url-token.json`, `invalid/extra-field.json` (a `sourceSnippet` key refuses), and the no-echo CLI assertion `a_hostile_observation_refuses_without_echoing_the_value`. |
| **AC7 — greenfield and brownfield recorded with one versioned schema** | One `run-record.schema.v0.4.0` for both: `pilot.mode: greenfield|brownfield` with the measured `scopeState` (untouched legacy stays `observed`). Proof: fixtures `valid/greenfield.json` and `valid/brownfield.json` validate against the same schema (contract gate), plus `run_history::tests::the_production_store_matches_the_schema_contract_bytes`. |
| **AC8 — the recorder has no export; #102 builds public payloads** | No `export`/`upload`/`publish`/`aggregate` subcommand, no destination argument, no raw-debug surface, no caller-selected input path (stdin only); every record carries `exportDisposition: local-private` and `exportEligibility: ineligible` by construction, with the exact frozen #120 policy/authority references. The only consumer seam is the typed local `dependents register/resolve` API with live digest revalidation. Proof: contract-gate custody checks (`scripts/test-run-history-contracts.mjs` step "closed custody labels"), CLI `the_recorder_has_no_export_surface` (refusal at the argument layer; help text scan), `a_scope_token_from_another_store_discloses_nothing`. |

## Issue requirements mapping

- *offline/local operation without account/network* — AC1 evidence.
- *repository/tenant isolation* — opaque locally generated
  `repositoryId`/`tenantScopeId`, composite `(tenantScopeId, ...)` lookups
  only, unknown-token denial without disclosure, single project-local home
  (no shared/global/remote location, no caller-selected store path):
  `run_history::tests::scopes_are_isolated_and_unknown_tokens_disclose_nothing`.
- *atomic writes and recovery* — the one-transaction rule and
  digest-verified recovery with index rebuild:
  `recovery_verifies_digests_and_rejects_tampered_records` (a tampered raw
  row refuses as `history.corrupt`; the index is rebuilt only from
  validated records), `cursors_bind_scope_and_generation_and_refuse_after_mutation`,
  `concurrent_writers_serialize_and_both_commit`, `clear_empties_one_scope_through_the_transactional_path`.
- *configurable retention/delete* — `history retention` bounds (validated
  ranges), enforced per scope under the strictest bound on append and
  prune; AC5 evidence.
- *missing metrics = unknown, not zero* — AC3 evidence.
- *assertions stored separately* — AC4 evidence.
- *records reference the exact policy version from #120* — the frozen
  accepted `dev.lekalo.privacy-export-policy@0.3.2` and
  `dev.lekalo.authority-matrix@0.3.2` references are compile-time constants
  (`privacy::refs`) embedded in every record, re-verified at read and
  recovery (`check_frozen_refs`; a foreign policy refuses as
  `history.policy-mismatch`). The references match the accepted manifest
  digests verified by `scripts/check-privacy.mjs`.
- *no source snippets/raw prompts/secrets/absolute paths by default* — AC6
  evidence; there is no escape hatch (schema-level closure).
- *deleting a raw record invalidates dependent indexes/claims* — AC5
  evidence.
- *no public export API (that is #102)* — AC8 evidence.

## Deviations and explicitly partial scope

- **Authority-matrix admission of recorder kinds is not claimed.** The
  research (§2) proposed accepted successor registry entries
  (`history.run-record`, ...). Minting an accepted successor of the frozen
  #120 authority family is the reviewed `successorProcedure` of #2/#120 and
  is outside this issue's authority; the recorder therefore treats
  `artifactKind` as a closed local schema label, claims no accepted
  boundary kind, and references the exact accepted 0.3.2 policy/authority
  family instead (`check_frozen_refs` re-verifies at read/recovery; the
  15 registered `history.*` diagnostic rules are the only registry
  additions). Recorded as partial: kind admission lands with the #120
  successor procedure.
- **Producer instrumentation (`--record-run` on existing commands) is not
  in this delivery.** The recorder API, observation contract, and CLI
  ingestion are complete; wiring `scan/verify/context/generate` to emit
  observations is follow-up work for the harness/consumer issues (#100,
  #118) that own those flows. The acceptance criteria do not require it
  (provenance enters through the closed observation contract; AC2 is
  proven at the recorder level).
- **Crash-point kill tests run at the transaction boundary, not as child
  process kills.** Recovery evidence comes from the transactional
  rollback/commit semantics plus the tampered-record, cursor, and
  concurrency tests (`recovery_verifies_digests...`,
  `concurrent_writers_serialize_and_both_commit`); a separate
  child-process kill-point harness (`scripts/test-run-history-recovery.mjs`
  in the research §8) was not added — recorded as partial with the
  transactional evidence above standing in.
- **Semantic invalid vectors (total-token inconsistency, status
  contradictions, unknown-metric sources, coverage-denominator rule,
  duplicate semantic ids) are enforced by the typed validator and covered
  by Rust tests, not by the Ajv gate** (Ajv validates wire shape; the
  scenario gates use the same split). Each rule has a named Rust test
  cited in the AC table.
- **`history scope create` tokens select isolation, not authentication.**
  Mutually untrusted actors require separate OS users; a same-user
  malicious process is outside the documented boundary (docs/run-history.md
  "Isolation").

## Gate evidence (run at delivery time)

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --all-targets --locked -- -D warnings` — clean
  (stable and `+1.80.0` MSRV).
- `cargo +1.80.0 check --workspace --all-targets --locked` — clean.
- `cargo test --workspace --locked` — 1764 tests passed, 0 failed across
  all suites (lekalo-core includes the 41 `run_history` module tests;
  lekalo-cli includes the 7 `history` CLI tests).
- `node scripts/test-run-history-contracts.mjs` —
  `{"ok":true,"schemas":4,"valid":11,"invalid":16}`.
- `node scripts/test-run-history-cli.mjs` — `{"ok":true,"offline":true}`.
- `node scripts/check-contract-versions.mjs --base HEAD^` —
  `{"ok":true,"product":"0.4.0","contractArtifacts":95}`.
- `node scripts/test-fixture-provenance.mjs` —
  `{"ok":true,"families":62,"synthetic":62}`.
- `node scripts/check-authority.mjs`, `node scripts/check-privacy.mjs`,
  `node scripts/test-privacy-contracts.mjs`,
  `node scripts/test-diagnostic-contracts.mjs`,
  `node scripts/check-structure.mjs`, `node scripts/test-contract-versions.mjs`
  — all pass unchanged (the #120 frozen family is untouched).

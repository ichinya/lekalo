# Issue #114: the first contracted pilot on Laravel + Vue — implementation notes and evidence

The issue defines the transition: the first contracted greenfield
pilot moves to a Laravel backend + Vue frontend while the Node
implementation stays the observed baseline. This note records what
landed, the verification numbers, and the honest gaps. Public model,
fixtures, and evidence name only `planner` and the two targets — no
consumer-application name appears anywhere.

## What landed

- **The Node observed baseline is pinned and re-verified.**
  `tests/fixtures/pilot/observed-baseline/` carries the closed
  baseline manifest pinning the observed event envelope (ledger
  entries `{kind, id, operation}` over the `dev.lekalo.scenario-run@0.4.0`
  record contract), the `lekalo.target/v1@0.3.2` integration protocol
  with the committed protocol goldens, both planner OpenAPI documents
  (the contracted slice and the #46 join projection), and the semantic
  behavior rows of the corpus. `scripts/test-observed-baseline.mjs`
  refuses stale pins and re-derives the behavior rows from a fresh
  corpus run — a baseline move is conscious, never silent (proven by a
  tampered-baseline negative control during development).
- **Model neutrality is a standing gate.**
  `scripts/test-planner-model-neutrality.mjs` scans the 22 planner
  model surfaces (the routes-corpus home plus the contracted slice)
  against a closed 43-token forbidden vocabulary (frameworks,
  languages, runtimes, package managers, storage engines, analysis
  tools), checks every definition kind against the closed Model IR
  vocabulary, and keeps the committed audit
  (`tests/fixtures/php-laravel/routes/evidence/model-neutrality.audit.json`)
  byte-fresh. The one sanctioned exception is a `target-binding`'s
  `target` field; the contracted slice's binding description was
  neutralized to match (`docs/adr/0047-laravel-vue-pilot-path.md`
  records the decision).
- **The portable corpus proves the four required legs.** Two scenario
  documents join the four from #47/#56, canonical bytes, production
  Scenario-IR decoding:
  - `planner.scenario.focus_denied` — authorization: the declared deny
    policy refuses the bulk actor and still allows a solo actor;
  - `planner.scenario.focus_rollback` — transaction: a failed focus
    writes nothing (no row, no effect-ledger entry, the seeded row
    unchanged), proven through the typed error, entity-state
    presence/count expectations, and the `forbidden_effect` ledger;
  - with the existing idempotency (`focus_idempotent`), declared
    unsupported concurrency (`focus_concurrent`), happy, and error
    legs. The policy resolves through the scenario-vocabulary spelling
    `planner.policies/deny-bulk-focus` of the IR symbol
    `planner.deny_bulk_focus` (the closed Scenario-IR namespacedId
    grammar carries slash-form ids), mapped inside the maintained
    fixture ports of both backends.
- **The neutral Node ↔ Laravel equivalence is durable evidence.**
  `scripts/test-php-laravel-parity.mjs --emit-evidence <path>` writes
  the deterministic per-scenario comparison (semantic rows for both
  backends over digest-pinned corpus bytes, the shared lying-port
  mutation included, no host data), committed as
  `tests/fixtures/pilot/equivalence/node-laravel.scenario-comparison.json`:
  every scenario `equal: true`. The written removal gate lives in
  `docs/m5/issue-114-equivalence-report.md` §4.
- **The pilot path is one command.**
  `scripts/test-pilot-laravel-vue.mjs` walks model neutrality →
  observed baseline → operations (13 stages) → routes (9) → ui (6) →
  Mago (fake always, real when the pinned toolchain is provisioned) →
  native gates → Laratesto corpus (6 scenarios) → parity, reusing the
  committed harnesses; provisioning stays outside verification.
- **The `focus_task` context capsule is a self-contained bundle.**
  `docs/m5/issue-114/focus-task-capsule.md` embeds the machine capsule
  verbatim and the complete fact set: command contract, the two focus
  endpoints' wire truth (auth, idempotency, correlation, the full #62
  error table), the generated client methods, and the scenario
  evidence — readable without opening the consumer repository.
- **Roadmap and tutorial state the primary path.**
  `docs/m5/roadmap.md` (Laravel + Vue primary, Node observed baseline,
  the transition plan with per-leg state),
  `docs/m5/tutorial-laravel-vue-pilot.md` (the runnable walk), the
  README pilot section, and ADR-0047.

## Measured split

- **Generated/checked:** unchanged from #50/#53 — the typed PHP
  surface, the OpenAPI document, the client module and sidecars, the
  scenario tests on both backends, all re-derivable from committed
  joins and byte-checked by their gates.
- **Maintained in this slice:** two scenario documents, two port
  policy-mapping updates (one comment-bearing block per backend) plus
  their self-tests, three harness extensions (counts, expectations,
  the evidence emitter), two new gates, and the documentation bundle.
  No core IR changed; the #62 registry, the planner model, and the
  generated surface are untouched.

## Verification (local battery, this branch)

- `cargo test --locked -p lekalo-core` — **1423 passed, 0 failed**
  across all targets (the six corpus scenarios decode and canonicalize
  through the production Scenario IR in `--test scenario`, 10 tests).
- `cargo fmt --all -- --check` — clean; `cargo clippy --workspace
  --all-targets --locked -- -D warnings` — clean.
- Node contract gates — all green: the 42 pinned-Ajv schema gates
  (Ajv 8.17.1 provisioned outside the checkout), the authority/privacy/
  structure/model/versioning/lockfile checkers, the adapter-manifest
  contract + golden gates, the PHP types/round-trip/adapter/scenario-
  bindings suites, the kernel/scanner/zod/native-gates/transport/
  openapi/client-SDK/scenario-units suites, the client-SDK runtime and
  contracts gates, the privacy evaluator/runtime/leak-corpus gates,
  the classification CLI gate, the migration pipeline gate (the
  PostgreSQL execution leg records an explicit local skip —
  `postgresSkipped: true`), and the adapter conformance batteries
  through the real CLI (`failures="0"` bare and scanner-configured).
- `node scripts/test-node-scenario-tests.mjs` — 6 scenarios, all green
  (concurrency skips honestly; reruns byte-identical).
- `node scripts/test-php-laravel-scenario-tests.mjs` —
  `{"ok":true,"scenarios":6}` (Testo/Laratesto over the real kernel).
- `node scripts/test-php-laravel-parity.mjs` —
  `{"ok":true,"scenarios":6}` plus the emitted comparison artifact.
- `node scripts/test-observed-baseline.mjs` — 5 stages green.
- `node scripts/test-planner-model-neutrality.mjs` — 22 files, zero
  hits, audit fresh.
- `node scripts/test-pilot-laravel-vue.mjs` — 10/10 legs (including
  the real Mago leg) in ~100 s.
- `php -n -l` over the touched PHP files — no syntax errors;
  `git diff --check` — clean.

## Honest gaps

- The concurrent-focus leg is declared unsupported on both backends
  (no race evaluator in the closed assertion vocabulary); equivalence
  over it is vacuous until one exists.
- The neutral authorization leg proves decisions, not HTTP
  enforcement; enforcement evidence stays target-side (the Laravel
  planning battery), and the Node side has no HTTP battery.
- The portable corpus covers the focus family; the planning family's
  portable documents are the mechanical follow-up.
- The requirements trace keeps `completeness: partial` (unchanged from
  #50/#53; the gate gap waits for the release-verification lanes).
- The local real-Mago leg ran with the checksum-pinned 1.0.0 release
  downloaded outside the checkout; CI's `mago-real` job remains the
  standing acceptance leg.

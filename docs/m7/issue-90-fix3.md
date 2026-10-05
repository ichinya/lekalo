# Issue #90 — fix round 3 report (R3-1)

Scope: exactly one finding, `docs/m7/issue-90-review3-cline.md` §R3-1
(major, pre-existing). Devin's R3 verdict was ACCEPT (hosted CI
37033729116 fully green on `b7a950c2`) and carries no findings; both R2
blockers and all R2 minors stay closed. No gate was weakened; no
determinism invariant moved (see gate outputs below).

## R3-1 — `plan` could not plan any of the 20 `diagnostic.*.pair` cases

**Root cause.** `scripts/update-golden-case.mjs` emitted
`declaredPath: null` for every candidate of a case whose descriptor
declares no `expected` outputs (20 of 21 cases), while the closed
schema `tests/fixtures/suite/schema/golden-update.schema.v1.0.0.json`
admits `declaredPath` only as a string (omission is legal; `null` is
not). Both `plan` and `apply` validate, so every pair case died at plan
with `plan-schema / after/files/0/declaredPath must be string`. The
update-policy gate rehearsed only `minimal.project` — the single case
whose descriptor declares `expected` — so the no-`expected` branch was
never executed by any gate.

**Disposition: fixed (reviewer's option (a) — omission, not schema
widening).** A `null` path declares nothing, so the member is now
omitted entirely instead of being set to `null`. This keeps the closed
contract tight and leaves `apply`'s `unmapped-candidate` refusal as the
write bound for the class: a pair case can be planned and reviewed, and
`apply` still refuses to publish anything for it because the descriptor
declares no destination. The schema is unchanged.

Changes:

- `scripts/update-golden-case.mjs` — `declaredPath` is emitted only
  when a declared expected path exists, both where candidates are built
  (`cold1.map(...)`, spread-omitted) and where `after.files` rows are
  projected (`...(row.declaredPath ? { declaredPath } : {})`).
  Comment added explaining the no-`expected` branch and the remaining
  `unmapped-candidate` write bound.
- `scripts/test-golden-update-policy.mjs` — new section 3b rehearses
  `diagnostic.type-recursion.pair` end to end: `plan` must exit 0 with
  a non-empty semantic summary and candidate rows that carry **no**
  `declaredPath` member; a wrong accept digest must be refused; apply
  with the **correct** digest must still be refused with exactly
  `unmapped-candidate`. The gate header and the success payload
  (`pairClassFlow`) document the new coverage, so the class cannot
  silently regress to plan-only-`minimal.project` again.

## Controls run before/with the fix

1. **Reproduction (pre-fix producer):** plan of
   `diagnostic.type-recursion.pair` fails with exactly
   `{"ok":false,"gate":"golden-update-plan","errors":[{"reason":"plan-schema",
   "errors":[{"instancePath":"/after/files/0/declaredPath","keyword":"type",
   "params":{"type":"string"},"message":"must be string",...}]}]}` —
   matching the reviewer's evidence.
2. **Post-fix plan (pair case):** exits 0; `after.files` =
   `expected/trigger.envelope.json`, `expected/non-trigger.envelope.json`,
   both `change: "added"`, neither row carrying a `declaredPath`
   member; `before.files` empty (no declared outputs to preimage).
3. **Post-fix apply (pair case, correct digest):** refused with exactly
   `{"ok":false,"gate":"golden-update-apply","errors":[{"reason":"unmapped-candidate",
   "path":"expected/trigger.envelope.json"}]}` — the destination-binding
   guarantee is intact.
4. **Negative control on the gate:** with the pre-fix
   `update-golden-case.mjs` temporarily restored, the extended
   `test-golden-update-policy.mjs` fails with `pair-plan-phase-failed`
   — the gate genuinely catches the regression instead of exercising
   only the path that works (the R2/R3 blind-spot class). The fixed
   producer was restored immediately afterwards.

## Verification outputs (this checkout, Windows, Node v24.13.0, debug binary, Ajv 8.17.1)

All ten suite gates plus `run-golden.mjs --verify`, re-run after the
fix; every exit code 0:

```text
node scripts/test-fixture-provenance.mjs
  {"ok":true,"families":64,"synthetic":64,"evidenceBacked":0}

node scripts/test-golden-catalog.mjs            (LEKALO_AJV_NODE_PATH set)
  {"ok":true,...,"coverageStates":{"test-witness":302,"family-fixture":114,
   "interaction-only":13,"suite-pair":20},"ajv":"8.17.1"}

node scripts/test-golden-hygiene.mjs
  {"ok":true,"gate":"golden-hygiene","files":259,"controls":6,"hostRootsChecked":3}

node scripts/test-golden-adapter-shared.mjs
  {"ok":true,"gate":"golden-adapter-shared","sharedEvidence":4,
   "fixtureRsIncludes":9,"sharedIrDefinitions":19,
   "sharedIrDigest":"sha256:e1a2173af461cb43cfa58f091bd92a439f8cfa53aba7aea33481d85d6977decd"}

node scripts/test-golden-diagnostic-coverage.mjs
  {"ok":true,"gate":"golden-diagnostic-coverage","activeRules":449,
   "suitePairs":20,"familyFixture":114,"testWitness":302,"interactionOnly":13}

node scripts/run-golden.mjs --verify
  {"ok":true,"gate":"golden-run",...,"cases":21,...} (exit 0; byte-identical roles)

node scripts/update-golden-run-manifest.mjs
  {"ok":true,"gate":"golden-run-manifest","cases":21,"rows":41,
   "lanes":["cold-1","cold-2"]}   (regenerated byte-identical; `git status` clean apart from this round's two files)

node scripts/test-golden-normalization.mjs
  {"ok":true,...,"producerVectors":["newline-crlf-equal-output",
   "separator-forward-slash-equal-output"]}

node scripts/test-golden-determinism.mjs
  {"ok":true,"gate":"golden-determinism","lanes":["cold-1","cold-2","warm-cache"],
   "rows":41,"cases":21,
   "manifestDigest":"sha256:92496524bb7624a4e4e716d366d611eebb5fab2dcb85802b38b6aa0b32345c90"}
  (manifest digest identical to the value both R2 reviews recorded — the fix
   did not perturb any case outcome)

node scripts/test-golden-planner-e2e.mjs
  stages ok, incl. "trace-manifest-chain" stage ok
  {"traceIdentity":"dev.lekalo.trace-manifest@0.2.16"}

node scripts/test-golden-update-policy.mjs      (LEKALO_AJV_NODE_PATH set)
  {"ok":true,"gate":"golden-update-policy","planSchemaKinds":11,
   "flow":"plan -> review -> apply(digest-bound)",
   "pairClassFlow":"diagnostic.type-recursion.pair: plan -> review -> apply(unmapped-candidate)"}
```

`git status` after all runs: only this round's two modified files;
tracked suite bytes untouched by the gates (the update-policy gate's
own tracked-tree snapshot check enforces that on every run).

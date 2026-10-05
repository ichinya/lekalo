# Privacy-safe public metrics export

Status: **Implemented** local export pipeline for issue #102, product 0.6.4.
Framework Lift evaluation execution remains owned by #100. Synthetic gates
validate the export pipeline; they are not measured Framework Lift evidence.

`lekalo metrics export` consumes a named `evaluation-export-input@0.6.4`
selection and the live versioned records in the current repository's tenant
scope. It reads no source files, observed indexes, prompts, raw tool output,
repository URLs or provider pricing. The input names paired baseline and
Lekalo-assisted trial units, run ids and required hard assertion ids. It
cannot carry metric values. The recorder's metrics and separate assertion
sets remain the only measurement sources.

```text
lekalo metrics export --evaluation evaluation.json --scope TOKEN --dry-run
lekalo metrics export --evaluation evaluation.json --scope TOKEN --dry-run --authorization authorization.json
lekalo metrics export --evaluation evaluation.json --scope TOKEN --confirm PREVIEW_DIGEST --authorization authorization.json
lekalo metrics status EXPORT_ID --scope TOKEN [--authorization authorization.json]
```

Inputs are confined project-relative files. `--project DIR` selects the
project; output homes are fixed. `--destination` reuses privacy's exact
`workspace`, `repository-store`, `transfer-tenant`, `transfer-external`,
`transfer-cross-tenant`, and `publish` specs; the default is `publish`.
Every destination prepares a local package. No uploader is implemented.

The first preview may be blocked because aggregation/transfer consent is
absent. It still shows exact candidate bytes, manifest, redaction diff,
leak report and authorization subject. `decisionTemplate` has a **null**
aggregation-evidence position: it is explicitly non-authorizing and cannot
pass the #119 ExportDecisionInput validation. An evidence issuer supplies
the complete #119 decision input in `--authorization`; the existing
evaluator verifies exact refs, subject binding, purpose, outcome and
declared verification/freshness. Metrics code never mints consent. A
changed subject or stale/expired evidence cannot authorize confirmation.
For internal source records, aggregation approval alone stays blocked.
The issuer must explicitly request `dataSensitivity:["public"]` with an
approved `declassificationDecision` removing only `internal`, and bind fresh
declassification evidence to that target subject. The preview reflects this
requested plan with a null evidence slot; the original source labels remain
in its source projection. Other sensitivity classes cannot be lowered by
this adapter. The unchanged evaluator checks the complete actual evidence.
Cancel by ending after preview: `--dry-run` writes no package/dependent.

Five contract families have schemas and synthetic goldens:

- `evaluation-export-input`: typed #100 membership handoff, no measurements;
- `metrics-aggregation-definition`: fixed versioned recipe and field list;
- `public-metrics`: portable closed aggregate payload;
- `metrics-export-manifest`: public manifest or explicitly private custody view;
- `metrics-export-preview`: candidate bytes and non-authorizing template.

The embedded recipe is `complete-population-sums-rationals-microcost/1`.
An optional `--recipe FILE` must equal that exact definition. It requires
five scheduled trial units in each arm/cohort/statistic. Retries and
duplicate ingestion do not increase samples. Counts and sums use checked
integer arithmetic; means/lift use exact rational strings. Cost uses
micro-units without rounding, grouped by currency and reported/estimated
basis. Incompatible currency/basis or incomplete cost stays unknown.
Zero verified successes gives unknown cost per success, never zero.

`known`, `unknown`, `withheld`, and `unsupported` remain separate. Only
known values carry a value, including measured zero. Complete-population
statistics with missing inputs remain unknown/withheld/unsupported.
Stack, cached tokens, fix cycles, first-pass success, human intervention and
statistical confidence are unknown until their producer semantics exist.
Hard assertions determine verified success; fail, warn, unsupported and
infrastructure outcomes remain distinct. Incomplete provenance leaves A/B
comparison unknown. Negative lift and neutral measured outcomes survive.

Profiles/models/harnesses have export-local aliases. Only numeric release
versions are projected; private names, semantic ids, Git/Model/lock/source
digests and assertion ids stay in private custody. Outcome/coverage
distributions with small nonzero buckets cause all cohort numeric values to
be withheld to avoid complement subtraction. V1 allows one confirmed
release per scope lifetime, including invalidated or interrupted releases;
an interrupted activation consumes this budget and needs a future recovery
capability. Minimum sample is
an engineering disclosure rule, not a formal anonymity proof.

The local measurement projection is a Lekalo-owned `aggregate.decision`
with original recorder policy/authority provenance. It supplies the admitted
source of the derived `aggregate.artifact` privacy decision. Raw recorder
kinds are not relabeled as HLV evidence or added to the frozen authority
matrix. Existing policy/authority 0.3.2 and decision/evidence satellites
0.2.16 remain unchanged. Aggregation approval is mandatory even when
payload scanning finds no leaks. No sensitivity is silently removed.

Payloads use sorted compact UTF-8 JSON plus one LF. The manifest binds
payload bytes and recipe/policy refs with SHA-256; its own digest is
external. `packageDigest` binds payload and manifest digests.
`previewDigest` additionally binds the private source selection,
generation, destination and canonical authorization content. Private custody
lives at `.lekalo/privacy/decisions/aggregate/`; payload and manifest live
under `.lekalo/privacy/aggregates/`. A governed history writer lock spans
final source rehash and immutable activation. The manifest is written
last; consumers must validate both digests and current status before use.
Private custody also binds the tenant scope; another scope cannot read status.

The manifest includes exact byte pins for policy, accepted manifest,
classification, evidence, subject profile and privacy input/output schemas.
The leak report declares `typed-public-string-values/1`: fixed public
contract identifiers and typed numerals are recognized before scanning
other string leaves through the unchanged #119 engine. This prevents
contract slashes and large aggregate counts from being mistaken for paths
or phone numbers. No input text is admitted through that grammar. The
redaction diff is empty because this closed projection has no content
fields to redact; unexpected text fails instead of modifying metrics.

Deletion, clear, prune and retention invalidate the registered
`aggregate-input` through the recorder transaction. `metrics status`
checks the dependent and rehashes every source/assertion plus package
bytes. Missing/deleted sources produce an invalidated manifest status.
Without freshly supplied authorization, status is unverified even if
sources are live. A supplied validly shaped authorization that is now denied
(including declared expired evidence) produces invalidated eligibility.
Local deletion cannot recall downloaded copies; future
AIFHub import must keep this distinction between historical byte integrity
and current validity. The immutable file records historical `valid-at-export`;
the authoritative `metrics status` response changes to `invalidated`. It
does not overwrite the old manifest or promise remote recall. Evidence
freshness remains the existing declared `current/stale/expired` contract;
the CLI has no issuer authenticity or wall-clock expiry service. No AIFHub
service integration is claimed here.

Run `scripts/test-metrics-export-contracts.mjs --family FAMILY` after
`cargo build --locked -p lekalo-cli`, with exact Ajv 8.17.1 provisioned
through `NODE_PATH`. Each CI family gate compiles closed schemas, validates
goldens, and executes the real offline binary against synthetic records,
including preview, confirmation, uncertainty, denial and deletion cases.

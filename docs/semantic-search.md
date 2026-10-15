# Semantic search and find

Issue #93 adds one typed, offline search service with two entry points:
`find` applies exact filters; `search` combines those filters with
deterministic lexical retrieval, optionally bounded-labeled fuzzy. The
core owns every decision (projection, ranking, pagination, coverage,
diagnostics); the binary only admits inputs, persists the cache-owned
index store, renders, and maps exits. See
`docs/m8/issue-93-research.md` for the implementation contract.

## Contract

Four closed contracts at the product version of the implementing
commit: `dev.lekalo.search-query@0.8.0` (the normalized request
grammar), `dev.lekalo.search-result@0.8.0` (bounded selection cards,
ranking explanations, coverage, page state),
`dev.lekalo.search-index@0.8.0` (the canonical snapshot manifest), and
`dev.lekalo.search-inputs@0.8.0` (the typed producer manifest). The
diagnostic family registers as `search.*` LEK-SRCH-001..008 in the
union successor `dev.lekalo.diagnostic-registry@0.8.0`; predecessor
registries stay embedded exactly as accepted.

## Ranking recipe

The versioned recipe `semantic-search/1` freezes normalization
(Unicode NFC, ASCII case folding, term splitting on whitespace and the
ID separators `.` `_` `-` `/` `#`), the ranking tiers (exact full ID,
exact qualified ID, exact normalized name, all exact terms, prefixes,
optional bounded fuzzy at edit distance at most 2 over at most 10,000
candidates), and the tie-breaks (matched-field priority, term
coverage, edit operations, then the qualified key in UTF-8 byte
order). Exact IDs never lose to fuzzy results. Semantic ties inside
the top rank group set `selection: "ambiguous"` and retain every tied
candidate; sorting never becomes a selection.

## Evidence admission

Producers enter only through the `search-inputs` manifest and are
decoded with their own published contracts: canonical effect-graph
snapshots (detected rows keep exact subjects and recorded trust;
stale-trust rows are retained with an explicit per-fact stale status
and match only under `--include-stale`), neutral trace manifests
(requirement identities stay provider-qualified:
`req:openspec:PLANNER-REQ-001`; the relation graph is traversed over
raw nodes, so test/artifact intermediate nodes and cited evidence
refs bridge the chain; a relation's own confidence/status survive per
relationship and are never strengthened into `verified`), observed
binding indexes (canonical bytes only; the recorded binding set is
admitted with an explicit binding gap — the observed wire carries no
Model/IR pin, so bindings coverage is partial, never silently
confirmed), and AI-lint waiver sets (revoked rows are stale and
visible only under `--include-stale`). Missing optional producers
report `unknown` coverage — unknown is never zero, and absence of
evidence is never proof of absence. The error-contract and
ownership/profile families carry their own `errors` and `profiles`
coverage rows for the same reason.

## Producer families

Eight closed typed producer kinds are admitted (the manifest pins each
one's contract identity and optional bytes digest; every producer byte
is read once, confined and bounded):

- `trace` — the neutral trace manifest, joined through the nodes'
  typed fields with transitive requirement/scenario/gate refs.
- `observed-index` — recorded native bindings and candidate sets.
- `effects` — the detected-effect evidence envelope, decoded through
  the published closed `lekalo/effects/v0.2.16` grammar with canonical
  subjects validated against the typed IR.
- `waivers` — the governance waiver store.
- `requirements` — the requirements attachment
  (`lekalo/requirements/v0.2.16`), decoded by its own published typed
  constructor; references land as provider-qualified requirement cards
  plus typed refs and relation witnesses on the linked symbols.
- `error-contract` — an error-registry slice
  (`lekalo/error-registry/v0.2.16`, canonical bytes), decoded and
  project-validated by the #62 constructors; operation bindings land
  as typed error-role signatures so `--error` resolves.
- `diagnostics` — the closed search diagnostic-set snapshot
  (`lekalo/search-diagnostics/v0.8.0`, published as
  `contracts/search-diagnostics.schema.v0.8.0.json`): admitted findings
  project as diagnostic cards (rule identity, severity subkind,
  `evidences` relation onto the subject, and the reverse relation on
  the subject card so `--diagnostic <rule> --kind command` joins);
  resolved findings stay visibly stale. Custody: every subject must
  name a real card of the admitted projection, and one finding id
  carries exactly one (rule, subject) identity — ghost subjects and
  conflicting identities are typed refusals, never silently merged
  cards.
- `ownership-profile` — the closed artifact-ownership/profile snapshot
  (`lekalo/search-ownership-profile/v0.8.0`, published as
  `contracts/search-ownership-profile.schema.v0.8.0.json`): artifact
  ownership fills the typed `ownership` property (`module` |
  `project` | `observed`) of definitions bound under the artifact path
  — a property deliberately distinct from Model `visibility`, so
  neither reading ever overrides the other and `--ownership observed`
  resolves recorded observed ownership. Profile memberships land as
  typed `profile-member` relations — membership, never conformance.
  Custody: profile members must name real cards; empty member sets
  refuse.

## Commands

```text
lekalo search "QUERY" [--fuzzy] [--limit N] [--cursor TOKEN]
             [--project DIR] [--inputs FILE] [filters...]
lekalo find [filters...] [--limit N] [--cursor TOKEN]
           [--project DIR] [--inputs FILE]
lekalo search-index build|update [--project DIR] [--inputs FILE]
lekalo search-index status [--project DIR]
```

Filters (repeatable values of one flag OR, different flags AND):
`--id`, `--kind`, `--module`, `--reads`, `--writes`, `--calls`,
`--requirement`, `--requirement-source`, `--binding-path`,
`--native-symbol`, `--target`, `--type`, `--input-type`,
`--output-type`, `--error`, `--relation` + `--related-to`,
`--ownership`, `--portability`, `--profile`, `--diagnostic`,
`--waiver`, `--include-stale`, and `--effect-scope exact|overlapping`.
`--ownership` states are `module` | `project` | `observed` (the typed
artifact-ownership property, distinct from Model visibility).
`find` requires at least one filter; `search` requires exactly one
nonempty query. `--no-cache` bypasses every cache read/write; a build
or a `search-index status` under the bypass is a usage error.

Ordinary cache-enabled queries without `--inputs` reuse the persisted
admitted registration when it is still fresh (see Status below):
the saved producer generation — with its recorded coverage — is the
service, not a discarded byproduct. A canonical-only cold query never
clobbers a producer-built registration it did not admit; only an
explicit `build`/`update` (or drift) replaces it.

Exact lexical tiers never spend the fuzzy budget: every admitted card
is scored for exact/exact-qualified/name/prefix matches first, and
the bounded fuzzy expansion runs only over the still-unmatched cards.
An exhausted fuzzy budget is disclosed as truncation, withholds the
visible total, and never confirms a `unique` verdict.

## Index custody

Generations live under the governed cache custody
(`.lekalo/cache/search/**` of the accepted `lekalo.cache` authority
kind) as immutable bare-64-hex digest files with one atomically
published `current.json` pointer. A generation payload serves only
when its name, its recorded digest, and the canonical-bytes digest of
exactly the stored payload all agree, and its postings and qualified
identities resolve to carried cards; `cache clear --yes` clears them
and keeps `.lekalo/cache/migrations/**` exactly as before. Cursors
bind the raw query, its normalization, the fuzzy mode, every filter
member, the policy digest, and the immutable generation digest; a
cursor page serves the validated durable generation revalidated
against the CURRENTLY admitted content/policy snapshot (a stale
Model, a changed manifest, or a reconstructed foreign generation is
`search.cursor-invalid`, never a page of old or forged bytes), a
missing or drifted generation is `search.cursor-invalid`, and a cold
cache-enabled query publishes its generation so an issued page token
always has one to bind. Cursor tokens are closed documents (no
unknown members at any depth, no duplicate keys, bounded tuple
invariants). No raw source text, file bodies, prompt/tool output, or
hidden lexical tokens is ever persisted.

## Status

`search-index status` is the durable freshness oracle: the published
generation carries its own input pins (producer kind, path, bytes
digest, contract identity), its Model partition digest, and its
admitting-manifest configuration (path, canonical digest, posting
cap), and the verdict re-verifies exactly those facts under the
canonical project root and recomputes the snapshot digest a fresh
build would publish. The verdict never depends on the caller's
working directory or on where the manifest happened to live: a
drifted or missing pin file is `stale` (`search.index-stale`), a
model or policy change is `stale`, a revoked, edited, or removed
admitting manifest is `stale` (admission changes never hide behind
surviving producer bytes), an admission failure surfaces as its own
diagnostic, and `ready` means every pin, the admission config, and
the recomputed snapshot are byte-identical. The same partition pins
(`update`'s no-op reuse path and the warm query path) make `update`
a genuine partition reuse: when the Model digest, every producer
pin, and the admission config all re-verify, the immutable generation
is republished as-is — reuse avoids rewriting the durable store, not
re-decoding producers or recomputing the projection, and the
derivation proof stays decisive on every path.

Implemented: the pure core (`lekalo_core::search`), the CLI surface,
the durable generation store, the diagnostic family, the four wire
contracts, and the `scripts/test-search-contracts.mjs`,
`scripts/test-search-cli.mjs`, and `scripts/bench-search.mjs` gates.
The benchmark gate records the reproducible `rg` comparison with
pre-authored answer sets; its timings are local-run distributions,
never universal claims.

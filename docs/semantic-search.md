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
snapshots (detected rows keep exact subjects and recorded trust),
neutral trace manifests (requirement identities stay
provider-qualified: `req:openspec:PLANNER-REQ-001`), observed binding
indexes (canonical bytes only), and AI-lint waiver sets (revoked rows
are stale and visible only under `--include-stale`). Missing optional
producers report `unknown` coverage — unknown is never zero, and
absence of evidence is never proof of absence.

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
`find` requires at least one filter; `search` requires exactly one
nonempty query. `--no-cache` bypasses every cache read/write; a build
under the bypass is a usage error.

## Index custody

Generations live under the governed cache custody
(`.lekalo/cache/search/**` of the accepted `lekalo.cache` authority
kind) as immutable bare-64-hex digest files with one atomically
published `current.json` pointer. `cache clear --yes` clears them and
keeps `.lekalo/cache/migrations/**` exactly as before. Cursors bind
the normalized query, the policy digest, and the immutable generation
digest; a mismatch is `search.cursor-invalid`, never a page into
another answer. No raw source text, file bodies, prompt/tool output,
or hidden lexical tokens is ever persisted.

## Status

Implemented: the pure core (`lekalo_core::search`), the CLI surface,
the durable generation store, the diagnostic family, the four wire
contracts, and the `scripts/test-search-contracts.mjs`,
`scripts/test-search-cli.mjs`, and `scripts/bench-search.mjs` gates.
The benchmark gate records the reproducible `rg` comparison with
pre-authored answer sets; its timings are local-run distributions,
never universal claims.

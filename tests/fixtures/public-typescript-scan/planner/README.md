# Public native scan / reconfirmation regression

Synthetic planner from the issue #167 public pilot. `baseline/` is the
unchanged native starting point. `priority-src/` is the retained working
priority implementation from that pilot; it is a regression input, not a
new agent run or measured efficiency result. `oracle.mjs` is the independent
20-case native oracle retained with that implementation.

`scripts/test-public-typescript-roundtrip.mjs` copies these inputs into a
disposable directory and runs the actual checkout CLI plus the matching
manifested production adapter. It checks source preservation, declaration
coverage, explicit unknown type evidence, signature drift, stale review,
source drift refusal, batch reconfirmation and deterministic rescans.
Node 24 and pinned Ajv 8.20.0 are required. No inference occurs.

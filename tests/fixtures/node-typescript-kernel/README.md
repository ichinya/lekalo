# node-typescript-kernel fixtures (issue #43)

`public-fixture`. Every byte in this tree is exclusively synthetic
provenance: invented module names, invented identifiers, no real
consumer, company, URL, credential, absolute host path, or copied
source text. Nothing here is derived from a private consumer project,
and no private consumer identity or path may ever be added (privacy
policy #120 / enforcement #119 govern anything beyond fixtures).

The fixture project exists to be *inert*: the kernel never parses
`src/`, never reads `package.json` for discovery, and never executes
anything. `project/package.json` deliberately declares poison package
scripts that must never run; the test suite asserts source/config
bytes are unchanged around every kernel operation.

Inventory (closed):

| Path | Purpose |
| --- | --- |
| `project/src/example.ts` | Inert synthetic TypeScript text; never parsed. |
| `project/test/example.test.ts` | Inert native-test text; never executed. |
| `project/package.json` | Poison tripwire package scripts; never read or run. |
| `profiles/standalone.valid.json` | The explicit internal resolved project profile (`src/**`, `test/**` tree roots). |
| `profiles/invalid.json` | Named invalid profiles: traversal, absolute/UNC/drive, sibling-prefix, digest/capability mismatches. |
| `requests/describe.json` | One valid protocol 0.2.16 describe request with a deterministic request id. |
| `requests/invalid.json` | Raw request byte vectors: duplicate keys, invalid UTF-8, two documents, unknown keys. |
| `extensions/results.json` | Synthetic scanner/runner outcomes: complete, partial, unknown, ambiguous, malformed, error, local-only canary. |
| `expected/normalization.json` | Expected normalized internal evidence and allowed public projection/refusal outcomes. |

Fixture custody rules: materialize fresh disposable copies for process
tests; never write inside this tree from tests; assert byte preservation
before/after every outcome class.

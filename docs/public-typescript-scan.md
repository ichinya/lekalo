# Public TypeScript observed scan

Status: **Implemented** for the explicit synthetic standalone/workspace
subset below. Owner: adapter-host and Node evidence maintainers. [#150](https://github.com/ichinya/lekalo/issues/150).
The cross-platform qualification gate is
[`test-public-typescript-scan.mjs`](../scripts/test-public-typescript-scan.mjs),
run by CI on Windows, Linux and macOS. A missing or failing platform lane
does not qualify that platform.

The gate uses the checkout CLI and the committed, integrity-checked
[`adapter.mjs`](../adapters/node-typescript/adapter.mjs) with its matching
[`adapter.manifest.json`](../adapters/node-typescript/adapter.manifest.json).
It never substitutes an unmanifested adapter or invokes an internal scanner
to obtain an accepted result. The older Hono/Drizzle
[contributor pilot](tutorial-brownfield-typescript.md) retains its separate
Experimental scope; this subset does not qualify that pilot.

## Prerequisites and scope

Use a built Lekalo CLI and Node 18 or newer; CI exercises Node 24 on all
three operating systems. Keep the adapter artifact and its manifest in the
same directory. No project dependencies, package manager, backend, database
or network access are needed to scan this example. Scanning executes no
project scripts or native gates.

The authored sources and profiles are in
[`tests/fixtures/public-typescript-scan`](../tests/fixtures/public-typescript-scan).
For an explicit local command/effect model without HTTP or storage, see
[Domain and client workspace](domain-client-workspace.md).
Copy either `standalone/` or `workspace/` into a disposable user project:

| Layout | Declared scan roots |
|---|---|
| Standalone | `package.json`, `tsconfig.json`, `src/**` |
| Workspace | `package.json`, `tsconfig.json`, `packages/**` |

These are literal profile roots admitted by the shipped manifest, not
permission to read every repository path. Root package ownership and nested
package ownership use the longest matching package root. The profile id
`standalone` names the adapter launch binding; it does not prohibit a
workspace underneath that project root. Portable source paths and the
scanner's existing bounded TypeScript subset still apply. Symbol proposals
use the existing semantic-id grammar (lowercase dotted segments); names
that cannot be represented refuse rather than being truncated or guessed.
Negotiated protocol `1.1.0` carries top-level binding proposals and the
complete bounded native declaration forest, including class/interface
members, method-local declarations and lexical owners. These native rows
are evidence, not inferred business entities. Each root subtree is limited
to 256 declarations, with at most 10,000 declarations globally and owner
depth 64; references are limited to 256 per entry. Overflow refuses the
whole scan. Predecessor protocols retain their narrower projection and
refuse nested inventories they cannot represent. Endpoint projections
remain outside this recipe; it does not qualify framework scans.

Discovery scopes in [adoption](adopt.md) control discovery only. The separate
scan `profile.json` controls the adapter's reads and exclusions; inspect and
select both policies explicitly. A missing launch profile is unsupported.
Changing the profile id or adding undeclared read scopes refuses before a
scan can update the accepted index. Unknown imports, incomplete evidence,
over-budget references and unrepresentable output remain refusals.
Protocol `1.1.0` can preserve `any-surface` type uncertainty explicitly,
with the exact source digest. A complete declaration inventory does not
establish complete type evidence or native correctness. Unresolved imports,
other incomplete evidence and compiler errors still refuse.

## Adopt and bind

From the copied standalone project, initialize its canonical home and the
semantic module:

```text
lekalo init --adopt --project-id public_standalone --project .
lekalo module new public_core --project .
```

For the workspace, use project id `public_workspace` and also declare
`public_web`. Adopt may already declare modules; create only missing ones.
Copy the explicitly authored `model/entities.yaml` from the fixture family
to `lekalo/modules/public_core/entities.yaml`. This is a semantic decision
provided by the example author, not business semantics inferred by a scan.

Pass the compact contents of the selected `profile.json` as one adapter
argument. In PowerShell:

```powershell
$profileJson = Get-Content -LiteralPath profile.json -Raw | ConvertFrom-Json | ConvertTo-Json -Depth 20 -Compress
lekalo scan --target node-typescript --profile standalone --project . node C:/path/to/lekalo/adapters/node-typescript/adapter.mjs --lekalo-project-profile-json $profileJson
```

In a POSIX shell:

```sh
profile_json=$(node -e 'process.stdout.write(JSON.stringify(JSON.parse(require("node:fs").readFileSync("profile.json","utf8"))))')
lekalo scan --target node-typescript --profile standalone --project . node /path/to/lekalo/adapters/node-typescript/adapter.mjs --lekalo-project-profile-json "$profile_json"
```

Place CLI options before the adapter program: trailing arguments belong
to the adapter. No fallback command is needed. A successful scan records
two symbols and leaves source/config bytes unchanged. Review the proposals,
then confirm each selected proposal id from the actual output:

```text
lekalo bindings propose --project .
lekalo bindings confirm prop-<actual-id> --project .
lekalo bindings audit --project .
lekalo validate --strict --project .
lekalo observe inspect public_core.score --project .
lekalo inspect public_core.score --project .
lekalo impact public_core.score --project .
lekalo context public_core.score --budget 4000 --project .
```

Confirmation is explicit. Scanning does not promote code ownership or
invent entity definitions, HTTP bindings or persistence mappings.

## Evidence and compatibility

The historical adapter package `0.5.0` introduced the explicitly negotiated ordinary scan
protocol [`0.9.0`](../contracts/target-protocol.schema.v0.9.0.json).
It retains the `0.8.0` wire surface, raises only scan detail capacity
from 128 to 2048 Unicode code points, requires each accepted entry's exact
`source_digest`, and requires the normalized trusted launch
`scan_profile_digest`. Watch selection still requires an explicit adapter
capability declaration. Published predecessor schemas retain their bytes
and bounds; an old session cannot silently receive enlarged details.

The core compares source digests with actual confined file bytes before
merging. The scan receipt exposes `profile_digest`; the revision also binds
the profile digest, adapter identity and source-bound request id. Unchanged
cold/repeated scans preserve revision and canonical index bytes. A changed
profile, including exclusions/provenance, changes the revision. Structural
signatures that the scanner cannot derive stay unknown, never guessed.

CI proves the public positive path, schema/decoder bounds, typed profile and
scope refusals, partial scans, broken protocol output, deliberate missing
evidence and forged source digests. Every refused scan must preserve the
previously accepted index. Fault adapters are confined to negative tests
and never count as the public positive lane.

## Native inventory and change review (1.1.0)

Adapter package `1.1.0` negotiates the new
[`1.1.0` wire schema](../contracts/target-protocol.schema.v1.1.0.json).
Observed scan/index successors carry native declarations, per-candidate
binding evidence and explicit type uncertainty. Published predecessor
contracts retain their original bytes. An accepted native index cannot be
replaced through an older scan contract.

The vendored compiler selects its embedded standard libraries using the
configured target. Signature policy `2` includes class/interface structure
and member/value types. Upgrading the adapter can therefore require review
of signatures previously absent or produced by policy `1`.

The public planner regression is
[`test-public-typescript-roundtrip.mjs`](../scripts/test-public-typescript-roundtrip.mjs).
It runs the actual CLI and matching production artifact, preserves source
bytes during adoption, and checks 14 root bindings / 38 declarations,
including 24 nested declarations. Its priority change checks native tests,
an independent 20-case oracle, signature drift, stale proposals, explicit
batch reconfirmation and deterministic repeated scans. It also refuses
source edits between proposal and confirmation and preserves the accepted
index after an incomplete scan. Run it with Node 24 and the pinned Ajv
8.20.0 dependency provisioned as described by CI. No model inference occurs.

This gate establishes the supported public workflow. It does not measure
token savings, qualify all TypeScript or prove native correctness from
binding currency alone. Cold adoption and subsequent agent changes still
need separate empirical measurement under issue #167.

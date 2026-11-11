# Verified implementation ownership transitions

Status: **Implemented** scoped ownership-transition service for issue #111.
Owner: implementation-ownership maintainers.

Ownership is a permission to replace or maintain particular implementation
artifacts. Changing a policy word is insufficient to transfer those files.
`lekalo ownership` joins reviewed contracts, immutable candidate verification,
artifact custody and recoverable publication. It does not require converting
the rest of a repository or installing HLV/AIFHub.

## Scope and authority

The starting module must exist in canonical Model and in the explicit project
policy `lekalo/ownership.json`. The policy uses the existing managed ownership
format. The service does not infer a missing policy or turn a retro-model
proposal into canonical semantics. A reviewed contracted declaration can bind
an existing consumer implementation; existing inferred observed bindings must
first be confirmed through their existing owner.

Module defaults and explicit symbol handoffs are recorded in
`lekalo/ownership-state.json`. No `mode` field is added to Model definitions.
One symbol cannot claim a physical file shared with an unselected symbol;
that scope refuses instead of silently expanding. A single admitted target is
selected per transition. A module with existing symbol overrides must continue
through those explicit scopes; a module-wide handoff cannot absorb mixed custody.
Unknown behavior or incomplete capability evidence
refuses the transition.

The established artifact manifest remains
`.lekalo/generated/manifests/ownership.json`. Scoped handoffs update its
generated/scaffolded lifecycles and preserve neighboring entries. Old managed
generation records remain immutable history. Legacy module-wide mutation of a
transition-owned module, or restoration of a handed-off path, is refused;
use the scoped ownership service. Unrelated legacy modules retain their owner.

`change ownership:set` remains a metadata-only operation for legacy policies.
It does not generate or relinquish native files and cannot bypass an active
scoped handoff.

## Prepare, execute, publish

The CLI surface is:

```text
lekalo ownership plan --request REQUEST --project . --json
lekalo ownership verify --plan PLAN --project . --json
lekalo ownership apply --plan PLAN --confirm PLAN_DIGEST --project . --json
lekalo ownership rollback --receipt PLAN_DIGEST --dry-run --project . --json
lekalo ownership rollback --receipt PLAN_DIGEST --confirm PLAN_DIGEST --project . --json
lekalo ownership recovery --dry-run --project . --json
lekalo ownership recovery --confirm PLAN_DIGEST --project . --json
lekalo ownership status --project . --json
lekalo ownership trace --receipt PLAN_DIGEST --project . --json
```

Request and plan schemas are
`contracts/ownership-transition-request.schema.v1.0.0.json` and
`contracts/ownership-transition-plan.schema.v1.0.0.json`. All nullable fields are
explicit, unknown fields refuse, and inputs/outputs have bounded sizes. A plan
file may be the exact `{"status":"valid","ownership":...}` CLI envelope saved
from stdout, or its inner plan. Other envelopes are not plan authority.

Preparation writes no project files. It records complete bounded file previews
and a semantic ownership change for the selected subject. Exact Model, source,
lock, declaration, scenario, adapter and extension inputs are pinned. A change
to any consumed input invalidates the prepared plan. Runtime dependencies and
test/oracle/config files must be explicitly included; the verifier does not
copy an entire consumer checkout or silently install packages.

The reviewed runner is selected by bare qualified program name, executable
SHA256 and argv. It reads `ownership-bindings.json` in each disposable copy.
The same runner executes baseline and candidate independently and emits a
closed observation array: scenario ID, output, error, final state, effects,
authorization and transaction observations. Both runs must satisfy the
declared deterministic oracle; two equally wrong results do not pass.

Consumed sources are immutable to the runner. Only private `scratch/**` is
writable. Network access is denied, environment variables are scrubbed and
children remain confined. An unavailable runtime/backend, nonzero execution,
timeout or incomplete corpus refuses the operation. There is no ambient
execution fallback. No fields are implicitly normalized or discarded. Any
fixture normalization belongs to the explicitly reviewed runner and its pinned
policy/data inputs; this is not the general normalization engine of issue #99.

Saved verification JSON never authorizes publication. Apply re-derives the
plan, checks the interpreter and executes both sides again under the shared
project writer lock. Verification evidence retains outcome hashes and a
reproducible counterexample scenario ID, rather than raw application output.

`ownership status` reports declared ownership, not a fresh conformance PASS.
`ownership trace` exposes derived plan/input pins, the semantic transition,
measured verification hashes and rollback references for HLV/AIFHub consumers.
Historical completion and current equality of postfiles and consumed inputs
are separate fields; opening
the trace never executes a workflow or publishes evidence to another service.

Observed to contracted installs reviewed declarations, bindings and attached
scenario IDs without rewriting maintained bodies. Contracted to managed uses
the existing capability-qualified renderer, stages the candidate and publishes
its admitted manifest only after verification. The maintained originals are
not deleted before a verified replacement exists. Managed to contracted keeps
the exact implementation bytes as maintained scaffold and removes generated
cleanup authority by publishing the scaffolded lifecycle.

## Rollback and recovery

Before publication, the transaction retains exact preimages, including typed
absence, of its consumed closure. A pending barrier prevents other writers and
Model readers from observing a completed handoff while publication is partial.
The pending grant in canonical ownership state pins the exact journal and its
baseline. A cache-only journal cannot grant restoration writes. The final state
is installed only after its cohort; the pending grant is removed last during
restoration. Receipt publication is the commit point; an interrupted cleanup can complete
that committed transaction without applying it twice.

Rollback restores the previous policy, bindings, files and artifact manifest.
Preview does not mutate the project. Confirmed restore preflights every touched
path and the unchanged consumed dependencies. A later user edit, deletion or
corrupt snapshot refuses restoration without overwriting that work. Recovery
uses the same checks, recognizes already-restored files and can resume after
another interruption. Journals are re-derived against retained inputs before
they can supply restoration coordinates.

An exact rollback permits a fresh verified apply of the same prepared plan.
Each replaced receipt is retained by content hash; `previousReceipt` links the
audit history, and trace reports the current receipt digest. Replayed apply
checks current postconditions and does not execute or write the handoff again.

## Reproduce the bounded consumer example

Build the matching checkout CLI first. Set `LEKALO_BIN` to its absolute path
when using an external Cargo target directory; the Windows filename is
`lekalo.exe`. The materializer refuses an existing output directory.

```sh
cargo build -p lekalo-cli --locked
node scripts/materialize-ownership-example.mjs --output .tmp/ownership-example
```

The new directory contains two maintained consumer functions, their explicit
contracts, a deterministic scenario oracle and three reviewed request files:
`to-contracted.json`, `to-managed.json`, and `to-maintained.json`. Its canonical
Model and adapter inputs reuse the existing managed planner fixture. The
materializer computes the current source/runtime and Model/IR coverage pins;
verification itself never refreshes them.

From that directory, prepare and verify the first request, retain the returned
`planDigest`, then confirm exactly that plan using the commands above. Repeat
for the managed request and the maintained request. Retain the receipts for
rollback. The executable acceptance gate performs the same lifecycle plus
negative and partial-scope checks in private temporary projects:

```sh
node scripts/test-ownership-contracts.mjs
node scripts/test-ownership-transitions.mjs
```

The example's managed renderer supports bounded TypeScript projections. It is
not a generator for complete Node/Laravel HTTP/PostgreSQL applications or
arbitrary maintained logic. Reference projects #185 supply a separate larger
behavior corpus. Automatic retro-model proposals (#81), all comparison modes
of #99 and unsupported generators remain separate work; a successful scoped
transition does not close those issues or prove behavior outside its corpus.

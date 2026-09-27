# `lekalo-target-php-laravel` — PHP/Laravel target adapter (issues #54, #55)

The `lekalo.target/v1` protocol kernel implemented in dependency-free
PHP, proving the language-neutral core claim of the issue: a second,
non-Node adapter speaking the same wire through the same production
`TargetClient` and the same confined runtime. The MVP passes the
conformance battery in both profiles and the capability declarations
stay honest about what the MVP does not do.

## Version history

- **0.1.0 (issue #54):** the protocol kernel MVP — the closed wire
  surface, the deterministic generation seam, and the honest
  unsupported capability map.
- **0.2.0 (issue #55):** the Mago analyzer seam — receipt-consumed
  analysis evidence, the strict-profile gates on validate/verify,
  bounded scan evidence, the embedded toolchain lock, and the bundled
  `analyzer.php`/`strict-profile.php` modules. A backward-compatible
  minor bump: the closed v0.3.2 wire shapes are unchanged, and every
  new behavior degrades to the #54 no-claims posture when no analyzer
  receipt is staged.

## Component composition (issue #54 architecture)

```text
runtime: php-laravel
analysis: mago
 testing: laratesto
storage: postgres-sql
transport: http-json
```

The target-profile catalogue already registers `php-laravel`,
`mago`, and `laratesto` as components; this MVP creates the adapter
process and the protocol/planning seam. Deep Mago graph integration is
issue #55 and the full Laratesto backend is issue #56 — their names
appear in the portability catalogue, not in this adapter's proven
surface.

## Invocation

```sh
# production protocol process: exactly one request, exactly one response
php adapters/php-laravel/adapter.php
# request over stdin, or when core declares only the file transport:
php adapters/php-laravel/adapter.php --lekalo-request-file <PATH>

# local metadata probe (NOT part of the wire protocol; only as the sole
# argument):
php adapters/php-laravel/adapter.php --version-json
```

Core launches the adapter as a direct argv vector —
`AdapterCommand { program: php, args: [adapter.php, …] }` — never through
a shell or a package manager. The confined runtime copies the executable
plus exactly the first-argument script into its private view, which is
why the shipped artifact is one physical file: the kernel has no
`include`/`require`, no sibling autoload, no `vendor/`, and no Composer
dependency at runtime. PHP built-ins (`json`, `hash`, SPL) are always
compiled in; no extension is required.

## Strictness

- `declare(strict_types=1)` everywhere; no dynamic properties, no
  `__get`/`__set`, no magic domain behavior.
- The request decoder is fatal about malformed UTF-8, duplicate keys
  (including `{"a":1,"\u0061":2}` alias collisions — `json_decode`
  alone silently last-wins them), nesting depth, and trailing content.
- The closed request contract rejects unknown members, explicit nulls,
  wrong member pairings, and every grammar violation exactly like the
  core's `validate_request`.
- Responses are canonical compact JSON (sorted keys, no whitespace).
- A refused request is a bounded stderr diagnostic plus exit 1 — never
  a synthetic envelope; a polluted stdout would be a response refusal,
  so the kernel disables CLI `display_errors` before any code can emit.

## The generation seam (MVP)

The kernel owns one deterministic artifact (`.lekalo/generated/
php-laravel/<target>/kernel.php`): a dry run plans its exact bytes and
digest, an apply writes exactly those bytes inside the staged private
view, and `plan-clean`/`clean` remove exactly the planned paths. The
apply echoes the client's pending plan authority; the kernel never
recomputes a plan id from inputs it cannot see (the binding identity
mixes server-side context and observed-state hashes).

The named capability map honestly declares `unsupported` for every deep
generator surface (`generate.zod`, `generate.openapi`,
`generate.transport-http`, `verify.scenarios`, `scan.symbols`,
`preserve.classification`) and the `plan-native` exchange answers an
in-envelope `unsupported` error: a declared absence, never a fabricated
plan summary.

The `scan` operation performs a real read-only enumeration of the
staged view's declared read roots (`.lekalo/ir`, `.lekalo/cache`):
every returned path was observed on the filesystem under a declared
scope, sorted by path, with `truncated: true` at the wire bound rather
than a silently cut inventory. It never emits an entry it did not
observe. `bind`/`validate`/`verify` remain the MVP's fixed-shape
answers over the conformance fixture (bindings echo the request
pairing; validate/verify report an honest empty findings set) — the
semantic PHP validation, scenario verification, and Mago/Laratesto
evidence behind them are the #55/#56 slices. Semantic PHP type
mapping, DTO emission, migration planning, and the Laravel Planner
fixture are likewise the follow-up slices coordinated with #55/#56.

## Platform availability of the confined runtime

The confinement sandbox copies the interpreter plus exactly the
first-argument script into its private view. Standard PHP builds carry
runtime dependencies the copy cannot include (macOS seatbelt refuses
the dyld deps of the copied binary; Windows refuses the copied
`php.exe` without its sibling `php8.dll` — STATUS_DLL_NOT_FOUND). The
confined kernel suite therefore proves what it proves per platform:
where a confined describe exchange succeeds (the Linux CI leg, whose
packaged PHP build is self-contained under the ro-bound `/usr`), the
suite runs for real; where the exchange crashes before an envelope,
the Rust suite skips with an explicit machine-readable reason printed
to stderr — visible evidence of what was not proved, never a silent
pass. The PHP unit suites (`tests/protocol.php`, `tests/process.php`,
`tests/analyzer.php`) and the packaging gate run everywhere PHP runs,
independent of confinement.

## The analysis seam (issue #55)

The kernel integrates [Mago](https://github.com/carthage-software/mago)
as parser/analyzer/linter/guard **evidence**, never by launching it:
the kernel stays subprocess-free. A core-owned runner executes the
pinned Mago release in isolation and stages a receipt at
`.lekalo/import/mago/receipt.json` inside the declared read view; the
kernel decodes that receipt through a closed, bounded, duplicate-key
fatal decoder (`src/analyzer.php`).

Four analysis states are kept strictly apart (the issue requires Mago
unavailable to differ from analysis failure):

| State | Meaning | validate/verify answer |
| --- | --- | --- |
| `unavailable` | no receipt staged (tool never ran) | the #54 no-claims empty success |
| `incompatible` | receipt's tool version/digest or lock digest differs from the pin | in-envelope `infrastructure` error |
| `failed` | recorded run did not complete, or the receipt was refused | in-envelope `infrastructure` error |
| `ok` | current, well-formed, successful run | findings + strict-profile verdict |

The toolchain pin lives in the adapter-owned `mago-toolchain.lock.json`
(pinned release 1.0.0 with probed artifact digests, exit-code semantics,
and recorded command argv). Its bytes feed the deterministic build, so
a reviewed toolchain upgrade changes the artifact digest. Recorded
capability fixtures live in `tests/fixtures/mago/toolchain/`.

The strict profile maps Lekalo rules onto evidence (see
`src/strict-profile.php`): lint rows reuse pinned Mago codes
(`strict-types`, `no-variable-variable`) verbatim with the exact
original code preserved; predicate rows (final/readonly profiles) run
deterministically over receipt symbol evidence; rows whose prerequisite
evidence is absent are `unsupported` — never a silent pass. Safe fixes
travel as bounded advice records (range, replacement, before digest,
conservative safety class); **no operation can apply them** — the wire
has no fix operation, and the closed request validator refuses any such
member. Dry-run fix preview (`mago lint --fix --dry-run`) is available
only through the real integration gate in a disposable sandbox.

What #55 deliberately does not claim: `scan.symbols` stays
`unsupported` (the bounded scan wire cannot carry a full native graph;
the wire projection carries at most eight references per path and
refuses silently-truncated claims), `verify.scenarios` stays
`unsupported` (Mago success is not scenario evidence — #56), Laravel
relation/route/container links are received as provenance-carrying
relation rows in the receipt but the Model graph is never mutated by
them, and fix **apply** is out of scope for a later issue.

## Build and verification

`adapter.php` is GENERATED by `build.php` (fixed-order concatenation of
a generated banner plus the kernel source; no timestamps, paths, or
environment lookups, so repeated builds are byte-identical):

```sh
php adapters/php-laravel/build.php --check   # verify committed bytes
node scripts/test-php-laravel-adapter.mjs    # suites + packaging + conformance
php adapters/php-laravel/tests/protocol.php  # 88 decoder/contract checks
php adapters/php-laravel/tests/process.php   # 40 process-level checks
php adapters/php-laravel/tests/analyzer.php  # 44 analyzer/strict-profile checks
node scripts/test-mago-integration.mjs --fake  # required fake gate (no Mago)
node scripts/test-mago-integration.mjs --real  # real gate (LEKALO_MAGO or PATH; visible skip when absent)
cargo test -p lekalo-core --test php_laravel_kernel
```

The production conformance battery:

```sh
cargo run --locked -p lekalo-cli -- adapter test --profile strict \
  --report json --timeout-ms 30000 -- php adapters/php-laravel/adapter.php
```

## Ownership modes

| Mode | MVP behavior |
| --- | --- |
| observed | bounded reads inside the declared `.lekalo/**` scopes only; no source-tree writes |
| contracted | maintained Laravel code is never touched; the adapter generates only support artifacts under `.lekalo/generated/**` |
| limited managed | exactly the manifest-declared write scope (`.lekalo/generated/php-laravel/**`); nothing else can ever publish |

No hidden `composer install/update`, no lifecycle scripts, no network:
the manifest's `permissions` block denies children and network, and the
kernel launches nothing.

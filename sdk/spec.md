# The language-neutral adapter SDK contract (issue #106)

This document is the one decode/build/write contract every SDK language
implements. The four runtimes are ports of one specification, proven
equivalent by the shared vector corpus
(`tests/fixtures/adapter-sdk/`) and the cross-language gate
(`scripts/test-adapter-sdk-vectors.mjs`), which compares outcomes and
canonical bytes for all four codecs.

## Runtime API (equivalent concepts in all languages)

`decodeRequest`, `validateRequest`, `describe` capability building,
`buildSuccess`, `buildError`, `validateResponse`,
`validateWritePlan`/`validateLogicalPath`, `buildFinding`,
`buildDiagnostic`, `buildArtifactManifest`, `normalizeScenarioResult`,
`stderrLogger`, and `runConformance`. Generated types carry no ambient
I/O. SDK-local errors use closed safe reason tokens; the host owns the
public domain result and the status/exit mapping.

## 1. Transport and lifetime

1. Read exactly one bounded UTF-8 JSON request from stdin or the
   host-owned `--lekalo-request-file PATH` coordinate. This file
   coordinate is transport-owned, never a portable project path or an
   extra project-read grant. When both channels are present the file
   coordinate WINS: the host always spawns adapters with piped stdin —
   including on file-transport exchanges, where the same request bytes
   ride both channels — so pipe-ness is never ambiguous, and the file
   bytes are the authoritative request document. Empty stdin without a
   file coordinate is the only missing-input refusal. Refuse trailing
   documents, duplicate decoded keys (including escaped spellings),
   invalid UTF-8, unsupported exact versions, and invalid operation
   shapes before invoking any callback. The strict JSON admission is
   byte-for-byte the host parser's (issue #106 review r4):
   - only JSON whitespace (space, tab, CR, LF) may surround a document
     — any other control byte prefix or suffix refuses (no trim-broad
     stripping that admits NUL or vertical tab);
   - `\u` escapes are strict 4-hex-digit windows, and surrogate escapes
     must arrive as complete, correctly paired sequences: a lone high
     or low surrogate, a low half outside DC00–DFFF, or a non-hex
     window is `not-json`, never a fabricated code point or a repair;
   - a decoded literal U+FFFD is a legal character (the host admits
     it); only malformed UTF-8 byte sequences refuse;
   - the `-0` integer lexeme is not an unsigned integer spelling —
     production types every wire number `u64`, so `-0` refuses exactly
     like a float lexeme (shape at `limits`, bounds elsewhere) instead
     of collapsing to an accepted zero;
   - container nesting is bounded at 128 levels like the host parser:
     deeper documents refuse `not-json` — a bounded refusal, never a
     stack overflow or an uncaught recursion error;
   - optional arrays default only when ABSENT: a present object,
     string, number, or explicit null refuses instead of collapsing to
     the empty default (`{}` and `{"0":…}` never pass as arrays);
   - the optional native-reference adapter identities carry the host's
     adapter token grammar (`scopes::is_token`), while `revision`
     keeps its separate bounded-string contract; and identifier
     grammars anchor at absolute end-of-value — a terminal LF inside a
     path or token refuses in every runtime.
2. Echo the host `request_id`, operation, and negotiated version.
   `describe` reports the actual executable identity/digest and precise
   operations/scopes/capabilities/IR pins. Unknown and partial support
   stay explicit; scope/capability claims never widen host policy. No
   hidden handshake cache authorizes a later process: host session
   custody controls every call.
3. Build into an in-memory bounded response, then validate schema **and
   semantic invariants** before writing any stdout bytes. Success is
   operation-specific: false `ok`, findings, unknown/unsupported
   evidence, wrong identity/echo, extra members, invalid writes, or
   mismatched hashes cannot silently pass. For a known request, emit a
   valid closed error envelope when possible; for undecodable input or
   invalid builder output, emit bounded safe stderr and exit nonzero —
   never a fabricated success.
4. Emit a single compact canonical JSON response on stdout (sorted
   keys, no whitespace, no trailing LF). Logs go to a bounded, redacted
   stderr logger and never carry raw request, source, or environment
   content. Progress stays in the accepted response member; stdout
   banners and streaming records are forbidden. The serve loop RETURNS
   the process exit code and every executable entry point propagates
   it: any refusal class (undecodable input, oversize, invalid UTF-8,
   output-limit, builder refusal) exits nonzero while a successful
   exchange exits 0 — launchers that classify by `$?` never see a
   fabricated success (the lifecycle lane asserts this per language).
5. Cooperative deadline/cancellation tokens reach callbacks using the
   accepted request limits. Cap local work and output at the tighter of
   the host request and SDK bounds. Hard cancellation and descendant
   reaping remain host responsibilities: no `cancel` operation, no
   automatic retry, and no catching termination to emit success.

## 2. Defaults and authority

- Default adapter configuration is read-only: no write scopes, no
  network, no subprocess helper, no shell. Generation examples opt into
  one narrow output scope and plan before apply; IR read scope is
  explicit.
- Dry-run computes a sorted plan and exact output byte hashes without
  writing. Apply echoes the host's opaque `plan_id` and checks the
  supplied plan against the input and before-state; it cannot mint or
  persist host authority. The host retains whole-stage verification,
  consumption of apply authority, and actual publication/rollback.
- Path helpers mirror the host's closed grammar — portable lowercase
  segments, optional trailing `**` recursion on scopes, no absolute
  paths, backslashes, drive letters, percent escapes, DOS devices or
  short names, NFC-normalized segments — and refuse protected canonical
  homes for writes. They are defense in depth: only the host's private
  view and confinement enforce reality, and a runtime without Unicode
  normalization tables skips the `requiresNfc` vectors explicitly
  (never silently).

## 3. Byte-level determinism

- Canonical output is byte-sorted compact JSON with no trailing LF;
  array order is preserved; string escapes use the closed JSON set;
  numbers keep integer/float distinction within the safe range.
- Integer wire members are admitted by LEXEME, not by rounded value
  (production parity): production types every wire number `u64`, so a
  limits member in range must be spelled as an integer (`1`) — a float
  spelling (`1.0`, `1e2`) is a `shape` refusal, the negative-zero
  spelling (`-0`) refuses exactly like one (issue #106 review r4),
  both limits members are REQUIRED whenever `limits` is present, and
  malformed number spellings (`010000`, `1.`) are `not-json` everywhere.
  The corpus pins the exact raw byte spellings in every language, and
  the emitted-output ceiling is enforced on ENCODED UTF-8 BYTES of the
  complete envelope (never UTF-16 code units), with the effective
  ceiling min(declared, default). The multibyte and budget vectors pin
  this in every language.
- Request identifiers hash the canonical envelope minus `request_id`;
  plan identifiers hash the canonical ordered plan; write digests frame
  exact file bytes. The framing is contract-specific — a universal JSON
  sorter is never a substitute.
- UTF-8 byte ordering, preserved ordered arrays, and digest framing
  must match production byte-for-byte; the canonical vector family
  pins cross-language equality.

## 4. Semantic admission (beyond the schema)

The closed shape refusals (unknown members, explicit nulls, bounds)
run at decode; the cross-member rules run at validate:

- request-side: operation/plan pairing, dry-run pairing, IR-path
  requirement, target/profile requirements, describe member
  prohibition, profile-resolution pairing, plan-native and lint seams
  with version gating. The lint-request identifier members carry the
  production ai-lint token grammar (scope items refuse as `lint-request`,
  binding `nativeId`/`symbol` as `lint-binding`, the revision pin's
  known value as `lint-pin`) — charset and byte-length, never a bare
  length bound;
- response-side: identity echo (protocol/version/operation/request_id),
  status/error pairing, operation payload completeness, write-plan
  presence and digest/action pairing, plan echo on apply.

The deep `lint_evidence` subtree stays host-owned: the SDK carries it
as a closed object and never builds one.

## 5. Compatibility matrix

On a protocol upgrade the generator re-emits the exact-version matrix
and the accept/refuse cases together with the descriptor and records:
old accepted inputs retain their result, newly admitted inputs succeed
only under the new version, new fields/operations under the old version
fail, unknown/future versions fail before IR disclosure or callback
execution, and downgrade never drops evidence silently. Run through the
actual host decoder as well as SDK codecs — advertising a schema just
because generation succeeded proves nothing without this matrix.

## 6. Capability honesty

`describe` reports only what the adapter implements, resolved against
the exported capability-definition pin. `unknown` is a declared state,
never an optimistic yes; `partial` declares the subset; `unsupported`
never emits. Declared scopes are claims the host polices, not grants.

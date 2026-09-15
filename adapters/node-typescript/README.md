# `lekalo-target-node-typescript` — observed MVP adapter kernel (issue #43)

A dependency-free, read-only process kernel for the `lekalo.target/v1`
protocol. Node built-ins only (`node:crypto`, `node:fs`, `node:path`),
zero runtime dependencies, one physical file.

## Invocation

```sh
# production protocol process: exactly one request, exactly one response
node adapters/node-typescript/adapter.mjs
# request over stdin, or when core declares only the file transport:
node adapters/node-typescript/adapter.mjs --lekalo-request-file <PATH>

# local metadata probe (NOT part of the wire protocol):
node adapters/node-typescript/adapter.mjs --version-json
```

Core launches the adapter as a direct argv vector —
`AdapterCommand { program: node, args: [adapter.mjs] }` — never through a
shell or a package manager. The confined runtime copies the executable
and exactly the first-argument script into its private view; relative
sibling imports and ambient `node_modules` would break confinement,
which is why this adapter is deliberately a single file.

## Kernel-only operation table

| Operation | #43 posture | Owner of the real behavior |
| --- | --- | --- |
| `describe` | Implemented. No project/source/config reads or writes. | Kernel, permanently. |
| `scan` | Framing recognized; absent scanner ⇒ `unsupported`, and `scan` is not advertised. Test-only injection exercises dispatch. | #44 |
| `bind` | Unsupported/undeclared. Profile validation is not binding. | #44/#42 |
| `validate` | Unsupported/undeclared. Needs IR; not a profile RPC. | Later semantic/target validator owner. |
| `verify` | Unsupported/undeclared; test-only runner injection exercises framing without spawning commands. | #48 (+#47 scenarios). |
| `generate` | Unsupported/undeclared for dry-run and apply. No fake plan. | #45–#47 after #40. |
| `plan-clean` | Unsupported/undeclared. No inferred deletions. | Generation lifecycle owner. |
| `clean` | Unsupported/undeclared, even with a plausible plan id. | Generation lifecycle owner. |

A direct valid request to an unavailable operation returns one valid
`status: "error"` response with `error.class: "unsupported"`, fixed code
`operation-unsupported`, fixed message, `partial: false`, and no
`result`/`writes`/`progress`. Core normally refuses an undeclared
operation **before launch** (`target.capability-unsupported`, exit 4);
a child-reported unsupported error instead maps through
`target.operation-failed` to invalid (exit 1). The two CLI statuses are
intentionally not identical.

## Identity and runtime versions

- Adapter id: `lekalo-target-node-typescript`; version `0.3.0` (the
  reserved product version — not a protocol version); digest `sha256:`
  over the exact launched entry bytes, echoed on every response.
- Protocol: `lekalo.target/v1`, the sole supported version `0.2.16`
  (exact membership; no ranges, aliases, or fallbacks).
- **Node runtime versions are not in the handshake.** The v0.2.16
  describe response has no slot for them (and rejects `result`/
  `progress` there). They are reported by the local
  `--version-json` probe (exact `process.versions.node`, adapter
  id/version/entry digest) and carried in internal evidence. Moving
  them into `describe` would be a reviewed contract change (see
  “Boundary decisions”).

## Profile authority

The wire's resolved profile (v0.2.16) is a token plus digest/capability
pairs — it carries **no read roots**. Roots therefore come exclusively
from the injected internal `ResolvedProjectProfile`
(`createKernel({ resolvedProjectProfile })`), a separate in-process
authority whose own digest domain is distinct from the core
`profiles.digest`. There is no package.json/tsconfig/workspace
discovery, no environment-derived roots, no profile file path on the
wire, and no ambient `src/**` fallback. A real root-bearing transport
for #44/#48 is an explicitly owned integration decision; if the wire
ever needs it, that is a reviewed contract change, not this kernel
loosening.

Every root is validated **lexically** (no absolute/drive/UNC/device/
URI spellings, no `..`/`.` segments, no backslashes, percent escapes,
colons/control characters, trailing dots/spaces, DOS devices,
short names, overlong paths, uppercase or whitespace segments; `.`
is only the protocol project root, never an all-files root) and then
**physically** (must exist inside the permitted private root, no
symlinks/junctions/reparse points, canonical containment and spelling)
before **any** extension callback runs. One invalid root — first or
last — prevents every scanner/runner invocation and yields one honest
`invalid` error.

## Standalone synthetic limitations

The production kernel has no roots and no extensions: it describes,
and refuses everything else. The committed fixture under
`tests/fixtures/node-typescript-kernel/` is exclusively synthetic
(`public-fixture`): the profile, callback results, and evidence are
invented fixture vocabulary, and the fixture `package.json` carries
poison scripts that must never run. Standalone tests inject the
fixture profile and fake scanner/runner descriptors through the
internal `createKernel` seam — a fake scanner proves nothing about
TypeScript support; it proves the boundary.

## Extension interface (#44/#48 attach here)

```text
createKernel({ identity?, resolvedProjectProfile?, extensionRegistry?, localEvidenceSink? })
  describe()
  dispatch(validatedRequest, trustedExecutionContext)

ExtensionDescriptor = {
  id, version, operations, namedCapabilities?, acceptedIrVersions?,
  invoke({ operation, request, profile, readView, cancellation, limits })
}
InternalOperationOutcome = { state: complete|partial|unknown|unsupported|failed,
                             data?, evidence?, diagnostics? }
```

Extensions are registered explicitly — never discovered from a project
or profile. Descriptor compatibility is validated at registration; the
advertised capability map is computed from installed, validated
implementations only. All roots are validated before `invoke`; every
returned value is normalized at one boundary; a throw, rejection, or
malformed result becomes an honest `infrastructure` error, never a
success. The boundary is an extension seam, **not** an isolation
boundary against malicious imported JavaScript.

Evidence (revision, provenance, confidence, freshness, full source
spans, original local references, dynamic candidate sets) is preserved
completely in the internal outcome and the local-only sink. The public
wire carries only what the closed v0.2.16 contract can represent;
partial/unknown/ambiguous outcomes surface as honest in-envelope
errors (`partial: true` where partial work exists — there is no
invented `status: "partial"`), and unrepresentable values are refused
rather than truncated. A `truncated` scan result is never projected,
because the current merging caller ignores the flag and would record a
silently complete scan.

## Errors

| Cause | Class | Fixed code |
| --- | --- | --- |
| Unimplemented operation | `unsupported` | `operation-unsupported` |
| Profile/request invalidity | `invalid` | `profile-invalid` |
| Root lexical/physical failure | `invalid` | `root-invalid` |
| Extension threw / malformed output | `infrastructure` | `extension-failed` |
| Controlled conflict / partial outcome | `conflict` | outcome-specific bounded token |

Wire bounds mirror the contract: code 1–128 code points, message
1–256, detail ≤ 16 entries of ≤ 128. No raw JS stacks, host paths, or
untrusted strings leave the process; richer diagnostics stay in the
local-only evidence when authorized.

## Boundary decisions frozen for #43

1. **Internal vs external evidence.** Full extension evidence is
   preserved internally and locally; the closed process `Evidence`
   (adapter identity, optional plan id) is unchanged. Full external
   round-trip requires a reviewed transport/consumer extension and
   stays out of #43.
2. **Root authority.** Roots come only from the injected resolved
   project profile; the wire profile resolution stays digest +
   capability pairs.
3. **Runtime metadata location.** Node version lives in the local
   `--version-json` probe and internal evidence, not in `describe`.
4. **Conformance scope.** The kernel passes applicable default-profile
   rows; strict `capability.surface` fails **by design** — the eight-
   operation surface belongs to later issues. No fake operations are
   advertised to obtain a badge.

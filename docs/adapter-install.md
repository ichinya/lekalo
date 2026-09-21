# Adapter install, store, and trust (issue #32)

The install machinery turns a discovered adapter package into a
selected, verifiable store entry — and back — without ever executing
unverified bytes. The manifest is defined in
[adapter-manifest.md](adapter-manifest.md); this document is the
normative store/plan/trust behavior.

> Custody: everything below lives under the governed `.lekalo/adapters/**`
> runtime tree (authority kinds `lekalo.adapter-package`,
> `lekalo.adapter-inventory`, `lekalo.adapter-quarantine`,
> `lekalo.adapter-evidence`; privacy default `local-private`).

## Homes

```
.lekalo/adapters/
  packages/<id>/<version>-<digest8>/   # immutable installed packages
  staging/<digest8>/                   # the journaled stage tree
  quarantine/<id>-<version>-<digest8>/ # bounded opaque custody
  evidence/                            # releases, registry, revocations
  inventory.json                       # the store document
```

Package bytes are immutable once promoted. The `selected` pin in
`inventory.json` is the only mutable field; update and rollback are pin
repoints, never in-place edits.

## Trust levels

`builtin`, `verified`, `local-development`, `community`, `revoked` —
spelled exactly as the execution-isolation issue consumes them. Trust
never widens runtime permissions: it selects candidacy, confinement
strictness, and auto-selection eligibility only.

| Source + verification | Trust |
| --- | --- |
| Shipped inside the Lekalo distribution | `builtin` |
| Signature-verified per `required` policy + passing conformance + anchored publisher | `verified` |
| Explicit project path (unsigned or `optional`) | `local-development` |
| PATH-found, release-record, or registry package | `community` |
| Recorded revocation | `revoked` |

`builtin`/`verified` are auto-selectable; `community` requires the
explicit `adapter trust` promotion or the per-invocation opt-in;
`local-development` is selectable only for the owning project; `revoked`
is never selectable. Trust is a **filter before** the deterministic
ordering (`trust-revoked`, `trust-quarantined`,
`trust-insufficient`), never a sort key.

## Revocation

The revocation store is append-only local evidence at
`.lekalo/adapters/evidence/revocations.json` with rows
`{id, version|*, reason}`. It is consulted before selection and again at
lock verification; a revocation overrides every other signal, including
`builtin`. A manifest's own `status: revoked` is publisher self-report —
the enforcing signal is this store, so an adversarial manifest cannot
un-revoke itself. `lekalo adapter revoke` appends; `lekalo doctor`
reports affected pins as blocked (`adapters.trust`).

## Quarantine

Quarantine is a state, not a flag: community packages are staged under
`.lekalo/adapters/quarantine/**` under opaque bounded names and are
never executable and never selectable until an install plan promotes
them. `lekalo adapter quarantine purge` is the only removal path.

## Install plan and confirmation

`lekalo adapter install SOURCE --dry-run` renders the deterministic
plan: the resolved manifest identity, the assigned trust, every staged
file `{path, digest, bytes}`, the permission/capability diff against the
currently selected version, and the `planId` (SHA-256 over the canonical
plan document). Planning writes nothing.

`--confirm sha256:PLAN_ID` applies exactly that preview:

1. recompute the plan id — any drift answers `adapter.source-changed`;
2. a permission-widening diff refuses without `--allow-escalation`
   (`adapter.permission-escalated`);
3. take the exclusive guard `.lekalo/cache/locks/adapter-install`;
4. stage, re-verifying every file digest;
5. atomically rename into `packages/<id>/<version>-<digest8>/`;
6. repoint the `selected` pin in the inventory.

Any failure rolls the journal back in reverse order; an ambiguous state
answers `adapter.recovery-required`. Rollback is the same machinery
against an already-installed immutable version — bytes are never
modified.

## Offline

`path`, `path-exec`, and the installed inventory are fully offline.
`release` and `registry` sources resolve only from explicit local
records; `--offline` plus a remote-only coordinate is an honest
`adapter.source-unavailable`, never a silent fallback or fetch.

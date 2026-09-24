# Adapter confinement and permission budgets (issue #89)

Every external adapter executes inside an OS-enforced sandbox whose
authority is bounded by a **confinement budget** derived from the
verified package manifest's `permissions` block. The manifest is the
enforcement ceiling: an adapter that describes or requests authority
beyond it is refused, never silently accommodated. Synthesized
(implicit) descriptors claim nothing and receive the strictest budget.

This document is the normative reference for the budget semantics, the
per-platform enforcement matrix, the escalation policy, and the
confinement evidence. The package custody chain itself (manifests,
integrity, trust, install) is [adapter-install.md](adapter-install.md)
and [adapter-manifest.md](adapter-manifest.md).

## The confinement budget

`SessionBudget` (core `adapter_package::budget`) is built from the
manifest's declared `permissions` after the trust and consistency
gates, before any child process exists:

| Dimension | Manifest block | Budget semantics |
| --- | --- | --- |
| Read scopes | `permissions.filesystem.readScopes` | Ceiling: every described read scope must fit inside. |
| Write scopes | `permissions.filesystem.writeScopes` | Ceiling: every described write scope must fit inside; only fitting scopes are mounted writable. |
| Environment | `permissions.environment.allowlist` | Exactly the named variables are granted; values are read from the host environment at spawn time and never stored. A granted name colliding (case-insensitively) with the platform's fixed private environment block (Windows LPAC) is dropped and recorded in `budget.envDropped`. |
| Secrets | `permissions.secrets.handles` | Each handle id resolves to the controlled name `LEKALO_SECRET_<HANDLE>`; the host value under that name is injected at spawn time only. |
| Network | `permissions.network.mode` | `denied` is enforced on every supported platform. `allowlist` **degrades to denial** (see the matrix) — it is recorded honestly, never widened. |
| Children | `permissions.processes.children` | `denied` is enforced or bounded where the platform has a primitive, and recorded as a gap where it does not. `declared` keeps the existing confinement (children stay inside the sandbox). |
| Expansion policy | invocation flag | `Refuse` by default; `AllowEscalated` only under an explicit operator policy. |

Fail-closed rule: an unenforceable declared permission is a refusal or
an honest `unenforced`/`degraded` evidence verdict — never a silent
allowance. Scope fitting mirrors the plan-coverage grammar: a
recursive cap (`a/**`) covers the contents below its base, never the
bare base itself — a described exact scope `a` under a cap `a/**`
refuses as escalation. Secret values and granted environment values
never enter canonical bytes, diagnostics, evidence, or logs; only
variable names and handle ids do.

### Implicit (synthesized) adapters

An adapter supplied without a package manifest is synthesized from its
entry bytes. It makes no independent claim, so its budget is the
strict default: no environment, no secrets, network denied, children
denied, and filesystem bounds equal to whatever the adapter itself
describes (the describe handshake always runs with empty scopes, so
the described bounds are verified against the manifest-vs-describe
consistency rules of issue #32 when a manifest exists).

## Scope escalation

When the adapter's described scopes exceed the manifest ceiling, the
run refuses with `adapter.permission-escalated` (denied, exit 3),
naming the exceeded member (`capabilities.readScopes` or
`capabilities.writeScopes`) in the diagnostic data.

An operator may permit the widening for one run with:

```sh
lekalo generate --allow-permission-expansion [--target T] -- PROGRAM [ARGS]
```

The flag applies to the generate pipeline only. Even when permitted,
the widening stays visible: the budget caps in the confinement
evidence remain the manifest ceiling, and `described` exceeds them.

## Enforcement matrix

| Dimension | Windows | Linux (bwrap) | macOS (sandbox-exec) |
| --- | --- | --- | --- |
| Filesystem scopes | private staged view; LPAC read-only grants outside write roots | private staged view; ro-binds + `--remount-ro /`; write roots bound | private staged view; write only into declared roots |
| Network | denied (LPAC, no network capability) | denied (`--unshare-all`) | denied (deny-default profile) |
| Network allowlist | `degraded-denied` — no namespace-level destination filter exists | `degraded-denied` | `degraded-denied` |
| Children denied | `denied-enforced`: job `ACTIVE_PROCESS` limit = 1 | `denied-bounded` (kernel ≥ 5.14, per-userns task bound): `prlimit --nproc=64` wrapper when the wrapper exists and the release bounds `RLIMIT_NPROC` per user namespace, else `denied-unenforced` | `denied-unenforced` (no primitive) |
| Children declared | `permitted` (children stay in the job) | `permitted` (children stay in the namespace) | `permitted` (children stay in the profile) |
| Memory bound | `enforced`: job process-memory limit (2 GiB constant) | `unenforced` (`RLIMIT_RSS` is a historical no-op) | `unenforced` (no primitive) |

Every run also keeps the unconditional invariants: private scoped
project view (write-only paths reveal shape, never source bytes),
direct argv with no shell, bounded request/stdin/stdout/stderr, kill
on deadline/cancel/exit with whole-tree custody, request files at mode
0600, `--preserve-symlinks` for node, and publication with per-file
atomic persist plus rollback.

The Windows memory bound is a fixed constant
(`SANDBOX_MEMORY_LIMIT_BYTES`, 2 GiB) applied to the job object of
every confined run — describe and scan included, not just generate. It
is not budget-derived or configurable, so a memory-heavy adapter on a
large project can fail against that documented ceiling even on a
read-only operation; the evidence reports it as
`resources.memoryLimit` with `resources.enforcement: enforced`.

## Write-plan vs actual audit

For every operation that declares writes, core compares the declared
plan against the actual staged changes and publishes the audit:

- `declared` — the number of entries in the verified plan;
- `changed` — the logical paths actually changed in the staged view;
- `outsideScopes` — changed paths outside the effective write scopes.

The passing case is an empty `outsideScopes`. A staged change outside
the effective write scopes is a confinement violation: the run refuses
with `adapter.security-failure` (`check: writes-outside-scopes`,
`detail: publication-guard`) before any publication, and the
publication guard independently refuses any write entry outside the
sandbox's write scopes.

Read-only operations (`describe`, `dry-run` planning, `verify`,
`plan-native`) never receive write roots and never publish; a dry-run
that mutates anything is refused as `target.dry-run-mutation`.

## Confinement evidence

Every completed exchange carries a deterministic `confinement` member.
It is persisted on the observed scan receipt only: the
generate/verify/clean receipts do not embed it (deferring that member
keeps the receipt contract additive), so the durable per-run audit
record exists for scans. The receipt embeds it verbatim:

```json
{
  "budget": {
    "readScopes": ["src/**"],
    "writeScopes": ["gen/**"],
    "scopeCeiling": "manifest",
    "env": ["LEKALO_GRANTED_VAR", "LEKALO_SECRET_PROBE"],
    "envDropped": [],
    "network": { "mode": "denied", "enforcement": "enforced" },
    "children": { "policy": "denied", "enforcement": "denied-enforced", "processLimit": 1 },
    "resources": { "memoryLimit": 2147483648, "enforcement": "enforced" }
  },
  "described": { "readScopes": ["src/**"], "writeScopes": ["gen/**"] },
  "effective": { "readScopes": ["src/**"], "writeScopes": ["gen/**"] },
  "writes": { "declared": 1, "changed": ["gen/x.ts"], "outsideScopes": [] },
  "platform": "windows-x86_64"
}
```

Field order is the wire order; arrays are sorted; `writes` is present
only for operations that declare writes. The document carries names
and tokens only — never environment values, secret material, or
absolute host paths — and there are no timestamps.

`budget.scopeCeiling` names the ceiling source: `manifest` when the
cap lists are the verified package manifest's ceilings, `described`
when the budget claims nothing independently (the strict implicit
default) and the adapter's own describe bounds apply — so empty cap
lists are read correctly instead of looking like an empty grant.
`budget.envDropped` lists granted environment names that were dropped
at spawn because they collide (case-insensitively) with the platform's
fixed private environment block (Windows LPAC); the private value
wins, never the host-sourced grant.

The enforcement vocabulary is closed:

- `network.enforcement`: `enforced` | `degraded-denied`
- `budget.scopeCeiling`: `manifest` | `described`
- `children.enforcement`: `denied-enforced` | `denied-bounded` |
  `denied-unenforced` | `permitted`
- `resources.enforcement`: `enforced` | `unenforced`

No new diagnostic ids were required: budget escalations reuse the
registered `adapter.permission-escalated` rule and confinement
violations reuse `adapter.security-failure`.

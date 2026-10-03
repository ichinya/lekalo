# Project layout

Status: **Implemented**, product 0.6.3. Owner: filesystem/reproducibility maintainers. Normative details: [canonical structure](canonical-structure.md), [ADR-0003](adr/0003-canonical-structure-and-path-safety.md), [issue #4](https://github.com/ichinya/lekalo/issues/4).

```text illustrative
project/
  lekalo/project.yaml          # canonical project marker
  lekalo/modules/<module>/    # canonical semantic definitions
  lekalo/targets/<target>.yaml # explicit target configuration
  lekalo.lock                 # committed resolution, when explicitly locked
  openspec/                   # requirements owned by OpenSpec
  .lekalo/                    # derived/runtime state; do not commit
  .ai-factory/                # workflow-consumer plans/state/QA custody
  .hlv/                       # validation-consumer result custody
  src/ and native tests/      # maintained or explicitly owned generated code
```

This diagram is a role map, not a permissive declaration that every path under these roots is admitted. [Authority](authority.md) and the exact [structure checker](../scripts/check-structure.mjs) determine permitted homes. Module IDs come from declarations, not directory names; source paths do not create semantic identity.

## Minimal project

**Implemented.** Build first as described in [README](../README.md). Work in an already-existing empty directory; the example gate creates its own scratch root:

```sh docs-example=minimal-init
lekalo --json init --project-id tutorial --module planner
lekalo --no-cache --json load --ir
lekalo --no-cache --json validate --strict
```

Expected: each exits 0/stdout. Init creates `lekalo/project.yaml`, the empty planner `module.yaml` and a managed `.gitignore` line; it installs no adapter/toolchain and generates no application. Load returns typed IR; strict validation returns valid. Repeating identical init is idempotent. A conflicting existing file refuses without overwriting it. `init --dry-run` previews without writes. The [bootstrap reference](bootstrap.md) owns the complete grammar.

Canonical definitions and a resolved lock belong in version control. Caches/import drafts/local history under `.lekalo/` are derived or runtime artifacts with their own retention rules. Promotion is explicit; copying a draft into a canonical home is not automatic acceptance.

Root selectors are invocation-relative and physically checked. Link/junction/traversal/case aliases and protected write homes are refused according to the current structure/protocol rules. Use [loader](loader.md), [lockfile](lockfile.md), [cache](cache.md) and [artifact manifest](artifact-manifest.md) for exact paths, limits, digests and ownership. Never add a new canonical home from a tutorial.

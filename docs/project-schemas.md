# Project JSON Schemas

Store a project-owned schema at `lekalo/contracts/<name>/<version>.schema.json`. For example, `lekalo/contracts/http-action/1.2.0.schema.json` has the identity `http-action@1.2.0`. Use lowercase ASCII paths, a component name containing letters, digits and hyphens, and an exact SemVer without build metadata. The directory and its files follow [canonical path rules](canonical-structure.md).

The registry admits up to 32 schemas, each at most 64 KiB and 32 levels deep. Each file contains one JSON Schema in Draft 2020-12. The standard dialect URI may appear as `$schema`. Local JSON Pointer references such as `#/$defs/action` stay inside the same file. External reference retrieval is disabled. The project identity comes from the path, so schemas omit `$id` at every subschema to preserve the built-in schema namespace.

The core registry can compile a project schema without rebuilding Lekalo. A caller selects an exact identity and expected SHA-256. The registry checks that digest before exposing the validator. It hashes the original file bytes, including line endings. Use LF in version control for the same digest across operating systems.

Admission returns `project-schema.path-denied`, `project-schema.limit`, `project-schema.identifier-conflict`, `project-schema.external-reference`, `project-schema.unsupported-dialect`, or `project-schema.invalid` on refusal. Selection returns `project-schema.missing` for an absent identity and `project-schema.digest-mismatch` for changed bytes.

Run `lekalo lock` to pin the registry in a new project lock. For an existing lock, preview changes with `lekalo update --dry-run` and apply the displayed plan with `lekalo update --apply <plan-id>`. The file records each exact name, version, and digest in `project_schemas`. Each pin participates in the canonical lock digest and in the update plan. The planner checks the registry again at apply time.

Projects with schema pins use [lock schema 1.3.0](../contracts/lock.schema.v1.3.0.json). Existing projects without pins continue to use [lock schema 0.3.2](../contracts/lock.schema.v0.3.2.json). Lock reads refuse additions, deletions, and changes until an explicit update. The lock commands report missing or changed pins as `lock.digest-mismatch`, external references as `lock.path-denied`, limit violations as `loader.limit-exceeded`, and identifier conflicts as `lock.reference-invalid`.

For application contracts between workspace members, use [module API bundles and consumer locks](module-api.md). A project schema update only accepts the schema bytes for the current project's lock.

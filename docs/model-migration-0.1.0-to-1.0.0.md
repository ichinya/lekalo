# Model guidance: 0.1.0 to 1.0.0

Status: implementation guidance for issue #6. This document defines
preconditions and expected edits; it does not add a migrate command, loader,
IR transformation, backup workflow, or generic version registry.

Model 0.1.0 and 1.0.0 are contract versions. They are independent of product
versions: 0.1.3 published the former, and issue #6 is the prospective product
0.1.4 candidate.

## Safe schema-version-only case

Changing schema_version from 0.1.0 to 1.0.0 in every model document is the
only automatic transformation allowed, and only when all preconditions pass:

1. The project ID is one valid ASCII segment and is not lekalo or dev.
2. Every module ID is one valid ASCII segment, is not reserved, and is unique
   in the project.
3. Every symbol ID has two segments whose first segment equals its declared
   module ID. Existing 0.1.0 IDs cannot already have the optional third
   segment.
4. Every reference names an exact live symbol after the same validation.
5. Module imports, when present, are valid semantic module IDs.
6. No 0.1.0-only behavior is being mistaken for rename history. History is
   added only from owner-reviewed evidence.

The golden pair under tests/fixtures/model-compat proves a conforming project
whose four documents differ only in schema_version and validate under their
respective exact schemas.

## Owner-authored migration case

Stop when any ID fails the new class, length, reserved-name, uniqueness, or
qualification rules. The owner chooses the new meaning-preserving ID and
updates every live reference. Tooling must never guess by:

- converting uppercase to lowercase;
- translating hyphen, whitespace, Unicode, slash, or colon to underscore;
- truncating a segment or complete ID;
- selecting a duplicate winner;
- inventing or removing a kind namespace;
- treating a historical ID as a live alias.

If an existing symbol truly keeps its identity under a new semantic ID, add
the old ID to the live symbol's renamed_from and add the matching
rename_history edge. If several proven historical aliases converge, every
edge needs same_identity true. If meanings merge or one symbol replaces
another, create the new live ID and tombstone each retired ID with reason
replaced and an exact live replaced_by. A deletion uses reason deleted and
has no replaced_by.

Project and module semantic IDs do not have rename history in this contract.
Changing one is outside this migration and requires a future typed contract;
moving or renaming a physical module directory requires no semantic ID
change.

## Manual validation sequence

1. Preserve a reviewed copy of the original project.
2. Run the 0.1.0 project through the exact checker.
3. Evaluate every ID against
   [dev.lekalo.semantic-ids@0.1.0](../contracts/semantic-ids.v0.1.0.json).
4. Make only owner-approved semantic edits, if needed.
5. Change every document version together; mixed projects are invalid.
6. Run both the project check and the independent suite:

       node scripts/check-model.mjs --project path/to/project
       node scripts/test-model-contracts.mjs

The future migration issue owns dry-run plans, diffs, writes, backups,
transactions, recovery, idempotence, strict SemVer policy, and support
ranges. This issue deliberately provides none of those behaviors.

## Rollback

The schema-version-only golden case can be restored from the preserved
0.1.0 bytes. Once an owner changes semantic IDs or adds registry history,
rollback is also semantic and must restore the complete pre-migration model;
changing only schema_version back would misrepresent identity.

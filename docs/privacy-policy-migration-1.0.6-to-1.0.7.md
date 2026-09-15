# Privacy policy migration 1.0.6 to 1.0.7

Status: accepted corrective contract successor after the M0 post-acceptance
audit on 2026-09-02. This migration does not rewrite the historical issue #120
closure, product `0.0.2` release, tag, or any `1.0.6` contract bytes.

Frozen accepted policy `1.0.6` remains byte-for-byte at its versioned paths.
Its semantic identity is
`sha256:99a813a89efbdf336340390c9589a4f05d0dbbc8805748708b455a3d7a329ca7`,
raw policy hash is
`sha256:29bf9a669a775442bf393b359a92c219eb1365014ad915b312768ff24414fbe6`,
and raw manifest hash is
`sha256:3cbc9b428d47218872c62124c36fb15eeda5df2fe565ca8d3afbf477b5e16747`.
The successor manifest records it as
`yanked-after-constraint-intersection-audit` and never accepts its old manifest
anchor.

## Corrected behavior

Policy `1.0.6` correctly denied a current operation, boundary or audience that
violated a sensitivity rule. Its local-constraint broadening check, however,
compared declared constraint sets only with the disposition, operation and
destination profiles. It did not compare the whole declared set with the
applicable sensitivity intersection. A well-formed `internal` local-use
decision could therefore declare `publish` in a constraint and still receive
`allow`, even though `internal` data forbids publication.

Policy `1.0.7` defines and enforces these effective baselines:

- allowed operations are the disposition allow/transform operations
  intersected with every applicable sensitivity rule;
- allowed trust boundaries are the current operation profile intersected with
  every applicable sensitivity rule;
- allowed audiences are the selected destination profile intersected with
  every applicable sensitivity rule.

Every project, profile or operation constraint must be a subset of all three
effective sets. Any broader member denies with
`constraint.broadening-forbidden`; only then is the current point checked for a
possible `constraint.narrowed-deny`. No grant is added, and the closed
vocabularies, input/output shapes, evidence registry and authorization-subject
projection remain unchanged.

## Exact successor identities

- policy `dev.lekalo.privacy-export-policy@1.0.7`, semantic digest
  `sha256:008ec26caac4771ee14f1f3cd6c1a8e24a714643b2efb8daffd7a4064bac0129`,
  raw hash
  `sha256:bc07f38737ddf0f00d5326edb87c39117787b5a45a87e843f744c1cb4c9c3f42`;
- decision contract `dev.lekalo.privacy-export-decision@1.6.0`;
- input schema `dev.lekalo.privacy-export-input-schema@2.6.0`, raw hash
  `sha256:e40a03c96472ff961b3aef5cddc682c6ec30c5e618a984d442f1c094fb5c73d7`;
- output schema `dev.lekalo.privacy-export-output-schema@1.6.0`, raw hash
  `sha256:380bcb9b5f91d0d95e6f6eb01bb23f6c5942cd7789ec0cc91cad0b4aeae19cb6`;
- accepted manifest format `1.6.0`, raw hash
  `sha256:54fc48c63d1562baa0fa63c6e339d08202c77b12a25ca8fc5415175576c1aaa8`.

The schema shapes do not change. Their versions advance because every decision
must carry the new exact policy and decision references, and the manifest pins
new immutable bytes.

## Consumer migration

Consumers must replace exact policy, decision and schema references as one
atomic update. Cached decisions or evidence bound to the old authorization
subject cannot be relabelled: evidence must be minted against the refreshed
decision input. Downstream adapters must continue to reject any local
constraint member outside the effective intersection and must preserve exit
codes `0` for allow, `3` for well-formed deny/transform-required, and `1` for
malformed input or custody failure.

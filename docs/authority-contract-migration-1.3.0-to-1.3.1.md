# Authority contract corrective migration: 1.3.0 to 1.3.1

Status: `1.3.0` is rejected/yanked; `1.3.1` is the reviewed successor.

## Exact lifecycle

| Lifecycle | Version | Exact-byte SHA-256 |
|---|---:|---|
| Rejected/yanked candidate | `1.3.0` | `50a4b9c8533644bd61841e695fc042e4ac52307952dcccb37bf6f2d3ab3a3211` |
| Accepted corrective successor | `1.3.1` | `5c96ed68fe27956512b6de37e0fa23d223e4430d391b6a5869b12d2a8cb522d3` |

Both triples use contract ID `dev.lekalo.authority-matrix`. The `1.3.0` file and
sidecar are preserved byte-for-byte for audit, but the manifest marks that
exact triple rejected and names `1.3.1` as replacement. The checker rejects a
`1.3.0` selection with malformed/unsupported-contract exit `1`; it never
evaluates an operation against it.

## Why correction was required

The candidate protected owners but did not bind each protected path to an exact
artifact kind and reader/writer policy. A same-owner broad kind could therefore
be selected for a more-specific path. Direct-evidence writer declarations also
allowed Lekalo to write source/native maps and symbol identities, and allowed
AI Factory/AIFHub to write HLV metrics.

`1.3.1` fixes custody generally:

- every protected boundary has exact `artifactKinds`, `readers`, and `writers`;
- every reference must satisfy both its kind policy and selected boundary;
- all matching patterns are collected, not resolved by declaration order;
- specificity is the tuple: literal prefix segments descending, literal
  segment count descending, literal characters descending, segment depth
  descending, wildcard token count ascending;
- equal-specificity overlapping policies must be identical or contract loading
  fails closed;
- referenced kinds must exist, owners must agree, and boundary reader/writer
  sets must be subsets compatible with every admitted kind.

Action access is explicit: read uses source/read; write uses target/write;
one-way sync uses source/read and target/write; bidirectional proposal and claim
are read/read; promote and adopt are read/write.

## Corrected direct-evidence decisions

`native.source-map` and `native.symbol-identity` remain source/native-owned
direct evidence. Only `source-native` may write them; Lekalo, AI Factory, and
AIFHub may read them. A future Lekalo-generated mapping requires a separately
reviewed derived kind and path.

`.hlv/evidence/metrics/**` is exactly
`metrics.evaluation-evidence`. HLV alone may write it. Other actors may read it
or create their own governed envelopes elsewhere; broad HLV labels cannot claim
that more-specific subtree.

## Migration procedure

1. Reject and remove any cached `1.3.0` authority selection.
2. Pin the exact `1.3.1` triple from the manifest.
3. Verify manifest, sidecar, exact bytes, contract ID, version, registry, and
   boundary policy before evaluating data.
4. Reclassify attempted non-owner writes as policy denials; do not relabel them
   to broad kinds.
5. Re-run operation fixtures, subprocess probes, contract mutations, and the
   exhaustive protected-boundary kind/actor matrix.

This is a corrective, intentionally stricter successor over a candidate that
was never admitted. It preserves all 49 stable IDs and former #2 guarantees.
No consumer may treat filename trust, recomputed digest, a local alias, or an
unknown successor as authority.

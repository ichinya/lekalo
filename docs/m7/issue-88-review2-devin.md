# Issue #88: Devin review round 2 (fix verification)

**Verdict: ACCEPT.** The P2 finding from Codex r1 is fixed in `92dc6d54` and
verified. See `issue-88-fix1.md` for the author record.

## F1 — unsupported applicable provenance pins: VERIFIED

- `ProfileState::fingerprint_requirements` supplies per-target applicability:
  reserved `model` target relaxes adapter/capabilities (inapplicable dims allow
  matching unsupported); native/non-Model targets require **all five** pins
  known. The native decoder already forbids `target:"model"` — verified the
  boundary is covered by a live control.
- `pin_differences` now requires known pins on both stored decision and current
  fact for every applicable dimension; missing → `fingerprint-unverifiable` →
  `unverifiable`, disposition unchanged.
- The shared matcher is used by `add` preview/apply, standalone audit AND live
  lint — a pre-existing structurally-valid store cannot bypass.
- Gate coverage confirmed in `waivers-contract-gate.mjs:47-76`: capabilities
  and revision pins × {unsupported, unknown, withheld} → `unverifiable` with
  `fingerprint-unverifiable`, both standalone and live lint paths; stale-
  detection (known-pin mismatches) unchanged.
- All four family gates green with `nativePinControls:7`; core waiver tests
  9/9 including the two new applicability tests; predecessor family still
  validates at schema 0.6.4.

# Issue #100: Devin review round 2 (fix verification)

**Verdict: ACCEPT.** All three Codex r1 findings fixed (`22605b4a`, `ef188c16`,
`2b4ea23b`), independently re-verified. See `issue-100-fix1.md`.

## F1 — token total reconciliation (P1): VERIFIED

Independent repro: `inputTokens=10001` with `totalTokens=50` →
`evaluation.metric-inconsistent`, exit 1 at `validate` (previously admitted and
emitted verified success under a 10000 cap). `validate_token_totals` is shared
between arm admission and result-row import — no import bypass. Lower-bound
rule (`max(input,cached)+max(output,reasoning)`) can prove cap violation even
with unavailable totals, and gets task-failure precedence. Unknown/unsupported
states are preserved, not invented.

## F2 — infrastructure assertion classification (P2): VERIFIED

Admitted `outcome:"infrastructure"` is classified directly in the
infrastructure class — no `failures` sidecar required. Frozen precedence
(custody-security > task > provider > infrastructure > unsupported >
interruption) retained; hard-fail + infra stays `task` even with judge 100.
Covered by the live no-sidecar vector and a Rust precedence regression.

## F3 — exact integral literal decoding (P2): VERIFIED

Independent repro: raw `attempt: 0e0` → exit 0 (previously
`protocol-invalid`). Normalization is text-based (digits/exponent/trailing
zeros), never float rounding; `1.000000000000000000001` still refuses, as do
fractional/negative/non-finite/oversized spellings. Safe-integer boundary
round-trips; canonical digests unchanged (byte-identical output to
integer-spelled receipts).

## Regression surface

- Framework-lift gate: **189 checks** (up from 49 — counterexample vectors),
  all 5 families live, `externalAgentRuns:0`, origin `recorded-simulation`.
- Core lib: **1058 passed**, 0 failed. `cargo fmt --check` clean.

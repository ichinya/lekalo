# Privacy policy migration 1.0.4 to 1.0.5

Status: superseded corrective successor. Frozen accepted `1.0.5` was yanked
after an exact re-review found an output-schema conformance blocker; the
accepted line continues at `1.0.6`. Issue #120 closed on 2026-08-30 and the
accepted product release is `0.0.2`.

Frozen accepted policy `1.0.4` remains byte-for-byte at its versioned paths.
Its semantic identity is
`sha256:259cf596fcdc38423fa45d1df0937e85591569621c481197f3940f894bbc4ce5`,
raw policy hash is
`sha256:76702466ddd1d54f1c542e73b63995de7dcc81861bc641914e4bf862623bbed1`,
and raw manifest hash is
`sha256:82ad6a16080dbe7fe8555d6726312f7123a6e0682d50d4ed30d0ce95d4fdc026`.
The new manifest records it as yanked and never accepts its old manifest anchor.

Accepted `1.0.5` changes the authorizing binding from a selected context tuple
to the complete canonical Authorization Subject Profile. Its identities are:

- policy `dev.lekalo.privacy-export-policy@1.0.5`, semantic digest
  `sha256:bebd0631c2b978cd2a8264877525f5047354781e26cfe60cbc3b9d9f2aefa87a`,
  raw hash `sha256:59052a176eb61d6a4dd7676f0f73938404a69d37825f83bcc3566f6f9330e9e3`;
- decision contract `dev.lekalo.privacy-export-decision@1.4.0`;
- input schema `dev.lekalo.privacy-export-input-schema@2.4.0`, raw hash
  `sha256:7de4fbccbedbd9221211a2eb82484b3c55d8c0fb8998c095078e11c4de9c6ef0`;
- output schema `dev.lekalo.privacy-export-output-schema@1.4.0`, raw hash
  `sha256:373c2ee3e706cb6fe2637576ff8bca33c89677bc0a007645b73e61b239a2c238`;
- evidence registry `dev.lekalo.privacy-authorizing-evidence@1.1.0`, raw hash
  `sha256:6402f7918f23edc06d68101341d0ebf44039c2e17351da97f506f688d4a6e4f4`;
- subject profile `dev.lekalo.privacy-authorization-subject-profile@1.0.0`,
  raw hash `sha256:825546e4a4df4551e122c2cd548ae81c1fa2063d0bd8f256d728bffc089676a1`.
- accepted manifest raw hash
  `sha256:7db5aecafce8d13966299d8071bf9c5eac17d02069cf208cceee160ef6b9768a`.

Every authorizing evidence binding now has the closed shape
`{subjectProfileRef, subjectDigest}`. The checker independently projects the
strict input under the exact profile and compares the digest before policy
semantics. A wrong-shaped profile or digest is malformed; a well-shaped stale
digest deterministically denies with `evidence.binding-mismatch`.

Trusted #119 runtime envelopes and #89 adapters must validate/normalize the
input, compute this exact profile, then mint/bind/verify evidence. #120 verifies
declared custody and digest coherence only; it does not prove physical or
cryptographic authenticity.

# Privacy policy migration 1.0.5 to 1.0.6

Status: accepted corrective contract successor; it passed cold review, issue
#120 was closed on 2026-08-30, and the accepted product release is `0.0.2`.

Frozen accepted policy `1.0.5` remains byte-for-byte at its versioned paths.
Its semantic identity is
`sha256:bebd0631c2b978cd2a8264877525f5047354781e26cfe60cbc3b9d9f2aefa87a`,
raw policy hash is
`sha256:59052a176eb61d6a4dd7676f0f73938404a69d37825f83bcc3566f6f9330e9e3`,
raw manifest hash is
`sha256:7db5aecafce8d13966299d8071bf9c5eac17d02069cf208cceee160ef6b9768a`,
and output schema `1.4.0` raw hash is
`sha256:373c2ee3e706cb6fe2637576ff8bca33c89677bc0a007645b73e61b239a2c238`.
The successor manifest records it as yanked and never accepts its old manifest
anchor.

Accepted `1.0.6` corrects one output-contract custody error. Output schema
`1.4.0` required classification version `1.1.0` and authorizing-evidence
version `1.0.0`, the reverse of the already accepted evaluator, policy,
manifest and input contract. Output schema `1.5.0` instead requires exact
classification contract
`dev.lekalo.privacy-classification-decision@1.0.0@sha256:58626d1889990bf6120874fcd194c9fc68f2d81f05af1f3b8f7d04336fb3aa9e`
and exact authorizing-evidence registry
`dev.lekalo.privacy-authorizing-evidence@1.1.0@sha256:6402f7918f23edc06d68101341d0ebf44039c2e17351da97f506f688d4a6e4f4`.
Authority `1.3.1`, classification `1.0.0`, evidence `1.1.0`, subject profile
`1.0.0`, evaluator branches and Authorization Subject Profile semantics are
otherwise unchanged.

Current identities are:

- policy `dev.lekalo.privacy-export-policy@1.0.6`, semantic digest
  `sha256:99a813a89efbdf336340390c9589a4f05d0dbbc8805748708b455a3d7a329ca7`,
  raw hash `sha256:29bf9a669a775442bf393b359a92c219eb1365014ad915b312768ff24414fbe6`;
- decision contract `dev.lekalo.privacy-export-decision@1.5.0`;
- input schema `dev.lekalo.privacy-export-input-schema@2.5.0`, raw hash
  `sha256:1f44df586b020267ceb1bcf982981ce1a7f7ab7fb0c0aa99a88dce29af1a2150`;
- output schema `dev.lekalo.privacy-export-output-schema@1.5.0`, raw hash
  `sha256:973786421855c93484492b589012606fd81481631735731a5b59c7d21bff1aac`;
- separate CLI startup/custody error schema
  `dev.lekalo.privacy-cli-error-schema@1.0.0`, raw hash
  `sha256:8b9525ff8c5431a09546c06baa36e1afe29ecedd14b80c99275509b7b0e13cfd`;
- accepted manifest raw hash
  `sha256:3cbc9b428d47218872c62124c36fb15eeda5df2fe565ca8d3afbf477b5e16747`.

Input `2.5.0` advances only because its exact policy and decision references
must point at the one accepted current successor. Its object shapes and subject
projection are unchanged from `2.4.0`. Trusted #119 runtime envelopes and #89
adapters must mint evidence against the refreshed exact input refs; #120 still
validates declared metadata only and does not prove physical or cryptographic
authenticity.

# npm-standalone native gate fixture

`public-fixture`. One-package standalone layout (no workspace
manifest): the pnpm detection must return npm-standalone, and the
confirmed direct `node gates/verify.mjs --mode test` command runs only
through the test-only fixture runner in a disposable copy.

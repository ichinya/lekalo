# pnpm-monorepo native gate fixture

`public-fixture`. Every byte is synthetic. The committed `package.json`
files carry poison scripts that must never run; the gate commands
(`gates/verify.mjs` per package) are plain Node scripts that verify one
source file and append one marker line. They are executed only through
the test-only fixture runner in a disposable copy, never during
development, and only as approved plan commands with stage-only writes.

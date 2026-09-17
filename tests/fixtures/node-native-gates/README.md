# node-native-gates fixtures (issue #48)

`public-fixture`. Exclusively synthetic provenance: invented package
names, invented gate scripts, no real consumer, credential, absolute
host path, or copied source text. Every committed `package.json`
carries poison lifecycle scripts that must never run — and nothing in
the planner or the test harness ever runs them. Gate commands are
plain deterministic Node scripts writing stage-only marker files; the
test-only fixture runner executes them exclusively inside a disposable
copy of the fixture, never against this tree, and only as approved
plan commands.

Inventory (closed):

| Path | Purpose |
| --- | --- |
| `pnpm-monorepo/` | planner -> api -> cli chain plus unrelated web/integration; confirmed literal gate scripts; stage-only markers. |
| `npm-standalone/` | one-package standalone layout for the npm-standalone capability path (AC13). |

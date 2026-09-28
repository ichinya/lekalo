# Composer/Laravel native-gates fixture (issue #61)

A disposable **overlay** over the planner fixture
(`tests/fixtures/php-laravel/planner`): the harness copies the planner
project and applies these files on top. Planning reads only the bytes
below — no vendor tree, no provisioning, no execution.

- `composer.json` — the single Composer root
  (`lekalo/planner-laravel-fixture`) with the explicit `gate:*` scripts
  the checked-in execution policy confirms. Every confirmed script is a
  narrow literal recipe (`@php <repo-relative program> <literal args>`);
  nothing interpolates, dispatches, installs, or touches the network.
- `.lekalo/import/native-selection.json` — the host-validated selection
  document: the declared `planner` and `legacy` module roots, the
  consumer→dependency edge between them, the checked test bindings
  (`laravel`, `legacy-suite`), and the mandatory cross-module gate
  `application-boot`.

Custody: `adapters/php-laravel/composer-gates-policy.json` pins the
manifest digest and every script-entry digest of this fixture; any edit
here refuses planning with `manifest-digest-drift` until the policy is
re-pinned. That is the point — the policy is confirmation data, the
manifest is the source of truth, and the planner verifies both.

Drivers:

- `scripts/test-php-laravel-native-gates.mjs` — the acceptance harness
  (targeted selection, exactly-once suites, hostile decoder probes,
  stale-custody zero-spawn refusals, release-full fallback, closed
  contract validation, and the Rust `native run` parity leg when
  `LEKALO_BIN` is built).
- The confined execution of these gates is the qualified host runner
  (`crates/lekalo-core/src/native_gate/runner.rs`) plus its test
  battery; production `native run` still refuses until runtime
  capability is proven.

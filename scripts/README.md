# Maintenance scripts

Scripts here are maintained repository tools. Use [contributing](../docs/contributing.md) for setup and check selection. This page inventories script roles. A fixture writer may be required even when CI invokes only its verifier.

| Family | Purpose and entry points |
| --- | --- |
| Verifiers | `test-*.mjs`, `check-*.mjs` and [run-golden.mjs](run-golden.mjs); required invocations and prerequisites are in [CI](../.github/workflows/ci.yml) and their owning reference pages. |
| Documentation | [test-docs-ownership.mjs](test-docs-ownership.mjs), [test-docs-examples.mjs](test-docs-examples.mjs); both accept `--static` for prose maintenance. Runtime replay additionally needs the matching built CLI and lane dependencies. |
| Documentation writer | [update-docs-owners.mjs](update-docs-owners.mjs) requires `--write` and a matching built CLI; updates the ownership inventory and CLI index. |
| Fixture and contract writers | `gen-*.mjs`, `regen-*.mjs`, [gen-trace-fixtures.cjs](gen-trace-fixtures.cjs) and `update-golden-*.mjs`; inspect each script's accepted arguments and output paths before use. Some write immediately; others require `--write`. Review exact versions, hashes and provenance. |
| Shared helpers | `lib/`, privacy helpers, [registry-union.mjs](registry-union.mjs), [model-fixture-manifest.mjs](model-fixture-manifest.mjs) and [requirements-trace-canonical.mjs](requirements-trace-canonical.mjs). |
| Benchmarks and integrations | [bench-search.mjs](bench-search.mjs), [benchmark-ai-workspace-context.mjs](benchmark-ai-workspace-context.mjs), [ai-workspace-hook.mjs](ai-workspace-hook.mjs) and [pilot-brownfield-ts.mjs](pilot-brownfield-ts.mjs); use the owning reference's declared scope. |
| Release tooling | [release.mjs](release.mjs), [release.test.mjs](release.test.mjs) and [release maintainer guide](../docs/releasing.md). |
| CI artifact handoff | [ci-contracts-cli.mjs](ci-contracts-cli.mjs) packages and verifies the Contracts Linux CLI against the checkout SHA and content digest; [ci-contracts-cli.test.mjs](ci-contracts-cli.test.mjs) covers refused and matching artifacts. SDK oracles and release gates retain their own builds. |

Do not use fixture generators as a way to make failing gates pass. Keep committed schemas, goldens and manifests as reviewed inputs. Runtime output, logs, per-run reviews and temporary helper scripts belong under ignored `target/`, outside public fixture trees.

The [reference-project quickstart](../examples/README.md) owns `test-reference-projects.mjs` (static pins or real HTTP/PostgreSQL), `test-reference-model.mjs` (production CLI and standalone exports), `test-reference-contract.mjs` (actual maintained reference bindings and drift refusals), `materialize-reference-project.mjs` (explicit new-directory export) and `update-reference-projects.mjs` (explicit reviewed source/adapter lock writer). Provision application dependencies and the isolated database before verification; verifiers never run the updater or installers.

The four `reserve-030/031/032/040-regen.mjs` recipes were one-time migrations pinned to obsolete product binaries; they were removed from the current checkout. Their historical implementations remain in [Git history](https://github.com/ichinya/lekalo/tree/f1876ebca20a260b8a300d6bcde7bc8561977567/scripts). They are not a current release or suite-wide regeneration path.

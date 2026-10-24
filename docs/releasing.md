# Release maintainer guide

This page covers publishing binary assets. End users should follow [installation](releases.md). The implementation is [release.yml](../.github/workflows/release.yml) plus [release.mjs](../scripts/release.mjs).

## Prepare the source

1. Choose the exact release commit and require its applicable CI checks to pass.
2. Verify the workspace, `lekalo-cli` and `lekalo-core` versions. Create `v<VERSION>` on that commit; tag, Cargo versions, release title and built `--version` must agree.
3. Wait for the tag/Cargo parity CI job. Publish a GitHub Release named `<VERSION>` or `v<VERSION>` for that tag. `release: published` starts binary builds, including prereleases.

Do not move an existing published tag to add later source changes. Choose a new version/tag when new source is intended. [Versioning](versioning.md) owns contract changes and exact-byte archival rules.

## Build and upload

The workflow checks out source from the resolved tag commit and automation from the workflow commit. It runs locked release builds for the five [installation targets](releases.md), probes `--version` and `--help`, packages and extracts each archive, and probes the extracted executable. Compatibility is checked outside the source checkout.

All platform builds must pass before upload. The job rejects version/title mismatches, dirty source, a moved tag, incomplete platforms or incorrect hashes. It writes `SHA256SUMS.txt` and per-archive `.build.json` metadata, then downloads the uploaded assets and compares them with the local files.

## Existing published tags

For a tag that predates this workflow, open **Actions → Release binaries → Run workflow**, select a branch containing the workflow and supply the existing published tag in `tag`. Source still comes from that tag; automation comes from the selected workflow revision. A Cargo/version mismatch still refuses.

Rerunning replaces assets with the same names in that release; it does not retag source or replace other releases. Inspect the resolved source commit and all platform results before treating an upload as complete.

## Validate the tooling

Run `node --test scripts/release.test.mjs` for release-script changes. This local test does not publish a release or replace the five native platform builds. Record the tag commit, workflow run and resulting asset hashes with the release.

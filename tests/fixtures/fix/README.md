# Issue #74 fix fixtures

`project/` is a small, valid, deliberately noncanonical Model project. All source
bytes are LF-pinned by the repository attributes; IDs and comments are synthetic.

`golden/safe-preview.json` is the exact deterministic CLI preview. `safe.json`,
`stale.json`, `conflict.json` and `rollback.json` are positive-proof outcome
goldens consumed by `crates/lekalo-core/src/fix/tests.rs`, not fabricated CLI
receipts. Rollback is tested after actual replacements, with existing history
and custody, through an in-crate failing post-validation oracle. The production
CLI has no fault-injection switch.

Run `cargo test -p lekalo-core --lib fix::` and the
`scripts/test-fix-{contracts,cli}.mjs` gates. The live CLI gate additionally
constructs an adapter-namespaced, explicitly breaking identity-field repair and
proves originating validation, exact operation scope and version-bound replay.

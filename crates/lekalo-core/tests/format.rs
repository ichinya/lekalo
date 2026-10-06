//! Issue #94 core integration tests: the pure format service over the
//! real fixture trees — policy pinning, check/apply/stdin dispositions,
//! idempotence, comment-loss blocking, verification, bounds, and the
//! fail-closed recovery gate.
//!
//! The project selector grammar is invocation-relative, so every call
//! runs chdir'd into its sandbox under a process-wide lock (the same
//! pattern the authorization and bindings suites use).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use lekalo_core::format::{FormatMode, FormatRequest, FormatScope};
use lekalo_core::result::{DomainResult, Status};

const FIXTURES: &str = "tests/fixtures/format";

static CWD_LOCK: Mutex<()> = Mutex::new(());

fn alias_free(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("fixture path must exist");
    #[cfg(windows)]
    match canonical.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => canonical,
    }
    #[cfg(not(windows))]
    canonical
}

fn workspace_root() -> PathBuf {
    alias_free(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../"))
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create dest");
    for entry in fs::read_dir(from).expect("read source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}

/// Materialize one fixture seed into a fresh sandbox and run `work`
/// chdir'd into it. The sandbox name is the test-unique handle.
fn in_sandbox<T>(name: &str, seed: &str, work: impl FnOnce(&Path) -> T) -> T {
    let destination = std::env::temp_dir().join(format!(
        "lekalo-format-core-{}-{}",
        name,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&destination);
    copy_dir(&workspace_root().join(FIXTURES).join(seed), &destination);
    let guard = CWD_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(&destination).expect("enter sandbox");
    let outcome = work(&destination);
    std::env::set_current_dir(original).expect("restore cwd");
    drop(guard);
    let _ = fs::remove_dir_all(&destination);
    outcome
}

fn request(scope: FormatScope, mode: FormatMode) -> FormatRequest {
    FormatRequest {
        project: Some(".".to_owned()),
        scope,
        mode,
        allow_lossy_comments: false,
    }
}

fn first_reason(result: &DomainResult) -> String {
    result
        .diagnostics()
        .first()
        .map(|diagnostic| diagnostic.id().to_owned())
        .unwrap_or_default()
}

#[test]
fn the_embedded_policy_pins_the_contract_digest() {
    let policy = lekalo_core::format::Policy::embedded().expect("policy valid");
    assert_eq!(policy.digest.len(), "sha256:".len() + 64);
}

#[test]
fn check_is_read_only_and_denies_noncanonical_sources() {
    in_sandbox("check", "seeds/noncanonical", |root| {
        let before = fs::read_dir(root.join("lekalo/modules/planner"))
            .expect("inventory")
            .count();
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Check), None);
        assert_eq!(result.status(), Status::Denied);
        assert_eq!(first_reason(&result), "format.diff");
        let after = fs::read_dir(root.join("lekalo/modules/planner"))
            .expect("inventory")
            .count();
        assert_eq!(before, after, "check creates no files");
        assert!(
            !root.join(".lekalo").exists(),
            "check creates no runtime tree"
        );
    });
}

#[test]
fn apply_is_idempotent_and_matches_the_reviewed_golden() {
    in_sandbox("apply", "seeds/noncanonical", |root| {
        let first = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(first.status(), Status::Valid, "{first:?}");
        let formatted = fs::read_to_string(root.join("lekalo/modules/planner/entities.yaml"))
            .expect("formatted");
        let second = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(second.status(), Status::Valid);
        assert_eq!(
            fs::read_to_string(root.join("lekalo/modules/planner/entities.yaml"))
                .expect("formatted"),
            formatted,
            "second apply changed no bytes"
        );
        let golden = workspace_root().join(FIXTURES).join("planner-canonical");
        for relative in [
            "lekalo/project.yaml",
            "lekalo/modules/planner/module.yaml",
            "lekalo/modules/planner/entities.yaml",
            "lekalo/modules/planner/scenarios.yaml",
            "lekalo/modules/scheduling/module.yaml",
        ] {
            assert_eq!(
                fs::read(root.join(relative)).expect("formatted"),
                fs::read(golden.join(relative)).expect("golden"),
                "canonical bytes drift for {relative}"
            );
        }
    });
}

#[test]
fn scoped_module_format_leaves_out_of_scope_sources_byte_identical() {
    in_sandbox("scope", "seeds/noncanonical", |root| {
        let outside_before = fs::read(root.join("lekalo/project.yaml")).expect("before");
        let result = lekalo_core::format::run(
            &request(
                FormatScope::Module("lekalo/modules/scheduling".to_owned()),
                FormatMode::Apply,
            ),
            None,
        );
        assert_eq!(result.status(), Status::Valid);
        assert_eq!(
            fs::read(root.join("lekalo/project.yaml")).expect("after"),
            outside_before,
            "out-of-scope sources stay byte-identical"
        );
    });
}

#[test]
fn an_unknown_selector_is_a_selection_diagnostic() {
    in_sandbox("selector", "seeds/noncanonical", |_root| {
        for selector in [
            "lekalo/modules/absent",
            "lekalo/attachment.yaml",
            "../escape",
            "lekalo/../escape",
        ] {
            let result = lekalo_core::format::run(
                &request(
                    FormatScope::Document(selector.to_owned()),
                    FormatMode::Check,
                ),
                None,
            );
            assert_eq!(result.status(), Status::Invalid, "{selector}");
            let reasons: Vec<&str> = result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.id())
                .collect();
            assert!(
                reasons.contains(&"format.selection-invalid")
                    || reasons.contains(&"structure.selection-traversal"),
                "{selector}: {reasons:?}"
            );
        }
    });
}

#[test]
fn a_well_formed_path_outside_the_model_family_is_an_unsupported_surface() {
    in_sandbox("surface", "planner-canonical", |_root| {
        for selector in ["lekalo.lock", "README.md", "notes/architecture.txt"] {
            let result = lekalo_core::format::run(
                &request(
                    FormatScope::Document(selector.to_owned()),
                    FormatMode::Check,
                ),
                None,
            );
            assert_eq!(result.status(), Status::Unsupported, "{selector}");
            assert_eq!(result.exit_code(), 4, "{selector}");
            let reasons: Vec<&str> = result
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.id())
                .collect();
            assert!(
                reasons.contains(&"format.unsupported-surface"),
                "{selector}: {reasons:?}"
            );
        }
    });
}

#[test]
fn parse_errors_stay_parse_errors_and_never_become_diffs() {
    in_sandbox("parse", "seeds/invalid-parse", |_root| {
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Check), None);
        assert_eq!(result.status(), Status::Invalid);
        assert_eq!(first_reason(&result), "loader.yaml-unsupported");
    });
}

#[test]
fn unsupported_versions_refuse_without_rewriting() {
    in_sandbox("version", "seeds/unsupported-version", |root| {
        let before = fs::read(root.join("lekalo/project.yaml")).expect("before");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Check), None);
        assert_eq!(result.status(), Status::UnsupportedVersion);
        assert_eq!(
            fs::read(root.join("lekalo/project.yaml")).expect("after"),
            before
        );
    });
}

#[test]
fn comment_loss_blocks_by_default_and_applies_under_explicit_opt_in() {
    in_sandbox("loss-blocked", "seeds/comment-loss", |root| {
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Denied);
        assert!(
            result
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.id() == "format.comments-loss"),
            "the loss is enumerated"
        );
        assert!(
            root.join("lekalo/modules/scheduling/module.yaml").exists(),
            "no source removed"
        );
    });
    in_sandbox("loss-lossy", "seeds/comment-loss", |root| {
        let mut lossy = request(FormatScope::All, FormatMode::Apply);
        lossy.allow_lossy_comments = true;
        let applied = lekalo_core::format::run(&lossy, None);
        assert_eq!(applied.status(), Status::Valid, "{applied:?}");
        let text = fs::read_to_string(root.join("lekalo/modules/scheduling/module.yaml"))
            .expect("formatted");
        assert!(!text.contains("# this trailing comment has no owner line"));
    });
}

#[test]
fn ordered_arrays_are_never_reordered() {
    in_sandbox("ordered", "seeds/ordered-swap", |root| {
        let before =
            fs::read_to_string(root.join("lekalo/modules/planner/scenarios.yaml")).expect("before");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid);
        let after =
            fs::read_to_string(root.join("lekalo/modules/planner/scenarios.yaml")).expect("after");
        let covers = |text: &str| -> Vec<String> {
            text.lines()
                .filter(|line| line.trim_start().starts_with("- \"planner."))
                .map(|line| {
                    line.trim()
                        .trim_start_matches("- ")
                        .trim_matches('"')
                        .to_owned()
                })
                .collect()
        };
        assert_eq!(
            covers(&before),
            covers(&after),
            "order is preserved exactly"
        );
    });
}

#[test]
fn json_syntax_stays_json_and_formats_expanded() {
    in_sandbox("json", "seeds/noncanonical-json", |root| {
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        let entities = fs::read_to_string(root.join("lekalo/modules/planner/entities.yaml"))
            .expect("formatted");
        assert!(entities.trim_start().starts_with('{'), "JSON stays JSON");
        assert!(entities.contains("\"definitions\": [\n"), "expanded arrays");
        let check = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Check), None);
        assert_eq!(check.status(), Status::Valid, "JSON output is idempotent");
    });
}

#[test]
fn editor_mode_returns_replacement_text_and_never_writes() {
    in_sandbox("stdin", "planner-canonical", |root| {
        let path = root.join("lekalo/modules/scheduling/module.yaml");
        let source = fs::read(&path).expect("source");
        let mtime = fs::metadata(&path)
            .expect("meta")
            .modified()
            .expect("mtime");
        let buffer = b"schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: Slot scheduling module\n".to_vec();
        let result = lekalo_core::format::run(
            &request(
                FormatScope::Document("lekalo/modules/scheduling/module.yaml".to_owned()),
                FormatMode::Stdin,
            ),
            Some(&buffer),
        );
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        let replacement = result.to_human_string("lekalo");
        assert!(
            replacement.starts_with('"'),
            "the human payload of editor mode is the replacement text"
        );
        assert_eq!(fs::read(&path).expect("after"), source, "disk untouched");
        assert_eq!(
            fs::metadata(&path)
                .expect("meta")
                .modified()
                .expect("mtime"),
            mtime,
            "stdin mode never writes"
        );
    });
}

#[test]
fn crlf_and_unicode_sources_canonicalize_to_reviewed_bytes() {
    for (seed, expected) in [
        ("portability/crlf-seed", "portability/crlf-expected"),
        ("portability/unicode-seed", "portability/unicode-expected"),
    ] {
        in_sandbox(&expected.replace('/', "-"), seed, |root| {
            let result =
                lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
            assert_eq!(result.status(), Status::Valid, "{seed}: {result:?}");
            assert_eq!(
                fs::read(root.join("lekalo/project.yaml")).expect("formatted"),
                fs::read(
                    workspace_root()
                        .join(FIXTURES)
                        .join(expected)
                        .join("lekalo/project.yaml")
                )
                .expect("reviewed bytes"),
                "{seed}: canonical bytes are platform-independent"
            );
        });
    }
}

#[test]
fn an_interrupted_transaction_blocks_every_reader() {
    in_sandbox("recovery", "planner-canonical", |root| {
        let journal = root.join(".lekalo/cache/format/abc123");
        fs::create_dir_all(&journal).expect("create home");
        fs::write(
            journal.join("journal.json"),
            "{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"planId\":\"abc123\",\"manifestSha256\":\"x\"}\n",
        )
        .expect("journal");
        let check = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Check), None);
        assert_eq!(check.status(), Status::Invalid);
        assert_eq!(first_reason(&check), "format.recovery-required");
        let load = lekalo_core::loader::run(
            &lekalo_core::loader::LoadSelection {
                project: Some(".".to_owned()),
            },
            false,
        );
        assert_eq!(load.status(), Status::Invalid);
        assert_eq!(first_reason(&load), "format.recovery-required");
    });
}

#[test]
fn a_corrupt_recovery_state_refuses_apply_instead_of_guessing() {
    in_sandbox("recovery-corrupt", "planner-canonical", |root| {
        let journal = root.join(".lekalo/cache/format/bad00bad");
        fs::create_dir_all(&journal).expect("create home");
        fs::write(
            journal.join("journal.json"),
            "{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"planId\":\"bad00bad\",\"manifestSha256\":\"deadbeef\"}\n",
        )
        .expect("journal");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Invalid);
        assert_eq!(first_reason(&result), "format.recovery-required");
        assert!(
            journal.join("journal.json").exists(),
            "recovery material is never silently dropped"
        );
    });
}

#[test]
fn real_recovery_restores_the_pinned_preimages_and_formats() {
    in_sandbox("recovery-real", "seeds/noncanonical", |root| {
        let module_path = root.join("lekalo/modules/scheduling/module.yaml");
        let before = fs::read(&module_path).expect("before bytes");
        let before_digest = lekalo_core::digest::sha256_hex(&before);
        let after = b"# reformatted under the interrupted plan\n\"definitions\":\n  - \"description\": \"Slot scheduling module\"\n    \"id\": \"scheduling\"\n    \"kind\": \"module\"\n    \"version\": 1\n\"schema_version\": \"0.2.16\"\n";
        fs::write(&module_path, after).expect("after bytes");
        let after_digest = lekalo_core::digest::sha256_hex(after);

        // The runtime record family is closed and pinned (finding A): the
        // plan home carries the 47-hex base identity plus the attempt
        // suffix, and the manifest's pins must recompute exactly that
        // base identity before recovery accepts the material.
        let base = {
            let policy_digest = format!("sha256:{}", "0".repeat(64));
            let mut material = String::new();
            material.push_str(&policy_digest);
            material.push('\u{1F}');
            material.push_str("lekalo/modules/scheduling/module.yaml");
            material.push('\u{1F}');
            material.push_str(&before_digest);
            material.push('\u{1F}');
            material.push_str(&after_digest);
            (
                policy_digest,
                lekalo_core::digest::sha256_hex(material.as_bytes()),
            )
        };
        let (policy_digest, base_digest) = base;
        let plan_id = format!("{}-1", &base_digest[..47]);
        let home = root.join(".lekalo/cache/format").join(&plan_id);
        fs::create_dir_all(home.join("before/modules/scheduling")).expect("backup dirs");
        fs::write(home.join("before/modules/scheduling/module.yaml"), &before)
            .expect("backup bytes");
        let manifest = format!(
            "{{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"plan_id\":\"{plan_id}\",\"policy_digest\":\"{policy_digest}\",\"files\":[{{\"path\":\"lekalo/modules/scheduling/module.yaml\",\"before_sha256\":\"{before_digest}\",\"after_sha256\":\"{after_digest}\"}}]}}\n"
        );
        fs::write(home.join("manifest.json"), &manifest).expect("manifest");
        let manifest_digest =
            lekalo_core::digest::sha256_hex(manifest.trim_end_matches('\n').as_bytes());
        fs::write(
            home.join("journal.json"),
            format!(
                "{{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"plan_id\":\"{plan_id}\",\"manifest_sha256\":\"{manifest_digest}\"}}\n"
            ),
        )
        .expect("journal");

        // The mutating mode recovers the interrupted plan, then formats.
        // If recovery had not restored the pinned preimage first, the
        // tampered bytes (with their marker comment) would have survived
        // as the formatting input.
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        let golden = workspace_root()
            .join(FIXTURES)
            .join("planner-canonical/lekalo/modules/scheduling/module.yaml");
        assert_eq!(
            fs::read(&module_path).expect("recovered-then-formatted"),
            fs::read(golden).expect("golden"),
            "recovery restored the exact preimage; apply formatted that"
        );
        assert!(
            !home.join("journal.json").exists(),
            "the journal cleared after verified recovery"
        );
    });
}

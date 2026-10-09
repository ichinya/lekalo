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

/// The exact policy pin valid recovery material must carry (finding r4
/// 5): the digest of the embedded contract this test binary plans
/// under. Synthetic recovery records use it so the boundary is
/// exercised with re-signed, genuinely supported records.
fn embedded_policy_digest() -> String {
    lekalo_core::format::Policy::embedded()
        .expect("embedded policy is pinned")
        .digest
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
            let policy_digest = embedded_policy_digest();
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

/// The base identity of one single-file plan record (the test-side
/// spelling of the runtime's `plan_identity`).
fn plan_record(
    policy_digest: &str,
    path: &str,
    before_digest: &str,
    after_digest: &str,
) -> (String, String, String) {
    let mut material = String::new();
    material.push_str(policy_digest);
    material.push('\u{1F}');
    material.push_str(path);
    material.push('\u{1F}');
    material.push_str(before_digest);
    material.push('\u{1F}');
    material.push_str(after_digest);
    let base = lekalo_core::digest::sha256_hex(material.as_bytes());
    let plan_id = format!("{}-1", &base[..47]);
    let manifest = format!(
        "{{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"plan_id\":\"{plan_id}\",\"policy_digest\":\"{policy_digest}\",\"files\":[{{\"path\":\"{path}\",\"before_sha256\":\"{before_digest}\",\"after_sha256\":\"{after_digest}\"}}]}}"
    );
    let journal = format!(
        "{{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"plan_id\":\"{plan_id}\",\"manifest_sha256\":\"{}\"}}\n",
        lekalo_core::digest::sha256_hex(manifest.as_bytes())
    );
    (plan_id, manifest, journal)
}

/// Plant a valid single-file interrupted transaction over `root`'s
/// scheduling module (current source bytes = `current`). The only
/// caller is the live-writer test, which plants through a Windows
/// share-mode-zero handle and is `#[cfg(windows)]` — on unix the helper
/// stays for platform parity and is deliberately unused.
#[cfg_attr(not(windows), allow(dead_code))]
fn plant_interrupted(root: &Path, current: &[u8]) -> (PathBuf, String) {
    let module = root.join("lekalo/modules/scheduling/module.yaml");
    let before = fs::read(&module).expect("preimage");
    let before_digest = lekalo_core::digest::sha256_hex(&before);
    let after_digest = lekalo_core::digest::sha256_hex(current);
    let (plan_id, manifest, journal) = plan_record(
        &embedded_policy_digest(),
        "lekalo/modules/scheduling/module.yaml",
        &before_digest,
        &after_digest,
    );
    let home = root.join(".lekalo/cache/format").join(&plan_id);
    fs::create_dir_all(home.join("before/modules/scheduling")).expect("backup dirs");
    fs::write(home.join("before/modules/scheduling/module.yaml"), &before).expect("backup");
    fs::write(home.join("manifest.json"), manifest).expect("manifest");
    fs::write(home.join("journal.json"), journal).expect("journal");
    fs::write(&module, current).expect("current");
    (home, plan_id)
}

#[test]
fn forged_recovery_records_fail_every_escape_and_shape_class() {
    let escape_paths = [
        "../../lekalo/project.yaml",
        "lekalo/../../outside.yaml",
        "lekalo/modules/a/../b/module.yaml",
        ".lekalo/cache/format/victim/module.yaml",
        "lekalo.lock",
    ];
    in_sandbox("escape", "planner-canonical", |root| {
        let victim = root.join("lekalo/modules/planner/entities.yaml");
        let victim_before = fs::read(&victim).expect("victim bytes");
        for hostile in escape_paths {
            let digest = "0".repeat(64);
            let (plan_id, manifest, journal) = plan_record(
                &format!("sha256:{}", "f".repeat(64)),
                hostile,
                &digest,
                &digest,
            );
            let home = root.join(".lekalo/cache/format").join(&plan_id);
            fs::create_dir_all(home.join("before")).expect("home");
            fs::write(home.join("manifest.json"), manifest).expect("manifest");
            fs::write(home.join("journal.json"), journal).expect("journal");
            let result =
                lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
            assert_eq!(
                result.status(),
                Status::Invalid,
                "{hostile}: forged record refused"
            );
            assert_eq!(first_reason(&result), "format.recovery-required");
            assert!(
                home.join("journal.json").exists(),
                "{hostile}: material never dropped"
            );
            assert_eq!(
                fs::read(&victim).expect("victim after"),
                victim_before,
                "{hostile}: the in-project target is untouched"
            );
            let _ = fs::remove_dir_all(root.join(".lekalo"));
        }
    });
}

#[test]
fn unknown_fields_and_versions_refuse_recovery() {
    in_sandbox("unknown-fields", "planner-canonical", |root| {
        let module = root.join("lekalo/modules/scheduling/module.yaml");
        let before = fs::read(&module).expect("preimage");
        let digest = lekalo_core::digest::sha256_hex(&before);
        let (plan_id, manifest, _journal) = plan_record(
            &embedded_policy_digest(),
            "lekalo/modules/scheduling/module.yaml",
            &digest,
            &digest,
        );
        let home = root.join(".lekalo/cache/format").join(&plan_id);
        fs::create_dir_all(&home).expect("home");
        let manifest_digest = lekalo_core::digest::sha256_hex(manifest.as_bytes());
        fs::write(home.join("manifest.json"), manifest).expect("manifest");
        let variants = [
            format!(
                "{{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"plan_id\":\"{plan_id}\",\"manifest_sha256\":\"{manifest_digest}\",\"extra\":true}}\n"
            ),
            format!(
                "{{\"schema_version\":\"lekalo/format-transaction/v9.9.9\",\"plan_id\":\"{plan_id}\",\"manifest_sha256\":\"{manifest_digest}\"}}\n"
            ),
            format!(
                "{{\"schema_version\":\"lekalo/format-transaction/v0.8.0\",\"plan_id\":\"{plan_id}\",\"manifest_sha256\":\"short\"}}\n"
            ),
        ];
        for variant in &variants {
            fs::write(home.join("journal.json"), variant).expect("journal");
            let result =
                lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
            assert_eq!(result.status(), Status::Invalid, "{variant}");
            assert_eq!(first_reason(&result), "format.recovery-required");
            assert!(home.join("journal.json").exists(), "never dropped");
            assert_eq!(fs::read(&module).expect("module"), before, "untouched");
        }
    });
}

#[test]
fn a_post_crash_edit_surfaces_as_the_conflict_and_is_never_erased() {
    in_sandbox("conflict", "seeds/noncanonical", |root| {
        // Recovered source: neither the pinned preimage nor the plan's
        // after bytes, but a user's post-crash edit.
        let edit = b"schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: A user's post-crash edit\n";
        let (home, _plan_id) = plant_interrupted_with_golden_after(root, edit);
        let module = root.join("lekalo/modules/scheduling/module.yaml");

        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Denied, "{result:?}");
        assert_eq!(first_reason(&result), "format.source-changed");
        let diagnostic = result.diagnostics().first().expect("diagnostic");
        assert_eq!(
            diagnostic.path(),
            Some("lekalo/modules/scheduling/module.yaml"),
            "the conflict names the diverged pin"
        );
        // The user's edit survived and the journal stays: recovery is
        // still required, and the explicit decision owns the outcome.
        assert_eq!(fs::read(&module).expect("module"), edit, "edit preserved");
        assert!(home.join("journal.json").exists(), "journal kept");
    });
}

#[test]
fn recovery_to_the_pinned_after_state_succeeds_without_rollback() {
    in_sandbox("conflict-after", "seeds/noncanonical", |root| {
        // The current bytes already equal the plan's after pin (the
        // replacement fully landed before the crash): recovery keeps
        // them, clears the journal, and formats the rest.
        let golden = fs::read(
            workspace_root()
                .join(FIXTURES)
                .join("planner-canonical/lekalo/modules/scheduling/module.yaml"),
        )
        .expect("golden");
        let (home, _plan_id) = plant_interrupted_with_golden_after(root, &golden);
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        assert!(!home.join("journal.json").exists(), "journal cleared");
    });
}

#[test]
fn a_retried_identical_plan_gets_a_fresh_home_and_succeeds() {
    in_sandbox("retry", "seeds/noncanonical", |root| {
        let home = root.join(".lekalo/cache/format");
        let module = root.join("lekalo/modules/planner/entities.yaml");
        let plan_homes = || -> Vec<String> {
            fs::read_dir(&home)
                .expect("home")
                .filter_map(|entry| entry.ok())
                .filter(|entry| entry.file_name() != "active.lock")
                .filter(|entry| entry.file_name() != "temps")
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        };
        // Attempt 1: formats and leaves the completed immutable home.
        let first = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(first.status(), Status::Valid, "{first:?}");
        let homes = plan_homes();
        assert_eq!(homes.len(), 1, "exactly one attempt home");
        let formatted = fs::read(&module).expect("formatted bytes");
        // External revert of the whole admitted source set to the seed's
        // original noncanonical bytes: the retry re-emits the exact same
        // plan identity (same policy, same pins) and must take the real
        // attempt-suffix path — a fresh home, never a collision with the
        // completed one (finding r2 14; the prior version wrote the
        // already-formatted bytes back, a vacuous no-op).
        copy_dir(
            &workspace_root()
                .join(FIXTURES)
                .join("seeds/noncanonical/lekalo"),
            &root.join("lekalo"),
        );
        let second = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(second.status(), Status::Valid, "{second:?}");
        assert_eq!(fs::read(&module).expect("bytes"), formatted);
        let homes = plan_homes();
        assert_eq!(homes.len(), 2, "the retry took a fresh attempt home");
        assert!(
            homes.iter().any(|name| name.ends_with("-2")),
            "attempt 2: {homes:?}"
        );
    });
}

#[test]
fn editor_mode_refuses_comment_losing_buffers_instead_of_returning_them() {
    in_sandbox("stdin-loss", "planner-canonical", |root| {
        let path = root.join("lekalo/modules/scheduling/module.yaml");
        let source = fs::read(&path).expect("source");
        // The buffer carries a trailing comment with no owner line.
        let buffer = b"schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: Slot scheduling\n    module # this trailing comment has no owner line\n";
        let result = lekalo_core::format::run(
            &request(
                FormatScope::Document("lekalo/modules/scheduling/module.yaml".to_owned()),
                FormatMode::Stdin,
            ),
            Some(buffer),
        );
        assert_eq!(result.status(), Status::Denied, "{result:?}");
        // The editor-mode refusal carries the registered editor identity
        // (LEK-FORMAT-011, finding r2 9): distinct from the disk-mode
        // loss denial, with the exact loss span (finding r2 8).
        assert_eq!(first_reason(&result), "format.stdin-loss");
        let diagnostic = result.diagnostics().first().expect("diagnostic");
        assert_eq!(
            diagnostic.path(),
            Some("lekalo/modules/scheduling/module.yaml")
        );
        let start = diagnostic.start().expect("the span requirement");
        assert_eq!(start.line, 7, "the loss line");
        assert!(start.byte > 0, "the real byte offset, never hardcoded 0");
        assert_eq!(fs::read(&path).expect("after"), source, "disk untouched");
    });
}

#[test]
fn editor_oracle_judges_semantic_edits_against_the_unsaved_buffer() {
    in_sandbox("stdin-oracle", "planner-canonical", |root| {
        let path = root.join("lekalo/modules/scheduling/module.yaml");
        let source = fs::read(&path).expect("saved bytes");
        // The buffer semantically diverges from the saved file (a new
        // description): against the saved file this would be refused as
        // format.semantic-change; the unsaved-buffer oracle accepts it.
        let buffer = b"schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: The unsaved semantic edit\n";
        let result = lekalo_core::format::run(
            &request(
                FormatScope::Document("lekalo/modules/scheduling/module.yaml".to_owned()),
                FormatMode::Stdin,
            ),
            Some(buffer),
        );
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        let replacement = result.to_human_string("lekalo");
        assert!(
            replacement.contains("The unsaved semantic edit"),
            "the replacement carries the buffer's edit: {replacement}"
        );
        assert_eq!(fs::read(&path).expect("after"), source, "disk untouched");
    });
}

#[cfg(windows)]
#[test]
fn a_live_windows_writer_is_refused_and_never_rolled_back() {
    in_sandbox("live-lock-win", "seeds/noncanonical", |root| {
        // A real live writer: a held share-mode-zero handle on the
        // ownership file (std only). The second formatter refuses as busy
        // and never rolls the live writer's applied sources back.
        let module = root.join("lekalo/modules/scheduling/module.yaml");
        // The live writer's applied bytes: the canonical rendering of the
        // pinned preimage (comment-only difference — a live writer's
        // replace is always the plan's after state, never a semantic
        // edit; finding r4 5's restore oracle refuses semantic drift).
        let replaced = b"# the live writer applied this\n\"definitions\":\n  - \"description\": \"Slot scheduling module\"\n    \"id\": \"scheduling\"\n    \"kind\": \"module\"\n    \"version\": 1\n\"schema_version\": \"0.2.16\"\n";
        let (home, _plan_id) = plant_interrupted(root, replaced);
        let lock = root.join(".lekalo/cache/format/active.lock");
        std::fs::create_dir_all(lock.parent().unwrap()).expect("home");
        use std::os::windows::fs::OpenOptionsExt;
        let guard = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(&lock)
            .expect("hold the ownership handle");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Denied, "{result:?}");
        assert_eq!(first_reason(&result), "format.locked");
        assert_eq!(
            fs::read(&module).expect("module"),
            replaced,
            "live state untouched"
        );
        assert!(home.join("journal.json").exists(), "journal untouched");
        drop(guard);
        let recovered =
            lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(recovered.status(), Status::Valid, "{recovered:?}");
        assert!(!home.join("journal.json").exists(), "journal cleared");
    });
}

/// Plant a valid interrupted transaction whose after pin is the canonical
/// formatting of the preimage (the golden bytes), while the current
/// source bytes are `current` — the honest post-crash shape.
fn plant_interrupted_with_golden_after(root: &Path, current: &[u8]) -> (PathBuf, String) {
    let module = root.join("lekalo/modules/scheduling/module.yaml");
    let before = fs::read(&module).expect("preimage");
    let before_digest = lekalo_core::digest::sha256_hex(&before);
    let golden = fs::read(
        workspace_root()
            .join(FIXTURES)
            .join("planner-canonical/lekalo/modules/scheduling/module.yaml"),
    )
    .expect("golden");
    let after_digest = lekalo_core::digest::sha256_hex(&golden);
    let (plan_id, manifest, journal) = plan_record(
        &embedded_policy_digest(),
        "lekalo/modules/scheduling/module.yaml",
        &before_digest,
        &after_digest,
    );
    let home = root.join(".lekalo/cache/format").join(&plan_id);
    fs::create_dir_all(home.join("before/modules/scheduling")).expect("backup dirs");
    fs::write(home.join("before/modules/scheduling/module.yaml"), &before).expect("backup");
    fs::write(home.join("manifest.json"), manifest).expect("manifest");
    fs::write(home.join("journal.json"), journal).expect("journal");
    fs::write(&module, current).expect("current");
    (home, plan_id)
}

#[cfg(unix)]
#[test]
fn a_link_spelled_format_home_is_denied_on_sight() {
    // Hostile-link probe: a symlinked runtime home (or journal) must be
    // refused by the read path, never followed into recovery.
    in_sandbox("hostile-link", "planner-canonical", |root| {
        let real_home = root.join("elsewhere/format");
        fs::create_dir_all(&real_home).expect("real home");
        fs::write(real_home.join("journal.json"), b"{}\n").expect("journal");
        // The link's parent chain must exist before linking (finding
        // r2 10): the runtime home may not exist in a canonical fixture,
        // and the setup must not panic before the formatter runs.
        fs::create_dir_all(root.join(".lekalo/cache")).expect("link parent");
        std::os::unix::fs::symlink(&real_home, root.join(".lekalo/cache/format"))
            .expect("symlinked home");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_ne!(
            result.status(),
            Status::Valid,
            "a linked home never recovers"
        );
        // The linked tree is untouched: recovery never followed the link.
        assert!(real_home.join("journal.json").exists());
    });
}

#[cfg(unix)]
#[test]
fn a_link_spelled_source_in_a_forged_record_is_refused() {
    in_sandbox("hostile-link-source", "planner-canonical", |root| {
        // A record pinning a path that exists on disk only as a symlink:
        // the restore must refuse the replace, never write through it.
        let victim = root.join("outside-victim.yaml");
        fs::write(&victim, b"victim: true\n").expect("victim");
        let link = root.join("lekalo/modules/scheduling/link.yaml");
        std::os::unix::fs::symlink(&victim, &link).expect("source link");
        let digest = "0".repeat(64);
        let (plan_id, manifest, journal) = plan_record(
            &format!("sha256:{}", "f".repeat(64)),
            "lekalo/modules/scheduling/link.yaml",
            &digest,
            &digest,
        );
        let home = root.join(".lekalo/cache/format").join(&plan_id);
        fs::create_dir_all(home.join("before")).expect("home");
        fs::write(home.join("manifest.json"), manifest).expect("manifest");
        fs::write(home.join("journal.json"), journal).expect("journal");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(
            result.status(),
            Status::Invalid,
            "link-pinned record refused"
        );
        assert_eq!(
            fs::read(&victim).expect("victim"),
            b"victim: true\n",
            "never written through"
        );
    });
}

#[test]
fn a_post_crash_deleted_source_is_the_conflict_not_a_generic_refusal() {
    in_sandbox("conflict-deleted", "seeds/noncanonical", |root| {
        // The pinned source was deleted after the crash (finding r2 12):
        // deletion is the most severe divergence, so it surfaces as the
        // explicit conflict decision — never the generic recovery
        // refusal — with the never-realizable zero digest.
        let golden = fs::read(
            workspace_root()
                .join(FIXTURES)
                .join("planner-canonical/lekalo/modules/scheduling/module.yaml"),
        )
        .expect("golden");
        let (home, _plan_id) = plant_interrupted_with_golden_after(root, &golden);
        fs::remove_file(root.join("lekalo/modules/scheduling/module.yaml")).expect("delete");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Denied, "{result:?}");
        assert_eq!(first_reason(&result), "format.source-changed");
        let diagnostic = result.diagnostics().first().expect("diagnostic");
        let deleted = matches!(
            diagnostic.data().get("currentDigest"),
            Some(lekalo_core::diagnostics::DataValue::Token(value))
                if value.chars().all(|character| character == '0') && value.len() == 64
        );
        assert!(deleted, "the deleted pin carries the zero spelling");
        assert!(home.join("journal.json").exists(), "journal kept");
        assert!(
            !root.join("lekalo/modules/scheduling/module.yaml").exists(),
            "recovery never invents the deleted file"
        );
    });
}

#[test]
fn stdin_mode_refuses_a_non_document_scope_pairing() {
    in_sandbox("stdin-scope", "seeds/noncanonical", |root| {
        // The public core refuses Stdin+All (finding r2 13): the editor
        // replacement is defined for exactly one declared document.
        let result = lekalo_core::format::run(
            &request(FormatScope::All, FormatMode::Stdin),
            Some(b"a: 1\n"),
        );
        assert_eq!(result.status(), Status::Invalid, "{result:?}");
        assert_eq!(first_reason(&result), "format.selection-invalid");
        let result = lekalo_core::format::run(
            &request(
                FormatScope::Module("lekalo/modules/scheduling".to_owned()),
                FormatMode::Stdin,
            ),
            Some(b"a: 1\n"),
        );
        assert_eq!(result.status(), Status::Invalid, "{result:?}");
        let _ = root;
    });
}

#[test]
fn a_crashed_pre_journal_home_is_swept_and_the_retry_succeeds() {
    in_sandbox("pre-journal-residue", "seeds/noncanonical", |root| {
        // A crash between ownership and the manifest write leaves a home
        // with neither record (finding r2 2): the mutating run reclaims
        // it and the identical retry succeeds instead of colliding on
        // exclusive backup writes forever.
        let home = root.join(".lekalo/cache/format");
        let residue = home.join("aabbccdd-1");
        fs::create_dir_all(residue.join("before/modules/planner")).expect("residue");
        fs::write(
            residue.join("before/modules/planner/entities.yaml"),
            b"leftover: partial\n",
        )
        .expect("partial backup");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        assert!(!residue.exists(), "the record-less residue was reclaimed");
        // And a second full run is a byte-identical no-op.
        let again = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(again.status(), Status::Valid, "{again:?}");
    });
}

#[test]
fn an_eighth_and_ninth_identical_retry_still_succeed() {
    in_sandbox("attempt-rotation", "seeds/noncanonical", |root| {
        // Nine format/revert cycles of one change set (finding r2 4):
        // the bounded attempt chain reclaims its oldest completed home
        // instead of permanently denying the plan after eight attempts.
        let module = root.join("lekalo/modules/planner/entities.yaml");
        let first = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(first.status(), Status::Valid, "{first:?}");
        let golden = fs::read(&module).expect("formatted bytes");
        for cycle in 1..=9usize {
            copy_dir(
                &workspace_root()
                    .join(FIXTURES)
                    .join("seeds/noncanonical/lekalo"),
                &root.join("lekalo"),
            );
            let result =
                lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
            assert_eq!(result.status(), Status::Valid, "cycle {cycle}: {result:?}");
            assert_eq!(fs::read(&module).expect("bytes"), golden);
        }
        let homes: Vec<_> = fs::read_dir(root.join(".lekalo/cache/format"))
            .expect("home")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name() != "active.lock")
            .filter(|entry| entry.file_name() != "temps")
            .collect();
        assert!(
            homes.len() <= 8,
            "the chain stays bounded: {:?}",
            homes.len()
        );
    });
}

#[test]
fn stdin_mode_formats_the_whole_buffer_of_its_declared_document() {
    // Already covered by editor_mode_returns_replacement_text; this test
    // pins the pairing gate positively: Stdin+Document is accepted.
    in_sandbox("stdin-doc-ok", "planner-canonical", |_root| {
        let buffer = b"schema_version: \"0.2.16\"\ndefinitions:\n- id: scheduling\n  kind: module\n  version: 1\n  description: Slot scheduling module\n".to_vec();
        let result = lekalo_core::format::run(
            &request(
                FormatScope::Document("lekalo/modules/scheduling/module.yaml".to_owned()),
                FormatMode::Stdin,
            ),
            Some(&buffer),
        );
        assert_eq!(result.status(), Status::Valid, "{result:?}");
    });
}

#[test]
fn a_hard_linked_ownership_file_is_refused_and_never_stamped() {
    // Finding r2 1/r3 1, platform-independent since the r3 Windows
    // verdict: a hard link at active.lock pointing at an external victim
    // must refuse the ownership (multiply-linked file) instead of
    // stamping the plan id through the link into the victim. The seed
    // needs formatting so the mutating run genuinely reaches the
    // ownership acquisition — a canonical tree is a no-op apply that
    // never touches the lock (the former canonical fixture proved
    // nothing). Windows joins the gate with the link count read through
    // GetFileInformationByHandle.
    in_sandbox("hardlink-lock", "seeds/noncanonical", |root| {
        let victim = root.join("outside-victim.txt");
        fs::write(&victim, b"DO NOT TOUCH EXTERNAL FILE\n").expect("victim");
        let home = root.join(".lekalo/cache/format");
        fs::create_dir_all(&home).expect("home");
        fs::hard_link(&victim, home.join("active.lock")).expect("planted hard link");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_ne!(
            result.status(),
            Status::Valid,
            "a multiply-linked ownership file never grants ownership"
        );
        assert_eq!(
            fs::read(&victim).expect("victim bytes"),
            b"DO NOT TOUCH EXTERNAL FILE\n",
            "the external victim is never clobbered through the link"
        );
        assert!(
            home.join("active.lock").exists(),
            "the lock is never removed"
        );
    });
}

#[cfg(unix)]
#[test]
fn a_symlinked_ownership_file_is_refused_by_the_open_itself() {
    // O_NOFOLLOW semantics: the open of the final component refuses the
    // symlink before any handle exists, so no stamp can follow it. The
    // seed needs formatting so the run genuinely reaches the open — a
    // canonical tree is a no-op apply that never touches the lock (the
    // former canonical fixture passed only through the loader's runtime
    // hygiene scan, never through the open).
    in_sandbox("symlink-lock", "seeds/noncanonical", |root| {
        let victim = root.join("outside-victim.txt");
        fs::write(&victim, b"victim\n").expect("victim");
        let home = root.join(".lekalo/cache/format");
        fs::create_dir_all(&home).expect("home");
        std::os::unix::fs::symlink(&victim, home.join("active.lock")).expect("planted symlink");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_ne!(result.status(), Status::Valid, "the link never owns");
        assert_eq!(fs::read(&victim).expect("victim"), b"victim\n");
    });
}

#[test]
fn an_orphaned_rename_temp_in_a_source_directory_is_reclaimed_by_apply() {
    // Finding r2 3 (platform-independent since the r3 4 fix): a legacy
    // crash left a temp sibling inside the governed tree with NO
    // formatter state on disk at all; the governed scan rejects the
    // dotfile, so the mutating run must reclaim it — through the owned
    // residue probe, never skipped for a missing runtime home — before
    // its own load succeeds.
    in_sandbox("stale-temp", "seeds/noncanonical", |root| {
        let stale = root.join("lekalo/modules/scheduling/.module.yaml.lekalo-format-new");
        fs::write(&stale, b"orphaned: partial bytes\n").expect("orphan temp");
        // Sanity: the governed scan really refuses this tree.
        let blocked = lekalo_core::loader::run(
            &lekalo_core::loader::LoadSelection {
                project: Some(".".to_owned()),
            },
            false,
        );
        assert_ne!(
            blocked.status(),
            Status::Valid,
            "the dotfile blocks readers"
        );
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        assert!(!stale.exists(), "the orphan was reclaimed");
        let temps = root.join(".lekalo/cache/format/temps");
        let leftover = temps.read_dir().map(|entries| entries.count()).unwrap_or(0);
        assert_eq!(leftover, 0, "no rename temp survives a committed apply");
    });
}

#[test]
fn an_orphaned_recovery_temp_does_not_poison_the_next_recovery() {
    // Finding r3 3: a process death between a recovery temp's creation
    // and its rename strands the fixed-name restore temp. Recovery used
    // to run before the sweep, so every retry died in the same
    // restore-failed state: the recovery's own exclusive create collided
    // with the orphan it could never reach. The mutating run now reclaims
    // the orphan on entry — before recovery — and recovery temps carry a
    // process-unique namespace, so a stale orphan can never block the
    // recovery it stranded.
    in_sandbox("recovery-orphan", "seeds/noncanonical", |root| {
        let golden = fs::read(
            workspace_root()
                .join(FIXTURES)
                .join("planner-canonical/lekalo/modules/scheduling/module.yaml"),
        )
        .expect("golden");
        let (home, _plan_id) = plant_interrupted_with_golden_after(root, &golden);
        // The exact orphan a crashed recovery leaves: the producer's
        // temp suffix under the runtime temp home.
        let temps = root.join(".lekalo/cache/format/temps");
        fs::create_dir_all(&temps).expect("temps home");
        let orphan = temps.join("owned.restore.module.yaml.lekalo-format-new");
        fs::write(&orphan, b"orphaned: partial bytes\n").expect("orphan temp");
        // Two consecutive mutating runs: the first reclaims the orphan
        // and recovers; the second proves the retry is no longer
        // poisoned by anything the recovery left behind.
        for attempt in 1..=2 {
            let result =
                lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
            assert_eq!(
                result.status(),
                Status::Valid,
                "attempt {attempt}: {result:?}"
            );
        }
        assert!(!orphan.exists(), "the stranded orphan was reclaimed");
        assert!(!home.join("journal.json").exists(), "the journal cleared");
        let leftover = temps.read_dir().map(|entries| entries.count()).unwrap_or(0);
        assert_eq!(leftover, 0, "no rename temp survives a committed recovery");
    });
}

#[test]
fn a_legacy_project_root_temp_is_reclaimed_without_any_formatter_state() {
    // Finding r3 4: the legacy emitter could strand its rename temp as
    // the `project.yaml` sibling at the `lekalo/` root, but the legacy
    // sweep covered only the module directories — the project stayed
    // permanently unloadable. The root is part of the closed sweep set
    // now, and the cleanup runs even with no formatter state on disk.
    in_sandbox("legacy-root-temp", "seeds/noncanonical", |root| {
        let orphan = root.join("lekalo/.project.yaml.lekalo-format-new");
        fs::write(&orphan, b"orphaned: partial bytes\n").expect("orphan temp");
        assert!(!root.join(".lekalo").exists(), "no formatter state planted");
        // Sanity: the governed scan really refuses this tree.
        let blocked = lekalo_core::loader::run(
            &lekalo_core::loader::LoadSelection {
                project: Some(".".to_owned()),
            },
            false,
        );
        assert_ne!(
            blocked.status(),
            Status::Valid,
            "the dotfile blocks readers"
        );
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        assert!(!orphan.exists(), "the project-root orphan was reclaimed");
    });
}

#[test]
fn a_module_temp_is_swept_when_the_runtime_temp_home_is_absent() {
    // Finding r3 4 control: a record-less plan home and a module orphan
    // with NO runtime temp home — the former sweep's `temps`-home error
    // returned before the legacy pass, so the orphan survived and the
    // run failed with structure.path-segment. The legacy pass is
    // independent of the optional runtime home now.
    in_sandbox("legacy-no-temps-home", "seeds/noncanonical", |root| {
        let orphan = root.join("lekalo/modules/scheduling/.module.yaml.lekalo-format-new");
        fs::write(&orphan, b"orphaned: partial bytes\n").expect("orphan temp");
        let home = root.join(".lekalo/cache/format/aabbccdd-1");
        fs::create_dir_all(&home).expect("plan home");
        assert!(!root.join(".lekalo/cache/format/temps").exists());
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        assert!(!orphan.exists(), "the module orphan was reclaimed");
        assert!(!home.exists(), "the record-less plan home was reclaimed");
    });
}

#[test]
fn quoted_scalar_trailing_comments_are_preserved_everywhere() {
    // Finding r4 1 (critical): a `#` comment after a quoted scalar fell
    // inside the parser's over-extended token span and was silently
    // destroyed in every mode. Every trailing comment now tokenizes and
    // rides its owner line through formatting.
    in_sandbox("quoted-comment", "planner-canonical", |root| {
        let path = root.join("lekalo/modules/scheduling/module.yaml");
        let seeded = b"\"definitions\":\n  - \"id\": \"scheduling\" # trailing on the map item\n    \"kind\": \"module\" # trailing on a nested entry\n    \"version\": 1\n    \"description\": \"Slot scheduling module\" # trailing on a quoted value\n\"schema_version\": \"0.2.16\" # trailing at the root\n";
        fs::write(&path, seeded).expect("seed");
        // Apply: no loss diagnostic, every comment preserved on its owner.
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Valid, "{result:?}");
        let formatted = fs::read_to_string(&path).expect("formatted");
        for marker in [
            "# trailing on the map item",
            "# trailing on a nested entry",
            "# trailing on a quoted value",
            "# trailing at the root",
        ] {
            assert!(formatted.contains(marker), "{marker}: {formatted}");
        }
        // The formatted form is a fixed point (check is clean).
        let check = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Check), None);
        assert_eq!(check.status(), Status::Valid, "{check:?}");
        // Editor mode: the replacement keeps the comment too.
        let buffer = seeded.to_vec();
        let stdin = lekalo_core::format::run(
            &request(
                FormatScope::Document("lekalo/modules/scheduling/module.yaml".to_owned()),
                FormatMode::Stdin,
            ),
            Some(&buffer),
        );
        assert_eq!(stdin.status(), Status::Valid, "{stdin:?}");
        assert!(
            stdin.to_human_string("lekalo").contains("# trailing at the root"),
            "editor replacement keeps the trailing comment"
        );
    });
}

#[test]
fn a_journal_in_the_temp_home_never_blocks_readers() {
    // Finding r4 3 (low): the reader gate treated any directory under
    // the formatter home — including the rename-temp home `temps` — as
    // a plan home, so a planted `temps/journal.json` blocked every
    // reader forever while `format` (which excludes `temps` from the
    // plan-home set) could never remediate it. The gate now skips the
    // runtime temp home exactly like recovery does.
    in_sandbox("temps-journal", "planner-canonical", |root| {
        let temps = root.join(".lekalo/cache/format/temps");
        fs::create_dir_all(&temps).expect("temp home");
        fs::write(temps.join("journal.json"), b"{\"planted\":true}\n").expect("planted record");
        let load = lekalo_core::loader::run(
            &lekalo_core::loader::LoadSelection {
                project: Some(".".to_owned()),
            },
            false,
        );
        assert_eq!(load.status(), Status::Valid, "readers are not blocked by temps custody");
        let check = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Check), None);
        assert_eq!(check.status(), Status::Valid, "check is not blocked either");
        // And a genuine plan home still blocks: the gate keeps its teeth.
        let real = root.join(".lekalo/cache/format/abc123");
        fs::create_dir_all(&real).expect("plan home");
        fs::write(real.join("journal.json"), b"{\"schema_version\":\"lekalo/format-transaction/v0.8.0\"}\n").expect("real journal");
        let blocked = lekalo_core::loader::run(
            &lekalo_core::loader::LoadSelection {
                project: Some(".".to_owned()),
            },
            false,
        );
        assert_eq!(blocked.status(), Status::Invalid);
        assert_eq!(first_reason(&blocked), "format.recovery-required");
    });
}

#[test]
fn legacy_cleanup_never_touches_unrelated_user_files() {
    // Finding r4 4 (P1): the legacy sweep treated any dotfile ending in
    // `.lekalo-format-new` as deletion authority. Cleanup is confined to
    // the exact emitted-Model temps: an unrelated user file with the
    // suffix stays byte-identical, while the genuinely owned temps (for
    // the emitted documents of the tree) are still reclaimed.
    in_sandbox("unowned-legacy", "seeds/noncanonical", |root| {
        let user_one = root.join("lekalo/.private-notes.lekalo-format-new");
        let user_two = root.join("lekalo/modules/planner/.unrelated.txt.lekalo-format-new");
        let user_three = root.join("lekalo/modules/planner/.invented.yaml.lekalo-format-new");
        fs::write(&user_one, b"USER FILE - NEVER A FORMATTER TEMP\n").expect("user file");
        fs::write(&user_two, b"USER FILE - NEVER A FORMATTER TEMP\n").expect("user file");
        fs::write(&user_three, b"USER FILE - NEVER A FORMATTER TEMP\n").expect("user file");
        // The owned legacy temps for the emitted documents of the tree.
        let owned_root = root.join("lekalo/.project.yaml.lekalo-format-new");
        let owned_module =
            root.join("lekalo/modules/planner/.module.yaml.lekalo-format-new");
        let owned_kind =
            root.join("lekalo/modules/planner/.scenarios.yaml.lekalo-format-new");
        fs::write(&owned_root, b"orphaned: root temp\n").expect("owned temp");
        fs::write(&owned_module, b"orphaned: module temp\n").expect("owned temp");
        fs::write(&owned_kind, b"orphaned: kind temp\n").expect("owned temp");
        let user_bytes = b"USER FILE - NEVER A FORMATTER TEMP\n";
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        // The user dotfiles are not Model source: the governed scan keeps
        // refusing the tree, and the run must not delete them to
        // legitimate it — the refusal, not deletion, is the answer.
        assert_ne!(result.status(), Status::Valid, "{result:?}");
        assert_eq!(
            fs::read(&user_one).expect("user file kept"),
            user_bytes,
            "the user file at the root is never deleted"
        );
        assert_eq!(
            fs::read(&user_two).expect("user file kept"),
            user_bytes,
            "the user file in a module directory is never deleted"
        );
        assert_eq!(
            fs::read(&user_three).expect("user file kept"),
            user_bytes,
            "an invented model-shaped name is still a user file"
        );
        assert!(!owned_root.exists(), "the owned root temp is reclaimed");
        assert!(!owned_module.exists(), "the owned module temp is reclaimed");
        assert!(!owned_kind.exists(), "the owned kind temp is reclaimed");
        // A tree carrying ONLY owned temps (the real stranded-crash
        // shape) formats cleanly: the sweep legitimately reclaims them.
        for user in [&user_one, &user_two, &user_three] {
            fs::remove_file(user).expect("remove user file");
        }
        let clean = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(clean.status(), Status::Valid, "{clean:?}");
    });
}

#[test]
fn an_unowned_suffixed_file_is_answered_by_the_structure_scan_not_the_sweep() {
    // Finding r4 4 control: a tree whose only anomaly is an unrelated
    // suffixed dotfile stays refused by the governed scan (the dotfile
    // is not Model source) — cleanup neither deletes it nor legitimates
    // the tree; the loader's own refusal answers it.
    in_sandbox("unowned-blocks", "planner-canonical", |root| {
        let user = root.join("lekalo/modules/scheduling/.notes.txt.lekalo-format-new");
        fs::write(&user, b"USER FILE\n").expect("user file");
        let blocked = lekalo_core::loader::run(
            &lekalo_core::loader::LoadSelection {
                project: Some(".".to_owned()),
            },
            false,
        );
        assert_ne!(blocked.status(), Status::Valid, "the governed scan refuses");
        assert!(user.exists(), "the user file survives the refusal");
        // The mutating run leaves it exactly where it was — even though
        // it could format the rest of the tree — because the sweep has
        // no authority over it and a partial legitimation is never the
        // formatter's answer.
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_ne!(result.status(), Status::Valid, "{result:?}");
        assert_eq!(fs::read(&user).expect("user file"), b"USER FILE\n");
    });
}

#[test]
fn a_forged_semantic_preimage_is_refused_before_any_restore() {
    // Finding r4 5: recovery material whose backup digest-pins a
    // semantic edit must never reach the disk. The record is fully
    // hash-consistent and carries the supported policy pin; its after
    // pin names the honest canonical source, so the preflight conflict
    // class never fires — the restore itself is what would smuggle the
    // edit in. The pre-restore semantic oracle refuses it.
    in_sandbox("forged-preimage-core", "planner-canonical", |root| {
        let module_path = root.join("lekalo/modules/scheduling/module.yaml");
        let before = fs::read(&module_path).expect("canonical bytes");
        let before_digest = lekalo_core::digest::sha256_hex(&before);
        let forged = b"# forged semantic edit\n\"definitions\":\n  - \"description\": \"FORGED SEMANTIC EDIT\"\n    \"id\": \"scheduling\"\n    \"kind\": \"module\"\n    \"version\": 1\n\"schema_version\": \"0.2.16\"\n";
        let forged_digest = lekalo_core::digest::sha256_hex(forged);
        let (plan_id, manifest, journal) = plan_record(
            &embedded_policy_digest(),
            "lekalo/modules/scheduling/module.yaml",
            &forged_digest,
            &before_digest,
        );
        let home = root.join(".lekalo/cache/format").join(&plan_id);
        fs::create_dir_all(home.join("before/modules/scheduling")).expect("backup dirs");
        fs::write(home.join("before/modules/scheduling/module.yaml"), forged).expect("forged backup");
        fs::write(home.join("manifest.json"), manifest).expect("manifest");
        fs::write(home.join("journal.json"), journal).expect("journal");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Invalid, "{result:?}");
        assert_eq!(first_reason(&result), "format.recovery-required");
        assert!(home.join("journal.json").exists(), "material never dropped");
        assert_eq!(
            fs::read(&module_path).expect("source"),
            before,
            "the canonical source never receives the forged semantics"
        );
    });
}

#[test]
fn an_unsupported_policy_pin_refuses_recovery() {
    // Finding r4 5 control: a perfectly re-signed record naming the
    // all-zeros policy pin is refused — hash-consistency is not
    // authenticity — while the identical record under the embedded pin
    // is admitted.
    in_sandbox("stale-pin-core", "planner-canonical", |root| {
        let module_path = root.join("lekalo/modules/scheduling/module.yaml");
        let before = fs::read(&module_path).expect("canonical bytes");
        let digest = lekalo_core::digest::sha256_hex(&before);
        let (plan_id, manifest, journal) = plan_record(
            &format!("sha256:{}", "9".repeat(64)),
            "lekalo/modules/scheduling/module.yaml",
            &digest,
            &digest,
        );
        let home = root.join(".lekalo/cache/format").join(&plan_id);
        fs::create_dir_all(home.join("before/modules/scheduling")).expect("backup dirs");
        fs::write(home.join("before/modules/scheduling/module.yaml"), &before).expect("backup");
        fs::write(home.join("manifest.json"), manifest).expect("manifest");
        fs::write(home.join("journal.json"), journal).expect("journal");
        let result = lekalo_core::format::run(&request(FormatScope::All, FormatMode::Apply), None);
        assert_eq!(result.status(), Status::Invalid, "{result:?}");
        assert_eq!(first_reason(&result), "format.recovery-required");
        assert!(home.join("journal.json").exists(), "material never dropped");
        assert_eq!(fs::read(&module_path).expect("source"), before);
    });
}

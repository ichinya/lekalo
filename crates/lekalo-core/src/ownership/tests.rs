use super::*;

#[test]
fn closed_request_refuses_duplicate_keys_and_unknown_members() {
    assert!(parse_request(br#"{"schemaVersion":"one","schemaVersion":"two"}"#).is_err());
    assert!(parse_request(
        br#"{"schemaVersion":"lekalo/ownership-transition-request/v1.0.0","unexpected":true}"#
    )
    .is_err());
    assert!(parse_plan(br#"{"schemaVersion":"lekalo/ownership-transition-plan/v1.0.0"}"#).is_err());
}

#[test]
fn scoped_handoffs_do_not_block_unrelated_legacy_modules() {
    let root = tempfile::tempdir().unwrap();
    let state = OwnershipState {
        schema_version: format!("lekalo/ownership-state/v{VERSION}"),
        pending: None,
        assignments: vec![Assignment {
            module: "planner".to_owned(),
            symbol: Some("planner.focus".to_owned()),
            mode: mg::Mode::Contracted,
            transition: mg::digest(b"transition"),
            artifacts: vec![],
        }],
    };
    mg::fs::write_file(root.path(), STATE, &bytes(&state)).unwrap();
    assert!(legacy_mutation_allowed(root.path(), "planner").is_err());
    assert!(legacy_mutation_allowed(root.path(), "legacy").is_ok());
}

#[test]
fn managed_and_ownership_mutations_share_one_os_lock() {
    let root = tempfile::tempdir().unwrap();
    let guard = crate::project_writer::WriterLock::mutation(
        root.path(),
        crate::project_writer::MutationFamily::Ownership,
    )
    .unwrap();
    assert!(mg::history::lock(root.path()).is_err());
    drop(guard);
    let managed = mg::history::lock(root.path()).unwrap();
    assert!(crate::project_writer::WriterLock::mutation(
        root.path(),
        crate::project_writer::MutationFamily::Ownership
    )
    .is_err());
    drop(managed);
}

#[test]
fn pending_ownership_blocks_other_writer_families() {
    let root = tempfile::tempdir().unwrap();
    mg::fs::write_file(root.path(), &format!("{HOME}/pending.json"), b"malformed").unwrap();
    assert!(crate::project_writer::WriterLock::mutation(
        root.path(),
        crate::project_writer::MutationFamily::Change
    )
    .is_err());
    assert!(crate::project_writer::WriterLock::mutation(
        root.path(),
        crate::project_writer::MutationFamily::Ownership
    )
    .is_ok());
}

#[test]
fn owned_inputs_refuse_hardlink_aliases() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("source"), b"external").unwrap();
    std::fs::hard_link(outside.path().join("source"), root.path().join("linked")).unwrap();
    assert!(read(root.path(), "linked").is_err());
    assert_eq!(
        std::fs::read(outside.path().join("source")).unwrap(),
        b"external"
    );
}

#[test]
fn old_verification_hosts_never_fall_back_to_writable_execution() {
    struct OldHost;
    impl VerificationHost for OldHost {
        fn resolve_program(&self, _: &str) -> Option<std::path::PathBuf> {
            None
        }
        fn run(&self, _: &Path, _: &[String], _: &Path) -> Result<(Option<i32>, String), String> {
            panic!("writable fallback")
        }
    }
    assert!(OldHost
        .run_immutable(Path::new("node"), &[], Path::new("."))
        .is_err());
}

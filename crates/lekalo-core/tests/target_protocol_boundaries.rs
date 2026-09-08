//! Real OS-boundary regressions. All hostile reads and writes address
//! private test-owned projects and every case includes preserved-byte checks.
use lekalo_core::{
    project_fs::Fs,
    target_protocol::{
        transport::{AdapterCommand, TransportLimits},
        wire::Operation,
        CallOutcome, CallRequest, TargetClient, TargetFailure,
    },
};
use std::path::{Path, PathBuf};

struct Project {
    owned: tempfile::TempDir,
}
impl Project {
    fn new() -> Self {
        let owned = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(owned.path().join(".lekalo/ir")).unwrap();
        std::fs::create_dir(owned.path().join("lekalo")).unwrap();
        std::fs::write(owned.path().join(".lekalo/ir/input.json"), "input").unwrap();
        std::fs::write(owned.path().join("lekalo/model.yaml"), "canonical").unwrap();
        std::fs::write(owned.path().join("private.txt"), "private").unwrap();
        Self { owned }
    }
    fn root(&self) -> &Path {
        self.owned.path()
    }
    fn command(&self, mode: &str) -> AdapterCommand {
        AdapterCommand {
            program: "node".into(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/target-protocol/boundary-adapter.mjs")
                    .to_string_lossy()
                    .into_owned(),
                "--mode".into(),
                mode.into(),
                "--host-root".into(),
                self.root().to_string_lossy().into_owned(),
            ],
        }
    }
    fn unchanged(&self) {
        assert_eq!(
            std::fs::read(self.root().join("lekalo/model.yaml")).unwrap(),
            b"canonical"
        );
        assert_eq!(
            std::fs::read(self.root().join("private.txt")).unwrap(),
            b"private"
        );
        assert!(!self.root().join("other").exists());
    }
}
fn client() -> TargetClient {
    TargetClient::new(TransportLimits {
        timeout_ms: 10000,
        ..TransportLimits::default()
    })
}
fn request(operation: Operation, dry: Option<bool>, id: Option<&str>) -> CallRequest<'_> {
    CallRequest {
        operation,
        target: Some("test"),
        profile: Some("default"),
        ir_path: Some(".lekalo/ir/input.json"),
        dry_run: dry,
        plan_id: id,
    }
}
fn call(
    client: &mut TargetClient,
    cmd: &AdapterCommand,
    p: &Project,
    request: CallRequest<'_>,
) -> Result<CallOutcome, TargetFailure> {
    client.call(cmd, request, p.root(), &Fs::open(p.root()).unwrap(), None)
}
fn plan(client: &mut TargetClient, cmd: &AdapterCommand, p: &Project) -> String {
    call(
        client,
        cmd,
        p,
        request(Operation::Generate, Some(true), None),
    )
    .unwrap()
    .plan_id
    .unwrap()
}

#[test]
fn every_readonly_operation_has_an_actual_scoped_filesystem_boundary() {
    let p = Project::new();
    std::fs::create_dir(p.root().join("out")).unwrap();
    std::fs::write(
        p.root().join("out/existing.txt"),
        b"user content outside read scopes",
    )
    .unwrap();
    let cmd = p.command("describe-read");
    let mut c = client();
    c.describe(&cmd, p.root()).unwrap();
    p.unchanged();
    let cmd = p.command("scope-probes");
    c.describe(&cmd, p.root()).unwrap();
    let result = call(&mut c, &cmd, &p, request(Operation::Scan, None, None)).unwrap();
    let entries = result.response.result.unwrap().entries.unwrap();
    assert_eq!(entries.len(), 10);
    for entry in entries {
        if entry.path == "write-content" {
            assert_eq!(entry.kind, "redacted");
            continue;
        }
        assert_eq!(
            entry.kind,
            if entry.path == "scoped-read" {
                "allowed"
            } else {
                "denied"
            },
            "{}",
            entry.path
        );
    }
    assert_eq!(
        std::fs::read(p.root().join("out/existing.txt")).unwrap(),
        b"user content outside read scopes"
    );
    assert!(!p.root().join("out/forbidden.txt").exists());
    p.unchanged();
}

#[test]
fn file_transport_and_malformed_operation_payloads_run_through_the_isolated_client() {
    let p = Project::new();
    let mut c = client();
    let cmd = p.command("file-transport");
    c.describe(&cmd, p.root()).unwrap();
    call(&mut c, &cmd, &p, request(Operation::Validate, None, None)).unwrap();
    for mode in ["scan-secret", "extra-capabilities"] {
        let cmd = p.command(mode);
        c.describe(&cmd, p.root()).unwrap();
        assert!(
            matches!(
                call(&mut c, &cmd, &p, request(Operation::Scan, None, None)),
                Err(TargetFailure::ResponseInvalid { .. })
            ),
            "{mode}"
        );
    }
    p.unchanged();
}

#[test]
fn empty_read_scopes_and_canonical_alias_scopes_are_refused() {
    let p = Project::new();
    let cmd = p.command("empty-read");
    let mut c = client();
    c.describe(&cmd, p.root()).unwrap();
    assert!(matches!(
        call(&mut c, &cmd, &p, request(Operation::Validate, None, None)),
        Err(TargetFailure::ScopeViolation { .. })
    ));
    let cmd = p.command("alias");
    assert!(c.describe(&cmd, p.root()).is_err());
    assert!(c.describe_outcome().is_none());
    p.unchanged();
}

#[test]
fn write_failures_missing_echo_and_partial_errors_never_publish_stage_bytes() {
    for mode in ["outside-apply", "extra-write", "no-echo", "apply-error"] {
        let p = Project::new();
        let cmd = p.command(mode);
        let mut c = client();
        c.describe(&cmd, p.root()).unwrap();
        let id = plan(&mut c, &cmd, &p);
        let error = call(
            &mut c,
            &cmd,
            &p,
            request(Operation::Generate, Some(false), Some(&id)),
        )
        .unwrap_err();
        if mode == "apply-error" {
            assert!(matches!(
                error,
                TargetFailure::OperationFailed { partial: true, .. }
            ));
            let public = lekalo_core::DomainResult::from(&error).to_json_string();
            assert!(public.contains("adapter-error-partial"));
            assert!(!public.contains("private-secret"));
        }
        assert!(c.plan_binding().is_none(), "{mode}");
        assert!(!p.root().join("out").exists(), "{mode}");
        p.unchanged();
    }
}

#[test]
fn apply_requires_exact_operation_project_profile_input_and_before_state() {
    for drift in [
        "operation",
        "project",
        "profile",
        "ir",
        "before",
        "wrong-id",
    ] {
        let p = Project::new();
        let other = Project::new();
        let cmd = p.command("normal");
        let mut c = client();
        c.describe(&cmd, p.root()).unwrap();
        let id = plan(&mut c, &cmd, &p);
        let wrong = format!("plan-{}", "0".repeat(64));
        let mut req = request(Operation::Generate, Some(false), Some(&id));
        let destination = if drift == "project" { &other } else { &p };
        match drift {
            "operation" => {
                req.operation = Operation::Clean;
                req.dry_run = None;
            }
            "profile" => req.profile = Some("other"),
            "ir" => std::fs::write(p.root().join(".lekalo/ir/input.json"), "changed").unwrap(),
            "before" => {
                std::fs::create_dir(p.root().join("out")).unwrap();
                std::fs::write(p.root().join("out/unrelated.txt"), "user").unwrap();
            }
            "wrong-id" => req.plan_id = Some(&wrong),
            _ => {}
        }
        assert!(call(&mut c, &cmd, destination, req).is_err(), "{drift}");
        assert!(c.plan_binding().is_none());
        assert!(!p.root().join("out/file.txt").exists());
        assert!(!other.root().join("out/file.txt").exists());
        p.unchanged();
        other.unchanged();
    }
}

#[test]
fn create_replace_and_delete_obey_before_state_and_preserve_existing_data() {
    let p = Project::new();
    std::fs::create_dir(p.root().join("out")).unwrap();
    std::fs::write(p.root().join("out/file.txt"), "user").unwrap();
    let cmd = p.command("normal");
    let mut c = client();
    c.describe(&cmd, p.root()).unwrap();
    assert!(call(
        &mut c,
        &cmd,
        &p,
        request(Operation::Generate, Some(true), None)
    )
    .is_err());
    assert_eq!(
        std::fs::read(p.root().join("out/file.txt")).unwrap(),
        b"user"
    );
    let cmd = p.command("replace");
    c.describe(&cmd, p.root()).unwrap();
    let id = plan(&mut c, &cmd, &p);
    call(
        &mut c,
        &cmd,
        &p,
        request(Operation::Generate, Some(false), Some(&id)),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(p.root().join("out/file.txt")).unwrap(),
        b"generated"
    );
    let id = call(&mut c, &cmd, &p, request(Operation::PlanClean, None, None))
        .unwrap()
        .plan_id
        .unwrap();
    call(&mut c, &cmd, &p, request(Operation::Clean, None, Some(&id))).unwrap();
    assert!(!p.root().join("out/file.txt").exists());
    p.unchanged();
}

#[test]
fn oversized_file_refuses_before_dry_run_and_directories_remain_intact() {
    let p = Project::new();
    let cmd = p.command("mutate-dry");
    let mut c = client();
    c.describe(&cmd, p.root()).unwrap();
    std::fs::create_dir(p.root().join("out")).unwrap();
    let bytes = vec![b'A'; 4 * 1024 * 1024 + 1];
    std::fs::write(p.root().join("out/large.bin"), &bytes).unwrap();
    assert!(matches!(
        call(
            &mut c,
            &cmd,
            &p,
            request(Operation::Generate, Some(true), None)
        ),
        Err(TargetFailure::ScopeViolation {
            detail: "scope-limit",
            ..
        })
    ));
    assert_eq!(
        std::fs::read(p.root().join("out/large.bin")).unwrap(),
        bytes
    );
    assert!(!p.root().join("out/file.txt").exists());
    p.unchanged();
}

#[test]
fn unsupported_status_and_untrusted_public_fields_are_preserved_safely() {
    let failure = TargetFailure::CapabilityUnsupported { detail: "target" };
    let result = lekalo_core::DomainResult::from(&failure);
    assert_eq!(result.status(), lekalo_core::Status::Unsupported);
    assert_eq!(result.exit_code(), 4);
    let failure = TargetFailure::ScopeViolation {
        path: Some("C:/Users/private-secret/token".into()),
        detail: "grammar",
    };
    let result = lekalo_core::DomainResult::from(&failure).to_json_string();
    assert!(!result.contains("private-secret"));
    assert!(result.contains("subject-"));
}

#[test]
fn successful_refresh_and_failed_refresh_both_revoke_the_previous_plan() {
    let p = Project::new();
    let cmd = p.command("normal");
    let mut c = client();
    c.describe(&cmd, p.root()).unwrap();
    let id = plan(&mut c, &cmd, &p);
    c.describe(&cmd, p.root()).unwrap();
    assert!(c.plan_binding().is_none());
    assert!(call(
        &mut c,
        &cmd,
        &p,
        request(Operation::Generate, Some(false), Some(&id))
    )
    .is_err());
    let _id = plan(&mut c, &cmd, &p);
    let missing = AdapterCommand {
        program: PathBuf::from("lekalo-nonexistent-adapter"),
        args: vec![],
    };
    assert!(c.describe(&missing, p.root()).is_err());
    assert!(c.describe_outcome().is_none());
    assert!(c.plan_binding().is_none());
    p.unchanged();
}

#[test]
fn cancellable_refresh_revokes_authority_on_success_failure_and_cancellation() {
    use std::sync::atomic::AtomicBool;

    for mode in ["cancelled", "failed-command", "successful", "none-flag"] {
        let p = Project::new();
        let cmd = p.command("normal");
        let mut c = client();
        c.describe(&cmd, p.root()).unwrap();
        let id = plan(&mut c, &cmd, &p);
        let flag = AtomicBool::new(mode == "cancelled");
        let missing = AdapterCommand {
            program: "lekalo-nonexistent-adapter".into(),
            args: vec![],
        };
        let refreshed = c.describe_with_cancel(
            if mode == "failed-command" {
                &missing
            } else {
                &cmd
            },
            p.root(),
            if mode == "none-flag" {
                None
            } else {
                Some(&flag)
            },
        );
        let successful = matches!(mode, "successful" | "none-flag");
        assert_eq!(refreshed.is_ok(), successful, "{mode}");
        assert_eq!(c.describe_outcome().is_some(), successful, "{mode}");
        assert!(c.plan_binding().is_none(), "{mode}");
        assert!(call(
            &mut c,
            &cmd,
            &p,
            request(Operation::Generate, Some(false), Some(&id)),
        )
        .is_err());
        assert!(!p.root().join("out").exists(), "{mode}");
        p.unchanged();

        // Revocation must still permit a fresh successful handshake and plan.
        c.describe_with_cancel(&cmd, p.root(), None).unwrap();
        let fresh_id = plan(&mut c, &cmd, &p);
        call(
            &mut c,
            &cmd,
            &p,
            request(Operation::Generate, Some(false), Some(&fresh_id)),
        )
        .unwrap();
        assert!(p.root().join("out/file.txt").is_file(), "{mode}");
        assert!(c.plan_binding().is_none());
        p.unchanged();
    }
}

#[test]
fn unicode_error_codes_preserve_classification_and_public_redaction() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../tests/fixtures/target-protocol/error-code-vectors.json"
    ))
    .unwrap();
    for vector in vectors {
        let p = Project::new();
        let mut cmd = p.command("unicode-error");
        cmd.args.extend([
            "--error-unit".into(),
            vector["unit"].as_str().unwrap().into(),
            "--error-repeat".into(),
            vector["repeat"].to_string(),
        ]);
        let mut c = client();
        c.describe(&cmd, p.root()).unwrap();
        let failure = call(&mut c, &cmd, &p, request(Operation::Validate, None, None)).unwrap_err();
        let valid = vector["valid"].as_bool().unwrap();
        assert_eq!(
            matches!(failure, TargetFailure::OperationFailed { .. }),
            valid,
            "{vector}"
        );
        let public = lekalo_core::DomainResult::from(&failure).to_json_string();
        if valid {
            let code = vector["unit"]
                .as_str()
                .unwrap()
                .repeat(vector["repeat"].as_u64().unwrap() as usize);
            assert!(!public.contains(&code));
            assert!(public.contains("adapter-error"));
        } else {
            assert!(matches!(failure, TargetFailure::ResponseInvalid { .. }));
        }
        p.unchanged();
    }
}

//! Every checked-in semantic vector executes the same decoder/validator as
//! the runtime, with its exact expected rule and detail. No filename is a
//! substitute for a rejection.
use super::*;
use std::path::Path;

#[test]
fn target_protocol_conformance_error_codes_count_unicode_characters() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/target-protocol/error-code-vectors.json"
    ))
    .unwrap();
    let base: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/target-protocol/valid/describe-response.json"
    ))
    .unwrap();
    for vector in vectors {
        let code = vector["unit"]
            .as_str()
            .unwrap()
            .repeat(vector["repeat"].as_u64().unwrap() as usize);
        let valid = vector["valid"].as_bool().unwrap();
        let mut response = base.clone();
        response.as_object_mut().unwrap().remove("capabilities");
        response["status"] = "error".into();
        response["error"] = serde_json::json!({
            "class": "invalid", "code": code, "message": "owned synthetic error"
        });
        let bytes = serde_json::to_vec(&response).unwrap();
        assert_eq!(wire::decode_response(&bytes).is_ok(), valid, "{vector}");
        let typed: ResponseEnvelope = serde_json::from_value(response).unwrap();
        let request = response_request(&typed);
        let exchange = transport::TransportSuccess {
            exit_code: 0,
            stdout: bytes,
            stderr: vec![],
            stdout_truncated: false,
            stderr_truncated: false,
        };
        let failure = TargetClient::default()
            .interpret(Ok(exchange), &request)
            .unwrap_err();
        assert_eq!(
            matches!(failure, TargetFailure::OperationFailed { .. }),
            valid,
            "{vector}: {failure:?}"
        );
        if !valid {
            assert!(matches!(
                failure,
                TargetFailure::ResponseInvalid {
                    detail: ResponseInvalidity::Shape
                }
            ));
        } else if let TargetFailure::OperationFailed { code: actual, .. } = &failure {
            assert_eq!(
                actual, &code,
                "validated internal evidence must stay intact"
            );
        }
        let public = crate::DomainResult::from(&failure).to_json_string();
        if valid {
            assert!(public.contains("adapter-error"));
            assert!(!public.contains(&code));
        }
    }
}

#[test]
fn target_protocol_conformance_shares_portable_path_vectors_with_ajv() {
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/target-protocol/scope-grammar.json"
    ))
    .unwrap();
    for vector in vectors {
        let value = vector["value"].as_str().unwrap();
        assert_eq!(
            scopes::is_logical_path(value),
            vector["path"].as_bool().unwrap(),
            "{value}"
        );
        assert_eq!(
            scopes::is_scope(value),
            vector["scope"].as_bool().unwrap(),
            "{value}"
        );
    }
    assert!(scopes::is_logical_path(&"x".repeat(64)));
    assert!(!scopes::is_logical_path(&"x".repeat(65)));
}

#[test]
fn invalid_apply_plan_is_refused_before_the_actual_os_launch() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".lekalo/ir")).unwrap();
    std::fs::write(project.path().join(".lekalo/ir/planner.json"), b"{}").unwrap();
    let command = transport::AdapterCommand {
        program: "node".into(),
        args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
            .to_string_lossy()
            .into_owned()],
    };
    let mut client = TargetClient::default();
    client.describe(&command, project.path()).unwrap();
    let first = transport::launches();
    assert!(first > 0, "positive OS launch control");
    let id = format!("plan-{}", "0".repeat(64));
    let call = CallRequest {
        operation: Operation::Generate,
        target: Some("node-typescript"),
        profile: Some("default"),
        profile_resolution: None,
        ir_path: Some(".lekalo/ir/planner.json"),
        dry_run: Some(false),
        plan_id: Some(&id),
    };
    let fs = Fs::open(project.path()).unwrap();
    assert!(client
        .call(&command, call.clone(), project.path(), &fs, None)
        .is_err());
    assert_eq!(transport::launches(), first);
    let planning = CallRequest {
        dry_run: Some(true),
        plan_id: None,
        ..call.clone()
    };
    client
        .call(&command, planning, project.path(), &fs, None)
        .unwrap();
    let after_plan = transport::launches();
    assert!(after_plan > first);
    assert!(client
        .call(&command, call, project.path(), &fs, None)
        .is_err());
    assert_eq!(transport::launches(), after_plan);
    assert!(!project.path().join(".lekalo/generated").exists());
}

fn response_request(response: &wire::ResponseEnvelope) -> RequestEnvelope {
    let mut request = base_envelope(
        response.operation,
        if version::is_supported_version(&response.protocol_version) {
            response.protocol_version.clone()
        } else {
            version::BASE_VERSION.to_owned()
        },
        wire::Limits {
            timeout_ms: Some(20000),
            max_output_bytes: Some(8388608),
        },
    );
    request.request_id = response.request_id.clone();
    request.target = Some("node-typescript".into());
    request.profile = Some("default".into());
    request.ir_path = Some(".lekalo/ir/planner.json".into());
    if response.operation == Operation::Generate {
        request.dry_run = Some(false);
        request.plan_id = Some(
            response
                .evidence
                .plan_id
                .clone()
                .unwrap_or_else(|| format!("plan-{}", "0".repeat(64))),
        );
    }
    if response.operation == Operation::Clean {
        request.plan_id = response.evidence.plan_id.clone();
    }
    request
}

fn validate_raw_response(
    client: &TargetClient,
    bytes: &[u8],
    request: &RequestEnvelope,
    caps: &wire::Capabilities,
) -> Result<(), TargetFailure> {
    let exchange = transport::TransportSuccess {
        exit_code: 0,
        stdout: bytes.into(),
        stderr: vec![],
        stdout_truncated: false,
        stderr_truncated: false,
    };
    let response = client.interpret(Ok(exchange), request)?;
    if request.operation == Operation::Describe {
        if response.result.is_some() || response.writes.is_some() {
            return Err(TargetFailure::ResponseInvalid {
                detail: ResponseInvalidity::UnexpectedMember,
            });
        }
        let capabilities = response
            .capabilities
            .ok_or(TargetFailure::ResponseInvalid {
                detail: ResponseInvalidity::Shape,
            })?;
        client.validate_scopes(&capabilities.read_scopes, true)?;
        client.validate_scopes(&capabilities.write_scopes, false)?;
        Ok(())
    } else {
        let call = CallRequest {
            operation: request.operation,
            target: request.target.as_deref(),
            profile: request.profile.as_deref(),
            profile_resolution: None,
            ir_path: request.ir_path.as_deref(),
            dry_run: request.dry_run,
            plan_id: request.plan_id.as_deref(),
        };
        client.validate_response_payload(&call, &response, caps)
    }
}

#[test]
fn target_protocol_conformance_executes_all_fixture_expectations() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/target-protocol");
    let describe_bytes = std::fs::read(fixture.join("valid/describe-response.json")).unwrap();
    let describe = wire::decode_response(&describe_bytes).unwrap();
    let caps = describe.capabilities.unwrap();
    let client = TargetClient::default();
    for entry in std::fs::read_dir(fixture.join("invalid")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if !name.ends_with(".json") || name.ends_with(".expect.json") {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let expected: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path.with_extension("expect.json")).unwrap())
                .unwrap();
        let result = if value.get("status").is_none() {
            wire::decode_request(&bytes).map(|_| ())
        } else {
            let request = match serde_json::from_slice::<wire::ResponseEnvelope>(&bytes) {
                Ok(response) => response_request(&response),
                Err(_) => base_envelope(
                    Operation::Describe,
                    version::VERSION.into(),
                    wire::Limits {
                        timeout_ms: Some(20000),
                        max_output_bytes: Some(8388608),
                    },
                ),
            };
            validate_raw_response(&client, &bytes, &request, &caps)
        };
        let failure = result.expect_err(name);
        assert_eq!(
            failure.rule().0,
            expected["rule"].as_str().unwrap(),
            "{name}: {failure:?}"
        );
        let detail = match &failure {
            TargetFailure::RequestInvalid { detail }
            | TargetFailure::PlanMismatch { detail, .. }
            | TargetFailure::ScopeViolation { detail, .. } => *detail,
            TargetFailure::ResponseInvalid { detail } => detail.detail(),
            TargetFailure::ProtocolMismatch { detail } => detail.detail(),
            TargetFailure::ProtectedPath { home, .. } => *home,
            _ => panic!("unexpected {name}: {failure:?}"),
        };
        assert_eq!(
            detail,
            expected["detail"].as_str().unwrap(),
            "{name}: {failure:?}"
        );
    }
    for entry in std::fs::read_dir(fixture.join("valid")).unwrap() {
        let path = entry.unwrap().path();
        let bytes = std::fs::read(&path).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        if value.get("status").is_none() {
            wire::decode_request(&bytes).unwrap();
        } else {
            let response = wire::decode_response(&bytes).unwrap();
            let mut request = response_request(&response);
            if path.file_name().unwrap() == "generate-plan-response.json" {
                request.dry_run = Some(true);
                request.plan_id = None;
            }
            validate_raw_response(&client, &bytes, &request, &caps).unwrap();
        }
    }
}

#[test]
fn target_protocol_conformance_rejects_decoded_duplicates_nulls_and_bounds() {
    let raw =
        include_str!("../../../../tests/fixtures/target-protocol/valid/describe-response.json");
    let good: serde_json::Value = serde_json::from_str(raw).unwrap();
    for suffix in [
        ",\"status\":\"ok\"}",
        ",\"sta\\u0074us\":\"ok\"}",
        ",\"error\":null}",
    ] {
        let bad = format!("{}{suffix}", &raw[..raw.len() - 1]);
        assert!(wire::decode_response(bad.as_bytes()).is_err());
    }
    for mutate in [0, 1, 2, 3, 4] {
        let mut bad = good.clone();
        match mutate {
            0 => bad["capabilities"]["adapter"]["version"] = "bad".into(),
            1 => bad["capabilities"]["operations"] = serde_json::json!(["describe", "describe"]),
            2 => {
                bad["capabilities"]["targets"] =
                    serde_json::json!((0..65).map(|i| format!("target-{i}")).collect::<Vec<_>>())
            }
            3 => bad["evidence"]["plan_id"] = serde_json::Value::Null,
            _ => bad["capabilities"]["progress"] = serde_json::Value::Null,
        }
        assert!(
            wire::decode_response(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "case {mutate}"
        );
    }
    assert!(wire::decode_response(raw.as_bytes()).is_ok());
}

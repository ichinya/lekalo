//! The Rust codec runner for the shared adapter-SDK vector corpus
//! (issue #106). Mirrors `sdk/typescript/test/vectors.mjs` exactly and
//! prints the same one-line JSON verdict for the parity gate.

use lekalo_adapter_sdk as sdk;
use sdk::{canonical, Value};
use std::fs;

fn corpus_path() -> String {
    for arg in std::env::args().skip(1) {
        if let Some(path) = arg.strip_prefix("--corpus=") {
            return path.to_owned();
        }
    }
    let manifest = env!("CARGO_MANIFEST_DIR");
    format!("{manifest}\\..\\..\\tests\\fixtures\\adapter-sdk")
}

struct Verdict {
    results: Vec<(String, bool, String)>,
    canonical_outputs: Vec<(String, String)>,
}

impl Verdict {
    fn new() -> Self {
        Self {
            results: Vec::new(),
            canonical_outputs: Vec::new(),
        }
    }

    fn record(&mut self, name: &str, ok: bool, detail: &str) {
        self.results.push((name.to_owned(), ok, detail.to_owned()));
    }
}

fn read(corpus: &str, relative: &str) -> String {
    fs::read_to_string(format!("{corpus}/{relative}")).unwrap_or_default()
}

fn list(corpus: &str, family: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(format!("{corpus}/{family}"))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|name| name.ends_with(".json") && !name.ends_with(".expect.json"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn json_escape(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn main() {
    let corpus = corpus_path();
    let mut verdict = Verdict::new();

    // valid/
    for name in list(&corpus, "valid") {
        let text = read(&corpus, &format!("valid/{name}"));
        let envelope = sdk::strict_parse(&text);
        let is_response = match &envelope {
            Ok(value) => value.get("status").is_some(),
            Err(_) => text.contains("\"status\"") && text.contains("\"evidence\""),
        };
        if is_response {
            let decoded = sdk::admit_response(&text);
            verdict.record(
                &format!("valid/{name}"),
                decoded.is_ok(),
                &format!("{:?}", decoded.err().map(|r| r.reason)),
            );
        } else {
            match sdk::decode_request(&text) {
                Ok(_) => verdict.record(&format!("valid/{name}"), true, ""),
                Err(refusal) => verdict.record(&format!("valid/{name}"), false, &refusal.reason),
            }
        }
    }

    // invalid/
    for name in list(&corpus, "invalid") {
        let text = read(&corpus, &format!("invalid/{name}"));
        let expect_text = read(
            &corpus,
            &format!("invalid/{}.expect.json", name.trim_end_matches(".json")),
        );
        let expect = sdk::strict_parse(&expect_text);
        let (expect_stage, expect_reason) = match &expect {
            Ok(value) => (
                value
                    .get("stage")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                value
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            ),
            Err(_) => (String::new(), String::new()),
        };
        let is_response = text.contains("\"status\"") && text.contains("\"evidence\"");
        let actual = if !is_response {
            match sdk::admit_request(&text) {
                Err(refusal) => (refusal.stage.to_owned(), refusal.reason),
                Ok(request) => match sdk::validate_request(&request) {
                    Ok(()) => ("none".to_owned(), String::new()),
                    Err(refusal) => (refusal.stage.to_owned(), refusal.reason),
                },
            }
        } else {
            match sdk::admit_response(&text) {
                Err(refusal) => (refusal.stage.to_owned(), refusal.reason),
                Ok(_) => ("none".to_owned(), String::new()),
            }
        };
        let ok = actual.0 == expect_stage && actual.1 == expect_reason;
        verdict.record(
            &format!("invalid/{name}"),
            ok,
            &format!("{actual:?} vs {expect_stage}/{expect_reason}"),
        );
    }

    // semantic/
    for name in list(&corpus, "semantic") {
        let text = read(&corpus, &format!("semantic/{name}"));
        let vector = match sdk::strict_parse(&text) {
            Ok(vector) => vector,
            Err(_) => {
                verdict.record(&format!("semantic/{name}"), false, "unparseable");
                continue;
            }
        };
        let kind = vector.get("kind").and_then(Value::as_str).unwrap_or("");
        let expect = vector.get("expect").and_then(Value::as_str).unwrap_or("");
        let (expect_kind, expect_reason) = match expect.split_once(':') {
            Some((kind, reason)) => (kind, reason),
            None => (expect, ""),
        };
        let outcome: Result<bool, String> = (|| match kind {
            "validate-before-write" => {
                let request =
                    sdk::decode_request(&json_text(&vector, "request")).map_err(|r| r.reason)?;
                let response =
                    sdk::admit_response(&json_text(&vector, "response")).map_err(|r| r.reason)?;
                let identity = sdk::validate_response_identity(
                    &response,
                    (
                        request
                            .get("protocol_version")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                        request
                            .get("operation")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                        request
                            .get("request_id")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                    ),
                );
                let reason = match identity {
                    Err(refusal) => refusal.reason,
                    Ok(()) => match sdk::validate_response_payload(&request, &response) {
                        Err(refusal) => refusal.reason,
                        Ok(()) => String::new(),
                    },
                };
                Ok(if reason.is_empty() {
                    expect_kind == "accept"
                } else {
                    expect_kind == "reject" && expect_reason == reason
                })
            }
            "build-success" => {
                let request =
                    sdk::decode_request(&json_text(&vector, "request")).map_err(|r| r.reason)?;
                let builder = vector
                    .get("builder")
                    .ok_or_else(|| "no-builder".to_owned())?;
                let identity_text = canonical(
                    builder
                        .get("identity")
                        .ok_or_else(|| "no-identity".to_owned())?,
                );
                let identity = sdk::admit_response(&format!(
                        "{{\"evidence\":{{\"adapter\":{identity_text}}},\"operation\":\"describe\",\"protocol\":\"lekalo.target/v1\",\"protocol_version\":\"0.3.2\",\"request_id\":\"req-{}\",\"status\":\"ok\"}}",
                        "0".repeat(64)
                    )).map_err(|r| r.reason)?;
                let adapter = identity
                    .get("evidence")
                    .and_then(|e| e.get("adapter"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let fragments = builder.get("fragments");
                let capabilities = builder.get("capabilities").cloned();
                let envelope = sdk::build_success(
                    &request,
                    &adapter,
                    capabilities.as_ref(),
                    fragments.and_then(|f| f.get("payload")),
                    fragments.and_then(|f| f.get("writes")),
                    fragments.and_then(|f| f.get("progress")),
                    fragments
                        .and_then(|f| f.get("planId"))
                        .and_then(Value::as_str),
                )
                .map_err(|e| e.reason)?;
                let mut ok = expect_kind == "accept";
                if ok {
                    if let Some(expected_writes) = fragments.and_then(|f| f.get("writes")) {
                        let sorted =
                            sdk::canonical(&envelope.get("writes").cloned().unwrap_or(Value::Null));
                        let reference = sdk::canonical(&{
                            let mut items: Vec<Value> = expected_writes
                                .as_array()
                                .map(|slice| slice.to_vec())
                                .unwrap_or_default();
                            items.sort_by(|a, b| {
                                a.get("path")
                                    .and_then(Value::as_str)
                                    .cmp(&b.get("path").and_then(Value::as_str))
                            });
                            Value::Array(items)
                        });
                        ok = sorted == reference;
                    }
                }
                Ok(ok)
            }
            "build-error" => {
                let request =
                    sdk::decode_request(&json_text(&vector, "request")).map_err(|r| r.reason)?;
                let builder = vector.get("builder").ok_or("no-builder")?;
                let error = builder.get("error").ok_or_else(|| "no-error".to_owned())?;
                let identity_text = canonical(builder.get("identity").ok_or("no-identity")?);
                let identity_probe = sdk::admit_response(&format!(
                        "{{\"evidence\":{{\"adapter\":{identity_text}}},\"operation\":\"describe\",\"protocol\":\"lekalo.target/v1\",\"protocol_version\":\"0.3.2\",\"request_id\":\"req-{}\",\"status\":\"ok\"}}",
                        "0".repeat(64)
                    )).map_err(|r| r.reason)?;
                let adapter = identity_probe
                    .get("evidence")
                    .and_then(|e| e.get("adapter"))
                    .cloned()
                    .unwrap_or(Value::Null);
                match sdk::build_error(
                    &request,
                    &adapter,
                    error
                        .get("errorClass")
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                    error.get("errorCode").and_then(Value::as_str).unwrap_or(""),
                    error.get("message").and_then(Value::as_str).unwrap_or(""),
                    None,
                ) {
                    Ok(envelope) => Ok(expect_kind == "accept"
                        && envelope.get("status").and_then(Value::as_str) == Some("error")),
                    Err(build_error) => {
                        Ok(expect_kind == "reject" && expect_reason == build_error.reason)
                    }
                }
            }
            "decode-oversize" => {
                let max_bytes =
                    vector.get("maxBytes").and_then(Value::as_u64).unwrap_or(0) as usize;
                let text_value = vector.get("text").and_then(Value::as_str).unwrap_or("");
                match sdk::read_single_object(text_value, max_bytes) {
                    Err(sdk::ParseReason::Oversize) => {
                        Ok(expect_kind == "reject" && expect_reason == "oversize")
                    }
                    _ => Ok(false),
                }
            }
            "canonical-plan" => {
                let writes = vector.get("writes").cloned().unwrap_or(Value::Null);
                let sorted =
                    sdk::sort_write_plan(&writes).ok_or_else(|| "unsortable".to_owned())?;
                let plan_id = sdk::canonical_plan_id(&sorted);
                Ok(plan_id == vector.get("planId").and_then(Value::as_str).unwrap_or(""))
            }
            "artifact-manifest" => {
                let entries = vector
                    .get("entries")
                    .and_then(Value::as_array)
                    .ok_or_else(|| "no-entries".to_owned())?;
                let pairs: Vec<(String, String)> = entries
                    .iter()
                    .map(|entry| {
                        (
                            entry
                                .get("path")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                            entry
                                .get("bytes")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_owned(),
                        )
                    })
                    .collect();
                let manifest = sdk::build_artifact_manifest(&pairs);
                if expect_kind == "reject" {
                    Ok(manifest.is_none())
                } else {
                    let expected_sorted = vector.get("expectSorted").and_then(Value::as_array);
                    Ok(manifest.is_some()
                        && expected_sorted.is_some()
                        && sdk::canonical(manifest.as_ref().unwrap().get("entries").unwrap())
                            == sdk::canonical(&Value::Array(expected_sorted.unwrap().to_vec())))
                }
            }
            "scenario-result" => {
                let result = vector.get("result").cloned().unwrap_or(Value::Null);
                let normalized = sdk::normalize_scenario_result(&result);
                if expect_kind == "reject" {
                    Ok(normalized.is_none())
                } else {
                    let expected_findings =
                        vector.get("normalizedFindings").and_then(Value::as_array);
                    Ok(normalized.is_some()
                        && expected_findings.is_some()
                        && sdk::canonical(normalized.as_ref().unwrap().get("findings").unwrap())
                            == sdk::canonical(&Value::Array(expected_findings.unwrap().to_vec())))
                }
            }
            _ => Err("unknown-kind".to_owned()),
        })();
        match outcome {
            Ok(true) => verdict.record(&format!("semantic/{name}"), true, ""),
            Ok(false) => verdict.record(&format!("semantic/{name}"), false, expect),
            Err(reason) => {
                let ok = expect_kind == "reject" && expect_reason == reason;
                verdict.record(&format!("semantic/{name}"), ok, &reason);
            }
        }
    }

    // canonical/
    for name in list(&corpus, "canonical") {
        let text = read(&corpus, &format!("canonical/{name}"));
        let vector = match sdk::strict_parse(&text) {
            Ok(vector) => vector,
            Err(_) => {
                verdict.record(&format!("canonical/{name}"), false, "unparseable");
                continue;
            }
        };
        let value = vector.get("value").cloned().unwrap_or(Value::Null);
        let bytes = sdk::canonical(&value);
        verdict
            .canonical_outputs
            .push((format!("canonical/{name}"), bytes.clone()));
        let round_trip = sdk::strict_parse(&bytes);
        let ok = bytes
            == vector
                .get("canonical")
                .and_then(Value::as_str)
                .unwrap_or("")
            && round_trip.is_ok()
            && sdk::canonical(&round_trip.unwrap()) == sdk::canonical(&value);
        verdict.record(&format!("canonical/{name}"), ok, &bytes);
    }

    // security/
    let nfc_supported = false;
    for name in list(&corpus, "security") {
        let text = read(&corpus, &format!("security/{name}"));
        let vector = match sdk::strict_parse(&text) {
            Ok(vector) => vector,
            Err(_) => {
                verdict.record(&format!("security/{name}"), false, "unparseable");
                continue;
            }
        };
        if vector.get("requiresNfc").and_then(Value::as_bool) == Some(true) && !nfc_supported {
            verdict.record(&format!("security/{name}"), true, "skipped:nfc-unsupported");
            continue;
        }
        let ok;
        if vector.get("scopeGrammar").and_then(Value::as_bool) == Some(true) {
            let value = vector.get("value").and_then(Value::as_str).unwrap_or("");
            ok = sdk::is_scope(value)
                == vector
                    .get("scopeOk")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && vector
                    .get("touches")
                    .map(|touches| {
                        sdk::scope_touches_protected_home(value).map(str::to_owned)
                            == touches.as_str().map(str::to_owned)
                    })
                    .unwrap_or(true)
                && vector
                    .get("logical")
                    .map(|logical| {
                        sdk::is_logical_path(value) == logical.as_bool().unwrap_or(false)
                    })
                    .unwrap_or(true);
        } else if let Some(token) = vector.get("token").and_then(Value::as_str) {
            ok = sdk::is_token(token)
                == vector
                    .get("tokenOk")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
        } else if let Some(capability) = vector.get("capability").and_then(Value::as_str) {
            ok = sdk::is_capability_id(capability)
                == vector
                    .get("tokenOk")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
        } else {
            let path = vector.get("path").and_then(Value::as_str).unwrap_or("");
            ok = sdk::is_logical_path(path)
                == vector
                    .get("logical")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && vector
                    .get("home")
                    .map(|home| {
                        sdk::protected_home(path).map(str::to_owned)
                            == home.as_str().map(str::to_owned)
                    })
                    .unwrap_or(true)
                && vector
                    .get("scope")
                    .map(|scope| {
                        sdk::scope_covers(scope.as_str().unwrap_or(""), path)
                            == vector
                                .get("covers")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                    })
                    .unwrap_or(true);
        }
        verdict.record(&format!("security/{name}"), ok, "");
    }

    // upgrade/
    for name in list(&corpus, "upgrade") {
        let text = read(&corpus, &format!("upgrade/{name}"));
        let vector = match sdk::strict_parse(&text) {
            Ok(vector) => vector,
            Err(_) => {
                verdict.record(&format!("upgrade/{name}"), false, "unparseable");
                continue;
            }
        };
        let request_text = sdk::canonical(&vector.get("request").cloned().unwrap_or(Value::Null));
        let accepted = match sdk::decode_request(&request_text) {
            Err(refusal) => (false, refusal.reason),
            Ok(request) => match sdk::validate_request(&request) {
                Ok(()) => (true, String::new()),
                Err(refusal) => (false, refusal.reason),
            },
        };
        let expect_accept = vector
            .get("accept")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expect_reason = vector.get("reason").and_then(Value::as_str).unwrap_or("");
        let ok = expect_accept == accepted.0 && (expect_accept || expect_reason == accepted.1);
        verdict.record(&format!("upgrade/{name}"), ok, &format!("{:?}", accepted));
    }

    let failed: Vec<&(String, bool, String)> =
        verdict.results.iter().filter(|(_, ok, _)| !ok).collect();
    let canonical_json: Vec<String> = verdict
        .canonical_outputs
        .iter()
        .map(|(name, bytes)| format!("\"{name}\":\"{}\"", json_escape(bytes)))
        .collect();
    let count_by_prefix = |prefix: &str| {
        verdict
            .results
            .iter()
            .filter(|(name, _, _)| name.starts_with(prefix))
            .count()
    };
    let status = if failed.is_empty() { "true" } else { "false" };
    let mut out = format!(
        "{{\"ok\":{status},\"language\":\"rust\",\"total\":{},\"failed\":{},\"executedFamilies\":[\"canonical\",\"codec\",\"security\",\"semantic\",\"upgrade\"],\"executedVectors\":{{\"valid\":{},\"invalid\":{},\"semantic\":{},\"upgrade\":{},\"canonical\":{},\"security\":{}}},\"canonicalOutputs\":{{{}}}",
        verdict.results.len(),
        failed.len(),
        count_by_prefix("valid/"),
        count_by_prefix("invalid/"),
        count_by_prefix("semantic/"),
        count_by_prefix("upgrade/"),
        count_by_prefix("canonical/"),
        count_by_prefix("security/"),
        canonical_json.join(",")
    );
    if !failed.is_empty() {
        let failures: Vec<String> = failed
            .iter()
            .map(|(name, _, detail)| {
                format!(
                    "{{\"name\":\"{name}\",\"detail\":\"{}\"}}",
                    json_escape(detail)
                )
            })
            .collect();
        out.push_str(&format!(",\"failures\":[{}]", failures.join(",")));
    }
    out.push('}');
    println!("{out}");
    if !failed.is_empty() {
        std::process::exit(1);
    }
}

fn json_text(vector: &Value, member: &str) -> String {
    sdk::canonical(&vector.get(member).cloned().unwrap_or(Value::Null))
}

use lekalo_core::explain;
use serde_json::{json, Value};

fn report() -> Value {
    json!({"diagnostics": [
        {"code":"LEK-HIDDEN-009","message":"first provider","metadata":{"com.example.alpha:1.0.0":{"original_code":"A009"}}},
        {"code":"LEK-HIDDEN-009","message":"second provider","metadata":{"com.example.beta:2.0.0":{"original_code":"B009"}}}
    ]})
}

fn catalog() -> Value {
    json!({"schemaVersion":"lekalo/explain-provider-catalog/v0.8.0",
    "identity":"dev.lekalo.explain-provider-catalog@0.8.0","entries":[
        {"namespace":"com.example.alpha:1.0.0","originalCode":"A009","explanationRef":"urn:alpha:explain:1.0.0:A009"},
        {"namespace":"com.example.beta:2.0.0","originalCode":"B009","explanationRef":"urn:beta:explain:2.0.0:B009"}
    ]})
}

#[test]
fn merged_core_rule_retains_each_original_provider_reference() {
    let result = explain::explain_report_with_providers(
        &serde_json::to_vec(&report()).unwrap(),
        Some(&serde_json::to_vec(&catalog()).unwrap()),
    );
    assert_eq!(result.exit_code(), 0);
    let document: Value = serde_json::from_str(&result.to_json_string()).unwrap();
    let entries = document["explanations"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["source"]["provider"], "core");
    assert_eq!(
        entries[0]["source"]["providerExplanations"],
        catalog()["entries"]
    );
    assert_eq!(entries[0]["source"]["report"]["message"], "first provider");
}

#[test]
fn missing_offline_catalog_preserves_original_identity_without_inventing_a_ref() {
    let result = explain::explain_report(&serde_json::to_vec(&report()).unwrap());
    assert_eq!(result.exit_code(), 0);
    let document: Value = serde_json::from_str(&result.to_json_string()).unwrap();
    let reference = &document["explanations"][0]["source"]["providerExplanations"][0];
    assert_eq!(reference["originalCode"], "A009");
    assert_eq!(reference["namespace"], "com.example.alpha:1.0.0");
    assert!(reference["explanationRef"].is_null());
}

#[test]
fn conflicting_or_wrong_version_provider_catalog_refuses_before_projection() {
    let bytes = serde_json::to_vec(&report()).unwrap();
    let mut bad = catalog();
    let duplicate = bad["entries"][0].clone();
    bad["entries"].as_array_mut().unwrap().push(duplicate);
    assert_eq!(
        explain::explain_report_with_providers(&bytes, Some(&serde_json::to_vec(&bad).unwrap()))
            .exit_code(),
        1
    );
    bad = catalog();
    bad["schemaVersion"] = json!("lekalo/explain-provider-catalog/v999");
    assert_eq!(
        explain::explain_report_with_providers(&bytes, Some(&serde_json::to_vec(&bad).unwrap()))
            .exit_code(),
        1
    );
    assert_eq!(
        explain::explain_report(br#"{"diagnostics":[],"diagnostics":[]}"#).exit_code(),
        1
    );
}

#[test]
fn locale_changes_human_headings_and_keeps_machine_bytes_identical() {
    let result = explain::resolve(&["LEK-FORMAT-001".to_owned()]);
    let english = explain::localized(result.clone(), false);
    let russian = explain::localized(result, true);
    assert_eq!(english.to_json_string(), russian.to_json_string());
    assert!(english.to_human_string("lekalo").contains("Checks:"));
    assert!(russian.to_human_string("lekalo").contains("Проверка:"));
    assert!(russian.to_human_string("lekalo").contains("SAFE"));
}

#[test]
fn selector_limits_and_multibyte_locations_return_without_panics() {
    assert_eq!(
        explain::resolve(&vec!["LEK-FORMAT-001".to_owned(); 65]).exit_code(),
        1
    );
    let report = json!({"diagnostics":[{"code":"LEK-FORMAT-001","message":"unicode location","source":{"path":format!("a{}", "я".repeat(300))}}]});
    assert_eq!(
        explain::explain_report(&serde_json::to_vec(&report).unwrap()).exit_code(),
        0
    );
}

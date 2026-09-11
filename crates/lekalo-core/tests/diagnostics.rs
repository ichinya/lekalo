//! Issue #11 core conformance: the embedded registry is validated and
//! closed, wire items serialize in the normative field order, and the
//! envelope keeps status, diagnostics, and derived reason codes in order.

use lekalo_core::diagnostics::registry::{DiagnosticRegistry, REGISTRY_BYTES};
use lekalo_core::diagnostics::types::limits;
use lekalo_core::result::DomainResult;

#[test]
fn embedded_registry_parses_and_is_closed() {
    let registry = DiagnosticRegistry::embedded().expect("embedded registry is valid");
    assert_eq!(registry.registry_version(), "1.22.0");
    assert!(registry.len() >= 100, "the core rule inventory is present");
    // A second parse of the exact bytes yields the same table (pure data).
    let again = DiagnosticRegistry::from_bytes(REGISTRY_BYTES).expect("registry bytes re-validate");
    assert_eq!(again, *registry);
}

#[test]
fn every_subsystem_prefix_is_allocated_with_stable_codes() {
    let registry = DiagnosticRegistry::embedded().expect("embedded registry is valid");
    let mut prefixes = std::collections::BTreeMap::new();
    for entry in registry.entries() {
        let code = entry.code();
        let rest = code.strip_prefix("LEK-").expect("LEK prefix");
        let (prefix, number) = rest.split_once('-').expect("subsystem-number");
        assert_eq!(number.len(), 3, "{code}: three digits");
        *prefixes.entry(prefix.to_owned()).or_insert(0usize) += 1;
    }
    for expected in ["STR", "LOAD", "IR", "VER", "LOCK", "CLI", "DIAG", "ADP"] {
        assert!(prefixes.contains_key(expected), "{expected} allocated");
    }
    // Spot-check identity bindings the docs promise.
    let usage = registry.entry("cli.usage").expect("cli.usage");
    assert_eq!(usage.code(), "LEK-CLI-001");
    assert_eq!(
        usage.default_severity(),
        lekalo_core::diagnostics::Severity::Error
    );
    let capability = registry
        .entry("core.capability-unavailable")
        .expect("capability");
    assert_eq!(
        capability.default_severity(),
        lekalo_core::diagnostics::Severity::Info
    );
    let unsupported = registry
        .entry("versioning.unsupported-version")
        .expect("rule");
    assert!(unsupported.allows_status(lekalo_core::result::Status::UnsupportedVersion));
    assert!(!unsupported.allows_status(lekalo_core::result::Status::Invalid));
}

#[test]
fn defensive_limits_match_the_published_contract() {
    assert_eq!(limits::DIAGNOSTICS_PER_RESULT, 256);
    assert_eq!(limits::RELATED_PER_ITEM, 32);
    assert_eq!(limits::CAUSES_PER_ITEM, 8);
    assert_eq!(limits::FIXES_PER_ITEM, 16);
    assert_eq!(limits::PROVIDER_NAMESPACES_PER_ITEM, 8);
    assert_eq!(limits::DATA_FIELDS_PER_ITEM, 16);
    assert_eq!(limits::LIST_MEMBERS, 64);
    assert_eq!(limits::ORIGINAL_CODE_BYTES, 128);
    assert_eq!(limits::TOKEN_BYTES, 256);
}

#[test]
fn the_failure_envelope_keeps_the_normative_field_order() {
    let result = DomainResult::usage_error();
    let json = result.to_json_string();
    let status = json.find("\"status\"").expect("status first");
    let diagnostics = json.find("\"diagnostics\"").expect("diagnostics second");
    let reason_codes = json.find("\"reasonCodes\"").expect("reasonCodes last");
    assert!(status < diagnostics && diagnostics < reason_codes);
    // The derived reason codes are the unique diagnostic ids in order.
    assert_eq!(result.reason_codes().len(), 1);
    assert_eq!(result.diagnostics().len(), 1);
    assert_eq!(result.exit_code(), 1);
    assert!(result.writes_stderr());
}

#[test]
fn version_results_omit_empty_diagnostic_fields() {
    let result = DomainResult::version("0.2.12");
    let json = result.to_json_string();
    assert_eq!(
        json,
        "{\n  \"status\": \"valid\",\n  \"version\": \"0.2.12\"\n}"
    );
    assert!(!json.contains("diagnostics"));
    assert!(!json.contains("reasonCodes"));
}

/// B2 regression, review instance 2 exactly: `model/v` + 100,000 digits is
/// a well-formed alias that resolves to the registered unsupported-version
/// rule, and the attacker-controlled `data.version` echo passes the
/// bounded-token invariant — control-clean and capped at the 256-byte
/// token bound, so the exit-5 envelope can never scale with the selector.
#[test]
fn hostile_unbounded_alias_echo_is_bounded_in_the_versioning_envelope() {
    use lekalo_core::versioning::migration::VersioningFailure;
    use lekalo_core::versioning::registry::VersionRegistry;
    use lekalo_core::versioning::support::ModelTarget;
    let alias = format!("v{}", "9".repeat(100_000));
    let target = ModelTarget::parse(&alias).expect("well-formed alias");
    let registry = VersionRegistry::embedded().expect("valid embedded registry");
    let Err(error) = target.resolve(registry) else {
        panic!("an unregistered 100,000-digit alias is unsupported");
    };
    let result = DomainResult::from(&VersioningFailure::from_target_error(error));
    let envelope: serde_json::Value =
        serde_json::from_str(&result.to_json_string()).expect("envelope parses");
    assert_eq!(envelope["status"], "unsupported-version");
    assert_eq!(
        envelope["diagnostics"][0]["id"],
        "versioning.unsupported-version"
    );
    let version = envelope["diagnostics"][0]["data"]["version"]
        .as_str()
        .expect("version echoed");
    assert!(
        version.len() <= limits::TOKEN_BYTES,
        "version token is bounded, got {} bytes",
        version.len()
    );
    assert!(!version.chars().any(char::is_control));
}

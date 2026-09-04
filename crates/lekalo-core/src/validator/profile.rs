//! The closed, independently versioned validation profile (issue #12).
//!
//! A profile selects registered semantic rules and, within the narrow
//! governance window, their severities. The contract lives in
//! `contracts/validation-profile.schema.v1.0.0.json`; the two built-ins are
//! embedded from the same files and parsed by the exact same closed parser,
//! so no profile can enter the validator except through this module.
//!
//! Fail-closed rules: unknown rule ids, unsupported profile or registry
//! versions, duplicate or unsorted rule entries, illegal severity
//! overrides, extra fields, and malformed JSON are all rejected. A
//! severity override is legal only when the registry default severity is
//! not `error` and the override is strictly lower than that default; P0
//! blockers are never downgradable.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::diagnostics::registry::DiagnosticRegistry;
use crate::diagnostics::types::Severity;
use crate::diagnostics::version::REGISTRY_VERSION;

/// The exact wire discriminator of the validation profile contract.
pub const PROFILE_SCHEMA_VERSION: &str = "lekalo/validation-profile/v1.0.0";
/// The embedded profile identity.
pub const PROFILE_IDENTITY: &str = "dev.lekalo.validation-profile@1.0.0";
/// The current validation profile contract version.
pub const PROFILE_VERSION: &str = "1.0.0";
/// The exact embedded default profile bytes.
pub const DEFAULT_PROFILE_BYTES: &[u8] =
    include_bytes!("../../../../contracts/validation-profile.default.v1.0.0.json");
/// The exact embedded strict profile bytes.
pub const STRICT_PROFILE_BYTES: &[u8] =
    include_bytes!("../../../../contracts/validation-profile.strict.v1.0.0.json");

/// Why embedded profile bytes could not be trusted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProfileError {
    /// The bytes are not a well-formed profile document.
    Malformed,
    /// The profile names an unsupported contract, registry, or rule identity.
    Unsupported,
    /// The profile violates a closed invariant (order, duplicates, overrides).
    Invariant,
    /// The embedded diagnostic registry itself is unusable.
    Registry,
}

/// One rule selection inside a parsed profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleSelection {
    id: String,
    enabled: bool,
    severity_override: Option<Severity>,
}

impl RuleSelection {
    /// The registered rule id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Whether the rule executes.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The profile-approved severity, when the default is overridden.
    pub fn severity_override(&self) -> Option<Severity> {
        self.severity_override
    }
}

/// A parsed, validated validation profile.
///
/// Fields are private: there is no public constructor and no `Default`, so
/// only embedded or explicitly parsed closed documents can select rules.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationProfile {
    profile_id: String,
    version: String,
    registry_version: String,
    rules: Vec<RuleSelection>,
}

impl ValidationProfile {
    /// The embedded, once-validated default profile.
    ///
    /// A validation failure is a developer fault: the error is cached and
    /// every caller fails closed instead of trusting an unvalidated table.
    pub fn embedded_default() -> Result<&'static Self, ProfileError> {
        static PROFILE: LazyLock<Result<ValidationProfile, ProfileError>> =
            LazyLock::new(|| ValidationProfile::from_bytes(DEFAULT_PROFILE_BYTES));
        PROFILE.as_ref().map_err(Clone::clone)
    }

    /// The embedded, once-validated strict profile.
    pub fn embedded_strict() -> Result<&'static Self, ProfileError> {
        static PROFILE: LazyLock<Result<ValidationProfile, ProfileError>> =
            LazyLock::new(|| ValidationProfile::from_bytes(STRICT_PROFILE_BYTES));
        PROFILE.as_ref().map_err(Clone::clone)
    }

    /// Parse and validate profile bytes against the embedded registry.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProfileError> {
        let wire: ProfileWire =
            serde_json::from_slice(bytes).map_err(|_| ProfileError::Malformed)?;
        if wire.schema_version != PROFILE_SCHEMA_VERSION
            || wire.identity != PROFILE_IDENTITY
            || wire.version != PROFILE_VERSION
            || wire.scope != "project"
        {
            return Err(ProfileError::Unsupported);
        }
        if wire.diagnostic_registry_version != REGISTRY_VERSION {
            return Err(ProfileError::Unsupported);
        }
        if !is_profile_id(&wire.profile_id) {
            return Err(ProfileError::Unsupported);
        }
        let registry = DiagnosticRegistry::embedded().map_err(|_| ProfileError::Registry)?;
        let mut rules = Vec::with_capacity(wire.rules.len());
        let mut seen = BTreeSet::new();
        let mut previous: Option<String> = None;
        for rule in wire.rules {
            if !seen.insert(rule.id.clone())
                || previous
                    .as_deref()
                    .is_some_and(|prior| rule.id.as_str() <= prior)
            {
                // Entries must be sorted by unsigned UTF-8 id bytes and
                // unique; a stale or reordered profile fails closed.
                return Err(ProfileError::Invariant);
            }
            previous = Some(rule.id.clone());
            let Some(entry) = registry.entry(&rule.id) else {
                return Err(ProfileError::Unsupported);
            };
            if entry.lifecycle() != crate::diagnostics::registry::Lifecycle::Active {
                return Err(ProfileError::Unsupported);
            }
            let severity_override = match rule.severity_override {
                None => None,
                Some(spelling) => {
                    let requested = Severity::parse(&spelling).ok_or(ProfileError::Unsupported)?;
                    let default = entry.default_severity();
                    if default == Severity::Error || requested >= default {
                        return Err(ProfileError::Invariant);
                    }
                    Some(requested)
                }
            };
            rules.push(RuleSelection {
                id: rule.id,
                enabled: rule.enabled,
                severity_override,
            });
        }
        Ok(Self {
            profile_id: wire.profile_id,
            version: wire.version,
            registry_version: wire.diagnostic_registry_version,
            rules,
        })
    }

    /// The profile identifier (for example `default` or `strict`).
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// The profile contract version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The diagnostic registry release this profile pins.
    pub fn registry_version(&self) -> &str {
        &self.registry_version
    }

    /// The selection for one registered rule id, when the profile lists it.
    pub fn selection(&self, rule_id: &str) -> Option<&RuleSelection> {
        self.rules
            .binary_search_by(|rule| rule.id.as_str().cmp(rule_id))
            .ok()
            .map(|index| &self.rules[index])
    }

    /// Whether the profile enables one registered rule id.
    pub(crate) fn is_enabled(&self, rule_id: &str) -> bool {
        self.selection(rule_id).is_some_and(|rule| rule.enabled())
    }

    /// The number of enabled rule selections.
    pub(crate) fn enabled_count(&self) -> usize {
        self.rules.iter().filter(|rule| rule.enabled()).count()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileWire {
    schema_version: String,
    identity: String,
    profile_id: String,
    version: String,
    diagnostic_registry_version: String,
    scope: String,
    rules: Vec<RuleWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleWire {
    id: String,
    enabled: bool,
    severity_override: Option<String>,
}

/// One `^[a-z][a-z0-9-]{0,62}$` profile identifier.
fn is_profile_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    !text.is_empty()
        && text.len() <= 63
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One compact single-rule profile wire used by the fail-closed tests.
    fn wire(rule: &str) -> String {
        let template = format!(
            concat!(
                "{{\"schema_version\":\"{}\",\"identity\":\"{}\",",
                "\"profile_id\":\"unit\",\"version\":\"1.0.0\",",
                "\"diagnostic_registry_version\":\"{}\",\"scope\":\"project\",",
                "\"rules\":[{{RULE}}]}}"
            ),
            PROFILE_SCHEMA_VERSION, PROFILE_IDENTITY, REGISTRY_VERSION,
        );
        template.replace("{RULE}", rule)
    }

    const PLAIN_RULE: &str = concat!(
        "{\"id\":\"semantic.type-ref-unresolved\",",
        "\"enabled\":true}"
    );

    #[test]
    fn embedded_profiles_parse_against_the_embedded_registry() {
        let default = ValidationProfile::embedded_default().expect("default parses");
        let strict = ValidationProfile::embedded_strict().expect("strict parses");
        assert_eq!(default.profile_id(), "default");
        assert_eq!(strict.profile_id(), "strict");
        assert_eq!(default.registry_version(), REGISTRY_VERSION);
        for rule in &default.rules {
            assert!(
                DiagnosticRegistry::embedded()
                    .expect("registry")
                    .entry(rule.id())
                    .is_some(),
                "unregistered rule {}",
                rule.id()
            );
        }
    }

    #[test]
    fn default_downgrades_the_portability_warning_strict_keeps_it() {
        let default = ValidationProfile::embedded_default().expect("default parses");
        let strict = ValidationProfile::embedded_strict().expect("strict parses");
        let selection = default
            .selection("semantic.portable-target-reference")
            .expect("listed");
        assert_eq!(selection.severity_override(), Some(Severity::Info));
        let selection = strict
            .selection("semantic.portable-target-reference")
            .expect("listed");
        assert_eq!(selection.severity_override(), None);
        // The default and strict profiles differ only in that override.
        assert_eq!(default.rules.len(), strict.rules.len());
    }

    #[test]
    fn single_valid_rule_parses() {
        let profile =
            ValidationProfile::from_bytes(wire(PLAIN_RULE).as_bytes()).expect("valid wire parses");
        assert_eq!(profile.profile_id(), "unit");
        assert!(profile.is_enabled("semantic.type-ref-unresolved"));
        assert_eq!(profile.enabled_count(), 1);
    }

    #[test]
    fn unknown_rule_ids_fail_closed() {
        let unknown =
            wire(PLAIN_RULE).replace("semantic.type-ref-unresolved", "semantic.ghost-rule");
        assert_eq!(
            ValidationProfile::from_bytes(unknown.as_bytes()),
            Err(ProfileError::Unsupported)
        );
    }

    #[test]
    fn error_severity_rules_are_never_overridable() {
        let downgrade = wire(PLAIN_RULE).replace(
            "\"enabled\":true}",
            "\"enabled\":true,\"severity_override\":\"warning\"}",
        );
        assert_eq!(
            ValidationProfile::from_bytes(downgrade.as_bytes()),
            Err(ProfileError::Invariant)
        );
    }

    #[test]
    fn severity_upgrades_fail_closed() {
        let upgrade = wire(PLAIN_RULE).replace(
            "\"enabled\":true}",
            "\"enabled\":true,\"severity_override\":\"error\"}",
        );
        assert_eq!(
            ValidationProfile::from_bytes(upgrade.as_bytes()),
            Err(ProfileError::Invariant)
        );
    }

    #[test]
    fn duplicate_rule_entries_fail_closed() {
        let duplicate = wire(&format!("{PLAIN_RULE},{PLAIN_RULE}"));
        assert_eq!(
            ValidationProfile::from_bytes(duplicate.as_bytes()),
            Err(ProfileError::Invariant)
        );
    }

    #[test]
    fn unsupported_profile_versions_fail_closed() {
        let stale = wire(PLAIN_RULE).replace("\"version\":\"1.0.0\"", "\"version\":\"2.0.0\"");
        assert_eq!(
            ValidationProfile::from_bytes(stale.as_bytes()),
            Err(ProfileError::Unsupported)
        );
    }

    #[test]
    fn malformed_wire_fails_closed() {
        assert_eq!(
            ValidationProfile::from_bytes(b"{"),
            Err(ProfileError::Malformed)
        );
    }
}

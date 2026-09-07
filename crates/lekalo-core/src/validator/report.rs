//! The validation report and its fixed wire projection (issue #12).
//!
//! A report carries the normalized diagnostic set, the identity of the
//! selected profile and the registry release it pinned, the enabled-rule
//! count, and severity counts. It never carries native paths, raw source,
//! source snippets, adapter data, timestamps, or host/user/process data.

use serde::Serialize;

use crate::diagnostics::types::Severity;
use crate::diagnostics::DiagnosticSet;

/// The severity totals of one report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SeverityCounts {
    pub error: u32,
    pub warning: u32,
    pub info: u32,
}

/// One successful semantic-validation run.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidationReport {
    profile_id: String,
    profile_version: String,
    registry_version: String,
    module_scope: Option<String>,
    rules_enabled: usize,
    diagnostics: DiagnosticSet,
    counts: SeverityCounts,
}

impl ValidationReport {
    /// Assemble from a normalized set; counts derive from the set.
    pub(crate) fn from_set(
        profile: &super::profile::ValidationProfile,
        module_scope: Option<String>,
        diagnostics: DiagnosticSet,
    ) -> Self {
        let mut counts = SeverityCounts {
            error: 0,
            warning: 0,
            info: 0,
        };
        for diagnostic in diagnostics.as_slice() {
            match diagnostic.severity() {
                Severity::Error => counts.error += 1,
                Severity::Warning => counts.warning += 1,
                Severity::Info => counts.info += 1,
            }
        }
        Self {
            profile_id: profile.profile_id().to_owned(),
            profile_version: profile.version().to_owned(),
            registry_version: profile.registry_version().to_owned(),
            module_scope,
            rules_enabled: profile.enabled_count(),
            counts,
            diagnostics,
        }
    }

    /// The selected profile identifier.
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    /// The profile contract version.
    pub fn profile_version(&self) -> &str {
        &self.profile_version
    }

    /// The diagnostic registry release the profile pinned.
    pub fn registry_version(&self) -> &str {
        &self.registry_version
    }

    /// The module scope, when validation ran module-scoped.
    pub fn module_scope(&self) -> Option<&str> {
        self.module_scope.as_deref()
    }

    /// The number of enabled rules.
    pub fn rules_enabled(&self) -> usize {
        self.rules_enabled
    }

    /// The normalized diagnostics in total order.
    pub fn diagnostics(&self) -> &DiagnosticSet {
        &self.diagnostics
    }

    /// The severity totals.
    pub fn counts(&self) -> &SeverityCounts {
        &self.counts
    }

    /// The compact JSON projection with the fixed field order.
    pub fn to_json(&self) -> String {
        #[derive(Serialize)]
        struct CountsWire<'a> {
            error: &'a u32,
            warning: &'a u32,
            info: &'a u32,
        }
        #[derive(Serialize)]
        struct ReportWire<'a> {
            profile: &'a str,
            #[serde(rename = "profileVersion")]
            profile_version: &'a str,
            #[serde(rename = "registryVersion")]
            registry_version: &'a str,
            #[serde(rename = "moduleScope", skip_serializing_if = "Option::is_none")]
            module_scope: Option<&'a str>,
            #[serde(rename = "rulesEnabled")]
            rules_enabled: usize,
            counts: CountsWire<'a>,
        }
        let wire = ReportWire {
            profile: &self.profile_id,
            profile_version: &self.profile_version,
            registry_version: &self.registry_version,
            module_scope: self.module_scope.as_deref(),
            rules_enabled: self.rules_enabled,
            counts: CountsWire {
                error: &self.counts.error,
                warning: &self.counts.warning,
                info: &self.counts.info,
            },
        };
        serde_json::to_string(&wire).expect("report serializes")
    }

    /// The stable one-line human summary.
    pub fn to_human(&self) -> String {
        let scope = match &self.module_scope {
            Some(scope) => format!(" (module {scope})"),
            None => String::new(),
        };
        format!(
            "validated {} profile v{}: {} rules, {} errors, {} warnings, {} info{}",
            self.profile_id,
            self.profile_version,
            self.rules_enabled,
            self.counts.error,
            self.counts.warning,
            self.counts.info,
            scope,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_projection_has_the_fixed_field_order() {
        let report = ValidationReport {
            profile_id: "default".to_owned(),
            profile_version: "1.0.0".to_owned(),
            registry_version: crate::diagnostics::version::REGISTRY_VERSION.to_owned(),
            module_scope: Some("planner".to_owned()),
            rules_enabled: 25,
            diagnostics: DiagnosticSet::empty(),
            counts: SeverityCounts {
                error: 0,
                warning: 1,
                info: 2,
            },
        };
        assert_eq!(
            report.to_json(),
            concat!(
                "{\"profile\":\"default\",\"profileVersion\":\"1.0.0\",",
                "\"registryVersion\":\"1.9.0\",\"moduleScope\":\"planner\",",
                "\"rulesEnabled\":25,\"counts\":{\"error\":0,\"warning\":1,\"info\":2}}"
            )
        );
        assert_eq!(
            report.to_human(),
            "validated default profile v1.0.0: 25 rules, 0 errors, 1 warnings, 2 info (module planner)"
        );
        let unscoped = ValidationReport {
            module_scope: None,
            ..report
        };
        assert!(unscoped
            .to_json()
            .contains("\"rulesEnabled\":25,\"counts\""));
        assert!(!unscoped.to_json().contains("moduleScope"));
    }
}

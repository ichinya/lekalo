//! Per-axis portability reports between resolved profiles (issue #29).
//!
//! [`portability`] compares two resolved profiles and reports, for every
//! axis separately, whether the component is reused or changes, and —
//! per axis — exactly which provided capabilities the change gains,
//! loses, strengthens, or weakens. The report is a plain serializable
//! value: two runs over the same inputs are byte-identical, every axis
//! appears in the fixed byte-sorted order, and no entry depends on
//! caller input order.
//!
//! Portability never launches anything and never resolves anything: it
//! consumes two already-resolved profiles, so its verdicts describe
//! semantics a run actually binds to.

use serde::Serialize;

use super::component::{Axis, Support};
use super::resolution::ResolvedProfile;

/// The verdict on one axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Both profiles resolve the same component on the axis.
    Reused,
    /// The profiles resolve different components on the axis.
    Changed,
}

impl Verdict {
    /// The stable lowercase wire token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Reused => "reused",
            Self::Changed => "changed",
        }
    }
}

/// One capability delta contributed by the changed axis component.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapabilityChange {
    /// The stable dotted capability id.
    pub id: String,
    /// The support the source profile's component provided, or `None`.
    pub source: Option<Support>,
    /// The support the target profile's component provides, or `None`.
    pub target: Option<Support>,
}

/// The portability of one axis.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AxisPortability {
    /// The axis.
    pub axis: Axis,
    /// The source profile's component id, when the axis was resolved.
    pub source: Option<String>,
    /// The target profile's component id, when the axis was resolved.
    pub target: Option<String>,
    /// Whether the component is reused or changes.
    pub verdict: Verdict,
    /// The capability deltas of the axis components, byte-sorted by id,
    /// present only when the component changes.
    pub capability_changes: Vec<CapabilityChange>,
}

/// The whole per-axis portability report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PortabilityReport {
    /// The source profile id.
    pub source: String,
    /// The target profile id.
    pub target: String,
    /// Every axis, byte-sorted.
    pub axes: Vec<AxisPortability>,
}

/// Compare two resolved profiles axis by axis.
pub fn portability(source: &ResolvedProfile, target: &ResolvedProfile) -> PortabilityReport {
    let axes = Axis::ALL
        .iter()
        .map(|axis| {
            let source_component = source.component(*axis);
            let target_component = target.component(*axis);
            let verdict = if source_component == target_component {
                Verdict::Reused
            } else {
                Verdict::Changed
            };
            let capability_changes = if verdict == Verdict::Reused {
                Vec::new()
            } else {
                capability_changes(source, target, *axis)
            };
            AxisPortability {
                axis: *axis,
                source: source_component.map(str::to_owned),
                target: target_component.map(str::to_owned),
                verdict,
                capability_changes,
            }
        })
        .collect();
    PortabilityReport {
        source: source.id.clone(),
        target: target.id.clone(),
        axes,
    }
}

/// The provided-capability delta of one axis between two profiles,
/// byte-sorted by id and restricted to actual differences.
fn capability_changes(
    source: &ResolvedProfile,
    target: &ResolvedProfile,
    axis: Axis,
) -> Vec<CapabilityChange> {
    let component_support = |profile: &ResolvedProfile| -> Vec<(String, Support)> {
        profile
            .components
            .iter()
            .filter(|component| component.axis == axis)
            .map(|component| component.id.clone())
            .flat_map(|id| {
                super::component::definition(axis, &id)
                    .into_iter()
                    .flat_map(|definition| definition.provides)
                    .map(|provide| (provide.id.to_owned(), provide.support))
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let mut changes: Vec<CapabilityChange> = Vec::new();
    let source_support = component_support(source);
    let target_support = component_support(target);
    for (id, source_state) in &source_support {
        let target_state = target_support
            .iter()
            .find(|(target_id, _)| target_id == id)
            .map(|(_, support)| *support);
        if target_state != Some(*source_state) {
            changes.push(CapabilityChange {
                id: id.clone(),
                source: Some(*source_state),
                target: target_state,
            });
        }
    }
    for (id, target_state) in &target_support {
        if !source_support.iter().any(|(source_id, _)| source_id == id) {
            changes.push(CapabilityChange {
                id: id.clone(),
                source: None,
                target: Some(*target_state),
            });
        }
    }
    changes.sort_by(|left, right| left.id.as_bytes().cmp(right.id.as_bytes()));
    changes.dedup_by(|left, right| left.id == right.id);
    changes
}

#[cfg(test)]
mod tests {
    use super::super::document::decode;
    use super::super::resolution::resolve;
    use super::*;

    fn profile_of(text: &str, id: &str) -> ResolvedProfile {
        let document = decode(text.as_bytes()).expect("decodes");
        let resolved = resolve(&document).expect("resolves");
        resolved
            .into_iter()
            .find(|profile| profile.id == id)
            .expect("profile resolves")
    }

    const MONOREPO: &str = r#"{
        "schema_version": "lekalo/target-profile/v1.0.0",
        "profiles": [
            {
                "id": "node-postgres-http",
                "version": "1.0.0",
                "components": {
                    "runtime": "node-typescript",
                    "storage": "postgres-sql",
                    "transport": "http-json",
                    "testing": "node-native",
                    "analysis": "typescript-native",
                    "deployment": "container"
                }
            },
            {
                "id": "laravel-postgres-http",
                "version": "1.0.0",
                "components": {
                    "runtime": "php-laravel",
                    "storage": "postgres-sql",
                    "transport": "http-json",
                    "testing": "laratesto",
                    "analysis": "mago",
                    "deployment": "container"
                }
            }
        ]
    }"#;

    #[test]
    fn report_names_exactly_the_changing_components() {
        let node = profile_of(MONOREPO, "node-postgres-http");
        let laravel = profile_of(MONOREPO, "laravel-postgres-http");
        let report = portability(&node, &laravel);
        assert_eq!(report.source, "node-postgres-http");
        assert_eq!(report.target, "laravel-postgres-http");
        assert_eq!(report.axes.len(), 6);
        for axis in [Axis::Storage, Axis::Transport, Axis::Deployment] {
            let entry = report
                .axes
                .iter()
                .find(|entry| entry.axis == axis)
                .expect("axis entry");
            assert_eq!(entry.verdict, Verdict::Reused, "{axis:?} is reused");
            assert!(entry.capability_changes.is_empty());
        }
        for axis in [Axis::Runtime, Axis::Testing, Axis::Analysis] {
            let entry = report
                .axes
                .iter()
                .find(|entry| entry.axis == axis)
                .expect("axis entry");
            assert_eq!(entry.verdict, Verdict::Changed, "{axis:?} changes");
            assert!(!entry.capability_changes.is_empty());
        }
        // runtime.async: node full -> php partial is a weakening the
        // report names explicitly.
        let async_entry = &report
            .axes
            .iter()
            .find(|entry| entry.axis == Axis::Runtime)
            .expect("runtime entry")
            .capability_changes;
        assert!(async_entry.contains(&CapabilityChange {
            id: "runtime.async".to_owned(),
            source: Some(Support::Full),
            target: Some(Support::Partial),
        }));
        // The whole report serializes deterministically: the first axis
        // is analysis, its deltas sorted by id.
        assert_eq!(
            serde_json::to_string(&report.axes[0]).expect("serializes"),
            "{\"axis\":\"analysis\",\"source\":\"typescript-native\",\
              \"target\":\"mago\",\"verdict\":\"changed\",\
              \"capability_changes\":[{\"id\":\"analysis.lint\",\"source\":null,\
              \"target\":\"full\"},{\"id\":\"analysis.types\",\"source\":\"full\",\
              \"target\":\"partial\"}]}"
        );
    }

    #[test]
    fn a_profile_is_reused_on_every_axis_against_itself() {
        let node = profile_of(MONOREPO, "node-postgres-http");
        let report = portability(&node, &node);
        assert!(report
            .axes
            .iter()
            .all(|entry| entry.verdict == Verdict::Reused));
        assert!(report
            .axes
            .iter()
            .all(|entry| entry.capability_changes.is_empty()));
        assert_eq!(
            serde_json::to_string(&report).expect("serializes"),
            serde_json::to_string(&portability(&node, &node)).expect("serializes")
        );
    }
}

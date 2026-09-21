//! The manifest-to-manifest update diff (issue #32).
//!
//! [`diff_manifests`] compares the currently selected manifest with the
//! candidate update manifest and renders the permission/capability
//! delta the update preview must show (issue acceptance criterion 3):
//! added/removed read+write scopes, network mode and destinations,
//! environment allowlist, process policy, secrets, hooks, named
//! capability state changes, and the protocol/IR compatibility deltas.
//!
//! A **widened** permission set is flagged `escalated: true` in the
//! result — never silent — and the install plan refuses to apply it
//! without the explicit `--allow-escalation` policy
//! (`adapter.permission-escalated`).

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::manifest::ManifestDocument;

/// One rendered diff direction for list members.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    /// The member was added by the update.
    Added,
    /// The member was removed by the update.
    Removed,
}

/// The rendered permission/capability delta between two manifests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ManifestDiff {
    /// Read scopes added/removed by the update.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub read_scopes: Vec<(Change, String)>,
    /// Write scopes added/removed by the update.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub write_scopes: Vec<(Change, String)>,
    /// The network mode transition, when it changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_mode: Option<(String, String)>,
    /// Network destinations added/removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub network_destinations: Vec<(Change, String)>,
    /// Environment allowlist entries added/removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub environment: Vec<(Change, String)>,
    /// The child-process policy transition, when it changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processes: Option<(String, String)>,
    /// Secret handles added/removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<(Change, String)>,
    /// Named capability state transitions (`id: before → after`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub named_capabilities: Vec<(String, String, String)>,
    /// Protocol versions added/removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub protocol_versions: Vec<(Change, String)>,
    /// IR versions added/removed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ir_versions: Vec<(Change, String)>,
    /// Whether the update widens any permission member. Never silent.
    pub escalated: bool,
    /// The first widened member, in canonical order (the diagnostic's
    /// `member` token).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalated_member: Option<String>,
}

impl ManifestDiff {
    /// Whether the diff carries any change at all.
    pub fn is_empty(&self) -> bool {
        !self.escalated
            && self.read_scopes.is_empty()
            && self.write_scopes.is_empty()
            && self.network_mode.is_none()
            && self.network_destinations.is_empty()
            && self.environment.is_empty()
            && self.processes.is_none()
            && self.secrets.is_empty()
            && self.named_capabilities.is_empty()
            && self.protocol_versions.is_empty()
            && self.ir_versions.is_empty()
    }
}

/// Compare the selected manifest (before) with the update candidate
/// (after). Both manifests are already validated; the diff is a pure
/// function of their canonical members. The JSON projection reads the
/// canonical stored documents so nothing depends on Rust-only state.
pub fn diff_manifests(before: &ManifestDocument, after: &ManifestDocument) -> ManifestDiff {
    let before_json = serde_json::from_slice::<serde_json::Value>(&before.canonical_bytes())
        .expect("canonical bytes are JSON");
    let after_json = serde_json::from_slice::<serde_json::Value>(&after.canonical_bytes())
        .expect("canonical bytes are JSON");
    let empty = serde_json::Value::Object(serde_json::Map::new());
    let before_caps = before_json.get("capabilities").unwrap_or(&empty);
    let after_caps = after_json.get("capabilities").unwrap_or(&empty);
    let before_perm = before_json.get("permissions").unwrap_or(&empty);
    let after_perm = after_json.get("permissions").unwrap_or(&empty);
    let mut diff = ManifestDiff {
        read_scopes: list_delta(before_caps, after_caps, "readScopes"),
        write_scopes: list_delta(before_caps, after_caps, "writeScopes"),
        network_mode: member_transition(before_perm, after_perm, &["network", "mode"]),
        network_destinations: list_delta(
            before_perm.get("network").unwrap_or(&empty),
            after_perm.get("network").unwrap_or(&empty),
            "destinations",
        ),
        environment: list_delta(
            before_perm.get("environment").unwrap_or(&empty),
            after_perm.get("environment").unwrap_or(&empty),
            "allowlist",
        ),
        processes: member_transition(before_perm, after_perm, &["processes", "children"]),
        secrets: list_delta(
            before_perm.get("secrets").unwrap_or(&empty),
            after_perm.get("secrets").unwrap_or(&empty),
            "handles",
        ),
        named_capabilities: named_delta(before_caps, after_caps),
        protocol_versions: list_delta(
            before_json.get("compatibility").unwrap_or(&empty),
            after_json.get("compatibility").unwrap_or(&empty),
            "protocolVersions",
        ),
        ir_versions: list_delta(
            before_json.get("compatibility").unwrap_or(&empty),
            after_json.get("compatibility").unwrap_or(&empty),
            "irVersions",
        ),
        escalated: false,
        escalated_member: None,
    };
    evaluate_escalation(&mut diff, before_perm, after_perm);
    diff
}

/// The added/removed members of one bounded list.
fn list_delta(
    before: &serde_json::Value,
    after: &serde_json::Value,
    member: &str,
) -> Vec<(Change, String)> {
    let items = |value: &serde_json::Value| -> BTreeSet<String> {
        value
            .get(member)
            .and_then(|member| member.as_array())
            .map(|array| {
                array
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let before_set = items(before);
    let after_set = items(after);
    let mut delta = Vec::new();
    for added in after_set.difference(&before_set) {
        delta.push((Change::Added, added.clone()));
    }
    for removed in before_set.difference(&after_set) {
        delta.push((Change::Removed, removed.clone()));
    }
    delta
}

/// A scalar member transition between two nested members.
fn member_transition(
    before: &serde_json::Value,
    after: &serde_json::Value,
    path: &[&str],
) -> Option<(String, String)> {
    let walk = |value: &serde_json::Value| -> Option<String> {
        let mut current = value;
        for key in path {
            current = current.get(*key)?;
        }
        current.as_str().map(str::to_owned)
    };
    match (walk(before), walk(after)) {
        (Some(before), Some(after)) if before != after => Some((before, after)),
        _ => None,
    }
}

/// Named capability state transitions, sorted by id.
fn named_delta(
    before: &serde_json::Value,
    after: &serde_json::Value,
) -> Vec<(String, String, String)> {
    let states = |value: &serde_json::Value| -> BTreeMap<String, String> {
        value
            .get("named")
            .and_then(|named| named.as_object())
            .map(|map| {
                map.iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|state| (key.clone(), state.to_owned()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let before_map = states(before);
    let after_map = states(after);
    let mut delta = Vec::new();
    for (id, after_state) in &after_map {
        match before_map.get(id) {
            Some(before_state) if before_state != after_state => {
                delta.push((id.clone(), before_state.clone(), after_state.clone()));
            }
            None => delta.push((id.clone(), "absent".to_owned(), after_state.clone())),
            _ => {}
        }
    }
    for (id, before_state) in &before_map {
        if !after_map.contains_key(id) {
            delta.push((id.clone(), before_state.clone(), "absent".to_owned()));
        }
    }
    delta.sort();
    delta
}

/// Decide whether the update widens any permission member, and record
/// the first widened member in canonical order.
fn evaluate_escalation(
    diff: &mut ManifestDiff,
    before: &serde_json::Value,
    after: &serde_json::Value,
) {
    let widened_list = |path: &[&str]| -> bool {
        let items = |value: &serde_json::Value| -> BTreeSet<String> {
            let mut current = value;
            for key in path {
                match current.get(*key) {
                    Some(next) => current = next,
                    None => return BTreeSet::new(),
                }
            }
            current
                .as_array()
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        !items(after)
            .difference(&items(before))
            .collect::<Vec<_>>()
            .is_empty()
    };
    // Network widening: allowlist mode introduced, or destinations added
    // under an existing allowlist.
    let before_mode = before
        .get("network")
        .and_then(|network| network.get("mode"))
        .and_then(|mode| mode.as_str());
    let after_mode = after
        .get("network")
        .and_then(|network| network.get("mode"))
        .and_then(|mode| mode.as_str());
    if before_mode == Some("denied") && after_mode == Some("allowlist") {
        diff.escalated = true;
        diff.escalated_member = Some("network".to_owned());
        return;
    }
    if widened_list(&["network", "destinations"]) {
        diff.escalated = true;
        diff.escalated_member = Some("network".to_owned());
        return;
    }
    if widened_list(&["environment", "allowlist"]) {
        diff.escalated = true;
        diff.escalated_member = Some("environment".to_owned());
        return;
    }
    if widened_list(&["filesystem", "readScopes"]) {
        diff.escalated = true;
        diff.escalated_member = Some("filesystem.readScopes".to_owned());
        return;
    }
    if widened_list(&["filesystem", "writeScopes"]) {
        diff.escalated = true;
        diff.escalated_member = Some("filesystem.writeScopes".to_owned());
        return;
    }
    // Child processes: denied → declared is a widening.
    if let Some((before_children, after_children)) =
        member_transition(before, after, &["processes", "children"])
    {
        if before_children == "denied" && after_children == "declared" {
            diff.escalated = true;
            diff.escalated_member = Some("processes".to_owned());
            return;
        }
    }
    if widened_list(&["secrets", "handles"]) {
        diff.escalated = true;
        diff.escalated_member = Some("secrets".to_owned());
    }
}

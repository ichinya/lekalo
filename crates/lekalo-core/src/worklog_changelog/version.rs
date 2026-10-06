//! Closed identity and owner-approved bounds of the pilot composition
//! contract (issue #51).

/// The exact wire discriminator.
pub const SCHEMA_VERSION: &str = "lekalo/worklog-changelog-pilot/v0.8.0";

/// The exact contract identity.
pub const IDENTITY: &str = "dev.lekalo.worklog-changelog-pilot@0.8.0";

/// The exact wire discriminator of the evidence report contract.
pub const EVIDENCE_SCHEMA_VERSION: &str = "lekalo/worklog-changelog-evidence/v0.8.0";

/// The exact contract identity of the evidence report contract.
pub const EVIDENCE_IDENTITY: &str = "dev.lekalo.worklog-changelog-evidence@0.8.0";

/// The contract version.
pub const VERSION: &str = "0.8.0";

/// The exact identity of the pinned IR contract.
pub const IR_IDENTITY: &str = "dev.lekalo.ir@0.2.16";

/// The exact accepted Model version the composition pins.
pub const MODEL_VERSION: &str = "0.2.16";

/// Maximum accepted document size (bytes).
pub const MAX_DOC_BYTES: usize = 256 * 1024;

/// Maximum number of declared operation roles.
pub const MAX_OPERATIONS: usize = 128;

/// Maximum number of independence rules.
pub const MAX_INDEPENDENCE_RULES: usize = 128;

/// Maximum number of correlation groups.
pub const MAX_CORRELATIONS: usize = 64;

/// Maximum number of approval bindings.
pub const MAX_APPROVALS: usize = 32;

/// Maximum number of snapshot declarations.
pub const MAX_SNAPSHOTS: usize = 32;

/// Maximum number of public allowlist fields.
pub const MAX_ALLOWLIST_FIELDS: usize = 64;

/// Maximum number of excluded private fields.
pub const MAX_EXCLUDED_FIELDS: usize = 64;

/// Maximum number of replay scopes.
pub const MAX_REPLAY_SCOPES: usize = 64;

/// Maximum number of capability requirements.
pub const MAX_CAPABILITY_REQUIREMENTS: usize = 64;

/// Maximum number of consumed attachment pins.
pub const MAX_CONSUMED: usize = 16;

/// Maximum number of consumers in one evidence report.
pub const MAX_CONSUMERS: usize = 128;

/// Maximum number of scenarios in one evidence report.
pub const MAX_SCENARIOS: usize = 64;

/// Maximum number of capability rows per target in one report.
pub const MAX_CAPABILITIES_PER_TARGET: usize = 32;

/// Maximum number of assertions per scenario row in one report.
pub const MAX_ASSERTIONS_PER_SCENARIO: usize = 256;

/// The closed consumed attachment families.
pub const CONSUMED_FAMILIES: &[&str] = &[
    "authorization",
    "transaction-concurrency",
    "extended-effects",
    "error-contract",
    "invariant-transition",
    "requirements",
];

/// The closed effect-kind vocabulary of a declared composition role,
/// aligned with the #14 canonical effect kinds the Model can declare.
pub const EFFECT_KINDS: &[&str] = &[
    "create",
    "update",
    "delete",
    "read",
    "emit-event",
    "enqueue-job",
    "external-call",
    "publish-output",
    "audit-log",
];

/// The closed commit-boundary vocabulary.
pub const COMMIT_BOUNDARIES: &[&str] = &["local", "local_then_external", "read_only"];

/// The closed operation roles of the pilot.
pub const OPERATION_ROLES: &[&str] = &[
    "worklog.complete_task",
    "worklog.write_worklog",
    "worklog.edit_worklog",
    "worklog.request_provider_comment",
    "worklog.deliver_provider_comment",
    "changelog.create_draft",
    "changelog.edit_draft",
    "changelog.preview_entry",
    "changelog.approve_entry",
    "changelog.publish_entry",
    "changelog.withdraw_entry",
    "changelog.public_entry",
];

/// The closed role-to-effect-kind vocabulary: the CRUD kinds must
/// match the declared Model effect operation; the infrastructure
/// kinds are the refinements the pilot's extended-effects and
/// transaction attachments attach to that role.
pub const ROLE_KINDS: &[(&str, &[&str])] = &[
    ("worklog.complete_task", &["update"]),
    ("worklog.write_worklog", &["create"]),
    ("worklog.edit_worklog", &["update"]),
    (
        "worklog.request_provider_comment",
        &["create", "enqueue-job"],
    ),
    (
        "worklog.deliver_provider_comment",
        &["update", "external-call"],
    ),
    ("changelog.create_draft", &["create"]),
    ("changelog.edit_draft", &["update"]),
    ("changelog.preview_entry", &["update"]),
    ("changelog.approve_entry", &["create"]),
    (
        "changelog.publish_entry",
        &["create", "update", "publish-output", "audit-log"],
    ),
    ("changelog.withdraw_entry", &["update", "create"]),
    ("changelog.public_entry", &["read"]),
];

/// The allowed effect kinds of one role.
pub fn role_kinds(operation: &str) -> &'static [&'static str] {
    ROLE_KINDS
        .iter()
        .find(|(role, _)| *role == operation)
        .map(|(_, kinds)| *kinds)
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_follow_the_product_version_rule() {
        assert_eq!(SCHEMA_VERSION, "lekalo/worklog-changelog-pilot/v0.8.0");
        assert_eq!(IDENTITY, "dev.lekalo.worklog-changelog-pilot@0.8.0");
        assert_eq!(VERSION, "0.8.0");
        assert_eq!(OPERATION_ROLES.len(), 12);
        assert!(OPERATION_ROLES
            .iter()
            .all(|role| role.starts_with("worklog.") || role.starts_with("changelog.")));
    }
}

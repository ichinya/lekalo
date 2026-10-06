//! Semantic validation stage markers and shared refusal tokens
//! (issue #106). The heavy lifting lives in `decode`/`build`; this module
//! keeps the closed token vocabulary in one place.

/// The closed validate-stage refusal tokens (distinct from decode).
pub const VALIDATE_REASONS: [&str; 21] = [
    "dry-run",
    "echo-operation",
    "echo-request-id",
    "echo-version",
    "error-pairing",
    "identity",
    "ir-path",
    "lint-binding",
    "lint-pin",
    "lint-request",
    "limits",
    "member",
    "native-request",
    "plan-id",
    "profile",
    "profile-capabilities",
    "profile-digest",
    "version",
    "writes-missing",
    "write-digest",
    "target",
];

/// The stage marker carried by validate refusals.
pub const VALIDATE_STAGE: &str = "validate";

/// Whether the operation belongs to the strict base surface.
pub fn is_base_operation(operation: &str) -> bool {
    crate::generated::protocol::BASE_OPERATIONS.contains(&operation)
}

/// Whether the operation belongs to this build's full vocabulary.
pub fn is_known_operation(operation: &str) -> bool {
    crate::generated::protocol::LATEST_OPERATIONS.contains(&operation)
}

/// Whether the value belongs to the closed support-state set.
pub fn is_support_state(value: &str) -> bool {
    crate::generated::protocol::enums::SUPPORTSTATE.contains(&value)
}

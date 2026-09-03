//! Version-aware grammar validation for typed IR identifiers (issue #8).
//!
//! Every grammar is the exact JSON Schema pattern of the active source Model
//! version; nothing is invented here. Reserved-word filtering (`lekalo`,
//! `dev`) stays with the Model validator, matching the accepted #7 loader
//! policy: the IR never becomes a second authority.

use crate::loader::ModelVersion;

/// One `^[a-z][a-z0-9_]{0,62}$` segment.
pub(crate) fn is_segment(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest.len() <= 62
        && rest
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// One `^[a-z][a-z0-9_]{0,63}$` field or enum-value name.
pub(crate) fn is_field_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest.len() <= 63
        && rest
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_')
}

/// One `^[a-z][a-z0-9_-]{0,62}$` target name.
pub(crate) fn is_target_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && rest.len() <= 62
        && rest
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-'))
}

pub(crate) fn is_requirement_id(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&first, rest)) = bytes.split_first() else {
        return false;
    };
    (first.is_ascii_uppercase() || first.is_ascii_digit())
        && rest.len() <= 127
        && rest
            .iter()
            .all(|byte| matches!(byte, b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'-'))
}

/// The symbol-ID grammar of the active Model version.
///
/// Model 0.1.0: `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)?$` with total length
/// 3-129 (one or two unbounded-length segments). Model 1.0.0: two or three
/// `segment` parts with total length 3-191.
pub(crate) fn is_symbol_id(version: ModelVersion, text: &str) -> bool {
    match version {
        ModelVersion::V0_1_0 => {
            if !(3..=129).contains(&text.chars().count()) {
                return false;
            }
            text.split('.').all(|part| {
                let bytes = part.as_bytes();
                let Some((&first, rest)) = bytes.split_first() else {
                    return false;
                };
                first.is_ascii_lowercase()
                    && rest.iter().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'_'
                    })
            }) && text.matches('.').count() <= 1
        }
        ModelVersion::V1_0_0 => {
            if !(3..=191).contains(&text.chars().count()) {
                return false;
            }
            let parts: Vec<&str> = text.split('.').collect();
            (2..=3).contains(&parts.len()) && parts.iter().all(|part| is_segment(part))
        }
    }
}

/// The project-ID grammar of the active Model version.
///
/// Model 0.1.0 uses the shared definition-ID grammar; Model 1.0.0 requires
/// exactly one `segment`.
pub(crate) fn is_project_id(version: ModelVersion, text: &str) -> bool {
    match version {
        ModelVersion::V0_1_0 => is_symbol_id(version, text),
        ModelVersion::V1_0_0 => is_segment(text),
    }
}

/// The endpoint path grammar: `^(/[a-z0-9:_{}-]+)+$` with a 512 length cap.
pub(crate) fn is_endpoint_path(text: &str) -> bool {
    if text.is_empty() || text.chars().count() > 512 {
        return false;
    }
    let segments = text.split('/');
    let Some(first) = segments.clone().next() else {
        return false;
    };
    if !first.is_empty() {
        return false;
    }
    segments.skip(1).all(|segment| {
        !segment.is_empty()
            && segment.bytes().all(|byte| {
                matches!(
                    byte,
                    b'a'..=b'z' | b'0'..=b'9' | b':' | b'_' | b'{' | b'}' | b'-'
                )
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_reject_underscores_at_start_and_overlong_parts() {
        assert!(is_segment("a"));
        assert!(is_segment("task_id"));
        assert!(is_segment(&"a".repeat(63)));
        assert!(!is_segment(""));
        assert!(!is_segment("A"));
        assert!(!is_segment("_a"));
        assert!(!is_segment(&"a".repeat(64)));
        assert!(!is_segment("a-b"));
        assert!(!is_segment("a.b"));
    }

    #[test]
    fn symbol_ids_follow_the_active_version() {
        assert!(is_symbol_id(ModelVersion::V0_1_0, "abc"));
        assert!(is_symbol_id(ModelVersion::V0_1_0, "a.b"));
        assert!(is_symbol_id(ModelVersion::V0_1_0, "planner.task_id"));
        assert!(!is_symbol_id(ModelVersion::V0_1_0, "ab"));
        assert!(!is_symbol_id(ModelVersion::V0_1_0, "a.b.c"));
        assert!(!is_symbol_id(ModelVersion::V0_1_0, "a..b"));
        assert!(is_symbol_id(ModelVersion::V1_0_0, "planner.task"));
        assert!(is_symbol_id(ModelVersion::V1_0_0, "planner.task.field"));
        assert!(!is_symbol_id(ModelVersion::V1_0_0, "planner"));
        assert!(!is_symbol_id(ModelVersion::V1_0_0, "planner.a.b.c"));
        assert!(is_symbol_id(ModelVersion::V1_0_0, "ab.c"));
    }

    #[test]
    fn project_ids_follow_the_active_version() {
        assert!(is_project_id(ModelVersion::V0_1_0, "planner"));
        assert!(is_project_id(ModelVersion::V0_1_0, "a.b"));
        assert!(is_project_id(ModelVersion::V1_0_0, "planner"));
        assert!(!is_project_id(ModelVersion::V1_0_0, "a.b"));
    }

    #[test]
    fn names_targets_and_requirements_reject_malformed_spelling() {
        assert!(is_field_name("task_id"));
        assert!(!is_field_name("Task"));
        assert!(is_target_name("node-typescript"));
        assert!(is_target_name("node_typescript"));
        assert!(!is_target_name("-node"));
        assert!(is_requirement_id("PLANNER-REQ-001"));
        assert!(!is_requirement_id("planner-req"));
        assert!(!is_requirement_id("-PLANNER"));
    }

    #[test]
    fn endpoint_paths_accept_parameterized_segments_only() {
        assert!(is_endpoint_path("/tasks"));
        assert!(is_endpoint_path("/users/{user_id}/tasks"));
        assert!(is_endpoint_path("/v1:scope/tasks-done"));
        assert!(!is_endpoint_path(""));
        assert!(!is_endpoint_path("tasks"));
        assert!(!is_endpoint_path("/tasks//done"));
        assert!(!is_endpoint_path("/Tasks"));
        assert!(!is_endpoint_path(&format!("/{}", "a".repeat(513))));
    }
}

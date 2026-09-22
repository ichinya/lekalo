//! Deterministic naming and JSON-pointer algebra of the OpenAPI
//! projection (issue #46).
//!
//! Component names follow the #45 export-name rule exactly
//! (`zod-map.mjs`): `<ModulePascal><NamePascal>` over the semantic
//! id's module and local segments — `planner.task` renders
//! `PlannerTask`, `planner.focus_conflict` renders
//! `PlannerFocusConflict`. One closed rule keeps the Zod and OpenAPI
//! renderers provably aligned, and deduplication by semantic id makes
//! a rendered-only rename impossible (plan §2.2).

/// The document-absolute prefix of every reusable schema reference.
pub const COMPONENTS_SCHEMAS: &str = "#/components/schemas";

/// `task_id` → `TaskId`; empty underscore segments contribute
/// nothing. The exact `pascal` spelling of the #45 export-name rule.
fn pascal(text: &str) -> String {
    text.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The reusable-schema component name of one semantic id: the
/// PascalCase module segment followed by the PascalCase local
/// segment (`planner.task` → `PlannerTask`).
pub fn component_name(symbol: &str) -> String {
    let (module, local) = match symbol.split_once('.') {
        Some((module, local)) => (module, local),
        None => ("", symbol),
    };
    pascal(module) + &pascal(local)
}

/// Escape one pointer token per RFC 6901: `~` → `~0`, `/` → `~1`.
pub fn escape_pointer(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// The JSON pointer of one rendered operation:
/// `/paths/<escaped template>/<method>` (lowercase verb).
pub fn paths_pointer(template: &str, method: &str) -> String {
    format!(
        "/paths/{}/{}",
        escape_pointer(template),
        method.to_ascii_lowercase()
    )
}

/// The JSON pointer of one reusable schema component.
pub fn schemas_pointer(name: &str) -> String {
    format!("/components/schemas/{}", escape_pointer(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_names_follow_the_45_export_name_rule() {
        assert_eq!(component_name("planner.task"), "PlannerTask");
        assert_eq!(component_name("planner.task_id"), "PlannerTaskId");
        assert_eq!(
            component_name("planner.focus_conflict"),
            "PlannerFocusConflict"
        );
        assert_eq!(component_name("planner.task_state"), "PlannerTaskState");
    }

    #[test]
    fn pointer_tokens_escape_per_rfc6901() {
        assert_eq!(escape_pointer("tasks/{id}"), "tasks~1{id}");
        assert_eq!(escape_pointer("a~b/c"), "a~0b~1c");
        assert_eq!(
            paths_pointer("/tasks/{id}", "GET"),
            "/paths/~1tasks~1{id}/get"
        );
        assert_eq!(
            schemas_pointer("PlannerTask"),
            "/components/schemas/PlannerTask"
        );
    }

    #[test]
    fn empty_segments_contribute_nothing() {
        assert_eq!(component_name("planner."), "Planner");
        assert_eq!(component_name("task"), "Task");
    }
}

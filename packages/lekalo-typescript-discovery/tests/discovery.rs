use lekalo_typescript_discovery::{dependency_hints, node_scripts, node_workspace_members};
use serde_json::json;

#[test]
fn both_npm_workspace_shapes_keep_only_declared_nonempty_strings() {
    assert_eq!(
        node_workspace_members(&json!({"workspaces": ["apps/*", "", 1, "packages/*"]})),
        Some(vec!["apps/*".into(), "packages/*".into()])
    );
    assert_eq!(
        node_workspace_members(&json!({"workspaces": {"packages": ["api"]}})),
        Some(vec!["api".into()])
    );
    assert_eq!(node_workspace_members(&json!({"workspaces": false})), None);
    assert_eq!(
        node_workspace_members(&json!({"workspaces": {"unknown": []}})),
        None
    );
}

#[test]
fn known_dependency_hints_keep_their_source_fields() {
    let manifest = json!({"dependencies": {"react":"1", "unknown":"1"}, "devDependencies":{"typescript":"1"}, "peerDependencies":{"vue":"1"}, "scripts":{"react":"do-not-run"}});
    assert_eq!(
        dependency_hints(&manifest),
        [
            ("react", "framework", "dependencies"),
            ("typescript", "language", "devDependencies"),
            ("vue", "framework", "peerDependencies")
        ]
    );
    assert!(dependency_hints(&json!({"dependencies":["react"]})).is_empty());
}

#[test]
fn scripts_are_names_only_not_executable_values() {
    assert_eq!(
        node_scripts(&json!({"scripts":{"test":"do-not-execute", "build":{"unexpected":true}}})),
        ["build", "test"]
    );
    assert!(node_scripts(&json!({"scripts":["test"]})).is_empty());
}

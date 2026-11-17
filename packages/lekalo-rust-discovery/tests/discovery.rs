use lekalo_rust_discovery::{
    cargo_package_name_bytes, cargo_workspace_members, has_cargo_workspace,
};

#[test]
fn package_name_is_read_only_from_the_package_section() {
    assert_eq!(cargo_package_name_bytes(b"[dependencies]\nname = \"wrong\"\n[package]\nname = \"service\"\n[lib]\nname = \"wrong-again\"\n"), Some("service".into()));
    assert_eq!(cargo_package_name_bytes(b"[workspace]\n"), None);
    assert_eq!(cargo_package_name_bytes(&[255]), None);
}

#[test]
fn workspace_markers_are_line_evidence() {
    assert!(has_cargo_workspace(b"[workspace]\n"));
    assert!(has_cargo_workspace(b"[workspace.dependencies]\n"));
    assert!(!has_cargo_workspace(b"# [workspace]\n[package]\n"));
    assert!(!has_cargo_workspace(&[255]));
}

#[test]
fn member_line_scan_preserves_globs_and_host_owned_boundary_decisions() {
    let bytes = b"[workspace]\nmembers = [\n \"crates/*\", # note\n '../sibling',\n \"web\"\n]\n";
    assert_eq!(
        cargo_workspace_members(bytes, 64),
        ["crates/*", "../sibling", "web"]
    );
    assert_eq!(cargo_workspace_members(bytes, 1), ["crates/*"]);
    assert!(cargo_workspace_members(bytes, 0).is_empty());
    assert_eq!(
        cargo_workspace_members(b"members = [\"api\", \"web\"]", 64),
        ["api", "web"]
    );
    assert!(cargo_workspace_members(&[255], 64).is_empty());
}

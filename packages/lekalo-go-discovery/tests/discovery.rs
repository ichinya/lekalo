use lekalo_go_discovery::{go_module_path_bytes, go_work_members};

#[test]
fn module_declaration_is_data_even_when_followed_by_commands() {
    assert_eq!(
        go_module_path_bytes(
            b"// module fake\nmodule example.com/team/service\nreplace x => ./local\n"
        ),
        Some("example.com/team/service".into())
    );
    assert_eq!(go_module_path_bytes(b"// module fake\n"), None);
    assert_eq!(go_module_path_bytes(&[255]), None);
}

#[test]
fn use_block_keeps_declared_order_and_stops_at_the_bound() {
    let bytes = b"go 1.23\nuse (\n ./api // comment\n ./web\n ../sibling\n)\n ./ignored\n";
    assert_eq!(go_work_members(bytes, 64), ["api", "web", "../sibling"]);
    assert_eq!(go_work_members(bytes, 1), ["api"]);
    assert!(go_work_members(bytes, 0).is_empty());
}

#[test]
fn unknown_or_non_utf8_workspace_bytes_produce_no_members() {
    assert!(go_work_members(&[255], 64).is_empty());
    assert!(go_work_members(b"// use (\n./outside\n", 64).is_empty());
}

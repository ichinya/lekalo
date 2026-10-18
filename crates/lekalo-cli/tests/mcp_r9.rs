//! Round-9 credential-tail regressions through both real MCP output surfaces.
#[path = "mcp_harness.rs"]
mod mcp_harness;

use mcp_harness::{call, frame, lifecycle, ServedRoot};
use serde_json::json;
use std::fs;

fn probe(description: &str, sensitive: bool) {
    let root = ServedRoot::new();
    let model = root
        .root
        .join("planner/lekalo/modules/planner/commands.yaml");
    let original = fs::read_to_string(&model).unwrap();
    assert!(original.contains("\"Focus one task\""));
    fs::write(
        &model,
        original.replace(
            "\"Focus one task\"",
            &serde_json::to_string(description).unwrap(),
        ),
    )
    .unwrap();
    let frames = [
        lifecycle(),
        vec![
            call(
                10,
                "lekalo_inspect",
                json!({"projectId":"planner", "symbol":"planner.focus_task"}),
            ),
            frame(
                11,
                "resources/read",
                json!({"uri":"lekalo://planner/symbol/planner.focus_task"}),
            ),
        ],
    ]
    .concat();
    let inventory = root.inventory();
    let session = root.serve_with_catalog(Some("mcp.local.json"), &frames, &[]);
    assert_eq!(session.code, 0, "{}", session.stderr);
    if sensitive {
        let tool = session.tool(10);
        assert_eq!(tool["status"], "denied", "{}", session.stdout);
        assert_eq!(
            tool["failure"]["failureClass"], "sensitive-output",
            "{}",
            session.stdout
        );
        assert_eq!(
            session.response(11)["error"]["message"],
            "mcp.sensitive-output",
            "{}",
            session.stdout
        );
        assert!(!session.stdout.contains("R9CANARY"), "{}", session.stdout);
        assert!(!session.stderr.contains("R9CANARY"), "{}", session.stderr);
    } else {
        assert_eq!(session.tool(10)["status"], "valid", "{}", session.stdout);
        assert!(
            session.response(11)["result"].is_object(),
            "{}",
            session.stdout
        );
    }
    assert_eq!(root.inventory(), inventory, "screening stays read-only");
}

#[test]
fn credential_tails_after_structural_punctuation_refuse_on_both_surfaces() {
    for quote in ["\"", "'", "\\\""] {
        for tail in ["]R9CANARY", "}R9CANARY", ",R9CANARY", " } ], \tR9CANARY"] {
            probe(&format!("password={quote}{quote}{tail}"), true);
        }
    }
}

#[test]
fn complete_empty_credentials_still_ship_on_both_surfaces() {
    for description in [
        r#"password="""#,
        "password=''",
        r#"{"password":""}"#,
        r#"[{"password":""}]"#,
    ] {
        probe(description, false);
    }
}

//! Round-9–11 credential-tail regressions through both real MCP output surfaces.
#[path = "mcp_harness.rs"]
mod mcp_harness;

use mcp_harness::{call, frame, lifecycle, ServedRoot};
use serde_json::json;
use std::fs;

fn probe(description: &str, sensitive: bool) {
    probe_with_catalog(description, sensitive, Some("mcp.local.json"));
}

fn probe_with_catalog(description: &str, sensitive: bool, config: Option<&str>) {
    let root = ServedRoot::new();
    let project_id = if config.is_some() {
        "planner"
    } else {
        "project"
    };
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
                json!({"projectId":project_id, "symbol":"planner.focus_task"}),
            ),
            frame(
                11,
                "resources/read",
                json!({"uri":format!("lekalo://{project_id}/symbol/planner.focus_task")}),
            ),
        ],
    ]
    .concat();
    let inventory = root.inventory();
    let session = root.serve_with_catalog(config, &frames, &[]);
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
        for canary in ["R9CANARY", "R10CANARY", "R11CANARY"] {
            assert!(!session.stdout.contains(canary), "{}", session.stdout);
            assert!(!session.stderr.contains(canary), "{}", session.stderr);
        }
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
fn composite_and_structural_credential_prefixes_refuse_on_both_surfaces() {
    for prefix in ["[]", "{}", "[]{}", "{}[]", "]", "}", ",", " } ], \t"] {
        for key in [
            "password",
            "db_passwords",
            "client-secret",
            "awsAccessKeyId",
        ] {
            probe(&format!("{key}={prefix}R10CANARY"), true);
        }
        probe(&format!("password={prefix} }} ], \tR10CANARY"), true);
    }
}

#[test]
fn numeric_token_count_tails_refuse_on_both_surfaces() {
    for config in [None, Some("mcp.local.json")] {
        for tail in [
            "]R11CANARY",
            "}R11CANARY",
            ",R11CANARY",
            " } ], \tR11CANARY",
        ] {
            for (prefix, suffix) in [("", ""), ("{", "}"), ("[{", "}]")] {
                let description = format!(r#"{prefix}"tokens":123{tail}{suffix}"#);
                probe_with_catalog(&description, true, config);
            }
        }
        for description in [
            r#"{"tokens":123,"value":"R11CANARY"}"#,
            "tokens=123]R11CANARY",
            r#""tokens":123R11CANARY"#,
            "password=R11CANARY",
        ] {
            probe_with_catalog(description, true, config);
        }
    }
}

#[test]
fn complete_numeric_token_counts_ship_on_both_surfaces() {
    for config in [None, Some("mcp.local.json")] {
        for description in [
            r#""tokens":123,"#,
            r#""tokens":123}"#,
            r#""tokens":123]"#,
            r#"{"tokens":123}"#,
            r#"[{"tokens":123}]"#,
            "\"tokens\":123 } ], \t",
        ] {
            probe_with_catalog(description, false, config);
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
        "password=[]",
        "password={}",
        "password=[] } ], \t",
        "password={} } ], \t",
        "password=] }, \t",
        r#"{"password":[]}"#,
        r#"[{"password":{}}]"#,
    ] {
        probe(description, false);
    }
}

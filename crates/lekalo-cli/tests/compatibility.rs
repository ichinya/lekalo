//! Issue #9 CLI regression: `lekalo compatibility` projects the embedded
//! registry deterministically in human and JSON renderings.

use std::path::Path;
use std::process::{Command, Output};

/// The versioning fixture root holding the golden projection.
const GOLDEN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../tests/fixtures/versioning/compatibility.golden.json"
);

fn lekalo(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .output()
        .expect("run the real lekalo binary")
}

fn assert_exit(output: &Output, expected: i32) {
    assert_eq!(output.status.code(), Some(expected), "{output:?}");
}

fn stdout_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn json_projection_is_byte_identical_to_the_golden_in_both_flag_orders() {
    let golden = std::fs::read_to_string(Path::new(GOLDEN)).expect("golden fixture");
    for args in [
        vec!["--json", "compatibility"],
        vec!["compatibility", "--json"],
    ] {
        let output = lekalo(&args);
        assert_exit(&output, 0);
        assert!(output.stderr.is_empty());
        assert_eq!(
            stdout_text(&output),
            golden,
            "registry projection is golden"
        );
        assert!(golden.ends_with('\n'));
        assert!(!golden.contains("\r"));
    }
}

#[test]
fn human_projection_is_one_stable_line() {
    let output = lekalo(&["compatibility"]);
    assert_exit(&output, 0);
    assert!(output.stderr.is_empty());
    assert_eq!(
        stdout_text(&output),
        "compatibility: model current 1.0.0 (0.1.0..1.0.0), ir current 0.1.0, \
         protocol current 1.2.0\n"
    );
}

#[test]
fn the_projection_reports_exactly_the_three_families_in_order() {
    let output = lekalo(&["compatibility", "--json"]);
    assert_exit(&output, 0);
    let value: serde_json::Value =
        serde_json::from_str(stdout_text(&output).trim()).expect("envelope parses");
    assert_eq!(value["status"], "valid");
    assert_eq!(value["registryVersion"], "1.2.0");
    let families = value["families"].as_array().expect("families array");
    assert_eq!(families.len(), 3);
    assert_eq!(families[0]["family"], "model");
    assert_eq!(families[0]["current"], "1.0.0");
    assert_eq!(families[0]["min"], "0.1.0");
    assert_eq!(families[0]["max"], "1.0.0");
    assert_eq!(families[0]["versions"][0]["state"], "deprecated");
    assert_eq!(families[0]["versions"][1]["state"], "supported");
    assert_eq!(families[0]["aliases"][0]["alias"], "v1");
    assert_eq!(families[0]["migrations"][0]["id"], "model-0.1.0-to-1.0.0@1");
    assert_eq!(families[1]["family"], "ir");
    assert_eq!(families[1]["current"], "0.1.0");
    assert_eq!(families[1]["migrations"].as_array().map(Vec::len), Some(0));
    assert_eq!(families[2]["family"], "protocol");
    assert_eq!(families[2]["current"], "1.2.0");
    assert_eq!(families[2]["versions"].as_array().map(Vec::len), Some(3));
    assert_eq!(families[2]["versions"][0]["version"], "1.0.0");
    assert_eq!(families[2]["versions"][1]["version"], "1.1.0");
    assert_eq!(families[2]["versions"][2]["version"], "1.2.0");
    assert_eq!(families[2]["aliases"][0]["alias"], "v1");
}

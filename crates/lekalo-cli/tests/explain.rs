use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lekalo"))
        .args(args)
        .output()
        .expect("run checkout CLI")
}

#[test]
fn locale_is_explicit_and_changes_only_the_human_projection() {
    let en = run(&["explain", "LEK-FORMAT-001", "--locale", "en", "--json"]);
    let ru = run(&["explain", "LEK-FORMAT-001", "--locale", "ru", "--json"]);
    assert!(en.status.success());
    assert!(ru.status.success());
    assert_eq!(en.stdout, ru.stdout);
    assert!(en.stderr.is_empty());
    assert!(ru.stderr.is_empty());
    let human = run(&["explain", "LEK-FORMAT-001", "--locale", "ru"]);
    assert!(human.status.success());
    assert!(String::from_utf8(human.stdout)
        .unwrap()
        .contains("Проверка:"));
}

#[test]
fn provider_catalog_requires_report_and_unknown_codes_keep_their_exit() {
    assert_eq!(
        run(&[
            "explain",
            "LEK-FORMAT-001",
            "--provider-catalog",
            "missing.json"
        ])
        .status
        .code(),
        Some(1)
    );
    let unknown = run(&["explain", "LEK-TYPE-014", "--json"]);
    assert_eq!(unknown.status.code(), Some(4));
    let value: serde_json::Value = serde_json::from_slice(&unknown.stdout).unwrap();
    assert_eq!(
        value["reasonCodes"],
        serde_json::json!(["explain.code-unexplained"])
    );
}

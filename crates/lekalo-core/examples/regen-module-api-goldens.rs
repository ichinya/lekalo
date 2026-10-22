// One-shot goldens mint for the issue #113 module API family (r3): the
// digest projection, owner-evidence projection, and closure capsules
// changed, so every committed golden is re-emitted from the current core.
// Run: cargo run -p lekalo-core --example mint-module-api-goldens
use lekalo_core::ir::Compilation;
use lekalo_core::loader::LoadSelection;
use lekalo_core::module_api::{
    check, compare_public, consumer_snapshot, export_public, plan_consumer_context, resolve,
    resolve_lock,
};

fn compile(project: &str) -> Compilation {
    let model = lekalo_core::loader::normalize_model(&LoadSelection {
        project: Some(project.to_owned()),
    })
    .expect("fixture loads");
    lekalo_core::ir::compile(&model).expect("fixture compiles")
}

fn attachment(project: &str) -> Vec<u8> {
    std::fs::read(format!("{project}/module-api.json")).expect("attachment")
}

fn write_json(path: &str, value: &impl serde::Serialize) {
    std::fs::write(path, serde_json::to_vec_pretty(value).expect("serializes")).expect("write");
}

fn main() {
    let reference = "tests/fixtures/module-api/reference/tasks";
    let compilation = compile(reference);
    let resolved = resolve(&attachment(reference), &compilation).expect("resolves");
    write_json(
        "tests/fixtures/module-api/standalone/golden-bundle.json",
        &export_public(&resolved, &compilation, "tasks").expect("bundle"),
    );
    let verdict = check(&attachment(reference), &compilation, false, None).expect("check");
    write_json(
        "tests/fixtures/module-api/valid/golden-report.json",
        &verdict.report,
    );
    write_json(
        "tests/fixtures/module-api/standalone/golden-consumers.json",
        &consumer_snapshot(&resolved, &compilation, "tasks").expect("snapshot"),
    );

    let lock_project = "tests/fixtures/module-api/lock";
    let lock_compilation = compile(lock_project);
    let lock_resolved = resolve(&attachment(lock_project), &lock_compilation).expect("resolves");
    write_json(
        "tests/fixtures/module-api/lock/golden-lock.json",
        &resolve_lock(&lock_resolved, &lock_compilation, "tasks", "planner").expect("lock"),
    );

    let bundle_bytes =
        std::fs::read("tests/fixtures/module-api/standalone/golden-bundle.json").expect("bundle");
    let uses = vec!["tasks.task_view".to_owned(), "tasks.get_task".to_owned()];
    write_json(
        "tests/fixtures/module-api/standalone/golden-context.json",
        &plan_consumer_context(&bundle_bytes, "planner", &uses, 4000).expect("capsule"),
    );
    write_json(
        "tests/fixtures/module-api/standalone/golden-context-tiny.json",
        &plan_consumer_context(&bundle_bytes, "planner", &uses, 16).expect("tiny capsule"),
    );

    let mut sides = Vec::new();
    for side in ["before", "after"] {
        let project = format!("tests/fixtures/module-api/diff/{side}");
        let compilation = compile(&project);
        let resolved = resolve(&attachment(&project), &compilation).expect("resolves");
        let bundle = export_public(&resolved, &compilation, "tasks").expect("bundle");
        let snapshot = consumer_snapshot(&resolved, &compilation, "tasks").expect("snapshot");
        write_json(
            &format!("tests/fixtures/module-api/diff/golden-bundle-{side}.json"),
            &bundle,
        );
        write_json(
            &format!("tests/fixtures/module-api/diff/golden-consumers-{side}.json"),
            &snapshot,
        );
        sides.push((
            serde_json::to_vec(&bundle).expect("serializes"),
            serde_json::to_vec(&snapshot).expect("serializes"),
        ));
    }
    let (report, _) = compare_public(
        &sides[0].0,
        &sides[1].0,
        Some(&sides[0].1),
        Some(&sides[1].1),
    )
    .expect("diff");
    write_json(
        "tests/fixtures/module-api/diff/golden-diff.json",
        &serde_json::json!({ "status": "valid", "moduleApiDiff": report }),
    );
    println!("goldens minted");
}

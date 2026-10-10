//! Issue #8 IR library conformance: Rust-only compilation from the loader
//! seam (no CLI), semantic equality across JSON/YAML twins, deterministic
//! canonical bytes, source-map coverage per definition/field/reference, and
//! compile-time exhaustiveness of every closed enum.
//!
//! Selectors are invocation-relative and the #4 grammar rejects traversal,
//! so the fixture-driven assertions run under one sequential test that pins
//! the process working directory to the workspace root and restores it.

use lekalo_core::ir::{
    compile, CommandDef, CompiledProject, Definition, DefinitionKind, EffectDef, EffectOperation,
    Tombstone, TypeRef, IDENTITY,
};
use lekalo_core::loader::{normalize_model, LoadSelection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const FULL_KINDS: &str = "tests/fixtures/ir/valid-full-kinds";
const JSON_TWIN: &str = "tests/fixtures/ir/valid-json-twin";

/// Serializes every test that changes the process working directory.
static CWD_LOCK: Mutex<()> = Mutex::new(());

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("core crate lives under workspace/crates")
        .to_path_buf()
}

fn fixture(name: &str) -> String {
    name.to_owned()
}

fn compile_fixture(name: &str) -> CompiledProject {
    let selection = LoadSelection {
        project: Some(fixture(name)),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("{name}: load failed: {}", outcome.to_json_string()),
    };
    match compile(&model) {
        Ok(compilation) => compilation.project,
        Err(failure) => panic!(
            "{name}: IR failed: {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

#[test]
fn library_suite_runs_from_the_workspace_root() {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(workspace_root()).expect("enter workspace root");
    let result = std::panic::catch_unwind(|| {
        ir_compiles_as_a_library_without_the_cli();
        json_and_yaml_twins_are_semantically_and_byte_equal();
        repeated_compilations_are_deterministic();
        definitions_and_modules_are_sorted_by_semantic_id();
        every_definition_field_and_reference_has_a_source_entry();
        every_definition_and_effect_kind_is_exhaustively_matchable();
        typed_references_carry_fully_qualified_ids_and_explicit_wrappers();
    });
    std::env::set_current_dir(original).expect("restore working dir");
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn ir_compiles_as_a_library_without_the_cli() {
    let ir = compile_fixture(FULL_KINDS);
    assert_eq!(ir.model_version.as_str(), "0.2.16");
    assert_eq!(ir.modules.len(), 2);
    assert_eq!(ir.definitions.len(), 19);
    assert!(ir.project.is_some(), "project definition is present");
    assert_eq!(ir.project.as_ref().expect("project").id.as_str(), "planner");
}

fn json_and_yaml_twins_are_semantically_and_byte_equal() {
    let yaml_ir = compile_fixture(FULL_KINDS);
    let json_ir = compile_fixture(JSON_TWIN);
    assert_eq!(yaml_ir, json_ir, "semantic equality ignores syntax");
    assert_eq!(
        yaml_ir.to_canonical_json(),
        json_ir.to_canonical_json(),
        "canonical bytes are identical"
    );
}

fn repeated_compilations_are_deterministic() {
    let first = compile_fixture(FULL_KINDS).to_canonical_json();
    let second = compile_fixture(FULL_KINDS).to_canonical_json();
    assert_eq!(first, second);
    assert!(first.contains("\"contract\":\"dev.lekalo.ir@0.2.16\""));
    assert_eq!(IDENTITY, "dev.lekalo.ir@0.2.16");
}

fn definitions_and_modules_are_sorted_by_semantic_id() {
    let ir = compile_fixture(FULL_KINDS);
    for pair in ir.definitions.windows(2) {
        assert!(
            pair[0].id().as_str().as_bytes() <= pair[1].id().as_str().as_bytes(),
            "definitions are byte-sorted"
        );
    }
    for pair in ir.modules.windows(2) {
        assert!(
            pair[0].id.as_str().as_bytes() <= pair[1].id.as_str().as_bytes(),
            "modules are byte-sorted"
        );
    }
}

fn every_definition_field_and_reference_has_a_source_entry() {
    let selection = LoadSelection {
        project: Some(fixture(FULL_KINDS)),
    };
    let model = match normalize_model(&selection) {
        Ok(model) => model,
        Err(outcome) => panic!("load failed: {}", outcome.to_json_string()),
    };
    let compilation = match compile(&model) {
        Ok(compilation) => compilation,
        Err(failure) => panic!("IR failed: {failure:?}"),
    };
    let entries = compilation.source_map.entries();

    for (index, definition) in compilation.project.definitions.iter().enumerate() {
        let pointer = format!("/definitions/{index}");
        assert!(
            entries.iter().any(|entry| entry.pointer == pointer),
            "definition entry {pointer} exists"
        );
        match definition {
            Definition::Entity(entity) => {
                for position in 0..entity.fields.len() {
                    let field_pointer = format!("{pointer}/fields/{position}/type");
                    assert!(
                        entries.iter().any(|entry| entry.pointer == field_pointer),
                        "field type entry {field_pointer} exists"
                    );
                }
                for position in 0..entity.identity.len() {
                    let member = format!("{pointer}/identity/{position}");
                    assert!(
                        entries.iter().any(|entry| entry.pointer == member),
                        "identity member entry {member} exists"
                    );
                }
            }
            Definition::Command(command) => {
                for position in 0..command.effects.len() {
                    let member = format!("{pointer}/effects/{position}");
                    assert!(
                        entries.iter().any(|entry| entry.pointer == member),
                        "effects member entry {member} exists"
                    );
                }
            }
            Definition::Effect(effect) => {
                for position in 0..effect.emits.len() {
                    let member = format!("{pointer}/emits/{position}");
                    assert!(
                        entries.iter().any(|entry| entry.pointer == member),
                        "emits member entry {member} exists"
                    );
                }
            }
            Definition::Query(query) => {
                for position in 0..query.reads.len() {
                    let member = format!("{pointer}/reads/{position}");
                    assert!(
                        entries.iter().any(|entry| entry.pointer == member),
                        "reads member entry {member} exists"
                    );
                }
            }
            _ => {}
        }
    }
}

fn every_definition_and_effect_kind_is_exhaustively_matchable() {
    let ir = compile_fixture(FULL_KINDS);
    let mut kinds_seen = Vec::new();
    let mut effects_seen = Vec::new();
    for definition in &ir.definitions {
        // No wildcard arm: adding a kind without updating consumers breaks
        // this match at compile time.
        let kind = match definition {
            Definition::Scalar(_) => DefinitionKind::Scalar,
            Definition::Enum(_) => DefinitionKind::Enum,
            Definition::ValueObject(_) => DefinitionKind::ValueObject,
            Definition::Entity(_) => DefinitionKind::Entity,
            Definition::Command(_) => DefinitionKind::Command,
            Definition::Query(_) => DefinitionKind::Query,
            Definition::Policy(_) => DefinitionKind::Policy,
            Definition::Event(_) => DefinitionKind::Event,
            Definition::Effect(effect) => {
                // No wildcard arm here either: exhaustiveness is compile
                // time, the label just records which operation appeared.
                let label = match effect.operation {
                    EffectOperation::Create => "create",
                    EffectOperation::Update => "update",
                    EffectOperation::Delete => "delete",
                };
                effects_seen.push(label);
                DefinitionKind::Effect
            }
            Definition::Endpoint(_) => DefinitionKind::Endpoint,
            Definition::Scenario(_) => DefinitionKind::Scenario,
            Definition::TargetBinding(_) => DefinitionKind::TargetBinding,
        };
        assert_eq!(kind.as_str(), definition.kind().as_str());
        kinds_seen.push(kind);
    }
    for kind in [
        DefinitionKind::Scalar,
        DefinitionKind::Enum,
        DefinitionKind::ValueObject,
        DefinitionKind::Entity,
        DefinitionKind::Command,
        DefinitionKind::Query,
        DefinitionKind::Policy,
        DefinitionKind::Event,
        DefinitionKind::Effect,
        DefinitionKind::Endpoint,
        DefinitionKind::Scenario,
        DefinitionKind::TargetBinding,
    ] {
        assert!(
            kinds_seen.contains(&kind),
            "fixture covers every definition kind: missing {kind:?}"
        );
    }
    assert!(effects_seen.contains(&"create"));
    assert!(effects_seen.contains(&"update"));

    // The tombstone enum is also closed and exhaustively matchable.
    let registry = ir
        .project
        .as_ref()
        .and_then(|project| project.id_registry.as_ref())
        .expect("project registry");
    for tombstone in &registry.tombstones {
        match tombstone {
            Tombstone::Replaced { .. } => {}
            Tombstone::Deleted { .. } => {}
        }
    }
}

fn typed_references_carry_fully_qualified_ids_and_explicit_wrappers() {
    let ir = compile_fixture(FULL_KINDS);
    let command = ir
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::Command(command) if command.id.as_str() == "notify.notify_user" => {
                Some(command)
            }
            _ => None,
        });
    let command: &CommandDef = command.expect("cross-module command");
    let task = command
        .input
        .iter()
        .find(|field| field.name.as_str() == "task")
        .expect("task input");
    match &task.r#type {
        TypeRef::Ref(id) => assert_eq!(
            id.as_str(),
            "planner.task",
            "short references are fully qualified in the IR"
        ),
        other => panic!("expected a plain reference, got {other:?}"),
    }

    let effect = ir
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::Effect(effect) if effect.id.as_str() == "planner.create_task" => {
                Some(effect)
            }
            _ => None,
        });
    let effect: &EffectDef = effect.expect("effect");
    assert_eq!(effect.operation, EffectOperation::Create);
    assert_eq!(effect.entity.as_str(), "planner.task");
    assert_eq!(effect.emits.len(), 1);
}

// ---------------------------------------------------------------------------
// Model 0.8.0 successor: required members, semantic relation ownership.
// ---------------------------------------------------------------------------

use lekalo_core::loader::ModelVersion;
use std::collections::BTreeMap;

/// One disposable 0.8.0 project written under the crate's own `target`
/// directory, referenced by an invocation-relative selector (the #4 path
/// policy refuses absolute selections and `..` escapes). Every caller
/// holds [`CWD_LOCK`] for its whole body, so the selector never races a
/// working-directory change. `body` is the bare definition list of one
/// entities document; the wrapper adds the document envelope.
fn successor_project(name: &str, body: &str) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/ir-semantics")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("lekalo/modules/alpha")).expect("temp project dirs");
    std::fs::write(
        root.join("lekalo/project.yaml"),
        "schema_version: \"0.8.0\"\ndefinitions:\n  - id: alpha\n    kind: project\n    version: 1\n",
    )
    .expect("project.yaml");
    std::fs::write(
        root.join("lekalo/modules/alpha/module.yaml"),
        "schema_version: \"0.8.0\"\ndefinitions:\n  - id: alpha\n    kind: module\n    version: 1\n",
    )
    .expect("module.yaml");
    std::fs::write(
        root.join("lekalo/modules/alpha/entities.yaml"),
        format!("schema_version: \"0.8.0\"\ndefinitions:\n{body}"),
    )
    .expect("entities.yaml");
    format!("target/ir-semantics/{name}")
}

/// Compile one temp 0.8.0 project and require success.
fn assert_compiles(name: &str, entities_yaml: &str) {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let project = successor_project(name, entities_yaml);
    let model = normalize_model(&LoadSelection {
        project: Some(project),
    })
    .expect("load must succeed");
    assert_eq!(model.model_version, ModelVersion::Successor);
    if let Err(failure) = compile(&model) {
        panic!(
            "{name}: expected a valid compilation, got {}",
            failure
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.clone())
                .collect::<Vec<_>>()
                .join(",")
        );
    }
}

/// Compile one temp 0.8.0 project and require refusal by registered code.
fn assert_refused(name: &'static str, entities_yaml: &str, expected: &[&str]) {
    let _guard = CWD_LOCK.lock().expect("cwd lock");
    let project = successor_project(name, entities_yaml);
    let model = normalize_model(&LoadSelection {
        project: Some(project),
    })
    .expect("load must succeed");
    let Err(failure) = compile(&model) else {
        panic!("{name}: expected the compilation to fail");
    };
    let codes: BTreeMap<&str, usize> =
        failure
            .diagnostics
            .iter()
            .fold(BTreeMap::new(), |mut acc, diagnostic| {
                *acc.entry(diagnostic.code.as_str()).or_insert(0) += 1;
                acc
            });
    for code in expected {
        assert!(
            codes.contains_key(*code),
            "{name}: expected {code} among {codes:?}"
        );
    }
}

const THING_SCALARS: &str =
    "  - id: alpha.thing\n    kind: scalar\n    version: 1\n    base: string\n";

/// One entity with a single `id` field plus optional typed members.
fn entity(id: &str, relations: &str, constraints: &str) -> String {
    let relations = if relations.is_empty() {
        String::new()
    } else {
        format!("    relations:\n{relations}")
    };
    let constraints = if constraints.is_empty() {
        String::new()
    } else {
        format!("    uniqueConstraints:\n{constraints}")
    };
    format!(
        "  - id: {id}\n    kind: entity\n    version: 1\n    fields:\n      - name: id\n        type: alpha.thing\n        required: true\n    identity:\n      - id\n{relations}{constraints}"
    )
}

fn relation(target: &str, local: &str, target_field: &str) -> String {
    format!(
        "      - id: \"alpha.rel.thing\"\n        target: \"{target}\"\n        localFields:\n          - {local}\n        targetFields:\n          - {target_field}\n        min: 0\n        max: null\n        ownership: local\n        onDetach: retain-history\n        onDelete: restrict\n"
    )
}

#[test]
fn successor_relations_validate_against_their_semantic_owner() {
    // The coherent positive control: a typed join over equal types.
    let good = format!(
        "{}{}\n{}",
        THING_SCALARS,
        entity("alpha.left", &relation("alpha.right", "id", "id"), ""),
        entity("alpha.right", "", "")
    );
    assert_compiles("positive", &good);

    // A ghost relation target fails the whole compilation...
    let ghost_target = format!(
        "{}{}\n{}",
        THING_SCALARS,
        entity("alpha.left", &relation("alpha.ghost", "id", "id"), ""),
        entity("alpha.right", "", "")
    );
    assert_refused("ghost-target", &ghost_target, &["ir.value-invalid"]);

    // ...even when the malformed owner is otherwise unreferenced.
    let isolated = format!(
        "{}{}\n{}\n{}",
        THING_SCALARS,
        entity("alpha.left", "", ""),
        entity("alpha.isolated", &relation("alpha.ghost", "id", "id"), ""),
        entity("alpha.right", "", "")
    );
    assert_refused("isolated-ghost", &isolated, &["ir.value-invalid"]);

    // Ghost local and target fields fail; so do mismatched join types.
    let ghost_local = format!(
        "{}{}\n{}",
        THING_SCALARS,
        entity(
            "alpha.left",
            &relation("alpha.right", "ghost_field", "id"),
            ""
        ),
        entity("alpha.right", "", "")
    );
    assert_refused("ghost-local", &ghost_local, &["ir.value-invalid"]);

    let ghost_target_field = format!(
        "{}{}\n{}",
        THING_SCALARS,
        entity(
            "alpha.left",
            &relation("alpha.right", "id", "ghost_field"),
            ""
        ),
        entity("alpha.right", "", "")
    );
    assert_refused(
        "ghost-target-field",
        &ghost_target_field,
        &["ir.value-invalid"],
    );

    // A typed join over two different scalars cannot exist.
    let second_scalar = "  - id: alpha.other\n    kind: scalar\n    version: 1\n    base: uuid\n";
    let mismatched = format!(
        "{}{}{}",
        THING_SCALARS,
        second_scalar,
        "  - id: alpha.right\n    kind: entity\n    version: 1\n    fields:\n      - name: id\n        type: alpha.other\n        required: true\n    identity:\n      - id\n    relations:\n      - id: \"alpha.rel.thing\"\n        target: \"alpha.left\"\n        localFields:\n          - id\n        targetFields:\n          - id\n        min: 0\n        max: null\n        ownership: local\n        onDetach: retain-history\n        onDelete: restrict\n"
    );
    assert_refused("type-mismatch", &mismatched, &["ir.value-invalid"]);
}

#[test]
fn successor_constraints_validate_fields_and_normalization() {
    let scalars = format!(
        "{THING_SCALARS}  - id: alpha.norm\n    kind: scalar\n    version: 1\n    base: string\n"
    );
    let exact = "      - id: \"alpha.uq.thing\"\n        fields:\n          - id\n        comparison: exact-text\n        scope: \"alpha-record-id\"\n";
    let declared = "      - id: \"alpha.uq.thing\"\n        fields:\n          - id\n        comparison: declared-normalization\n        normalizationRef: alpha.norm\n        scope: \"alpha-record-id\"\n";
    assert_compiles(
        "constraint-positive",
        &format!("{scalars}{}", entity("alpha.record", "", exact)),
    );
    assert_compiles(
        "constraint-declared",
        &format!("{scalars}{}", entity("alpha.record", "", declared)),
    );

    let ghost_field = "      - id: \"alpha.uq.thing\"\n        fields:\n          - ghost_field\n        comparison: exact-text\n        scope: \"alpha-record-id\"\n";
    assert_refused(
        "constraint-ghost-field",
        &format!("{scalars}{}", entity("alpha.record", "", ghost_field)),
        &["ir.value-invalid"],
    );

    let ghost_normalization = "      - id: \"alpha.uq.thing\"\n        fields:\n          - id\n        comparison: declared-normalization\n        normalizationRef: alpha.ghost\n        scope: \"alpha-record-id\"\n";
    assert_refused(
        "constraint-ghost-normalization",
        &format!(
            "{scalars}{}",
            entity("alpha.record", "", ghost_normalization)
        ),
        &["ir.value-invalid"],
    );
}

#[test]
fn missing_required_members_fail_their_owning_definition() {
    // Every required relation member is diagnosed with its own
    // `ir.missing-field`, even on an otherwise unreferenced owner.
    let relation_members: [(&str, &str); 9] = [
        ("id", "        id: \"alpha.rel.thing\"\n"),
        ("target", "        target: \"alpha.right\"\n"),
        ("localFields", "        localFields:\n          - id\n"),
        ("targetFields", "        targetFields:\n          - id\n"),
        ("min", "        min: 0\n"),
        ("max", "        max: null\n"),
        ("ownership", "        ownership: local\n"),
        ("onDetach", "        onDetach: retain-history\n"),
        ("onDelete", "        onDelete: restrict\n"),
    ];
    for (missing, _) in relation_members.iter().enumerate() {
        let mut body = String::new();
        body.push_str(THING_SCALARS);
        body.push_str(&entity("alpha.left", "", ""));
        body.push('\n');
        body.push_str(&entity("alpha.right", "", ""));
        body.push_str("\n  - id: alpha.isolated\n    kind: entity\n    version: 1\n    fields:\n      - name: id\n        type: alpha.thing\n        required: true\n    identity:\n      - id\n    relations:\n");
        let mut first = true;
        for (index, (_, member)) in relation_members.iter().enumerate() {
            if index == missing {
                continue;
            }
            // The first surviving member opens the relation list item.
            if first {
                body.push_str("      - ");
                body.push_str(member.trim_start());
                first = false;
            } else {
                body.push_str(member);
            }
        }
        assert_refused(
            LEAKED_MISSING_RELATION[missing],
            &body,
            &["ir.missing-field"],
        );
    }

    // The required constraint members diagnose too.
    let constraint_members: [(&str, &str, &'static str); 4] = [
        (
            "id",
            "        id: \"alpha.uq.thing\"\n",
            "constraint-missing-id",
        ),
        (
            "fields",
            "        fields:\n          - id\n",
            "constraint-missing-fields",
        ),
        (
            "comparison",
            "        comparison: exact-text\n",
            "constraint-missing-comparison",
        ),
        (
            "scope",
            "        scope: \"alpha-record-id\"\n",
            "constraint-missing-scope",
        ),
    ];
    for (missing, _, label) in constraint_members.iter() {
        let mut body = String::new();
        body.push_str(THING_SCALARS);
        body.push_str(&entity("alpha.record", "", ""));
        body.push_str("\n    uniqueConstraints:\n");
        let mut first = true;
        for (key, member, _) in constraint_members.iter() {
            if key == missing {
                continue;
            }
            if first {
                body.push_str("      - ");
                body.push_str(member.trim_start());
                first = false;
            } else {
                body.push_str(member);
            }
        }
        assert_refused(label, &body, &["ir.missing-field"]);
    }

    // The nine-field bound mirrors the published schema exactly.
    let mut nine = String::new();
    nine.push_str(THING_SCALARS);
    nine.push_str("  - id: alpha.nine\n    kind: entity\n    version: 1\n    fields:\n");
    for index in 0..9 {
        nine.push_str(&format!(
            "      - name: field_{index}\n        type: alpha.thing\n        required: true\n"
        ));
    }
    nine.push_str("    identity:\n      - field_0\n    relations:\n      - id: \"alpha.rel.thing\"\n        target: \"alpha.left\"\n        localFields:\n");
    for index in 0..9 {
        nine.push_str(&format!("          - field_{index}\n"));
    }
    nine.push_str("        targetFields:\n");
    for index in 0..9 {
        nine.push_str(&format!("          - field_{index}\n"));
    }
    nine.push_str("        min: 0\n        max: null\n        ownership: local\n        onDetach: retain-history\n        onDelete: restrict\n");
    assert_refused("nine-fields", &nine, &["ir.value-invalid"]);
}

/// Static names for the parameterized missing-member refusals.
static LEAKED_MISSING_RELATION: [&str; 9] = [
    "missing-relation-id",
    "missing-relation-target",
    "missing-relation-localFields",
    "missing-relation-targetFields",
    "missing-relation-min",
    "missing-relation-max",
    "missing-relation-ownership",
    "missing-relation-onDetach",
    "missing-relation-onDelete",
];

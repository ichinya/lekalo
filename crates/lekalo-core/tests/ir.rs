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
    assert_eq!(ir.model_version.as_str(), "1.0.0");
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
    assert!(first.contains("\"contract\":\"dev.lekalo.ir@0.1.0\""));
    assert_eq!(IDENTITY, "dev.lekalo.ir@0.1.0");
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

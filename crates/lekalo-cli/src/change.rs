//! The `lekalo change` command surface (issue #73): clap syntax and
//! presentation only. The core owns every decision; sugar flags lower to
//! the same closed request DTO a JSON request file carries, so the CLI and
//! a future MCP tool consume byte-identical preparation and receipts.
//! Preview (`--dry-run`, the default) never writes; `--apply` requires an
//! idempotency key and runs the durable transaction.

use clap::{Args, Subcommand};
use lekalo_core::change::{self, ChangeError, PrepareOptions, ReceiptState};
use lekalo_core::DomainResult;
use serde_json::Value as Json;
/// The `change` subcommands: the P1 surface with the field, enum,
/// definition, effect, and signature verticals implemented; every
/// implemented verb has first-class sugar that lowers to the same closed
/// request DTO a JSON request file carries. Cataloged operations without
/// an executor refuse through the registered unsupported outcome.
#[derive(Debug, Subcommand)]
pub(crate) enum ChangeCommands {
    /// Add one field to an entity or value-object.
    #[command(name = "entity-field:add")]
    EntityFieldAdd(AddFieldArgs),
    /// Remove one field from an entity or value-object.
    #[command(name = "entity-field:remove")]
    EntityFieldRemove(RemoveFieldArgs),
    /// Rename one field of an entity or value-object in place.
    #[command(name = "entity-field:rename")]
    EntityFieldRename(RenameFieldArgs),
    /// Add one case to an enum definition.
    #[command(name = "enum-case:add")]
    EnumCaseAdd(AddCaseArgs),
    /// Rename or redescribe one existing enum case.
    #[command(name = "enum-case:change")]
    EnumCaseChange(ChangeCaseArgs),
    /// Add one command, query, event, policy, or scenario definition.
    #[command(name = "definition:add")]
    DefinitionAdd(AddDefinitionArgs),
    /// Append one effect reference to a command.
    #[command(name = "command-effect:add")]
    CommandEffectAdd(CommandEffectArgs),
    /// Remove one effect reference from a command.
    #[command(name = "command-effect:remove")]
    CommandEffectRemove(CommandEffectArgs),
    /// Replace a command's input field list.
    #[command(name = "operation-input:set")]
    OperationInputSet(OperationInputArgs),
    /// Replace a query's return type.
    #[command(name = "operation-output:set")]
    OperationOutputSet(OperationOutputArgs),
    /// Add one read dependency to a query.
    #[command(name = "query-effect:add")]
    QueryEffectAdd(QueryEffectArgs),
    /// Remove one read dependency from a query.
    #[command(name = "query-effect:remove")]
    QueryEffectRemove(QueryEffectArgs),
    /// Link one declared requirement identity to a symbol.
    #[command(name = "requirement:link")]
    RequirementLink(RequirementArgs),
    /// Unlink one requirement identity from a symbol.
    #[command(name = "requirement:unlink")]
    RequirementUnlink(RequirementArgs),
    /// Apply the registered model.entity-field-rename.v0.8.0 recipe.
    #[command(name = "migration:apply")]
    MigrationApply(MigrationArgs),
    /// Relocate a physical module directory, preserving its semantic ID.
    #[command(name = "module:move")]
    ModuleMove(ModuleMoveArgs),
    /// Move or rename a semantic symbol and rewrite its Model references.
    #[command(name = "symbol:move")]
    SymbolMove(SymbolMoveArgs),
    /// Change one module's mode in the pinned managed ownership policy.
    #[command(name = "ownership:set")]
    OwnershipSet(OwnershipArgs),
    /// Add a canonical target binding or explicitly confirm an observed proposal.
    #[command(name = "binding:add")]
    BindingAdd(BindingArgs),
    /// Replace a command/query error union in the pinned project registry.
    #[command(name = "operation-error:set")]
    OperationErrorSet(OperationErrorArgs),
    /// Run the complete JSON request surface from a file.
    Request(RequestArgs),
    /// Print the committed receipt of one idempotency key.
    Receipt {
        /// The durable idempotency key (only its digest is ever stored).
        #[arg(long, value_name = "KEY")]
        idempotency_key: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Initialize a checkout's local trust epoch. Existing records remain
    /// audit-only; only future local commits can replay. Never replaces a pin.
    CustodyInit {
        /// Digest of the exact reviewed canonical history bytes.
        #[arg(long, value_name = "DIGEST")]
        expect_history: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
    /// Recover one interrupted change transaction; readers stay blocked
    /// until this succeeds.
    Recover {
        /// The transaction id awaiting recovery (`change receipt`/status).
        #[arg(long, value_name = "ID")]
        transaction: String,
        /// Project root selector, relative to the invocation directory.
        #[arg(long, value_name = "DIR")]
        project: Option<String>,
    },
}

/// The shared execution flags of one mutating or previewing invocation.
#[derive(Debug, Args)]
pub(crate) struct ExecutionArgs {
    /// Prepare without writing (explicit; the default without `--apply`).
    #[arg(long, conflicts_with = "apply")]
    dry_run: bool,
    /// Apply through the durable transaction (requires `--idempotency-key`).
    #[arg(long, requires = "idempotency_key")]
    apply: bool,
    /// The durable idempotency key; required by `--apply`.
    #[arg(long, value_name = "KEY")]
    idempotency_key: Option<String>,
    /// The closed change-policy document.
    #[arg(long, value_name = "FILE")]
    policy: Option<String>,
    /// Explicitly accept reported breaking classes (recorded; never
    /// overrides malformed requests, invalid models, or denied evidence).
    #[arg(long)]
    allow_breaking: bool,
    /// Exact expected Model digest (`sha256:<64hex>`); a mismatch refuses.
    #[arg(long, value_name = "DIGEST")]
    expect_model: Option<String>,
    /// Exact expected plan identity (`sha256:<64hex>`); a mismatch refuses
    /// at apply. Every mutating verb accepts it (r6, finding omp F8): the
    /// sealed-plan pin is a common execution precondition, not a
    /// request-file-only one.
    #[arg(long, value_name = "DIGEST")]
    expect_plan: Option<String>,
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct MigrationArgs {
    /// Registered, versioned application-model recipe identity.
    #[arg(long, value_name = "ID")]
    step: String,
    /// Closed recipe parameters; field rename takes entity, name, and to.
    #[arg(long, value_name = "JSON", default_value = "{}")]
    parameters_json: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

#[derive(Debug, Args)]
pub(crate) struct ModuleMoveArgs {
    #[arg(long, value_name = "ID")]
    module: String,
    #[arg(long, value_name = "LEAF")]
    directory: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

#[derive(Debug, Args)]
pub(crate) struct SymbolMoveArgs {
    #[arg(long, value_name = "ID")]
    symbol: String,
    #[arg(long, value_name = "ID")]
    to_module: String,
    #[arg(long, value_name = "NAME")]
    to_name: Option<String>,
    #[command(flatten)]
    execution: ExecutionArgs,
}

#[derive(Debug, Args)]
pub(crate) struct OwnershipArgs {
    /// Canonical module ID; per-symbol ownership is not a module transition.
    #[arg(long, value_name = "ID")]
    module: String,
    #[arg(long, value_parser = ["observed", "contracted", "managed"])]
    mode: String,
    /// ownershipDigest plus exact coverage sources for a managed transition.
    #[arg(long, value_name = "JSON")]
    evidence_json: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

#[derive(Debug, Args)]
pub(crate) struct BindingArgs {
    /// New canonical binding ID, or existing observed symbol ID.
    #[arg(long, value_name = "ID")]
    symbol: String,
    /// Closed canonical-target-binding or observed-confirmation payload.
    #[arg(long, value_name = "JSON")]
    definition_json: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

#[derive(Debug, Args)]
pub(crate) struct OperationErrorArgs {
    #[arg(long, value_name = "ID")]
    operation: String,
    /// Closed errors and registryDigest payload.
    #[arg(long, value_name = "JSON")]
    definition_json: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `entity-field:add` sugar: typed flags, no guessed names.
#[derive(Debug, Args)]
pub(crate) struct AddFieldArgs {
    /// The semantic id of the entity or value-object.
    #[arg(long, value_name = "ID")]
    entity: String,
    /// The new field name.
    #[arg(long, value_name = "NAME")]
    name: String,
    /// The Model type sugar (`ref`, `list<ref>`, `ref?`).
    #[arg(long, value_name = "TYPE")]
    r#type: String,
    /// Mark the field required.
    #[arg(long)]
    required: bool,
    /// Bounded description text.
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    /// String/enum-literal default (requires the reviewed Model successor).
    #[arg(long, value_name = "LITERAL", conflicts_with = "default_json")]
    default: Option<String>,
    /// Typed JSON-literal default (requires the reviewed Model successor).
    #[arg(long, value_name = "JSON", conflicts_with = "default")]
    default_json: Option<String>,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `entity-field:remove` sugar.
#[derive(Debug, Args)]
pub(crate) struct RemoveFieldArgs {
    /// The semantic id of the entity or value-object.
    #[arg(long, value_name = "ID")]
    entity: String,
    /// The field name to remove.
    #[arg(long, value_name = "NAME")]
    name: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `entity-field:rename` sugar.
#[derive(Debug, Args)]
pub(crate) struct RenameFieldArgs {
    /// The semantic id of the entity or value-object.
    #[arg(long, value_name = "ID")]
    entity: String,
    /// The field name to rename.
    #[arg(long, value_name = "NAME")]
    name: String,
    /// The new field name.
    #[arg(long, value_name = "NAME")]
    to: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `enum-case:add` sugar.
#[derive(Debug, Args)]
pub(crate) struct AddCaseArgs {
    /// The semantic id of the enum.
    #[arg(long = "enum", value_name = "ID")]
    enum_id: String,
    /// The new case name.
    #[arg(long, value_name = "NAME")]
    name: String,
    /// Bounded description text.
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    /// Declared wire value (requires the reviewed Model successor).
    #[arg(long = "value-json", value_name = "JSON")]
    value_json: Option<String>,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `enum-case:change` sugar: rename and/or redescribe one case; the
/// closed surface requires at least one member (the core refuses an
/// empty change with the registered request-invalid rule).
#[derive(Debug, Args)]
pub(crate) struct ChangeCaseArgs {
    /// The semantic id of the enum.
    #[arg(long = "enum", value_name = "ID")]
    enum_id: String,
    /// The case name to change.
    #[arg(long, value_name = "NAME")]
    name: String,
    /// The new case name.
    #[arg(long, value_name = "NAME")]
    rename: Option<String>,
    /// The new bounded description text.
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `definition:add` sugar. The members the declared kind does not use
/// are refused by resolution, so only inapplicable flags need omitting.
#[derive(Debug, Args)]
pub(crate) struct AddDefinitionArgs {
    /// The definition kind: command, query, event, policy, or scenario.
    #[arg(long, value_name = "KIND")]
    kind: String,
    /// The owning module name.
    #[arg(long, value_name = "NAME")]
    module: String,
    /// The new definition name.
    #[arg(long, value_name = "NAME")]
    name: String,
    /// Bounded description text.
    #[arg(long, value_name = "TEXT")]
    description: Option<String>,
    /// Typed JSON array of input field declarations (command/query).
    #[arg(long, value_name = "JSON")]
    input_json: Option<String>,
    /// Effect references the command appends (repeatable).
    #[arg(long, value_name = "ID")]
    effects: Vec<String>,
    /// Read references the query appends (repeatable).
    #[arg(long, value_name = "ID")]
    reads: Vec<String>,
    /// The return type sugar (`ref`, `list<ref>`, `ref?`) of a query.
    #[arg(long, value_name = "TYPE")]
    returns: Option<String>,
    /// Typed JSON array of payload field declarations (event).
    #[arg(long, value_name = "JSON")]
    payload_json: Option<String>,
    /// Subjects the policy applies to (repeatable).
    #[arg(long, value_name = "ID")]
    applies_to: Vec<String>,
    /// The policy decision: allow or deny.
    #[arg(long, value_name = "DECISION")]
    decision: Option<String>,
    /// Bounded scenario summary text.
    #[arg(long, value_name = "TEXT")]
    summary: Option<String>,
    /// Requirements the scenario covers (repeatable).
    #[arg(long, value_name = "ID")]
    covers: Vec<String>,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `command-effect:add`/`command-effect:remove` sugar: exactly one of
/// the effect reference or the (deferred) emission surface.
#[derive(Debug, Args)]
pub(crate) struct CommandEffectArgs {
    /// The semantic id of the command.
    #[arg(long, value_name = "ID")]
    command: String,
    /// The effect reference to add or remove.
    #[arg(long, value_name = "ID", conflicts_with = "emit")]
    effect: Option<String>,
    /// The emission surface to bind (deferred to the reviewed successor).
    #[arg(long, value_name = "ID")]
    emit: Option<String>,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `operation-input:set` sugar: the complete replacement field list as
/// one typed JSON array of field declarations.
#[derive(Debug, Args)]
pub(crate) struct OperationInputArgs {
    /// The semantic id of the command whose input is replaced.
    #[arg(long = "operation", value_name = "ID")]
    operation_target: String,
    /// Typed JSON array of the complete new input field declarations.
    #[arg(long, value_name = "JSON")]
    input_json: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// `operation-output:set` sugar.
#[derive(Debug, Args)]
pub(crate) struct OperationOutputArgs {
    /// The semantic id of the query whose return type is replaced.
    #[arg(long = "operation", value_name = "ID")]
    operation_target: String,
    /// The new return type sugar (`ref`, `list<ref>`, `ref?`).
    #[arg(long, value_name = "TYPE")]
    r#type: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// The complete JSON request surface.
#[derive(Debug, Args)]
pub(crate) struct QueryEffectArgs {
    #[arg(long, value_name = "ID")]
    query: String,
    #[arg(long, value_name = "ID")]
    effect: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

#[derive(Debug, Args)]
pub(crate) struct RequirementArgs {
    #[arg(long, value_name = "ID")]
    symbol: String,
    #[arg(long, value_name = "ID")]
    requirement: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// The complete JSON request surface.
#[derive(Debug, Args)]
pub(crate) struct RequestArgs {
    /// Path of the change request document.
    #[arg(long, value_name = "FILE")]
    request: String,
    #[command(flatten)]
    execution: ExecutionArgs,
}

/// Parse one Model type sugar spelling into the closed request type ref.
fn parse_type_spelling(text: &str) -> Result<Json, DomainResult> {
    if let Some(inner) = text.strip_suffix('?') {
        let inner = parse_type_spelling(inner)?;
        return Ok(serde_json::json!({ "optional": inner }));
    }
    if let Some(inner) = text
        .strip_prefix("list<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        let inner = parse_type_spelling(inner)?;
        return Ok(serde_json::json!({ "list": inner }));
    }
    if !is_plain_reference(text) {
        return Err(ChangeError::RequestInvalid {
            detail: "type-grammar",
        }
        .into());
    }
    Ok(serde_json::json!({ "ref": text }))
}

/// Whether one spelling is a plain reference token (no sugar punctuation).
fn is_plain_reference(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 192
        && !text.chars().any(|c| {
            c.is_whitespace() || matches!(c, '<' | '>' | '?' | '{' | '}' | '[' | ']' | ',' | ':')
        })
}

/// Build the closed `entity-field:add` operation JSON from the sugar flags.
fn add_field_json(args: &AddFieldArgs) -> Result<Json, DomainResult> {
    let r#type = parse_type_spelling(&args.r#type)?;
    let mut field = serde_json::json!({ "name": args.name, "type": r#type });
    if args.required {
        field["required"] = Json::Bool(true);
    }
    if let Some(description) = &args.description {
        field["description"] = Json::String(description.clone());
    }
    if let Some(default) = &args.default {
        field["default"] = Json::String(default.clone());
    }
    if let Some(default_json) = &args.default_json {
        field["default"] = change::parse_strict_json_literal(default_json)?;
    }
    Ok(serde_json::json!({
        "operation": "entity-field:add",
        "entity": args.entity,
        "field": field,
    }))
}

/// Lower one sugar invocation to exact request bytes.
fn lower(command: &ChangeCommands) -> Result<Vec<u8>, DomainResult> {
    let operation = match command {
        ChangeCommands::EntityFieldAdd(args) => add_field_json(args)?,
        ChangeCommands::EntityFieldRemove(args) => serde_json::json!({
            "operation": "entity-field:remove",
            "entity": args.entity,
            "name": args.name,
        }),
        ChangeCommands::EntityFieldRename(args) => serde_json::json!({
            "operation": "entity-field:rename",
            "entity": args.entity,
            "name": args.name,
            "to": args.to,
        }),
        ChangeCommands::EnumCaseAdd(args) => {
            let mut case = serde_json::json!({ "name": args.name });
            if let Some(description) = &args.description {
                case["description"] = Json::String(description.clone());
            }
            if let Some(value_json) = &args.value_json {
                case["value"] = change::parse_strict_json_literal(value_json)?;
            }
            serde_json::json!({
                "operation": "enum-case:add",
                "enum": args.enum_id,
                "case": case,
            })
        }
        ChangeCommands::EnumCaseChange(args) => {
            let mut definition = serde_json::Map::new();
            if let Some(rename) = &args.rename {
                definition.insert("rename".to_owned(), Json::String(rename.clone()));
            }
            if let Some(description) = &args.description {
                definition.insert("description".to_owned(), Json::String(description.clone()));
            }
            serde_json::json!({
                "operation": "enum-case:change",
                "enum": args.enum_id,
                "name": args.name,
                "definition": definition,
            })
        }
        ChangeCommands::DefinitionAdd(args) => {
            let mut definition = serde_json::Map::new();
            definition.insert("name".to_owned(), Json::String(args.name.clone()));
            if let Some(description) = &args.description {
                definition.insert("description".to_owned(), Json::String(description.clone()));
            }
            if let Some(input_json) = &args.input_json {
                definition.insert(
                    "input".to_owned(),
                    change::parse_strict_json_literal(input_json)?,
                );
            }
            if !args.effects.is_empty() {
                definition.insert(
                    "effects".to_owned(),
                    Json::Array(
                        args.effects
                            .iter()
                            .map(|id| Json::String(id.clone()))
                            .collect(),
                    ),
                );
            }
            if !args.reads.is_empty() {
                definition.insert(
                    "reads".to_owned(),
                    Json::Array(
                        args.reads
                            .iter()
                            .map(|id| Json::String(id.clone()))
                            .collect(),
                    ),
                );
            }
            if let Some(returns) = &args.returns {
                definition.insert("returns".to_owned(), parse_type_spelling(returns)?);
            }
            if let Some(payload_json) = &args.payload_json {
                definition.insert(
                    "payload".to_owned(),
                    change::parse_strict_json_literal(payload_json)?,
                );
            }
            if !args.applies_to.is_empty() {
                definition.insert(
                    "appliesTo".to_owned(),
                    Json::Array(
                        args.applies_to
                            .iter()
                            .map(|id| Json::String(id.clone()))
                            .collect(),
                    ),
                );
            }
            if let Some(decision) = &args.decision {
                definition.insert("decision".to_owned(), Json::String(decision.clone()));
            }
            if let Some(summary) = &args.summary {
                definition.insert("summary".to_owned(), Json::String(summary.clone()));
            }
            if !args.covers.is_empty() {
                definition.insert(
                    "covers".to_owned(),
                    Json::Array(
                        args.covers
                            .iter()
                            .map(|id| Json::String(id.clone()))
                            .collect(),
                    ),
                );
            }
            serde_json::json!({
                "operation": "definition:add",
                "kind": args.kind,
                "module": args.module,
                "definition": definition,
            })
        }
        ChangeCommands::CommandEffectAdd(args) | ChangeCommands::CommandEffectRemove(args) => {
            let operation = match command {
                ChangeCommands::CommandEffectAdd(_) => "command-effect:add",
                _ => "command-effect:remove",
            };
            let mut effect = serde_json::json!({
                "operation": operation,
                "command": args.command,
            });
            if let Some(reference) = &args.effect {
                effect["effect"] = Json::String(reference.clone());
            }
            if let Some(emit) = &args.emit {
                effect["emit"] = Json::String(emit.clone());
            }
            effect
        }
        ChangeCommands::OperationInputSet(args) => serde_json::json!({
            "operation": "operation-input:set",
            "operationTarget": args.operation_target,
            "definition": {
                "input": change::parse_strict_json_literal(&args.input_json)?,
            },
        }),
        ChangeCommands::OperationOutputSet(args) => serde_json::json!({
            "operation": "operation-output:set",
            "operationTarget": args.operation_target,
            "type": parse_type_spelling(&args.r#type)?,
        }),
        ChangeCommands::QueryEffectAdd(args) | ChangeCommands::QueryEffectRemove(args) => {
            let operation = if matches!(command, ChangeCommands::QueryEffectAdd(_)) {
                "query-effect:add"
            } else {
                "query-effect:remove"
            };
            serde_json::json!({"operation":operation,"query":args.query,"effect":args.effect})
        }
        ChangeCommands::RequirementLink(args) | ChangeCommands::RequirementUnlink(args) => {
            let operation = if matches!(command, ChangeCommands::RequirementLink(_)) {
                "requirement:link"
            } else {
                "requirement:unlink"
            };
            serde_json::json!({"operation":operation,"symbol":args.symbol,"requirement":args.requirement})
        }
        ChangeCommands::MigrationApply(args) => serde_json::json!({
            "operation":"migration:apply", "step":args.step,
            "parameters":change::parse_strict_json_literal(&args.parameters_json)?,
        }),
        ChangeCommands::ModuleMove(args) => serde_json::json!({
            "operation":"module:move", "module":args.module, "directory":args.directory,
        }),
        ChangeCommands::OwnershipSet(args) => serde_json::json!({
            "operation":"ownership:set", "subject":args.module, "mode":args.mode,
            "evidence":change::parse_strict_json_literal(&args.evidence_json)?,
        }),
        ChangeCommands::SymbolMove(args) => {
            let mut operation = serde_json::json!({"operation":"symbol:move", "symbol":args.symbol, "toModule":args.to_module});
            if let Some(name) = &args.to_name {
                operation["toName"] = Json::String(name.clone());
            }
            operation
        }
        ChangeCommands::BindingAdd(args) => serde_json::json!({
            "operation":"binding:add", "symbol":args.symbol,
            "definition":change::parse_strict_json_literal(&args.definition_json)?,
        }),
        ChangeCommands::OperationErrorSet(args) => serde_json::json!({
            "operation":"operation-error:set", "operationTarget":args.operation,
            "definition":change::parse_strict_json_literal(&args.definition_json)?,
        }),
        _ => unreachable!("lower() handles sugar commands only"),
    };
    let mut request = serde_json::json!({
        "schemaVersion": "lekalo/change-request/v0.8.0",
        "operations": [operation],
    });
    let execution = match command {
        ChangeCommands::EntityFieldAdd(args) => &args.execution,
        ChangeCommands::EntityFieldRemove(args) => &args.execution,
        ChangeCommands::EntityFieldRename(args) => &args.execution,
        ChangeCommands::EnumCaseAdd(args) => &args.execution,
        ChangeCommands::EnumCaseChange(args) => &args.execution,
        ChangeCommands::DefinitionAdd(args) => &args.execution,
        ChangeCommands::CommandEffectAdd(args) => &args.execution,
        ChangeCommands::CommandEffectRemove(args) => &args.execution,
        ChangeCommands::OperationInputSet(args) => &args.execution,
        ChangeCommands::OperationOutputSet(args) => &args.execution,
        ChangeCommands::QueryEffectAdd(args) | ChangeCommands::QueryEffectRemove(args) => {
            &args.execution
        }
        ChangeCommands::RequirementLink(args) | ChangeCommands::RequirementUnlink(args) => {
            &args.execution
        }
        ChangeCommands::MigrationApply(args) => &args.execution,
        ChangeCommands::ModuleMove(args) => &args.execution,
        ChangeCommands::OwnershipSet(args) => &args.execution,
        ChangeCommands::SymbolMove(args) => &args.execution,
        ChangeCommands::BindingAdd(args) => &args.execution,
        ChangeCommands::OperationErrorSet(args) => &args.execution,
        _ => unreachable!("lower() handles sugar commands only"),
    };
    if let Some(expect_model) = &execution.expect_model {
        request["expectModel"] = Json::String(expect_model.clone());
    }
    serde_json::to_vec(&request)
        .map_err(|_| ChangeError::RequestInvalid { detail: "lowering" }.into())
}

/// The effective policy bytes of one invocation (None = restrictive default).
fn policy_bytes(execution: &ExecutionArgs) -> Result<Option<Vec<u8>>, DomainResult> {
    match &execution.policy {
        None => Ok(None),
        Some(path) => std::fs::read(path).map(Some).map_err(|_| {
            ChangeError::RequestInvalid {
                detail: "policy-read",
            }
            .into()
        }),
    }
}

/// Run one change command; the caller renders the result.
pub(crate) fn run(command: ChangeCommands) -> DomainResult {
    match command {
        ChangeCommands::Receipt {
            idempotency_key,
            project,
        } => match change::lookup_receipt(project.as_deref(), &idempotency_key) {
            Ok(Some(receipt)) => receipt_result(&receipt, receipt_state_human(&receipt)),
            Ok(None) => ChangeError::PreconditionFailed {
                subject: "idempotency-key".to_owned(),
                detail: "unknown",
            }
            .into(),
            Err(error) => error.into(),
        },
        ChangeCommands::CustodyInit {
            expect_history,
            project,
        } => match change::initialize_custody(project.as_deref(), &expect_history) {
            Ok(()) => receipt_result(
                &change::Receipt {
                    status: "valid".to_owned(),
                    state: "no-op".to_owned(),
                    plan_id: None,
                    request_digest: None,
                    key_digest: None,
                    operations: Vec::new(),
                    files: Vec::new(),
                    policy: None,
                    history: None,
                    transaction: None,
                    diagnostic_ids: Vec::new(),
                    replayed: false,
                },
                "local custody epoch initialized; earlier records remain audit-only",
            ),
            Err(error) => error.into(),
        },
        ChangeCommands::Recover {
            transaction,
            project,
        } => match change::recover_transaction(project.as_deref(), &transaction) {
            Ok(outcome) => {
                // The flat receipt document at the root, exactly like
                // `apply`/`receipt` (review finding 6): the durable
                // resolution is reported `rolled-back` or `committed` —
                // never a fabricated opposite — and the document validates
                // against the change-receipt contract (no wrapper, no
                // duplicate `status`).
                let receipt = serde_json::json!({
                    "status": "valid",
                    "schemaVersion": "lekalo/change-receipt/v0.8.0",
                    "identity": "dev.lekalo.change-receipt@0.8.0",
                    "state": outcome.state,
                    "planId": Json::Null,
                    "requestDigest": Json::Null,
                    "keyDigest": Json::Null,
                    "operations": [],
                    "files": outcome.files,
                    "policy": Json::Null,
                    "history": Json::Null,
                    "transaction": {
                        "id": transaction,
                        "state": outcome.state,
                    },
                    "diagnosticIds": [],
                    "replayed": false,
                });
                let human = if outcome.state == "committed" {
                    format!(
                        "recovered transaction {transaction}: durably committed, cleanup finished"
                    )
                } else {
                    format!("recovered transaction {transaction}: originals restored")
                };
                DomainResult::receipt(
                    serde_json::to_string_pretty(&receipt).unwrap_or_default(),
                    human,
                )
            }
            Err(error) => error.into(),
        },
        ChangeCommands::Request(args) => {
            let bytes = match std::fs::read(&args.request) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return ChangeError::RequestInvalid {
                        detail: "request-read",
                    }
                    .into()
                }
            };
            // The shared --expect-model flag merges into the JSON request
            // through the core's strict helper (round 3, finding 4): raw
            // bytes are duplicate-key-checked before any transformation,
            // so the flag can never launder a malformed request into an
            // accepted mutation; a declared disagreement refuses.
            let bytes = if args.execution.expect_model.is_some() {
                match change::merge_expect_model_flag(
                    &bytes,
                    args.execution.expect_model.as_deref().unwrap_or_default(),
                ) {
                    Ok(bytes) => bytes,
                    Err(error) => return error.into(),
                }
            } else {
                bytes
            };
            execute(
                &args.execution,
                &bytes,
                args.execution.expect_plan.as_deref(),
                "change request",
            )
        }
        sugar => match lower(&sugar) {
            Ok(bytes) => {
                let (execution, expect_plan) = match &sugar {
                    ChangeCommands::EntityFieldAdd(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::EntityFieldRemove(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::EntityFieldRename(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::EnumCaseAdd(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::EnumCaseChange(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::DefinitionAdd(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::CommandEffectAdd(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::CommandEffectRemove(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::OperationInputSet(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::OperationOutputSet(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::QueryEffectAdd(args)
                    | ChangeCommands::QueryEffectRemove(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::RequirementLink(args)
                    | ChangeCommands::RequirementUnlink(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::MigrationApply(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::ModuleMove(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::OwnershipSet(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::SymbolMove(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::BindingAdd(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    ChangeCommands::OperationErrorSet(args) => {
                        (&args.execution, args.execution.expect_plan.as_deref())
                    }
                    _ => unreachable!("sugar arm"),
                };
                execute(execution, &bytes, expect_plan, "change operation")
            }
            Err(error) => error,
        },
    }
}

/// Execute one lowered request: preview by default, apply with the flags.
fn execute(
    execution: &ExecutionArgs,
    request_bytes: &[u8],
    expect_plan: Option<&str>,
    human_subject: &str,
) -> DomainResult {
    let policy = match policy_bytes(execution) {
        Ok(policy) => policy,
        Err(error) => return error,
    };
    if execution.apply {
        let Some(key) = execution.idempotency_key.as_deref() else {
            return DomainResult::usage_error();
        };
        // Ledger-first apply: a repeated key with the same request returns
        // the original committed receipt even though the Model already
        // carries the change.
        match change::apply_request(&change::ApplyRequestOptions {
            project: execution.project.as_deref(),
            request: request_bytes,
            policy: policy.as_deref(),
            allow_breaking: execution.allow_breaking,
            idempotency_key: key,
            expect_plan,
        }) {
            Ok(receipt) => receipt_result(&receipt, format!("{human_subject} committed")),
            Err(error) => error.into(),
        }
    } else {
        let options = PrepareOptions {
            project: execution.project.as_deref(),
            request: request_bytes,
            policy: policy.as_deref(),
            allow_breaking: execution.allow_breaking,
        };
        match change::prepare(&options) {
            Ok(prepared) => preview_result(&prepared, human_subject),
            Err(error) => error.into(),
        }
    }
}

/// A preview success: the exact plan wire bytes (the closed document
/// carries its own `status`) plus the bounded human summary. An export
/// over the contract's own bounds refuses with the registered rule
/// (r6, finding omp F1).
fn preview_result(prepared: &change::PreparedChange, human_subject: &str) -> DomainResult {
    let plan = match prepared.to_json_string() {
        Ok(plan) => plan,
        Err(error) => return error.into(),
    };
    let human = format!(
        "{human_subject}: {} operation(s), {} file edit(s), equal={}",
        prepared.operations().len(),
        prepared.file_edits().len(),
        prepared.is_no_op()
    );
    DomainResult::receipt(plan, human)
}

/// A receipt success: the exact receipt wire bytes plus the summary.
fn receipt_result(receipt: &change::Receipt, human: impl std::fmt::Display) -> DomainResult {
    DomainResult::receipt(receipt.to_json_string(), human.to_string())
}

/// The stable human summary of one receipt state.
fn receipt_state_human(receipt: &change::Receipt) -> String {
    match receipt.state() {
        ReceiptState::Committed => "change committed".to_owned(),
        ReceiptState::Replayed => "change replayed (original committed receipt)".to_owned(),
        ReceiptState::NoOp => "change recorded as no-op".to_owned(),
        ReceiptState::Prepared => "change prepared".to_owned(),
        other => format!("change receipt state {}", other.as_str()),
    }
}

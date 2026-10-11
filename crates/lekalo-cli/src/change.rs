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
/// The `change` subcommands: the P1 surface with the field vertical
/// implemented; cataloged operations without an executor refuse through
/// the registered unsupported outcome.
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
    /// Project root selector, relative to the invocation directory.
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
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

/// The complete JSON request surface.
#[derive(Debug, Args)]
pub(crate) struct RequestArgs {
    /// Path of the change request document.
    #[arg(long, value_name = "FILE")]
    request: String,
    /// The expected plan identity; a mismatch refuses before mutation.
    #[arg(long, value_name = "DIGEST")]
    expect_plan: Option<String>,
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
                args.expect_plan.as_deref(),
                "change request",
            )
        }
        sugar => match lower(&sugar) {
            Ok(bytes) => {
                let execution = match &sugar {
                    ChangeCommands::EntityFieldAdd(args) => &args.execution,
                    ChangeCommands::EntityFieldRemove(args) => &args.execution,
                    ChangeCommands::EntityFieldRename(args) => &args.execution,
                    ChangeCommands::EnumCaseAdd(args) => &args.execution,
                    _ => unreachable!("sugar arm"),
                };
                execute(execution, &bytes, None, "change operation")
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
/// carries its own `status`) plus the bounded human summary.
fn preview_result(prepared: &change::PreparedChange, human_subject: &str) -> DomainResult {
    let plan = prepared.to_json_string();
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

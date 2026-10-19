//! Versioned diagnostic fix recipes (issue #74).
//!
//! Recipes are data, not scripts or patches. The only safe executor is the
//! formatter's structured, Model+IR-equal lowering. Other recipes lower to the
//! closed change DTO and cannot exceed their declared semantic scope. Preview
//! is pure; application requires the exact preview identity and reuses change's
//! writer lock, complete-input CAS, journal, custody and durable audit receipt.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as Json};

use crate::change::{self, ChangeError, PreparedChange};
use crate::diagnostics::normalize::{build, with_fixes};
use crate::diagnostics::types::token_value;
use crate::diagnostics::{
    DataObject, DiagnosticRegistry, DiagnosticSet, FixApplicability, FixId, SuggestedFix,
};
use crate::loader::{LoadSelection, ProjectLoad};
use crate::result::{DomainResult, Status};

pub const RECIPE_SCHEMA_VERSION: &str = "lekalo/fix-recipe/v0.8.0";
pub const RESULT_SCHEMA_VERSION: &str = "lekalo/fix-result/v0.8.0";
pub const MAX_RECIPE_BYTES: usize = 256 * 1024;

/// Closed risk classes; no severity or adapter assertion can authorize a write.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    Safe,
    Unsafe,
    Breaking,
    ManualOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Preconditions {
    pub source_fingerprint: Option<String>,
}

/// Semantic identities, not filesystem paths or regex match expressions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub operations: Vec<String>,
    pub subjects: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Operation {
    ModelFormat,
    ModelChange { request: Json },
    ManualOnly,
}

/// A reproducible, contract-bound descriptor. Null fingerprint is catalogue
/// metadata only; executable imported recipes must pin a source revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Recipe {
    pub schema_version: String,
    pub identity: String,
    pub id: String,
    pub version: String,
    pub class: Class,
    pub owner: String,
    pub target: Option<String>,
    pub diagnostic_codes: Vec<String>,
    pub explanation: String,
    pub preconditions: Preconditions,
    pub scope: Scope,
    pub operation: Operation,
    pub post_validation: String,
}

struct EmbeddedOnly;
impl jsonschema::Retrieve for EmbeddedOnly {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> Result<Json, Box<dyn std::error::Error + Send + Sync>> {
        if uri.as_str() == "https://dev.lekalo/change-request.schema.v0.8.0.json" {
            Ok(serde_json::from_slice(include_bytes!(
                "../../../../contracts/change-request.schema.v0.8.0.json"
            ))?)
        } else {
            Err("external schema retrieval is forbidden".into())
        }
    }
}

impl Recipe {
    pub fn decode(bytes: &[u8]) -> Result<Self, FixError> {
        if bytes.len() > MAX_RECIPE_BYTES {
            return Err(FixError::rule("fix.recipe-invalid", "over-limit"));
        }
        let value = change::request::unique_json(bytes)
            .map_err(|_| FixError::rule("fix.recipe-invalid", "json-shape"))?;
        static VALIDATOR: OnceLock<Result<jsonschema::Validator, ()>> = OnceLock::new();
        let validator = VALIDATOR
            .get_or_init(|| {
                let schema = serde_json::from_slice(include_bytes!(
                    "../../../../contracts/fix-recipe.schema.v0.8.0.json"
                ))
                .map_err(|_| ())?;
                jsonschema::options()
                    .with_draft(jsonschema::Draft::Draft202012)
                    .with_retriever(EmbeddedOnly)
                    .build(&schema)
                    .map_err(|_| ())
            })
            .as_ref()
            .map_err(|_| FixError::rule("fix.recipe-invalid", "embedded-schema"))?;
        if !validator.is_valid(&value) {
            return Err(FixError::rule("fix.recipe-invalid", "schema"));
        }
        let recipe: Self = serde_json::from_value(value)
            .map_err(|_| FixError::rule("fix.recipe-invalid", "shape"))?;
        recipe.check_authority()?;
        Ok(recipe)
    }

    fn check_authority(&self) -> Result<(), FixError> {
        let registry = DiagnosticRegistry::change_successor()
            .map_err(|_| FixError::rule("fix.recipe-invalid", "registry"))?;
        // Registry entries never acquire caller-supplied diagnostics or fixes.
        if self
            .diagnostic_codes
            .iter()
            .any(|code| !registry.entries().any(|entry| entry.code() == code))
        {
            return Err(FixError::rule(
                "fix.recipe-unavailable",
                "diagnostic-unregistered",
            ));
        }
        if self.owner == "core" {
            let mut unbound = self.clone();
            unbound.preconditions.source_fingerprint = None;
            if unbound != canonical_recipe() {
                return Err(FixError::rule(
                    "fix.recipe-unavailable",
                    "core-recipe-version",
                ));
            }
            return Ok(());
        }
        if !self.id.starts_with(&format!("{}.", self.owner)) {
            return Err(FixError::rule("fix.scope-denied", "recipe-namespace"));
        }
        if let Some(namespace) = self.owner.strip_prefix("adapter.") {
            if self.target.as_deref() != Some(namespace) {
                return Err(FixError::rule("fix.scope-denied", "adapter-target"));
            }
        } else if self.target.is_some() {
            return Err(FixError::rule("fix.scope-denied", "user-target"));
        }
        match (&self.operation, self.class, self.post_validation.as_str()) {
            (Operation::ManualOnly, Class::ManualOnly, "manual-only")
                if self.scope.operations.is_empty() && self.scope.subjects.is_empty() =>
            {
                Ok(())
            }
            (Operation::ModelChange { request }, Class::Unsafe | Class::Breaking, "validate") => {
                let request = change::ChangeRequest::decode(
                    &serde_json::to_vec(request)
                        .map_err(|_| FixError::rule("fix.recipe-invalid", "request"))?,
                )
                .map_err(ChangeError::from)?;
                // One audit operation joins the existing bounded batch.
                if request.operations.len() >= change::MAX_OPERATIONS {
                    return Err(FixError::rule("fix.recipe-invalid", "operation-limit"));
                }
                for operation in &request.operations {
                    if !self
                        .scope
                        .operations
                        .iter()
                        .any(|op| op == operation.operation())
                        || !operation
                            .subject()
                            .is_some_and(|subject| self.scope.subjects.iter().any(|s| s == subject))
                    {
                        return Err(FixError::rule("fix.scope-denied", "semantic-operation"));
                    }
                }
                // Contract-shape operations require the breaking class even
                // when compatibility calls the delta additive. Only the
                // explicit effect vertical can be implementation-only unsafe.
                if self.class == Class::Unsafe
                    && request.operations.iter().any(|operation| {
                        !matches!(
                            operation.operation(),
                            "command-effect:add" | "command-effect:remove"
                        )
                    })
                {
                    return Err(FixError::rule("fix.class-denied", "contract-operation"));
                }
                Ok(())
            }
            _ => Err(FixError::rule("fix.class-denied", "executor-class")),
        }
    }
}

pub fn canonical_recipe() -> Recipe {
    Recipe {
        schema_version: RECIPE_SCHEMA_VERSION.to_owned(), identity: "dev.lekalo.fix-recipe@0.8.0".to_owned(),
        id: "core.canonical-format".to_owned(), version: "1.0.0".to_owned(), class: Class::Safe,
        owner: "core".to_owned(), target: None,
        diagnostic_codes: vec!["LEK-FORMAT-001".to_owned(), "LEK-FIX-009".to_owned()],
        explanation: "Render admitted Model documents with the pinned canonical formatter; preserve comments and prove normalized Model and IR equality. Re-run the formatting diagnostic before committing.".to_owned(),
        preconditions: Preconditions { source_fingerprint: None },
        scope: Scope { operations: vec!["model:format".to_owned()], subjects: vec!["project".to_owned()] },
        operation: Operation::ModelFormat, post_validation: "format-check".to_owned(),
    }
}

/// The source fingerprint binds the entire admitted input inventory, including
/// byte-neutral edits, additions and removals, but not mutable audit history.
fn fingerprint(load: &ProjectLoad) -> String {
    let mut pins: Vec<_> = load
        .documents
        .iter()
        .map(|d| (d.document.path.clone(), digest(&d.bytes)))
        .collect();
    pins.sort();
    digest(&serde_json::to_vec(&pins).expect("pins serialize"))
}
fn digest(bytes: &[u8]) -> String {
    format!("sha256:{}", crate::digest::sha256_hex(bytes))
}

#[derive(Clone, Debug)]
pub enum Selection<'a> {
    Safe,
    Diagnostic(&'a str),
    Recipe(&'a [u8]),
}

#[derive(Clone, Debug)]
pub struct PrepareOptions<'a> {
    pub project: Option<&'a str>,
    pub selection: Selection<'a>,
    pub policy: Option<&'a [u8]>,
    /// Only unsafe recipes admit this acknowledgement; it is not a generic
    /// option to be silently ignored by safe or breaking recipes.
    pub allow_unsafe: bool,
    pub allow_breaking: bool,
}

/// Sealed in memory. A JSON preview is not executable patch authority.
#[derive(Clone, Debug)]
pub struct PreparedFix {
    recipe: Recipe,
    change: PreparedChange,
}
impl PreparedFix {
    pub fn recipe(&self) -> &Recipe {
        &self.recipe
    }
    pub fn plan_id(&self) -> String {
        self.change.plan_id()
    }
    pub fn is_no_op(&self) -> bool {
        self.change.is_no_op()
    }
    pub fn preview(&self) -> Result<Json, FixError> {
        Ok(report(
            "preview",
            true,
            vec![self.recipe.clone()],
            Some(self.plan_id()),
            Some(
                serde_json::from_str(&self.change.to_json_string()?)
                    .map_err(|_| FixError::rule("fix.recipe-invalid", "plan-export"))?,
            ),
            None,
            "not-run",
        ))
    }
}

fn select(selection: &Selection<'_>) -> Result<Recipe, FixError> {
    match selection {
        Selection::Safe => Ok(canonical_recipe()),
        Selection::Diagnostic(id)
            if [
                "core.canonical-format",
                "core.canonical-format@1.0.0",
                "LEK-FORMAT-001",
                "format.diff",
                "LEK-FIX-009",
                "fix.format-required",
            ]
            .contains(id) =>
        {
            Ok(canonical_recipe())
        }
        Selection::Diagnostic(_) => Err(FixError::rule("fix.recipe-unavailable", "selector")),
        Selection::Recipe(bytes) => Recipe::decode(bytes),
    }
}

fn admit_options(recipe: &Recipe, options: &PrepareOptions<'_>) -> Result<(), FixError> {
    if options.allow_unsafe && recipe.class != Class::Unsafe {
        return Err(FixError::rule("fix.class-denied", "unsafe-option"));
    }
    Ok(())
}

pub fn prepare(options: &PrepareOptions<'_>) -> Result<PreparedFix, FixError> {
    let mut recipe = select(&options.selection)?;
    if recipe.class == Class::ManualOnly {
        return Err(FixError::rule("fix.manual-only", "never-apply"));
    }
    admit_options(&recipe, options)?;
    let selection = LoadSelection {
        project: options.project.map(str::to_owned),
    };
    let load = crate::loader::load_with_snapshot(&selection).map_err(FixError::Load)?;
    let actual = fingerprint(&load);
    if let Some(expected) = &recipe.preconditions.source_fingerprint {
        if expected != &actual {
            return Err(FixError::rule(
                "fix.precondition-failed",
                "source-fingerprint",
            ));
        }
    } else if matches!(options.selection, Selection::Recipe(_)) {
        return Err(FixError::rule("fix.precondition-failed", "unbound-recipe"));
    }
    recipe.preconditions.source_fingerprint = Some(actual.clone());
    let recipe_bytes =
        serde_json::to_vec(&recipe).map_err(|_| FixError::rule("fix.recipe-invalid", "encode"))?;
    let audit = change::plan::ResolvedOperation {
        operation: "fix:apply".to_owned(),
        subject: load
            .model
            .project
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_default(),
        subject_family: "project".to_owned(),
        status: "resolved".to_owned(),
        detail: Some(format!(
            "{}@{};{}",
            recipe.id,
            recipe.version,
            digest(&recipe_bytes)
        )),
    };
    let prepared = match &recipe.operation {
        Operation::ModelFormat => change::source_fix::prepare_format(
            &load,
            recipe_bytes,
            audit,
            options.policy,
            options.allow_breaking,
        )?,
        Operation::ModelChange { request } => {
            let codes = validation_codes(&load)?;
            if !recipe
                .diagnostic_codes
                .iter()
                .all(|code| codes.contains(code))
            {
                return Err(FixError::rule(
                    "fix.precondition-failed",
                    "originating-diagnostic",
                ));
            }
            let bytes = serde_json::to_vec(request)
                .map_err(|_| FixError::rule("fix.recipe-invalid", "request"))?;
            let mut prepared = change::prepare(&change::PrepareOptions {
                project: options.project,
                request: &bytes,
                policy: options.policy,
                allow_breaking: options.allow_breaking,
            })?;
            let mut pins: Vec<_> = prepared
                .inputs
                .iter()
                .filter(|p| p.path != change::version::HISTORY_LOGICAL_PATH)
                .map(|p| (p.path.clone(), p.digest.clone()))
                .collect();
            pins.sort();
            if digest(&serde_json::to_vec(&pins).expect("pins serialize")) != actual {
                return Err(FixError::rule(
                    "fix.precondition-failed",
                    "preparation-race",
                ));
            }
            if recipe.class != Class::Breaking
                && prepared
                    .semantic_diff
                    .classification
                    .iter()
                    .any(|c| change::policy::BREAKING_CLASSES.contains(&c.as_str()))
            {
                return Err(FixError::rule("fix.class-denied", "breaking-delta"));
            }
            prepared.request_bytes = recipe_bytes;
            prepared.operations.push(audit);
            prepared
        }
        Operation::ManualOnly => return Err(FixError::rule("fix.manual-only", "never-apply")),
    };
    if !prepared.diff_complete || !prepared.semantic_diff.complete {
        return Err(FixError::rule(
            "fix.confirmation-required",
            "incomplete-preview",
        ));
    }
    // Force bounded export BEFORE a sealed executable fix can escape.
    prepared.to_json_string()?;
    Ok(PreparedFix {
        recipe,
        change: prepared,
    })
}

fn validation_codes(load: &ProjectLoad) -> Result<Vec<String>, FixError> {
    let compiled =
        crate::ir::compile(&load.model).map_err(|failure| FixError::Load(failure.into_result()))?;
    let profile = crate::validator::ValidationProfile::embedded_default()
        .map_err(|_| FixError::rule("fix.recipe-invalid", "profile"))?;
    let diagnostics = match crate::validator::validate(&compiled, profile) {
        Ok(report) => report.diagnostics().as_slice().to_vec(),
        Err(set) => set.as_slice().to_vec(),
    };
    Ok(diagnostics.iter().map(|d| d.code().to_owned()).collect())
}

#[derive(Clone, Debug)]
pub struct Confirmation<'a> {
    pub expect_plan: Option<&'a str>,
    pub allow_unsafe: bool,
    pub allow_breaking: bool,
    /// Bulk --safe selection can NEVER authorize a non-safe recipe.
    pub safe_only: bool,
}

pub fn apply(
    prepared: &PreparedFix,
    confirmation: &Confirmation<'_>,
) -> Result<Option<change::Receipt>, FixError> {
    confirm(prepared, confirmation)?;
    // Even no-op application must pass the shared writer/CAS gate; a sealed
    // no-op may have become stale. Its explicit audit receipt is idempotent
    // under the same plan key, just like a mutating fix.
    let recipe = &prepared.recipe;
    let key = format!("fix:{}", prepared.plan_id());
    let receipt = change::apply_with_validation(
        &prepared.change,
        &change::ApplyOptions {
            idempotency_key: &key,
            expect_plan: Some(&prepared.plan_id()),
            allow_breaking: confirmation.allow_breaking,
        },
        &|load| validate_origin(recipe, load),
    )?;
    Ok(Some(receipt))
}

fn confirm(prepared: &PreparedFix, confirmation: &Confirmation<'_>) -> Result<(), FixError> {
    let Some(expected) = confirmation.expect_plan else {
        return Err(FixError::rule("fix.confirmation-required", "expect-plan"));
    };
    if expected != prepared.plan_id() {
        return Err(FixError::rule("fix.conflict", "preview-identity"));
    }
    confirm_class(&prepared.recipe, confirmation)?;
    if confirmation.allow_breaking != prepared.change.allow_breaking {
        return Err(FixError::rule("fix.conflict", "preview-options"));
    }
    Ok(())
}

fn confirm_class(recipe: &Recipe, confirmation: &Confirmation<'_>) -> Result<(), FixError> {
    if confirmation.allow_unsafe && !matches!(recipe.class, Class::Unsafe | Class::ManualOnly) {
        return Err(FixError::rule("fix.class-denied", "unsafe-option"));
    }
    match recipe.class {
        Class::Safe => Ok(()),
        Class::Unsafe if !confirmation.safe_only && confirmation.allow_unsafe => Ok(()),
        Class::Breaking if !confirmation.safe_only && confirmation.allow_breaking => Ok(()),
        Class::ManualOnly => Err(FixError::rule("fix.manual-only", "never-apply")),
        _ => Err(FixError::rule(
            "fix.class-denied",
            "explicit-class-selection",
        )),
    }
}

fn validate_origin(recipe: &Recipe, load: &ProjectLoad) -> Result<(), ChangeError> {
    let compilation = crate::ir::compile(&load.model)
        .map_err(|failure| ChangeError::LoadFailed(failure.into_result()))?;
    let profile = crate::validator::ValidationProfile::embedded_default()
        .map_err(|_| ChangeError::RequestInvalid { detail: "profile" })?;
    let report = crate::validator::validate(&compilation, profile)
        .map_err(|set| ChangeError::LoadFailed(DomainResult::invalid(set)))?;
    let passed = match &recipe.operation {
        Operation::ModelFormat => {
            crate::format::fix_candidates(load).is_ok_and(|edits| edits.is_empty())
        }
        Operation::ModelChange { .. } => {
            !report.diagnostics().as_slice().iter().any(|diagnostic| {
                recipe
                    .diagnostic_codes
                    .iter()
                    .any(|code| code == diagnostic.code())
            })
        }
        Operation::ManualOnly => false,
    };
    if passed {
        Ok(())
    } else {
        Err(ChangeError::PreconditionFailed {
            subject: "recipe".to_owned(),
            detail: "originating-diagnostic",
        })
    }
}

#[derive(Clone, Debug)]
pub enum FixError {
    Rule {
        id: &'static str,
        detail: &'static str,
    },
    Change(ChangeError),
    Load(DomainResult),
}
impl From<ChangeError> for FixError {
    fn from(error: ChangeError) -> Self {
        Self::Change(error)
    }
}
impl FixError {
    fn rule(id: &'static str, detail: &'static str) -> Self {
        Self::Rule { id, detail }
    }
    pub fn into_result(self) -> DomainResult {
        match self {
            Self::Rule { id, detail } => {
                let status = if id == "fix.recipe-invalid" {
                    Status::Invalid
                } else if id == "fix.recipe-unavailable" {
                    Status::Unsupported
                } else {
                    Status::Denied
                };
                let mut data = DataObject::new();
                data.insert("detail".to_owned(), token_value(detail));
                let diagnostics = build(id, None, None, data).ok().into_iter().collect();
                let set = DiagnosticSet::try_from_unsorted(diagnostics, status)
                    .unwrap_or_else(|_| crate::result::fallback_set());
                match status {
                    Status::Invalid => DomainResult::invalid(set),
                    Status::Unsupported => DomainResult::unsupported_operation(set),
                    _ => DomainResult::denied(set),
                }
            }
            Self::Change(ChangeError::PreconditionFailed { .. }) => {
                Self::rule("fix.precondition-failed", "sealed-inputs").into_result()
            }
            Self::Change(ChangeError::IdempotencyConflict) => {
                Self::rule("fix.conflict", "audit-intent").into_result()
            }
            Self::Change(ChangeError::HistoryInvalid {
                detail: "postcondition",
            }) => Self::rule("fix.precondition-failed", "replay-postcondition").into_result(),
            Self::Change(error) => error.into_result(),
            Self::Load(result) => result,
        }
    }
}

fn report(
    state: &str,
    dry_run: bool,
    recipes: Vec<Recipe>,
    plan_id: Option<String>,
    plan: Option<Json>,
    receipt: Option<Json>,
    post: &str,
) -> Json {
    json!({"status": "valid", "schemaVersion": RESULT_SCHEMA_VERSION, "identity": "dev.lekalo.fix-result@0.8.0", "state": state, "planId": plan_id, "dryRun": dry_run, "recipes": recipes, "plan": plan, "receipt": receipt, "postValidation": post})
}
fn result(value: Json, human: String) -> DomainResult {
    DomainResult::receipt(
        serde_json::to_string_pretty(&value).expect("fix result serializes"),
        human,
    )
}

pub fn list() -> DomainResult {
    result(
        report(
            "listed",
            true,
            vec![canonical_recipe()],
            None,
            None,
            None,
            "not-run",
        ),
        "core.canonical-format@1.0.0 [safe]: explicit preview and confirmation required".to_owned(),
    )
}

/// CLI and future protocol consumers share this orchestration, not a patch API.
pub fn run(options: &PrepareOptions<'_>, confirmation: Option<&Confirmation<'_>>) -> DomainResult {
    // Manual-only remains explainable without loading or touching a project.
    if let Ok(recipe) = select(&options.selection) {
        if recipe.class == Class::ManualOnly {
            let mut value = report(
                "manual-only",
                confirmation.is_none(),
                vec![recipe.clone()],
                None,
                None,
                None,
                "not-run",
            );
            if confirmation.is_some() {
                value["status"] = json!("denied");
                let refusal = FixError::rule("fix.manual-only", "never-apply").into_result();
                let set =
                    DomainResult::from_wire_set(Status::Denied, refusal.diagnostics().to_vec());
                return DomainResult::denied_json(value.to_string(), recipe.explanation, set);
            }
            return result(value, recipe.explanation);
        }
        if let Err(error) = admit_options(&recipe, options) {
            return error.into_result();
        }
        if let Some(confirmation) = confirmation {
            if let Err(error) = confirm_class(&recipe, confirmation) {
                return error.into_result();
            }
            if confirmation.allow_breaking != options.allow_breaking {
                return FixError::rule("fix.conflict", "preview-options").into_result();
            }
            if let Some(expected) = confirmation.expect_plan {
                let recipe_digest = matches!(options.selection, Selection::Recipe(_))
                    .then(|| digest(&serde_json::to_vec(&recipe).expect("recipe serializes")));
                match change::source_fix::replay(
                    options.project,
                    expected,
                    &format!("{}@{};", recipe.id, recipe.version),
                    recipe_digest.as_deref(),
                    options.policy,
                    options.allow_breaking,
                    &|load| validate_origin(&recipe, load),
                ) {
                    Ok(Some(receipt)) => {
                        return result(
                            report(
                                "replayed",
                                false,
                                vec![recipe],
                                Some(expected.to_owned()),
                                None,
                                serde_json::from_str(&receipt.to_json_string()).ok(),
                                "passed",
                            ),
                            "fix replayed: original local audit receipt; no writes".to_owned(),
                        )
                    }
                    Ok(None) => {}
                    Err(error) => return FixError::from(error).into_result(),
                }
            }
        }
    }
    let prepared = match prepare(options) {
        Ok(prepared) => prepared,
        Err(error) => return error.into_result(),
    };
    let mut preview = match prepared.preview() {
        Ok(value) => value,
        Err(error) => return error.into_result(),
    };
    let Some(confirmation) = confirmation else {
        let human = format!("{}@{}: {} file edit(s); no writes\nplan: {}\nsemantic equal: {}\n{}\nApply only after review with --apply --expect-plan {}", prepared.recipe.id, prepared.recipe.version, prepared.change.file_edits().len(), prepared.plan_id(), prepared.change.semantic_diff.equal,
            prepared.change.file_diffs().iter().flat_map(|file| std::iter::once(format!("--- {}\n+++ {}", file.path, file.path)).chain(file.patch.iter().map(|line| format!("{}{}", match line.kind { "insert" => "+", "delete" => "-", _ => " " }, line.text)))).collect::<Vec<_>>().join("\n"), prepared.plan_id());
        if !prepared.is_no_op() && prepared.recipe.class == Class::Safe {
            let mut data = DataObject::new();
            data.insert(
                "detail".to_owned(),
                token_value("core.canonical-format@1.0.0"),
            );
            if let Ok(diagnostic) = build("fix.format-required", None, None, data) {
                let diagnostic = with_fixes(diagnostic, vec![SuggestedFix { fix_id: FixId::new("core.canonical-format").expect("static fix id"), applicability: FixApplicability::Safe, message: "Preview core.canonical-format@1.0.0 with lekalo fix --safe; applying requires the exact preview plan ID.".to_owned(), target: None }]);
                preview["diagnostics"] = json!([diagnostic]);
                preview["reasonCodes"] = json!(["fix.format-required"]);
            }
        }
        return result(preview, human);
    };
    match apply(&prepared, confirmation) {
        Ok(receipt) => {
            let state = receipt
                .as_ref()
                .map(|r| r.state.as_str())
                .unwrap_or("no-op");
            preview["state"] = json!(state);
            preview["dryRun"] = json!(false);
            preview["receipt"] = receipt
                .as_ref()
                .and_then(|r| serde_json::from_str::<Json>(&r.to_json_string()).ok())
                .unwrap_or(Json::Null);
            preview["postValidation"] = json!("passed");
            result(
                preview,
                format!(
                    "fix {state}: {}@{}; audit receipt {}",
                    prepared.recipe.id,
                    prepared.recipe.version,
                    if receipt.is_some() {
                        "persisted in lekalo/change-history.json"
                    } else {
                        "not needed (no writes)"
                    }
                ),
            )
        }
        Err(FixError::Change(ChangeError::CommitFailed {
            detail: "post-validation",
            ..
        })) => {
            preview["status"] = json!("denied");
            preview["state"] = json!("rolled-back");
            preview["dryRun"] = json!(false);
            preview["postValidation"] = json!("failed");
            let refusal =
                FixError::rule("fix.post-validation-failed", "originals-restored").into_result();
            let set = DomainResult::from_wire_set(Status::Denied, refusal.diagnostics().to_vec());
            DomainResult::denied_json(preview.to_string(), "fix rolled back: post-validation failed; original source, history and custody restored".to_owned(), set)
        }
        Err(error) => error.into_result(),
    }
}

#[cfg(test)]
mod tests;

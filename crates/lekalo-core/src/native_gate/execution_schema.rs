//! Independent closed schemas. External references never read host files or network.
use jsonschema::{Draft, Retrieve, Uri, Validator};
use serde_json::Value;
use std::sync::OnceLock;

struct EmbeddedOnly;
impl Retrieve for EmbeddedOnly {
    fn retrieve(
        &self,
        _uri: &Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("external schema retrieval is forbidden".into())
    }
}

pub(super) enum Document {
    Plan,
    PlanTools,
    Policy,
    Catalog,
    CatalogTools,
    Report,
    RunTools,
}

pub(super) fn valid(bytes: &[u8], document: Document) -> bool {
    static PLAN: OnceLock<Result<Validator, ()>> = OnceLock::new();
    static POLICY: OnceLock<Result<Validator, ()>> = OnceLock::new();
    static CATALOG: OnceLock<Result<Validator, ()>> = OnceLock::new();
    static PLAN_TOOLS: OnceLock<Result<Validator, ()>> = OnceLock::new();
    static CATALOG_TOOLS: OnceLock<Result<Validator, ()>> = OnceLock::new();
    static RUN_TOOLS: OnceLock<Result<Validator, ()>> = OnceLock::new();
    static REPORT: OnceLock<Result<Validator, ()>> = OnceLock::new();
    let (cache, schema): (_, &[u8]) = match document {
        Document::RunTools => (
            &RUN_TOOLS,
            include_bytes!("../../../../contracts/native-gate-run.schema.v1.0.0.json"),
        ),
        Document::PlanTools => (
            &PLAN_TOOLS,
            include_bytes!("../../../../contracts/native-execution-plan.schema.v1.0.0.json"),
        ),
        Document::CatalogTools => (
            &CATALOG_TOOLS,
            include_bytes!("../../../../contracts/native-tool-catalog.schema.v1.0.0.json"),
        ),
        Document::Report => (
            &REPORT,
            include_bytes!("../../../../contracts/native-execution-report.schema.v1.0.0.json"),
        ),
        Document::Plan => (
            &PLAN,
            include_bytes!("../../../../contracts/native-execution-plan.schema.v0.9.1.json"),
        ),
        Document::Policy => (
            &POLICY,
            include_bytes!("../../../../contracts/native-execution-policy.schema.v0.9.1.json"),
        ),
        Document::Catalog => (
            &CATALOG,
            include_bytes!("../../../../contracts/native-tool-catalog.schema.v0.9.1.json"),
        ),
    };
    let validator = cache.get_or_init(|| {
        let schema: Value = serde_json::from_slice(schema).map_err(|_| ())?;
        jsonschema::options()
            .with_draft(Draft::Draft202012)
            .with_retriever(EmbeddedOnly)
            .build(&schema)
            .map_err(|_| ())
    });
    let value = std::str::from_utf8(bytes)
        .ok()
        .and_then(|text| crate::classification::parse_checked(text).ok());
    match (validator, value) {
        (Ok(validator), Some(value)) => validator.is_valid(&value),
        _ => false,
    }
}

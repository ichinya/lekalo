//! Issue #72 golden regenerator: emits the committed client-SDK
//! projection golden from the valid planner transport attachment
//! joined with the fixture project, the embedded #62 registry, and
//! the bound #64 query-model attachment. Run from the repository
//! root:
//!
//!     cargo run -p lekalo-core --example regen-client-sdk -- .
//!
//! The script writes only
//! `tests/fixtures/client-sdk/golden/planner.expect.json` (compact
//! canonical bytes, no trailing LF). Normal tests compare against the
//! committed golden and never update it.

use lekalo_core::client_sdk::{project, ClientConfig};
use lekalo_core::error_contract::ErrorRegistry;
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::query_model::QueryModelAttachment;
use lekalo_core::transport_http::{
    CapabilityMap, TransportDocument, ValidationContext,
};

fn main() {
    let root = std::env::args().nth(1).expect("repo root");
    let fixture_root = format!("{root}/tests/fixtures/transport-http");

    let selection = LoadSelection {
        project: Some(format!("{fixture_root}/project")),
    };
    let model = normalize_model(&selection).expect("fixture load");
    let compilation = lekalo_core::ir::compile(&model).expect("fixture compile");
    let registry = ErrorRegistry::embedded().expect("embedded registry");
    let query_model_value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{fixture_root}/query-model.json")).expect("fixture"),
    )
    .expect("json");
    let query_model = QueryModelAttachment::from_value(&query_model_value).expect("attachment");
    let document_value: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{fixture_root}/valid/planner.transport.json"))
            .expect("fixture"),
    )
    .expect("json");
    let document = TransportDocument::from_value(&document_value).expect("attachment");
    let capabilities = CapabilityMap::http_json();
    let context = ValidationContext::new(&compilation.project)
        .with_errors(registry)
        .with_query_model(&query_model)
        .with_capabilities(&capabilities);

    let contract = project(&document, &context, &ClientConfig::generated())
        .expect("projection");
    let bytes = contract.canonical_bytes().expect("canonical bytes");
    let digest = contract.digest().expect("digest");

    let out_dir = format!("{root}/tests/fixtures/client-sdk/golden");
    std::fs::create_dir_all(&out_dir).expect("golden dir");
    std::fs::write(format!("{out_dir}/planner.expect.json"), bytes).expect("write golden");
    std::fs::write(
        format!("{out_dir}/planner.expect.digest.txt"),
        format!("{}\n", digest.as_str()),
    )
    .expect("write digest");
    println!("wrote {out_dir}/planner.expect.json digest {}", digest.as_str());
}

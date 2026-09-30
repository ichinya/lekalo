//! Issue #53 regenerator: re-pins the staged routes-fixture evidence
//! of the php-laravel planning screen from one validated join. Run
//! from the repository root after touching the routes model home, its
//! transport attachment, or the embedded #62 registry:
//!
//!     cargo run -p lekalo-core --example regen-planner-routes -- .
//!
//! The script loads `tests/fixtures/php-laravel/routes/model`, prints
//! the canonical Model and IR digests (the values every committed
//! attachment re-pins), validates the staged transport attachment
//! against the compilation plus the embedded registry and the
//! `lekalo/query-model.yaml` home, then rewrites — as canonical bytes
//! plus one trailing newline — the committed transport, OpenAPI, and
//! client-SDK evidence under `tests/fixtures/php-laravel/routes/
//! inputs/`. It never writes anywhere else.

use lekalo_core::client_sdk::{project, source::read_query_model, ClientConfig};
use lekalo_core::digest::sha256_hex;
use lekalo_core::error_contract::ErrorRegistry;
use lekalo_core::loader::{normalize_model, LoadSelection};
use lekalo_core::openapi::{render, RenderConfig};
use lekalo_core::transport_http::{CapabilityMap, TransportDocument, ValidationContext};

fn envelope_digest(selection: &LoadSelection) -> String {
    match lekalo_core::loader::run(selection, false) {
        lekalo_core::DomainResult::Valid {
            payload: lekalo_core::result::SuccessPayload::Model { json, .. },
            ..
        } => format!("sha256:{}", sha256_hex(json.as_bytes())),
        other => panic!("model envelope: {:?}", other.status()),
    }
}

fn compilation_of(model: &lekalo_core::loader::NormalizedModel) -> lekalo_core::ir::Compilation {
    lekalo_core::ir::compile(model).expect("ir compile")
}

fn main() {
    let root = std::env::args().nth(1).expect("repo root");
    let model_dir = format!("{root}/tests/fixtures/php-laravel/routes/model");
    let inputs = format!("{root}/tests/fixtures/php-laravel/routes/inputs");

    // The compiled Model and IR digests every attachment re-pins.
    let selection = LoadSelection {
        project: Some(model_dir.clone()),
    };
    let model = normalize_model(&selection).expect("model load");
    let compilation = compilation_of(&model);
    let ir_digest = format!(
        "sha256:{}",
        sha256_hex(compilation.project.to_canonical_json().as_bytes())
    );
    // The Model-custody digest: the exact `lekalo load` envelope bytes
    // the requirements and query-model attachments pin.
    let model_digest = envelope_digest(&selection);
    println!("model digest: {model_digest}");
    println!("ir digest: {ir_digest}");

    // The validated transport join (issue #70) over the staged bytes.
    let transport_bytes =
        std::fs::read_to_string(format!("{inputs}/transport.json")).expect("transport input");
    let transport_value: serde_json::Value =
        serde_json::from_str(&transport_bytes).expect("transport json");
    let document = TransportDocument::from_value(&transport_value).expect("transport attachment");
    let registry = ErrorRegistry::embedded().expect("embedded registry");
    let capabilities = CapabilityMap::http_json();
    let query_model = read_query_model(std::path::Path::new(&model_dir))
        .expect("query-model home readable")
        .expect("query-model home present");

    let full_context = ValidationContext::new(&compilation.project)
        .with_errors(registry)
        .with_query_model(&query_model)
        .with_capabilities(&capabilities);
    lekalo_core::transport_http::validate(&document, &full_context).expect("transport valid");

    let canonical = document.canonical_bytes().expect("canonical bytes");
    std::fs::write(format!("{inputs}/transport.json"), format!("{canonical}\n"))
        .expect("transport write");

    // The #46 OpenAPI projection of the same join (defaults, the
    // embedded registry bound for identity variants — the exact
    // preflight posture of `lekalo generate`).
    let openapi_context = ValidationContext::new(&compilation.project).with_errors(registry);
    let rendered =
        render(&document, &openapi_context, &RenderConfig::new()).expect("openapi render");
    std::fs::write(
        format!("{inputs}/openapi/planner.openapi.json"),
        format!("{}\n", rendered.canonical_bytes()),
    )
    .expect("openapi write");

    // The #72 client-SDK projection of the same join.
    let sdk = project(&document, &full_context, &ClientConfig::generated())
        .expect("client-sdk projection");
    std::fs::write(
        format!("{inputs}/client-sdk/planner.json"),
        format!("{}\n", sdk.canonical_bytes().expect("canonical bytes")),
    )
    .expect("client-sdk write");
    println!(
        "operations: {} types: {}",
        sdk.operations().len(),
        sdk.types().len()
    );
}

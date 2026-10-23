//! Successor signatures must survive the real transport/OpenAPI/SDK joins.
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use lekalo_core::client_sdk::{self, ClientConfig};
use lekalo_core::error_contract::ErrorRegistry;
use lekalo_core::ir;
use lekalo_core::loader::{self, LoadSelection};
use lekalo_core::openapi::{self, DocumentVersion, RenderConfig};
use lekalo_core::query_model::QueryModelAttachment;
use lekalo_core::transport_http::{self, CapabilityMap, TransportDocument, ValidationContext};
use serde_json::{json, Value};

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("sha256:{}", lekalo_core::digest::sha256_hex(bytes.as_ref()))
}

fn rebind(wire: &mut Value, query: &mut Value) -> (loader::NormalizedModel, ir::CompiledProject) {
    let model = loader::normalize_model(&LoadSelection {
        project: Some(".".to_owned()),
    })
    .unwrap();
    let project = ir::compile(&model).unwrap().project;
    for document in [wire, query] {
        document["modelRef"]["digest"] = json!(digest(
            loader::render_load_output(&model, false).to_json_string()
        ));
        document["irRef"]["digest"] = json!(digest(project.to_canonical_json()));
    }
    (model, project)
}

struct RestoreCwd(std::path::PathBuf);
impl Drop for RestoreCwd {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

#[test]
fn successor_query_inputs_bind_exactly_and_project_without_flattening() {
    let root = tempfile::tempdir().unwrap();
    let _cwd = RestoreCwd(std::env::current_dir().unwrap());
    std::env::set_current_dir(root.path()).unwrap();
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/change/field-vertical"),
        root.path(),
    );
    for file in [
        "lekalo/project.yaml",
        "lekalo/modules/tasks/module.yaml",
        "lekalo/modules/tasks/entities.yaml",
    ] {
        let path = root.path().join(file);
        fs::write(
            &path,
            fs::read_to_string(&path)
                .unwrap()
                .replace("0.2.16", "0.8.0"),
        )
        .unwrap();
    }
    fs::write(root.path().join("lekalo/modules/tasks/queries.yaml"), "schema_version: '0.8.0'\ndefinitions:\n  - id: tasks.find\n    kind: query\n    version: 1\n    input:\n      - name: title\n        type: tasks.text\n        required: true\n      - name: tags\n        type: 'list<tasks.text>?'\n    reads:\n      - tasks.task\n    returns: tasks.task\n").unwrap();
    fs::write(root.path().join("lekalo/modules/tasks/bindings.yaml"), "schema_version: '0.8.0'\ndefinitions:\n  - id: tasks.find_http\n    kind: endpoint\n    version: 1\n    method: GET\n    path: /tasks\n    invokes: tasks.find\n").unwrap();
    let model = loader::normalize_model(&LoadSelection {
        project: Some(".".to_owned()),
    })
    .unwrap();
    let project = ir::compile(&model).unwrap().project;
    let model_pin = json!({"modelVersion":"0.8.0", "digest":digest(loader::render_load_output(&model, false).to_json_string())});
    let ir_pin =
        json!({"identity":"dev.lekalo.ir@0.8.0", "digest":digest(project.to_canonical_json())});
    let query_wire = json!({
        "schemaVersion":"lekalo/query-model/v0.8.0", "identity":"dev.lekalo.query-model@0.8.0", "attachmentRevision":"1.0.0", "projectId":"tasks", "modelRef":model_pin, "irRef":ir_pin,
        "tenancy":[], "queries":[{"query":"tasks.find", "source":"tasks.task", "cardinality":"one", "consistency":"strong", "parameters":[{"name":"title", "type":"tasks.text", "required":true},{"name":"tags", "type":"list<tasks.text>?", "required":false}]}]
    });
    let query = QueryModelAttachment::from_value(&query_wire).unwrap();
    let wire = json!({
        "schemaVersion":"lekalo/transport-http/v0.8.0", "identity":"dev.lekalo.transport-http@0.8.0", "attachmentRevision":"1.0.0", "projectId":"tasks", "modelRef":model_pin, "irRef":ir_pin,
        "wire":{"dialect":"lekalo-http-wire/v1", "contentType":"application/json"},
        "defaults":{"errorEnvelope":"canonical-v1", "idempotencyHeader":"Idempotency-Key", "correlationHeaders":["X-Request-Id"]}, "securitySchemes":[],
        "endpoints":[{"endpoint":"tasks.find_http", "params":[{"name":"title", "in":"query", "field":"title", "required":true},{"name":"tags", "in":"query", "field":"tags", "required":false, "style":"form", "explode":true}], "success":{"status":200,"body":{"mode":"whole-output"}}, "errorDefaults":{"validation":400,"auth":403,"conflict":409,"not-found":404,"domain":422,"infrastructure":500}}]
    });
    let document = TransportDocument::from_value(&wire).unwrap();
    document.validate_model_pin(&model).unwrap();
    let registry = ErrorRegistry::embedded().unwrap();
    let capabilities = CapabilityMap::http_json();
    let context = ValidationContext::new(&project)
        .with_query_model(&query)
        .with_errors(registry)
        .with_capabilities(&capabilities);
    transport_http::validate(&document, &context).unwrap();
    let canonical: Value = serde_json::from_str(&document.canonical_bytes().unwrap()).unwrap();
    assert_eq!(canonical["identity"], "dev.lekalo.transport-http@0.8.0");
    assert_eq!(canonical["irRef"], ir_pin);
    for version in [DocumentVersion::V3_0, DocumentVersion::V3_1] {
        let config = RenderConfig::new().with_version(version);
        let rendered = openapi::render(&document, &context, &config).unwrap();
        assert!(rendered.findings().is_empty());
        let parameters = rendered.root()["paths"]["/tasks"]["get"]["parameters"]
            .as_array()
            .unwrap();
        let tags = parameters.iter().find(|row| row["name"] == "tags").unwrap();
        assert_eq!(tags["required"], false);
        assert!(tags["schema"].to_string().contains("array"));
        if version == DocumentVersion::V3_0 {
            assert_eq!(tags["schema"]["nullable"], true);
        } else {
            assert!(tags["schema"]["type"]
                .as_array()
                .unwrap()
                .contains(&json!("null")));
        }
        assert_eq!(rendered.root()["x-lekalo-provenance"]["irRef"], ir_pin);
    }
    let sdk = client_sdk::project(&document, &context, &ClientConfig::generated()).unwrap();
    let sdk_wire: Value = serde_json::from_str(&sdk.canonical_bytes().unwrap()).unwrap();
    assert_eq!(sdk_wire["identity"], "dev.lekalo.client-sdk@0.8.0");
    assert_eq!(sdk_wire["transportRef"]["identity"], document.identity());
    assert_eq!(sdk_wire["irRef"], ir_pin);
    let tags = sdk_wire["operations"][0]["params"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "tags")
        .unwrap();
    assert_eq!(tags["typeRef"], "tasks.text");
    assert_eq!(tags["shape"], "list");
    assert_eq!(tags["nullable"], true);
    assert_eq!(tags["required"], false);
    for schema_file in [
        "transport-http.schema.v0.8.0.json",
        "client-sdk.schema.v0.8.0.json",
    ] {
        let schema: Value = serde_json::from_slice(
            &fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../contracts")
                    .join(schema_file),
            )
            .unwrap(),
        )
        .unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        assert!(validator.is_valid(if schema_file.starts_with("transport") {
            &canonical
        } else {
            &sdk_wire
        }));
    }
    let mut weak = wire.clone();
    weak["endpoints"][0]["params"][0]["required"] = json!(false);
    let weak = TransportDocument::from_value(&weak).unwrap();
    assert!(transport_http::validate(&weak, &context).is_err());
    assert!(transport_http::project(&weak, &context, "node").is_err());
    assert!(openapi::render(&weak, &context, &RenderConfig::new()).is_err());
    assert!(client_sdk::project(&weak, &context, &ClientConfig::generated()).is_err());
    assert!(transport_http::validate(&document, &ValidationContext::new(&project)).is_err());
    let mut wrong = query_wire.clone();
    wrong["queries"][0]["parameters"][0]["type"] = json!("tasks.bool");
    let wrong = QueryModelAttachment::from_value(&wrong).unwrap();
    assert!(transport_http::validate(
        &document,
        &ValidationContext::new(&project).with_query_model(&wrong)
    )
    .is_err());
    for (member, value) in [
        (
            "irRef",
            json!({"identity":"dev.lekalo.ir@0.8.0", "digest":format!("sha256:{}", "0".repeat(64))}),
        ),
        (
            "modelRef",
            json!({"modelVersion":"0.8.0", "digest":format!("sha256:{}", "0".repeat(64))}),
        ),
    ] {
        let mut stale = wire.clone();
        stale[member] = value;
        let stale = TransportDocument::from_value(&stale).unwrap();
        assert!(transport_http::validate(&stale, &context).is_err());
    }
    for (member, value) in [
        ("identity", json!("dev.lekalo.transport-http@0.4.0")),
        (
            "modelRef",
            json!({"modelVersion":"0.2.16", "digest":model_pin["digest"]}),
        ),
        (
            "irRef",
            json!({"identity":"dev.lekalo.ir@0.2.16", "digest":ir_pin["digest"]}),
        ),
    ] {
        let mut mixed = wire.clone();
        mixed[member] = value;
        assert!(TransportDocument::from_value(&mixed).is_err());
    }

    // Whole-input and explicit request bodies use the same authored query
    // fields as parameter projections; no empty or dangling input type.
    let bindings = root.path().join("lekalo/modules/tasks/bindings.yaml");
    fs::write(
        &bindings,
        fs::read_to_string(&bindings)
            .unwrap()
            .replace("method: GET", "method: POST"),
    )
    .unwrap();
    let mut body_wire = wire.clone();
    body_wire["endpoints"][0]["params"] = json!([]);
    body_wire["endpoints"][0]["body"] =
        json!({"mode":"whole-input","contentType":"application/json"});
    let mut body_query = query_wire.clone();
    let (body_model, body_project) = rebind(&mut body_wire, &mut body_query);
    let query = QueryModelAttachment::from_value(&body_query).unwrap();
    let document = TransportDocument::from_value(&body_wire).unwrap();
    document.validate_model_pin(&body_model).unwrap();
    let context = ValidationContext::new(&body_project)
        .with_query_model(&query)
        .with_errors(registry)
        .with_capabilities(&capabilities);
    let rendered = openapi::render(&document, &context, &RenderConfig::new()).unwrap();
    let body = &rendered.root()["paths"]["/tasks"]["post"]["requestBody"]["content"]
        ["application/json"]["schema"];
    assert_eq!(body["required"], json!(["title"]));
    assert_eq!(body["properties"]["tags"]["type"], json!(["array", "null"]));
    let sdk = client_sdk::project(&document, &context, &ClientConfig::generated()).unwrap();
    let sdk: Value = serde_json::from_str(&sdk.canonical_bytes().unwrap()).unwrap();
    assert_eq!(sdk["operations"][0]["body"]["typeRef"], "tasks.find.input");
    assert!(sdk["types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["typeId"] == "tasks.find.input" && row["kind"] == "object"));
    body_wire["endpoints"][0]["body"] = json!({"mode":"explicit","contentType":"application/json","fields":[{"name":"title","field":"title","required":true},{"name":"tags","field":"tags","required":false}]});
    let explicit = TransportDocument::from_value(&body_wire).unwrap();
    let sdk = client_sdk::project(&explicit, &context, &ClientConfig::generated()).unwrap();
    let sdk: Value = serde_json::from_str(&sdk.canonical_bytes().unwrap()).unwrap();
    let tags = sdk["operations"][0]["body"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "tags")
        .unwrap();
    assert_eq!(
        [tags["shape"].clone(), tags["nullable"].clone()],
        [json!("list"), json!(true)]
    );

    // A deeper authored type remains exact in OpenAPI; this SDK owner's
    // flat shape contract must refuse rather than collapse its inner list.
    let queries = root.path().join("lekalo/modules/tasks/queries.yaml");
    fs::write(
        &queries,
        fs::read_to_string(&queries)
            .unwrap()
            .replace("list<tasks.text>?", "list<list<tasks.text>>"),
    )
    .unwrap();
    body_query["queries"][0]["parameters"][1]["type"] = json!("list<list<tasks.text>>");
    let (_, nested_project) = rebind(&mut body_wire, &mut body_query);
    let query = QueryModelAttachment::from_value(&body_query).unwrap();
    let document = TransportDocument::from_value(&body_wire).unwrap();
    let context = ValidationContext::new(&nested_project)
        .with_query_model(&query)
        .with_errors(registry)
        .with_capabilities(&capabilities);
    let rendered = openapi::render(&document, &context, &RenderConfig::new()).unwrap();
    assert_eq!(
        rendered.root()["paths"]["/tasks"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["properties"]["tags"]["items"]["type"],
        "array"
    );
    let refused = client_sdk::project(&document, &context, &ClientConfig::generated()).unwrap_err();
    assert!(format!("{refused:?}").contains("query-input-shape-unsupported"));

    // A declared successor empty input is a closed empty object, not an
    // undeclared/open schema or an empty typeRef in either body projection.
    let source = fs::read_to_string(&queries).unwrap();
    let start = source.find("    input:\n").unwrap();
    let end = source.find("    reads:\n").unwrap();
    fs::write(&queries, format!("{}{}", &source[..start], &source[end..])).unwrap();
    body_query["queries"][0]["parameters"] = json!([]);
    body_wire["endpoints"][0]["body"] =
        json!({"mode":"whole-input","contentType":"application/json"});
    let (_, empty_project) = rebind(&mut body_wire, &mut body_query);
    let query = QueryModelAttachment::from_value(&body_query).unwrap();
    let document = TransportDocument::from_value(&body_wire).unwrap();
    let context = ValidationContext::new(&empty_project)
        .with_query_model(&query)
        .with_errors(registry)
        .with_capabilities(&capabilities);
    let rendered = openapi::render(&document, &context, &RenderConfig::new()).unwrap();
    let body = &rendered.root()["paths"]["/tasks"]["post"]["requestBody"]["content"]
        ["application/json"]["schema"];
    assert_eq!(body["properties"], json!({}));
    assert_eq!(body["additionalProperties"], false);
    assert!(rendered.findings().is_empty());
    let sdk = client_sdk::project(&document, &context, &ClientConfig::generated()).unwrap();
    let sdk: Value = serde_json::from_str(&sdk.canonical_bytes().unwrap()).unwrap();
    assert_eq!(sdk["operations"][0]["body"]["typeRef"], "tasks.find.input");
    let input = sdk["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["typeId"] == "tasks.find.input")
        .unwrap();
    assert_eq!(input["fields"], json!([]));

    // Whole-body SDK inputs must cover the full successor field bound,
    // rather than emitting a document rejected by its own JSON Schema.
    let mut fields = String::new();
    for index in 0..256 {
        writeln!(
            &mut fields,
            "      - name: p{index}\n        type: tasks.text"
        )
        .unwrap();
    }
    fs::write(&queries,format!("schema_version: '0.8.0'\ndefinitions:\n  - id: tasks.find\n    kind: query\n    version: 1\n    input:\n{fields}    reads:\n      - tasks.task\n    returns: tasks.task\n")).unwrap();
    body_query["queries"][0]["parameters"] = json!((0..256)
        .map(|index| json!({"name":format!("p{index}"),"type":"tasks.text","required":false}))
        .collect::<Vec<_>>());
    let (_, wide_project) = rebind(&mut body_wire, &mut body_query);
    let query = QueryModelAttachment::from_value(&body_query).unwrap();
    let document = TransportDocument::from_value(&body_wire).unwrap();
    let context = ValidationContext::new(&wide_project)
        .with_query_model(&query)
        .with_errors(registry)
        .with_capabilities(&capabilities);
    let sdk = client_sdk::project(&document, &context, &ClientConfig::generated()).unwrap();
    let sdk: Value = serde_json::from_str(&sdk.canonical_bytes().unwrap()).unwrap();
    let input = sdk["types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["typeId"] == "tasks.find.input")
        .unwrap();
    assert_eq!(input["fields"].as_array().unwrap().len(), 256);
    let schema: Value = serde_json::from_slice(
        &fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/client-sdk.schema.v0.8.0.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(jsonschema::validator_for(&schema).unwrap().is_valid(&sdk));
}

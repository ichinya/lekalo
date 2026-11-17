use lekalo_adapter_sdk::json::{strict_parse, Value};
use lekalo_python_fastapi::{apply, Source, Symbol};

fn fixture(route: &str) -> (Source, Vec<Value>) {
    let source = Source {
        path: "src/shop.py".into(), module: "shop".into(), digest: "sha256:fixture".into(),
        text: format!("from fastapi import FastAPI\napp = FastAPI()\n@app.get({route})\nasync def items():\n    return []\n"),
        symbols: vec![
            Symbol { native: "app".into(), qualified_name: "shop.app".into(), line: 2, declaration_only: false },
            Symbol { native: "items".into(), qualified_name: "shop.items".into(), line: 3, declaration_only: false },
        ],
    };
    let entry = strict_parse(r#"{"path":"src/shop.py","kind":"function","detail":"{\"s\":\"shop.items\",\"n\":\"items\"}","evidence":{"native_symbols":[{"native":"app"},{"native":"items"}]}}"#).unwrap();
    (source, vec![entry])
}

#[test]
fn attaches_route_only_to_admitted_handler() {
    let (source, mut entries) = fixture("'/items'");
    let result = apply(&[source], &mut entries, "sha256:policy");
    assert!(!result.truncated);
    assert!(result.uncertainty.is_empty(), "{:?}", result.uncertainty);
    assert_eq!(
        entries[0].get("kind").and_then(Value::as_str),
        Some("endpoint")
    );
    let routes = entries[0]
        .get("evidence")
        .unwrap()
        .get("http_routes")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(
        routes[0].get("path").and_then(Value::as_str),
        Some("/items")
    );
}

#[test]
fn dynamic_path_is_uncertain_not_a_route() {
    let (source, mut entries) = fixture("unknown_path()");
    let result = apply(&[source], &mut entries, "sha256:policy");
    assert!(!result.uncertainty.is_empty());
    assert!(entries[0]
        .get("evidence")
        .unwrap()
        .get("http_routes")
        .is_none());
}

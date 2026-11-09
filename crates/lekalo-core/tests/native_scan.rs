//! Adversarial admission of the public native inventory, using production decoding.
use lekalo_core::target_protocol::wire::decode_response;
use serde_json::{json, Value};

fn response() -> Value {
    let digest = format!("sha256:{}", "a".repeat(64));
    json!({
        "protocol":"lekalo.target/v1","protocol_version":"1.1.0","operation":"scan","status":"ok",
        "request_id":format!("req-{}","0".repeat(64)),
        "evidence":{"adapter":{"id":"test","version":"1.1.0","digest":digest},
                    "scan_profile_digest":digest,"native_uncertainty":[]},
        "result":{"entries":[{"path":"src/task.ts","kind":"entity",
            "detail":"{\"s\":\"planner.task\",\"n\":\"root\",\"l\":1,\"q\":\"medium\"}",
            "evidence":{"source_digest":digest,"signature":digest,"native_symbols":[
                {"native":"child","qualified_name":"Task.id","family":"property","path":"src/task.ts",
                 "line":2,"end_line":2,"slot":"instance","declaration_only":false,"member_of":"root","signature":digest},
                {"native":"root","qualified_name":"Task","family":"interface","path":"src/task.ts",
                 "line":1,"end_line":3,"slot":"instance","declaration_only":true,"signature":digest}
            ]}}]}
    })
}
fn admits(value: &Value) -> bool {
    decode_response(&serde_json::to_vec(value).unwrap()).is_ok()
}

#[test]
fn source_bound_nested_inventory_is_admitted_only_at_its_exact_version() {
    let good = response();
    assert!(admits(&good));
    let mut old = good.clone();
    old["protocol_version"] = json!("0.9.0");
    assert!(!admits(&old), "new members may not leak into a predecessor");
    old["evidence"]
        .as_object_mut()
        .unwrap()
        .remove("native_uncertainty");
    old["result"]["entries"][0]["evidence"]
        .as_object_mut()
        .unwrap()
        .remove("native_symbols");
    assert!(admits(&old), "unchanged predecessor scan stays admitted");
}
#[test]
fn invalid_ownership_shape_and_custody_metadata_refuse() {
    let mutations: Vec<fn(&mut Value)> = vec![
        |v| {
            v["result"]["entries"][0]["evidence"]
                .as_object_mut()
                .unwrap()
                .remove("native_symbols");
        },
        |v| {
            v["result"]["entries"][0]["evidence"]["native_symbols"][0]["member_of"] =
                json!("absent")
        },
        |v| {
            v["result"]["entries"][0]["evidence"]["native_symbols"][0]["member_of"] = json!("child")
        },
        |v| {
            v["result"]["entries"][0]["evidence"]["native_symbols"][0]["path"] =
                json!("src/other.ts")
        },
        |v| v["result"]["entries"][0]["evidence"]["native_symbols"][0]["end_line"] = json!(9),
        |v| v["result"]["entries"][0]["evidence"]["native_symbols"][0]["line"] = json!(0),
        |v| {
            v["result"]["entries"][0]["evidence"]["native_symbols"][0]["family"] =
                json!("unknown-kind")
        },
        |v| v["result"]["entries"][0]["evidence"]["native_symbols"][0]["member_of"] = Value::Null,
        |v| {
            v["result"]["entries"][0]["evidence"]["native_symbols"][0]["secret"] =
                json!("not representable")
        },
        |v| {
            v["result"]["entries"][0]["evidence"]["native_symbols"][0]["signature"] =
                json!("invalid")
        },
        |v| v["result"]["entries"][0]["evidence"]["native_symbols"][0]["native"] = json!("root"),
        |v| {
            v["result"]["entries"][0]["evidence"]["native_symbols"][1]["native"] =
                json!("wrong-root")
        },
        |v| v["result"]["entries"][0]["evidence"]["source_digest"] = json!("invalid"),
        |v| {
            v["evidence"]
                .as_object_mut()
                .unwrap()
                .remove("native_uncertainty");
        },
        |v| v["evidence"]["native_uncertainty"] = json!([{"path":"../secret","kind":"any-surface","detail":"Task","source_digest":format!("sha256:{}","a".repeat(64))}]),
        |v| v["evidence"]["native_uncertainty"] = json!([{"path":"src/task.ts","kind":"invented","detail":"Task","source_digest":format!("sha256:{}","a".repeat(64))}]),
    ];
    for (i, mutate) in mutations.into_iter().enumerate() {
        let mut value = response();
        mutate(&mut value);
        assert!(!admits(&value), "mutation {i}");
    }
}
#[test]
fn native_inventory_bounds_and_duplicates_refuse_without_truncating() {
    let mut value = response();
    let rows = value["result"]["entries"][0]["evidence"]["native_symbols"]
        .as_array_mut()
        .unwrap();
    rows.push(rows[0].clone());
    assert!(!admits(&value));
    let mut value = response();
    let root = value["result"]["entries"][0]["evidence"]["native_symbols"][1].clone();
    value["result"]["entries"][0]["evidence"]["native_symbols"] = Value::Array(
        (0..257)
            .map(|i| {
                let mut row = root.clone();
                row["native"] = json!(format!("n{i:04}"));
                row
            })
            .collect(),
    );
    assert!(!admits(&value));
}

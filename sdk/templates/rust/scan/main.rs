//! The SDK scan example (issue #106): a capability-limited scan-only
//! adapter on `lekalo-adapter-sdk-rust`. It declares describe and scan
//! honestly and refuses everything else with a closed in-envelope
//! error; read-only by default, no write scopes, no network, no shell.

use lekalo_adapter_sdk as sdk;
use sdk::{Adapter, Output, Value};

struct ScanAdapter {
    identity: Value,
}

impl Adapter for ScanAdapter {
    fn identity(&self) -> Value {
        self.identity.clone()
    }

    fn handle(&self, operation: &str, _request: &Value) -> Output {
        match operation {
            "describe" => {
                match sdk::build_capabilities(
                    &self.identity,
                    &["0.3.2"],
                    &["describe", "scan"],
                    &["file", "stdin"],
                    &["sdk-scan-example"],
                    &[],
                    &["src/**"],
                    &[],
                    &[],
                    false,
                    Some(vec![("scan.symbols", "full")]),
                    None,
                ) {
                    Ok(value) => Output { capabilities: Some(value), ..Output::default() },
                    Err(_) => refused(),
                }
            }
            "scan" => Output {
                payload: Some(Value::Object(vec![
                    (
                        "entries".to_owned(),
                        Value::Array(vec![
                            Value::Object(vec![
                                ("kind".to_owned(), Value::Str("source".to_owned())),
                                ("path".to_owned(), Value::Str("src/main.ts".to_owned())),
                            ]),
                            Value::Object(vec![
                                ("kind".to_owned(), Value::Str("source".to_owned())),
                                ("path".to_owned(), Value::Str("src/util.ts".to_owned())),
                            ]),
                        ]),
                    ),
                    ("truncated".to_owned(), Value::Bool(false)),
                ])),
                ..Output::default()
            },
            _ => Output {
                error: Some((
                    "unsupported",
                    "operation-unsupported",
                    "the scan example implements describe and scan only".to_owned(),
                )),
                ..Output::default()
            },
        }
    }
}

fn refused() -> Output {
    Output {
        error: Some((
            "infrastructure",
            "sdk-build-refused",
            "the capability builder refused the declaration".to_owned(),
        )),
        ..Output::default()
    }
}

fn main() {
    let identity = sdk::build_adapter_identity(
        "sdk-scan-example",
        "0.1.0",
        &sdk::sha256("lekalo adapter-sdk scan example"),
    )
    .expect("identity admits");
    std::process::exit(sdk::serve(&ScanAdapter { identity }));
}

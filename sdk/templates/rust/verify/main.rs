//! The SDK verify example (issue #106): a verification adapter on
//! `lekalo-adapter-sdk-rust`. It declares describe and verify honestly,
//! reports findings through the typed builder, and never widens its
//! declared read surface. Read-only; no writes; no network; no shell.

use lekalo_adapter_sdk as sdk;
use sdk::{Adapter, Output, Value};

struct VerifyAdapter {
    identity: Value,
}

impl Adapter for VerifyAdapter {
    fn identity(&self) -> Value {
        self.identity.clone()
    }

    fn handle(&self, operation: &str, _request: &Value) -> Output {
        match operation {
            "describe" => match sdk::build_capabilities(
                &self.identity,
                &["0.3.2"],
                &["describe", "verify"],
                &["file", "stdin"],
                &["sdk-verify-example"],
                &[],
                &[".lekalo/ir/**"],
                &[],
                &["0.2.16"],
                false,
                Some(vec![("verify.scenarios", "full")]),
                None,
            ) {
                Ok(value) => Output { capabilities: Some(value), ..Output::default() },
                Err(_) => refused(),
            },
            "verify" => match sdk::build_finding("scenario-missing", "src/main.ts", Some("no scenarios declared")) {
                Ok(finding) => Output {
                    payload: Some(Value::Object(vec![
                        ("findings".to_owned(), Value::Array(vec![finding])),
                        ("ok".to_owned(), Value::Bool(true)),
                    ])),
                    ..Output::default()
                },
                Err(_) => refused(),
            },
            _ => Output {
                error: Some((
                    "unsupported",
                    "operation-unsupported",
                    "the verify example implements describe and verify only".to_owned(),
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
        "sdk-verify-example",
        "0.1.0",
        &sdk::sha256("lekalo adapter-sdk verify example"),
    )
    .expect("identity admits");
    std::process::exit(sdk::serve(&VerifyAdapter { identity }));
}

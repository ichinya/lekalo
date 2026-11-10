//! Authoring gate for the public TargetClient cancellation/resource contract.
//! The shipped CLI's public scan gate separately qualifies observed behavior.
use lekalo_core::target_protocol::{
    transport::{AdapterCommand, TransportLimits},
    TargetClient, TargetFailure,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().expect("mode");
    let command = AdapterCommand {
        program: args.next().expect("program").into(),
        args: args.collect(),
    };
    let mut limits = TransportLimits {
        timeout_ms: if mode == "timeout" { 1500 } else { 5000 },
        ..TransportLimits::default()
    };
    if mode == "output" {
        limits.max_output_bytes = 1024;
    }
    if mode == "request" {
        limits.max_request_bytes = 1;
    }
    let mut client = TargetClient::new(limits);
    let cancel = AtomicBool::new(mode == "cancel-before");
    let started = Instant::now();
    let result = std::thread::scope(|scope| {
        if mode == "cancel" {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(500));
                cancel.store(true, Ordering::Release);
            });
        }
        client
            .describe_with_cancel(&command, &std::env::current_dir().unwrap(), Some(&cancel))
            .map(|description| description.negotiated_version.to_owned())
    });
    let (outcome, version) = match result {
        Ok(version) => ("ok", Some(version)),
        Err(TargetFailure::Cancelled) => ("cancelled", None),
        Err(TargetFailure::Timeout) => ("timeout", None),
        Err(TargetFailure::OutputLimit { .. }) => ("output-limit", None),
        Err(TargetFailure::TransportFailed {
            detail: "request-size",
        }) => ("request-size", None),
        Err(TargetFailure::TransportFailed { detail: "spawn" }) => ("peer-failure", None),
        Err(TargetFailure::ResponseInvalid {
            detail: lekalo_core::target_protocol::wire::ResponseInvalidity::Evidence,
        }) => ("provenance-refused", None),
        Err(error) => {
            eprintln!("{error:?}");
            std::process::exit(1);
        }
    };
    println!(
        "{}",
        serde_json::json!({"outcome": outcome, "version": version, "elapsed_ms": started.elapsed().as_millis()})
    );
}

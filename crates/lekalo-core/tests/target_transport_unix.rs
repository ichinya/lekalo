//! Hosted Unix qualification; Windows does not execute these tests.
//! Each subprocess has an independent watchdog and self-terminating owned
//! descendants, so a broken transport cannot hang the CI worker indefinitely.
#![cfg(unix)]
use lekalo_core::target_protocol::transport::{
    self, AdapterCommand, Stream, TransportFailure, TransportLimits,
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

fn run_guarded(mode: &str) {
    let root = tempfile::tempdir().unwrap();
    let script = root.path().join("process.mjs");
    let code = r#"
import {spawn} from 'node:child_process';
import {writeFileSync} from 'node:fs';
const mode=process.argv[2];
if(mode==='permission') {
 const {statSync,readFileSync}=await import('node:fs');
 const index=process.argv.indexOf('--lekalo-request-file');
 const path=process.argv[index+1];
 process.stdout.write(JSON.stringify({mode:statSync(path).mode&0o777,body:readFileSync(path,'utf8'),path}));
} else {
 const child=spawn(process.execPath,['-e',"setTimeout(()=>require('fs').writeFileSync('descendant-survived','bad'),1600);setTimeout(()=>{},2200)"],{stdio:['ignore','inherit','inherit']});
 writeFileSync('owned-child',String(child.pid));
 if(mode==='output') process.stdout.write('x'.repeat(10240));
 else if(mode==='stderr') process.stderr.write('x'.repeat(10240));
 if(mode==='parent-exit') process.exit(0);
 setTimeout(()=>{},2200);
}
"#;
    std::fs::write(&script, code).unwrap();
    let command = AdapterCommand {
        program: "node".into(),
        args: vec![script.to_string_lossy().into_owned(), mode.into()],
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let cancelling = mode == "cancel";
    let cancel_thread = std::thread::spawn(move || {
        if cancelling {
            std::thread::sleep(Duration::from_millis(500));
            flag.store(true, Ordering::Relaxed);
        }
    });
    let limits = TransportLimits {
        timeout_ms: if mode == "cancel" { 5000 } else { 800 },
        max_output_bytes: if mode == "output" { 256 } else { 8192 },
        max_stderr_bytes: if mode == "stderr" { 256 } else { 8192 },
        ..TransportLimits::default()
    };
    let begin = Instant::now();
    let result = transport::run(
        &command,
        b"private-request",
        &limits,
        root.path(),
        mode == "permission",
        Some(&cancel),
    );
    cancel_thread.join().unwrap();
    assert!(
        begin.elapsed() < Duration::from_millis(1800),
        "{mode}: {:?}",
        begin.elapsed()
    );
    if mode == "permission" {
        let response: serde_json::Value = serde_json::from_slice(&result.unwrap().stdout).unwrap();
        assert_eq!(response["mode"], 0o600);
        assert_eq!(response["body"], "private-request");
        assert!(!Path::new(response["path"].as_str().unwrap()).exists());
        return;
    }
    let expected = match mode {
        "cancel" => TransportFailure::Cancelled,
        "output" => TransportFailure::OutputLimit {
            stream: Stream::Stdout,
        },
        "stderr" => TransportFailure::OutputLimit {
            stream: Stream::Stderr,
        },
        _ => TransportFailure::Timeout,
    };
    assert_eq!(result.unwrap_err(), expected);
    assert!(
        root.path().join("owned-child").is_file(),
        "positive child-start control"
    );
    std::thread::sleep(Duration::from_millis(1800));
    assert!(
        !root.path().join("descendant-survived").exists(),
        "{mode}: descendant survived group cleanup"
    );
}

#[test]
fn unix_timeout_cancel_output_and_descendant_pipes_are_bounded() {
    // A separate harness process is the watchdog, not the implementation's
    // own timeout. If a regression blocks it, kill only this owned test pid.
    if let Ok(mode) = std::env::var("LEKALO_OWNED_UNIX_TRANSPORT_CASE") {
        run_guarded(&mode);
        return;
    }
    for mode in [
        "timeout",
        "cancel",
        "output",
        "stderr",
        "parent-exit",
        "permission",
    ] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "unix_timeout_cancel_output_and_descendant_pipes_are_bounded",
                "--nocapture",
            ])
            .env("LEKALO_OWNED_UNIX_TRANSPORT_CASE", mode)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "{mode}: {status}");
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{mode}: independent watchdog elapsed");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

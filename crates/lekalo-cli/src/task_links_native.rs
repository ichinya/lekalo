//! Explicit opt-in execution owner for the maintained Laravel port. Authority
//! comes from this process launching the embedded worker, controlling barriers,
//! observing termination and independently checking fresh durable state. Neither
//! a database, sidecar, environment flag nor a child's success label is admitted.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lekalo_core::task_links::{
    self as tl,
    portability::{EvidenceSource, Provided, Report},
    Attachment,
};
use lekalo_core::DomainResult;

const IMPLEMENTATION: &str =
    include_str!("../../../tests/fixtures/php-laravel/planner/app/Lekalo/TaskLinksStore.php");
const WORKER: &str =
    include_str!("../../../tests/fixtures/php-laravel/planner/src/task-links-worker.php");
const LOCK: &[u8] = include_bytes!("../../../tests/fixtures/php-laravel/planner/composer.lock");
const TARGET: &str = "php-laravel-maintained";

fn refused() -> DomainResult {
    tl::into_result(&tl::input_failure("native-execution-unqualified"))
}
fn check(condition: bool) -> Result<(), DomainResult> {
    if condition {
        Ok(())
    } else {
        Err(refused())
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, DomainResult> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| refused())?
            .as_nanos();
        for ordinal in 0..32 {
            let path = std::env::temp_dir().join(format!(
                "lekalo-tasklinks-native-{}-{stamp}-{ordinal}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(refused()),
            }
        }
        Err(refused())
    }
    fn stage(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, DomainResult> {
        let path = self.0.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|_| refused())?;
        file.write_all(bytes).map_err(|_| refused())?;
        Ok(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct WorkerProcess {
    child: Child,
    lines: Receiver<String>,
}
impl WorkerProcess {
    fn line(&self) -> Result<String, DomainResult> {
        self.lines
            .recv_timeout(Duration::from_secs(30))
            .map_err(|_| refused())
    }
    fn start(&mut self) -> Result<(), DomainResult> {
        self.child
            .stdin
            .as_mut()
            .ok_or_else(refused)?
            .write_all(b"start\n")
            .map_err(|_| refused())
    }
    fn finish(&mut self, run: &str, operation: &str) -> Result<String, DomainResult> {
        let line = self.line()?;
        let result = tl::json_parse(line.as_bytes()).map_err(|_| refused())?;
        check(
            result.as_object().is_some_and(|object| object.len() == 5)
                && result["runId"] == run
                && result["operation"] == operation
                && result["runtime"] == "laravel/framework@13.33.0"
                && result["engine"] == "pdo-sqlite",
        )?;
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().map_err(|_| refused())? {
                check(status.success())?;
                return result["result"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(refused);
            }
            check(start.elapsed() < Duration::from_secs(30))?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn kill_at_boundary(&mut self) -> Result<(), DomainResult> {
        check(self.line()? == "boundary")?;
        // Keep stdin open until termination. EOF would be graceful rollback,
        // not the controlled ungraceful process interruption being asserted.
        check(self.child.try_wait().map_err(|_| refused())?.is_none())?;
        self.child.kill().map_err(|_| refused())?;
        check(!self.child.wait().map_err(|_| refused())?.success())
    }
}
impl Drop for WorkerProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Runner<'a> {
    script: PathBuf,
    store: PathBuf,
    autoload: PathBuf,
    database: PathBuf,
    run: String,
    policy: &'a str,
    capability: &'a str,
    max_attempts: u64,
}
impl Runner<'_> {
    fn spawn(
        &self,
        operation: &str,
        intent: &str,
        token: &str,
        task: &str,
    ) -> Result<WorkerProcess, DomainResult> {
        let mut child = Command::new("php")
            .arg(&self.script)
            .arg(&self.autoload)
            .arg(&self.store)
            .arg(&self.database)
            .args([
                operation,
                self.run.as_str(),
                self.policy,
                self.capability,
                intent,
                token,
                task,
            ])
            .arg(self.max_attempts.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| refused())?;
        let stdout = child.stdout.take().ok_or_else(refused)?;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            // Fixed worker protocol, bounded even if the toolchain is broken.
            for _ in 0..4 {
                let mut line = String::new();
                if reader
                    .by_ref()
                    .take(16 * 1024)
                    .read_line(&mut line)
                    .unwrap_or(0)
                    == 0
                {
                    break;
                }
                if line.len() >= 16 * 1024 || sender.send(line.trim_end().to_owned()).is_err() {
                    break;
                }
            }
        });
        let process = WorkerProcess {
            child,
            lines: receiver,
        };
        check(process.line()? == "ready")?;
        Ok(process)
    }
    fn run(
        &self,
        operation: &str,
        intent: &str,
        token: &str,
        task: &str,
    ) -> Result<String, DomainResult> {
        let mut child = self.spawn(operation, intent, token, task)?;
        child.start()?;
        child.finish(&self.run, operation)
    }
    fn pair(
        &self,
        operation: &str,
        first: &str,
        second: &str,
        intent: &str,
    ) -> Result<Vec<String>, DomainResult> {
        let mut left = self.spawn(operation, intent, first, first)?;
        let mut right = self.spawn(operation, intent, second, second)?;
        // Both distinct processes have reached the owner-controlled barrier.
        left.start()?;
        right.start()?;
        let mut results = vec![
            left.finish(&self.run, operation)?,
            right.finish(&self.run, operation)?,
        ];
        results.sort();
        Ok(results)
    }
}

fn integer(db: &rusqlite::Connection, sql: &str) -> Result<i64, DomainResult> {
    db.query_row(sql, [], |row| row.get(0))
        .map_err(|_| refused())
}
fn text(db: &rusqlite::Connection, sql: &str) -> Result<String, DomainResult> {
    db.query_row(sql, [], |row| row.get(0))
        .map_err(|_| refused())
}
fn open(path: &Path) -> Result<rusqlite::Connection, DomainResult> {
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| refused())
}

/// Private, live capability. There is intentionally no Deserialize/import path.
struct NativeExecution {
    target: &'static str,
    profile_digest: String,
    result_digest: String,
    facts: Vec<&'static str>,
    provenance: tl::portability::ExecutionProvenance,
}

fn execute(attachment: &Attachment, profile_digest: &str) -> Result<NativeExecution, DomainResult> {
    let policy = attachment
        .action_policies()
        .iter()
        .find(|row| row.action == "tasklinks.request_action")
        .ok_or_else(refused)?;
    check(policy.actor == "identity.user" && policy.decision == "allow")?;
    let capability = attachment
        .capability_requirements()
        .iter()
        .find(|row| policy.capability_ref.as_ref() == Some(&row.capability_id))
        .ok_or_else(refused)?;
    let fields = attachment
        .field_policies()
        .iter()
        .find(|row| {
            row.direction == "outbound" && row.actor == policy.actor && row.decision == "allow"
        })
        .ok_or_else(refused)?;
    check(fields.fields == ["state", "title"])?;
    let root = std::env::current_dir().map_err(|_| refused())?;
    let fixture = root.join("tests/fixtures/php-laravel/planner");
    // Locked maintained framework runtime, provisioned explicitly outside this
    // execution command. Missing/mismatched dependencies are a refusal.
    check(std::fs::read(fixture.join("composer.lock")).map_err(|_| refused())? == LOCK)?;
    let autoload = fixture.join("vendor/autoload.php");
    check(autoload.is_file())?;
    let scratch = Scratch::new()?;
    let script = scratch.stage("worker.php", WORKER.as_bytes())?;
    let store = scratch.stage("TaskLinksStore.php", IMPLEMENTATION.as_bytes())?;
    let database = scratch.stage("run.sqlite", &[])?;
    let run = tl::wire_digest(scratch.0.to_string_lossy().as_bytes())
        .as_str()
        .to_owned();
    let runner = Runner {
        script,
        store,
        autoload,
        database: database.clone(),
        run: run.clone(),
        policy: policy.revision.as_str(),
        capability: capability.revision.as_str(),
        max_attempts: attachment.delivery_rules().max_attempts,
    };
    check(runner.run("init", "", "", "")? == "initialized")?;

    // Synchronized competing maintained processes: DB-level exact identity and
    // cross-task refusal. Child success labels are independently checked below.
    check(
        runner.pair("attach", "task-one", "task-two", "")?
            == ["attached", "identity_already_bound"],
    )?;
    let db = open(&database)?;
    check(integer(&db, "SELECT count(*) FROM link")? == 1)?;
    // Ensure the next battery leg has a local task link whichever race won.
    let winner = text(&db, "SELECT task_id FROM link")?;
    drop(db);
    if winner == "task-two" {
        // A fresh seed after the race, not a silent reassignment of its identity.
        let seed = rusqlite::Connection::open(&database).map_err(|_| refused())?;
        seed.execute("DELETE FROM link", [])
            .map_err(|_| refused())?;
        drop(seed);
        check(runner.run("attach", "", "", "task-one")? == "attached")?;
    }
    check(runner.run("attach", "", "", "task-one")? == "replayed")?;
    check(runner.run("attach", "", "", "task-two")? == "identity_already_bound")?;

    // Kill inside the real maintained transaction after all FOUR writes, before
    // commit. Reopen independently: zero partial writes and task unchanged.
    let mut interrupted = runner.spawn("request-kill", "uncommitted", "", "")?;
    interrupted.start()?;
    interrupted.kill_at_boundary()?;
    let db = open(&database)?;
    check(text(&db, "SELECT title FROM task")? == "before")?;
    for table in ["decision", "outbox", "audit"] {
        check(integer(&db, &format!("SELECT count(*) FROM {table}"))? == 0)?;
    }
    drop(db);
    check(runner.run("request", "ack-window", "", "")? == "committed")?;
    check(runner.pair("claim", "fence-one", "fence-two", "ack-window")? == ["claimed", "refused"])?;
    let db = open(&database)?;
    let token = text(
        &db,
        "SELECT lease_token FROM outbox WHERE intent_id='ack-window'",
    )?;
    check(
        integer(
            &db,
            "SELECT attempts FROM outbox WHERE intent_id='ack-window'",
        )? == 1,
    )?;
    // Recover the FULL persisted originating command/authority context, not just
    // state/attempt/generation/token. Every member is independently asserted.
    let context: Vec<String> = db.query_row("SELECT command,action,workspace_id,task_id,link_id,actor,fields,policy_revision,capability_revision,payload_digest,causation_id,idempotency_key,run_id FROM outbox WHERE intent_id='ack-window'", [], |row| (0..13).map(|index| row.get(index)).collect()).map_err(|_| refused())?;
    check(
        context
            == vec![
                "tasklinks.request_action".to_owned(),
                "tasklinks.request_action".to_owned(),
                "workspace-one".to_owned(),
                "task-one".to_owned(),
                "link-one".to_owned(),
                "identity.user".to_owned(),
                "[\"state\",\"title\"]".to_owned(),
                runner.policy.to_owned(),
                runner.capability.to_owned(),
                tl::wire_digest(b"[\"state\",\"title\"]")
                    .as_str()
                    .to_owned(),
                "ack-window".to_owned(),
                "ack-window".to_owned(),
                run.clone(),
            ],
    )?;
    check(
        integer(&db, "SELECT generation FROM outbox")? == 1
            && integer(&db, "SELECT link_version FROM outbox")? == 1,
    )?;
    drop(db);
    let mut accepted = runner.spawn("accept-kill", "ack-window", &token, "")?;
    accepted.start()?;
    accepted.kill_at_boundary()?;
    let db = open(&database)?;
    check(
        text(&db, "SELECT state FROM outbox WHERE intent_id='ack-window'")?
            == "accepted_pending_ack",
    )?;
    check(integer(&db, "SELECT count(*) FROM remote_receipt")? == 1)?;
    drop(db);
    check(runner.run("recover", "ack-window", "", "")? == "unknown_delivery")?;
    check(runner.run("claim", "ack-window", "no-resend", "")? == "refused")?;
    check(runner.run("reconcile", "ack-window", "", "")? == "delivered")?;
    let db = open(&database)?;
    check(
        text(
            &db,
            "SELECT receipt_digest FROM outbox WHERE intent_id='ack-window'",
        )? == tl::wire_digest(b"ack-window").as_str(),
    )?;
    check(integer(&db, "SELECT count(*) FROM remote_receipt")? == 1)?;
    drop(db);

    // Durable bounded retry across NEW worker processes; revoked/stale authority
    // cannot borrow the pre-restart decision and never consumes an attempt.
    check(runner.run("request", "bounded-retry", "", "")? == "committed")?;
    let bad = Runner {
        policy: "sha256:revoked",
        ..Runner {
            script: runner.script.clone(),
            store: runner.store.clone(),
            autoload: runner.autoload.clone(),
            database: runner.database.clone(),
            run: runner.run.clone(),
            policy: runner.policy,
            capability: runner.capability,
            max_attempts: runner.max_attempts,
        }
    };
    check(bad.run("claim", "bounded-retry", "revoked", "")? == "refused")?;
    for attempt in 1..=runner.max_attempts {
        let token = format!("attempt-{attempt}");
        check(runner.run("claim", "bounded-retry", &token, "")? == "claimed")?;
        check(
            runner.run("fail", "bounded-retry", &token, "")?
                == if attempt == runner.max_attempts {
                    "dead_letter"
                } else {
                    "retry_wait"
                },
        )?;
    }
    check(runner.run("claim", "bounded-retry", "attempt-four", "")? == "refused")?;
    let db = open(&database)?;
    check(
        integer(
            &db,
            "SELECT attempts FROM outbox WHERE intent_id='bounded-retry'",
        )? == runner.max_attempts as i64,
    )?;
    check(
        text(
            &db,
            "SELECT idempotency_key FROM outbox WHERE intent_id='bounded-retry'",
        )? == "bounded-retry",
    )?;
    check(
        integer(&db, "SELECT count(*) FROM decision")? == 2
            && integer(&db, "SELECT count(*) FROM audit")? == 2,
    )?;
    drop(db);
    check(runner.pair("cas", "cas-left", "cas-right", "")? == ["stale", "updated"])?;
    let db = open(&database)?;
    check(integer(&db, "SELECT version FROM task")? == 2)?;
    let durable_state = (
        text(&db, "SELECT title FROM task")?,
        context,
        integer(&db, "SELECT count(*) FROM link")?,
        text(&db, "SELECT state FROM outbox WHERE intent_id='ack-window'")?,
        integer(
            &db,
            "SELECT attempts FROM outbox WHERE intent_id='bounded-retry'",
        )?,
    );
    drop(db);
    let framework = std::fs::read(
        fixture.join("vendor/laravel/framework/src/Illuminate/Database/Connection.php"),
    )
    .map_err(|_| refused())?;
    let build_digest = tl::wire_digest(
        &[
            IMPLEMENTATION.as_bytes(),
            WORKER.as_bytes(),
            LOCK,
            &framework,
        ]
        .concat(),
    );
    let input = (
        attachment.computed_revision(),
        attachment.ir_digest().as_str(),
        TARGET,
        profile_digest,
        build_digest.as_str(),
        runner.policy,
        runner.capability,
    );
    let input_digest = tl::wire_digest(&serde_json::to_vec(&input).map_err(|_| refused())?)
        .as_str()
        .to_owned();
    let facts = vec![
        "unique-index",
        "compare-and-swap",
        "local-transaction",
        "durable-outbox",
    ];
    let result_digest = tl::wire_digest(
        &serde_json::to_vec(&(
            input,
            durable_state,
            &facts,
            "synchronized-processes-ungraceful-kill-full-context",
            run,
        ))
        .map_err(|_| refused())?,
    )
    .as_str()
    .to_owned();
    let provenance = tl::portability::ExecutionProvenance {
        owner: "lekalo-cli/maintained-native-execution".to_owned(),
        implementation: "php-laravel-maintained/laravel-13.33.0-pdo-sqlite".to_owned(),
        build_digest: build_digest.as_str().to_owned(),
        source_digest: attachment.ir_digest().as_str().to_owned(),
        input_digest,
        result_digest: result_digest.clone(),
        profile_id: TARGET.to_owned(),
        profile_digest: profile_digest.to_owned(),
        origin: "native".to_owned(),
        assertions: facts
            .iter()
            .map(|fact| (*fact).to_owned())
            .chain([
                "process-interruption-rollback".to_owned(),
                "process-restart-full-context".to_owned(),
                "synchronized-competing-workers".to_owned(),
                "acceptance-before-ack-no-resend".to_owned(),
                "durable-attempt-bound".to_owned(),
            ])
            .collect(),
    };
    Ok(NativeExecution {
        target: TARGET,
        profile_digest: profile_digest.to_owned(),
        result_digest,
        facts,
        provenance,
    })
}

pub(super) fn qualify(attachment: &Attachment, report: &mut Report) -> Result<(), DomainResult> {
    let executed = execute(attachment, &report.to_profile.digest)?;
    // Export the observed binding even on a target mismatch; never change its
    // executed profile identity to the selected unexecuted target.
    report.execution_provenance.push(executed.provenance);
    // A genuine Laravel run cannot qualify Node or the Rust SQLite profile.
    if report.to_profile.profile_id == executed.target
        && report.to_profile.digest == executed.profile_digest
    {
        for row in &mut report.target_guarantees {
            if executed.facts.contains(&row.fact) {
                row.provided = Provided::Supported;
                row.evidence_source = EvidenceSource::NativeEvidence;
                row.pin = executed.result_digest.clone();
            }
        }
        report.evidence_coverage.native_evidence_ids.push(format!(
            "tasklinks.execution.native.ev{}",
            &executed.result_digest[7..23]
        ));
        report.evidence_coverage.unknown_required = report
            .core_semantics
            .iter()
            .chain(&report.provider_adapters)
            .chain(&report.target_guarantees)
            .filter(|row| {
                row.requirement == tl::portability::Requirement::Required
                    && row.provided == Provided::Unknown
            })
            .count() as u64;
    }
    Ok(())
}

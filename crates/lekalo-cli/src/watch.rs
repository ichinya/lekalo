//! Thin watch host: capability-safe capture, native/poll subscription,
//! bounded event delivery, independent reconciliation, worker ownership
//! and JSONL/human output. It never invokes a mutating command.
use clap::Args;
use lekalo_core::loader::LoadSelection;
use lekalo_core::result::{DomainResult, Status};
use lekalo_core::watch::{
    self as core,
    events::WatchEvent,
    request::{WatchRequest, WatchStages},
    scheduler::{Decision, Scheduler},
    snapshot::{self, CapturePolicy, InputSnapshot},
    RunContext, RunOutcome, WatchBackend, WatchService,
};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, TryRecvError},
    Arc,
};
use std::time::{Duration, Instant};

#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Emit closed watch JSON Lines on stdout (not global --json).
    #[arg(long)]
    json_lines: bool,
    #[arg(long, value_name = "DIR")]
    project: Option<String>,
    #[arg(long, value_name = "MODULE")]
    module: Option<String>,
    #[arg(long = "target", value_name = "TARGET")]
    targets: Vec<String>,
    #[arg(long)]
    validate: bool,
    #[arg(long)]
    impact: bool,
    #[arg(long)]
    generate_check: bool,
    #[arg(long)]
    locked: bool,
    #[arg(long)]
    strict: bool,
    #[arg(long, default_value_t = 150, value_name = "MS")]
    debounce_ms: u64,
    #[arg(long, default_value_t = 1000, value_name = "MS")]
    max_wait_ms: u64,
    #[arg(long,default_value="auto",value_parser=["auto","native","poll"])]
    backend: String,
    #[arg(long, default_value_t = 1000, value_name = "MS")]
    poll_interval_ms: u64,
    #[arg(long, default_value_t = 5000, value_name = "MS")]
    reconcile_ms: u64,
    #[arg(long, default_value_t = 30000, value_name = "MS")]
    adapter_timeout_ms: u64,
    /// Bounded test/session hook; zero means run until interrupted.
    #[arg(long, hide = true, default_value_t = 0)]
    stop_after_runs: u64,
}
impl WatchArgs {
    fn request(&self, no_cache: bool) -> WatchRequest {
        let stages = WatchStages {
            validate: self.validate,
            impact: self.impact,
            generate_check: self.generate_check,
        };
        WatchRequest {
            project: self
                .project
                .clone()
                .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
            module: self.module.clone(),
            targets: self.targets.clone(),
            no_cache,
            locked: self.locked,
            strict: self.strict,
            stages: if stages.none() {
                WatchStages::default_selection()
            } else {
                stages
            },
            debounce_ms: self.debounce_ms,
            max_wait_ms: self.max_wait_ms,
            backend: WatchBackend::parse(&self.backend).expect("clap backend enum"),
            poll_interval_ms: self.poll_interval_ms,
            reconcile_ms: self.reconcile_ms,
            adapter_timeout_ms: self.adapter_timeout_ms,
        }
    }
}

struct Native {
    watcher: RecommendedWatcher,
    watched: BTreeSet<PathBuf>,
}
impl Native {
    fn open(
        tx: mpsc::SyncSender<PathBuf>,
        lost: Arc<AtomicBool>,
        failed: Arc<AtomicBool>,
    ) -> Result<Self, ()> {
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            native_event(event, &tx, &lost, &failed);
        })
        .map_err(|_| ())?;
        Ok(Self {
            watcher,
            watched: BTreeSet::new(),
        })
    }
    fn reconcile(&mut self, root: &Path, inventory: &InputSnapshot) -> Result<(), ()> {
        let desired = BTreeSet::from_iter(
            std::iter::once(root.to_owned())
                .chain(inventory.directories.iter().map(|dir| root.join(dir))),
        );
        if desired.len() > core::request::limits::MAX_WATCHED_DIRS {
            return Err(());
        }
        for old in self.watched.difference(&desired) {
            let _ = self.watcher.unwatch(old);
        }
        for new in desired.difference(&self.watched) {
            self.watcher
                .watch(new, RecursiveMode::NonRecursive)
                .map_err(|_| ())?;
        }
        self.watched = desired;
        Ok(())
    }
}
// Shared by the real callback and hermetic inotify/FSEvents event traces.
fn native_event(
    event: notify::Result<notify::Event>,
    tx: &mpsc::SyncSender<PathBuf>,
    lost: &AtomicBool,
    failed: &AtomicBool,
) {
    use core::backend::{native_verdict, NativeEvent, NativeVerdict};
    let model = match &event {
        Ok(event) => NativeEvent {
            error: false,
            access_only: matches!(event.kind, EventKind::Access(_)),
            metadata_only: matches!(
                event.kind,
                EventKind::Modify(notify::event::ModifyKind::Metadata(_))
            ),
            need_rescan: event.need_rescan(),
        },
        Err(_) => NativeEvent {
            error: true,
            ..Default::default()
        },
    };
    match native_verdict(model) {
        NativeVerdict::Ignore => {}
        NativeVerdict::Deliver => {
            // Preserve native paths without a lossy string round trip.
            for path in event.expect("deliver is an ordinary event").paths {
                if tx.try_send(path).is_err() {
                    lost.store(true, Ordering::SeqCst);
                }
            }
        }
        NativeVerdict::Overflow => lost.store(true, Ordering::SeqCst),
        NativeVerdict::BackendFailed => failed.store(true, Ordering::SeqCst),
    }
}
#[cfg(test)]
#[path = "watch_backend_tests.rs"]
mod backend_tests;
#[cfg(test)]
#[path = "watch_output_tests.rs"]
mod output_tests;

struct Output {
    json: bool,
    sequence: u64,
    session: String,
    writer: mpsc::SyncSender<(String, mpsc::SyncSender<io::Result<()>>)>,
    failed: std::cell::Cell<bool>,
}
impl Output {
    fn new(json: bool) -> Self {
        Self::with_writer(json, io::stdout())
    }
    fn with_writer(json: bool, mut out: impl Write + Send + 'static) -> Self {
        let (writer, rx) = mpsc::sync_channel::<(String, mpsc::SyncSender<io::Result<()>>)>(1);
        std::thread::spawn(move || {
            while let Ok((line, reply)) = rx.recv() {
                let result = out.write_all(line.as_bytes()).and_then(|()| out.flush());
                let failed = result.is_err();
                let _ = reply.send(result);
                if failed {
                    break;
                }
            }
        });
        Self {
            json,
            sequence: 0,
            session: format!("s-{}", std::process::id()),
            writer,
            failed: std::cell::Cell::new(false),
        }
    }
    fn deliver(&self, line: String, timeout: Duration) -> io::Result<()> {
        if self.failed.get() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "watch output failed",
            ));
        }
        let result = (|| {
            if line.len() > core::request::limits::MAX_OUTPUT_QUEUE_BYTES {
                return Err(io::Error::other("watch output bound"));
            }
            let (reply, rx) = mpsc::sync_channel(1);
            self.writer.try_send((line, reply)).map_err(|_| {
                io::Error::new(io::ErrorKind::BrokenPipe, "watch output unavailable")
            })?;
            rx.recv_timeout(timeout)
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "watch output deadline"))?
        })();
        if result.is_err() {
            self.failed.set(true);
        }
        result
    }
    fn next(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }
    fn emit(&self, event: WatchEvent) -> io::Result<()> {
        if self.failed.get() {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "watch output failed",
            ));
        }
        let mut line = if self.json {
            event.to_json_line()
        } else {
            let value = event.value();
            match event.event() {
                "session-started" => format!(
                    "watch: {} backend{}",
                    value["backend"].as_str().unwrap_or("unknown"),
                    if value["fallback"] == true {
                        " (poll fallback)"
                    } else {
                        ""
                    }
                ),
                "run-completed" => {
                    let result = &value["result"];
                    format!(
                        "watch run {}: {} | {} executed, {} reused | {} diagnostics | {}",
                        value["runId"],
                        value["status"].as_str().unwrap_or("unknown"),
                        result["counts"]["executedUnits"],
                        result["counts"]["reusedUnits"],
                        result["counts"]["diagnostics"],
                        value["sourceRevision"].as_str().unwrap_or("unknown")
                    )
                }
                _ => format!(
                    "watch: {}{}",
                    event.event(),
                    event
                        .run_id()
                        .map(|id| format!(" run {id}"))
                        .unwrap_or_default()
                ),
            }
        };
        if line.len() > core::events::MAX_EVENT_BYTES {
            return Err(io::Error::other("watch output bound"));
        }
        line.push('\n');
        if !self.json && event.event() == "run-completed" {
            for diagnostic in event.value()["result"]["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
            {
                use std::fmt::Write as _;
                writeln!(
                    line,
                    "  {} [{}]",
                    diagnostic["id"].as_str().unwrap_or("diagnostic"),
                    diagnostic["severity"].as_str().unwrap_or("unknown")
                )
                .expect("String write");
            }
        }
        // One producer awaits each acknowledgement. Failure is latched, so
        // at most one <=16 MiB record is outstanding even after a timeout.
        // A blocked OS write is process-owned; fail-stop the CLI rather than
        // blocking cancellation/adapter teardown on an unresponsive reader.
        self.deliver(line, Duration::from_secs(5))
    }
}
struct Active {
    id: u64,
    generation: u64,
    captured: InputSnapshot,
    cancel: Arc<AtomicBool>,
    cancel_since: Option<Instant>,
    reply: Receiver<(WatchService, Result<RunOutcome, DomainResult>)>,
    join: std::thread::JoinHandle<()>,
}
fn start(
    service: WatchService,
    snapshot: InputSnapshot,
    id: u64,
    generation: u64,
    full: bool,
) -> Active {
    let cancel = Arc::new(AtomicBool::new(core::interrupt::interrupted()));
    let worker_cancel = cancel.clone();
    let (tx, reply) = mpsc::sync_channel(1);
    let captured = snapshot.without_bytes();
    let join = std::thread::spawn(move || {
        let mut service = service;
        let outcome = tempfile::tempdir()
            .map_err(|_| warning_result("auto", Status::Unavailable))
            .map(|scratch| {
                service.run_revision(
                    &snapshot,
                    scratch.path(),
                    &RunContext {
                        run_id: id,
                        generation,
                        full_rescan: full,
                        cancel: &worker_cancel,
                    },
                )
            });
        drop(snapshot);
        let _ = tx.send((service, outcome));
    });
    Active {
        id,
        generation,
        captured,
        cancel,
        cancel_since: None,
        reply,
        join,
    }
}
const TEARDOWN_TIMEOUT: Duration = Duration::from_secs(15);
fn remaining_teardown(worker: &Active) -> Duration {
    TEARDOWN_TIMEOUT.saturating_sub(
        worker
            .cancel_since
            .map_or(Duration::ZERO, |since| since.elapsed()),
    )
}
fn wait_finished(join: &std::thread::JoinHandle<()>, timeout: Duration) -> bool {
    let since = Instant::now();
    while !join.is_finished() {
        if since.elapsed() >= timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    true
}
fn report_incomplete(output: &mut Output, worker: &Active) {
    let seq = output.next();
    let _ = output.emit(WatchEvent::cancellation_incomplete(
        seq,
        &output.session,
        worker.id,
        worker.generation,
        &worker.captured.source_revision(),
    ));
}
fn dirty(
    rx: &Receiver<PathBuf>,
    root: &Path,
    policy: &CapturePolicy,
    inventory: &InputSnapshot,
    scheduler: &mut Scheduler,
    now: u64,
) {
    let paths: BTreeSet<PathBuf> = std::iter::from_fn(|| rx.try_recv().ok())
        .take(core::request::limits::MAX_PENDING_PATHS)
        .collect();
    let mut read_budget = 64 * 1024 * 1024usize;
    for path in paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let path = relative.to_string_lossy().replace('\\', "/");
        let classified = policy.classify_event_path(&path);
        if classified == snapshot::EventClass::Ignored
            || (classified == snapshot::EventClass::Generated && !policy.generate_check)
        {
            continue;
        }
        let record = inventory
            .files
            .get(&path)
            .or_else(|| inventory.generated.get(&path));
        if let Ok(fs) = lekalo_core::project_fs::Fs::open(root) {
            let (dir, name) = path.rsplit_once('/').unwrap_or(("", &path));
            if inventory.directories.contains(&path)
                && matches!(
                    fs.entry_type(dir, name),
                    Ok(lekalo_core::project_fs::EntryType::Directory)
                )
            {
                continue;
            }
            if let Some(record) = record {
                if read_budget == 0 {
                    scheduler.require_full_rescan(now);
                    break;
                }
                let max = read_budget.min(8 * 1024 * 1024);
                if let Ok(Some(bytes)) = fs.read_file_opt(dir, name, max) {
                    read_budget = read_budget.saturating_sub(bytes.len());
                    // Poll semantics: identical bytes after a touch are
                    // not a change (content digest, never the timestamp).
                    if !core::backend::poll_content_changed(
                        Some(record.sha256.as_str()),
                        Some(&bytes),
                    ) {
                        continue;
                    }
                }
            }
        }
        match classified {
            snapshot::EventClass::Semantic => {
                scheduler.note_path(&path, now);
            }
            snapshot::EventClass::Generated if policy.generate_check => {
                scheduler.note_path(&path, now);
            }
            _ => {}
        }
    }
}
fn report_refusal(
    output: &mut Output,
    previous: &mut Option<String>,
    error: &snapshot::CaptureError,
) -> io::Result<()> {
    let failure = error.domain_result();
    let key =
        serde_json::to_string(failure.diagnostics()).expect("classified diagnostics serialize");
    if previous.as_ref() != Some(&key) {
        let seq = output.next();
        output.emit(WatchEvent::recovery_update(
            seq,
            &output.session,
            "inventory-mismatch",
            None,
            Some(&failure),
        ))?;
        *previous = Some(key);
    }
    Ok(())
}
fn warning_result(rule: &str, status: Status) -> DomainResult {
    let diagnostic = core::diagnostics::backend_unavailable(rule);
    let set = lekalo_core::diagnostics::DiagnosticSet::try_from_unsorted(vec![diagnostic], status)
        .expect("watch backend rule");
    DomainResult::unavailable(set)
}

/// Before a session starts, ordinary classified failures go through the
/// CLI DomainResult emitter. Once started, stdout contains only watch
/// events, including shutdown; the return value owns the process exit.
pub fn run(args: WatchArgs, json_requested: bool, no_cache: bool) -> Result<u8, DomainResult> {
    let stream = args.json_lines && !json_requested;
    match run_inner(args, json_requested, no_cache) {
        Err(_) if core::interrupt::interrupted() => {
            if stream {
                let output = Output::new(true);
                if output
                    .emit(WatchEvent::session_stopped(
                        1,
                        &output.session,
                        "interrupt",
                        0,
                        None,
                    ))
                    .is_err()
                {
                    return Ok(1);
                }
            }
            Ok(0)
        }
        Err(failure) if stream => {
            let mut output = Output::new(true);
            let sequence = output.next();
            if output
                .emit(WatchEvent::recovery_update(
                    sequence,
                    &output.session,
                    "inventory-mismatch",
                    None,
                    Some(&failure),
                ))
                .is_err()
            {
                return Ok(1);
            }
            let sequence = output.next();
            if output
                .emit(WatchEvent::session_stopped(
                    sequence,
                    &output.session,
                    "terminal-refusal",
                    0,
                    Some(failure.status()),
                ))
                .is_err()
            {
                return Ok(1);
            }
            Ok(failure.exit_code())
        }
        result => result,
    }
}

fn run_inner(args: WatchArgs, json_requested: bool, no_cache: bool) -> Result<u8, DomainResult> {
    if json_requested {
        return Err(core::diagnostics::json_usage_error());
    }
    let json = args.json_lines;
    let request = args.request(no_cache);
    request.validate()?;
    if !core::interrupt::install() {
        return Err(warning_result(
            request.backend.as_str(),
            Status::Unavailable,
        ));
    }
    let root = lekalo_core::orchestration::project_root(&LoadSelection {
        project: request.project.clone(),
    })?;
    let policy = CapturePolicy {
        generate_check: request.stages.generate_check,
        target_roots: core::targets::preflight(
            &root,
            &request.targets,
            request.adapter_timeout_ms,
        )?,
    };
    let discovery = snapshot::capture_with_policy(&root, &policy).map_err(|e| e.domain_result())?;
    if let Some(scope) = &request.module {
        let scratch =
            tempfile::tempdir().map_err(|_| warning_result("auto", Status::Unavailable))?;
        snapshot::materialize(&discovery, scratch.path()).map_err(|error| error.domain_result())?;
        let model = lekalo_core::loader::normalize_captured_root(scratch.path())?;
        let compilation =
            lekalo_core::ir::compile(&model).map_err(|failure| failure.into_result())?;
        if !compilation
            .project
            .modules
            .iter()
            .any(|module| module.id.as_str() == scope)
        {
            let profile =
                lekalo_core::validator::ValidationProfile::embedded_default().map_err(|_| {
                    DomainResult::invalid(lekalo_core::validator::registry_invariant_failure())
                })?;
            return Err(DomainResult::invalid(
                lekalo_core::validator::validate_scoped(&compilation, profile, scope)
                    .expect_err("unknown scope refuses before rules"),
            ));
        }
    }
    let (tx, rx) = mpsc::sync_channel(core::request::limits::MAX_PENDING_PATHS);
    let lost = Arc::new(AtomicBool::new(false));
    let failed = Arc::new(AtomicBool::new(false));
    let mut native = if request.backend == WatchBackend::Poll {
        None
    } else {
        match Native::open(tx, lost.clone(), failed.clone())
            .and_then(|mut native| native.reconcile(&root, &discovery).map(|()| native))
        {
            Ok(native) => Some(native),
            Err(_) if request.backend == WatchBackend::Auto => None,
            Err(_) => return Err(warning_result("native", Status::Unavailable)),
        }
    };
    let baseline =
        snapshot::capture_with_policy(&root, &policy).map_err(|error| error.domain_result())?;
    let service = WatchService::new(request.clone())?;
    let mut output = Output::new(json);
    let backend = if native.is_some() { "native" } else { "poll" };
    let seq = output.next();
    if output
        .emit(WatchEvent::session_started(
            seq,
            &output.session,
            backend,
            native.is_none() && request.backend == WatchBackend::Auto,
            request.stages.selected(),
            request.module.clone(),
            request.debounce_ms,
            request.max_wait_ms,
        ))
        .is_err()
    {
        return Ok(1);
    }
    // Readiness precedes the published baseline: discovery was not a
    // committed run, and the subscription is already accepting events.
    let mut inventory = baseline.without_bytes();
    let seq = output.next();
    if output
        .emit(WatchEvent::run_started(
            seq,
            &output.session,
            1,
            0,
            &baseline.source_revision(),
            (baseline.files.len() + baseline.generated.len()) as u64,
            true,
        ))
        .is_err()
    {
        return Ok(1);
    }
    let mut scheduler = Scheduler::new(&request);
    scheduler.baseline_started(1);
    let mut active = Some(start(service, baseline, 1, 0, true));
    let mut available = None;
    let epoch = Instant::now();
    let mut last_poll = 0;
    let mut runs = 0;
    let mut last_status = None;
    let mut stop_reason = "interrupt";
    let mut terminal = false;
    let mut blocked = false;
    let mut last_refusal = None;
    loop {
        let now = epoch.elapsed().as_millis().min(u64::MAX as u128) as u64;
        if active.as_ref().is_some_and(|worker| {
            worker.cancel_since.is_some() && remaining_teardown(worker).is_zero()
        }) {
            stop_reason = "terminal-refusal";
            terminal = true;
            break;
        }
        dirty(&rx, &root, &policy, &inventory, &mut scheduler, now);
        let subscription_lost = failed.swap(false, Ordering::SeqCst);
        let overflow = lost.swap(false, Ordering::SeqCst);
        let recovery = core::backend::recovery(request.backend, subscription_lost, overflow);
        if recovery != core::backend::Recovery::None {
            scheduler.require_full_rescan(now);
            if recovery == core::backend::Recovery::PollFallback {
                native = None;
            }
            let seq = output.next();
            if output
                .emit(WatchEvent::recovery_update(
                    seq,
                    &output.session,
                    if subscription_lost {
                        "subscription-lost"
                    } else {
                        "overflow"
                    },
                    Some(if native.is_some() { "native" } else { "poll" }),
                    None,
                ))
                .is_err()
            {
                stop_reason = "output-failure";
                terminal = true;
                break;
            }
        }
        if recovery == core::backend::Recovery::Stop {
            stop_reason = "terminal-refusal";
            terminal = true;
            break;
        }
        let interval = if native.is_some() {
            request.reconcile_ms
        } else {
            request.poll_interval_ms
        };
        if now.saturating_sub(last_poll) >= interval {
            last_poll = now;
            match snapshot::poll_with_policy(&root, &policy) {
                Ok(current) => {
                    if blocked {
                        blocked = false;
                        last_refusal = None;
                        scheduler.require_full_rescan(now);
                    }
                    // A capture may already have installed the new
                    // inventory; reconcile subscriptions independently.
                    if let Some(backend) = &mut native {
                        if backend.reconcile(&root, &current).is_err() {
                            failed.store(true, Ordering::SeqCst);
                            scheduler.require_full_rescan(now);
                        }
                    }
                    if !current.same_inventory(&inventory) {
                        for row in core::changes::diff(&inventory, &current).rows {
                            scheduler.note_path(&row.path, now);
                        }
                        if current.directories != inventory.directories {
                            scheduler.require_full_rescan(now);
                        }
                        inventory = current.clone();
                    }
                }
                Err(error) => {
                    blocked = true;
                    scheduler.require_full_rescan(now);
                    if report_refusal(&mut output, &mut last_refusal, &error).is_err() {
                        stop_reason = "output-failure";
                        terminal = true;
                        break;
                    }
                }
            }
        }
        if core::interrupt::interrupted() {
            break;
        }
        match if blocked && !scheduler.is_active() {
            Decision::None
        } else {
            scheduler.poll(now)
        } {
            Decision::CancelStale { .. } => {
                if let Some(active) = &mut active {
                    active.cancel.store(true, Ordering::SeqCst);
                    active.cancel_since.get_or_insert_with(Instant::now);
                }
            }
            Decision::Start {
                run_id,
                generation,
                full_rescan,
                ..
            } => match snapshot::capture_with_policy(&root, &policy) {
                Ok(snapshot) => {
                    let changed_files = core::changes::diff(&inventory, &snapshot).len() as u64;
                    inventory = snapshot.without_bytes();
                    let seq = output.next();
                    if output
                        .emit(WatchEvent::run_started(
                            seq,
                            &output.session,
                            run_id,
                            generation,
                            &snapshot.source_revision(),
                            changed_files,
                            full_rescan,
                        ))
                        .is_err()
                    {
                        stop_reason = "output-failure";
                        terminal = true;
                        break;
                    }
                    active = Some(start(
                        available.take().expect("previous worker terminated"),
                        snapshot,
                        run_id,
                        generation,
                        full_rescan,
                    ));
                }
                Err(error) => {
                    scheduler.run_finished(run_id);
                    scheduler.require_full_rescan(now);
                    blocked = true;
                    if report_refusal(&mut output, &mut last_refusal, &error).is_err() {
                        stop_reason = "output-failure";
                        terminal = true;
                        break;
                    }
                }
            },
            Decision::None => {}
        }
        let reply = active.as_ref().map(|worker| worker.reply.try_recv());
        match reply {
            Some(Ok((mut service, outcome))) => {
                let worker = active.take().expect("reply owns active worker");
                if !wait_finished(&worker.join, remaining_teardown(&worker)) {
                    worker.cancel.store(true, Ordering::SeqCst);
                    report_incomplete(&mut output, &worker);
                    stop_reason = "terminal-refusal";
                    terminal = true;
                    break;
                }
                if worker.join.join().is_err() {
                    stop_reason = "terminal-refusal";
                    terminal = true;
                    break;
                }
                dirty(&rx, &root, &policy, &inventory, &mut scheduler, now);
                // Notification delivery is not authority. Revalidate the
                // complete captured inventory immediately before commit.
                match snapshot::poll_with_policy(&root, &policy) {
                    Ok(current) if !current.same_inventory(&worker.captured) => {
                        for row in core::changes::diff(&worker.captured, &current).rows {
                            scheduler.note_path(&row.path, now);
                        }
                        if current.directories != worker.captured.directories {
                            scheduler.require_full_rescan(now);
                        }
                        inventory = current;
                    }
                    Err(error) => {
                        blocked = true;
                        scheduler.require_full_rescan(now);
                        if report_refusal(&mut output, &mut last_refusal, &error).is_err() {
                            stop_reason = "output-failure";
                            terminal = true;
                        }
                    }
                    _ => {}
                }
                let stale = worker.cancel.load(Ordering::SeqCst)
                    || scheduler.generation() != worker.generation;
                scheduler.run_finished(worker.id);
                let seq = output.next();
                let event = match outcome {
                    Ok(outcome) if !stale && !outcome.stale => {
                        last_status = Some(outcome.status);
                        runs += 1;
                        let event = WatchEvent::run_completed(
                            seq,
                            &output.session,
                            worker.id,
                            worker.generation,
                            &outcome.source_revision,
                            outcome.status,
                            outcome.result,
                        );
                        service.commit(outcome.staged);
                        event
                    }
                    Ok(_) => WatchEvent::run_cancelled(
                        seq,
                        &output.session,
                        worker.id,
                        worker.generation,
                        &worker.captured.source_revision(),
                        scheduler.generation(),
                    ),
                    Err(_) => {
                        stop_reason = "terminal-refusal";
                        terminal = true;
                        break;
                    }
                };
                available = Some(service);
                if output.emit(event).is_err() {
                    stop_reason = "output-failure";
                    terminal = true;
                    break;
                }
                if args.stop_after_runs > 0 && runs >= args.stop_after_runs {
                    stop_reason = "stop-after-runs";
                    break;
                }
            }
            Some(Err(TryRecvError::Disconnected)) => {
                stop_reason = "terminal-refusal";
                terminal = true;
                break;
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(mut worker) = active {
        worker.cancel.store(true, Ordering::SeqCst);
        worker.cancel_since.get_or_insert_with(Instant::now);
        if wait_finished(&worker.join, remaining_teardown(&worker)) {
            if worker.join.join().is_err() {
                terminal = true;
                stop_reason = "terminal-refusal";
            }
            let seq = output.next();
            let _ = output.emit(WatchEvent::run_cancelled(
                seq,
                &output.session,
                worker.id,
                worker.generation,
                &worker.captured.source_revision(),
                scheduler.generation(),
            ));
        } else {
            // No fake cancellation terminal and no overlapping replacement.
            report_incomplete(&mut output, &worker);
            terminal = true;
            stop_reason = "terminal-refusal";
        }
    }
    drop(native);
    let seq = output.next();
    if output
        .emit(WatchEvent::session_stopped(
            seq,
            &output.session,
            stop_reason,
            runs,
            last_status,
        ))
        .is_err()
    {
        return Ok(1);
    }
    Ok(if stop_reason == "output-failure" {
        1
    } else if terminal {
        4
    } else if stop_reason == "interrupt" {
        0
    } else {
        last_status.unwrap_or(Status::Valid).exit_code()
    })
}

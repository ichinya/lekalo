//! Cooperative MCP worker cancellation at the CLI read/process boundary.
//!
//! A timed-out, cancelled, or EOF-abandoned worker may be inside a pure
//! core traversal that cannot be interrupted safely; it keeps its slot
//! until completion. Its thread-local guard nevertheless prevents NEW
//! child processes after abandonment (review r4 F10), and the bounded
//! Git adapters terminate their process trees on checkpoints during waits.

use std::cell::RefCell;
use std::io::{self, Read};
use std::process::{Child, Command, Output};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct WorkGuard {
    abandoned: Arc<AtomicBool>,
    spawn_gate: Arc<Mutex<()>>,
    deadline: Instant,
}

impl WorkGuard {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            abandoned: Arc::new(AtomicBool::new(false)),
            spawn_gate: Arc::new(Mutex::new(())),
            deadline: Instant::now() + Duration::from_millis(timeout_ms),
        }
    }

    pub fn abandon(&self) {
        // Serialize abandonment with a worker's actual process spawn:
        // after this returns, no new child can be created (r4 F10).
        let _gate = self.spawn_gate.lock().expect("spawn gate");
        self.abandoned.store(true, Ordering::SeqCst);
    }

    pub fn should_stop(&self) -> bool {
        self.abandoned.load(Ordering::SeqCst) || Instant::now() >= self.deadline
    }
}

thread_local! {
    static CURRENT: RefCell<Option<WorkGuard>> = const { RefCell::new(None) };
}

/// Enter one worker's guard for the duration of its CLI/core call; the
/// previous context is restored even if the worker panics.
pub fn with_guard<T>(guard: WorkGuard, work: impl FnOnce() -> T) -> T {
    struct Restore(Option<WorkGuard>);
    impl Drop for Restore {
        fn drop(&mut self) {
            CURRENT.with(|slot| *slot.borrow_mut() = self.0.take());
        }
    }
    let previous = CURRENT.with(|slot| slot.borrow_mut().replace(guard));
    let _restore = Restore(previous);
    work()
}

/// A spawn/read checkpoint from the CLI edge: ordinary CLI commands have
/// no MCP guard and continue unchanged.
pub fn should_stop() -> bool {
    CURRENT.with(|slot| slot.borrow().as_ref().is_some_and(WorkGuard::should_stop))
}

/// Serialize a worker's spawn with cancellation; ordinary CLI commands
/// still spawn normally. A returned `None` means the worker was already
/// abandoned/deadlined and must not start another process.
pub fn spawn_if_active(command: &mut Command) -> std::io::Result<Option<Child>> {
    CURRENT.with(|slot| {
        let borrowed = slot.borrow();
        let Some(guard) = borrowed.as_ref() else {
            return command.spawn().map(Some);
        };
        let _gate = guard.spawn_gate.lock().expect("spawn gate");
        if guard.should_stop() {
            Ok(None)
        } else {
            command.spawn().map(Some)
        }
    })
}

/// Overrides precede the subcommand, where Git actually parses them.
/// Optional locks alone do not prevent fsmonitor hooks, clean filters,
/// submodule helpers, or partial-clone lazy fetches (r8 F1).
const NEUTRAL_GIT_CONFIG: [(&str, &str); 10] = [
    ("core.fsmonitor", "false"),
    ("core.untrackedCache", "false"),
    ("core.pager", ""),
    ("core.commitGraph", "false"),
    ("core.multiPackIndex", "false"),
    ("submodule.recurse", "false"),
    ("commit.gpgsign", "false"),
    ("tag.gpgsign", "false"),
    ("protocol.allow", "never"),
    ("protocol.file.allow", "never"),
];

#[cfg(windows)]
const NULL_FILE: &str = "NUL";
#[cfg(not(windows))]
const NULL_FILE: &str = "/dev/null";

/// Admit the adapters' exact argument shapes, not just read-like command
/// names: e.g. `diff --output=...` is a write even though `diff` is a read.
fn admitted_git_args(args: &[&str]) -> bool {
    let resolved = |revision: &str| {
        revision == "HEAD" || lekalo_core::impact::input::is_revision_grammar(revision)
    };
    match args {
        ["status", "--porcelain"] | ["ls-files", "--others", "--exclude-standard", "-z"] => true,
        ["rev-parse", "--verify", "--quiet", revision] => {
            let selector = revision.strip_suffix("^{commit}").unwrap_or(revision);
            !selector.is_empty()
                && !selector.starts_with('-')
                && selector.len() <= 512
                && !selector.chars().any(char::is_control)
        }
        ["diff", "--name-status", "--no-renames", "-z", one] => resolved(one),
        ["diff", "--name-status", "--no-renames", "-z", one, two] => resolved(one) && resolved(two),
        ["rev-list", "--max-count=1", "--parents", revision] => resolved(revision),
        _ => false,
    }
}

fn neutral_git(command: &Command) -> Command {
    let mut rebuilt = Command::new(command.get_program());
    rebuilt.arg("--no-pager");
    for (key, value) in NEUTRAL_GIT_CONFIG {
        rebuilt.arg("-c").arg(format!("{key}={value}"));
    }
    for key in ["core.hooksPath", "core.attributesFile"] {
        rebuilt.arg("-c").arg(format!("{key}={NULL_FILE}"));
    }
    rebuilt
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_SYSTEM", NULL_FILE)
        .env("GIT_CONFIG_GLOBAL", NULL_FILE)
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_ALLOW_PROTOCOL", "")
        .env("GIT_TERMINAL_PROMPT", "0");
    // Native Git on Windows needs the OS directory, not the launcher's
    // Git config, tracing, repository selectors, or helper environment.
    #[cfg(windows)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        rebuilt.env("SystemRoot", system_root);
    }
    if let Some(directory) = command.get_current_dir() {
        rebuilt.current_dir(directory);
    }
    rebuilt.stdin(std::process::Stdio::null());
    rebuilt.stdout(std::process::Stdio::piped());
    rebuilt.stderr(std::process::Stdio::piped());
    rebuilt
}

/// Hermetic read-only Git adapter seam (r8 F1). In particular, Git can
/// invoke clean/process filters even for name-only worktree comparisons.
/// Discover only filter KEY NAMES with Git's non-executing config reader,
/// then override every driver before the admitted fact command starts.
pub fn git_output(command: &mut Command) -> io::Result<Output> {
    let args: Option<Vec<&str>> = command.get_args().map(|arg| arg.to_str()).collect();
    let args = args
        .filter(|args| admitted_git_args(args))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unadmitted Git read"))?;
    let head = args[0];
    let mut config = neutral_git(command);
    config.args([
        "config",
        "--null",
        "--name-only",
        "--get-regexp",
        r"^filter\..*\.(clean|smudge|process|required)$",
    ]);
    let filters = bounded_output(&mut config)?;
    if !filters.status.success() && filters.status.code() != Some(1) {
        return Err(io::Error::other("Git configuration unavailable"));
    }
    let keys = std::str::from_utf8(&filters.stdout)
        .map_err(|_| io::Error::other("Git filter configuration unavailable"))?;
    let mut rebuilt = neutral_git(command);
    for key in keys.split('\0').filter(|key| !key.is_empty()) {
        let (driver, _) = key
            .rsplit_once('.')
            .ok_or_else(|| io::Error::other("Git filter key"))?;
        for suffix in ["clean=", "smudge=", "process=", "required=false"] {
            rebuilt.arg("-c").arg(format!("{driver}.{suffix}"));
        }
    }
    rebuilt.arg(head);
    if head == "status" {
        rebuilt.arg("--ignore-submodules=all");
    } else if head == "diff" {
        rebuilt.args(["--no-ext-diff", "--no-textconv", "--ignore-submodules=all"]);
    }
    rebuilt.args(&args[1..]);
    bounded_output(&mut rebuilt)
}

/// Shared lifetime supervision, also exercised with deterministic process
/// fixtures: drain both pipes under a hard cap and reap the entire tree.
fn bounded_output(command: &mut Command) -> io::Result<Output> {
    const CAP: usize = 4 * 1024 * 1024;
    const TIMEOUT: Duration = Duration::from_secs(5);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = spawn_if_active(command)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::Interrupted, "abandoned Git work"))?;
    let tree = match ProcessTree::attach(&child) {
        Ok(tree) => tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    let flooded = Arc::new(AtomicBool::new(false));
    let drain = |pipe: Option<Box<dyn Read + Send>>, flooded: Arc<AtomicBool>| {
        std::thread::spawn(move || -> io::Result<Vec<u8>> {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let mut chunk = [0; 8192];
                loop {
                    let read = pipe.read(&mut chunk)?;
                    if read == 0 {
                        break;
                    }
                    let keep = read.min(CAP.saturating_sub(bytes.len()));
                    bytes.extend_from_slice(&chunk[..keep]);
                    if keep < read {
                        flooded.store(true, Ordering::SeqCst);
                    }
                }
            }
            Ok(bytes)
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        flooded.clone(),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
        flooded.clone(),
    );
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if should_stop() || Instant::now() >= deadline || flooded.load(Ordering::SeqCst) {
            break Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "bounded Git work refused",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) if stdout.is_finished() && stderr.is_finished() => break Ok(status),
            Ok(_) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => break Err(error),
        }
    };
    // Also reap descendants that closed their pipes but outlived Git.
    tree.terminate();
    if status.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    let stdout = stdout
        .join()
        .map_err(|_| io::Error::other("Git stdout reader panicked"))?;
    let stderr = stderr
        .join()
        .map_err(|_| io::Error::other("Git stderr reader panicked"))?;
    let status = status?;
    if flooded.load(Ordering::SeqCst) {
        return Err(io::Error::other("Git output limit"));
    }
    Ok(Output {
        status,
        stdout: stdout?,
        stderr: stderr?,
    })
}

/// OS lifetime custody, not a detached thread pretending the child stopped.
struct ProcessTree {
    #[cfg(unix)]
    pid: u32,
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}

impl ProcessTree {
    fn attach(child: &Child) -> io::Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self { pid: child.id() })
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::*;
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let tree = Self { job };
                let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32,
                ) == 0
                    || AssignProcessToJobObject(job, child.as_raw_handle()) == 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(tree)
            }
        }
    }

    fn terminate(&self) {
        #[cfg(unix)]
        if let Some(pid) = rustix::process::Pid::from_raw(self.pid as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
    }
}

impl Drop for ProcessTree {
    fn drop(&mut self) {
        self.terminate();
        #[cfg(windows)]
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The same test executable is a portable native subprocess fixture;
    // no shell, global environment mutation, or external runtime needed.
    #[test]
    fn subprocess_fixture() {
        let Ok(mode) = std::env::var("LEKALO_MCP_GIT_TEST") else {
            return;
        };
        let marker = std::path::PathBuf::from(std::env::var_os("LEKALO_MCP_GIT_MARKER").unwrap());
        if mode == "flood" {
            use std::io::Write;
            std::io::stdout()
                .write_all(&vec![b'x'; 5 * 1024 * 1024])
                .unwrap();
            std::thread::sleep(Duration::from_secs(30));
            return;
        }
        let mut descendant = if mode == "root" {
            Some(
                fixture_command("leaf", &marker.with_extension("leaf"))
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            )
        } else {
            None
        };
        let writing = marker.with_extension(std::process::id().to_string());
        std::fs::write(&writing, std::process::id().to_string()).unwrap();
        std::fs::rename(writing, &marker).unwrap();
        std::thread::sleep(Duration::from_secs(30));
        if let Some(child) = descendant.as_mut() {
            child.wait().unwrap();
        }
    }

    fn fixture_command(mode: &str, marker: &std::path::Path) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "mcp::work::tests::subprocess_fixture",
                "--nocapture",
            ])
            .env("LEKALO_MCP_GIT_TEST", mode)
            .env("LEKALO_MCP_GIT_MARKER", marker)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        command
    }

    fn process_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            rustix::process::test_kill_process(rustix::process::Pid::from_raw(pid as i32).unwrap())
                .is_ok()
        }
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::System::Threading::*;
            let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if handle.is_null() {
                return false;
            }
            let alive =
                WaitForSingleObject(handle, 0) == windows_sys::Win32::Foundation::WAIT_TIMEOUT;
            windows_sys::Win32::Foundation::CloseHandle(handle);
            alive
        }
    }

    fn assert_stopped(pid: u32) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while process_alive(pid) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !process_alive(pid),
            "supervised process {pid} survived cleanup"
        );
    }

    #[test]
    fn running_git_children_and_descendants_stop_on_cancel_and_timeout() {
        for cancel in [true, false] {
            let scratch = tempfile::tempdir().unwrap();
            let marker = scratch.path().join("root.pid");
            let leaf = marker.with_extension("leaf");
            let guard = WorkGuard::new(if cancel { 30_000 } else { 1500 });
            let worker_guard = guard.clone();
            let mut command = fixture_command("root", &marker);
            let worker = std::thread::spawn(move || {
                with_guard(worker_guard, || bounded_output(&mut command))
            });
            let deadline = Instant::now() + Duration::from_secs(5);
            while (!marker.exists() || !leaf.exists()) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            let parent: u32 = std::fs::read_to_string(&marker)
                .expect("Git child started")
                .parse()
                .unwrap();
            let descendant: u32 = std::fs::read_to_string(&leaf)
                .expect("Git descendant started")
                .parse()
                .unwrap();
            assert!(process_alive(parent), "test must cancel running work");
            if cancel {
                guard.abandon();
            }
            assert!(worker.join().unwrap().is_err());
            assert_stopped(parent);
            assert_stopped(descendant);
        }
    }

    #[test]
    fn chatty_git_is_bounded_and_reaped_without_pipe_deadlock() {
        let scratch = tempfile::tempdir().unwrap();
        let mut command = fixture_command("flood", &scratch.path().join("unused"));
        let started = Instant::now();
        assert!(bounded_output(&mut command).is_err());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn the_git_seam_neutralizes_repository_configured_helpers() {
        // omp r8 F1: with a hostile repository config present, the seam
        // must not let Git execute it. A configured fsmonitor hook (the
        // reviewer's exact vector) is inert under `-c core.fsmonitor=false`.
        let scratch = tempfile::tempdir().unwrap();
        let repo = scratch.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
        let mut command = Command::new("git");
        command
            .current_dir(&repo)
            .args(["status", "--porcelain"])
            .env("GIT_OPTIONAL_LOCKS", "1")
            .env("GIT_CONFIG_GLOBAL", scratch.path().join("global.ini"))
            .env("GIT_CONFIG_SYSTEM", scratch.path().join("system.ini"))
            .env("HOME", scratch.path())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        std::fs::write(
            scratch.path().join("global.ini"),
            "[core]\n\tpager = hostile\n",
        )
        .unwrap();
        let output = git_output(&mut command).expect("read-only status must succeed");
        assert!(
            output.status.success(),
            "git status must succeed hermetically"
        );
        assert!(
            output.stdout.is_empty(),
            "a clean repo has no porcelain rows"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_configured_fsmonitor_hook_never_executes_under_the_seam() {
        // The reviewer's concrete vector (omp r8 F1): repository-configured
        // `core.fsmonitor` executes a native binary on every `status`, even
        // with optional locks disabled. Under the neutralized seam the
        // marker the "hook" would write must stay absent.
        let scratch = tempfile::tempdir().unwrap();
        let repo = scratch.path().join("repo");
        let marker = scratch.path().join("fsmonitor-marker");
        std::fs::create_dir_all(&repo).unwrap();
        assert!(Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
        let hook = scratch.path().join("fake-fsmonitor");
        std::fs::write(
            &hook,
            format!("#!/bin/sh\necho x > '{}\'\nsleep 30\n", marker.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            repo.join(".git/config"),
            format!("[core]\n\tfsmonitor = {}\n", hook.display()),
        )
        .unwrap();
        std::fs::write(repo.join("tracked.txt"), "x\n").unwrap();
        assert!(Command::new("git")
            .arg("add")
            .arg("tracked.txt")
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());
        let mut command = Command::new("git");
        command
            .current_dir(&repo)
            .args(["status", "--porcelain"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let output = git_output(&mut command).expect("read-only status must succeed");
        assert!(output.status.success());
        assert!(
            !marker.exists(),
            "the repository fsmonitor hook must never start"
        );
    }

    #[test]
    fn non_read_only_git_heads_are_not_admitted_by_the_seam() {
        // Admission is a runtime refusal, including release builds.
        let scratch = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(scratch.path().join("repo/.git")).unwrap();
        let mut command = Command::new("git");
        command
            .current_dir(scratch.path().join("repo"))
            .args(["push", "--dry-run"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        assert_eq!(
            git_output(&mut command).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        assert!(!admitted_git_args(&[
            "diff",
            "--output=outside.txt",
            "HEAD"
        ]));
        assert!(!admitted_git_args(&[
            "rev-parse",
            "--verify",
            "--quiet",
            "--option^{commit}"
        ]));
        assert!(admitted_git_args(&[
            "diff",
            "--name-status",
            "--no-renames",
            "-z",
            "HEAD"
        ]));
    }

    #[test]
    fn abandoned_worker_cannot_pass_a_spawn_checkpoint() {
        assert!(!should_stop(), "ordinary CLI has no MCP cancellation");
        let guard = WorkGuard::new(30_000);
        let clone = guard.clone();
        with_guard(guard, || {
            assert!(!should_stop());
            clone.abandon();
            assert!(should_stop());
            assert!(spawn_if_active(&mut Command::new("git")).unwrap().is_none());
        });
        assert!(!should_stop(), "guard must not leak into another call");
        with_guard(WorkGuard::new(0), || assert!(should_stop()));
    }
}

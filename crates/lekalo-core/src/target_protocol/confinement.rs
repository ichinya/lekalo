//! Private scoped project views and OS-enforced execution. The adapter
//! never receives write access to the real project. Only a verified plan
//! is published by core after the isolated process and all its children exit.

use super::{plan, scopes, transport, wire, TargetFailure};
use crate::project_fs::Fs;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[cfg(windows)]
#[path = "confinement_windows.rs"]
mod windows;

/// Windows' hard job-memory cap, also macOS watch's sampled resident
/// kill threshold (degraded, not a hard cap). Never host-derived.
pub(super) const SANDBOX_MEMORY_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The bounded process/task count applied when the budget denies
/// children on Linux. `RLIMIT_NPROC` counts tasks (threads included),
/// so the bound must admit a runtime's own thread pool while still
/// capping fork bombs. It is applied only where it bounds the sandbox
/// namespace itself (kernel ≥ 5.14, see
/// [`PER_USERNS_NPROC_KERNEL`]); it is an honest bound, not a fork
/// primitive.
pub(super) const SANDBOX_TASK_BOUND: u64 = 64;

/// The Linux address-space bound (`RLIMIT_AS`) applied through the
/// prlimit wrapper inside the sandbox namespace. Unlike the Windows
/// job commit-charge cap this counts *reservations*: a V8 runtime
/// reserves multi-GiB virtual cages while committing far less, so the
/// honest bound must cover the reservation, not the commit charge. It
/// is a real cap on runaway mappings — enforcement is recorded
/// `Degraded`, never claimed equivalent to the Windows memory job.
pub(super) const SANDBOX_ADDRESS_SPACE_LIMIT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// The read-only execution lane one host can actually build. This is
/// the hermetic model the platform probes resolve into; the watch
/// target admission (`TargetClient::bounded_read_only_available`)
/// requires both a memory and a process bound before any adapter runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReadOnlyLane {
    /// Windows job object: commit-charge memory cap and an
    /// active-process cap of one when children are denied.
    WindowsJob,
    /// Linux bubblewrap namespace with the prlimit wrapper: an
    /// address-space cap plus the per-user-namespace task bound
    /// (kernel ≥ 5.14). Resource enforcement is honestly `Degraded`.
    LinuxNamespace,
    /// macOS seatbelt denies fork; the host samples owned-process RSS
    /// and kills at the resident threshold. Sampling can overshoot:
    /// resource evidence is degraded, never a hard allocation ceiling.
    MacOsSupervised,
    /// No bounded lane exists (unknown platform, or Linux without
    /// bubblewrap/prlimit/kernel support).
    Unavailable,
}

/// Resolve the lane from platform facts. Pure and testable on any
/// development platform; the callers feed it their `cfg!`/host probes.
pub(super) fn read_only_lane(
    windows: bool,
    linux: bool,
    macos: bool,
    bwrap: bool,
    prlimit: bool,
) -> ReadOnlyLane {
    if windows {
        ReadOnlyLane::WindowsJob
    } else if linux {
        if bwrap && prlimit {
            ReadOnlyLane::LinuxNamespace
        } else {
            ReadOnlyLane::Unavailable
        }
    } else if macos {
        ReadOnlyLane::MacOsSupervised
    } else {
        ReadOnlyLane::Unavailable
    }
}

/// The lane's honest memory bound (`None` when the platform has no
/// primitive and execution must refuse).
pub(super) fn lane_memory_bound(lane: ReadOnlyLane) -> Option<u64> {
    match lane {
        ReadOnlyLane::WindowsJob => Some(SANDBOX_MEMORY_LIMIT_BYTES),
        ReadOnlyLane::LinuxNamespace => Some(SANDBOX_ADDRESS_SPACE_LIMIT_BYTES),
        ReadOnlyLane::MacOsSupervised => Some(SANDBOX_MEMORY_LIMIT_BYTES),
        ReadOnlyLane::Unavailable => None,
    }
}

/// The lane's honest process/task bound (`None` when children are
/// merely unenforced rather than bounded).
pub(super) fn lane_process_bound(lane: ReadOnlyLane, children_denied: bool) -> Option<u64> {
    if !children_denied {
        return None;
    }
    match lane {
        ReadOnlyLane::WindowsJob => Some(1),
        ReadOnlyLane::LinuxNamespace => Some(SANDBOX_TASK_BOUND),
        ReadOnlyLane::MacOsSupervised => Some(1),
        ReadOnlyLane::Unavailable => None,
    }
}

/// Whether the Linux bubblewrap sandbox binary exists (the namespace
/// the prlimit bounds apply inside).
#[cfg(target_os = "linux")]
fn bwrap_available() -> bool {
    Path::new("/usr/bin/bwrap").is_file()
}

#[cfg(not(target_os = "linux"))]
fn bwrap_available() -> bool {
    false
}

/// The per-session sandbox policy the budget projects onto the OS
/// primitives (issue #89).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SandboxPolicy {
    /// Watch's opt-in resource controls (Linux address space / macOS
    /// sampled RSS); predecessor command policies remain unchanged.
    pub bounded_resources: bool,
    /// `processes.children: denied` — children are refused where the
    /// platform has a primitive and bounded/honestly reported where it
    /// does not. Children that do spawn stay inside the sandbox either
    /// way.
    pub children_denied: bool,
    /// `network.mode: allowlist` — no supported platform offers a
    /// namespace-level destination filter, so the sandbox keeps its
    /// denial and the report degrades honestly (issue #89).
    pub network_allowlist: bool,
}

impl SandboxPolicy {
    /// The strictest policy: children denied, network denied (read-only
    /// probes and the describe handshake).
    pub fn strict() -> Self {
        Self {
            bounded_resources: false,
            children_denied: true,
            network_allowlist: false,
        }
    }

    pub fn with_bounded_resources(mut self, required: bool) -> Self {
        self.bounded_resources = required;
        self
    }

    /// Project the session budget onto the sandbox policy.
    pub fn from_budget(budget: &crate::adapter_package::budget::SessionBudget) -> Self {
        use crate::adapter_package::budget::{ChildPolicy, NetworkBudget};
        Self {
            bounded_resources: false,
            children_denied: budget.children() == ChildPolicy::Denied,
            network_allowlist: matches!(budget.network(), NetworkBudget::Allowlist(_)),
        }
    }
}

/// The honest enforcement verdict of one confinement dimension.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Enforcement {
    /// An OS primitive enforces the declared policy on this platform.
    Enforced,
    /// The declared policy cannot be enforced as spelled; the sandbox
    /// degrades to denial/bounding and says so. Never a silent
    /// allowance.
    Degraded,
    /// The platform has no primitive; the gap is recorded, and the
    /// namespace/job containment still applies.
    Unenforced,
}

impl Enforcement {
    /// The stable evidence token.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Enforced => "enforced",
            Self::Degraded => "degraded",
            Self::Unenforced => "unenforced",
        }
    }
}

/// The honest per-dimension confinement record of one sandbox: what
/// the platform enforced, degraded, or could not enforce. Network
/// denial is `enforced` on every supported platform; an allowlist
/// mode degrades to denial (`Degraded`) because no supported platform
/// offers a namespace-level destination filter (issue #89).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ConfinementReport {
    pub network: Enforcement,
    pub children: Enforcement,
    pub resources: Enforcement,
    pub children_denied: bool,
}

impl ConfinementReport {
    /// Compute the honest record for the host platform from the sandbox
    /// policy.
    pub(super) fn compute(policy: SandboxPolicy) -> Self {
        let children = if policy.children_denied {
            if cfg!(windows) || (policy.bounded_resources && macos_supervisor_available()) {
                Enforcement::Enforced
            } else if cfg!(target_os = "linux") && prlimit_available() {
                // Bounded, and only where the bound is namespace-local.
                Enforcement::Degraded
            } else {
                Enforcement::Unenforced
            }
        } else {
            // `declared` is the policy the sandbox already implements:
            // children stay inside the namespace/job.
            Enforcement::Enforced
        };
        let resources = if cfg!(windows) {
            Enforcement::Enforced
        } else if policy.bounded_resources
            && cfg!(target_os = "linux")
            && prlimit_available()
            && bwrap_available()
        {
            // The namespace bounds the adapter's address space through
            // RLIMIT_AS. That is a real cap on runaway mappings, but it
            // counts reservations rather than commit charge — an honest
            // `Degraded`, never a claim of Windows-job equivalence.
            Enforcement::Degraded
        } else if policy.bounded_resources && macos_supervisor_available() {
            // Host-observed RSS kill threshold, not a hard allocation cap.
            Enforcement::Degraded
        } else {
            // Linux RLIMIT_RSS is a historical no-op and macOS exposes
            // no sandbox primitive: recording anything stronger would
            // be a dishonest claim.
            Enforcement::Unenforced
        };
        Self {
            network: if policy.network_allowlist {
                Enforcement::Degraded
            } else {
                Enforcement::Enforced
            },
            children,
            resources,
            children_denied: policy.children_denied,
        }
    }

    /// The effective process bound for the evidence (Windows job cap 1
    /// when denied; the Linux task bound where the wrapper exists and
    /// the kernel bounds the namespace, `None` otherwise).
    pub(super) fn process_limit(&self) -> Option<u64> {
        lane_process_bound(host_read_only_lane(), self.children_denied)
            .filter(|_| self.children != Enforcement::Unenforced)
    }

    /// The effective memory bound for the evidence: the Windows job
    /// commit-charge cap, or the Linux namespace address-space bound.
    pub(super) fn memory_limit(&self) -> Option<u64> {
        lane_memory_bound(host_read_only_lane()).filter(|_| {
            self.resources == Enforcement::Enforced || self.resources == Enforcement::Degraded
        })
    }
}

/// The lane this host can actually build, from its `cfg` platform and
/// the resolved sandbox/wrapper probes.
fn host_read_only_lane() -> ReadOnlyLane {
    if cfg!(target_os = "macos") && !macos_supervisor_available() {
        return ReadOnlyLane::Unavailable;
    }
    read_only_lane(
        cfg!(windows),
        cfg!(target_os = "linux"),
        cfg!(target_os = "macos"),
        bwrap_available(),
        prlimit_available(),
    )
}

#[cfg(target_os = "macos")]
fn macos_supervisor_available() -> bool {
    super::memory::available()
}
#[cfg(not(target_os = "macos"))]
fn macos_supervisor_available() -> bool {
    false
}

/// Whether the Linux `prlimit` wrapper is available for the bounded
/// children policy — and the kernel charges `RLIMIT_NPROC` per user
/// namespace, so a fixed `--nproc` bound tracks the sandbox namespace
/// rather than the whole-uid host task count. Otherwise the wrapper is
/// skipped and the report honestly says `denied-unenforced`
/// (issue #89 fix round 2, C-F2).
#[cfg(target_os = "linux")]
fn prlimit_available() -> bool {
    prlimit_path().is_some() && kernel_bounds_nproc_per_userns()
}

/// The prlimit wrapper location, resolved once to the first existing
/// path. The availability check and the spawned wrapper share this one
/// resolution, so a report of `denied-bounded` always corresponds to a
/// wrapper that was actually spawnable (issue #89 fix round 2, C-F7).
#[cfg(target_os = "linux")]
fn prlimit_path() -> Option<PathBuf> {
    static RESOLVED: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    RESOLVED
        .get_or_init(|| {
            ["/usr/bin/prlimit", "/bin/prlimit"]
                .iter()
                .map(Path::new)
                .find(|path| path.is_file())
                .map(Path::to_path_buf)
        })
        .clone()
}

/// The kernel release where the ucounts rework (5.14,
/// torvalds/linux@21d1c5e386bc) made `RLIMIT_NPROC` a per-uid-per-
/// user-namespace charge: a fresh `--unshare-all` namespace starts its
/// task count at zero, so a fixed `--nproc` bound bounds the namespace
/// deterministically. Older kernels count tasks per real uid across the
/// whole host, where a fixed bound denies the runtime's own threads on
/// any busy machine — fail-closed, but wrong.
#[cfg(target_os = "linux")]
const PER_USERNS_NPROC_KERNEL: (u64, u64) = (5, 14);

/// Parse the leading `major.minor` of a kernel release string
/// (`6.5.0-18-generic` → `(6, 5)`); `None` when unparseable.
#[cfg(target_os = "linux")]
fn parse_kernel_release(release: &str) -> Option<(u64, u64)> {
    let mut parts = release.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    Some((major, minor))
}

/// Whether the host kernel charges `RLIMIT_NPROC` per user namespace
/// (release ≥ [`PER_USERNS_NPROC_KERNEL`]), resolved once per process.
/// An unreadable or unparseable release fails closed: the bound is
/// skipped and reported `denied-unenforced`.
#[cfg(target_os = "linux")]
fn kernel_bounds_nproc_per_userns() -> bool {
    static PER_USERNS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PER_USERNS.get_or_init(|| {
        std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .ok()
            .and_then(|release| parse_kernel_release(release.trim()))
            .is_some_and(|release| release >= PER_USERNS_NPROC_KERNEL)
    })
}

#[cfg(not(target_os = "linux"))]
fn prlimit_available() -> bool {
    false
}

/// The fixed private environment names of the Windows LPAC block
/// (pointing at the per-exchange staging paths). They are provided
/// unconditionally so the runtime can locate system directories
/// without inheriting the host environment.
pub(super) const WINDOWS_PRIVATE_ENV_NAMES: [&str; 8] = [
    "APPDATA",
    "LOCALAPPDATA",
    "SystemDrive",
    "SystemRoot",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "windir",
];

/// Whether a granted environment name collides (case-insensitively)
/// with the fixed private block of this platform and is therefore
/// dropped at spawn: the private value must never be overridden by a
/// host-sourced grant. The spawn path and the evidence consult this
/// one predicate, so a dropped name can never reach the child and is
/// always visible in `budget.envDropped` (issue #89 fix round 2, C-F5).
pub(super) fn env_grant_dropped(name: &str) -> bool {
    cfg!(windows)
        && WINDOWS_PRIVATE_ENV_NAMES
            .iter()
            .any(|fixed| fixed.eq_ignore_ascii_case(name))
}

pub(super) struct Sandbox {
    owned: tempfile::TempDir,
    pub project: PathBuf,
    runtime: PathBuf,
    write_roots: Vec<PathBuf>,
    /// The effective write scopes this sandbox was built for; the
    /// publication guard validates every published entry against them
    /// (issue #89: a staged file outside the scopes cannot publish).
    write_scopes: Vec<String>,
    /// The sandbox policy projected from the session budget. Read by
    /// the Windows job cap and the Linux task bound; the macOS seatbelt
    /// profile expresses the same denial through its static rule set, so
    /// the field is platform-dead there.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(super) policy: SandboxPolicy,
    /// The honest per-dimension enforcement record.
    pub(super) report: ConfinementReport,
}

fn refusal(detail: &'static str) -> TargetFailure {
    TargetFailure::TransportFailed { detail }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // On Windows the confined child's directory handles can outlive
        // its exit status by a few hundred ms; a single remove_dir_all
        // would then fail and leak the staging dir. Retry briefly —
        // the TempDir field still performs the final removal after this.
        #[cfg(windows)]
        {
            let path = self.owned.path().to_path_buf();
            for _ in 0..40 {
                if !path.exists() || std::fs::remove_dir_all(&path).is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
}

impl Sandbox {
    pub fn publish(
        &self,
        root: &Path,
        writes: &[wire::WriteEntry],
        before: &plan::Snapshot,
    ) -> Result<(), TargetFailure> {
        use std::io::Write;
        // Issue #89 publication guard: only paths inside the effective
        // write scopes this sandbox was built for can ever publish. A
        // staged file outside them refuses before any real-project I/O.
        for entry in writes {
            if !plan::covered_by(&entry.path, &self.write_scopes) {
                return Err(TargetFailure::SecurityRefusal {
                    check: "writes-outside-scopes",
                    detail: "publication-guard",
                });
            }
        }
        plan::validate_preconditions(writes, before)?;
        let staged = Fs::open(&self.project).map_err(|_| refusal("sandbox-view"))?;
        let real = Fs::open(root).map_err(|_| refusal("project-root"))?;
        // Retain before bytes for ordinary I/O rollback. Files are replaced
        // atomically, never modified in place (including hard-linked files).
        let mut documents = Vec::new();
        for entry in writes {
            let (dir, name) = entry.path.rsplit_once('/').unwrap_or(("", &entry.path));
            let previous = match real.read_file_opt(dir, name, super::version::MAX_FILE_BYTES) {
                Ok(value) => value,
                Err(crate::project_fs::FsErrorKind::NotFound) => None,
                Err(_) => return Err(refusal("publish-preflight")),
            };
            if previous.as_ref().map(|b| plan::sha256_hex(b)).as_ref() != before.get(&entry.path) {
                return Err(TargetFailure::plan_mismatch(None, "before-drift"));
            }
            let after = if entry.action == wire::WriteAction::Delete {
                None
            } else {
                Some(
                    staged
                        .read_file_opt(dir, name, super::version::MAX_FILE_BYTES)
                        .map_err(|_| refusal("publish-preflight"))?
                        .ok_or_else(|| refusal("publish-preflight"))?,
                )
            };
            documents.push((entry, previous, after));
        }
        let mut applied: Vec<usize> = Vec::new();
        let mut directories = Vec::new();
        for (index, (entry, _, bytes)) in documents.iter().enumerate() {
            let result = (|| -> std::io::Result<()> {
                let destination = checked_destination(root, &entry.path, &mut directories)?;
                match bytes {
                    Some(bytes) => {
                        let mut temporary =
                            tempfile::NamedTempFile::new_in(destination.parent().expect("parent"))?;
                        temporary.write_all(bytes)?;
                        temporary.as_file().sync_all()?;
                        if entry.action == wire::WriteAction::Create {
                            temporary
                                .persist_noclobber(&destination)
                                .map_err(|e| e.error)?;
                        } else {
                            temporary.persist(&destination).map_err(|e| e.error)?;
                        }
                    }
                    None => std::fs::remove_file(destination)?,
                }
                Ok(())
            })();
            if result.is_err() {
                let mut partial = false;
                for &done in applied.iter().rev() {
                    let (entry, original, _) = &documents[done];
                    let restored = (|| -> std::io::Result<()> {
                        let destination = checked_destination(root, &entry.path, &mut directories)?;
                        if let Some(original) = original {
                            let mut file = tempfile::NamedTempFile::new_in(
                                destination.parent().expect("parent"),
                            )?;
                            file.write_all(original)?;
                            file.as_file().sync_all()?;
                            file.persist(destination).map_err(|e| e.error)?;
                        } else {
                            std::fs::remove_file(destination)?;
                        }
                        Ok(())
                    })();
                    partial |= restored.is_err();
                }
                for directory in directories.iter().rev() {
                    let _ = std::fs::remove_dir(directory);
                }
                return Err(TargetFailure::OperationFailed {
                    class: wire::ErrorClass::Infrastructure,
                    code: "core-publication".into(),
                    partial,
                });
            }
            applied.push(index);
        }
        Ok(())
    }
    pub fn new(
        root: &Path,
        reads: &[String],
        writes: &[String],
        apply: bool,
        policy: SandboxPolicy,
    ) -> Result<Self, TargetFailure> {
        let root = std::fs::canonicalize(root).map_err(|_| refusal("project-root"))?;
        #[cfg(windows)]
        windows::check_project_boundary(&root).map_err(|_| refusal("sandbox-project-access"))?;
        #[cfg(target_os = "linux")]
        if ["/usr", "/bin", "/lib", "/lib64", "/proc", "/dev"]
            .iter()
            .any(|p| root.starts_with(p))
        {
            return Err(refusal("sandbox-project-location"));
        }
        #[cfg(target_os = "macos")]
        if ["/System", "/usr/lib", "/dev"]
            .iter()
            .any(|p| root.starts_with(p))
        {
            return Err(refusal("sandbox-project-location"));
        }
        let owned = tempfile::Builder::new()
            .prefix("lekalo-target-sandbox-")
            .tempdir()
            .map_err(|_| refusal("sandbox-create"))?;
        // macOS checks executable and profile paths after resolving /var.
        // Use one canonical spelling for the whole private view, including
        // the copied program, script, request and writable output paths.
        #[cfg(target_os = "macos")]
        let owned_root =
            std::fs::canonicalize(owned.path()).map_err(|_| refusal("sandbox-create"))?;
        #[cfg(not(target_os = "macos"))]
        let owned_root = owned.path();
        let project = owned_root.join("project");
        let runtime = owned_root.join("runtime");
        std::fs::create_dir(&project)
            .and_then(|()| std::fs::create_dir(&runtime))
            .map_err(|_| refusal("sandbox-create"))?;
        let fs = Fs::open(&root).map_err(|_| refusal("sandbox-input"))?;
        let mut snapshot = plan::snapshot_scopes(&fs, reads).map_err(super::snapshot_rejection)?;
        for (path, digest) in
            plan::snapshot_scopes(&fs, writes).map_err(super::snapshot_rejection)?
        {
            snapshot.insert(path, digest);
        }
        let mut total = 0usize;
        for (path, digest) in snapshot {
            if !scopes::is_logical_path(&path) {
                return Err(refusal("sandbox-path"));
            }
            let dest = project.join(&path);
            if digest == "directory" {
                std::fs::create_dir_all(dest).map_err(|_| refusal("sandbox-copy"))?;
            } else {
                let (dir, name) = path.rsplit_once('/').unwrap_or(("", &path));
                let bytes = fs
                    .read_file_opt(dir, name, super::version::MAX_FILE_BYTES)
                    .map_err(|_| refusal("sandbox-input"))?
                    .ok_or_else(|| refusal("sandbox-input"))?;
                total = total.saturating_add(bytes.len());
                if total > 64 * 1024 * 1024 || plan::sha256_hex(&bytes) != digest {
                    return Err(refusal("sandbox-input"));
                }
                // Write authority reveals only the existing output's shape,
                // never its source bytes. Reading content requires an explicit
                // read scope even for a path that can be replaced or deleted.
                let bytes = if plan::covered_by(&path, reads) {
                    bytes
                } else {
                    Vec::new()
                };
                std::fs::create_dir_all(dest.parent().expect("inside project"))
                    .and_then(|()| std::fs::write(dest, bytes))
                    .map_err(|_| refusal("sandbox-copy"))?;
            }
        }
        let mut write_roots = Vec::new();
        if apply {
            for scope in writes {
                let relative = scope.strip_suffix("/**").unwrap_or(scope);
                let destination = project.join(relative);
                if scope.ends_with("/**") {
                    std::fs::create_dir_all(&destination).map_err(|_| refusal("sandbox-copy"))?;
                } else if cfg!(target_os = "linux") || !destination.exists() {
                    // Creating an exact file needs a writable parent. Linux
                    // also needs that directory for unlink: binding an existing
                    // file itself leaves its directory entry read-only.
                    // This grants only private-stage access; whole-tree
                    // verification still permits publication of only the plan.
                    let parent = destination.parent().expect("project child");
                    std::fs::create_dir_all(parent).map_err(|_| refusal("sandbox-copy"))?;
                    write_roots.push(parent.to_path_buf());
                    continue;
                }
                write_roots.push(destination);
            }
        }
        Ok(Self {
            owned,
            project,
            runtime,
            write_roots,
            write_scopes: writes.to_vec(),
            report: ConfinementReport::compute(policy),
            policy,
        })
    }

    /// The honest per-dimension enforcement record of this sandbox.
    pub(super) fn report(&self) -> ConfinementReport {
        self.report
    }

    fn command(
        &self,
        command: &transport::AdapterCommand,
    ) -> Result<transport::AdapterCommand, TargetFailure> {
        let program =
            resolve_program(&command.program).ok_or_else(|| refusal("sandbox-program"))?;
        let filename = program
            .file_name()
            .ok_or_else(|| refusal("sandbox-program"))?;
        let destination = self.runtime.join(filename);
        std::fs::copy(&program, &destination).map_err(|_| refusal("sandbox-runtime"))?;
        let mut args = command.args.clone();
        // A script explicitly supplied as the interpreter's first argument
        // is executable code, copied read-only. Other argv values confer no
        // filesystem grants. Packages with additional runtime assets need a
        // future explicit runtime bundle contract, not an ambient fallback.
        if let Some(first) = args.first_mut() {
            let script = Path::new(first);
            if script.is_file() {
                let script_name = script
                    .file_name()
                    .ok_or_else(|| refusal("sandbox-runtime"))?;
                let target = self.runtime.join(script_name);
                if target == destination {
                    return Err(refusal("sandbox-runtime"));
                }
                std::fs::copy(script, &target).map_err(|_| refusal("sandbox-runtime"))?;
                *first = target.to_string_lossy().into_owned();
            }
        }
        // Node otherwise walks every host ancestor of an absolute main
        // script with lstat. The private runtime contains no links; preserving
        // that spelling avoids granting access to C:\ or user-home ancestors.
        if matches!(filename.to_str(), Some("node" | "node.exe")) {
            args.splice(
                0..0,
                [
                    "--preserve-symlinks".into(),
                    "--preserve-symlinks-main".into(),
                ],
            );
        }
        Ok(transport::AdapterCommand {
            program: destination,
            args,
        })
    }

    pub fn run(
        &self,
        command: &transport::AdapterCommand,
        request: &[u8],
        limits: &transport::TransportLimits,
        file_transport: bool,
        cancel: Option<&AtomicBool>,
        env: &[(String, String)],
    ) -> Result<transport::TransportSuccess, TargetFailure> {
        let _custody = self.owned.path();
        if request.len() > limits.max_request_bytes {
            return Err(refusal("request-size"));
        }
        let mut command = self.command(command)?;
        if file_transport {
            let path = self.runtime.join("request.json");
            use std::io::Write;
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&path).map_err(|_| refusal("request-write"))?;
            file.write_all(request)
                .map_err(|_| refusal("request-write"))?;
            command.args.extend([
                "--lekalo-request-file".to_owned(),
                path.to_string_lossy().into_owned(),
            ]);
        }
        #[cfg(windows)]
        let result = windows::run(&command, request, limits, self, cancel, env);
        #[cfg(target_os = "linux")]
        let result = self.run_linux(&command, request, limits, cancel, env);
        #[cfg(target_os = "macos")]
        let result = self.run_macos(&command, request, limits, cancel, env);
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let result = Err(transport::TransportFailure::Spawn);
        result.map_err(super::transport_failure)
    }

    #[cfg(target_os = "linux")]
    fn run_linux(
        &self,
        command: &transport::AdapterCommand,
        request: &[u8],
        limits: &transport::TransportLimits,
        cancel: Option<&AtomicBool>,
        env: &[(String, String)],
    ) -> Result<transport::TransportSuccess, transport::TransportFailure> {
        // Issue #89 (fix round 1, devin F-1): no `--clearenv`/`--setenv`
        // — `--setenv` would put granted values (secrets included) into
        // the bwrap argv, readable via /proc/<pid>/cmdline for the whole
        // run. `run_private` env_clears the spawned process and injects
        // exactly the grant pairs via envp, so bwrap's own environment
        // is the grant set and the namespaced child inherits it
        // identically without any argv exposure. envp is the only
        // environment channel on this platform.
        let mut args: Vec<String> = [
            "--die-with-parent",
            "--unshare-all",
            "--new-session",
            "--proc",
            "/proc",
            "--dev",
            "/dev",
        ]
        .map(str::to_owned)
        .to_vec();
        for root in ["/usr", "/bin", "/lib", "/lib64"] {
            if Path::new(root).exists() {
                args.extend(["--ro-bind".into(), root.into(), root.into()]);
            }
        }
        for root in [&self.runtime, &self.project] {
            args.extend([
                "--ro-bind".into(),
                root.to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned(),
            ]);
        }
        for root in &self.write_roots {
            args.extend([
                "--bind".into(),
                root.to_string_lossy().into_owned(),
                root.to_string_lossy().into_owned(),
            ]);
        }
        args.extend([
            // The synthetic root and bind-mount ancestor directories are
            // otherwise writable. A nonrecursive remount preserves only
            // the separate output mounts explicitly granted above.
            "--remount-ro".into(),
            "/".into(),
            "--chdir".into(),
            self.project.to_string_lossy().into_owned(),
            "--".into(),
        ]);
        // Issue #89/#95: the prlimit wrapper bounds the payload INSIDE
        // the namespace, not around bwrap. Outside the namespace the
        // rlimit check compares against the whole-uid task total, so a
        // busy host trips the bound before bwrap's setup fork even
        // runs; inside the fresh userns the count covers only this
        // tree. The namespace lane additionally caps the address space
        // (`--as=`, reservation-inclusive, recorded honestly as a
        // degraded memory bound); the task bound applies when children
        // are denied. The report records whichever bounds actually
        // applied (or their honest absence).
        let lane = self.policy.bounded_resources
            && matches!(host_read_only_lane(), ReadOnlyLane::LinuxNamespace);
        let mut wrapper_limits = Vec::new();
        if lane {
            wrapper_limits.push(format!("--as={SANDBOX_ADDRESS_SPACE_LIMIT_BYTES}"));
        }
        if self.policy.children_denied {
            wrapper_limits.push(format!("--nproc={SANDBOX_TASK_BOUND}"));
        }
        if !wrapper_limits.is_empty() {
            if let Some(prlimit) = prlimit_path() {
                args.push(prlimit.to_string_lossy().into_owned());
                args.extend(wrapper_limits);
                args.push("--".into());
            }
        }
        args.push(command.program.to_string_lossy().into_owned());
        args.extend(command.args.clone());
        let wrapper = transport::AdapterCommand {
            program: "/usr/bin/bwrap".into(),
            args,
        };
        transport::run_private(&wrapper, request, limits, &self.project, cancel, env)
    }

    #[cfg(target_os = "macos")]
    fn run_macos(
        &self,
        command: &transport::AdapterCommand,
        request: &[u8],
        limits: &transport::TransportLimits,
        cancel: Option<&AtomicBool>,
        env: &[(String, String)],
    ) -> Result<transport::TransportSuccess, transport::TransportFailure> {
        // sandbox-exec inherits the spawning environment; the private
        // runner scrubs it (env -i semantics) and re-grants exactly the
        // budget pairs at spawn, keeping host values out of argv.
        transport::run_private_macos(
            &self.macos_command(command),
            request,
            limits,
            &self.project,
            cancel,
            env,
            self.policy.bounded_resources,
        )
    }

    #[cfg(target_os = "macos")]
    fn macos_command(&self, command: &transport::AdapterCommand) -> transport::AdapterCommand {
        let quoted = |p: &Path| serde_json::to_string(&p.to_string_lossy()).expect("string");
        // dyld opens the filesystem root while resolving its runtime paths.
        // This literal permits that directory alone, never its descendants.
        let mut profile = format!("(version 1)(deny default)(allow process-exec)(allow sysctl-read)(allow mach-lookup (global-name \"com.apple.system.logger\"))(allow file-read* (literal \"/\") (subpath \"/System\") (subpath \"/usr/lib\") (subpath \"/dev\") (subpath {}) (subpath {}))", quoted(&self.runtime), quoted(&self.project));
        if self.policy.bounded_resources && self.policy.children_denied {
            profile.push_str("(deny process-fork)");
        }
        for root in &self.write_roots {
            profile.push_str(&format!("(allow file-write* (subpath {}))", quoted(root)));
        }
        let mut args = vec![
            "-p".into(),
            profile,
            command.program.to_string_lossy().into_owned(),
        ];
        args.extend(command.args.clone());
        transport::AdapterCommand {
            program: "/usr/bin/sandbox-exec".into(),
            args,
        }
    }
}

fn checked_destination(
    root: &Path,
    logical: &str,
    created: &mut Vec<PathBuf>,
) -> std::io::Result<PathBuf> {
    let mut current = root.to_path_buf();
    let segments: Vec<_> = logical.split('/').collect();
    for (index, segment) in segments.iter().enumerate() {
        current.push(segment);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                let link = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let link = {
                    use std::os::windows::fs::MetadataExt;
                    link || metadata.file_attributes() & 0x400 != 0
                };
                if link
                    || (index + 1 < segments.len() && !metadata.is_dir())
                    || (index + 1 == segments.len() && !metadata.is_file())
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "project-entry",
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if index + 1 < segments.len() {
                    std::fs::create_dir(&current)?;
                    created.push(current.clone());
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

fn resolve_program(program: &Path) -> Option<PathBuf> {
    if program.is_absolute() || program.components().count() > 1 {
        return program.is_file().then(|| program.to_path_buf());
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH")?) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // The lane model is hermetic: every platform combination resolves
    // identically on any development host, so the Linux/macOS watch
    // target admission behavior is provable without a hosted lane
    // (issue #95). Real bwrap/prlimit execution stays on the unix
    // qualification test below and the hosted CI matrix.
    #[test]
    fn read_only_lanes_resolve_from_platform_facts() {
        assert_eq!(
            read_only_lane(true, false, false, false, false),
            ReadOnlyLane::WindowsJob
        );
        // Linux needs both bubblewrap and the prlimit wrapper: either
        // missing is an honest refusal, never an unbounded run.
        assert_eq!(
            read_only_lane(false, true, false, true, true),
            ReadOnlyLane::LinuxNamespace
        );
        assert_eq!(
            read_only_lane(false, true, false, true, false),
            ReadOnlyLane::Unavailable
        );
        assert_eq!(
            read_only_lane(false, true, false, false, true),
            ReadOnlyLane::Unavailable
        );
        // macOS supervision is degraded sampling, not a kernel ceiling.
        assert_eq!(
            read_only_lane(false, false, true, true, true),
            ReadOnlyLane::MacOsSupervised
        );
        assert_eq!(
            read_only_lane(false, false, false, true, true),
            ReadOnlyLane::Unavailable
        );
    }

    #[test]
    fn lane_bounds_admit_only_both_dimensions() {
        // A watch target run requires a memory AND a process bound
        // (`TargetClient::bounded_read_only_available`): the job lane
        // and supervised macOS qualify; unavailable never.
        for (lane, memory, processes) in [
            (
                ReadOnlyLane::WindowsJob,
                Some(SANDBOX_MEMORY_LIMIT_BYTES),
                Some(1),
            ),
            (
                ReadOnlyLane::LinuxNamespace,
                Some(SANDBOX_ADDRESS_SPACE_LIMIT_BYTES),
                Some(SANDBOX_TASK_BOUND),
            ),
            (
                ReadOnlyLane::MacOsSupervised,
                Some(SANDBOX_MEMORY_LIMIT_BYTES),
                Some(1),
            ),
            (ReadOnlyLane::Unavailable, None, None),
        ] {
            assert_eq!(lane_memory_bound(lane), memory, "{lane:?}");
            assert_eq!(lane_process_bound(lane, true), processes, "{lane:?}");
            assert_eq!(
                lane_process_bound(lane, false),
                None,
                "{lane:?} declared children"
            );
        }
        // The Linux address-space bound is a weaker semantics than the
        // Windows commit-charge cap: it must stay strictly larger so a
        // degraded report can never claim job-equivalence by accident.
        const _: () = assert!(SANDBOX_ADDRESS_SPACE_LIMIT_BYTES > SANDBOX_MEMORY_LIMIT_BYTES);
    }

    #[test]
    fn linux_lane_reports_degraded_resources_and_qualifies_for_watch() {
        // The namespace lane computes a Degraded (never Enforced)
        // resources dimension with both bounds present.
        #[cfg(target_os = "linux")]
        if matches!(host_read_only_lane(), ReadOnlyLane::LinuxNamespace) {
            let report =
                ConfinementReport::compute(SandboxPolicy::strict().with_bounded_resources(true));
            assert_eq!(report.resources, Enforcement::Degraded);
            let legacy = ConfinementReport::compute(SandboxPolicy::strict());
            assert_eq!(legacy.resources, Enforcement::Unenforced);
            assert_eq!(legacy.memory_limit(), None);
            assert!(report.memory_limit().is_some() && report.process_limit().is_some());
            return;
        }
        // On any host without a live namespace lane, the model still
        // proves the report a Degraded lane would publish.
        assert!(lane_memory_bound(ReadOnlyLane::LinuxNamespace).is_some());
    }

    // Qualification-only evidence from fixed code and owned synthetic data.
    // Product diagnostics must continue to discard raw backend/provider stderr.
    #[cfg(unix)]
    #[test]
    fn unix_confined_runtime_qualification() {
        let root = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::new(root.path(), &[], &[], false, SandboxPolicy::strict()).unwrap();
        let command = transport::AdapterCommand {
            program: "node".into(),
            args: vec![
                "-e".into(),
                "process.stdout.write('lekalo-confined-node-ok')".into(),
            ],
        };
        let limits = transport::TransportLimits {
            timeout_ms: 5_000,
            max_output_bytes: 1_024,
            max_stderr_bytes: 8_192,
            ..Default::default()
        };
        let result = sandbox
            .run(&command, b"", &limits, false, None, &[])
            .unwrap();
        assert_eq!(
            result.exit_code,
            0,
            "private synthetic launch evidence: stderr={:?}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"lekalo-confined-node-ok");

        // Loading a copied module and reading the request exercise runtime
        // paths that the inline startup control above does not touch.
        let sandbox = Sandbox::new(root.path(), &[], &[], false, SandboxPolicy::strict()).unwrap();
        let command = transport::AdapterCommand {
            program: "node".into(),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
                .to_string_lossy()
                .into_owned()],
        };
        let request = include_bytes!(
            "../../../../tests/fixtures/target-protocol/valid/describe-request.json"
        );
        let limits = transport::TransportLimits {
            max_output_bytes: 8_192,
            ..limits
        };
        let result = sandbox
            .run(&command, request, &limits, false, None, &[])
            .unwrap();
        assert_eq!(
            result.exit_code,
            0,
            "private synthetic adapter evidence: stderr={:?}",
            String::from_utf8_lossy(&result.stderr)
        );
        let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(response["operation"], "describe");

        // Denied absolute writes must fail inside the namespace as well as
        // preserve the real project. Empty mount ancestors are not outputs.
        std::fs::create_dir_all(root.path().join(".lekalo/ir")).unwrap();
        std::fs::write(root.path().join(".lekalo/ir/input.json"), b"owned input").unwrap();
        let sandbox = Sandbox::new(root.path(), &[], &[], false, SandboxPolicy::strict()).unwrap();
        let command = transport::AdapterCommand {
            program: "node".into(),
            args: vec![
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tests/fixtures/target-protocol/boundary-adapter.mjs")
                    .to_string_lossy()
                    .into_owned(),
                "--mode".into(),
                "describe-read".into(),
                "--host-root".into(),
                root.path().to_string_lossy().into_owned(),
            ],
        };
        let result = sandbox
            .run(&command, request, &limits, false, None, &[])
            .unwrap();
        assert_eq!(
            result.exit_code,
            0,
            "private synthetic boundary evidence: stderr={:?}",
            String::from_utf8_lossy(&result.stderr)
        );
        let response: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(response["operation"], "describe");
        assert_eq!(
            std::fs::read(root.path().join(".lekalo/ir/input.json")).unwrap(),
            b"owned input"
        );
        assert!(!root.path().join("other").exists());
    }

    // The kernel-release parser runs the per-userns gate; its floor and
    // spellings are pinned here (linux-only: the gate reads /proc).
    #[cfg(target_os = "linux")]
    #[test]
    fn kernel_release_parsing_drives_the_per_userns_floor() {
        assert_eq!(parse_kernel_release("6.5.0-18-generic"), Some((6, 5)));
        assert_eq!(parse_kernel_release("5.14.0-1022-azure"), Some((5, 14)));
        assert_eq!(parse_kernel_release("5.15"), Some((5, 15)));
        assert_eq!(parse_kernel_release("4.19.0"), Some((4, 19)));
        assert_eq!(parse_kernel_release(""), None);
        assert_eq!(parse_kernel_release("x.y"), None);
        // The floor itself passes; anything below it does not.
        assert!((5, 14) >= PER_USERNS_NPROC_KERNEL);
        assert!((5, 13) < PER_USERNS_NPROC_KERNEL);
        assert!((6, 0) >= PER_USERNS_NPROC_KERNEL);
        assert!((4, 20) < PER_USERNS_NPROC_KERNEL);
    }

    #[test]
    fn env_grants_colliding_with_the_private_block_are_dropped() {
        // The matching itself is case-insensitive over the fixed names.
        assert!(super::WINDOWS_PRIVATE_ENV_NAMES
            .iter()
            .any(|fixed| fixed.eq_ignore_ascii_case("temp")));
        assert!(super::WINDOWS_PRIVATE_ENV_NAMES
            .iter()
            .any(|fixed| fixed.eq_ignore_ascii_case("WINDIR")));
        assert!(!super::WINDOWS_PRIVATE_ENV_NAMES
            .iter()
            .any(|fixed| fixed.eq_ignore_ascii_case("PATH")));
        // On Windows the grant is dropped; elsewhere there is no
        // private block and nothing is dropped.
        assert_eq!(env_grant_dropped("TEMP"), cfg!(windows));
        assert!(!env_grant_dropped("LEKALO_GRANTED_VAR"));
    }

    #[test]
    fn isolated_node_handshake_has_a_working_positive_control() {
        let root = tempfile::tempdir().unwrap();
        let mut client = super::super::TargetClient::default();
        let command = transport::AdapterCommand {
            program: "node".into(),
            args: vec![Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/target-protocol/fake-adapter.mjs")
                .to_string_lossy()
                .into_owned()],
        };
        let result = client.describe(&command, root.path());
        assert!(result.is_ok(), "{result:?}");
    }

    // Issue #89 (plan S5): read-only modes never receive write authority
    // — `apply` false builds no write roots at all.
    #[test]
    fn read_only_sandboxes_have_no_write_roots() {
        let root = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::new(
            root.path(),
            &["a/**".into()],
            &["b/**".into()],
            false,
            SandboxPolicy::strict(),
        )
        .unwrap();
        assert!(sandbox.write_roots.is_empty(), "no write roots");
        let sandbox = Sandbox::new(
            root.path(),
            &["a/**".into()],
            &["b/**".into()],
            true,
            SandboxPolicy::strict(),
        )
        .unwrap();
        assert!(!sandbox.write_roots.is_empty(), "apply builds write roots");
        assert_eq!(sandbox.write_scopes, ["b/**"], "effective scopes stored");
    }

    // Issue #89 (plan S5): a staged file outside the effective write
    // scopes cannot publish — the publication guard refuses before any
    // real-project I/O. The guard logic is platform-independent, so the
    // test runs everywhere (issue #89 fix round 2, C-F9).
    #[test]
    fn a_staged_file_outside_the_write_scopes_cannot_publish() {
        let root = tempfile::tempdir().unwrap();
        let sandbox = Sandbox::new(
            root.path(),
            &[".lekalo/ir/**".into()],
            &["out/**".into()],
            true,
            SandboxPolicy::strict(),
        )
        .unwrap();
        // Simulate an impossible-in-confinement out-of-scope staged
        // write (an adapter breaching its sandbox view).
        let staged = sandbox.project.join("elsewhere");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::write(staged.join("escape.txt"), b"breach").unwrap();
        let entries = [wire::WriteEntry {
            path: "elsewhere/escape.txt".into(),
            action: wire::WriteAction::Create,
            sha256: Some(format!("sha256:{}", plan::sha256_hex(b"breach"))),
        }];
        let before = plan::Snapshot::new();
        let error = sandbox
            .publish(root.path(), &entries, &before)
            .expect_err("publication guard");
        assert!(
            matches!(error, TargetFailure::SecurityRefusal { .. }),
            "security refusal, got {error:?}"
        );
        assert!(
            !root.path().join("elsewhere").exists(),
            "the real project is untouched"
        );
    }
}

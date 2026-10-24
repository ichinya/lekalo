//! Production native execution uses an opaque, OS-qualified sandbox.
//! Callers cannot qualify a run by supplying capability strings.
use super::transport::{AdapterCommand, TransportLimits, TransportSuccess};
use super::TargetFailure;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

pub(crate) struct NativeSandbox {
    #[cfg(windows)]
    sandbox: super::confinement::Sandbox,
    qualified: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeSandboxProof {
    pub network: &'static str,
    pub processes: &'static str,
    pub backend: &'static str,
    pub memory_bytes: u64,
    pub process_limit: u64,
    pub digest: String,
}

fn unavailable() -> TargetFailure {
    TargetFailure::CapabilityUnsupported {
        detail: "native-execution-platform",
    }
}

impl NativeSandbox {
    pub(crate) fn prepare(pinned_private_root: &Path) -> Result<Self, TargetFailure> {
        #[cfg(windows)]
        {
            Ok(Self {
                sandbox: super::confinement::Sandbox::native_stage(pinned_private_root)?,
                qualified: false,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = pinned_private_root;
            Err(unavailable())
        }
    }

    /// A fixed runtime probe, performed after custody/approval preflight.
    /// It never loads a project script or package manager.
    pub(crate) fn qualify(
        &mut self,
        program: &Path,
        digest: &str,
        version: Option<&str>,
    ) -> Result<NativeSandboxProof, TargetFailure> {
        #[cfg(windows)]
        {
            use super::confinement::Enforcement;
            let report = self.sandbox.report();
            if report.network != Enforcement::Enforced
                || report.children != Enforcement::Enforced
                || report.resources != Enforcement::Enforced
            {
                return Err(unavailable());
            }
            let command = AdapterCommand {
                program: program.to_path_buf(),
                args: vec![
                    "-e".into(),
                    "process.stdout.write('native-ready:'+process.versions.node+':'+process.arch)"
                        .into(),
                ],
            };
            let result = self.sandbox.run_native_at(
                &command,
                ".",
                digest,
                &TransportLimits {
                    timeout_ms: 10_000,
                    max_output_bytes: 1024,
                    max_stderr_bytes: 1024,
                    max_request_bytes: 1,
                },
                None,
                &[],
            )?;
            let output = std::str::from_utf8(&result.stdout).map_err(|_| unavailable())?;
            let actual = output
                .strip_prefix("native-ready:")
                .ok_or_else(unavailable)?;
            let (actual, architecture) = actual.rsplit_once(':').ok_or_else(unavailable)?;
            if result.exit_code != 0
                || architecture != "x64"
                || version.is_some_and(|expected| expected.trim_start_matches('v') != actual)
            {
                return Err(unavailable());
            }
            self.qualified = true;
            let memory_bytes = report.memory_limit().ok_or_else(unavailable)?;
            let process_limit = report.process_limit().ok_or_else(unavailable)?;
            let evidence = serde_json::json!({"backend":"windows-lpac-job", "memoryBytes":memory_bytes, "processLimit":process_limit, "network":"enforced", "processContainment":"enforced"});
            let digest = format!(
                "sha256:{}",
                crate::digest::sha256_hex(
                    &serde_json::to_vec(&evidence).map_err(|_| unavailable())?
                )
            );
            Ok(NativeSandboxProof {
                network: "enforced",
                processes: "enforced",
                backend: "windows-lpac-job",
                memory_bytes,
                process_limit,
                digest,
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (program, digest, version);
            Err(unavailable())
        }
    }

    pub(crate) fn project_root(&self) -> Result<PathBuf, TargetFailure> {
        if !self.qualified {
            return Err(unavailable());
        }
        #[cfg(windows)]
        {
            Ok(self.sandbox.project_root().to_path_buf())
        }
        #[cfg(not(windows))]
        {
            Err(unavailable())
        }
    }

    pub(crate) fn run(
        &self,
        command: &AdapterCommand,
        cwd: &str,
        digest: &str,
        limits: &TransportLimits,
        cancel: Option<&AtomicBool>,
        env: &[(String, String)],
    ) -> Result<TransportSuccess, TargetFailure> {
        if !self.qualified {
            return Err(unavailable());
        }
        #[cfg(windows)]
        {
            let Some(first) = command.args.first() else {
                return Err(unavailable());
            };
            let script = std::fs::canonicalize(first).map_err(|_| unavailable())?;
            let project =
                std::fs::canonicalize(self.sandbox.project_root()).map_err(|_| unavailable())?;
            if !script.starts_with(project) {
                return Err(unavailable());
            }
            self.sandbox
                .run_native_at(command, cwd, digest, limits, cancel, env)
        }
        #[cfg(not(windows))]
        {
            let _ = (command, cwd, digest, limits, cancel, env);
            Err(unavailable())
        }
    }

    pub(crate) fn close(self) -> Result<(), TargetFailure> {
        #[cfg(windows)]
        {
            self.sandbox.close_native()
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }
}

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
    assets: bool,
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
    /// Use the ordinary drive spelling for Node/Vite while physical checks
    /// continue using canonical paths. A verbatim cwd leaks '?' into tool roots.
    pub(crate) fn runtime_path(path: &Path) -> PathBuf {
        #[cfg(windows)]
        {
            use std::path::{Component, Prefix};
            let mut parts = path.components();
            if let Some(Component::Prefix(prefix)) = parts.next() {
                if let Prefix::VerbatimDisk(letter) = prefix.kind() {
                    let mut ordinary = PathBuf::from(format!("{}:", char::from(letter)));
                    ordinary.extend(parts);
                    return ordinary;
                }
            }
        }
        path.to_path_buf()
    }

    pub(crate) fn prepare_assets(
        pinned_private_root: &Path,
        assets: Option<&Path>,
    ) -> Result<Self, TargetFailure> {
        #[cfg(windows)]
        {
            Ok(Self {
                sandbox: super::confinement::Sandbox::native_stage(pinned_private_root, assets)?,
                qualified: false,
                assets: assets.is_some(),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = (pinned_private_root, assets);
            Err(unavailable())
        }
    }

    pub(crate) fn tool_script(&self, entry: &str, digest: &str) -> Result<PathBuf, TargetFailure> {
        if !self.qualified
            || !self.assets
            || entry
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == "..")
        {
            return Err(unavailable());
        }
        let project = self.project_root()?;
        let assets = project
            .parent()
            .ok_or_else(unavailable)?
            .join("node_modules");
        let (dir, name) = entry.rsplit_once('/').unwrap_or(("", entry));
        let bytes = crate::project_fs::Fs::open(&assets)
            .map_err(|_| unavailable())?
            .read_file_opt(dir, name, 32 * 1024 * 1024)
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        if format!("sha256:{}", crate::digest::sha256_hex(&bytes)) != digest {
            return Err(unavailable());
        }
        Ok(assets.join(entry))
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
            let asset_root = project
                .parent()
                .ok_or_else(unavailable)?
                .join("node_modules");
            let admitted_asset = self.assets
                && std::fs::canonicalize(asset_root).is_ok_and(|root| script.starts_with(root));
            if !script.starts_with(project) && !admitted_asset {
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

#[cfg(all(test, windows))]
mod path_tests {
    use super::*;
    #[test]
    fn node_paths_use_drive_spelling_without_changing_physical_identity() {
        use std::path::{Component, Prefix};
        let root = tempfile::tempdir().unwrap();
        let canonical = std::fs::canonicalize(root.path()).unwrap();
        let ordinary = NativeSandbox::runtime_path(&canonical);
        assert!(
            matches!(ordinary.components().next(),Some(Component::Prefix(p)) if matches!(p.kind(),Prefix::Disk(_)))
        );
        assert_eq!(std::fs::canonicalize(ordinary).unwrap(), canonical);
    }
}

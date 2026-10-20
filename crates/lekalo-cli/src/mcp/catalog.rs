//! The trusted server catalog (issue #33): admitted projects, named
//! module groups, approved trace manifest inputs, changed-mode Git
//! admission, and limit reductions.
//!
//! The catalog is a trusted launcher operand: `--catalog-config FILE` is
//! supplied by the local launcher, never by a protocol request. Physical
//! roots resolve once at startup and never enter a discovery or result
//! payload. An explicit config is optional for single-project startup.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use lekalo_core::mcp::policy::{self, Catalog, GroupGrant, Limits};
use lekalo_core::mcp::version::{
    CONFIG_IDENTITY, CONFIG_SCHEMA_VERSION, DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_PAGE_SIZE,
    DEFAULT_TIMEOUT_MS, MAX_SEMANTIC_ROOTS,
};
use lekalo_core::DomainResult;

/// One admitted project: the opaque id, the resolved physical root for
/// the no-follow read seam, and the loader-facing selector (the trusted
/// operand spelling, which the core selection policy keeps relative to
/// the server's working directory — MCP never widens core policy).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectEntry {
    pub id: String,
    pub root: PathBuf,
    pub selector: String,
}

/// The resolved server catalog.
#[derive(Clone, Debug)]
pub struct ServerCatalog {
    pub core: Catalog,
    pub projects: BTreeMap<String, ProjectEntry>,
    pub limits: Limits,
}

impl ServerCatalog {
    /// The single-project startup catalog: one admitted project under the
    /// fixed opaque id `project`, no groups, no manifest grants, and no
    /// changed-mode Git admission.
    pub fn single(selector: &str, root: &Path) -> Self {
        let core = Catalog::empty()
            .with_project("project")
            .expect("literal id");
        let mut projects = BTreeMap::new();
        projects.insert(
            "project".to_owned(),
            ProjectEntry {
                id: "project".to_owned(),
                root: root.to_path_buf(),
                selector: selector.to_owned(),
            },
        );
        Self {
            core,
            projects,
            limits: Limits::default(),
        }
    }
}

/// The typed trusted-config wire document.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigWire {
    #[serde(rename = "schemaVersion")]
    schema_version: String,
    identity: String,
    projects: Vec<ProjectWire>,
    #[serde(default)]
    groups: Vec<GroupWire>,
    #[serde(default)]
    manifests: Vec<ManifestWire>,
    #[serde(default)]
    git: Option<GitWire>,
    #[serde(default)]
    limits: Option<LimitsWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectWire {
    id: String,
    root: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupWire {
    id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    modules: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestWire {
    id: String,
    #[serde(rename = "projectId")]
    project_id: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GitWire {
    #[serde(rename = "changedMode")]
    changed_mode: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LimitsWire {
    #[serde(rename = "timeoutMs", default)]
    timeout_ms: Option<u64>,
    #[serde(rename = "maxResponseBytes", default)]
    max_response_bytes: Option<usize>,
    #[serde(rename = "pageSize", default)]
    page_size: Option<usize>,
}

/// Why one trusted catalog could not serve.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CatalogError {
    /// The exact config pin is unknown (fail closed, no downgrade).
    UnsupportedPin,
    /// The document is malformed or violates a bound.
    Malformed,
    /// A granted root does not resolve to an existing directory.
    Root,
}

impl CatalogError {
    /// The pre-server CLI diagnostic, before any transport starts.
    pub fn domain_result(&self) -> DomainResult {
        match self {
            Self::UnsupportedPin => DomainResult::unsupported_version(
                lekalo_core::mcp::diagnostic::contract_unsupported_set("server-config"),
            ),
            Self::Malformed => DomainResult::invalid(
                lekalo_core::mcp::diagnostic::input_invalid_set("config", "malformed"),
            ),
            Self::Root => DomainResult::invalid(lekalo_core::mcp::diagnostic::scope_denied_set(
                "project-root",
                "unresolved",
            )),
        }
    }
}

/// Load and validate the trusted catalog from its exact bytes.
pub fn from_bytes(bytes: &[u8]) -> Result<ServerCatalog, CatalogError> {
    let wire: ConfigWire = serde_json::from_slice(bytes).map_err(|_| CatalogError::Malformed)?;
    if wire.schema_version != CONFIG_SCHEMA_VERSION || wire.identity != CONFIG_IDENTITY {
        return Err(CatalogError::UnsupportedPin);
    }
    if wire.projects.is_empty() || wire.projects.len() > MAX_SEMANTIC_ROOTS {
        return Err(CatalogError::Malformed);
    }
    let mut core = Catalog::empty();
    let mut projects = BTreeMap::new();
    for project in &wire.projects {
        core = core
            .with_project(&project.id)
            .map_err(|_| CatalogError::Malformed)?;
        let root = PathBuf::from(&project.root);
        if !root.is_dir() {
            return Err(CatalogError::Root);
        }
        // The loader-facing selector: the project's spelling relative to
        // the server's working directory, or `.` when the root is the
        // working directory itself. A root outside the working directory
        // would widen the core selection policy, so it refuses at
        // startup instead of serving restricted tools.
        let selector = relative_selector(&root).ok_or(CatalogError::Root)?;
        if projects
            .insert(
                project.id.clone(),
                ProjectEntry {
                    id: project.id.clone(),
                    root,
                    selector,
                },
            )
            .is_some()
        {
            return Err(CatalogError::Malformed);
        }
    }
    for manifest in &wire.manifests {
        core = core
            .with_manifest(policy::ManifestGrant {
                id: manifest.id.clone(),
                project: manifest.project_id.clone(),
                path: manifest.path.clone(),
            })
            .map_err(|_| CatalogError::Malformed)?;
    }
    for group in &wire.groups {
        core = core
            .with_group(GroupGrant {
                id: group.id.clone(),
                project: group.project_id.clone(),
                modules: group.modules.iter().cloned().collect(),
            })
            .map_err(|_| CatalogError::Malformed)?;
    }
    let git_admitted = wire
        .git
        .as_ref()
        .map(|git| git.changed_mode)
        .unwrap_or(false);
    core = core.with_git_changed_mode(git_admitted);
    let mut limits = Limits::default();
    if let Some(configured) = &wire.limits {
        // Config reductions only: the wire grammar already caps each
        // member at the recorded bound.
        if let Some(timeout) = configured.timeout_ms {
            if timeout == 0 || timeout > DEFAULT_TIMEOUT_MS {
                return Err(CatalogError::Malformed);
            }
            limits.timeout_ms = timeout;
        }
        if let Some(response) = configured.max_response_bytes {
            if !(1024..=DEFAULT_MAX_RESPONSE_BYTES).contains(&response) {
                return Err(CatalogError::Malformed);
            }
            limits.max_response_bytes = response;
        }
        if let Some(page) = configured.page_size {
            if page == 0 || page > DEFAULT_PAGE_SIZE {
                return Err(CatalogError::Malformed);
            }
            limits.page_size = page;
        }
    }
    Ok(ServerCatalog {
        core,
        projects,
        limits,
    })
}

/// The loader-facing selector of one resolved root: the canonical root
/// spelled relative to the current directory with / separators, or None
/// when the root is not inside the working directory.
fn relative_selector(root: &Path) -> Option<String> {
    let canonical_root = strip_verbatim(&root.canonicalize().ok()?);
    let cwd = strip_verbatim(&std::env::current_dir().ok()?);
    let relative = canonical_root.strip_prefix(&cwd).ok()?;
    if relative.as_os_str().is_empty() {
        return Some(".".to_owned());
    }
    Some(relative.to_string_lossy().replace('\\', "/"))
}

#[cfg(windows)]
fn strip_verbatim(path: &Path) -> std::path::PathBuf {
    match path.to_string_lossy().strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => std::path::PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

#[cfg(not(windows))]
fn strip_verbatim(path: &Path) -> std::path::PathBuf {
    path.to_path_buf()
}

/// Load the trusted catalog from a launcher-supplied path operand.
pub fn load(path: &str) -> Result<ServerCatalog, DomainResult> {
    let bytes = std::fs::read(path).map_err(|_| {
        DomainResult::invalid(lekalo_core::mcp::diagnostic::input_invalid_set(
            "catalog-config",
            "unreadable",
        ))
    })?;
    from_bytes(&bytes).map_err(|error| error.domain_result())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> String {
        format!(
            r#"{{
  "schemaVersion": "{CONFIG_SCHEMA_VERSION}",
  "identity": "{CONFIG_IDENTITY}",
  "projects": [{{"id": "p", "root": "."}}],
  "groups": [{{"id": "g", "projectId": "p", "modules": ["m"]}}],
  "manifests": [{{"id": "m", "projectId": "p", "path": "traces/t.json"}}],
  "git": {{"changedMode": true}},
  "limits": {{"timeoutMs": 1000}}
}}"#
        )
    }

    #[test]
    fn trusted_config_loads_the_closed_grammar() {
        let catalog = from_bytes(config().as_bytes()).expect("catalog");
        assert!(catalog.core.admits_project("p"));
        assert!(catalog.core.admits_git_changed_mode());
        assert_eq!(catalog.limits.timeout_ms, 1000);
        assert_eq!(
            catalog.core.admits_manifest("p", "m"),
            Some("traces/t.json")
        );
    }

    #[test]
    fn unknown_pins_and_malformed_documents_fail_closed() {
        let swapped = config().replace(CONFIG_SCHEMA_VERSION, "lekalo/mcp-server-config/v9.9.9");
        assert!(matches!(
            from_bytes(swapped.as_bytes()),
            Err(CatalogError::UnsupportedPin)
        ));
        assert!(matches!(
            from_bytes(b"{}".as_slice()),
            Err(CatalogError::Malformed)
        ));
        let unknown = config().replace(
            "\"changedMode\": true",
            "\"changedMode\": true, \"write\": true",
        );
        assert!(matches!(
            from_bytes(unknown.as_bytes()),
            Err(CatalogError::Malformed)
        ));
        let missing_root =
            config().replace("\"root\": \".\"", "\"root\": \"./definitely-missing\"");
        assert!(matches!(
            from_bytes(missing_root.as_bytes()),
            Err(CatalogError::Root)
        ));
    }

    #[test]
    fn single_project_startup_admits_one_fixed_identity() {
        let catalog = ServerCatalog::single(".", Path::new("."));
        assert!(catalog.core.admits_project("project"));
        assert_eq!(catalog.projects.len(), 1);
        assert!(!catalog.core.admits_git_changed_mode());
        assert_eq!(catalog.core.admits_manifest("project", "x"), None);
    }

    #[test]
    fn limit_reductions_only_shrink() {
        let raised = config().replace("\"timeoutMs\": 1000", "\"timeoutMs\": 999999");
        assert!(matches!(
            from_bytes(raised.as_bytes()),
            Err(CatalogError::Malformed)
        ));
    }
}

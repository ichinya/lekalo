//! Pure MCP admission policy (issue #33): effective limits, identifier
//! grammar, path admission, and semantic scope decisions.
//!
//! Nothing here touches the filesystem, the network, or a process: the
//! module is the pure half of the read boundary that the CLI seam applies
//! at every enumeration and open boundary. Effective limits are the
//! minimum of the server configuration and the caller request; a caller
//! parameter may reduce a limit, never raise one.

use std::collections::{BTreeMap, BTreeSet};

use super::version::{
    DEFAULT_MAX_RESPONSE_BYTES, DEFAULT_PAGE_SIZE, DEFAULT_TIMEOUT_MS, MAX_CONCURRENT_CALLS,
    MAX_FRAME_BYTES, MAX_QUEUED_REQUESTS, MAX_RESPONSE_BYTES_CEILING, MAX_SEMANTIC_ROOTS,
    MAX_WATCHED_IDENTITIES, PAGE_SIZE_CEILING, TIMEOUT_MS_CEILING,
};

/// The admission bounds of one server session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Maximum incoming JSON-RPC frame size in bytes.
    pub frame_bytes: usize,
    /// Maximum complete encoded response size in bytes.
    pub max_response_bytes: usize,
    /// Per-request deadline in milliseconds.
    pub timeout_ms: u64,
    /// Admitted concurrent tool calls.
    pub concurrent_calls: usize,
    /// Admitted queued requests.
    pub queued_requests: usize,
    /// Resource/tool listing page size.
    pub page_size: usize,
    /// Watched/subscribed resource identities.
    pub watched_identities: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            frame_bytes: MAX_FRAME_BYTES,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            concurrent_calls: MAX_CONCURRENT_CALLS,
            queued_requests: MAX_QUEUED_REQUESTS,
            page_size: DEFAULT_PAGE_SIZE,
            watched_identities: MAX_WATCHED_IDENTITIES,
        }
    }
}

impl Limits {
    /// Clamp a caller-supplied response cap into the effective range.
    pub fn effective_max_response_bytes(requested: Option<usize>, configured: usize) -> usize {
        configured
            .min(MAX_RESPONSE_BYTES_CEILING)
            .min(requested.unwrap_or(usize::MAX).max(1))
    }

    /// Clamp a caller-supplied deadline into the effective range.
    pub fn effective_timeout_ms(requested: Option<u64>, configured: u64) -> u64 {
        configured
            .min(TIMEOUT_MS_CEILING)
            .min(requested.unwrap_or(u64::MAX).max(1))
    }

    /// Clamp a caller-supplied page size into the effective range.
    pub fn effective_page_size(requested: Option<usize>, configured: usize) -> usize {
        configured
            .min(PAGE_SIZE_CEILING)
            .min(requested.unwrap_or(usize::MAX).max(1))
    }
}

/// Why one identifier or path failed admission. The value itself is never
/// echoed: refusals carry closed reasons only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionReason {
    /// Empty or beyond the recorded byte bound.
    Malformed,
    /// A character outside the closed identifier/path grammar.
    InvalidCharacter,
    /// Absolute, drive, UNC, or URL-like spelling.
    Absolute,
    /// A traversal or encoded-traversal segment.
    Traversal,
    /// A dot segment (dotfiles and dot directories are never admitted).
    DotSegment,
    /// A Windows alternate data stream or NUL byte.
    StreamOrNul,
    /// A known secret/config location refused as an arbitrary input.
    SecretLocation,
}

/// Validate one opaque catalog identifier (`project`/`group`/`manifest`):
/// lowercase alphanumerics and dashes, 1..=63 bytes.
pub fn admit_id(id: &str) -> Result<(), AdmissionReason> {
    if id.is_empty() || id.len() > 63 {
        return Err(AdmissionReason::Malformed);
    }
    let mut characters = id.chars();
    let first = characters.next().expect("non-empty");
    if !first.is_ascii_lowercase() {
        return Err(AdmissionReason::InvalidCharacter);
    }
    for character in characters {
        if !(character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-') {
            return Err(AdmissionReason::InvalidCharacter);
        }
    }
    Ok(())
}

/// Validate one module identifier: the same grammar as catalog ids.
pub fn admit_module_id(module: &str) -> Result<(), AdmissionReason> {
    admit_id(module)
}

/// Lexical admission of one project-relative logical path before any open.
///
/// This is the pure half of read admission: traversal, encoded traversal,
/// drive/UNC/URL spellings, NUL/ADS, mixed separators, dotfiles, and known
/// secret/config locations are refused before the filesystem seam runs its
/// no-follow/link checks. Lexical containment alone is never treated as
/// sufficient; the CLI seam enforces the capability checks as well.
pub fn admit_relative_path(path: &str) -> Result<(), AdmissionReason> {
    if path.is_empty() || path.len() > 512 {
        return Err(AdmissionReason::Malformed);
    }
    if path.contains('\0') {
        return Err(AdmissionReason::StreamOrNul);
    }
    if path.contains('\\') || path.contains(':') {
        return Err(AdmissionReason::Absolute);
    }
    if path.starts_with('/') {
        return Err(AdmissionReason::Absolute);
    }
    for segment in path.split('/') {
        match segment {
            "" | "." | ".." => return Err(AdmissionReason::Traversal),
            other if other.starts_with('.') => return Err(AdmissionReason::DotSegment),
            _ => {}
        }
    }
    if contains_percent_or_scheme(path) {
        return Err(AdmissionReason::Traversal);
    }
    if let Some(last) = path.rsplit('/').next() {
        if is_secret_name(last) {
            return Err(AdmissionReason::SecretLocation);
        }
    }
    Ok(())
}

fn contains_percent_or_scheme(path: &str) -> bool {
    path.contains('%') || path.contains("://") || path.to_ascii_lowercase().contains("file:")
}

fn is_secret_name(name: &str) -> bool {
    const SECRET_NAMES: [&str; 8] = [
        ".env",
        "config",
        "credentials",
        "id_rsa",
        "id_ed25519",
        ".npmrc",
        ".netrc",
        ".git-credentials",
    ];
    SECRET_NAMES.contains(&name)
}

/// Whether one dot-prefixed path is a governed internal input the server
/// may read under explicit admission (never `.git`, never as resource
/// content): governed `.lekalo/**` evidence and lock metadata.
pub fn governed_internal_path(path: &str) -> bool {
    if let Some(rest) = path.strip_prefix(".lekalo/") {
        !rest.is_empty() && !rest.contains("..")
    } else {
        false
    }
}

/// One approved trace manifest input: an opaque catalog id bound to one
/// project and one project-relative logical path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestGrant {
    pub id: String,
    pub project: String,
    pub path: String,
}

/// One named server-local allow-list of modules within one approved
/// project: a semantic disclosure constraint, never a claim that
/// module filtering alone limits filesystem reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupGrant {
    pub id: String,
    pub project: String,
    pub modules: BTreeSet<String>,
}

/// The trusted catalog grants: admitted projects, per-project named
/// module groups, approved trace manifest inputs, and the explicit
/// changed-mode Git admission. Physical roots live in the CLI seam only;
/// they never enter this pure policy or any response payload.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Catalog {
    projects: BTreeSet<String>,
    groups: BTreeMap<String, GroupGrant>,
    manifests: BTreeMap<String, ManifestGrant>,
    git_changed_mode: bool,
}

impl Catalog {
    /// An empty catalog; admission fails closed until projects are added.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Register one admitted project; bounds-checked by the CLI seam.
    pub fn with_project(mut self, project: &str) -> Result<Self, AdmissionReason> {
        admit_id(project)?;
        if self.projects.len() >= MAX_SEMANTIC_ROOTS {
            return Err(AdmissionReason::Malformed);
        }
        self.projects.insert(project.to_owned());
        Ok(self)
    }

    /// Register one named group over an already admitted project.
    pub fn with_group(mut self, grant: GroupGrant) -> Result<Self, AdmissionReason> {
        admit_id(&grant.id)?;
        admit_id(&grant.project)?;
        if !self.projects.contains(&grant.project) {
            return Err(AdmissionReason::Malformed);
        }
        if grant.modules.is_empty() {
            return Err(AdmissionReason::Malformed);
        }
        for module in &grant.modules {
            admit_module_id(module)?;
        }
        self.groups.insert(grant.id.clone(), grant);
        Ok(self)
    }

    /// Register one approved trace manifest input.
    pub fn with_manifest(mut self, grant: ManifestGrant) -> Result<Self, AdmissionReason> {
        admit_id(&grant.id)?;
        admit_id(&grant.project)?;
        admit_relative_path(&grant.path)?;
        if !self.projects.contains(&grant.project) {
            return Err(AdmissionReason::Malformed);
        }
        self.manifests.insert(grant.id.clone(), grant);
        Ok(self)
    }

    /// Explicitly admit changed-mode Git access for the whole catalog.
    pub fn with_git_changed_mode(mut self, admitted: bool) -> Self {
        self.git_changed_mode = admitted;
        self
    }

    /// Whether the project id is admitted at all.
    pub fn admits_project(&self, project: &str) -> bool {
        self.projects.contains(project)
    }

    /// Whether the group id is admitted for exactly this project.
    pub fn admits_group(&self, project: &str, group: &str) -> ScopeDecision {
        match self.groups.get(group) {
            Some(grant) if grant.project == project => ScopeDecision::Admitted,
            Some(_) => ScopeDecision::Widening,
            None => ScopeDecision::Unregistered,
        }
    }

    /// Whether the manifest ref is admitted for exactly this project and
    /// returns its approved logical path.
    pub fn admits_manifest(&self, project: &str, manifest: &str) -> Option<&str> {
        let grant = self.manifests.get(manifest)?;
        (grant.project == project).then_some(grant.path.as_str())
    }

    /// Whether changed-mode Git access was explicitly admitted.
    pub fn admits_git_changed_mode(&self) -> bool {
        self.git_changed_mode
    }

    /// The sorted admitted projects.
    pub fn projects(&self) -> impl Iterator<Item = &str> {
        self.projects.iter().map(String::as_str)
    }

    /// The sorted group grants of one project.
    pub fn groups_of<'a>(&'a self, project: &'a str) -> impl Iterator<Item = &'a GroupGrant> + 'a {
        self.groups
            .values()
            .filter(move |grant| grant.project == project)
    }

    /// The named group, when it belongs to the project.
    pub fn group(&self, project: &str, group: &str) -> Option<&GroupGrant> {
        let grant = self.groups.get(group)?;
        (grant.project == project).then_some(grant)
    }
}

/// The semantic scope decision for one request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopeDecision {
    /// The request stays inside the admitted closure.
    Admitted,
    /// The request names an unregistered identity.
    Unregistered,
    /// The request attempts to widen beyond the group allow-list.
    Widening,
}

/// Decide whether one required module set fits the admitted closure.
///
/// A group request is refused before any read when its required module
/// closure leaves the allow-list: neither output filtering nor client
/// roots prove safe reads, and protected context facts are never dropped
/// while labeling the result complete.
pub fn modules_admitted(
    catalog: &Catalog,
    project: &str,
    group: Option<&str>,
    required_modules: &BTreeSet<String>,
) -> ScopeDecision {
    let Some(group) = group else {
        // Full project scope: every admitted project module is in scope.
        if catalog.admits_project(project) {
            return ScopeDecision::Admitted;
        }
        return ScopeDecision::Unregistered;
    };
    let Some(grant) = catalog.group(project, group) else {
        return catalog.admits_group(project, group);
    };
    if required_modules.is_subset(&grant.modules) {
        ScopeDecision::Admitted
    } else {
        ScopeDecision::Widening
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::empty()
            .with_project("planner")
            .expect("project")
            .with_project("other")
            .expect("project")
            .with_group(GroupGrant {
                id: "planner-review".to_owned(),
                project: "planner".to_owned(),
                modules: ["planner".to_owned(), "audit".to_owned()]
                    .into_iter()
                    .collect(),
            })
            .expect("group")
            .with_manifest(ManifestGrant {
                id: "baseline".to_owned(),
                project: "planner".to_owned(),
                path: "traces/full.trace.json".to_owned(),
            })
            .expect("manifest")
            .with_git_changed_mode(false)
    }

    #[test]
    fn ids_follow_the_closed_grammar() {
        assert!(admit_id("planner").is_ok());
        assert!(admit_id("a1-2").is_ok());
        assert_eq!(admit_id(""), Err(AdmissionReason::Malformed));
        assert_eq!(admit_id(&"a".repeat(64)), Err(AdmissionReason::Malformed));
        assert_eq!(admit_id("Planner"), Err(AdmissionReason::InvalidCharacter));
        assert_eq!(
            admit_id("planner_x"),
            Err(AdmissionReason::InvalidCharacter)
        );
        assert_eq!(admit_id("../etc"), Err(AdmissionReason::InvalidCharacter));
    }

    #[test]
    fn paths_are_admitted_lexically_before_any_open() {
        assert!(admit_relative_path("lekalo/project.yaml").is_ok());
        assert!(admit_relative_path("traces/full.trace.json").is_ok());
        assert_eq!(
            admit_relative_path("../outside"),
            Err(AdmissionReason::Traversal)
        );
        assert_eq!(
            admit_relative_path("a/../../b"),
            Err(AdmissionReason::Traversal)
        );
        assert_eq!(
            admit_relative_path("%2e%2e/secret"),
            Err(AdmissionReason::Traversal)
        );
        assert_eq!(
            admit_relative_path("file:///etc/passwd"),
            Err(AdmissionReason::Absolute)
        );
        assert_eq!(
            admit_relative_path("C:\\Windows\\system32"),
            Err(AdmissionReason::Absolute)
        );
        assert_eq!(
            admit_relative_path("//server/share"),
            Err(AdmissionReason::Absolute)
        );
        assert_eq!(
            admit_relative_path("lekalo/model:stream"),
            Err(AdmissionReason::Absolute)
        );
        assert_eq!(
            admit_relative_path(".env"),
            Err(AdmissionReason::DotSegment)
        );
        assert_eq!(
            admit_relative_path("dir/.git/config"),
            Err(AdmissionReason::DotSegment)
        );
        assert_eq!(
            admit_relative_path("nul\0.txt"),
            Err(AdmissionReason::StreamOrNul)
        );
        assert_eq!(
            admit_relative_path("keys/id_rsa"),
            Err(AdmissionReason::SecretLocation)
        );
    }

    #[test]
    fn only_governed_lekalo_paths_are_internal_inputs() {
        assert!(governed_internal_path(".lekalo/lock.yaml"));
        assert!(governed_internal_path(".lekalo/evidence/run.json"));
        assert!(!governed_internal_path(".lekalo"));
        assert!(!governed_internal_path(".lekalo/../escape"));
        assert!(!governed_internal_path(".git/config"));
        assert!(!governed_internal_path("lekalo/project.yaml"));
    }

    #[test]
    fn catalog_admission_is_closed_over_registered_identities() {
        let catalog = catalog();
        assert!(catalog.admits_project("planner"));
        assert!(!catalog.admits_project("unknown"));
        assert_eq!(
            catalog.admits_group("planner", "planner-review"),
            ScopeDecision::Admitted
        );
        assert_eq!(
            catalog.admits_group("other", "planner-review"),
            ScopeDecision::Widening
        );
        assert_eq!(
            catalog.admits_group("planner", "nope"),
            ScopeDecision::Unregistered
        );
        assert_eq!(
            catalog.admits_manifest("planner", "baseline"),
            Some("traces/full.trace.json")
        );
        assert_eq!(catalog.admits_manifest("other", "baseline"), None);
        assert!(!catalog.admits_git_changed_mode());
    }

    #[test]
    fn group_closure_is_checked_before_any_read() {
        let catalog = catalog();
        let inside: BTreeSet<String> = ["planner".to_owned()].into_iter().collect();
        let outside: BTreeSet<String> = ["notify".to_owned()].into_iter().collect();
        assert_eq!(
            modules_admitted(&catalog, "planner", Some("planner-review"), &inside),
            ScopeDecision::Admitted
        );
        assert_eq!(
            modules_admitted(&catalog, "planner", Some("planner-review"), &outside),
            ScopeDecision::Widening
        );
        assert_eq!(
            modules_admitted(&catalog, "planner", None, &outside),
            ScopeDecision::Admitted
        );
        assert_eq!(
            modules_admitted(&catalog, "unknown", None, &outside),
            ScopeDecision::Unregistered
        );
    }

    #[test]
    fn effective_limits_only_shrink() {
        assert_eq!(Limits::effective_timeout_ms(Some(5_000), 30_000), 5_000);
        assert_eq!(Limits::effective_timeout_ms(Some(500_000), 30_000), 30_000);
        assert_eq!(Limits::effective_timeout_ms(None, 30_000), 30_000);
        assert_eq!(
            Limits::effective_max_response_bytes(Some(2_097_152), 1_048_576),
            1_048_576
        );
        assert_eq!(Limits::effective_max_response_bytes(Some(0), 1_048_576), 1);
        assert_eq!(Limits::effective_page_size(Some(10), 100), 10);
        assert_eq!(Limits::effective_page_size(Some(10_000), 100), 100);
    }
}

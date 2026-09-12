//! The closed success wire of `lekalo init --adopt`.
//!
//! The receipt is the only place detection leaves the process: every
//! proposal carries its evidence path, a closed confidence vocabulary, and
//! the constant `observed` mode. Field order is normative; collections are
//! sorted; no entry ever carries an absolute filesystem path, a timestamp,
//! or raw tool output.

use serde::Serialize;

/// One evidence-backed detection entry (path plus a closed detail token).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Evidence {
    /// Logical project-relative POSIX path of the evidence.
    pub path: String,
    /// Closed detail token describing what was observed.
    pub detail: String,
}

/// One language or framework hint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Hint {
    /// The closed hint label (`typescript`, `react`, `laravel`, ...).
    pub label: String,
    /// `language` or `framework`.
    pub kind: &'static str,
    /// Logical evidence path.
    pub path: String,
    /// The declaring field when the hint came from a manifest.
    pub field: Option<String>,
    /// The closed confidence vocabulary.
    pub confidence: Confidence,
}

/// One monorepo/workspace root.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkspaceRoot {
    /// Logical project-relative POSIX path of the workspace manifest.
    pub path: String,
    /// The closed ecosystem token (`node`, `go`, `rust`, `gradle`).
    pub ecosystem: &'static str,
    /// The declared member glob/list entries, verbatim.
    pub members: Vec<String>,
    /// The closed confidence vocabulary.
    pub confidence: Confidence,
}

/// One native gate command proposal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct GateProposal {
    /// The closed gate name (`build`, `test`, `lint`, `check`).
    pub gate: &'static str,
    /// The proposed command, never executed by Lekalo.
    pub command: String,
    /// `workflow` (the project declares it) or `derived` (ecosystem
    /// convention over declared evidence).
    pub source: &'static str,
    /// Logical evidence path.
    pub evidence: String,
    /// The closed confidence vocabulary.
    pub confidence: Confidence,
}

/// One target-adapter availability report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Adapter {
    /// The target id whose adapter was probed.
    pub target: String,
    /// Whether a `lekalo-target-<id>` executable is on `PATH`.
    pub installed: bool,
    /// The closed evidence token (`PATH`).
    pub evidence: &'static str,
    /// The closed confidence vocabulary.
    pub confidence: Confidence,
}

/// Where the adopted project id came from.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectIdProvenance {
    /// The closed derivation source (`--project-id`,
    /// `package.json#name`, `composer.json#name`,
    /// `Cargo.toml#package.name`, `go.mod#module`, `directory-name`).
    pub source: String,
    /// The raw observed text when the id was derived.
    pub original: Option<String>,
    /// The closed confidence vocabulary.
    pub confidence: Confidence,
}

impl ProjectIdProvenance {
    /// The provenance of an explicit `--project-id`.
    pub fn explicit() -> Self {
        Self {
            source: "--project-id".to_owned(),
            original: None,
            confidence: Confidence::High,
        }
    }

    /// The provenance of a sanitized bootstrap directory name.
    pub fn directory_name(original: String) -> Self {
        Self {
            source: "directory-name".to_owned(),
            original: Some(original),
            confidence: Confidence::Medium,
        }
    }
}

/// One externally owned layout detected at the adopted root.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Layout {
    /// The closed layout kind (`openspec`, `ai-factory`, `hlv`).
    pub kind: &'static str,
    /// Logical project-relative POSIX path of the layout root.
    pub path: String,
    /// The closed confidence vocabulary.
    pub confidence: Confidence,
}

/// One observed module proposal: a module Lekalo noticed, never authored.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ObservedModule {
    /// The observed identity as found (a package name or directory name).
    pub name: String,
    /// Logical project-relative POSIX path of the module directory.
    pub path: String,
    /// The constant `observed` mode of adoption detection.
    pub mode: &'static str,
    /// Sorted evidence paths.
    pub evidence: Vec<String>,
    /// The closed confidence vocabulary.
    pub confidence: Confidence,
}

/// The closed confidence vocabulary of every detection entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Declared directly by a manifest field.
    High,
    /// Derived from file presence or ecosystem convention.
    Medium,
    /// Weak or ambiguous evidence.
    Low,
}

impl Confidence {
    /// The stable wire spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

/// The detection section of the receipt.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Detection {
    /// How the adopted root was chosen (`explicit`, `workspace-root`,
    /// `manifest`, or `invocation-directory`).
    pub basis: &'static str,
    /// Invocation-relative candidate spellings when ambiguity was resolved.
    pub candidates: Vec<String>,
    pub manifests: Vec<Evidence>,
    #[serde(rename = "packageManagers")]
    pub package_managers: Vec<Evidence>,
    pub languages: Vec<Hint>,
    #[serde(rename = "workspaceRoots")]
    pub workspace_roots: Vec<WorkspaceRoot>,
    #[serde(rename = "monorepoTools")]
    pub monorepo_tools: Vec<Evidence>,
    #[serde(rename = "sourceDirs")]
    pub source_dirs: Vec<String>,
    #[serde(rename = "testDirs")]
    pub test_dirs: Vec<String>,
    pub openapi: Vec<Evidence>,
    pub layouts: Vec<Layout>,
    pub gates: Vec<GateProposal>,
    pub adapters: Vec<Adapter>,
    pub modules: Vec<ObservedModule>,
}

/// One planned or executed skeleton write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WriteEntry {
    /// Logical project-relative POSIX path of the file.
    pub path: String,
    /// The closed action (`create`).
    pub action: &'static str,
    /// SHA-256 of the exact resulting bytes (lowercase hex).
    pub sha256: String,
    /// The closed disposition (`create`, `already-present`, `conflict`).
    pub disposition: &'static str,
}

/// The post-write gate outcome of an applied adoption.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Gate {
    /// Always `valid` on a receipt.
    pub status: &'static str,
    /// The built-in validation profile used (`default`).
    pub profile: &'static str,
    /// The loaded Model contract version.
    #[serde(rename = "modelVersion")]
    pub model_version: String,
}

/// The closed success wire object of `lekalo init --adopt`.
/// Field order is normative.
#[derive(Clone, Debug, Serialize)]
pub struct AdoptReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `init`.
    pub operation: &'static str,
    /// `dry-run` or `apply`.
    pub mode: &'static str,
    /// Whether this invocation wrote any bytes.
    pub changed: bool,
    /// The adopted project's canonical project id.
    #[serde(rename = "projectId")]
    pub project_id: String,
    /// Where that id came from.
    #[serde(rename = "projectIdSource")]
    pub project_id_source: ProjectIdProvenance,
    /// The explicitly selected target, when `--target` was passed.
    pub target: Option<String>,
    /// The explicitly selected adapter profile, when `--profile` was
    /// passed (requires `--target`; the #28 token grammar).
    #[serde(rename = "adapterProfile")]
    pub adapter_profile: Option<String>,
    /// The validation profile of the post-write gate (`default`).
    pub profile: &'static str,
    /// Planned writes already present with identical bytes.
    #[serde(rename = "alreadyPresent")]
    pub already_present: usize,
    /// Planned writes blocked by the no-overwrite policy.
    pub conflicts: usize,
    /// Writes this invocation created.
    pub created: usize,
    /// Every planned write in canonical path order.
    pub writes: Vec<WriteEntry>,
    /// The post-write gate outcome; `None` when nothing was written.
    pub gate: Option<Gate>,
    pub detection: Detection,
}

/// One template-version record: which in-code bootstrap template
/// produced an artifact, and which product version that template
/// ships as. The templates version with the product; the receipt is
/// the durable record of what generated the tree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TemplateRecord {
    /// The closed artifact family (`project`, `module`, `target`,
    /// `gitignore`, `editor-hints`).
    pub artifact: &'static str,
    /// The template identity (`lekalo/init/<artifact>`).
    pub template: &'static str,
    /// The product version the template ships as.
    pub version: &'static str,
}

impl TemplateRecord {
    /// The `lekalo/init/project` record.
    pub const fn project(version: &'static str) -> Self {
        Self {
            artifact: "project",
            template: "lekalo/init/project",
            version,
        }
    }

    /// The `lekalo/init/module` record.
    pub const fn module(version: &'static str) -> Self {
        Self {
            artifact: "module",
            template: "lekalo/init/module",
            version,
        }
    }

    /// The `lekalo/init/target` record.
    pub const fn target(version: &'static str) -> Self {
        Self {
            artifact: "target",
            template: "lekalo/init/target",
            version,
        }
    }

    /// The `lekalo/init/gitignore` record.
    pub const fn gitignore(version: &'static str) -> Self {
        Self {
            artifact: "gitignore",
            template: "lekalo/init/gitignore",
            version,
        }
    }

    /// The `lekalo/init/editor-hints` record.
    pub const fn editor_hints(version: &'static str) -> Self {
        Self {
            artifact: "editor-hints",
            template: "lekalo/init/editor-hints",
            version,
        }
    }
}

/// The closed success wire object of greenfield `lekalo init`.
/// Field order is normative.
#[derive(Clone, Debug, Serialize)]
pub struct BootstrapReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `init`.
    pub operation: &'static str,
    /// `dry-run` or `apply`.
    pub mode: &'static str,
    /// Whether this invocation wrote any bytes.
    pub changed: bool,
    /// The closed root basis (`explicit`, `invocation-directory`).
    #[serde(rename = "rootBasis")]
    pub root_basis: &'static str,
    /// The bootstrap project's canonical project id.
    #[serde(rename = "projectId")]
    pub project_id: String,
    /// Where that id came from.
    #[serde(rename = "projectIdSource")]
    pub project_id_source: ProjectIdProvenance,
    /// The semantic id of the first module.
    pub module: String,
    /// The canonical model frontend of the generated documents
    /// (`yaml` or `json`).
    pub frontend: &'static str,
    /// The explicitly selected target, when `--target` was passed.
    pub target: Option<String>,
    /// The explicitly selected adapter profile, when `--profile` was
    /// passed (requires `--target`; the #28 token grammar). Recorded
    /// only — never executed or resolved against an adapter.
    #[serde(rename = "adapterProfile")]
    pub adapter_profile: Option<String>,
    /// Whether the opt-in editor/schema hints were planned.
    #[serde(rename = "editorHints")]
    pub editor_hints: bool,
    /// Artifacts this invocation added (creates plus appends).
    pub added: usize,
    /// Planned artifacts already present with identical bytes.
    pub unchanged: usize,
    /// Planned artifacts blocked by the no-overwrite policy.
    pub conflicting: usize,
    /// Every planned artifact in canonical plan order.
    pub writes: Vec<WriteEntry>,
    /// The post-write gate outcome; `None` when nothing was written.
    pub gate: Option<Gate>,
    /// The template-version record of every planned artifact.
    pub templates: Vec<TemplateRecord>,
}

impl BootstrapReceipt {
    /// The stable one-line human summary.
    pub fn human_summary(&self) -> String {
        match self.mode {
            "dry-run" => format!(
                "init dry-run: {} planned write(s), {} unchanged, {} conflicting",
                self.added, self.unchanged, self.conflicting
            ),
            _ => {
                let gate = self
                    .gate
                    .as_ref()
                    .map(|gate| format!("gate {}", gate.status))
                    .unwrap_or_else(|| "no writes".to_owned());
                format!(
                    "init apply: {} added, {} unchanged, {} conflicting; {gate}",
                    self.added, self.unchanged, self.conflicting
                )
            }
        }
    }
}

/// The closed success wire object of `lekalo module new`.
/// Field order is normative.
#[derive(Clone, Debug, Serialize)]
pub struct ModuleReceipt {
    /// Always `valid`.
    pub status: &'static str,
    /// Always `module`.
    pub operation: &'static str,
    /// `dry-run` or `apply`.
    pub mode: &'static str,
    /// Whether this invocation wrote any bytes.
    pub changed: bool,
    /// The new module's semantic id.
    #[serde(rename = "moduleId")]
    pub module_id: String,
    /// The canonical model frontend of the generated document.
    pub frontend: &'static str,
    /// Documents this invocation added.
    pub added: usize,
    /// Planned documents already present with identical bytes.
    pub unchanged: usize,
    /// Planned documents blocked by the no-overwrite policy.
    pub conflicting: usize,
    /// Every planned write in canonical plan order.
    pub writes: Vec<WriteEntry>,
    /// The post-write gate outcome; `None` when nothing was written.
    pub gate: Option<Gate>,
    /// The template-version record of the planned artifact.
    pub templates: Vec<TemplateRecord>,
}

impl ModuleReceipt {
    /// The stable one-line human summary.
    pub fn human_summary(&self) -> String {
        match self.mode {
            "dry-run" => format!(
                "module new dry-run: {} planned write(s), {} unchanged, {} conflicting",
                self.added, self.unchanged, self.conflicting
            ),
            _ => {
                let gate = self
                    .gate
                    .as_ref()
                    .map(|gate| format!("gate {}", gate.status))
                    .unwrap_or_else(|| "no writes".to_owned());
                format!(
                    "module new apply: {} added, {} unchanged, {} conflicting; {gate}",
                    self.added, self.unchanged, self.conflicting
                )
            }
        }
    }
}

//! Backend bindings of the Scenario IR (issue #23).
//!
//! A binding is a typed reference, never execution: it names a closed
//! backend kind (`fake-reference` or `native`), a stable runner, its own
//! independent version pins (runner, optional protocol, optional
//! profile), a capability-set digest over the sorted capability list,
//! the stable native-test or fake-evaluator identity, and optional mode
//! and evidence digests. Native source tests stay source-native-owned —
//! the IR references them without copying paths or content; the fake
//! reference binding points at #107 capabilities and never embeds
//! evaluator code. Execution, process control, and evidence production
//! stay with #47/#56/#107/#31/#91.

use serde_json::Value as Json;

use crate::diagnostics::DiagnosticSet;
use crate::lockfile::types::{SemVer, Sha256Digest};

use super::diagnostic;
use super::id::NamespacedId;

/// The closed backend kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Backend {
    /// The fake reference backend (#107 owns its evaluator).
    FakeReference,
    /// A native test backend (source-native-owned tests).
    Native,
}

impl Backend {
    /// The wire tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::FakeReference => "fake-reference",
            Backend::Native => "native",
        }
    }

    /// The kind for one wire tag.
    pub(crate) fn from_wire(text: &str) -> Option<Backend> {
        match text {
            "fake-reference" => Some(Backend::FakeReference),
            "native" => Some(Backend::Native),
            _ => None,
        }
    }
}

/// The closed binding modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Mode {
    /// The native test was generated.
    Generated,
    /// The native test was scaffolded and completed by hand.
    Scaffolded,
    /// The binding is checked against an existing test.
    Checked,
}

impl Mode {
    /// The wire tag.
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::Generated => "generated",
            Mode::Scaffolded => "scaffolded",
            Mode::Checked => "checked",
        }
    }

    /// The kind for one wire tag.
    pub(crate) fn from_wire(text: &str) -> Option<Mode> {
        match text {
            "generated" => Some(Mode::Generated),
            "scaffolded" => Some(Mode::Scaffolded),
            "checked" => Some(Mode::Checked),
            _ => None,
        }
    }
}

/// One versioned protocol pin.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProtocolPin {
    /// The namespaced protocol identity.
    pub id: NamespacedId,
    /// The exact protocol version.
    pub version: SemVer,
}

/// One versioned profile pin.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ProfilePin {
    /// The namespaced profile identity.
    pub id: NamespacedId,
    /// The exact profile version.
    pub version: SemVer,
}

/// One typed backend binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    /// The closed backend kind.
    pub backend: Backend,
    /// The stable runner identity.
    pub runner: NamespacedId,
    /// The exact runner version.
    pub runner_version: SemVer,
    /// The optional protocol pin.
    pub protocol: Option<ProtocolPin>,
    /// The optional profile pin.
    pub profile: Option<ProfilePin>,
    /// The sorted capability set the backend must provide.
    pub capabilities: Vec<NamespacedId>,
    /// The digest over the sorted capability set.
    pub capability_digest: Sha256Digest,
    /// The stable native-test or fake-evaluator scenario identity.
    pub test: NamespacedId,
    /// The optional closed binding mode.
    pub mode: Option<Mode>,
    /// The optional expected evidence digest.
    pub evidence_digest: Option<Sha256Digest>,
}

impl Binding {
    /// Normalize one wire binding, or return the typed rejection set.
    pub(crate) fn from_json(json: &Json, role: &str) -> Result<Binding, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| diagnostic::input_invalid("binding-shape", Some(role)))?;
        let mut backend = None;
        let mut runner = None;
        let mut runner_version = None;
        let mut protocol = None;
        let mut profile = None;
        let mut capabilities = Vec::new();
        let mut capability_digest = None;
        let mut test = None;
        let mut mode = None;
        let mut evidence_digest = None;
        for (key, value) in object {
            match key.as_str() {
                "backend" => {
                    backend = Some(
                        Backend::from_wire(value.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("binding-backend", Some(role))
                        })?)
                        .ok_or_else(|| diagnostic::input_invalid("binding-backend", Some(role)))?,
                    );
                }
                "runner" => {
                    runner = Some(
                        NamespacedId::parse(value.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("binding-runner", Some(role))
                        })?)
                        .map_err(|_| diagnostic::input_invalid("binding-runner", Some(role)))?,
                    );
                }
                "runnerVersion" => {
                    runner_version = Some(
                        SemVer::parse(value.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("binding-runner-version", Some(role))
                        })?)
                        .map_err(|_| {
                            diagnostic::input_invalid("binding-runner-version", Some(role))
                        })?,
                    );
                }
                "protocol" => {
                    let pin = value
                        .as_object()
                        .ok_or_else(|| diagnostic::input_invalid("binding-protocol", Some(role)))?;
                    if pin
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<&str>>()
                        .as_slice()
                        != ["id", "version"]
                    {
                        return Err(diagnostic::input_invalid("binding-protocol", Some(role)));
                    }
                    protocol = Some(ProtocolPin {
                        id: NamespacedId::parse(pin.get("id").and_then(Json::as_str).ok_or_else(
                            || diagnostic::input_invalid("binding-protocol", Some(role)),
                        )?)
                        .map_err(|_| diagnostic::input_invalid("binding-protocol", Some(role)))?,
                        version: SemVer::parse(
                            pin.get("version").and_then(Json::as_str).ok_or_else(|| {
                                diagnostic::input_invalid("binding-protocol", Some(role))
                            })?,
                        )
                        .map_err(|_| diagnostic::input_invalid("binding-protocol", Some(role)))?,
                    });
                }
                "profile" => {
                    let pin = value
                        .as_object()
                        .ok_or_else(|| diagnostic::input_invalid("binding-profile", Some(role)))?;
                    if pin
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<&str>>()
                        .as_slice()
                        != ["id", "version"]
                    {
                        return Err(diagnostic::input_invalid("binding-profile", Some(role)));
                    }
                    profile = Some(ProfilePin {
                        id: NamespacedId::parse(pin.get("id").and_then(Json::as_str).ok_or_else(
                            || diagnostic::input_invalid("binding-profile", Some(role)),
                        )?)
                        .map_err(|_| diagnostic::input_invalid("binding-profile", Some(role)))?,
                        version: SemVer::parse(
                            pin.get("version").and_then(Json::as_str).ok_or_else(|| {
                                diagnostic::input_invalid("binding-profile", Some(role))
                            })?,
                        )
                        .map_err(|_| diagnostic::input_invalid("binding-profile", Some(role)))?,
                    });
                }
                "capabilities" => {
                    let items = value.as_array().ok_or_else(|| {
                        diagnostic::input_invalid("binding-capabilities", Some(role))
                    })?;
                    if items.len() > super::version::MAX_CAPABILITIES {
                        return Err(diagnostic::limit_set("capabilities"));
                    }
                    let mut parsed = Vec::with_capacity(items.len());
                    for item in items {
                        parsed.push(
                            NamespacedId::parse(item.as_str().ok_or_else(|| {
                                diagnostic::input_invalid("binding-capabilities", Some(role))
                            })?)
                            .map_err(|_| {
                                diagnostic::input_invalid("binding-capabilities", Some(role))
                            })?,
                        );
                    }
                    if super::precondition::has_duplicates(&parsed) {
                        return Err(diagnostic::input_invalid(
                            "duplicate-capability",
                            Some(role),
                        ));
                    }
                    parsed.sort();
                    capabilities = parsed;
                }
                "capabilityDigest" => {
                    capability_digest = Some(
                        Sha256Digest::parse(value.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("binding-capability-digest", Some(role))
                        })?)
                        .map_err(|_| {
                            diagnostic::input_invalid("binding-capability-digest", Some(role))
                        })?,
                    );
                }
                "test" => {
                    test = Some(
                        NamespacedId::parse(value.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("binding-test", Some(role))
                        })?)
                        .map_err(|_| diagnostic::input_invalid("binding-test", Some(role)))?,
                    );
                }
                "mode" => {
                    mode = Some(
                        Mode::from_wire(value.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("binding-mode", Some(role))
                        })?)
                        .ok_or_else(|| diagnostic::input_invalid("binding-mode", Some(role)))?,
                    );
                }
                "evidenceDigest" => {
                    evidence_digest = Some(
                        Sha256Digest::parse(value.as_str().ok_or_else(|| {
                            diagnostic::input_invalid("binding-evidence-digest", Some(role))
                        })?)
                        .map_err(|_| {
                            diagnostic::input_invalid("binding-evidence-digest", Some(role))
                        })?,
                    );
                }
                _ => return Err(diagnostic::input_invalid("binding-shape", Some(role))),
            }
        }
        let (
            Some(backend),
            Some(runner),
            Some(runner_version),
            Some(capability_digest),
            Some(test),
        ) = (backend, runner, runner_version, capability_digest, test)
        else {
            return Err(diagnostic::input_invalid("binding-shape", Some(role)));
        };
        Ok(Binding {
            backend,
            runner,
            runner_version,
            protocol,
            profile,
            capabilities,
            capability_digest,
            test,
            mode,
            evidence_digest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn digest(suffix: &str) -> String {
        format!("sha256:{suffix:0>64}")
    }

    fn binding() -> Json {
        json!({
            "backend": "fake-reference",
            "runner": "core.runners/fake-reference",
            "runnerVersion": "1.0.0",
            "capabilities": ["core.capabilities/entity-state"],
            "capabilityDigest": digest("aa"),
            "test": "core.fake/planner-switch-focus"
        })
    }

    #[test]
    fn bindings_parse_minimal_and_full_shapes() {
        let minimal = Binding::from_json(&binding(), "b").expect("minimal binding");
        assert_eq!(minimal.backend, Backend::FakeReference);
        assert!(minimal.protocol.is_none() && minimal.mode.is_none());
        let full_json = json!({
            "backend": "native",
            "runner": "adapters.typescript/vitest",
            "runnerVersion": "2.1.0",
            "protocol": {"id": "adapters.protocol/test", "version": "1.0.0"},
            "profile": {"id": "adapters.profile/node", "version": "24.0.0"},
            "capabilities": ["core.capabilities/entity-state", "core.capabilities/events"],
            "capabilityDigest": digest("bb"),
            "test": "adapters.tests/planner-switch-focus",
            "mode": "checked",
            "evidenceDigest": digest("cc")
        });
        let full = Binding::from_json(&full_json, "b").expect("full binding");
        assert_eq!(full.backend, Backend::Native);
        assert_eq!(full.mode, Some(Mode::Checked));
        assert_eq!(full.capabilities.len(), 2);
    }

    #[test]
    fn capability_lists_are_sorted_and_deduplicated() {
        let mut raw = binding();
        raw["capabilities"] = json!([
            "core/capabilities/events",
            "core/capabilities/entity-state",
            "core/capabilities/entity-state"
        ]);
        assert!(Binding::from_json(&raw, "b").is_err());
    }

    #[test]
    fn required_members_and_unknown_members_are_enforced() {
        assert!(Binding::from_json(&json!({"backend": "native"}), "b").is_err());
        let mut extra = binding();
        extra["evaluator"] = json!("inline code");
        assert!(Binding::from_json(&extra, "b").is_err());
    }
}

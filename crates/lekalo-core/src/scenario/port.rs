//! The project test-port contract (issue #47, plan S2).
//!
//! A scenario test never binds to production internals: it binds to one
//! project-declared **test port** declared at `lekalo/test-port.json` in
//! the project contract home. The declaration is pure data — a
//! project-relative module path plus the closed set of exports the
//! module promises — so the target adapter can map every scenario
//! feature onto the port surface, and every feature without a port
//! surface onto an explicit unsupported diagnostic, before anything is
//! emitted.
//!
//! This module is pure contract parsing over an already-decoded JSON
//! document (the same closed-shape custody every Scenario IR module
//! applies). File location, reading, and module loading stay with the
//! callers: the loader for hosted flows, the target adapter for the
//! confined wire, and the fixtures for conformance.

use serde_json::Value as Json;

use super::diagnostic;
use crate::diagnostics::DiagnosticSet;

/// The exact wire discriminator of the test-port contract.
pub const SCHEMA_VERSION: &str = "lekalo/test-port/v0.4.0";

/// The exact contract identity.
pub const IDENTITY: &str = "dev.lekalo.test-port@0.4.0";

/// The project-relative logical path the declaration lives at.
pub const PROJECT_PATH: &str = "lekalo/test-port.json";

/// The closed optional port surface, beyond the mandatory `invoke`.
const OPTIONAL_EXPORTS: &[&str] = &[
    "state",
    "fixtures",
    "actor",
    "clock",
    "ids",
    "emissions",
    "effects",
    "authorize",
    "contractCheck",
    "fixtureDigest",
    "reset",
];

/// One validated project test-port declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TestPort {
    /// The project-relative logical path of the runtime port module.
    path: String,
    /// The declared port surface (closed flags; `invoke` always true).
    exports: PortExports,
}

/// The closed port surface flags. Absent means `false`: the
/// corresponding scenario feature compiles to an explicit unsupported
/// diagnostic, never a guess.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub struct PortExports {
    /// Mandatory dispatch surface (`invoke(operation, input, ctx)`).
    pub invoke: bool,
    /// `state.seed` / `state.query` entity state surface.
    pub state: bool,
    /// `fixtures.load` / `fixtures.digest` fixture surface.
    pub fixtures: bool,
    /// `actor(ref, scope)` typed actor surface.
    pub actor: bool,
    /// `clock.freeze(iso)` deterministic clock surface.
    pub clock: bool,
    /// `ids.seed({algorithm, seed})` deterministic ID surface.
    pub ids: bool,
    /// `emissions()` capture-log surface.
    pub emissions: bool,
    /// `effects()` effect-ledger surface.
    pub effects: bool,
    /// `authorize(actor, policy, operation)` surface.
    pub authorize: bool,
    /// `contractCheck(contract, projection, actual)` surface.
    pub contract_check: bool,
    /// `fixtureDigest(fixture)` digest surface.
    pub fixture_digest: bool,
    /// `reset()` rerun-isolation surface.
    pub reset: bool,
}

impl PortExports {
    /// Whether the named closed surface is declared (`true`).
    pub fn declares(&self, surface: &str) -> bool {
        match surface {
            "invoke" => self.invoke,
            "state" => self.state,
            "fixtures" => self.fixtures,
            "actor" => self.actor,
            "clock" => self.clock,
            "ids" => self.ids,
            "emissions" => self.emissions,
            "effects" => self.effects,
            "authorize" => self.authorize,
            "contractCheck" => self.contract_check,
            "fixtureDigest" => self.fixture_digest,
            "reset" => self.reset,
            _ => false,
        }
    }
}

impl TestPort {
    /// The project-relative logical path of the runtime port module.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The declared port surface.
    pub fn exports(&self) -> &PortExports {
        &self.exports
    }

    /// Normalize one decoded JSON document into a validated test-port
    /// declaration, or return the typed rejection set. Pure: no file
    /// system, module loading, or target access of any kind.
    pub fn from_value(json: &Json) -> Result<TestPort, DiagnosticSet> {
        let object = json
            .as_object()
            .ok_or_else(|| port_shape("top-level-shape"))?;
        let keys: Vec<&str> = object.keys().map(String::as_str).collect();
        if keys != ["identity", "port", "schema_version"] {
            return Err(port_shape(if object.len() == 3 {
                "member-set"
            } else {
                "top-level-shape"
            }));
        }
        if object.get("schema_version").and_then(Json::as_str) != Some(SCHEMA_VERSION) {
            return Err(port_shape("schema-version"));
        }
        if object.get("identity").and_then(Json::as_str) != Some(IDENTITY) {
            return Err(port_shape("identity"));
        }
        let port = object
            .get("port")
            .and_then(Json::as_object)
            .ok_or_else(|| port_shape("port-shape"))?;
        let port_keys: Vec<&str> = port.keys().map(String::as_str).collect();
        if port_keys != ["exports", "path"] {
            return Err(port_shape("port-shape"));
        }
        let path = port
            .get("path")
            .and_then(Json::as_str)
            .ok_or_else(|| port_shape("path-shape"))?;
        if !path_valid(path) {
            return Err(port_shape("path-grammar"));
        }
        let exports = port
            .get("exports")
            .and_then(Json::as_object)
            .ok_or_else(|| port_shape("exports-shape"))?;
        let mut surface = PortExports {
            invoke: false,
            ..PortExports::default()
        };
        for (key, value) in exports {
            let declared = value.as_bool().ok_or_else(|| port_shape("export-flag"))?;
            if key == "invoke" {
                if !declared {
                    // The dispatch surface is the port: declaring it
                    // false is a shape violation, not an empty port.
                    return Err(port_shape("invoke-mandatory"));
                }
                surface.invoke = true;
                continue;
            }
            if !OPTIONAL_EXPORTS.contains(&key.as_str()) {
                return Err(port_shape("export-unknown"));
            }
            if declared {
                set_export(&mut surface, key);
            }
        }
        if !surface.invoke {
            return Err(port_shape("invoke-missing"));
        }
        Ok(TestPort {
            path: path.to_owned(),
            exports: surface,
        })
    }
}

fn set_export(surface: &mut PortExports, key: &str) {
    match key {
        "state" => surface.state = true,
        "fixtures" => surface.fixtures = true,
        "actor" => surface.actor = true,
        "clock" => surface.clock = true,
        "ids" => surface.ids = true,
        "emissions" => surface.emissions = true,
        "effects" => surface.effects = true,
        "authorize" => surface.authorize = true,
        "contractCheck" => surface.contract_check = true,
        "fixtureDigest" => surface.fixture_digest = true,
        "reset" => surface.reset = true,
        _ => {}
    }
}

/// The closed project-relative module path grammar: forward slashes, no
/// `..` segment, never absolute, always a `.mjs` or `.ts` module.
fn path_valid(path: &str) -> bool {
    if path.is_empty() || path.len() > 256 {
        return false;
    }
    if !path.ends_with(".mjs") && !path.ends_with(".ts") {
        return false;
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') {
        return false;
    }
    if path.contains(':') {
        return false;
    }
    path.split('/').all(|segment| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && segment.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'-'
            })
    })
}

/// The fatal set for one port-contract violation: one registered
/// `scenario.port-shape` diagnostic with a fixed class token.
fn port_shape(detail: &'static str) -> DiagnosticSet {
    let mut data = crate::diagnostics::DataObject::new();
    data.insert("detail".to_owned(), diagnostic::token(detail));
    match diagnostic::one("scenario.port-shape", None, data) {
        Ok(diagnostic) => diagnostic::invalid_set(vec![diagnostic]),
        Err(_) => crate::result::singleton_set("diagnostics.registry-invalid"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid() -> Json {
        json!({
            "schema_version": "lekalo/test-port/v0.4.0",
            "identity": "dev.lekalo.test-port@0.4.0",
            "port": {
                "path": "src/testing/port.mjs",
                "exports": {
                    "invoke": true,
                    "state": true,
                    "reset": true
                }
            }
        })
    }

    #[test]
    fn parses_a_declared_port_and_its_closed_surface() {
        let port = TestPort::from_value(&valid()).expect("valid port");
        assert_eq!(port.path(), "src/testing/port.mjs");
        assert!(port.exports().invoke);
        assert!(port.exports().declares("state"));
        assert!(port.exports().declares("reset"));
        assert!(!port.exports().declares("emissions"));
        assert!(!port.exports().declares("contractCheck"));
    }

    #[test]
    fn every_optional_member_absent_is_a_valid_invoke_only_port() {
        let document = json!({
            "schema_version": "lekalo/test-port/v0.4.0",
            "identity": "dev.lekalo.test-port@0.4.0",
            "port": {
                "path": "port.ts",
                "exports": {"invoke": true}
            }
        });
        let port = TestPort::from_value(&document).expect("invoke-only port");
        assert_eq!(port.path(), "port.ts");
        for surface in OPTIONAL_EXPORTS {
            assert!(!port.exports().declares(surface));
        }
    }

    #[test]
    fn unknown_members_kinds_and_identities_are_refused() {
        let mutated = |mutate: &dyn Fn(&mut Json)| {
            let mut document = valid();
            mutate(&mut document);
            assert!(TestPort::from_value(&document).is_err());
        };
        mutated(&|document| {
            document["schema_version"] = json!("lekalo/test-port/v0.2.16");
        });
        mutated(&|document| {
            document["identity"] = json!("dev.lekalo.test-port@0.2.16");
        });
        mutated(&|document| {
            document["extra"] = json!(1);
        });
        mutated(&|document| {
            document["port"]["extra"] = json!(1);
        });
        mutated(&|document| {
            document["port"]["exports"]["mystery"] = json!(true);
        });
        mutated(&|document| {
            document["port"]["exports"]["invoke"] = json!(false);
        });
        mutated(&|document| {
            document["port"]["exports"]["invoke"] = json!("true");
        });
        mutated(&|document| {
            document["port"]["path"] = json!("../escape.mjs");
        });
        mutated(&|document| {
            document["port"]["path"] = json!("/abs/port.mjs");
        });
        mutated(&|document| {
            document["port"]["path"] = json!("port.json");
        });
        mutated(&|document| {
            document["port"]["path"] = json!("space path.mjs");
        });
        assert!(TestPort::from_value(&json!([])).is_err());
    }

    #[test]
    fn identity_and_project_path_are_pinned() {
        assert_eq!(PROJECT_PATH, "lekalo/test-port.json");
        let (name, version) = IDENTITY.rsplit_once('@').expect("identity spelling");
        assert_eq!(name, "dev.lekalo.test-port");
        assert_eq!(version, "0.4.0");
        assert_eq!(SCHEMA_VERSION, "lekalo/test-port/v0.4.0");
    }
}

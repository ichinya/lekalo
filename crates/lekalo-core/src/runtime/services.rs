//! Named, explicitly selected process services. No language/provider registry.
use super::{config, error, Result, Session};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

pub struct Service {
    pub id: String,
    session: Session,
    operations: Vec<String>,
    pin: Option<super::RuntimePin>,
}
impl Service {
    pub fn invoke(&self, operation: &str, input: Value) -> Result<Value> {
        if !self.operations.iter().any(|op| op == operation) {
            return Err(error("service-operation-unsupported"));
        }
        if let Some(pin) = &self.pin {
            super::verify_runtime(pin)?;
        }
        let result = self.session.invoke(operation, input)?;
        if let Some(pin) = &self.pin {
            super::verify_runtime(pin)?;
        }
        Ok(result)
    }
}

/// LEKALO_SERVICES is an explicit development/embedding override. Project
/// selections always use lekalo.yaml and its verified runtime lock.
pub fn selected(root: &Path, role: &str) -> Result<Vec<Service>> {
    let matches =
        |id: &str| id == role || id.strip_prefix(role).is_some_and(|s| s.starts_with('/'));
    if let Ok(raw) = std::env::var("LEKALO_SERVICES") {
        let sources: BTreeMap<String, config::Source> =
            serde_json::from_value(config::value(raw.as_bytes())?)
                .map_err(|_| error("services-invalid"))?;
        if sources.len() > 32 || sources.keys().any(|id| !config::token(id)) {
            return Err(error("services-invalid"));
        }
        return sources
            .into_iter()
            .filter(|(id, _)| matches(id))
            .map(|(id, source)| {
                let session = Session::new(root, &source)?;
                let description =
                    super::describe_service(root, &session.command, session.package.as_ref())?;
                Ok(Service {
                    id,
                    session,
                    operations: description.operations,
                    pin: None,
                })
            })
            .collect();
    }
    // An absent optional config is different from an unreadable/invalid one.
    let fs = crate::project_fs::Fs::open(root).map_err(|_| error("project-unreadable"))?;
    let bytes = fs
        .read_file_opt("", config::CONFIG, 128 * 1024)
        .map_err(|_| error("config-unreadable"))?;
    let Some(bytes) = bytes else {
        return Ok(Vec::new());
    };
    let declaration = config::decode(&bytes)?;
    if !declaration.services.keys().any(|id| matches(id)) {
        return Ok(Vec::new());
    }
    let lock = super::check_lock(root)?;
    lock.services
        .into_iter()
        .filter(|(id, _)| matches(id))
        .map(|(id, pin)| {
            let current = super::describe_service(root, &pin.command, pin.package.as_ref())?;
            if serde_json::to_value(&current).unwrap()
                != serde_json::to_value(&pin.description).unwrap()
            {
                return Err(error("runtime-description-changed"));
            }
            Ok(Service {
                id,
                operations: pin.description.operations.clone(),
                pin: Some(pin.clone()),
                session: Session {
                    root: root.to_path_buf(),
                    command: pin.command,
                    package: pin.package,
                },
            })
        })
        .collect()
}

pub fn one(root: &Path, role: &str) -> Result<Service> {
    let mut services = selected(root, role)?;
    if services.len() != 1 {
        return Err(error("service-selection-required"));
    }
    Ok(services.remove(0))
}

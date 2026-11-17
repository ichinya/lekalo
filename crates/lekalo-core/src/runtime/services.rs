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
    let Some(declaration) = optional_config(root)? else {
        return Ok(Vec::new());
    };
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
                    files: pin.files,
                    package: pin.package,
                },
            })
        })
        .collect()
}

fn optional_config(root: &Path) -> Result<Option<config::Config>> {
    use crate::project_fs::{Fs, FsErrorKind};
    let fs = Fs::open(root).map_err(|_| error("project-unreadable"))?;
    match fs.read_file_opt("", config::CONFIG, 128 * 1024) {
        Ok(Some(bytes)) => config::decode(&bytes).map(Some),
        Ok(None) | Err(FsErrorKind::NotFound) => Ok(None),
        Err(_) => Err(error("config-unreadable")),
    }
}

pub fn one(root: &Path, role: &str) -> Result<Service> {
    let mut services = selected(root, role)?;
    if services.len() != 1 {
        return Err(error("service-selection-required"));
    }
    Ok(services.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_service_config_is_optional_but_invalid_entries_refuse() {
        let root = tempfile::tempdir().unwrap();
        assert!(optional_config(root.path()).unwrap().is_none());
        let path = root.path().join(config::CONFIG);
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            optional_config(root.path()).err().unwrap().detail,
            "config-unreadable"
        );
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, b"[invalid").unwrap();
        assert!(optional_config(root.path()).is_err());
    }
}

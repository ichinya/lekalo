//! Language-neutral component configuration. Parsing never launches a package.
use super::{error, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Component as PathComponent, Path};

pub const CONFIG: &str = "lekalo.yaml";
pub const LOCK: &str = "lekalo.runtime.lock";
pub const SCHEMA: &str = "lekalo/runtime-config/v1";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub components: BTreeMap<String, Component>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub path: String,
    pub runtime: Source,
    #[serde(default)]
    pub rules: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Source {
    Package(String),
    Executable(Executable),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executable {
    /// Direct argv, never shell text. A script argument is pinned as a file.
    pub command: Vec<String>,
}

pub fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._/@".contains(&b))
}

pub fn relative(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && !s.contains(['\\', ':', '\0'])
        && !s.starts_with('/')
        && s.split('/').all(|p| !p.is_empty() && p != "..")
        && Path::new(s)
            .components()
            .all(|p| matches!(p, PathComponent::Normal(_) | PathComponent::CurDir))
}

/// Use the existing strict YAML/JSON frontends (including duplicate-key and
/// alias rejection), then deserialize the same closed data model.
pub fn value(bytes: &[u8]) -> Result<Value> {
    use crate::loader::frontends::{self, Node, Parsed, Scalar, Value as NodeValue};
    fn convert(node: Node) -> Result<Value> {
        Ok(match node.value {
            NodeValue::Scalar(s) => match s {
                Scalar::Null => Value::Null,
                Scalar::Bool(v) => Value::Bool(v),
                Scalar::Str(v) => Value::String(v),
                Scalar::Int(v) => Value::Number(
                    i64::try_from(v)
                        .map_err(|_| error("document-number"))?
                        .into(),
                ),
                Scalar::Float(v) => Value::Number(
                    serde_json::Number::from_f64(v).ok_or_else(|| error("document-number"))?,
                ),
            },
            NodeValue::Seq(items) => {
                Value::Array(items.into_iter().map(convert).collect::<Result<_>>()?)
            }
            NodeValue::Map(items) => {
                let mut out = Map::new();
                for item in items {
                    out.insert(item.key, convert(item.value)?);
                }
                Value::Object(out)
            }
        })
    }
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(error("document-limit"));
    }
    let text = std::str::from_utf8(bytes).map_err(|_| error("document-encoding"))?;
    match frontends::parse_document(text, &crate::loader::source::LineIndex::new(text))
        .map_err(|_| error("document-invalid"))?
    {
        Parsed::Root(root) => convert(*root),
        Parsed::Empty => Err(error("document-empty")),
    }
}

pub fn decode(bytes: &[u8]) -> Result<Config> {
    let config: Config =
        serde_json::from_value(value(bytes)?).map_err(|_| error("config-invalid"))?;
    if config.schema != SCHEMA {
        return Err(Error::version("config-version"));
    }
    if config.components.len() > 64 {
        return Err(error("component-limit"));
    }
    let mut paths = Vec::new();
    for (id, component) in &config.components {
        if !token(id)
            || !relative(&component.path)
            || component.rules.len() > 32
            || component
                .rules
                .iter()
                .any(|s| s.is_empty() || s.len() > 2048 || s.chars().any(char::is_control))
        {
            return Err(error("component-invalid"));
        }
        let path: std::path::PathBuf = Path::new(&component.path)
            .components()
            .filter(|p| matches!(p, PathComponent::Normal(_)))
            .collect();
        if paths
            .iter()
            .any(|p: &std::path::PathBuf| path.starts_with(p) || p.starts_with(&path))
        {
            return Err(error("component-overlap"));
        }
        paths.push(path);
        match &component.runtime {
            Source::Package(s) if s.is_empty() || s.len() > 2048 => {
                return Err(error("source-invalid"))
            }
            Source::Executable(e)
                if e.command.is_empty()
                    || e.command.len() > 16
                    || e.command
                        .iter()
                        .any(|a| a.is_empty() || a.len() > 4096 || a.contains('\0')) =>
            {
                return Err(error("command-invalid"))
            }
            _ => (),
        }
    }
    Ok(config)
}

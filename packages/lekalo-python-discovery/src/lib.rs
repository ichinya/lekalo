//! Python manifest recognition used before a runtime has been installed.
//! These are filename observations, not permission to run setup.py or import code.
#![forbid(unsafe_code)]

pub const MANIFESTS: &[(&str, &str)] = &[
    ("pyproject.toml", "python"),
    ("requirements.txt", "python"),
    ("setup.py", "python"),
];
pub const MODULE_MANIFEST: &str = "pyproject.toml";

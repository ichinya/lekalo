//! Explicit preprovisioned tool assets, distinct from Model/source paths.
use super::*;

pub(super) const TOOLS_CATALOG_VERSION: &str = "lekalo/native-tool-catalog/v1.0.0";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetFile {
    pub path: String,
    pub digest: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetPin {
    pub manifest_digest: String,
    pub file_count: usize,
    pub total_bytes: usize,
    pub runtime_version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCatalog {
    schema_version: String,
    runtime: Runtime,
    assets: Assets,
    tools: Vec<Tool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    program: PathBuf,
    program_digest: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Assets {
    root: PathBuf,
    manifest_digest: String,
    files: Vec<AssetFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Tool {
    id: String,
    entry: String,
    entry_digest: String,
    artifact_digest: String,
    version: String,
}

pub(super) struct CapturedTools {
    pub catalog: Catalog,
    pub pin: AssetPin,
    pub entries: BTreeMap<String, (String, String)>,
    files: BTreeMap<String, Vec<u8>>,
}

pub(crate) fn asset_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && path.split('/').all(|part| {
            !part.is_empty()
                && part.len() <= 128
                && part != "."
                && part != ".."
                && !matches!(
                    part.to_ascii_lowercase().as_str(),
                    ".git" | ".ssh" | ".npmrc" | ".netrc"
                )
                && !part.to_ascii_lowercase().starts_with(".env")
                && !part.ends_with('.')
                && !part.ends_with(' ')
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"@_.+-".contains(&c))
                && !matches!(
                    part.split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_uppercase()
                        .as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
}

fn physical_root(root: &Path) -> Result<(), Failure> {
    if !root.is_absolute() {
        return Err(invalid("execution-assets-root"));
    }
    for ancestor in root.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| Failure::PlanStale)?;
        if !metadata.is_dir() || crate::project_fs::metadata_is_physical_link(&metadata) {
            return Err(Failure::SecurityViolation);
        }
    }
    Ok(())
}

fn capture_assets(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, Failure> {
    physical_root(root)?;
    let fs = Fs::open(root).map_err(|_| Failure::SecurityViolation)?;
    let mut files = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut stack = vec![(String::new(), 0usize)];
    let mut directories = 0usize;
    let mut total = 0usize;
    while let Some((directory, depth)) = stack.pop() {
        if depth > 32 {
            return Err(invalid("execution-assets-bound"));
        }
        for (name, kind) in fs
            .entries_bounded(&directory, 8192)
            .map_err(|_| Failure::SecurityViolation)?
        {
            if name == ".bin" {
                continue;
            }
            let path = if directory.is_empty() {
                name.clone()
            } else {
                format!("{directory}/{name}")
            };
            if !asset_path(&path) || !seen.insert(path.to_ascii_lowercase()) {
                return Err(Failure::SecurityViolation);
            }
            match kind {
                EntryType::Directory => {
                    directories += 1;
                    if directories > 4096 {
                        return Err(invalid("execution-assets-bound"));
                    }
                    stack.push((path, depth + 1));
                }
                EntryType::File => {
                    if files.len() >= 4096 {
                        return Err(invalid("execution-assets-bound"));
                    }
                    let bytes = fs
                        .read_file_opt(&directory, &name, 32 * 1024 * 1024)
                        .map_err(|_| Failure::SecurityViolation)?
                        .ok_or(Failure::PlanStale)?;
                    total = total
                        .checked_add(bytes.len())
                        .ok_or_else(|| invalid("execution-assets-bound"))?;
                    if total > 128 * 1024 * 1024 {
                        return Err(invalid("execution-assets-bound"));
                    }
                    files.insert(path, bytes);
                }
                _ => return Err(Failure::SecurityViolation),
            }
        }
    }
    Ok(files)
}

pub(super) fn capture(bytes: &[u8]) -> Result<CapturedTools, Failure> {
    let source: ToolCatalog = decode(bytes)?;
    if source.schema_version != TOOLS_CATALOG_VERSION
        || source.tools.is_empty()
        || source.tools.len() > 32
        || source.assets.files.is_empty()
        || source.assets.files.len() > 4096
        || !source.runtime.version.starts_with("24.")
    {
        return Err(invalid("execution-tools-catalog"));
    }
    let files = capture_assets(&source.assets.root)?;
    let manifest: Vec<_> = files
        .iter()
        .map(|(path, bytes)| AssetFile {
            path: path.clone(),
            digest: hash(bytes),
            bytes: bytes.len() as u64,
        })
        .collect();
    if manifest != source.assets.files
        || hash(&canonical(&manifest)?) != source.assets.manifest_digest
    {
        return Err(Failure::PlanStale);
    }
    let mut tools = Vec::new();
    let mut entries = BTreeMap::new();
    for tool in source.tools {
        if !scopes::is_token(&tool.id)
            || !asset_path(&tool.entry)
            || entries.contains_key(&tool.id)
            || tool.artifact_digest != source.assets.manifest_digest
            || files
                .get(&tool.entry)
                .is_none_or(|bytes| hash(bytes) != tool.entry_digest)
        {
            return Err(Failure::PlanStale);
        }
        let mut directory = tool
            .entry
            .rsplit_once('/')
            .map(|(dir, _)| dir)
            .unwrap_or("");
        let mut admitted = false;
        loop {
            let manifest_path = if directory.is_empty() {
                "package.json".to_owned()
            } else {
                format!("{directory}/package.json")
            };
            if let Some(bytes) = files.get(&manifest_path) {
                let package: Value = decode(bytes)?;
                let bin = package
                    .get("bin")
                    .and_then(|bin| bin.get(&tool.id).or_else(|| bin.as_str().map(|_| bin)))
                    .and_then(Value::as_str);
                let entry = bin.map(|bin| {
                    format!(
                        "{}{}{}",
                        directory,
                        if directory.is_empty() { "" } else { "/" },
                        bin.strip_prefix("./").unwrap_or(bin)
                    )
                });
                admitted = package.get("version").and_then(Value::as_str)
                    == Some(tool.version.as_str())
                    && entry.as_deref() == Some(tool.entry.as_str());
                break;
            }
            if directory.is_empty() {
                break;
            }
            directory = directory
                .rsplit_once('/')
                .map(|(parent, _)| parent)
                .unwrap_or("");
        }
        if !admitted {
            return Err(invalid("execution-tool-package-bin"));
        }
        entries.insert(tool.id.clone(), (tool.entry, tool.entry_digest));
        tools.push(CatalogTool {
            id: tool.id,
            program: source.runtime.program.clone(),
            program_digest: source.runtime.program_digest.clone(),
            artifact_digest: tool.artifact_digest,
            version: tool.version,
        });
    }
    let pin = AssetPin {
        manifest_digest: source.assets.manifest_digest,
        file_count: files.len(),
        total_bytes: files.values().map(Vec::len).sum(),
        runtime_version: source.runtime.version,
    };
    Ok(CapturedTools {
        catalog: Catalog {
            schema_version: TOOLS_CATALOG_VERSION.into(),
            tools,
        },
        pin,
        entries,
        files,
    })
}

impl CapturedTools {
    pub(super) fn stage(&self, root: &Path) -> Result<(), Failure> {
        for (path, bytes) in &self.files {
            let destination = root.join(path);
            std::fs::create_dir_all(
                destination
                    .parent()
                    .ok_or_else(|| invalid("execution-assets-path"))?,
            )
            .map_err(|_| Failure::RunInfrastructure)?;
            std::fs::write(destination, bytes).map_err(|_| Failure::RunInfrastructure)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Value) {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("bundle")).unwrap();
        std::fs::write(root.path().join("bundle/cli.mjs"), b"process.exit(0)").unwrap();
        std::fs::write(
            root.path().join("bundle/package.json"),
            br#"{"name":"example","version":"1.2.3","bin":{"check":"cli.mjs"}}"#,
        )
        .unwrap();
        let files = capture_assets(root.path()).unwrap();
        let manifest: Vec<_> = files
            .iter()
            .map(|(path, bytes)| AssetFile {
                path: path.clone(),
                digest: hash(bytes),
                bytes: bytes.len() as u64,
            })
            .collect();
        let digest = hash(&canonical(&manifest).unwrap());
        let value = json!({"schema_version":TOOLS_CATALOG_VERSION,"runtime":{"program":root.path().join("node.exe"),"program_digest":hash(b"node"),"version":"24.13.0"},"assets":{"root":root.path(),"manifest_digest":digest,"files":manifest},"tools":[{"id":"check","entry":"bundle/cli.mjs","entry_digest":hash(b"process.exit(0)"),"artifact_digest":digest,"version":"1.2.3"}]});
        (root, value)
    }
    #[test]
    fn asset_paths_are_distinct_from_source_paths_but_never_escape() {
        for valid in [
            "@scope/pkg/dist/Loader+One.cjs",
            "pkg/.data/file.json",
            "pkg/index-1.mjs",
        ] {
            assert!(asset_path(valid), "{valid}");
        }
        for invalid in [
            "../file",
            "/absolute",
            "pkg/../file",
            "pkg//file",
            "pkg/file.",
            "pkg/CON.js",
            "pkg/.env.local",
            ".git/config",
            "pkg/.npmrc",
            "pkg/file:stream",
            "pkg/file name",
        ] {
            assert!(!asset_path(invalid), "{invalid}");
        }
    }
    #[test]
    fn pinned_assets_versions_and_bins_are_independent_custody() {
        let (root, value) = fixture();
        assert!(capture(&canonical(&value).unwrap()).is_ok());
        for changed in ["entry_digest", "version", "entry"] {
            let mut bad = value.clone();
            bad["tools"][0][changed] = json!("wrong");
            assert!(capture(&canonical(&bad).unwrap()).is_err(), "{changed}");
        }
        std::fs::write(root.path().join("bundle/cli.mjs"), b"process.exit(1)").unwrap();
        assert!(matches!(
            capture(&canonical(&value).unwrap()),
            Err(Failure::PlanStale)
        ));
    }
    #[test]
    fn hardlinks_and_excess_depth_refuse_before_assets_are_admitted() {
        let (root, _) = fixture();
        std::fs::hard_link(
            root.path().join("bundle/cli.mjs"),
            root.path().join("bundle/alias.mjs"),
        )
        .unwrap();
        assert!(capture_assets(root.path()).is_err());
        std::fs::remove_file(root.path().join("bundle/alias.mjs")).unwrap();
        let mut directory = root.path().to_path_buf();
        for _ in 0..34 {
            directory = directory.join("a");
            std::fs::create_dir(&directory).unwrap();
        }
        assert!(capture_assets(root.path()).is_err());
    }
    #[test]
    fn an_oversize_asset_is_refused_before_allocating_it() {
        let (root, _) = fixture();
        let file = std::fs::File::create(root.path().join("large.bin")).unwrap();
        file.set_len(32 * 1024 * 1024 + 1).unwrap();
        assert!(capture_assets(root.path()).is_err());
    }
}

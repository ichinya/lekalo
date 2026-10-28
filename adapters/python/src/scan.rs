//! Read-scope-bounded file inventory and scan-entry assembly (issue
//! #146). The walk honors declared read roots and exclusions, refuses
//! symlinks/junctions by skipping them, and never reads outside the
//! staged view — `project_root` is always the confined `"."`.

use std::fs;
use std::path::{Path, PathBuf};

use lekalo_adapter_sdk::json::{canonical, Value};

use crate::ids;
use crate::profile::{Profile, RootKind};
use crate::symbols;
use crate::symbols::Sym;

const MAX_FILES: usize = 4096;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 24;
/// The wire bound is per entry forest (carrier plus transitive
/// members), never per file: `scanEntryEvidence.native_symbols`
/// accepts exactly one complete subtree of at most 256 symbols.
const MAX_ENTRY_SYMBOLS: usize = 256;
/// `result.entries` wire bound.
const MAX_RESULT_ENTRIES: usize = 10_000;
/// The host's total native-inventory bound across the whole response:
/// the sum of every entry's `native_symbols` may never exceed it.
const MAX_TOTAL_SYMBOLS: usize = 10_000;
/// `evidence.native_uncertainty` wire bound.
const MAX_UNCERTAINTY: usize = 4096;
const REFS_PREDECESSOR: usize = 8;
const REFS_SUCCESSOR: usize = 256;
/// Predecessor entries keep the closed detail token (128 chars); the
/// custody successors widen `scanEntry.detail` to 2048.
const DETAIL_PREDECESSOR: usize = 128;

/// Version tier of the negotiated protocol: predecessor entries carry
/// signature + ≤8 references, 0.9.x adds custody, 1.1.0 adds native.
#[derive(Clone, Copy, PartialEq)]
pub enum Tier {
    Predecessor,
    Custody,
    Native,
}

pub fn tier(version: &str) -> Tier {
    match version {
        "1.1.0" => Tier::Native,
        "0.9.0" | "0.9.1" => Tier::Custody,
        _ => Tier::Predecessor,
    }
}

pub struct Scan {
    pub entries: Vec<Value>,
    pub uncertainty: Vec<Value>,
    pub truncated: bool,
}

struct FileRec {
    path: String,
    module: String,
    bytes: Vec<u8>,
}

pub fn run(profile: &Profile, version: &str) -> Result<Scan, &'static str> {
    let tier = tier(version);
    let mut files: Vec<FileRec> = Vec::new();
    let mut truncated = false;
    let mut total_bytes: u64 = 0;
    'roots: for root in &profile.roots {
        match root.kind {
            RootKind::File => {
                if !is_python(&root.path) || profile.excludes(&root.path) {
                    continue;
                }
                if !push_file(&root.path, &root.path, &mut files, &mut total_bytes, &mut truncated) {
                    break 'roots;
                }
            }
            RootKind::Tree => {
                let base = PathBuf::from(root.path.replace('/', &std::path::MAIN_SEPARATOR.to_string()));
                walk_tree(&base, &root.path, &root.path, 0, profile, &mut files, &mut total_bytes, &mut truncated);
                if truncated {
                    break 'roots;
                }
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));

    // One entry per top-level declaration — the node kernel's shape:
    // `detail` proposes the semantic id, the evidence block carries the
    // symbol plus its owned member forest and the module's outbound
    // references. A declaration-free module reports its module root so
    // the file stays visible to custody.
    let mut entries: Vec<Value> = Vec::new();
    let mut total_symbols = 0usize;
    let mut uncertainty: Vec<symbols::UncRow> = Vec::new();
    for file in files {
        let digest = ids::source_digest(&file.bytes);
        let stub = file.path.ends_with(".pyi");
        let text = match std::str::from_utf8(&file.bytes) {
            Ok(text) => text.to_owned(),
            Err(_) => {
                uncertainty.push(symbols::UncRow {
                    path: file.path.clone(),
                    detail: "non-utf8-source".to_owned(),
                    line: None,
                    source_digest: digest.clone(),
                });
                String::new()
            }
        };
        let mut scan = symbols::extract(&file.path, &file.module, &text, stub, &digest);
        if text.is_empty() {
            scan.symbols.truncate(1); // module root only
        }
        let overflow = scan
            .references
            .len()
            .saturating_sub(if tier == Tier::Native {
                REFS_SUCCESSOR
            } else {
                REFS_PREDECESSOR
            });
        let (produced, bounded) = file_entries(&file.path, &scan, &digest, stub, tier);
        let mut bound = overflow > 0 || bounded;
        bound |= accept_entries(produced, &mut entries, &mut total_symbols);
        if bound {
            truncated = true;
            uncertainty.push(symbols::UncRow {
                path: file.path.clone(),
                detail: "native-inventory-bound".to_owned(),
                line: None,
                source_digest: digest.clone(),
            });
        }
        uncertainty.extend(scan.uncertainty.drain(..));
    }

    uncertainty.sort();
    uncertainty.dedup();
    if uncertainty.len() > MAX_UNCERTAINTY {
        uncertainty.truncate(MAX_UNCERTAINTY);
        truncated = true;
    }
    Ok(Scan {
        entries,
        uncertainty: uncertainty
            .into_iter()
            .map(|row| {
                let mut members = vec![
                    ("detail".to_owned(), Value::Str(row.detail)),
                    ("kind".to_owned(), Value::Str("any-surface".to_owned())),
                ];
                if let Some(line) = row.line {
                    members.push(("line".to_owned(), Value::Int(line as i64)));
                }
                members.extend([
                    ("path".to_owned(), Value::Str(row.path)),
                    (
                        "source_digest".to_owned(),
                        Value::Str(row.source_digest),
                    ),
                ]);
                Value::Object(members)
            })
            .collect(),
        truncated,
    })
}

fn is_python(path: &str) -> bool {
    path.ends_with(".py") || path.ends_with(".pyi")
}

/// The importable module name of one scanned file: relative to its read
/// root, extension stripped, separators folded to dots. `__init__.py[i]`
/// is the package marker — its module is the package directory itself,
/// so `pkg/__init__.py` declares `pkg.*`, not `pkg.__init__.*`.
fn module_name(logical: &str, root: &str) -> String {
    let relative = logical
        .strip_prefix(root)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or_else(|| logical.rsplit('/').next().unwrap_or(logical));
    let stem = relative
        .strip_suffix(".pyi")
        .or_else(|| relative.strip_suffix(".py"))
        .unwrap_or(relative);
    let mut segments: Vec<&str> = stem.split('/').collect();
    if segments.last() == Some(&"__init__") {
        segments.pop();
    }
    if segments.is_empty() {
        // `__init__.py` at the read root: the package is the containing
        // directory itself, so its basename is the module name (the
        // root may be a file scope, so the parent comes from `logical`).
        return logical
            .rsplit('/')
            .nth(1)
            .filter(|name| !name.is_empty())
            .unwrap_or("__init__")
            .to_owned();
    }
    segments.join(".")
}

/// Read one file under the staged view; false means the inventory is
/// full and the caller records truncation.
fn push_file(
    logical: &str,
    root: &str,
    files: &mut Vec<FileRec>,
    total: &mut u64,
    truncated: &mut bool,
) -> bool {
    if files.len() >= MAX_FILES || *total >= MAX_TOTAL_BYTES {
        *truncated = true;
        return false;
    }
    let disk = PathBuf::from(logical.replace('/', &std::path::MAIN_SEPARATOR.to_string()));
    let Ok(bytes) = fs::read(&disk) else {
        return true; // vanished between walk and read: skip, never fail open
    };
    if bytes.len() as u64 > MAX_FILE_BYTES || *total + bytes.len() as u64 > MAX_TOTAL_BYTES {
        *truncated = true;
        return false;
    }
    *total += bytes.len() as u64;
    files.push(FileRec {
        path: logical.to_owned(),
        module: module_name(logical, root),
        bytes,
    });
    true
}

fn walk_tree(
    disk: &Path,
    logical: &str,
    root: &str,
    depth: usize,
    profile: &Profile,
    files: &mut Vec<FileRec>,
    total: &mut u64,
    truncated: &mut bool,
) {
    if depth > MAX_DEPTH || files.len() >= MAX_FILES || *total >= MAX_TOTAL_BYTES {
        *truncated = true;
        return;
    }
    let Ok(read_dir) = fs::read_dir(disk) else {
        return; // an unreadable root subtree is absence, not authority
    };
    let mut names: Vec<_> = read_dir.flatten().collect();
    names.sort_by_key(|entry| entry.file_name());
    for entry in names {
        if files.len() >= MAX_FILES || *total >= MAX_TOTAL_BYTES {
            *truncated = true;
            return;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let child_logical = format!("{logical}/{name}");
        if profile.excludes(&child_logical) {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if meta.is_symlink() || is_reparse(&entry.path()) {
            continue;
        }
        if meta.is_dir() {
            walk_tree(
                &entry.path(),
                &child_logical,
                root,
                depth + 1,
                profile,
                files,
                total,
                truncated,
            );
        } else if meta.is_file() && is_python(&child_logical) {
            // The read root doubles as the import root for the file.
            if !push_file(&child_logical, root, files, total, truncated) {
                return;
            }
        }
    }
}

#[cfg(windows)]
fn is_reparse(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    fs::metadata(path)
        .map(|meta| meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_reparse(_path: &Path) -> bool {
    false
}

/// Accept produced entries under the response-wide bounds: 10 000
/// entries and 10 000 total native symbols. The first entry that does
/// not fit ends the intake — the remainder is reported as bounded
/// rather than silently merged into a partial inventory.
fn accept_entries(
    produced: Vec<(Value, usize)>,
    entries: &mut Vec<Value>,
    total_symbols: &mut usize,
) -> bool {
    for (value, count) in produced {
        if entries.len() >= MAX_RESULT_ENTRIES || *total_symbols + count > MAX_TOTAL_SYMBOLS {
            return true;
        }
        *total_symbols += count;
        entries.push(value);
    }
    false
}

/// The closed `ScanEntry` set of one file: one row per top-level
/// declaration, each carrying its own symbol forest in `native_symbols`
/// and the module's bounded outbound references — the node kernel's
/// per-symbol shape, so the observed merge consumes it unchanged.
fn file_entries(
    path: &str,
    scan: &symbols::FileScan,
    digest: &str,
    stub: bool,
    tier: Tier,
) -> (Vec<(Value, usize)>, bool) {
    let ref_cap = if tier == Tier::Native {
        REFS_SUCCESSOR
    } else {
        REFS_PREDECESSOR
    };
    let references: Vec<Value> = scan
        .references
        .iter()
        .take(ref_cap)
        .map(|row| {
            Value::Object(vec![
                (
                    "confidence".to_owned(),
                    Value::Str(row.confidence.to_owned()),
                ),
                ("role".to_owned(), Value::Str(row.role.to_owned())),
                ("target".to_owned(), Value::Str(row.target.clone())),
            ])
        })
        .collect();
    // Top-level carriers are the symbols owned directly by the module
    // root; a declaration-free file reports the module root itself.
    let mut carriers: Vec<&Sym> = scan
        .symbols
        .iter()
        .filter(|sym| sym.member_of.as_deref() == Some(scan.root_native.as_str()))
        .collect();
    if carriers.is_empty() {
        if let Some(root) = scan.symbols.first() {
            carriers.push(root);
        }
    }
    let mut bounded = false;
    let mut produced: Vec<(Value, usize)> = carriers
        .into_iter()
        .filter_map(|sym| entry(path, sym, scan, &references, digest, stub, tier, &mut bounded))
        .collect();
    if produced.is_empty() {
        // Every carrier was unrepresentable — the declaration-free
        // fallback still keeps the file visible through its module
        // root when that root itself fits the tier's detail budget.
        if let Some(root) = scan.symbols.first() {
            if let Some(row) = entry(path, root, scan, &references, digest, stub, tier, &mut bounded)
            {
                produced.push(row);
            }
        }
    }
    (produced, bounded)
}

fn entry(
    path: &str,
    sym: &Sym,
    scan: &symbols::FileScan,
    references: &[Value],
    digest: &str,
    stub: bool,
    tier: Tier,
    bounded: &mut bool,
) -> Option<(Value, usize)> {
    // Predecessor detail is the closed 128-char token: the mandatory
    // `s`/`n` pair still fits once the optional members are dropped and
    // `n` carries a stable 64-bit prefix instead of the full digest. A
    // name that cannot fit even that minimal form is bounded away —
    // never a truncated or fabricated proposal.
    let mut detail = match tier {
        Tier::Predecessor => {
            let mut members = vec![
                (
                    "n".to_owned(),
                    Value::Str(sym.native.get(..20).unwrap_or(&sym.native).to_owned()),
                ),
                ("s".to_owned(), Value::Str(sym.qualified_name.clone())),
            ];
            if stub {
                members.push(("q".to_owned(), Value::Str("low".to_owned())));
            }
            members
        }
        _ => vec![
            ("n".to_owned(), Value::Str(sym.native.clone())),
            ("s".to_owned(), Value::Str(sym.qualified_name.clone())),
            ("l".to_owned(), Value::Int(sym.line as i64)),
            (
                "q".to_owned(),
                Value::Str(if stub { "low" } else { "medium" }.to_owned()),
            ),
        ],
    };
    detail.sort_by(|a, b| a.0.cmp(&b.0));
    let detail = canonical(&Value::Object(detail));
    if tier == Tier::Predecessor && detail.len() > DETAIL_PREDECESSOR {
        *bounded = true;
        return None;
    }
    let mut evidence = Vec::new();
    if !references.is_empty() {
        evidence.push((
            "references".to_owned(),
            Value::Array(references.to_vec()),
        ));
    }
    if let Some(signature) = &sym.signature {
        evidence.push((
            "signature".to_owned(),
            Value::Str(signature.clone()),
        ));
    }
    if tier != Tier::Predecessor {
        evidence.push((
            "source_digest".to_owned(),
            Value::Str(digest.to_owned()),
        ));
    }
    let mut forest = 0usize;
    if tier == Tier::Native {
        // The carrier plus every symbol transitively owned by it. On the
        // wire each entry is its own closed forest: the carrier is the
        // single root, so its `member_of` link to the module is dropped
        // (the host revalidates exactly one root per entry). The forest
        // bound is per entry: an oversized subtree keeps its
        // ancestor-closed prefix — a member never precedes its owner's
        // opening line, and the depth tie-break keeps same-line members
        // below their owner — and the cut is reported instead of
        // fabricating completeness.
        let mut owned: Vec<Sym> = scan
            .symbols
            .iter()
            .filter(|row| owned_by(row, &sym.native, scan))
            .cloned()
            .collect();
        if owned.len() > MAX_ENTRY_SYMBOLS {
            owned.sort_by(|a, b| {
                (a.line, depth_of(a, scan), &a.native).cmp(&(b.line, depth_of(b, scan), &b.native))
            });
            owned.truncate(MAX_ENTRY_SYMBOLS);
            *bounded = true;
        }
        forest = owned.len();
        for row in &mut owned {
            if row.native == sym.native {
                row.member_of = None;
            }
        }
        owned.sort_by(|a, b| a.native.cmp(&b.native));
        evidence.push((
            "native_symbols".to_owned(),
            Value::Array(owned.iter().map(|row| row.value()).collect()),
        ));
    }
    Some((
        Value::Object(vec![
            ("detail".to_owned(), Value::Str(detail)),
            ("evidence".to_owned(), Value::Object(evidence)),
            ("kind".to_owned(), Value::Str("entity".to_owned())),
            ("path".to_owned(), Value::Str(path.to_owned())),
        ]),
        forest,
    ))
}

/// Member's `member_of` chain length within the file forest — the
/// sort tie-break that keeps an owner ahead of same-line members.
fn depth_of(row: &Sym, scan: &symbols::FileScan) -> usize {
    let mut depth = 0usize;
    let mut cursor = row.member_of.as_deref();
    for _ in 0..32 {
        match cursor {
            Some(parent) => {
                depth += 1;
                cursor = scan
                    .symbols
                    .iter()
                    .find(|sym| sym.native == parent)
                    .and_then(|sym| sym.member_of.as_deref());
            }
            None => break,
        }
    }
    depth
}

/// Whether `row` is the carrier itself or a transitive member of it.
fn owned_by(row: &Sym, owner: &str, scan: &symbols::FileScan) -> bool {
    if row.native == owner {
        return true;
    }
    let mut cursor = row.member_of.as_deref();
    for _ in 0..32 {
        match cursor {
            Some(parent) if parent == owner => return true,
            Some(parent) => {
                cursor = scan
                    .symbols
                    .iter()
                    .find(|sym| sym.native == parent)
                    .and_then(|sym| sym.member_of.as_deref());
            }
            None => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_file_names_its_package_not_the_marker() {
        assert_eq!(module_name("src/shop/__init__.py", "src"), "shop");
        assert_eq!(module_name("src/a/b/__init__.pyi", "src"), "a.b");
        assert_eq!(
            module_name("src/shop/__init__.py", "src/shop/__init__.py"),
            "shop"
        );
        assert_eq!(module_name("src/__init__.py", "src"), "src");
        assert_eq!(module_name("__init__.py", "__init__.py"), "__init__");
    }

    #[test]
    fn ordinary_modules_fold_to_dotted_names() {
        assert_eq!(module_name("src/shop/orders.py", "src"), "shop.orders");
        assert_eq!(module_name("src/shop/__main__.py", "src"), "shop.__main__");
        assert_eq!(module_name("src/shop/_private.py", "src"), "shop._private");
    }

    fn native_count(entry: &Value) -> usize {
        let Value::Object(members) = entry else {
            panic!("entry is an object");
        };
        let Some((_, Value::Object(evidence))) = members
            .iter()
            .find(|(key, _)| key == "evidence")
        else {
            panic!("entry carries evidence");
        };
        let Some((_, Value::Array(symbols))) = evidence
            .iter()
            .find(|(key, _)| key == "native_symbols")
        else {
            panic!("evidence carries native_symbols");
        };
        symbols.len()
    }

    #[test]
    fn wide_files_emit_one_entry_per_declaration() {
        // 300 top-level declarations are 300 single-symbol forests, not
        // one truncated file: the wire bound is per entry, not per file.
        let source: String = (0..300).map(|i| format!("v{i} = {i}\n")).collect();
        let scan = symbols::extract("pkg/mod.py", "pkg.mod", &source, false, "sha256:x");
        let (entries, bounded) =
            file_entries("pkg/mod.py", &scan, "sha256:x", false, Tier::Native);
        assert!(!bounded);
        assert_eq!(entries.len(), 300);
    }

    #[test]
    fn oversized_forest_reports_its_bound() {
        // One class with 300 methods keeps the ancestor-closed 256-symbol
        // prefix and reports the bound instead of fabricating completeness.
        let methods: String = (0..300)
            .map(|i| format!("    def m{i}(self):\n        pass\n"))
            .collect();
        let source = format!("class A:\n{methods}");
        let scan = symbols::extract("pkg/mod.py", "pkg.mod", &source, false, "sha256:x");
        let (entries, bounded) =
            file_entries("pkg/mod.py", &scan, "sha256:x", false, Tier::Native);
        assert!(bounded);
        assert_eq!(entries.len(), 1);
        assert_eq!(native_count(&entries[0].0), MAX_ENTRY_SYMBOLS);
    }

    #[test]
    fn total_symbol_bound_ends_intake() {
        // The response-wide 10 000-symbol budget is cumulative across
        // entries: the first entry whose forest does not fit closes the
        // intake and the overflow is reported as bounded.
        let mut entries = Vec::new();
        let mut total = 0usize;
        let produced: Vec<(Value, usize)> = (0..45)
            .map(|_| (Value::Object(vec![]), MAX_ENTRY_SYMBOLS))
            .collect();
        assert!(accept_entries(produced, &mut entries, &mut total));
        assert_eq!(entries.len(), 39);
        assert_eq!(total, 39 * MAX_ENTRY_SYMBOLS);
    }

    #[test]
    fn predecessor_detail_stays_under_its_bound() {
        // A long qualified name cannot fit the closed 128-char token:
        // the entry is dropped and the file reports the bound rather
        // than emitting oversized detail or a fabricated proposal.
        let name = "a".repeat(100);
        let source = format!("def {name}_x():\n    pass\ndef short():\n    pass\n");
        let scan = symbols::extract("pkg/mod.py", "pkg.mod", &source, false, "sha256:x");
        let (entries, bounded) =
            file_entries("pkg/mod.py", &scan, "sha256:x", false, Tier::Predecessor);
        assert!(bounded);
        assert_eq!(entries.len(), 1);
        let Value::Object(members) = &entries[0].0 else {
            panic!("entry is an object");
        };
        let Some((_, Value::Str(detail))) = members.iter().find(|(key, _)| key == "detail")
        else {
            panic!("entry carries detail");
        };
        assert!(detail.len() <= DETAIL_PREDECESSOR);
    }
}

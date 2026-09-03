//! Import graph: module-ID keyed vertices, duplicate detection, missing
//! imports, and deterministic cycle reporting.

use std::collections::BTreeMap;

use super::error::Diagnostic;

/// One discovered module: its semantic ID, physical directory name, and the
/// imports declared in its `module.yaml`.
#[derive(Clone, Debug)]
pub struct ModuleVertex {
    /// Semantic module ID from the module definition (`module.yaml`).
    pub module_id: String,
    /// Physical directory name under `lekalo/modules/`.
    pub directory: String,
    /// Declared imports as `(module id, source path)` pairs.
    pub imports: Vec<String>,
}

/// Duplicate module IDs across physical directories.
pub fn duplicate_module_ids(vertices: &[ModuleVertex]) -> Vec<Diagnostic> {
    let mut by_id: BTreeMap<&str, Vec<&ModuleVertex>> = BTreeMap::new();
    for vertex in vertices {
        by_id
            .entry(vertex.module_id.as_str())
            .or_default()
            .push(vertex);
    }
    let mut diagnostics = Vec::new();
    for (module_id, group) in by_id {
        if group.len() > 1 {
            let directories: Vec<String> = group
                .iter()
                .map(|vertex| format!("lekalo/modules/{}", vertex.directory))
                .collect();
            diagnostics.push(
                Diagnostic::new("loader.duplicate-module-id")
                    .with_path(format!("lekalo/modules/{}/module.yaml", group[0].directory))
                    .with_data(serde_json::json!({
                        "module": module_id,
                        "directories": directories,
                    })),
            );
        }
    }
    diagnostics
}

/// Declared imports that name no discovered module.
pub fn missing_imports(vertices: &[ModuleVertex]) -> Vec<Diagnostic> {
    let known: std::collections::BTreeSet<&str> = vertices
        .iter()
        .map(|vertex| vertex.module_id.as_str())
        .collect();
    let mut diagnostics = Vec::new();
    for vertex in vertices {
        for import in &vertex.imports {
            if !known.contains(import.as_str()) {
                diagnostics.push(
                    Diagnostic::new("loader.import-missing")
                        .with_path(format!("lekalo/modules/{}/module.yaml", vertex.directory))
                        .with_data(serde_json::json!({
                            "module": vertex.module_id,
                            "import": import,
                        })),
                );
            }
        }
    }
    diagnostics
}

/// Find cycles and report each as the lexicographically normalized closed
/// cycle, e.g. `[alpha,beta,alpha]`.
///
/// Vertices and edges are visited in sorted order so the same graph always
/// produces the same cycle set in the same order.
pub fn import_cycles(vertices: &[ModuleVertex]) -> Vec<Diagnostic> {
    let mut names: Vec<&str> = vertices
        .iter()
        .map(|vertex| vertex.module_id.as_str())
        .collect();
    names.sort_unstable();
    let index_of: BTreeMap<&str, usize> = names
        .iter()
        .enumerate()
        .map(|(index, name)| (*name, index))
        .collect();
    let edges: Vec<Vec<usize>> = names
        .iter()
        .map(|name| {
            let vertex = vertices
                .iter()
                .find(|vertex| vertex.module_id == *name)
                .expect("vertex for sorted name");
            let mut targets: Vec<usize> = vertex
                .imports
                .iter()
                .filter_map(|import| index_of.get(import.as_str()).copied())
                .collect();
            targets.sort_unstable();
            targets.dedup();
            targets
        })
        .collect();

    // Iterative three-color DFS in sorted order.
    const WHITE: u8 = 0;
    const GRAY: u8 = 1;
    const BLACK: u8 = 2;
    let mut colors = vec![WHITE; names.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut found: Vec<Vec<usize>> = Vec::new();
    let mut reported: std::collections::BTreeSet<Vec<usize>> = std::collections::BTreeSet::new();

    for start in 0..names.len() {
        if colors[start] != 0 {
            continue;
        }
        // Explicit DFS stack of (vertex, next-edge-cursor).
        let mut work: Vec<(usize, usize)> = vec![(start, 0)];
        stack.push(start);
        colors[start] = GRAY;
        while let Some(&mut (vertex, ref mut cursor)) = work.last_mut() {
            if *cursor < edges[vertex].len() {
                let next = edges[vertex][*cursor];
                *cursor += 1;
                match colors[next] {
                    0 => {
                        colors[next] = GRAY;
                        stack.push(next);
                        work.push((next, 0));
                    }
                    1 => {
                        // Back edge: extract the closed cycle from the stack.
                        let position = stack
                            .iter()
                            .position(|v| *v == next)
                            .expect("gray on stack");
                        let cycle: Vec<usize> = stack[position..].to_vec();
                        // Normalize rotation so the smallest vertex leads.
                        let (rotate_at, _) = cycle
                            .iter()
                            .enumerate()
                            .min_by_key(|(index, value)| (*value, *index))
                            .expect("cycle is non-empty");
                        let mut normalized: Vec<usize> = cycle[rotate_at..].to_vec();
                        normalized.extend_from_slice(&cycle[..rotate_at]);
                        normalized.push(normalized[0]);
                        if reported.insert(normalized.clone()) {
                            found.push(normalized);
                        }
                    }
                    _ => {}
                }
            } else {
                colors[vertex] = BLACK;
                stack.pop();
                work.pop();
            }
        }
    }

    found.sort();
    let mut diagnostics = Vec::new();
    for cycle in found {
        let labels: Vec<String> = cycle.iter().map(|index| names[*index].to_owned()).collect();
        diagnostics.push(
            Diagnostic::new("loader.import-cycle")
                .with_path(format!("lekalo/modules/{}/module.yaml", {
                    // The cycle lead vertex's declaring document.
                    vertices
                        .iter()
                        .find(|vertex| vertex.module_id == labels[0])
                        .map(|vertex| vertex.directory.as_str())
                        .unwrap_or_default()
                }))
                .with_data(serde_json::json!({ "cycle": labels })),
        );
    }
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(module_id: &str, directory: &str, imports: &[&str]) -> ModuleVertex {
        ModuleVertex {
            module_id: module_id.to_owned(),
            directory: directory.to_owned(),
            imports: imports.iter().map(|import| (*import).to_owned()).collect(),
        }
    }

    #[test]
    fn duplicate_module_ids_across_directories_are_reported_once_per_id() {
        let vertices = vec![
            vertex("planner", "planner", &[]),
            vertex("planner", "planner-v2", &[]),
            vertex("other", "other", &[]),
        ];
        let diagnostics = duplicate_module_ids(&vertices);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "loader.duplicate-module-id");
        assert_eq!(diagnostics[0].data.as_ref().unwrap()["module"], "planner");
    }

    #[test]
    fn missing_imports_name_the_declaring_module() {
        let vertices = vec![
            vertex("alpha", "alpha", &["ghost"]),
            vertex("beta", "beta", &[]),
        ];
        let diagnostics = missing_imports(&vertices);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "loader.import-missing");
        assert_eq!(diagnostics[0].data.as_ref().unwrap()["import"], "ghost");
    }

    #[test]
    fn cycles_report_normalized_closed_forms_deterministically() {
        let vertices = vec![
            vertex("alpha", "alpha", &["beta"]),
            vertex("beta", "beta", &["gamma"]),
            vertex("gamma", "gamma", &["alpha"]),
            vertex("delta", "delta", &[]),
        ];
        let diagnostics = import_cycles(&vertices);
        assert_eq!(diagnostics.len(), 1);
        let cycle: Vec<String> =
            serde_json::from_value(diagnostics[0].data.as_ref().unwrap()["cycle"].clone()).unwrap();
        assert_eq!(cycle, ["alpha", "beta", "gamma", "alpha"]);
    }

    #[test]
    fn self_imports_are_one_node_cycles() {
        let vertices = vec![vertex("solo", "solo", &["solo"])];
        let diagnostics = import_cycles(&vertices);
        assert_eq!(diagnostics.len(), 1);
        let cycle: Vec<String> =
            serde_json::from_value(diagnostics[0].data.as_ref().unwrap()["cycle"].clone()).unwrap();
        assert_eq!(cycle, ["solo", "solo"]);
    }

    #[test]
    fn rotated_cycles_deduplicate_to_one_report() {
        let vertices = vec![vertex("a", "a", &["b"]), vertex("b", "b", &["a"])];
        let diagnostics = import_cycles(&vertices);
        assert_eq!(diagnostics.len(), 1);
        let cycle: Vec<String> =
            serde_json::from_value(diagnostics[0].data.as_ref().unwrap()["cycle"].clone()).unwrap();
        assert_eq!(cycle, ["a", "b", "a"]);
    }
}

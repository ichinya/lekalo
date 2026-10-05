//! Path-independent architectural projection and bounded indexed traversal.
use super::wire::{Cycle, Edge, Partition};
use crate::graph::{DependencyGraph, EdgeProvenance, ReferenceRole};
use crate::ir::{CompiledProject, Definition, Field, Portability, TypeRef, Visibility};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub struct Node {
    pub module: String,
    pub class: &'static str,
    pub target_specific: bool,
    pub effect_operation: String,
}
pub struct Projection {
    pub nodes: BTreeMap<String, Node>,
    pub edges: Vec<Edge>,
    pub incoming: BTreeMap<String, Vec<usize>>,
    pub outgoing: BTreeMap<String, Vec<usize>>,
    pub fields: BTreeMap<String, Vec<(String, Vec<String>)>>,
}
pub fn fields(d: &Definition) -> &[Field] {
    match d {
        Definition::Entity(v) => &v.fields,
        Definition::ValueObject(v) => &v.fields,
        Definition::Command(v) => &v.input,
        Definition::Event(v) => &v.payload,
        _ => &[],
    }
}
fn leaves(t: &TypeRef, into: &mut Vec<String>) {
    match t {
        TypeRef::Ref(v) => into.push(v.as_str().to_owned()),
        TypeRef::List(v) | TypeRef::Optional(v) => leaves(v, into),
    }
}
impl Projection {
    pub fn declared_writers(&self) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
        let mut writers: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
        for command in self.edges.iter().filter(|e| e.role == "command-effect") {
            for entity in self
                .outgoing
                .get(&command.to)
                .into_iter()
                .flatten()
                .map(|i| &self.edges[*i])
                .filter(|e| e.role == "effect-entity")
            {
                let resource = entity
                    .to
                    .split_once(':')
                    .map(|(_, id)| id)
                    .unwrap_or_default();
                let kind = &self.nodes[&command.to].effect_operation;
                let key = format!(
                    "{}|{}|{}|{}|{}",
                    command.from, kind, resource, command.to, command.occurrence
                );
                writers
                    .entry(resource.into())
                    .or_default()
                    .entry(command.from.clone())
                    .or_default()
                    .push(key);
            }
        }
        writers
    }
    pub fn snapshot(&self) -> super::wire::Snapshot {
        super::wire::Snapshot {
            symbols: self
                .nodes
                .iter()
                .map(|(id, n)| {
                    (
                        id.clone(),
                        super::wire::SymbolFact {
                            module: n.module.clone(),
                            class: n.class.into(),
                            target_specific: n.target_specific,
                            effect_operation: n.effect_operation.clone(),
                        },
                    )
                })
                .collect(),
            edges: self.edges.clone(),
            fields: self
                .fields
                .iter()
                .map(|(id, fs)| {
                    (
                        id.clone(),
                        fs.iter()
                            .map(|(name, refs)| {
                                (
                                    name.clone(),
                                    refs.iter()
                                        .cloned()
                                        .collect::<BTreeSet<_>>()
                                        .into_iter()
                                        .collect(),
                                )
                            })
                            .collect(),
                    )
                })
                .collect(),
        }
    }
    pub fn from_snapshot(
        s: &super::wire::Snapshot,
    ) -> Result<Self, crate::diagnostics::DiagnosticSet> {
        let bad = || super::diagnostic::invalid("projection-invariant");
        if s.symbols.len() > 100_000
            || s.edges.len() > 1_000_000
            || s.edges.windows(2).any(|w| w[0].key >= w[1].key)
        {
            return Err(bad());
        }
        let mut nodes = BTreeMap::new();
        for (id, n) in &s.symbols {
            if crate::graph::NodeId::from_qualified(id).is_none()
                || !n.module.is_empty() && !super::evidence::safe_token(&n.module)
            {
                return Err(bad());
            }
            let class = match n.class.as_str() {
                "public" => "public",
                "internal" => "internal",
                "supporting" => "supporting",
                "unclassified" => "unclassified",
                _ => return Err(bad()),
            };
            if !matches!(
                n.effect_operation.as_str(),
                "none" | "create" | "update" | "delete"
            ) || id.starts_with("effect:") == (n.effect_operation == "none")
            {
                return Err(bad());
            }
            nodes.insert(
                id.clone(),
                Node {
                    module: n.module.clone(),
                    class,
                    target_specific: n.target_specific,
                    effect_operation: n.effect_operation.clone(),
                },
            );
        }
        let mut incoming: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut outgoing: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, e) in s.edges.iter().enumerate() {
            if !nodes.contains_key(&e.from)
                || !nodes.contains_key(&e.to)
                || e.key != format!("{}|{}|{}|{}", e.from, e.relation, e.to, e.occurrence)
                || e.confidence != "canonical"
            {
                return Err(bad());
            }
            let relation = match e.role.as_str() {
                "entity-field" | "value-object-field" | "event-payload" | "command-effect"
                | "effect-entity" => "references",
                "command-input" => "accepts",
                "query-returns" => "returns",
                "query-reads" => "reads",
                "endpoint-invokes" => "exposes",
                "effect-emits" => "emits",
                _ => return Err(bad()),
            };
            if e.relation != relation {
                return Err(bad());
            }
            incoming.entry(e.to.clone()).or_default().push(i);
            outgoing.entry(e.from.clone()).or_default().push(i);
        }
        let mut fields = BTreeMap::new();
        let pairs: BTreeSet<_> = s.edges.iter().map(|e| (&e.from, &e.to)).collect();
        for (id, fs) in &s.fields {
            if !nodes.contains_key(id) || fs.len() > 10_000 {
                return Err(bad());
            }
            let mut result = vec![];
            for (name, refs) in fs {
                if !crate::impact::input::is_field_grammar(name)
                    || refs.len() > 1
                    || refs.windows(2).any(|w| w[0] >= w[1])
                    || refs.iter().any(|r| !pairs.contains(&(id, r)))
                {
                    return Err(bad());
                }
                result.push((name.clone(), refs.clone()));
            }
            fields.insert(id.clone(), result);
        }
        Ok(Self {
            nodes,
            edges: s.edges.clone(),
            incoming,
            outgoing,
            fields,
        })
    }
    pub fn new(project: &CompiledProject, graph: &DependencyGraph) -> Self {
        let definitions: BTreeMap<_, _> = project
            .definitions
            .iter()
            .map(|d| (d.id().as_str(), d))
            .collect();
        let mut nodes = BTreeMap::new();
        let mut field_map = BTreeMap::new();
        for n in graph.nodes() {
            let d = definitions.get(n.id().semantic_id());
            let class = match d {
                Some(d)
                    if matches!(
                        d,
                        Definition::Scalar(_)
                            | Definition::Enum(_)
                            | Definition::ValueObject(_)
                            | Definition::Entity(_)
                            | Definition::Command(_)
                            | Definition::Query(_)
                            | Definition::Event(_)
                            | Definition::Endpoint(_)
                    ) =>
                {
                    if d.common().visibility.unwrap_or(Visibility::Project) == Visibility::Project {
                        "public"
                    } else {
                        "internal"
                    }
                }
                Some(_) => "supporting",
                None => "unclassified",
            };
            nodes.insert(
                n.id().as_str().to_owned(),
                Node {
                    module: n.module().unwrap_or_default().to_owned(),
                    class,
                    target_specific: d.is_some_and(|d| {
                        d.common().portability == Some(Portability::TargetSpecific)
                    }),
                    effect_operation: match d {
                        Some(Definition::Effect(e)) => e.operation.as_str().into(),
                        _ => "none".into(),
                    },
                },
            );
            if let Some(d) = d {
                let f = fields(d)
                    .iter()
                    .map(|f| {
                        let mut refs = vec![];
                        leaves(&f.r#type, &mut refs);
                        let refs = refs
                            .iter()
                            .filter_map(|id| graph.resolve_id(id).map(|n| n.as_str().to_owned()))
                            .collect();
                        (f.name.as_str().to_owned(), refs)
                    })
                    .collect::<Vec<_>>();
                if !f.is_empty() {
                    field_map.insert(n.id().as_str().to_owned(), f);
                }
            }
        }
        let mut edges = vec![];
        for e in graph.edges() {
            let EdgeProvenance::CanonicalIr { reference_role, .. } = e.provenance() else {
                continue;
            };
            if !matches!(
                reference_role,
                ReferenceRole::EntityField
                    | ReferenceRole::ValueObjectField
                    | ReferenceRole::EventPayload
                    | ReferenceRole::CommandEffect
                    | ReferenceRole::EffectEntity
                    | ReferenceRole::CommandInput
                    | ReferenceRole::QueryReturns
                    | ReferenceRole::QueryReads
                    | ReferenceRole::EndpointInvokes
                    | ReferenceRole::EffectEmits
            ) {
                continue;
            }
            edges.push(Edge {
                key: e.key().to_canonical_string(),
                from: e.key().from().as_str().into(),
                to: e.key().to().as_str().into(),
                relation: e.key().relation().key().into(),
                occurrence: e.key().occurrence().get(),
                role: reference_role.as_str().into(),
                confidence: e.confidence().as_str().into(),
            });
        }
        edges.sort_by(|a, b| a.key.cmp(&b.key));
        let mut incoming: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut outgoing: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, e) in edges.iter().enumerate() {
            incoming.entry(e.to.clone()).or_default().push(i);
            outgoing.entry(e.from.clone()).or_default().push(i);
        }
        Self {
            nodes,
            edges,
            incoming,
            outgoing,
            fields: field_map,
        }
    }
    pub fn partition(&self, ids: &BTreeSet<String>) -> Partition {
        let mut p = Partition::default();
        for id in ids {
            match self
                .nodes
                .get(id)
                .map(|n| n.class)
                .unwrap_or("unclassified")
            {
                "public" => p.public_contracts.push(id.clone()),
                "internal" => p.internal_symbols.push(id.clone()),
                "supporting" => p.supporting_symbols.push(id.clone()),
                _ => p.unclassified_symbols.push(id.clone()),
            }
        }
        p
    }
    pub fn modules(&self, ids: impl Iterator<Item = impl AsRef<str>>) -> Vec<String> {
        ids.filter_map(|id| self.nodes.get(id.as_ref()))
            .map(|n| &n.module)
            .filter(|m| !m.is_empty())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn walk(&self, root: &str, reverse: bool, work: &mut usize) -> Walk {
        let mut paths = BTreeMap::from([(root.to_owned(), vec![])]);
        let mut queue = VecDeque::from([root.to_owned()]);
        let mut visits = 0;
        let mut complete = true;
        while let Some(node) = queue.pop_front() {
            let adjacent = if reverse {
                self.incoming.get(&node)
            } else {
                self.outgoing.get(&node)
            };
            for &i in adjacent.into_iter().flatten() {
                visits += 1;
                *work += 1;
                if *work > 1_000_000 || visits > 250_000 || paths.len() >= 50_000 {
                    complete = false;
                    break;
                }
                let e = &self.edges[i];
                let next = if reverse { &e.from } else { &e.to };
                if paths.contains_key(next) {
                    continue;
                }
                let mut path = paths[&node].clone();
                path.push(i);
                if path.len() > 256 {
                    complete = false;
                    continue;
                }
                paths.insert(next.clone(), path);
                queue.push_back(next.clone());
            }
            if *work > 1_000_000 || visits > 250_000 || paths.len() >= 50_000 {
                break;
            }
        }
        Walk { paths, complete }
    }
    pub fn cycles(&self) -> Vec<Cycle> {
        let adjacency: BTreeMap<String, Vec<String>> = self
            .nodes
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    self.outgoing
                        .get(id)
                        .into_iter()
                        .flatten()
                        .map(|i| self.edges[*i].to.clone())
                        .collect(),
                )
            })
            .collect();
        let mut result = vec![];
        for members in scc(&adjacency) {
            let set: BTreeSet<_> = members.iter().cloned().collect();
            let modules = self.modules(set.iter());
            if modules.len() > 1 {
                let edge_keys = members
                    .iter()
                    .flat_map(|id| self.outgoing.get(id).into_iter().flatten())
                    .map(|i| &self.edges[*i])
                    .filter(|e| set.contains(&e.to))
                    .map(|e| e.key.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                result.push(Cycle {
                    series: "symbols".into(),
                    members,
                    modules,
                    edge_keys,
                });
            }
        }
        let mut module_adjacency: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut module_edges: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, e) in self.edges.iter().enumerate() {
            let a = &self.nodes[&e.from].module;
            let b = &self.nodes[&e.to].module;
            if a != b && !a.is_empty() && !b.is_empty() {
                module_edges.entry(a).or_default().push(i);
                module_adjacency
                    .entry(a.clone())
                    .or_default()
                    .push(b.clone());
                module_adjacency.entry(b.clone()).or_default();
            }
        }
        for members in scc(&module_adjacency) {
            if members.len() > 1 {
                let set: BTreeSet<_> = members.iter().collect();
                let edge_keys = members
                    .iter()
                    .flat_map(|m| module_edges.get(m.as_str()).into_iter().flatten())
                    .map(|i| &self.edges[*i])
                    .filter(|e| {
                        let a = &self.nodes[&e.from].module;
                        let b = &self.nodes[&e.to].module;
                        a != b && set.contains(a) && set.contains(b)
                    })
                    .map(|e| e.key.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                result.push(Cycle {
                    series: "modules".into(),
                    modules: members.clone(),
                    members,
                    edge_keys,
                });
            }
        }
        result.sort_by(|a, b| (&a.series, &a.members).cmp(&(&b.series, &b.members)));
        result
    }
}
pub struct Walk {
    pub paths: BTreeMap<String, Vec<usize>>,
    pub complete: bool,
}

/// Iterative Kosaraju: a long semantic chain cannot exhaust the Rust stack.
fn scc(adjacency: &BTreeMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut order = vec![];
    for root in adjacency.keys() {
        if seen.contains(root) {
            continue;
        }
        let mut stack = vec![(root.clone(), false)];
        while let Some((node, done)) = stack.pop() {
            if done {
                order.push(node);
                continue;
            }
            if !seen.insert(node.clone()) {
                continue;
            }
            stack.push((node.clone(), true));
            for next in adjacency.get(&node).into_iter().flatten().rev() {
                if !seen.contains(next) {
                    stack.push((next.clone(), false));
                }
            }
        }
    }
    let mut reverse: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (a, bs) in adjacency {
        for b in bs {
            reverse.entry(b.clone()).or_default().push(a.clone());
        }
    }
    seen.clear();
    let mut components = vec![];
    for root in order.into_iter().rev() {
        if seen.contains(&root) {
            continue;
        }
        let mut stack = vec![root];
        let mut component = vec![];
        while let Some(node) = stack.pop() {
            if !seen.insert(node.clone()) {
                continue;
            }
            stack.extend(reverse.get(&node).into_iter().flatten().cloned());
            component.push(node);
        }
        component.sort();
        components.push(component);
    }
    components
}

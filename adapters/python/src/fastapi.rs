//! Bounded, opt-in static FastAPI registration evidence. No Python evaluation.
//! The interpreter resolves only imports, names, constants and documented route
//! calls. Unknown flow/escapes taint objects instead of inventing runtime facts.
use crate::{ids, symbols::Sym};
use lekalo_adapter_sdk::json::{canonical, strict_parse, Value};
use ruff_python_ast::visitor::source_order::{walk_expr, walk_stmt, SourceOrderVisitor};
use ruff_python_ast::{Expr, Operator, PySourceType, Stmt};
use ruff_source_file::LineIndex;
use ruff_text_size::Ranged;
use std::collections::{BTreeMap, BTreeSet};

pub const REVISION: &str = "fastapi-static-v1";
const METHODS: [&str; 8] = [
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE",
];
const MAX_WORK: usize = 100_000;

#[derive(Clone)]
pub struct Source {
    pub path: String,
    pub module: String,
    pub text: String,
    pub digest: String,
    pub symbols: Vec<Sym>,
}
#[derive(Clone, Debug)]
enum Atom {
    Unknown,
    External(String),
    Module(usize),
    Object(usize),
    Function(String),
    Text(String),
    List(usize, Vec<String>),
    Method(usize, String),
}
#[derive(Clone, Debug)]
struct Bound {
    atom: Atom,
    deps: BTreeSet<usize>,
}
impl Bound {
    fn new(atom: Atom, source: usize) -> Self {
        Self {
            atom,
            deps: BTreeSet::from([source]),
        }
    }
    fn unknown(source: usize) -> Self {
        Self::new(Atom::Unknown, source)
    }
}
type Env = BTreeMap<String, Bound>;
type RouteKey = (String, String, String);
type HandlerKey = (String, Option<String>);
#[derive(Clone)]
struct Route {
    handler: String,
    method: String,
    path: String,
    operation: Option<String>,
    source: usize,
    line: u64,
    deps: BTreeSet<usize>,
}
#[derive(Clone)]
struct Include {
    child: usize,
    prefix: String,
    deps: BTreeSet<usize>,
}
#[derive(Clone)]
struct Object {
    native: String,
    source: usize,
    app: bool,
    prefix: String,
    deps: BTreeSet<usize>,
    routes: Vec<Route>,
    includes: Vec<Include>,
    tainted: bool,
}

struct Analysis<'a> {
    sources: &'a [Source],
    modules: BTreeMap<String, Vec<usize>>,
    state: Vec<u8>,
    exports: Vec<Env>,
    objects: Vec<Object>,
    unknown: BTreeSet<(usize, u64, &'static str)>,
    work: usize,
    bounded: bool,
    tainted_all: bool,
    lines: Vec<LineIndex>,
    next_list: usize,
    invalid_lists: BTreeSet<usize>,
}

pub struct Outcome {
    pub uncertainty: Vec<Value>,
    pub truncated: bool,
}

/// Attach facts to the actual handler carrier. Only already accepted native
/// trees/source digests may be referenced by the public protocol.
pub fn apply(sources: &[Source], entries: &mut [Value], policy: &str) -> Outcome {
    let mut a = Analysis {
        sources,
        modules: BTreeMap::new(),
        state: vec![0; sources.len()],
        exports: vec![Env::new(); sources.len()],
        objects: vec![],
        unknown: BTreeSet::new(),
        work: 0,
        bounded: false,
        tainted_all: false,
        lines: sources
            .iter()
            .map(|s| LineIndex::from_source_text(&s.text))
            .collect(),
        next_list: 0,
        invalid_lists: BTreeSet::new(),
    };
    for (i, s) in sources.iter().enumerate() {
        if s.path.ends_with(".py") {
            a.modules.entry(s.module.clone()).or_default().push(i);
        }
    }
    for (i, source) in sources.iter().enumerate() {
        if source.path.ends_with(".py") {
            a.module(i, 0);
        }
    }
    let inventory: BTreeSet<String> = entries
        .iter()
        .filter_map(|e| {
            e.get("evidence")
                .and_then(|e| e.get("native_symbols"))
                .and_then(Value::as_array)
        })
        .flatten()
        .filter_map(|s| s.get("native").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let admitted: BTreeSet<String> = entries
        .iter()
        .filter_map(|e| e.get("path").and_then(Value::as_str).map(str::to_owned))
        .collect();
    let mut resolved = vec![];
    'applications: for (id, object) in a.objects.clone().iter().enumerate().filter(|(_, o)| o.app) {
        let mut routes = vec![];
        if a.expand(id, "", &BTreeSet::new(), &mut vec![], &mut routes)
            .is_err()
        {
            a.note(object.source, 1, "router-graph-unresolved");
            continue;
        }
        for route in routes {
            if resolved.len() >= 2000 {
                a.note(route.source, route.line, "route-inventory-bound");
                a.bounded = true;
                break 'applications;
            }
            let mut deps = route.deps.clone();
            deps.extend(&object.deps);
            deps.insert(object.source);
            if deps.len() > 64
                || !inventory.contains(&object.native)
                || !inventory.contains(&route.handler)
                || deps.iter().any(|i| !admitted.contains(&sources[*i].path))
            {
                a.note(route.source, route.line, "route-custody-bound");
                a.bounded = true;
                continue;
            }
            resolved.push((object.native.clone(), route, deps));
        }
    }
    let mut mounted = BTreeSet::new();
    let mut pending: Vec<usize> = a
        .objects
        .iter()
        .enumerate()
        .filter_map(|(i, o)| o.app.then_some(i))
        .collect();
    while let Some(i) = pending.pop() {
        if mounted.insert(i) {
            pending.extend(a.objects[i].includes.iter().map(|e| e.child));
        }
    }
    for (i, o) in a.objects.clone().iter().enumerate() {
        if !o.app && !mounted.contains(&i) && !o.routes.is_empty() {
            a.note(o.source, 1, "unmounted-router-prefix-unknown");
        }
    }
    // Multiple handlers for one app/method/path have runtime dispatch semantics
    // outside this provider. Repeated identical inclusion is deduplicated.
    let mut handlers: BTreeMap<RouteKey, BTreeSet<HandlerKey>> = BTreeMap::new();
    for (app, r, _) in &resolved {
        handlers
            .entry((app.clone(), r.method.clone(), r.path.clone()))
            .or_default()
            .insert((r.handler.clone(), r.operation.clone()));
    }
    let mut unique: BTreeMap<(String, String, String), (Route, BTreeSet<usize>)> = BTreeMap::new();
    for (app, r, deps) in resolved {
        let key = (app, r.method.clone(), r.path.clone());
        if handlers[&key].len() != 1 {
            a.note(r.source, r.line, "ambiguous-route-handler");
            continue;
        }
        unique
            .entry(key)
            .and_modify(|(old, all)| {
                all.extend(&deps);
                if (r.source, r.line) < (old.source, old.line) {
                    *old = r.clone();
                }
            })
            .or_insert((r, deps));
    }
    let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for ((app, _, _), (r, deps)) in unique {
        if deps.len() > 64 {
            a.note(r.source, r.line, "route-custody-bound");
            a.bounded = true;
            continue;
        }
        let mut fields = vec![
            ("provider".into(), Value::Str("fastapi".into())),
            ("rules_revision".into(), Value::Str(REVISION.into())),
            ("policy_digest".into(), Value::Str(policy.into())),
            ("method".into(), Value::Str(r.method)),
            ("path".into(), Value::Str(r.path)),
            ("application_native".into(), Value::Str(app)),
            ("handler_native".into(), Value::Str(r.handler.clone())),
            (
                "registration_path".into(),
                Value::Str(sources[r.source].path.clone()),
            ),
            ("registration_line".into(), Value::Int(r.line as i64)),
            (
                "sources".into(),
                Value::Array(
                    deps.iter()
                        .map(|i| {
                            Value::Object(vec![
                                ("path".into(), Value::Str(sources[*i].path.clone())),
                                (
                                    "source_digest".into(),
                                    Value::Str(sources[*i].digest.clone()),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("confidence".into(), Value::Str("high".into())),
        ];
        if let Some(op) = r.operation {
            fields.push(("operation_id".into(), Value::Str(op)));
        }
        grouped
            .entry(r.handler)
            .or_default()
            .push(Value::Object(fields));
    }
    let mut semantic_counts = BTreeMap::new();
    for entry in entries.iter() {
        if let Some(detail) = entry
            .get("detail")
            .and_then(Value::as_str)
            .and_then(|s| strict_parse(s).ok())
        {
            if let Some(semantic) = detail.get("s").and_then(Value::as_str) {
                *semantic_counts.entry(semantic.to_owned()).or_insert(0usize) += 1;
            }
        }
    }
    for entry in entries {
        let Some(detail) = entry
            .get("detail")
            .and_then(Value::as_str)
            .and_then(|s| strict_parse(s).ok())
        else {
            continue;
        };
        let Some(native) = detail.get("n").and_then(Value::as_str) else {
            continue;
        };
        let Some(mut routes) = grouped.remove(native) else {
            continue;
        };
        if detail
            .get("s")
            .and_then(Value::as_str)
            .is_none_or(|semantic| semantic_counts.get(semantic) != Some(&1))
        {
            if let Some(i) = sources
                .iter()
                .position(|s| s.symbols.iter().any(|s| s.native == native))
            {
                a.note(i, 1, "ambiguous-handler-carrier");
            }
            continue;
        }
        routes.sort_by_key(route_key);
        routes.dedup_by(|a, b| canonical(a) == canonical(b));
        if routes.len() > 64 {
            a.bounded = true;
            if let Some(i) = sources
                .iter()
                .position(|s| s.symbols.iter().any(|s| s.native == native))
            {
                a.note(i, 1, "handler-route-bound");
            }
            continue;
        }
        set(entry, "kind", Value::Str("endpoint".into()));
        if let Some(evidence) = get_mut(entry, "evidence") {
            let base = evidence.get("signature").cloned().unwrap_or(Value::Null);
            let signature = ids::signature(Value::Array(vec![
                Value::Str(REVISION.into()),
                base,
                Value::Array(routes.clone()),
            ]));
            if let Some(Value::Array(nodes)) = get_mut(evidence, "native_symbols") {
                for node in nodes {
                    if node.get("native").and_then(Value::as_str) == Some(native) {
                        set(node, "signature", Value::Str(signature.clone()));
                    }
                }
            }
            set(evidence, "signature", Value::Str(signature));
            set(evidence, "http_routes", Value::Array(routes));
        }
    }
    for (native, _) in grouped {
        if let Some(i) = sources
            .iter()
            .position(|s| s.symbols.iter().any(|s| s.native == native))
        {
            a.note(i, 1, "handler-carrier-unavailable");
        }
    }
    let uncertainty = a
        .unknown
        .iter()
        .take(4096)
        .map(|(i, line, detail)| {
            Value::Object(vec![
                ("kind".into(), Value::Str("framework-route".into())),
                ("detail".into(), Value::Str((*detail).into())),
                ("path".into(), Value::Str(sources[*i].path.clone())),
                ("line".into(), Value::Int(*line as i64)),
                (
                    "source_digest".into(),
                    Value::Str(sources[*i].digest.clone()),
                ),
            ])
        })
        .collect();
    Outcome {
        uncertainty,
        truncated: a.bounded || a.unknown.len() > 4096,
    }
}

fn route_key(v: &Value) -> (Vec<String>, u64, String) {
    (
        [
            "provider",
            "rules_revision",
            "policy_digest",
            "method",
            "path",
            "application_native",
            "handler_native",
            "operation_id",
            "registration_path",
        ]
        .iter()
        .map(|k| v.get(k).and_then(Value::as_str).unwrap_or("").to_owned())
        .collect(),
        v.get("registration_line")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        canonical(v),
    )
}
fn get_mut<'a>(v: &'a mut Value, key: &str) -> Option<&'a mut Value> {
    if let Value::Object(rows) = v {
        rows.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v)
    } else {
        None
    }
}
fn set(v: &mut Value, key: &str, value: Value) {
    if let Value::Object(rows) = v {
        if let Some((_, old)) = rows.iter_mut().find(|(k, _)| k == key) {
            *old = value;
        } else {
            rows.push((key.into(), value));
        }
    }
}
fn printable(s: &str, n: usize) -> bool {
    s.chars().count() <= n && !s.chars().any(char::is_control)
}
fn valid_prefix(s: &str) -> bool {
    s.is_empty() || (s.starts_with('/') && !s.ends_with('/') && printable(s, 2048))
}
fn route_method(s: &str) -> bool {
    METHODS.iter().any(|m| m.to_ascii_lowercase() == s)
        || matches!(s, "api_route" | "add_api_route" | "include_router")
}

impl Analysis<'_> {
    fn note(&mut self, i: usize, line: u64, detail: &'static str) {
        if self.unknown.len() < 4097 {
            self.unknown.insert((i, line, detail));
        } else {
            self.bounded = true;
        }
    }
    fn tick(&mut self, i: usize, depth: usize) -> bool {
        self.work += 1;
        if self.work > MAX_WORK || depth > 32 {
            self.bounded = true;
            self.note(i, 1, "analysis-bound");
            false
        } else {
            true
        }
    }
    fn line(&self, i: usize, e: impl Ranged) -> u64 {
        self.lines[i].line_index(e.range().start()).get() as u64
    }
    fn symbol(&self, i: usize, name: &str, line: u64) -> Option<String> {
        self.sources[i]
            .symbols
            .iter()
            .find(|s| {
                s.qualified_name == format!("{}.{name}", self.sources[i].module)
                    && s.line == line
                    && !s.declaration_only
            })
            .map(|s| s.native.clone())
    }
    fn module(&mut self, i: usize, depth: usize) -> Env {
        if !self.tick(i, depth) {
            return Env::new();
        }
        if self.state[i] == 2 {
            return self.exports[i].clone();
        }
        if self.state[i] == 1 {
            self.note(i, 1, "import-cycle");
            return Env::new();
        }
        self.state[i] = 1;
        let parsed =
            ruff_python_parser::parse_unchecked_source(&self.sources[i].text, PySourceType::Python);
        if !parsed.errors().is_empty() {
            self.note(i, 1, "framework-parse-error");
            self.state[i] = 2;
            return Env::new();
        }
        let mut env = Env::new();
        for stmt in &parsed.syntax().body {
            self.statement(i, stmt, &mut env, depth + 1);
        }
        self.state[i] = 2;
        self.exports[i] = env.clone();
        env
    }
    fn import(&mut self, i: usize, name: &str, depth: usize) -> Bound {
        if let Some(indices) = self.modules.get(name).cloned() {
            if indices.len() != 1 {
                self.note(i, 1, "ambiguous-module");
                return Bound::unknown(i);
            }
            let j = indices[0];
            self.module(j, depth + 1);
            return Bound {
                atom: Atom::Module(j),
                deps: BTreeSet::from([i, j]),
            };
        }
        if name == "fastapi" || name.starts_with("fastapi.") {
            if self
                .modules
                .keys()
                .any(|m| m == "fastapi" || m.starts_with("fastapi."))
            {
                self.note(i, 1, "local-fastapi-shadow");
                return Bound::unknown(i);
            }
            return Bound::new(Atom::External(name.into()), i);
        }
        Bound::unknown(i)
    }
    fn attribute(&mut self, i: usize, base: Bound, name: &str, depth: usize) -> Bound {
        let mut value = match base.atom {
            Atom::External(path) => Bound::new(Atom::External(format!("{path}.{name}")), i),
            Atom::Module(j) => self.exports[j].get(name).cloned().unwrap_or_else(|| {
                let name = format!("{}.{name}", self.sources[j].module);
                self.import(i, &name, depth + 1)
            }),
            Atom::Object(id) => Bound::new(Atom::Method(id, name.into()), i),
            _ => Bound::unknown(i),
        };
        value.deps.extend(base.deps);
        value
    }
    fn eval(&mut self, i: usize, e: &Expr, env: &Env, depth: usize) -> Bound {
        if !self.tick(i, depth) {
            return Bound::unknown(i);
        }
        let mut value = match e {
            Expr::Name(n) => env
                .get(n.id.as_str())
                .cloned()
                .unwrap_or_else(|| Bound::unknown(i)),
            Expr::Attribute(a) => {
                let base = self.eval(i, &a.value, env, depth + 1);
                self.attribute(i, base, a.attr.as_str(), depth + 1)
            }
            Expr::StringLiteral(s) => Bound::new(Atom::Text(s.value.to_str().into()), i),
            Expr::List(l) => self.list(i, &l.elts, env, depth),
            Expr::Tuple(l) => self.list(i, &l.elts, env, depth),
            Expr::BinOp(b) if b.op == Operator::Add => {
                let mut left = self.eval(i, &b.left, env, depth + 1);
                let right = self.eval(i, &b.right, env, depth + 1);
                left.deps.extend(right.deps);
                left.atom = match (left.atom, right.atom) {
                    (Atom::Text(a), Atom::Text(b)) if a.len() + b.len() <= 8192 => {
                        Atom::Text(a + &b)
                    }
                    _ => Atom::Unknown,
                };
                left
            }
            // Calls in expression positions can leak framework objects. Known
            // constructor calls are handled only at a named assignment below.
            _ => Bound::unknown(i),
        };
        if matches!(&value.atom, Atom::List(id, _) if self.invalid_lists.contains(id)) {
            value.atom = Atom::Unknown;
        }
        value
    }
    fn list(&mut self, i: usize, values: &[Expr], env: &Env, depth: usize) -> Bound {
        if values.len() > 8 {
            return Bound::unknown(i);
        }
        let mut deps = BTreeSet::from([i]);
        let mut out = vec![];
        for e in values {
            let v = self.eval(i, e, env, depth + 1);
            deps.extend(v.deps);
            if let Atom::Text(s) = v.atom {
                out.push(s);
            } else {
                return Bound::unknown(i);
            }
        }
        let identity = self.next_list;
        self.next_list += 1;
        Bound {
            atom: Atom::List(identity, out),
            deps,
        }
    }
    fn statement(&mut self, i: usize, s: &Stmt, env: &mut Env, depth: usize) {
        if !self.tick(i, depth) {
            return;
        }
        match s {
            Stmt::Import(row) => {
                for alias in &row.names {
                    let full = alias.name.as_str();
                    let name = alias
                        .asname
                        .as_ref()
                        .map(|s| s.as_str())
                        .unwrap_or_else(|| full.split('.').next().unwrap_or(full));
                    let imported = if alias.asname.is_some() {
                        full
                    } else {
                        full.split('.').next().unwrap_or(full)
                    };
                    let v = self.import(i, imported, depth + 1);
                    env.insert(name.into(), v);
                }
            }
            Stmt::ImportFrom(row) => {
                let name = if row.level == 0 {
                    row.module.as_ref().map(|m| m.to_string())
                } else {
                    let mut parts: Vec<&str> = self.sources[i].module.split('.').collect();
                    let count = if self.sources[i].path.ends_with("/__init__.py") {
                        row.level.saturating_sub(1)
                    } else {
                        row.level
                    };
                    for _ in 0..count {
                        parts.pop();
                    }
                    row.module
                        .as_ref()
                        .map(|m| {
                            parts.extend(m.as_str().split('.'));
                            parts.join(".")
                        })
                        .or_else(|| Some(parts.join(".")))
                };
                if let Some(module) = name {
                    let base = self.import(i, &module, depth + 1);
                    for alias in &row.names {
                        if alias.name.as_str() == "*" {
                            self.note(i, self.line(i, s), "star-import");
                            self.taint_all();
                            env.clear();
                            continue;
                        }
                        let v = self.attribute(i, base.clone(), alias.name.as_str(), depth + 1);
                        env.insert(alias.asname.as_ref().unwrap_or(&alias.name).to_string(), v);
                    }
                }
            }
            Stmt::Assign(row) => {
                let name = if row.targets.len() == 1 {
                    if let Expr::Name(n) = &row.targets[0] {
                        Some(n.id.as_str())
                    } else {
                        None
                    }
                } else {
                    None
                };
                let v = self.assignment(i, name, &row.value, env, self.line(i, s), depth + 1);
                for target in &row.targets {
                    if let Expr::Name(n) = target {
                        env.insert(n.id.to_string(), v.clone());
                    } else {
                        self.unsupported(i, s, env);
                    }
                }
            }
            Stmt::AnnAssign(row) => {
                if let Expr::Name(n) = row.target.as_ref() {
                    let v = row
                        .value
                        .as_ref()
                        .map(|e| {
                            self.assignment(
                                i,
                                Some(n.id.as_str()),
                                e,
                                env,
                                self.line(i, s),
                                depth + 1,
                            )
                        })
                        .unwrap_or_else(|| Bound::unknown(i));
                    env.insert(n.id.to_string(), v);
                } else {
                    self.unsupported(i, s, env);
                }
            }
            Stmt::FunctionDef(row) => {
                let line = self.line(i, s);
                let mut immediate = ImmediateCalls::default();
                immediate.function_header(row, false);
                self.immediate_calls(i, immediate, env, depth + 1);
                let native = self.symbol(i, row.name.as_str(), line);
                let handler = native
                    .clone()
                    .map(|n| Bound::new(Atom::Function(n), i))
                    .unwrap_or_else(|| Bound::unknown(i));
                let mut registrations = vec![];
                let mut safe = true;
                for decorator in &row.decorator_list {
                    if let Expr::Call(call) = &decorator.expression {
                        // A route decorator's arguments execute before it is
                        // applied to the handler; nested calls may mutate state.
                        let mut immediate = ImmediateCalls::default();
                        immediate.visit_expr(&call.func);
                        immediate.visit_arguments(&call.arguments);
                        self.immediate_calls(i, immediate, env, depth + 1);
                        let target = self.eval(i, &call.func, env, depth + 1);
                        if let Atom::Method(object, method) = &target.atom {
                            if route_method(method)
                                && method != "include_router"
                                && method != "add_api_route"
                            {
                                registrations.push((
                                    *object,
                                    method.clone(),
                                    call,
                                    target.deps.clone(),
                                ));
                                continue;
                            }
                        }
                        if let Expr::Attribute(attr) = call.func.as_ref() {
                            if route_method(attr.attr.as_str()) {
                                self.note(i, line, "registration-target-unresolved");
                            }
                        }
                    }
                    safe = false;
                }
                if safe {
                    for (object, method, call, deps) in registrations {
                        self.register(
                            i,
                            object,
                            &method,
                            call,
                            Some(handler.clone()),
                            deps,
                            env,
                            depth + 1,
                        );
                    }
                } else {
                    self.note(i, line, "decorated-handler-unresolved");
                    self.taint_all();
                }
                // Registration inside a function/factory is conditional runtime
                // behavior, including mutation of an outer app/router.
                let mut probe = Probe::default();
                for stmt in &row.body {
                    probe.visit_stmt(stmt);
                }
                if probe.route {
                    self.note(i, line, "runtime-registration");
                    self.taint_all();
                }
                env.insert(
                    row.name.to_string(),
                    if safe { handler } else { Bound::unknown(i) },
                );
            }
            Stmt::ClassDef(row) => {
                let mut immediate = ImmediateCalls::default();
                immediate.visit_stmt(s);
                self.immediate_calls(i, immediate, env, depth + 1);
                env.insert(row.name.to_string(), Bound::unknown(i));
                let mut probe = Probe::default();
                for stmt in &row.body {
                    probe.visit_stmt(stmt);
                }
                if probe.route {
                    self.note(i, self.line(i, s), "nested-registration");
                    self.taint_all();
                }
            }
            Stmt::Expr(row) => {
                if let Expr::Call(call) = row.value.as_ref() {
                    self.call(i, call, env, depth + 1);
                }
            }
            Stmt::Pass(_) => {}
            _ => self.unsupported(i, s, env),
        }
    }
    fn assignment(
        &mut self,
        i: usize,
        name: Option<&str>,
        e: &Expr,
        env: &Env,
        line: u64,
        depth: usize,
    ) -> Bound {
        if let Expr::Call(call) = e {
            let constructor = self.eval(i, &call.func, env, depth + 1);
            if let Atom::External(ref class) = constructor.atom {
                let app = matches!(
                    class.as_str(),
                    "fastapi.FastAPI" | "fastapi.applications.FastAPI"
                );
                let router = matches!(
                    class.as_str(),
                    "fastapi.APIRouter" | "fastapi.routing.APIRouter"
                );
                if app || router {
                    let native = name.and_then(|n| self.symbol(i, n, line));
                    let mut deps = constructor.deps;
                    deps.insert(i);
                    let mut prefix = String::new();
                    let mut safe = call.arguments.args.is_empty();
                    for keyword in &call.arguments.keywords {
                        match keyword.arg.as_ref().map(|a| a.as_str()) {
                            Some("prefix") if router => {
                                let v = self.eval(i, &keyword.value, env, depth + 1);
                                deps.extend(v.deps);
                                if let Atom::Text(s) = v.atom {
                                    if valid_prefix(&s) {
                                        prefix = s;
                                    } else {
                                        safe = false;
                                    }
                                } else {
                                    safe = false;
                                }
                            }
                            Some("routes" | "route_class") => safe = false,
                            None => safe = false,
                            _ => {}
                        }
                    }
                    if safe && self.objects.len() < 1024 {
                        if let Some(native) = native {
                            let id = self.objects.len();
                            self.objects.push(Object {
                                native,
                                source: i,
                                app,
                                prefix,
                                deps: deps.clone(),
                                routes: vec![],
                                includes: vec![],
                                tainted: self.tainted_all,
                            });
                            return Bound {
                                atom: Atom::Object(id),
                                deps,
                            };
                        }
                    }
                    self.note(i, line, "constructor-unresolved");
                    return Bound::unknown(i);
                }
            }
            self.call(i, call, env, depth + 1);
        }
        let value = self.eval(i, e, env, depth + 1);
        if matches!(value.atom, Atom::Unknown) {
            let mut probe = Probe::default();
            probe.visit_expr(e);
            self.escape_names(i, line, &probe.names, env);
        }
        value
    }
    fn call(&mut self, i: usize, call: &ruff_python_ast::ExprCall, env: &Env, depth: usize) {
        let bound = self.eval(i, &call.func, env, depth + 1);
        if let Atom::Method(id, method) = &bound.atom {
            if route_method(method) {
                self.register(i, *id, method, call, None, bound.deps, env, depth + 1);
                return;
            }
            // Unknown calls on the framework object can mutate the route graph.
            self.objects[*id].tainted = true;
            self.note(i, self.line(i, call), "object-method-unresolved");
            return;
        }
        let mut probe = Probe::default();
        probe.visit_expr(&call.func);
        for arg in &call.arguments.args {
            probe.visit_expr(arg);
        }
        for kw in &call.arguments.keywords {
            probe.visit_expr(&kw.value);
        }
        self.escape_names(i, self.line(i, call), &probe.names, env);
        if let Expr::Attribute(attr) = call.func.as_ref() {
            if route_method(attr.attr.as_str()) {
                self.note(i, self.line(i, call), "registration-target-unresolved");
            }
        }
    }
    // AST node, lexical environment and bounded recursion remain explicit.
    #[allow(clippy::too_many_arguments)]
    fn register(
        &mut self,
        i: usize,
        id: usize,
        method: &str,
        call: &ruff_python_ast::ExprCall,
        handler: Option<Bound>,
        mut deps: BTreeSet<usize>,
        env: &Env,
        depth: usize,
    ) {
        let line = self.line(i, call);
        let include = method == "include_router";
        let mut values = BTreeMap::new();
        let positional = if include {
            vec!["router"]
        } else if method == "add_api_route" {
            vec!["path", "endpoint"]
        } else {
            vec!["path"]
        };
        let mut safe = call.arguments.args.len() <= positional.len();
        for (name, arg) in positional.into_iter().zip(&call.arguments.args) {
            values.insert(name.to_owned(), self.eval(i, arg, env, depth + 1));
        }
        for kw in &call.arguments.keywords {
            if let Some(name) = &kw.arg {
                let key = name.to_string();
                let value = self.eval(i, &kw.value, env, depth + 1);
                if values.insert(key, value).is_some() {
                    safe = false;
                }
            } else {
                safe = false;
            }
        }
        for v in values.values() {
            deps.extend(&v.deps);
        }
        deps.insert(i);
        const ROUTE_OPTIONS: &[&str] = &[
            "path",
            "response_model",
            "status_code",
            "tags",
            "dependencies",
            "summary",
            "description",
            "response_description",
            "responses",
            "deprecated",
            "operation_id",
            "response_model_include",
            "response_model_exclude",
            "response_model_by_alias",
            "response_model_exclude_unset",
            "response_model_exclude_defaults",
            "response_model_exclude_none",
            "include_in_schema",
            "response_class",
            "name",
            "callbacks",
            "openapi_extra",
            "generate_unique_id_function",
        ];
        const INCLUDE_OPTIONS: &[&str] = &[
            "router",
            "prefix",
            "tags",
            "dependencies",
            "default_response_class",
            "responses",
            "callbacks",
            "deprecated",
            "include_in_schema",
            "generate_unique_id_function",
        ];
        if values.keys().any(|key| {
            if include {
                !INCLUDE_OPTIONS.contains(&key.as_str())
            } else {
                !ROUTE_OPTIONS.contains(&key.as_str())
                    && !(key == "methods" && matches!(method, "api_route" | "add_api_route"))
                    && !(key == "endpoint" && method == "add_api_route")
                    && !(key == "route_class_override" && method == "add_api_route")
            }
        }) {
            safe = false;
        }
        // Custom route classes/decorators or ID generators are not evidence of
        // the standard registration contract. Other response/DI metadata is
        // deliberately outside this static route inventory.
        if [
            "route_class_override",
            "route_class",
            "generate_unique_id_function",
            "callbacks",
        ]
        .iter()
        .any(|k| values.contains_key(*k))
        {
            safe = false;
        }
        if include {
            let prefix = match values.get("prefix").map(|b| &b.atom) {
                None => Some(String::new()),
                Some(Atom::Text(s)) if valid_prefix(s) => Some(s.clone()),
                _ => None,
            };
            let child = match values.get("router").map(|b| &b.atom) {
                Some(Atom::Object(child)) if !self.objects[*child].app => Some(*child),
                _ => None,
            };
            if safe {
                if let (Some(child), Some(prefix)) = (child, prefix) {
                    if self.objects[id].includes.len() < 2000 {
                        self.objects[id].includes.push(Include {
                            child,
                            prefix,
                            deps,
                        });
                        return;
                    }
                }
            }
            self.objects[id].tainted = true;
            self.note(i, line, "include-router-unresolved");
            return;
        }
        let path = match values.get("path").map(|b| &b.atom) {
            Some(Atom::Text(path)) if path.starts_with('/') && printable(path, 2048) => {
                Some(path.clone())
            }
            _ => None,
        };
        let handler = handler.or_else(|| values.get("endpoint").cloned());
        let native = handler.and_then(|h| {
            deps.extend(h.deps);
            if let Atom::Function(native) = h.atom {
                Some(native)
            } else {
                None
            }
        });
        let methods = if matches!(method, "api_route" | "add_api_route") {
            match values.get("methods").map(|b| &b.atom) {
                None => Some(vec!["GET".to_owned()]),
                Some(Atom::List(_, ms))
                    if !ms.is_empty()
                        && ms
                            .iter()
                            .all(|m| METHODS.contains(&m.to_ascii_uppercase().as_str())) =>
                {
                    Some(ms.iter().map(|m| m.to_ascii_uppercase()).collect())
                }
                _ => None,
            }
        } else {
            Some(vec![method.to_ascii_uppercase()])
        };
        let operation = match values.get("operation_id").map(|b| &b.atom) {
            None => Some(None),
            Some(Atom::Text(s)) if !s.is_empty() && printable(s, 256) => Some(Some(s.clone())),
            _ => None,
        };
        if safe {
            if let (Some(path), Some(handler), Some(methods), Some(operation)) =
                (path, native, methods, operation)
            {
                if self.objects.iter().map(|o| o.routes.len()).sum::<usize>() + methods.len()
                    <= 2000
                {
                    for method in methods {
                        self.objects[id].routes.push(Route {
                            handler: handler.clone(),
                            method,
                            path: path.clone(),
                            operation: operation.clone(),
                            source: i,
                            line,
                            deps: deps.clone(),
                        });
                    }
                    return;
                }
                self.bounded = true;
            }
        }
        self.note(i, line, "route-registration-unresolved");
    }
    fn escape_names(&mut self, i: usize, line: u64, names: &BTreeSet<String>, env: &Env) {
        let mut pending: Vec<Atom> = names
            .iter()
            .filter_map(|name| env.get(name).map(|b| b.atom.clone()))
            .collect();
        let mut modules = BTreeSet::new();
        while let Some(atom) = pending.pop() {
            if !self.tick(i, 0) {
                self.taint_all();
                return;
            }
            match atom {
                Atom::Object(id) | Atom::Method(id, _) => {
                    self.objects[id].tainted = true;
                    self.note(i, line, "framework-object-escaped");
                }
                Atom::Module(id) if modules.insert(id) => {
                    pending.extend(self.exports[id].values().map(|b| b.atom.clone()));
                }
                Atom::List(id, _) => {
                    self.invalid_lists.insert(id);
                    self.note(i, line, "method-list-mutation-or-escape");
                }
                _ => {}
            }
        }
    }
    fn immediate_calls(&mut self, i: usize, calls: ImmediateCalls<'_>, env: &Env, depth: usize) {
        self.work = self.work.saturating_add(calls.visited);
        if calls.truncated || !self.tick(i, depth) {
            self.bounded = true;
            self.taint_all();
            return;
        }
        for call in calls.calls {
            self.call(i, call, env, depth + 1);
        }
    }
    fn taint_all(&mut self) {
        self.tainted_all = true;
        for o in &mut self.objects {
            o.tainted = true;
        }
    }
    fn unsupported(&mut self, i: usize, s: &Stmt, env: &mut Env) {
        let mut probe = Probe::default();
        probe.visit_stmt(s);
        self.escape_names(i, self.line(i, s), &probe.names, env);
        let relevant = probe.route
            || probe.names.iter().any(|n| {
                matches!(
                    env.get(n).map(|b| &b.atom),
                    Some(Atom::Object(_) | Atom::External(_))
                )
            });
        if relevant {
            self.taint_all();
            self.note(i, self.line(i, s), "control-flow-or-mutation");
        }
        // Any name mentioned by unsupported statement may have been rebound.
        for name in probe.names {
            if env.contains_key(&name) {
                env.insert(name, Bound::unknown(i));
            }
        }
    }
    fn expand(
        &mut self,
        id: usize,
        prefix: &str,
        deps: &BTreeSet<usize>,
        stack: &mut Vec<usize>,
        out: &mut Vec<Route>,
    ) -> Result<(), ()> {
        let o = self.objects[id].clone();
        if !self.tick(o.source, stack.len()) {
            return Err(());
        }
        if o.tainted || stack.contains(&id) || stack.len() >= 32 || out.len() > 2000 {
            return Err(());
        }
        let prefix = format!("{prefix}{}", o.prefix);
        if prefix.len() > 2048 {
            return Err(());
        }
        let mut deps = deps.clone();
        deps.extend(&o.deps);
        deps.insert(o.source);
        stack.push(id);
        for mut route in o.routes {
            route.path = format!("{prefix}{}", route.path);
            route.deps.extend(&deps);
            if route.path.chars().count() > 2048 {
                return Err(());
            }
            out.push(route);
            if out.len() > 2000 {
                self.bounded = true;
                return Err(());
            }
        }
        for edge in o.includes {
            let mut deps = deps.clone();
            deps.extend(edge.deps);
            self.expand(
                edge.child,
                &format!("{prefix}{}", edge.prefix),
                &deps,
                stack,
                out,
            )?;
        }
        stack.pop();
        Ok(())
    }
}

/// Definition-time calls only: function/lambda bodies remain unevaluated.
#[derive(Default)]
struct ImmediateCalls<'a> {
    calls: Vec<&'a ruff_python_ast::ExprCall>,
    visited: usize,
    truncated: bool,
}
impl<'a> ImmediateCalls<'a> {
    fn function_header(&mut self, row: &'a ruff_python_ast::StmtFunctionDef, decorators: bool) {
        if decorators {
            for decorator in &row.decorator_list {
                self.visit_expr(&decorator.expression);
            }
        }
        self.visit_parameters(&row.parameters);
        if let Some(annotation) = &row.returns {
            self.visit_annotation(annotation);
        }
    }
}
impl<'a> SourceOrderVisitor<'a> for ImmediateCalls<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if self.visited >= MAX_WORK {
            self.truncated = true;
            return;
        }
        self.visited += 1;
        if let Expr::Lambda(lambda) = expr {
            if let Some(parameters) = &lambda.parameters {
                self.visit_parameters(parameters);
            }
            return;
        }
        walk_expr(self, expr);
        if let Expr::Call(call) = expr {
            self.calls.push(call);
        }
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(row) => self.function_header(row, true),
            _ => walk_stmt(self, stmt),
        }
    }
}

#[derive(Default)]
struct Probe {
    names: BTreeSet<String>,
    route: bool,
}
impl<'a> SourceOrderVisitor<'a> for Probe {
    fn visit_expr(&mut self, e: &'a Expr) {
        match e {
            Expr::Name(n) => {
                self.names.insert(n.id.to_string());
            }
            Expr::Attribute(a) if route_method(a.attr.as_str()) => self.route = true,
            _ => {}
        }
        walk_expr(self, e);
    }
    fn visit_stmt(&mut self, s: &'a Stmt) {
        walk_stmt(self, s);
    }
}

//! Static Python declaration extraction (issue #146): `ruff_python_parser`
//! AST → closed native-symbol forest, import/call reference edges, and
//! `any-surface` uncertainty rows. Nothing executes: no imports, no
//! interpreter, no framework introspection.

use std::collections::{BTreeMap, BTreeSet};

use ruff_python_ast::visitor::source_order::{walk_expr, SourceOrderVisitor};
use ruff_python_ast::{AnyParameterRef, Expr, PySourceType, Stmt};
use ruff_source_file::LineIndex;
use ruff_text_size::{Ranged, TextRange};

use crate::ids;

#[derive(Clone, Debug)]
pub struct Sym {
    pub native: String,
    pub qualified_name: String,
    pub family: String,
    pub path: String,
    pub line: u64,
    pub end_line: u64,
    pub slot: &'static str,
    pub declaration_only: bool,
    pub member_of: Option<String>,
    pub signature: Option<String>,
}

impl Sym {
    pub fn value(&self) -> lekalo_adapter_sdk::json::Value {
        use lekalo_adapter_sdk::json::Value;
        let mut members = vec![
            ("declaration_only".to_owned(), Value::Bool(self.declaration_only)),
            ("end_line".to_owned(), Value::Int(self.end_line as i64)),
            ("family".to_owned(), Value::Str(self.family.clone())),
            ("line".to_owned(), Value::Int(self.line as i64)),
        ];
        if let Some(owner) = &self.member_of {
            members.push(("member_of".to_owned(), Value::Str(owner.clone())));
        }
        members.extend([
            ("native".to_owned(), Value::Str(self.native.clone())),
            ("path".to_owned(), Value::Str(self.path.clone())),
            (
                "qualified_name".to_owned(),
                Value::Str(self.qualified_name.clone()),
            ),
        ]);
        if let Some(signature) = &self.signature {
            members.push(("signature".to_owned(), Value::Str(signature.clone())));
        }
        members.push(("slot".to_owned(), Value::Str(self.slot.to_owned())));
        Value::Object(members)
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RefRow {
    pub target: String,
    pub role: &'static str,
    pub confidence: &'static str,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct UncRow {
    pub path: String,
    pub detail: String,
    pub line: Option<u64>,
    pub source_digest: String,
}

pub struct FileScan {
    pub root_native: String,
    pub symbols: Vec<Sym>,
    pub references: BTreeSet<RefRow>,
    pub uncertainty: Vec<UncRow>,
}

/// Extract the closed declaration forest of one source file. `module`
/// is the importable dotted name relative to the file's read root —
/// qualified names and semantic proposals are module-relative, so they
/// stay stable when the same tree is scanned under a different root.
/// Parse recovery degrades to partial symbols plus an `any-surface`
/// row — never silently dropped evidence and never a fabricated
/// complete scan.
pub fn extract(
    path: &str,
    module: &str,
    source: &str,
    declaration_only: bool,
    digest: &str,
) -> FileScan {
    let line_index = LineIndex::from_source_text(source);
    let source_type = if declaration_only {
        PySourceType::Stub
    } else {
        PySourceType::Python
    };
    let parsed = ruff_python_parser::parse_unchecked_source(source, source_type);

    let mut uncertainty: Vec<UncRow> = Vec::new();
    for error in parsed.errors().iter().take(8) {
        uncertainty.push(UncRow {
            path: path.to_owned(),
            detail: "parse-error".to_owned(),
            line: Some(line(&line_index, error.location)),
            source_digest: digest.to_owned(),
        });
    }

    let module_name = module.to_owned();
    let root = Sym {
        native: ids::native_id(path, &module_name, "module", "static", 0),
        qualified_name: module_name.clone(),
        family: "module".to_owned(),
        path: path.to_owned(),
        line: 1,
        end_line: line_index.line_count().max(1) as u64,
        slot: "static",
        declaration_only,
        member_of: None,
        signature: Some(ids::signature(lekalo_adapter_sdk::json::Value::Array(
            vec![
                lekalo_adapter_sdk::json::Value::Str(ids::SIGNATURE_DOMAIN.to_owned()),
                lekalo_adapter_sdk::json::Value::Str("module".to_owned()),
                lekalo_adapter_sdk::json::Value::Str(module_name.clone()),
            ],
        ))),
    };
    let is_init = path
        .rsplit('/')
        .next()
        .is_some_and(|base| base == "__init__.py" || base == "__init__.pyi");
    let mut ctx = Ctx {
        path,
        source,
        line_index: &line_index,
        declaration_only,
        digest,
        module,
        is_init,
        symbols: Vec::with_capacity(32),
        references: BTreeSet::new(),
        uncertainty: Vec::new(),
        occurrences: BTreeMap::new(),
    };
    walk_block(
        &parsed.syntax().body,
        &root,
        &module_name,
        ScopeKind::Module,
        0,
        &mut ctx,
    );

    // Dynamic constructs are honest evidence: eval/exec, importlib,
    // __import__, monkey-patch-shaped attribute writes, wildcard imports.
    let mut dynamic = DynamicScan {
        hits: Vec::new(),
        calls: Vec::new(),
    };
    for stmt in parsed.syntax().body.iter() {
        ruff_python_ast::visitor::source_order::walk_stmt(&mut dynamic, stmt);
    }
    for name in dynamic.calls {
        if lekalo_adapter_sdk::paths::is_semantic_id(&name) {
            ctx.references.insert(RefRow {
                target: name,
                role: "call",
                confidence: "low",
            });
        }
    }
    for (range, detail) in dynamic.hits {
        ctx.uncertainty.push(UncRow {
            path: path.to_owned(),
            detail: detail.to_owned(),
            line: Some(line(ctx.line_index, range)),
            source_digest: digest.to_owned(),
        });
    }

    let mut symbols = vec![root.clone()];
    symbols.extend(ctx.symbols);
    FileScan {
        root_native: root.native.clone(),
        symbols,
        references: ctx.references,
        uncertainty: {
            uncertainty.extend(ctx.uncertainty);
            uncertainty
        },
    }
}

#[derive(Clone, Copy)]
enum ScopeKind {
    Module,
    Class,
    Function,
}

struct Ctx<'a> {
    path: &'a str,
    source: &'a str,
    line_index: &'a LineIndex,
    declaration_only: bool,
    digest: &'a str,
    /// The file's root-relative importable module name — the base for
    /// `from .`/`from ..` resolution regardless of the lexical owner.
    module: &'a str,
    /// Whether the file is a package marker (`__init__.py[i]`): its
    /// module name is the package itself, so `from .` resolves inside
    /// it rather than its parent.
    is_init: bool,
    symbols: Vec<Sym>,
    references: BTreeSet<RefRow>,
    uncertainty: Vec<UncRow>,
    /// Occurrence ordinals of each `(qualified_name, family)` spelling.
    /// The owner deliberately stays out of the key: two same-named
    /// owners produce same-named members, and only this ordinal keeps
    /// their native identities distinct.
    occurrences: BTreeMap<(String, String), u64>,
}

fn line(index: &LineIndex, range: TextRange) -> u64 {
    index.line_index(range.start()).get() as u64
}

fn end_line(index: &LineIndex, range: TextRange) -> u64 {
    index.line_index(range.end()).get() as u64
}

fn slice<'a>(source: &'a str, range: TextRange) -> &'a str {
    &source[range.start().to_usize()..range.end().to_usize()]
}

/// Dotted spelling of a Name/Attribute/Call expression, if statically flat.
fn dotted(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str().to_owned()),
        Expr::Attribute(attribute) => {
            let base = dotted(&attribute.value)?;
            Some(format!("{base}.{}", attribute.attr.as_str()))
        }
        Expr::Call(call) => dotted(&call.func),
        _ => None,
    }
}

fn last_segment(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str().to_owned()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str().to_owned()),
        Expr::Call(call) => last_segment(&call.func),
        _ => None,
    }
}

/// Dotted spellings of decorator expressions. The full spelling — not
/// the last segment — is what distinguishes `@x.setter`/`@x.deleter`
/// from a plain `@setter` name and keeps the signature honest.
fn decorator_names(list: &[ruff_python_ast::Decorator]) -> Vec<String> {
    list.iter()
        .filter_map(|decorator| dotted(&decorator.expression))
        .collect()
}

fn walk_block(
    body: &[Stmt],
    owner: &Sym,
    owner_qname: &str,
    scope: ScopeKind,
    conditional_depth: u32,
    ctx: &mut Ctx,
) {
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(def) => {
                let name = def.name.as_str();
                let decorators = decorator_names(&def.decorator_list);
                let is_property = decorators.iter().any(|d| {
                    d == "property"
                        || d.ends_with(".setter")
                        || d.ends_with(".deleter")
                        || d.ends_with(".getter")
                });
                let is_static = decorators
                    .iter()
                    .any(|d| d == "staticmethod" || d == "classmethod");
                let family = if is_property {
                    "property"
                } else if matches!(scope, ScopeKind::Class) {
                    "method"
                } else {
                    "function"
                };
                let slot: &'static str = match scope {
                    ScopeKind::Class => {
                        if is_static {
                            "static"
                        } else {
                            "instance"
                        }
                    }
                    _ => "static",
                };
                let local = matches!(scope, ScopeKind::Function);
                let qualified_name = if local {
                    format!("{owner_qname}.<locals>.{name}")
                } else {
                    format!("{owner_qname}.{name}")
                };
                let params: Vec<lekalo_adapter_sdk::json::Value> = def
                    .parameters
                    .iter_source_order()
                    .map(|param| param_value(ctx.source, &def.parameters, param))
                    .collect();
                let signature = ids::signature(lekalo_adapter_sdk::json::Value::Array(vec![
                    lekalo_adapter_sdk::json::Value::Str(ids::SIGNATURE_DOMAIN.to_owned()),
                    lekalo_adapter_sdk::json::Value::Str(family.to_owned()),
                    lekalo_adapter_sdk::json::Value::Str(qualified_name.clone()),
                    lekalo_adapter_sdk::json::Value::Str(slot.to_owned()),
                    lekalo_adapter_sdk::json::Value::Bool(def.is_async),
                    lekalo_adapter_sdk::json::Value::Array(params),
                    def.returns.as_deref().map_or(
                        lekalo_adapter_sdk::json::Value::Null,
                        |expr| {
                            lekalo_adapter_sdk::json::Value::Str(
                                slice(ctx.source, expr.range()).to_owned(),
                            )
                        },
                    ),
                    lekalo_adapter_sdk::json::Value::Array(
                        decorators
                            .iter()
                            .cloned()
                            .map(lekalo_adapter_sdk::json::Value::Str)
                            .collect(),
                    ),
                ]));
                let sym = push_sym(
                    ctx,
                    owner,
                    qualified_name,
                    family,
                    slot,
                    Some(signature),
                    def.range,
                );
                walk_block(&def.body, &sym, &sym.qualified_name, ScopeKind::Function, 0, ctx);
            }
            Stmt::ClassDef(def) => {
                let name = def.name.as_str();
                let decorators = decorator_names(&def.decorator_list);
                let qualified_name = format!("{owner_qname}.{name}");
                let bases: Vec<lekalo_adapter_sdk::json::Value> = def
                    .arguments
                    .as_deref()
                    .map(|arguments| {
                        arguments
                            .args
                            .iter()
                            .map(|expr| {
                                lekalo_adapter_sdk::json::Value::Str(
                                    slice(ctx.source, expr.range()).to_owned(),
                                )
                            })
                            .chain(arguments.keywords.iter().map(|keyword| {
                                lekalo_adapter_sdk::json::Value::Str(format!(
                                    "{}={}",
                                    keyword
                                        .arg
                                        .as_ref()
                                        .map(|arg| arg.as_str())
                                        .unwrap_or("**"),
                                    slice(ctx.source, keyword.value.range())
                                ))
                            }))
                            .collect()
                    })
                    .unwrap_or_default();
                let signature = ids::signature(lekalo_adapter_sdk::json::Value::Array(vec![
                    lekalo_adapter_sdk::json::Value::Str(ids::SIGNATURE_DOMAIN.to_owned()),
                    lekalo_adapter_sdk::json::Value::Str("class".to_owned()),
                    lekalo_adapter_sdk::json::Value::Str(qualified_name.clone()),
                    lekalo_adapter_sdk::json::Value::Array(bases),
                    lekalo_adapter_sdk::json::Value::Array(
                        decorators
                            .iter()
                            .cloned()
                            .map(lekalo_adapter_sdk::json::Value::Str)
                            .collect(),
                    ),
                ]));
                let sym = push_sym(
                    ctx,
                    owner,
                    qualified_name,
                    "class",
                    "static",
                    Some(signature),
                    def.range,
                );
                walk_block(&def.body, &sym, &sym.qualified_name, ScopeKind::Class, 0, ctx);
            }
            Stmt::TypeAlias(def) => {
                if let Expr::Name(name) = def.name.as_ref() {
                    let qualified_name = format!("{owner_qname}.{}", name.id.as_str());
                    let signature = ids::signature(lekalo_adapter_sdk::json::Value::Array(
                        vec![
                            lekalo_adapter_sdk::json::Value::Str(
                                ids::SIGNATURE_DOMAIN.to_owned(),
                            ),
                            lekalo_adapter_sdk::json::Value::Str("type-alias".to_owned()),
                            lekalo_adapter_sdk::json::Value::Str(qualified_name.clone()),
                            lekalo_adapter_sdk::json::Value::Str(
                                slice(ctx.source, def.value.range()).to_owned(),
                            ),
                        ],
                    ));
                    push_sym(
                        ctx,
                        owner,
                        qualified_name,
                        "type-alias",
                        "static",
                        Some(signature),
                        def.range,
                    );
                }
            }
            Stmt::Assign(def) => {
                for target in &def.targets {
                    record_target(ctx, owner, owner_qname, &scope, target, None);
                }
            }
            Stmt::AnnAssign(def) => {
                record_target(
                    ctx,
                    owner,
                    owner_qname,
                    &scope,
                    &def.target,
                    Some(slice(ctx.source, def.annotation.range())),
                );
            }
            Stmt::Import(def) => {
                for alias in &def.names {
                    let module = alias.name.as_str();
                    if lekalo_adapter_sdk::paths::is_semantic_id(module) {
                        ctx.references.insert(RefRow {
                            target: module.to_owned(),
                            role: "reference",
                            confidence: "exact",
                        });
                    }
                }
                if conditional_depth > 0 {
                    push_unc(ctx, "conditional-import", def.range);
                }
            }
            Stmt::ImportFrom(def) => {
                let star = def.names.iter().any(|alias| alias.name.as_str() == "*");
                if star {
                    push_unc(ctx, "star-import", def.range);
                    continue;
                }
                let base: Option<String> = if def.level == 0 {
                    def.module.as_ref().map(|m| m.as_str().to_owned())
                } else {
                    resolve_relative(ctx.module, ctx.is_init, def.level, def.module.as_ref())
                };
                match base {
                    Some(base) => {
                        for alias in &def.names {
                            let target = format!("{base}.{}", alias.name.as_str());
                            if lekalo_adapter_sdk::paths::is_semantic_id(&target) {
                                ctx.references.insert(RefRow {
                                    target,
                                    role: "reference",
                                    confidence: "exact",
                                });
                            }
                        }
                    }
                    None => push_unc(ctx, "unresolved-relative-import", def.range),
                }
                if conditional_depth > 0 {
                    push_unc(ctx, "conditional-import", def.range);
                }
            }
            Stmt::Global(_) => {
                push_unc(ctx, "global-declaration", stmt.range());
            }
            Stmt::Nonlocal(_) => {
                push_unc(ctx, "nonlocal-declaration", stmt.range());
            }
            Stmt::If(def) => {
                walk_block(&def.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
                for clause in &def.elif_else_clauses {
                    walk_block(&clause.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
                }
            }
            Stmt::For(def) => {
                walk_block(&def.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
                walk_block(&def.orelse, owner, owner_qname, scope, conditional_depth + 1, ctx);
            }
            Stmt::While(def) => {
                walk_block(&def.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
                walk_block(&def.orelse, owner, owner_qname, scope, conditional_depth + 1, ctx);
            }
            Stmt::With(def) => {
                walk_block(&def.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
            }
            Stmt::Try(def) => {
                walk_block(&def.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
                for handler in &def.handlers {
                    let ruff_python_ast::ExceptHandler::ExceptHandler(handler) = handler;
                    walk_block(&handler.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
                }
                walk_block(&def.orelse, owner, owner_qname, scope, conditional_depth + 1, ctx);
                walk_block(&def.finalbody, owner, owner_qname, scope, conditional_depth + 1, ctx);
            }
            Stmt::Match(def) => {
                for case in &def.cases {
                    walk_block(&case.body, owner, owner_qname, scope, conditional_depth + 1, ctx);
                }
            }
            _ => {}
        }
    }
}

/// The source-order slot of one parameter inside the call contract:
/// `posonly`/`kwonly` markers carry the `/` and `*` separators that
/// bare name lists lose, so `f(a, /)` and `f(*, a)` can never share a
/// signature digest.
fn param_kind(parameters: &ruff_python_ast::Parameters, param: AnyParameterRef<'_>) -> &'static str {
    let range = param.range();
    if parameters.vararg.as_deref().is_some_and(|p| p.range() == range) {
        return "*";
    }
    if parameters.kwarg.as_deref().is_some_and(|p| p.range() == range) {
        return "**";
    }
    if parameters.kwonlyargs.iter().any(|p| p.range() == range) {
        return "kwonly";
    }
    if parameters.posonlyargs.iter().any(|p| p.range() == range) {
        return "posonly";
    }
    "regular"
}

fn param_value(
    source: &str,
    parameters: &ruff_python_ast::Parameters,
    param: AnyParameterRef<'_>,
) -> lekalo_adapter_sdk::json::Value {
    use lekalo_adapter_sdk::json::Value;
    Value::Array(vec![
        Value::Str(param.name().as_str().to_owned()),
        param
            .annotation()
            .map(|expr| Value::Str(slice(source, expr.range()).to_owned()))
            .unwrap_or(Value::Null),
        Value::Bool(param.default().is_some()),
        Value::Bool(param.is_variadic()),
        Value::Str(param_kind(parameters, param).to_owned()),
    ])
}

fn push_sym(
    ctx: &mut Ctx,
    owner: &Sym,
    qualified_name: String,
    family: &str,
    slot: &'static str,
    signature: Option<String>,
    range: TextRange,
) -> Sym {
    // Same-spelling declarations in same-named owners share one
    // qualified name; the owner is deliberately absent so the second
    // `class A`'s `f` is occurrence 1, not a duplicate of occurrence 0.
    let key = (qualified_name.clone(), family.to_owned());
    let occurrence = ctx.occurrences.entry(key).or_insert(0);
    let index = *occurrence;
    *occurrence += 1;
    let sym = Sym {
        native: ids::native_id(ctx.path, &qualified_name, family, slot, index),
        qualified_name,
        family: family.to_owned(),
        path: ctx.path.to_owned(),
        line: line(ctx.line_index, range),
        end_line: end_line(ctx.line_index, range),
        slot,
        declaration_only: ctx.declaration_only,
        member_of: Some(owner.native.clone()),
        signature,
    };
    ctx.symbols.push(sym.clone());
    sym
}

fn record_target(
    ctx: &mut Ctx,
    owner: &Sym,
    owner_qname: &str,
    scope: &ScopeKind,
    target: &Expr,
    annotation: Option<&str>,
) {
    match target {
        Expr::Name(name) => {
            if matches!(scope, ScopeKind::Function) {
                return;
            }
            let (family, slot): (&str, &'static str) = match scope {
                ScopeKind::Class => ("property", "static"),
                _ => ("variable", "static"),
            };
            let qualified_name = format!("{owner_qname}.{}", name.id.as_str());
            let signature = ids::signature(lekalo_adapter_sdk::json::Value::Array(vec![
                lekalo_adapter_sdk::json::Value::Str(ids::SIGNATURE_DOMAIN.to_owned()),
                lekalo_adapter_sdk::json::Value::Str(family.to_owned()),
                lekalo_adapter_sdk::json::Value::Str(qualified_name.clone()),
                annotation.map_or(lekalo_adapter_sdk::json::Value::Null, |text| {
                    lekalo_adapter_sdk::json::Value::Str(text.to_owned())
                }),
            ]));
            push_sym(
                ctx,
                owner,
                qualified_name,
                family,
                slot,
                Some(signature),
                target.range(),
            );
        }
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                record_target(ctx, owner, owner_qname, scope, element, annotation);
            }
        }
        Expr::List(list) => {
            for element in &list.elts {
                record_target(ctx, owner, owner_qname, scope, element, annotation);
            }
        }
        Expr::Starred(starred) => {
            record_target(ctx, owner, owner_qname, scope, &starred.value, annotation);
        }
        Expr::Attribute(attribute) => {
            // `module.attr = value` is the monkey-patch shape: retain it
            // as uncertainty, never a declaration. `self`/`cls` attribute
            // writes are ordinary instance/class state, not patching.
            if let Expr::Name(base) = attribute.value.as_ref() {
                if !matches!(base.id.as_str(), "self" | "cls") {
                    push_unc(ctx, "monkey-patch-candidate", attribute.range);
                }
            }
        }
        _ => {}
    }
}

/// Resolve `from <dots><module> import …` to the absolute dotted base.
/// The anchor is the file's importable module name, never the lexical
/// owner: `from .x import y` inside `pkg/mod.py::f` still means
/// `pkg.x`. A plain module pops its own name once per leading dot
/// (`__package__` is the parent); an `__init__.py[i]` module IS the
/// package, so level 1 pops nothing. A resolution that climbs above
/// the top-level package stays honestly unresolved.
fn resolve_relative(
    module_name: &str,
    is_init: bool,
    level: u32,
    module: Option<&ruff_python_ast::Identifier>,
) -> Option<String> {
    let mut segments: Vec<&str> = module_name.split('.').collect();
    let pops = if is_init { level.saturating_sub(1) } else { level };
    for _ in 0..pops {
        segments.pop()?;
    }
    if segments.is_empty() {
        return None;
    }
    if let Some(module) = module {
        segments.extend(module.as_str().split('.'));
    }
    Some(segments.join("."))
}

fn push_unc(ctx: &mut Ctx, detail: &str, range: TextRange) {
    ctx.uncertainty.push(UncRow {
        path: ctx.path.to_owned(),
        detail: detail.to_owned(),
        line: Some(line(ctx.line_index, range)),
        source_digest: ctx.digest.to_owned(),
    });
}

/// Whole-module dynamic-construct scan: calls into `eval`/`exec`/
/// `__import__`/`importlib`, `getattr`/`setattr` and friends surface as
/// `any-surface` rows. Lookalike names are recorded, never resolved.
struct DynamicScan {
    hits: Vec<(TextRange, &'static str)>,
    calls: Vec<String>,
}

const DYNAMIC_CALLS: &[(&str, &str)] = &[
    ("eval", "dynamic-eval"),
    ("exec", "dynamic-exec"),
    ("compile", "dynamic-compile"),
    ("__import__", "dynamic-import"),
    ("import_module", "dynamic-import"),
    ("getattr", "dynamic-getattr"),
    ("setattr", "dynamic-setattr"),
    ("delattr", "dynamic-delattr"),
    ("globals", "dynamic-globals"),
    ("locals", "dynamic-locals"),
    ("vars", "dynamic-vars"),
];

impl<'a> SourceOrderVisitor<'a> for DynamicScan {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Call(call) = expr {
            if let Some(name) = last_segment(&call.func) {
                if let Some((_, detail)) = DYNAMIC_CALLS.iter().find(|(n, _)| *n == name) {
                    self.hits.push((expr.range(), *detail));
                }
            }
            if let Some(name) = dotted(&call.func) {
                self.calls.push(name);
            }
        }
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(path: &str, module: &str, source: &str) -> FileScan {
        extract(path, module, source, false, "sha256:test")
    }

    #[test]
    fn redeclared_owner_members_never_collide() {
        let out = scan(
            "pkg/mod.py",
            "pkg.mod",
            "class A:\n    def f(self):\n        pass\n\nclass A:\n    def f(self):\n        pass\n",
        );
        let methods: Vec<_> = out
            .symbols
            .iter()
            .filter(|sym| sym.qualified_name == "pkg.mod.A.f")
            .collect();
        assert_eq!(methods.len(), 2);
        assert_ne!(methods[0].native, methods[1].native);
        // Same for the owners themselves.
        let owners: Vec<_> = out
            .symbols
            .iter()
            .filter(|sym| sym.qualified_name == "pkg.mod.A")
            .collect();
        assert_eq!(owners.len(), 2);
        assert_ne!(owners[0].native, owners[1].native);
        assert_ne!(methods[0].member_of, methods[1].member_of);
    }

    #[test]
    fn property_accessors_stay_in_the_property_family() {
        let out = scan(
            "pkg/mod.py",
            "pkg.mod",
            "class A:\n    @property\n    def x(self):\n        return self._x\n\n    @x.setter\n    def x(self, value):\n        self._x = value\n\n    @x.deleter\n    def x(self):\n        del self._x\n",
        );
        let families: Vec<_> = out
            .symbols
            .iter()
            .filter(|sym| sym.qualified_name == "pkg.mod.A.x")
            .map(|sym| sym.family.as_str())
            .collect();
        assert_eq!(families, ["property", "property", "property"]);
    }

    #[test]
    fn relative_import_anchors_on_the_module_not_the_owner() {
        let out = scan(
            "pkg/mod.py",
            "pkg.mod",
            "def f():\n    from .other import Thing\n    return Thing()\n",
        );
        assert!(out
            .references
            .iter()
            .any(|row| row.target == "pkg.other.Thing" && row.confidence == "exact"));
        assert!(!out.references.iter().any(|row| row.target.contains("mod.other")));
    }

    #[test]
    fn init_module_resolves_relative_import_inside_itself() {
        let out = scan(
            "pkg/__init__.py",
            "pkg",
            "from .sub import thing\n",
        );
        assert!(out
            .references
            .iter()
            .any(|row| row.target == "pkg.sub.thing"));
    }

    #[test]
    fn beyond_top_level_relative_import_stays_unresolved() {
        let out = scan(
            "pkg/__init__.py",
            "pkg",
            "from ..elsewhere import thing\n",
        );
        assert!(!out.references.iter().any(|row| row.target.contains("elsewhere")));
        assert!(out
            .uncertainty
            .iter()
            .any(|row| row.detail == "unresolved-relative-import"));
    }

    #[test]
    fn call_contract_kind_changes_the_signature() {
        // `f(a, /)` and `f(*, a)` are different public contracts: the
        // signature digest must see the separator slot, not just names.
        fn signature_of(source: &str) -> Option<String> {
            scan("pkg/mod.py", "pkg.mod", source)
                .symbols
                .into_iter()
                .find(|sym| sym.qualified_name == "pkg.mod.f")
                .and_then(|sym| sym.signature)
        }
        let posonly = signature_of("def f(a, /):\n    pass\n");
        let kwonly = signature_of("def f(*, a):\n    pass\n");
        let regular = signature_of("def f(a):\n    pass\n");
        assert_ne!(posonly, kwonly);
        assert_ne!(posonly, regular);
        assert_ne!(kwonly, regular);
        // *args and **kwargs are distinct kinds too.
        let vararg = signature_of("def f(*a):\n    pass\n");
        let kwarg = signature_of("def f(**a):\n    pass\n");
        assert_ne!(vararg, kwarg);
    }
}

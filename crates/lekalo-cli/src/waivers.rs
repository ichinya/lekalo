//! User-owned, versioned governance configuration; add previews before apply.
use clap::{Args, Subcommand};
use lekalo_core::{
    ai_lint::{self, input},
    waivers::{
        self as w,
        policy::{self, ProfileState},
        *,
    },
    DomainResult,
};
use std::path::Path;

#[derive(Debug, Args)]
pub(crate) struct Common {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value = "lekalo.waivers.json")]
    pub store: String,
    #[arg(long)]
    pub facts: Option<String>,
    #[arg(long)]
    pub as_of: Option<String>,
    #[arg(long, default_value="validation", value_parser=["validation","ai-lint","target"])]
    pub profile_kind: String,
    #[arg(long)]
    pub profile: Option<String>,
    #[arg(long)]
    pub profile_file: Option<String>,
    #[arg(long)]
    pub lint_config: Option<String>,
    #[arg(long, default_value_t = 604800)]
    pub expiring_window: u64,
    /// Require the source report's exact current lekalo.lock digest.
    #[arg(long)]
    pub locked: bool,
}
#[derive(Debug, Subcommand)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Commands {
    /// List committed waiver entries without claiming current effectiveness.
    List {
        #[command(flatten)]
        args: Common,
    },
    /// Audit immutable supplied facts under an existing resolved profile.
    Audit {
        #[command(flatten)]
        args: Common,
        #[arg(long)]
        base: Option<String>,
        #[arg(long)]
        check: bool,
        /// Verify exact prior done evidence; changed inputs/time/store refuse.
        #[arg(long)]
        done: Option<String>,
    },
    /// Preview a scoped waiver candidate; --apply writes the exact reviewed plan.
    Add {
        selector: String,
        #[command(flatten)]
        args: Common,
        #[arg(long)]
        id: String,
        #[arg(long)]
        symbol: Option<String>,
        #[arg(long)]
        module: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        project_scope: bool,
        #[arg(long)]
        target_scope: bool,
        #[arg(long)]
        profile_scope: bool,
        #[arg(long)]
        target: String,
        #[arg(long)]
        subject: Option<String>,
        #[arg(long)]
        capability: bool,
        #[arg(long)]
        owner: String,
        #[arg(long)]
        approver: String,
        #[arg(long)]
        approval_ref: String,
        #[arg(long)]
        reason: String,
        #[arg(long)]
        source_issue: Option<String>,
        #[arg(long)]
        source_decision: Option<String>,
        #[arg(long, default_value = "correctness")]
        risk: String,
        #[arg(long)]
        expires: Option<String>,
        #[arg(long)]
        review_after: Option<String>,
        #[arg(long)]
        supersedes: Option<String>,
        #[arg(long)]
        apply: Option<String>,
    },
}
enum Loaded {
    Validation(lekalo_core::validator::ValidationProfile, String),
    Lint(ai_lint::Config, String),
    Target(lekalo_core::target_profile::resolution::ResolvedProfile),
}
impl ProfileState for Loaded {
    fn producer_domain_admitted(&self, f: &Fact) -> bool {
        match self {
            Self::Validation(p, d) => policy::ValidationState {
                profile: p,
                digest: d.clone(),
            }
            .producer_domain_admitted(f),
            Self::Lint(c, id) => policy::LintState { config: c, id }.producer_domain_admitted(f),
            Self::Target(p) => policy::TargetState { profile: p }.producer_domain_admitted(f),
        }
    }
    fn fingerprint_requirements(&self, f: &Fact) -> policy::FingerprintRequirements {
        match self {
            Self::Validation(p, d) => policy::ValidationState {
                profile: p,
                digest: d.clone(),
            }
            .fingerprint_requirements(f),
            Self::Lint(c, id) => policy::LintState { config: c, id }.fingerprint_requirements(f),
            Self::Target(p) => policy::TargetState { profile: p }.fingerprint_requirements(f),
        }
    }
    fn reference(&self) -> ProfileRef {
        match self {
            Self::Validation(p, d) => policy::ValidationState {
                profile: p,
                digest: d.clone(),
            }
            .reference(),
            Self::Lint(c, id) => policy::LintState { config: c, id }.reference(),
            Self::Target(p) => policy::TargetState { profile: p }.reference(),
        }
    }
    fn rule(&self, s: &Selector, f: &Fact) -> Option<policy::RuleState> {
        match self {
            Self::Validation(p, d) => policy::ValidationState {
                profile: p,
                digest: d.clone(),
            }
            .rule(s, f),
            Self::Lint(c, id) => policy::LintState { config: c, id }.rule(s, f),
            Self::Target(p) => policy::TargetState { profile: p }.rule(s, f),
        }
    }
}
fn read(root: &Path, name: &str) -> Result<Vec<u8>, DomainResult> {
    if lekalo_core::project_fs::path_violation(name).is_some() {
        return Err(w::failure("waivers-input-path"));
    }
    let (dir, file) = name.rsplit_once('/').unwrap_or(("", name));
    lekalo_core::project_fs::Fs::open(root)
        .and_then(|f| f.read_file_opt(dir, file, input::MAX_BYTES))
        .map_err(|_| w::failure("waivers-input-read"))?
        .ok_or_else(|| w::failure("waivers-input-missing"))
}
fn load_profile(args: &Common, root: &Path) -> Result<Loaded, DomainResult> {
    match args.profile_kind.as_str() {
        "validation" => {
            let bytes = if let Some(file) = &args.profile_file {
                read(root, file)?
            } else {
                match args.profile.as_deref().unwrap_or("default") {
                    "default" => lekalo_core::validator::profile::DEFAULT_PROFILE_BYTES.to_vec(),
                    "strict" => lekalo_core::validator::profile::STRICT_PROFILE_BYTES.to_vec(),
                    _ => return Err(w::failure("waivers-profile-unknown")),
                }
            };
            let value: serde_json::Value = w::parse(&bytes)?;
            let profile = lekalo_core::validator::ValidationProfile::from_bytes(&bytes)
                .map_err(|_| w::failure("waivers-profile-invalid"))?;
            if args
                .profile
                .as_ref()
                .is_some_and(|id| id != profile.profile_id())
            {
                return Err(w::failure("waivers-profile-id"));
            }
            Ok(Loaded::Validation(profile, input::hash(&value)))
        }
        "ai-lint" => {
            let config = if let Some(file) = &args.lint_config {
                input::parse_config(&read(root, file)?)?
            } else {
                ai_lint::default_config()
            };
            let id = args.profile.clone().unwrap_or_else(|| "advisory".into());
            if !config.profiles.iter().any(|p| p.id == id) {
                return Err(w::failure("waivers-profile-unknown"));
            }
            Ok(Loaded::Lint(config, id))
        }
        "target" => {
            let file = args
                .profile_file
                .as_ref()
                .ok_or_else(|| w::failure("waivers-profile-required"))?;
            let doc = lekalo_core::target_profile::document::decode(&read(root, file)?)
                .map_err(|_| w::failure("waivers-profile-invalid"))?;
            let mut profiles = lekalo_core::target_profile::resolution::resolve(&doc)
                .map_err(|_| w::failure("waivers-profile-invalid"))?;
            let pos = profiles
                .iter()
                .position(|p| args.profile.as_ref().is_some_and(|id| id == &p.id))
                .ok_or_else(|| w::failure("waivers-profile-id"))?;
            Ok(Loaded::Target(profiles.remove(pos)))
        }
        _ => Err(DomainResult::usage_error()),
    }
}
fn evaluate(
    args: &Common,
    ctx: &lekalo_core::observed::ObservedContext,
) -> Result<(Input, Loaded, String), DomainResult> {
    let profile = load_profile(args, &ctx.root)?;
    let facts: Input = w::parse(&read(
        &ctx.root,
        args.facts
            .as_ref()
            .ok_or_else(|| w::failure("waivers-facts-required"))?,
    )?)?;
    w::validate_input(&facts)?;
    let compilation = lekalo_core::ir::compile(&ctx.model).map_err(|f| f.into_result())?;
    let project = compilation
        .project
        .project
        .as_ref()
        .ok_or_else(|| w::failure("waivers-project-missing"))?;
    if facts.project_id != project.id.as_str() || facts.profile_ref != profile.reference() {
        return Err(w::failure("waivers-project-profile"));
    }
    let model = input::digest(lekalo_core::loader::canonical_model_bytes(&ctx.model).as_bytes());
    let ir = input::digest(compilation.project.to_canonical_json().as_bytes());
    for fact in &facts.facts {
        if fact.fingerprint.model.known() != Some(&model)
            || fact.fingerprint.ir.known() != Some(&ir)
            || fact.symbol.known().is_some_and(|s| {
                !compilation
                    .project
                    .definitions
                    .iter()
                    .any(|d| d.id().as_str() == s)
            })
            || fact.module.known().is_some_and(|s| {
                !compilation
                    .project
                    .modules
                    .iter()
                    .any(|m| m.id.as_str() == s)
            })
        {
            return Err(w::failure("waivers-current-model"));
        }
    }
    if args.locked {
        let lock = read(&ctx.root, "lekalo.lock")?;
        let _ = lekalo_core::lockfile::Lockfile::parse_canonical(&lock)
            .map_err(|_| w::failure("waivers-lock-invalid"))?;
        let digest = input::digest(lock.strip_suffix(b"\n").unwrap_or(&lock));
        if facts.lock_ref.known() != Some(&digest) {
            return Err(w::failure("waivers-lock-changed"));
        }
    }
    let as_of = w::evaluation_time(
        args.as_of
            .as_ref()
            .ok_or_else(|| w::failure("waivers-as-of-required"))?,
    )?;
    Ok((facts, profile, as_of))
}
pub(crate) fn run(command: Commands) -> DomainResult {
    match execute(command) {
        Ok(r) | Err(r) => r,
    }
}
fn execute(command: Commands) -> Result<DomainResult, DomainResult> {
    let args = match &command {
        Commands::List { args } | Commands::Audit { args, .. } | Commands::Add { args, .. } => args,
    };
    let selection = lekalo_core::loader::LoadSelection {
        project: args
            .project
            .clone()
            .or_else(|| std::env::var("LEKALO_PROJECT").ok()),
    };
    let ctx = lekalo_core::observed::context(&selection)?;
    let before = w::store::read(&ctx.root, &args.store)?;
    if let Commands::List { .. } = &command {
        let store: Store = w::parse(
            before
                .as_deref()
                .ok_or_else(|| w::failure("waivers-store-missing"))?,
        )?;
        w::validate_store(&store)?;
        let compilation = lekalo_core::ir::compile(&ctx.model).map_err(|f| f.into_result())?;
        if compilation.project.project.as_ref().map(|p| p.id.as_str())
            != Some(store.project_id.as_str())
        {
            return Err(w::failure("waivers-store-project"));
        }
        return Ok(DomainResult::graph(serde_json::json!({"status":"valid","store":store,"storeDigest":input::hash(&store),"effectiveness":"unexamined"}).to_string(),format!("{} waiver entries",store.entries.len()),Vec::new()));
    }
    let (facts, profile, as_of) = evaluate(args, &ctx)?;
    let mut store: Store = before
        .as_deref()
        .map(w::parse)
        .transpose()?
        .unwrap_or_else(|| w::empty_store(facts.project_id.clone()));
    w::validate_store(&store)?;
    match command {
        Commands::Audit {
            args,
            base,
            check,
            done,
        } => {
            let base: Option<Store> = base
                .as_ref()
                .map(|p| read(&ctx.root, p).and_then(|b| w::parse(&b)))
                .transpose()?;
            let audit = w::audit(
                &store,
                &facts,
                &profile,
                &as_of,
                args.expiring_window,
                base.as_ref(),
            )?;
            if done.as_ref().is_some_and(|d| d != &audit.done_digest) {
                return Err(w::failure("waivers-done-stale"));
            }
            Ok(w::render(&audit, check))
        }
        Commands::Add {
            selector,
            args,
            id,
            symbol,
            module,
            path,
            project_scope,
            target_scope,
            profile_scope,
            target,
            subject,
            capability,
            owner,
            approver,
            approval_ref,
            reason,
            source_issue,
            source_decision,
            risk,
            expires,
            review_after,
            supersedes,
            apply,
        } => {
            let choices = usize::from(symbol.is_some())
                + usize::from(module.is_some())
                + usize::from(path.is_some())
                + usize::from(project_scope)
                + usize::from(target_scope)
                + usize::from(profile_scope);
            if choices != 1
                || usize::from(source_issue.is_some()) + usize::from(source_decision.is_some()) != 1
            {
                return Err(DomainResult::usage_error());
            }
            let selector = if capability {
                Selector {
                    kind: SelectorKind::Capability,
                    id: selector,
                }
            } else {
                let registry = lekalo_core::diagnostics::registry::DiagnosticRegistry::embedded()
                    .map_err(|_| w::failure("waivers-registry"))?;
                let entries: serde_json::Value =
                    serde_json::from_slice(lekalo_core::diagnostics::registry::REGISTRY_BYTES)
                        .expect("embedded registry");
                let id = if registry.entry(&selector).is_some() {
                    selector
                } else {
                    entries["entries"]
                        .as_array()
                        .expect("entries")
                        .iter()
                        .find(|e| e["code"].as_str() == Some(&selector))
                        .and_then(|e| e["id"].as_str())
                        .ok_or_else(|| w::failure("waivers-rule-unknown"))?
                        .into()
                };
                Selector {
                    kind: SelectorKind::Rule,
                    id,
                }
            };
            let scope = if let Some(id) = symbol {
                Scope {
                    kind: ScopeKind::Symbol,
                    id,
                }
            } else if let Some(id) = module {
                Scope {
                    kind: ScopeKind::Module,
                    id,
                }
            } else if let Some(id) = path {
                Scope {
                    kind: ScopeKind::Path,
                    id,
                }
            } else if project_scope {
                Scope {
                    kind: ScopeKind::Project,
                    id: facts.project_id.clone(),
                }
            } else if target_scope {
                Scope {
                    kind: ScopeKind::Target,
                    id: target.clone(),
                }
            } else {
                Scope {
                    kind: ScopeKind::Profile,
                    id: profile.reference().id.clone(),
                }
            };
            let matches: Vec<_> = facts
                .facts
                .iter()
                .filter(|f| {
                    f.selector == selector
                        && f.target == target
                        && subject.as_ref().map_or(true, |s| s == &f.subject)
                        && match scope.kind {
                            ScopeKind::Symbol => f.symbol.known() == Some(&scope.id),
                            ScopeKind::Module => f.module.known() == Some(&scope.id),
                            ScopeKind::Path => f.path.known() == Some(&scope.id),
                            _ => true,
                        }
                })
                .collect();
            if matches.len() != 1 {
                return Err(w::failure("waivers-exact-occurrence-required"));
            }
            let fact = matches[0];
            if store.entries.iter().any(|e| e.id == id) {
                return Err(w::failure("waivers-id-reused"));
            }
            let mut entry = Entry {
                id,
                finding_id: fact.id.clone(),
                fact_digest: input::hash(fact),
                selector,
                scope,
                subject: fact.subject.clone(),
                target,
                profile_ref: profile.reference(),
                condition_digest: fact.condition_digest.clone(),
                owner,
                approver,
                approval_ref: Approval {
                    id: approval_ref,
                    subject_digest: input::digest(b"pending"),
                },
                reason,
                created_at: as_of.clone(),
                expires_at: expires
                    .map(|d| w::evaluation_time(&d))
                    .transpose()?
                    .map_or(State::Unknown, State::Known),
                review_after: review_after
                    .map(|d| w::evaluation_time(&d))
                    .transpose()?
                    .map_or(State::Unknown, State::Known),
                source_ref: SourceRef {
                    kind: if source_issue.is_some() {
                        "issue"
                    } else {
                        "decision"
                    }
                    .into(),
                    id: source_issue.or(source_decision).expect("one source"),
                    digest: State::Unknown,
                },
                accepted_risk: w::parse(serde_json::to_string(&risk).expect("string").as_bytes())?,
                fingerprint: fact.fingerprint.clone(),
                lifecycle: Lifecycle::Active,
                supersedes: supersedes.clone().map_or(State::Unknown, State::Known),
            };
            entry.approval_ref.subject_digest = w::approval_subject(&entry);
            if let Some(prior) = supersedes {
                let old = store
                    .entries
                    .iter_mut()
                    .find(|e| e.id == prior)
                    .ok_or_else(|| w::failure("waivers-supersedes-missing"))?;
                if old.lifecycle != Lifecycle::Active {
                    return Err(w::failure("waivers-supersedes-inactive"));
                }
                old.lifecycle = Lifecycle::Superseded;
            }
            let added_id = entry.id.clone();
            store.entries.push(entry);
            store.entries.sort_by(|a, b| a.id.cmp(&b.id));
            let audit = w::audit(&store, &facts, &profile, &as_of, args.expiring_window, None)?;
            if !audit
                .entries
                .iter()
                .any(|e| e.id == added_id && e.effective)
            {
                return Err(w::failure("waivers-ineligible-candidate"));
            }
            let plan = w::store::plan_id(before.as_deref(), &store);
            let applied = apply.is_some();
            if let Some(expected) = apply {
                w::store::apply(&ctx.root, &args.store, before.as_deref(), &store, &expected)?;
            }
            Ok(DomainResult::graph(serde_json::json!({"status":"valid","candidate":store,"planId":plan,"applied":applied,"audit":audit}).to_string(),format!("Waiver candidate {}; {}",added_id,if applied{"written; commit and review required"}else{"preview; no files written"}),Vec::new()))
        }
        Commands::List { .. } => unreachable!(),
    }
}

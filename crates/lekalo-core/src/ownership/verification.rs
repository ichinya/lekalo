use super::*;
use std::{collections::BTreeMap, path::Path};

pub(crate) fn snapshot(files: &BTreeMap<String, Option<Vec<u8>>>) -> Outcome<tempfile::TempDir> {
    let dir = tempfile::Builder::new()
        .prefix("lekalo-ownership-")
        .tempdir()
        .map_err(|_| fail("ownership-staging-unavailable"))?;
    for (path, content) in files {
        if !path_ok(path) {
            return Err(fail("ownership-staging-path"));
        }
        if let Some(content) = content {
            mg::fs::write_file(dir.path(), path, content)?;
        }
    }
    Ok(dir)
}

fn run(
    host: &dyn VerificationHost,
    program: &Path,
    plan: &TransitionPlan,
    copy: &Path,
    bindings: &[Binding],
) -> Outcome<Vec<Observation>> {
    if read(copy, "ownership-bindings.json")?.is_some() {
        return Err(fail("ownership-runner-reserved-path"));
    }
    mg::fs::write_file(copy, "ownership-bindings.json", &bytes(&bindings))?;
    let (code, stdout) = host
        .run_immutable(program, &plan.request.runner.args, copy)
        .map_err(|_| fail("ownership-execution-unavailable"))?;
    if code != Some(0) {
        return Err(fail("ownership-execution-incomplete"));
    }
    decode(stdout.as_bytes())
}

/// Both copies execute the same reviewed, interpreter-pinned runner. Execution
/// happens only through the existing confined host capability; core never spawns.
pub fn verify(
    root: &Path,
    plan: &TransitionPlan,
    host: &dyn VerificationHost,
) -> Outcome<Verification> {
    revalidate(root, plan)?;
    let runner = &plan.request.runner;
    if !["node", "python", "python3", "deno", "bun"].contains(&runner.program.as_str())
        || runner.args.is_empty()
        || runner.args.len() > 64
        || !mg::pin(&runner.digest)
        || runner
            .args
            .iter()
            .any(|arg| arg.len() > 1024 || arg.contains('\0'))
    {
        return Err(fail("ownership-runner"));
    }
    let program = host
        .resolve_program(&runner.program)
        .ok_or_else(|| fail("ownership-tool-unavailable"))?;
    let runtime_bytes = std::fs::read(&program).map_err(|_| fail("ownership-tool-unreadable"))?;
    if mg::digest(&runtime_bytes) != runner.digest {
        return Err(fail("ownership-tool-stale"));
    }
    let mut scenarios = std::collections::BTreeSet::new();
    for expected in &runner.expected {
        if !mg::token(&expected.scenario) || !scenarios.insert(&expected.scenario) {
            return Err(fail("ownership-scenario-id"));
        }
    }
    let mut files = BTreeMap::new();
    for pin in &plan.inputs {
        if !pin.path.starts_with(".lekalo/") {
            files.insert(pin.path.clone(), read(root, &pin.path)?);
        }
    }
    let baseline = snapshot(&files)?;
    let candidate = snapshot(&files)?;
    mg::fs::ensure_dir(baseline.path(), "scratch")?;
    mg::fs::ensure_dir(candidate.path(), "scratch")?;
    for edit in &plan.edits {
        if edit.path.starts_with(".lekalo/") {
            continue;
        }
        if let Some(content) = &edit.content {
            mg::fs::write_file(candidate.path(), &edit.path, content.as_bytes())?;
        } else {
            mg::fs::remove_file(candidate.path(), &edit.path)?;
        }
    }
    // A reviewed argv entry naming a source/config must be in the sealed closure.
    for arg in &runner.args {
        if !arg.starts_with('-')
            && path_ok(arg)
            && read(root, arg)?.is_some()
            && !files.contains_key(arg)
        {
            return Err(fail("ownership-runner-input-unpinned"));
        }
    }
    let before = run(host, &program, plan, baseline.path(), &plan.baseline)?;
    let after = run(host, &program, plan, candidate.path(), &plan.candidate)?;
    // Independently check both against the oracle; two equally wrong versions
    // are not equivalent. JSON ordering/normalization is never guessed here.
    let mismatch = runner.expected.iter().find(|expected| {
        before
            .iter()
            .filter(|row| row.scenario == expected.scenario)
            .count()
            != 1
            || after
                .iter()
                .filter(|row| row.scenario == expected.scenario)
                .count()
                != 1
            || before.iter().find(|row| row.scenario == expected.scenario) != Some(*expected)
            || after.iter().find(|row| row.scenario == expected.scenario) != Some(*expected)
    });
    let complete = before.len() == runner.expected.len() && after.len() == runner.expected.len();
    revalidate(root, plan)?;
    Ok(Verification {
        schema_version: format!("lekalo/ownership-verification/v{VERSION}"),
        plan_digest: plan.plan_digest.clone(),
        runner_digest: runner.digest.clone(),
        verdict: if mismatch.is_none() && complete {
            "match"
        } else {
            "mismatch"
        }
        .to_owned(),
        observations: runner
            .expected
            .iter()
            .map(|expected| ObservationEvidence {
                scenario: expected.scenario.clone(),
                baseline_digest: before
                    .iter()
                    .find(|row| row.scenario == expected.scenario)
                    .map(mg::hash),
                candidate_digest: after
                    .iter()
                    .find(|row| row.scenario == expected.scenario)
                    .map(mg::hash),
                expected_digest: mg::hash(expected),
            })
            .collect(),
        counterexample: mismatch
            .map(|row| row.scenario.clone())
            .or_else(|| (!complete).then(|| "corpus-size".to_owned())),
    })
}

pub fn render_verification(report: &Verification) -> DomainResult {
    if report.verdict == "match" {
        return render(report);
    }
    DomainResult::denied_json(
        serde_json::json!({"status":"denied", "ownership":report}).to_string(),
        serde_json::to_string_pretty(report).expect("verification report"),
        super::diagnostic::set("ownership-equivalence-failed"),
    )
}

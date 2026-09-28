<?php
/**
 * Pure Composer/Laravel native gate plan construction (issue #61, plan
 * slice B; research doc §1 step 2 and §4).
 *
 * `php_build_native_plan` joins the verified Composer metadata (the
 * manifest bytes already checked by `php_verify_confirmations`), the
 * confirmed (package, gate, cwd, argv, digests) tuples, the host-
 * validated selection manifest, and the bounded changed inputs into
 * one immutable proposed `lekalo/native-gate-plan/v0.4.0` document.
 * It is a pure function of its inputs: no clock, no environment, no
 * absolute paths, no randomness, no I/O, no process launch.
 *
 * The plan is a proposal only — it can never authorize its own
 * execution, and the approval always lives outside the plan. The
 * canonical digest is SHA-256 over `domain || canonical(plan)` with
 * the canonical form UTF-8 JSON with recursively bytewise-sorted keys
 * and compact separators, `plan_digest` absent — byte-compatible with
 * the Rust core and the Node planner (shared golden vectors).
 */

declare(strict_types=1);

const PHP_PLAN_DIGEST_DOMAIN = 'lekalo.native-plan.v0.4.0';
const PHP_PLAN_SCHEMA_VERSION = 'lekalo/native-gate-plan/v0.4.0';
const PHP_SELECTION_DIGEST_DOMAIN = 'lekalo.native-selection.v0.4.0';
/** The planner capability id declared in every produced plan. */
const PHP_PLAN_CAPABILITY = 'plan.native-gates';
const PHP_PLANNER_VERSION = '0.4.0';
const PHP_CANONICALIZATION_VERSION = '0.4.0';

/** Plan bounds (mirrors the Rust wire validator). */
const PHP_MAX_CHANGED_FILES = 1024;
const PHP_MAX_MODULES = 256;
const PHP_MAX_MANDATORY = 128;
const PHP_MAX_COMMANDS = 128;

/**
 * One typed plan refusal with a stable bounded code.
 */
final class PhpPlanRefusal extends RuntimeException
{
    public function __construct(string $code)
    {
        parent::__construct($code);
    }
}

/**
 * The digest-addressed selection document reference: sha256 over
 * `PHP_SELECTION_DIGEST_DOMAIN || canonical(selection)`. Every command
 * of the plan carries this digest so the approved plan pins its own
 * selection artifacts; a post-approval edit of the selection member
 * cannot validate without changing the plan digest too.
 */
function php_selection_digest(array $selection): string
{
    return php_domain_digest(PHP_SELECTION_DIGEST_DOMAIN, $selection);
}

/**
 * The plan digest: sha256 over `PHP_PLAN_DIGEST_DOMAIN ||
 * canonical(plan without plan_digest)`.
 */
function php_plan_digest(array $plan): string
{
    unset($plan['plan_digest']);
    return php_domain_digest(PHP_PLAN_DIGEST_DOMAIN, $plan);
}

/**
 * Validate and normalize the closed selection manifest the host
 * validated (module roots, explicit edges, checked test bindings,
 * mandatory cross-module gate ids). Unknown members are refused —
 * the manifest is a closed document, never a guessing input.
 *
 * Shape:
 *   {schema_version: "lekalo/native-selection/v0.4.0", completeness: string,
 *    modules: [{id, roots: list<string>, gates: list<string>}],
 *    edges: [{from, to, kind: "dependency"|"target-binding"}],
 *    tests: [{suite_id, module, gate_id}],
 *    mandatory_gate_ids: list<string>}
 */
function php_decode_selection_manifest(array $manifest): array
{
    if (($manifest['schema_version'] ?? null) !== 'lekalo/native-selection/v0.4.0') {
        throw new PhpPlanRefusal('selection-manifest-version');
    }
    $allowed = ['schema_version', 'completeness', 'modules', 'edges', 'tests', 'mandatory_gate_ids'];
    foreach (array_keys($manifest) as $key) {
        if (!in_array($key, $allowed, true)) {
            throw new PhpPlanRefusal('selection-manifest-member');
        }
    }
    $completeness = $manifest['completeness'] ?? 'complete';
    if (!in_array($completeness, ['complete', 'incomplete', 'unknown'], true)) {
        throw new PhpPlanRefusal('selection-manifest-completeness');
    }
    $modules = [];
    $moduleIds = [];
    foreach (($manifest['modules'] ?? []) as $module) {
        if (!is_array($module) || !isset($module['id'], $module['roots'], $module['gates'])
            || !is_string($module['id']) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $module['id'])
            || !is_array($module['roots']) || !is_array($module['gates'])) {
            throw new PhpPlanRefusal('selection-manifest-module');
        }
        if (isset($moduleIds[$module['id']]) || count($modules) >= PHP_MAX_MODULES) {
            throw new PhpPlanRefusal('selection-manifest-module');
        }
        $moduleIds[$module['id']] = true;
        $roots = [];
        foreach ($module['roots'] as $root) {
            if (!is_string($root) || $root === '' || str_starts_with($root, '/')
                || str_contains($root, '\\') || str_contains($root, '..')) {
                throw new PhpPlanRefusal('selection-manifest-root');
            }
            $roots[] = $root;
        }
        $gates = [];
        foreach ($module['gates'] as $gateId) {
            if (!is_string($gateId) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $gateId)) {
                throw new PhpPlanRefusal('selection-manifest-gate');
            }
            $gates[] = $gateId;
        }
        sort($roots);
        sort($gates);
        $modules[] = ['id' => $module['id'], 'roots' => $roots, 'gates' => $gates];
    }
    $edges = [];
    foreach (($manifest['edges'] ?? []) as $edge) {
        if (!is_array($edge) || !isset($edge['from'], $edge['to'], $edge['kind'])
            || !isset($moduleIds[$edge['from']]) || !isset($moduleIds[$edge['to']])
            || $edge['from'] === $edge['to']
            || !in_array($edge['kind'], ['dependency', 'target-binding'], true)) {
            throw new PhpPlanRefusal('selection-manifest-edge');
        }
        $edges[] = ['from' => $edge['from'], 'to' => $edge['to'], 'kind' => $edge['kind']];
    }
    $tests = [];
    $testKeys = [];
    foreach (($manifest['tests'] ?? []) as $test) {
        if (!is_array($test) || !isset($test['suite_id'], $test['module'], $test['gate_id'])
            || !is_string($test['suite_id']) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $test['suite_id'])
            || !isset($moduleIds[$test['module']])
            || !is_string($test['gate_id'])) {
            throw new PhpPlanRefusal('selection-manifest-test');
        }
        // One suite may be reachable through several gates (a filtered
        // leaf and a full-run aggregate); only the exact binding triple
        // must be unique.
        $key = $test['module'] . "\n" . $test['suite_id'] . "\n" . $test['gate_id'];
        if (isset($testKeys[$key])) {
            throw new PhpPlanRefusal('selection-manifest-test');
        }
        $testKeys[$key] = true;
        $tests[] = ['suite_id' => $test['suite_id'], 'module' => $test['module'], 'gate_id' => $test['gate_id']];
    }
    $mandatory = [];
    foreach (($manifest['mandatory_gate_ids'] ?? []) as $gateId) {
        if (!is_string($gateId) || !preg_match('/^[a-z0-9][a-z0-9._-]{0,63}$/', $gateId)) {
            throw new PhpPlanRefusal('selection-manifest-mandatory');
        }
        $mandatory[] = $gateId;
    }
    if (count($mandatory) > PHP_MAX_MANDATORY) {
        throw new PhpPlanRefusal('selection-manifest-mandatory');
    }
    sort($mandatory);
    return [
        'completeness' => $completeness,
        'modules' => $modules,
        'edges' => $edges,
        'tests' => $tests,
        'mandatory_gate_ids' => $mandatory,
    ];
}

/**
 * The deepest module whose declared root is a path prefix of the
 * changed logical path, or null when ownership is unknown (bare
 * symbols always stay unattributed uncertainty).
 *
 * @param list<array{id: string, roots: list<string>, gates: list<string>}> $modules
 */
function php_module_of_path(array $modules, string $path): ?string
{
    $best = null;
    $bestLength = -1;
    foreach ($modules as $module) {
        foreach ($module['roots'] as $root) {
            $prefix = $root === '.' ? '' : rtrim($root, '/');
            $matches = $prefix === ''
                || $path === $prefix
                || str_starts_with($path, $prefix . '/');
            if ($matches && strlen($prefix) > $bestLength) {
                $best = $module['id'];
                $bestLength = strlen($prefix);
            }
        }
    }
    return $best;
}

/**
 * The affected closure: changed modules plus the reverse transitive
 * dependency closure over the declared module edges, with bounded
 * reason provenance. Deterministic sorted traversal.
 *
 * @param list<array{from: string, to: string, kind: string}> $edges
 * @return list<array{package_id: string, reasons: list<array<string, mixed>>}>
 */
function php_affected_closure(array $edges, array $changedModules): array
{
    $consumers = [];
    foreach ($edges as $edge) {
        $consumers[$edge['to']][] = $edge['from'];
    }
    $affected = [];
    $visit = function (string $moduleId, string $kind, string $sourceRef, array $path) use (&$visit, &$affected, $consumers): void {
        if (count($path) > 32) {
            return; // bounded explanation paths
        }
        $reason = ['kind' => $kind, 'source_ref' => $sourceRef, 'edge_path' => $path];
        if (isset($affected[$moduleId])) {
            foreach ($affected[$moduleId]['reasons'] as $existing) {
                if ($existing['kind'] === $kind && $existing['source_ref'] === $sourceRef) {
                    return;
                }
            }
            $affected[$moduleId]['reasons'][] = $reason;
            usort($affected[$moduleId]['reasons'], static fn (array $l, array $r): int
                => strcmp($l['kind'], $r['kind']) ?: strcmp($l['source_ref'], $r['source_ref']));
        } else {
            $affected[$moduleId] = ['package_id' => $moduleId, 'reasons' => [$reason]];
        }
        foreach ($consumers[$moduleId] ?? [] as $consumer) {
            $nextKind = $kind === 'changed-module' ? 'dependent-closure' : $kind;
            $visit($consumer, $nextKind, $kind === 'changed-module' ? $moduleId : $sourceRef, [...$path, $moduleId]);
        }
    };
    $changed = array_unique($changedModules);
    sort($changed);
    foreach ($changed as $moduleId) {
        $visit($moduleId, 'changed-module', $moduleId, []);
    }
    $list = array_values($affected);
    usort($list, static fn (array $l, array $r): int => strcmp($l['package_id'], $r['package_id']));
    return $list;
}

/**
 * Build the proposed native gate plan (issue #61). All inputs are
 * already validated: `$confirmed` from `php_verify_confirmations`,
 * `$manifest` the decoded selection manifest, `$custody` the pinned
 * digests. Missing confirmations exclude a module with a stable
 * reason — they never invent commands; mandatory cross-module gates
 * are always unioned into the selection.
 *
 * Input keys:
 *   composer            array  — the decoded composer.json (name)
 *   composer_lock_state string — present|absent|unreadable
 *   composer_lock_digest ?string
 *   policy              array  — the checked-in execution policy
 *   confirmed           list   — verified confirmation tuples
 *   changes             array  — {files: [{path, change}], symbols: []}
 *   selection_manifest  array  — the host-validated selection document
 *   custody             array  — input_manifest_digest, tool_catalog_digest,
 *                                capability_snapshot_digest, scan_ref, observed_ref,
 *                                profile_id, profile_digest, adapter_identity
 */
function php_build_native_plan(array $input): array
{
    foreach (['composer', 'policy', 'confirmed', 'changes', 'selection_manifest', 'custody'] as $key) {
        if (!array_key_exists($key, $input)) {
            throw new PhpPlanRefusal('plan-input-missing-' . $key);
        }
    }
    $composer = $input['composer'];
    $policy = $input['policy'];
    $confirmed = $input['confirmed'];
    $changes = $input['changes'];
    $custody = $input['custody'];
    $manifest = php_decode_selection_manifest($input['selection_manifest']);
    $packageName = $composer['name'] ?? null;
    if (!is_string($packageName) || $packageName === '') {
        throw new PhpPlanRefusal('composer-manifest-invalid');
    }
    $packageId = '.' . '=' . $packageName;

    // Attribute the changed inputs onto modules; unattributable
    // entries are recorded uncertainty, never a silent empty green.
    $uncertainties = [];
    $changedModules = [];
    $files = is_array($changes['files'] ?? null) ? $changes['files'] : [];
    if (count($files) > PHP_MAX_CHANGED_FILES) {
        throw new PhpPlanRefusal('changes-bound');
    }
    foreach ($files as $file) {
        $path = is_array($file) ? ($file['path'] ?? null) : null;
        if (!is_string($path)) {
            throw new PhpPlanRefusal('changes-entry');
        }
        $moduleId = php_module_of_path($manifest['modules'], $path);
        if ($moduleId === null) {
            $uncertainties[] = [
                'kind' => 'unknown',
                'detail' => 'changed path without module attribution: ' . substr($path, 0, 128),
            ];
            continue;
        }
        $changedModules[] = $moduleId;
    }
    foreach ((is_array($changes['symbols'] ?? null) ? $changes['symbols'] : []) as $symbol) {
        if (!is_string($symbol)) {
            throw new PhpPlanRefusal('changes-entry');
        }
        $uncertainties[] = [
            'kind' => 'unknown',
            'detail' => 'changed symbol without module attribution: ' . substr($symbol, 0, 128),
        ];
    }
    $affected = php_affected_closure($manifest['edges'], $changedModules);
    $affectedIds = array_map(static fn (array $entry): string => $entry['package_id'], $affected);
    // The plan workspace has exactly one package (the Composer root):
    // affected entries stay package-scoped, and the module provenance
    // travels in the closed reason kinds (source_ref names the module).
    $packageAffectedReasons = [];
    $seenReasons = [];
    foreach ($affected as $entry) {
        foreach ($entry['reasons'] as $reason) {
            $kind = $reason['kind'] === 'changed-module' ? 'changed-package' : $reason['kind'];
            $key = $kind . "\n" . $reason['source_ref'];
            if (isset($seenReasons[$key])) {
                continue;
            }
            $seenReasons[$key] = true;
            $packageAffectedReasons[] = [
                'kind' => $kind,
                'source_ref' => substr($reason['source_ref'], 0, 512),
                'edge_path' => array_slice($reason['edge_path'], 0, 32),
            ];
        }
    }
    $packageAffected = [];
    if ($packageAffectedReasons !== []) {
        // Package-scoped reasons carry module provenance in source_ref;
        // the edge_path member stays absent (its hops are package ids,
        // and the single-root workspace has no package hops to name).
        $packageAffected[] = ['package_id' => $packageId, 'reasons' => array_map(
            static function (array $reason): array {
                unset($reason['edge_path']);
                return $reason;
            },
            $packageAffectedReasons,
        )];
    }

    // Gate selection: affected modules' bound gates unioned with the
    // mandatory cross-module gates, joined per stable gate id, exactly
    // once. An aggregate gate that covers a suite suppresses the leaf
    // gates covering the same suites (exactly-once execution).
    $confirmedByGate = [];
    foreach ($confirmed as $tuple) {
        if ($tuple['package_id'] !== $packageId) {
            continue;
        }
        $confirmedByGate[$tuple['gate_id']] = $tuple;
    }
    $selectedGateIds = [];
    $excluded = [];
    $moduleGaps = [];
    $selectionMode = 'targeted';
    $fallbackRuleRef = null;
    foreach ($affectedIds as $moduleId) {
        $module = null;
        foreach ($manifest['modules'] as $candidate) {
            if ($candidate['id'] === $moduleId) {
                $module = $candidate;
                break;
            }
        }
        if ($module === null) {
            $moduleGaps[] = 'affected module without declared gates: ' . $moduleId;
            continue;
        }
        if ($module['gates'] === []) {
            $moduleGaps[] = 'affected module without confirmed gate: ' . $moduleId;
            continue;
        }
        foreach ($module['gates'] as $gateId) {
            if (!isset($confirmedByGate[$gateId])) {
                $moduleGaps[] = 'gate without confirmed recipe: ' . $gateId;
                continue;
            }
            $selectedGateIds[$gateId] = true;
        }
    }
    foreach ($moduleGaps as $gap) {
        $uncertainties[] = ['kind' => 'unknown', 'detail' => substr($gap, 0, 256), 'package_id' => $packageId];
    }
    // A mandatory gate without a confirmed recipe is a blocker, never
    // a silent omission.
    foreach ($manifest['mandatory_gate_ids'] as $gateId) {
        if (!isset($confirmedByGate[$gateId])) {
            throw new PhpPlanRefusal('mandatory-gate-unconfirmed-' . substr($gateId, 0, 48));
        }
    }
    // Exactly-once suites: each bound suite is owned by the covering
    // gate with the widest coverage (the aggregate), ties broken by
    // the smallest gate id; leaf gates bound to a suite already owned
    // by another selected gate are suppressed — the suite executes
    // exactly once (issue acceptance: no double-run).
    $coveringBySuite = [];
    foreach (array_keys($selectedGateIds) as $gateId) {
        foreach ($confirmedByGate[$gateId]['covers_suite_ids'] as $suiteId) {
            $coveringBySuite[$suiteId][] = $gateId;
        }
    }
    $coveredSuites = [];
    foreach ($coveringBySuite as $suiteId => $gateIds) {
        $gateIds = array_values(array_unique($gateIds));
        usort($gateIds, static function (string $left, string $right) use ($confirmedByGate): int {
            $leftCount = count($confirmedByGate[$left]['covers_suite_ids']);
            $rightCount = count($confirmedByGate[$right]['covers_suite_ids']);
            return $rightCount <=> $leftCount ?: strcmp($left, $right);
        });
        $coveredSuites[$suiteId] = $gateIds[0];
    }
    $suiteOwnerByGate = [];
    foreach ($manifest['tests'] as $test) {
        $suiteOwnerByGate[$test['gate_id']][] = $test['suite_id'];
    }
    foreach (array_keys($selectedGateIds) as $gateId) {
        foreach ($suiteOwnerByGate[$gateId] ?? [] as $suiteId) {
            $owner = $coveredSuites[$suiteId] ?? null;
            if ($owner !== null && $owner !== $gateId) {
                // The suite already rides the covering aggregate gate.
                unset($selectedGateIds[$gateId]);
                break;
            }
        }
    }
    // Mandatory cross-module gates ride every targeted selection that
    // executes anything at all (an empty selection claims no coverage,
    // so it cannot name unexecuted mandatory gates either).
    if ($selectedGateIds !== [] && $selectionMode === 'targeted') {
        foreach ($manifest['mandatory_gate_ids'] as $gateId) {
            $selectedGateIds[$gateId] = true;
        }
    }

    // Explicit full fallback: only a checked release-full rule with a
    // recorded digest expands the selection to the full confirmed
    // inventory, and the expansion enumerates every gate — never a
    // label over the affected list.
    $fallbackRule = $policy['fallback_rule'] ?? ['mode' => 'none'];
    if (is_array($fallbackRule) && ($fallbackRule['mode'] ?? null) === 'release-full') {
        $ruleDigest = $fallbackRule['rule_digest'] ?? null;
        if (!is_string($ruleDigest) || !preg_match('/^sha256:[0-9a-f]{64}$/', $ruleDigest)) {
            throw new PhpPlanRefusal('fallback-rule-digest');
        }
        $selectionMode = 'release-full';
        $fallbackRuleRef = $ruleDigest;
        foreach ($confirmedByGate as $gateId => $tuple) {
            $selectedGateIds[$gateId] = true;
        }
    }
    ksort($selectedGateIds);
    // An empty selection is an honest blocked plan, never a refusal:
    // nothing is affected, nothing executes, and the plan claims no
    // coverage (the mandatory gate ids ride only selections that run).
    $emptySelection = $selectedGateIds === [];
    if ($emptySelection) {
        $excluded = [['package_id' => $packageId, 'reason' => 'no-reason']];
    }

    // Commands: deterministic order by gate id; every command carries
    // the pinned selection reference and its covered suites.
    $selection = [
        'mode' => $selectionMode,
        'modules' => array_values(array_unique(array_merge(
            $affectedIds,
            array_map(static function (string $gateId) use ($manifest, $packageId): string {
                // Modules of mandatory gates ride the selection too.
                foreach ($manifest['modules'] as $module) {
                    if (in_array($gateId, $module['gates'], true)) {
                        return $module['id'];
                    }
                }
                return 'root';
            }, array_keys($selectedGateIds)),
        ))),
        'tests' => [],
        'mandatory_gate_ids' => $emptySelection ? [] : $manifest['mandatory_gate_ids'],
        'excluded' => $excluded,
        'uncertainties' => $uncertainties,
        'fallback_rule_ref' => $fallbackRuleRef,
    ];
    sort($selection['modules']);
    $suiteIds = [];
    foreach ($manifest['tests'] as $test) {
        if (isset($selectedGateIds[$test['gate_id']])) {
            $suiteIds[$test['suite_id']] = true;
        }
    }
    $selection['tests'] = array_keys($suiteIds);
    sort($selection['tests']);
    $selectionRef = php_selection_digest($selection);

    // Commands: deterministic order by gate id; every command carries
    // the pinned selection reference and its covered suites. The
    // reason provenance distinguishes affected-module gates, mandatory
    // cross-module gates, and full-fallback expansion.
    $gateModule = [];
    foreach ($manifest['modules'] as $module) {
        foreach ($module['gates'] as $gateId) {
            $gateModule[$gateId] = $module['id'];
        }
    }
    $commands = [];
    foreach (array_keys($selectedGateIds) as $gateId) {
        $tuple = $confirmedByGate[$gateId];
        $reasonRef = $packageId . '#explicit-binding';
        foreach ($affected as $entry) {
            if (($gateModule[$gateId] ?? null) === $entry['package_id']) {
                $reasonRef = $packageId . '#' . $entry['reasons'][0]['kind'];
                break;
            }
        }
        if (!isset($gateModule[$gateId]) && $fallbackRuleRef !== null) {
            $reasonRef = $packageId . '#release-rule';
        }
        $commands[] = [
            'id' => 'gate-' . $gateId,
            'package_id' => $packageId,
            'gate' => $tuple['gate'],
            'gate_id' => $gateId,
            'gate_kind' => $tuple['gate_kind'],
            'required' => $tuple['required'],
            'selection_ref' => $selectionRef,
            'covers_suite_ids' => $tuple['covers_suite_ids'],
            'script_name' => $tuple['script_name'],
            'script_digest' => $tuple['script_digest'],
            'confirmation_ref' => $tuple['confirmation_ref'],
            'cwd' => $tuple['cwd'],
            'tool_ref' => $tuple['tool_ref'],
            'argv' => $tuple['argv'],
            'env' => [],
            'depends_on' => [],
            'affected_reason_refs' => [$reasonRef],
            'read_manifest_ref' => (string) $custody['input_manifest_digest'],
            'allowed_writes' => ['mode' => 'stage-only'],
            'limits' => $policy['limits'],
        ];
    }
    // Prerequisite ordering rides the declared module edges: an edge
    // is consumer -> dependency (the plan workspace convention), so a
    // command of a consuming module depends on every selected command
    // of its dependency modules (bounded transitive chain).
    $commandsByModule = [];
    foreach ($commands as $index => $command) {
        $commandsByModule[($gateModule[$command['gate_id']] ?? 'root')][] = $index;
    }
    foreach ($commands as $index => $command) {
        $moduleId = $gateModule[$command['gate_id']] ?? null;
        if ($moduleId === null) {
            continue;
        }
        $prerequisites = [];
        $queue = [$moduleId];
        $visited = [];
        while ($queue !== []) {
            $current = array_shift($queue);
            if (isset($visited[$current])) {
                continue;
            }
            $visited[$current] = true;
            foreach ($manifest['edges'] as $edge) {
                if ($edge['from'] === $current && !isset($visited[$edge['to']])) {
                    // The dependency side of the edge.
                    $queue[] = $edge['to'];
                    foreach ($commandsByModule[$edge['to']] ?? [] as $prerequisiteIndex) {
                        $prerequisites[] = $commands[$prerequisiteIndex]['id'];
                    }
                }
            }
        }
        $prerequisites = array_values(array_unique(array_diff($prerequisites, [$command['id']])));
        sort($prerequisites);
        if ($prerequisites !== []) {
            $commands[$index]['depends_on'] = array_slice($prerequisites, 0, 128);
        }
    }

    $toolCatalog = $input['tool_catalog'] ?? [];
    $plan = [
        'schema_version' => PHP_PLAN_SCHEMA_VERSION,
        'kind' => 'native-plan',
        'plan_digest' => 'sha256:' . str_repeat('0', 64),
        'adapter' => $custody['adapter_identity'],
        'planner_version' => PHP_PLANNER_VERSION,
        'canonicalization_version' => PHP_CANONICALIZATION_VERSION,
        'repository_role' => $policy['repository_role'],
        'trust' => $policy['trust'],
        'authority_ref' => $policy['authority_ref'],
        'policy_ref' => $policy['policy_ref'],
        'classification_ref' => $policy['classification_ref'],
        'execution_policy_ref' => [
            'id' => $policy['identity']['id'],
            'version' => $policy['identity']['version'],
            'digest' => $policy['policy_digest'],
        ],
        'profile_ref' => [
            'id' => $custody['profile_id'],
            'digest' => $custody['profile_digest'],
        ],
        'input_manifest_digest' => (string) $custody['input_manifest_digest'],
        'scan_ref' => [
            'id' => 'scan',
            'version' => PHP_PLANNER_VERSION,
            'digest' => (string) $custody['scan_ref'],
        ],
        'observed_ref' => [
            'id' => 'observed',
            'version' => PHP_PLANNER_VERSION,
            'digest' => (string) $custody['observed_ref'],
        ],
        'tool_catalog_digest' => (string) $custody['tool_catalog_digest'],
        'capability_snapshot_digest' => (string) $custody['capability_snapshot_digest'],
        'workspace' => [
            'manager' => 'composer-project',
            'declared_version' => 'unknown',
            'compatibility_path' => 'partial',
            'root' => '.',
            'manifest_digest' => $confirmed[0]['manifest_digest'],
            'lock_digest_state' => $input['composer_lock_state'] ?? 'absent',
            'lock_digest' => $input['composer_lock_digest'] ?? null,
            'packages' => [[
                'id' => $packageId,
                'name' => $packageName,
                'root' => '.',
                'manifest_digest' => $confirmed[0]['manifest_digest'],
            ]],
            'edges' => [],
            'completeness' => $manifest['completeness'] === 'complete' && $uncertainties === [] ? 'complete' : 'incomplete',
            'uncertainties' => $uncertainties,
        ],
        'changes' => [
            'files' => array_map(static fn (array $file): array => [
                'path' => (string) $file['path'],
                'change' => in_array($file['change'] ?? null, ['added', 'modified', 'deleted', 'renamed'], true)
                    ? $file['change'] : 'modified',
            ], $files),
            'symbols' => array_values(array_filter(
                is_array($changes['symbols'] ?? null) ? $changes['symbols'] : [],
                'is_string',
            )),
        ],
        'affected' => $packageAffected,
        'excluded' => $excluded,
        'selection_mode' => $selectionMode,
        'selection' => $selection,
        'commands' => $commands,
        'env' => $policy['env_recipe'],
        'tools' => $toolCatalog,
        'required_capabilities' => [PHP_PLAN_CAPABILITY],
        'capabilities' => [[
            'id' => PHP_PLAN_CAPABILITY,
            'definition_version' => PHP_PLANNER_VERSION,
            'state' => 'full',
            'source' => 'declared',
        ]],
        'run_eligibility' => [
            'state' => $emptySelection ? 'blocked' : 'plan-only',
            'reason_codes' => $emptySelection
                ? ['no-commands', 'fixture-runner-not-in-production']
                : ['fixture-runner-not-in-production'],
        ],
        'limits' => $policy['limits'],
        'write_policy' => $policy['write_policy'],
    ];
    $plan['plan_digest'] = php_plan_digest($plan);
    return $plan;
}

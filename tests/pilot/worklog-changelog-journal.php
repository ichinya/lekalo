<?php
// Issue #51 r6: regressions exercise the real application and journal,
// not a copied allocation/transition algorithm. No network or wall clock.
declare(strict_types=1);
define('LEKALO_LANE_LIBRARY_ONLY', true);
require dirname(__DIR__) . '/fixtures/pilot/worklog-changelog/php/run.php';

function check(bool $ok, string $case): void
{
    if (!$ok) {
        throw new RuntimeException($case);
    }
}

function refuses(callable $operation, string $error, string $case): void
{
    try {
        $operation();
    } catch (store_unavailable $failure) {
        check($failure->error === $error, $case . ': ' . $failure->error);
        return;
    }
    throw new RuntimeException($case . ': did not refuse');
}

function app_over(durable_journal $journal, bool $restore = false): application
{
    $app = new application(
        new task_store(), new worklog_store(),
        $restore ? idempotency_store::restore($journal) : new idempotency_store($journal),
        $restore ? provider_outbox::restore($journal) : new provider_outbox($journal),
        new changelog_store(), new fake_transport(), new public_renderer(), $journal,
    );
    $app->tasks->insert('task-A', 'tenant-A', 'workspace-A');
    $app->tasks->insert('task-B', 'tenant-B', 'workspace-B');
    return $app;
}

function request_input(string $key = 'key-A', string $tenant = 'tenant-A', string $task = 'task-A'): array
{
    return [
        'task_id' => $task, 'tenant_id' => $tenant, 'requestor_id' => 'actor-A',
        'correlation_key' => $key, 'payload_digest' => 'payload-' . $key,
    ];
}

$journals = [];
$cases = [];
$new_journal = static function () use (&$journals): durable_journal {
    $journal = new durable_journal();
    $journals[] = $journal;
    return $journal;
};
try {
    // F1: two live generations, distinct scopes/keys, both acknowledged
    // commits survive. Replay and transport preserve each request's identity.
    $j = $new_journal();
    $a = app_over($j);
    $b = app_over($j);
    $one = $a->request_provider_comment(request_input());
    $two = $b->request_provider_comment(request_input('key-B', 'tenant-B', 'task-B'));
    check($one['outcome'] === 'success' && $two['outcome'] === 'success'
        && $one['action_id'] !== $two['action_id'], 'distinct action ids');
    $restored = app_over($j, true);
    check(count($restored->outbox->all()) === 2, 'both committed rows survive');
    check($restored->outbox->get($one['action_id'])['tenant_id'] === 'tenant-A'
        && $restored->outbox->get($two['action_id'])['tenant_id'] === 'tenant-B', 'identity binding');
    $replay = $restored->request_provider_comment(request_input());
    check($replay['outcome'] === 'replayed' && $replay['recorded'] === $one, 'first request replay');
    check($restored->deliver_provider_comment(['action_id' => $one['action_id'], 'tenant_id' => 'tenant-B'])['outcome'] === 'forbidden', 'foreign delivery');
    check($restored->deliver_provider_comment(['action_id' => $one['action_id'], 'tenant_id' => 'tenant-A'])['outcome'] === 'success', 'first request delivery');
    check($restored->deliver_provider_comment(['action_id' => $two['action_id'], 'tenant_id' => 'tenant-B'])['outcome'] === 'success', 'second request delivery');
    $cases[] = 'F1-live-action-allocation';

    // The commit boundary itself re-arbitrates a key even when begin()
    // previously returned fresh in two generations.
    $j = $new_journal();
    $a = app_over($j);
    $b = app_over($j);
    $row = ['tenant_id' => 'tenant-A', 'task_id' => 'task-A', 'requestor_id' => 'actor-A',
        'correlation_key' => 'same', 'payload_digest' => 'body', 'delivery_state' => 'pending'];
    check($a->idempotency->begin('scope', 'same', 'payload') === 'fresh'
        && $b->idempotency->begin('scope', 'same', 'payload') === 'fresh', 'overlapping begin');
    $first = $a->outbox->commit_request($row, 'scope', 'same', 'payload', ['outcome' => 'success'], $a->idempotency);
    $second = $b->outbox->commit_request($row, 'scope', 'same', 'payload', ['outcome' => 'success'], $b->idempotency);
    check($second === ['outcome' => 'replayed', 'recorded' => $first]
        && count(provider_outbox::restore($j)->all()) === 1, 'commit replay arbitration');
    $duplicate = $j->lines()[0];
    $duplicate['row']['scope'] = 'other';
    $j->append($duplicate['kind'], $duplicate['row']);
    refuses(static fn() => provider_outbox::restore($j), 'journal-conflict', 'duplicate committed action id');
    $cases[] = 'F1-commit-arbitration-and-duplicate-refusal';

    // F3: a pre-delivery snapshot must not repeat a confirmed effect.
    $j = $new_journal();
    $seed = app_over($j);
    $request = $seed->request_provider_comment(request_input());
    $left = app_over($j, true);
    $right = app_over($j, true);
    $input = ['action_id' => $request['action_id'], 'tenant_id' => 'tenant-A'];
    check($left->deliver_provider_comment($input)['outcome'] === 'success', 'first delivery');
    check($right->deliver_provider_comment($input)['outcome'] === 'replayed'
        && count($right->transport->attempts) === 0 && count($left->transport->attempts) === 1, 'stale delivered replay');
    check($right->outbox->claim_delivery($request['action_id']) === 'delivered'
        && count($j->lines()) === 3, 'claim acquisition itself refuses duplicate delivery');
    $cases[] = 'F3-stale-confirmed-delivery';

    // F3: unresolved claim and explicit ambiguous acceptance both stop
    // already-live generations before any transport invocation.
    foreach (['claim-only', 'ambiguous'] as $window) {
        $j = $new_journal();
        $seed = app_over($j);
        $request = $seed->request_provider_comment(request_input());
        $left = app_over($j, true);
        $right = app_over($j, true);
        $input = ['action_id' => $request['action_id'], 'tenant_id' => 'tenant-A'];
        if ($window === 'claim-only') {
            check($left->outbox->claim_delivery($request['action_id']) === 'claimed', 'first durable claim');
        } else {
            $left->transport->arm('ambiguous');
            check($left->deliver_provider_comment($input)['error'] === 'taskhub.provider_ambiguous', 'ambiguous transport');
        }
        check($right->outbox->claim_delivery($request['action_id']) === 'unknown', 'atomic outstanding claim');
        check($right->deliver_provider_comment($input)['outcome'] === 'recovery_required'
            && $right->transport->attempts === [], 'stale unknown never transports');
        $cases[] = 'F3-stale-' . $window;
    }

    // F4: restore -> acknowledged new commit -> second restore/replay.
    $j = $new_journal();
    $seed = app_over($j);
    $first = $seed->request_provider_comment(request_input());
    file_put_contents($j->path(), '{"kind":"provider_request","row":', FILE_APPEND);
    $next = app_over($j, true);
    $second = $next->request_provider_comment(request_input('next'));
    check($second['outcome'] === 'success', 'commit after torn tail');
    $restored = app_over($j, true);
    check(count($restored->outbox->all()) === 2, 'second restore after torn tail');
    check($restored->request_provider_comment(request_input())['recorded'] === $first
        && $restored->request_provider_comment(request_input('next'))['recorded'] === $second, 'both requests replay after repair');
    file_put_contents($j->path(), "{not-json}\n", FILE_APPEND);
    refuses(static fn() => app_over($j, true), 'journal-corrupt', 'complete corruption is not truncated');
    $cases[] = 'F4-torn-tail-new-commit-second-restore';

    // F6: missing and unreadable recovery sources are not new journals.
    foreach (['missing', 'unreadable'] as $fault) {
        $j = $new_journal();
        $seed = app_over($j);
        $request = $seed->request_provider_comment(request_input());
        check($request['outcome'] === 'success', 'seed committed journal');
        unlink($j->path());
        if ($fault === 'unreadable') {
            mkdir($j->path()); // Portable unreadable-as-file source, even as administrator.
        }
        refuses(static fn() => $j->raw(), 'journal-read-failed', 'raw unavailable source');
        refuses(static fn() => idempotency_store::restore($j), 'journal-read-failed', 'replay restore unavailable');
        refuses(static fn() => provider_outbox::restore($j), 'journal-read-failed', 'outbox restore unavailable');
        $recovered_identity = new durable_journal($j->path());
        refuses(static fn() => app_over($recovered_identity, true), 'journal-read-failed', 'explicit recovery never creates');
        check($seed->request_provider_comment(request_input()) === ['outcome' => 'error', 'error' => 'journal-read-failed'], 'live cached replay refuses unavailable source');
        check($seed->deliver_provider_comment(['action_id' => $request['action_id'], 'tenant_id' => 'tenant-A'])['outcome'] === 'error'
            && $seed->transport->attempts === [], 'unavailable delivery never transports');
        if ($fault === 'unreadable') {
            rmdir($j->path());
        }
        check(!file_exists($j->path()), 'unavailable source not recreated');
        $cases[] = 'F6-' . $fault . '-journal';
    }

    // F8: malformed transition references and shapes refuse recovery.
    foreach (['delivery_claim', 'outbox-outcome'] as $kind) {
        $j = $new_journal();
        $row = ['action_id' => 'action-ghost'];
        if ($kind === 'outbox-outcome') {
            $row['delivery_state'] = 'delivered';
        }
        $j->append($kind, $row);
        refuses(static fn() => provider_outbox::restore($j), 'journal-corrupt', 'orphan ' . $kind);
        $cases[] = 'F8-orphan-' . $kind;
    }
    $j = $new_journal();
    $j->append('outbox', ['action_id' => 'action-ghost', 'delivery_state' => 'pending']);
    refuses(static fn() => provider_outbox::restore($j), 'journal-corrupt', 'partial outbox row');
    $cases[] = 'F8-malformed-row';

    // F7: known AND unknown spaced scheme spellings share the allowlist.
    $renderer = new public_renderer();
    foreach (['mailto: a@b.c', 'ftp: //h/x', 'intent: x', 'unlisted+scheme: x', 'mailto:a@b.c', 'ftp://h/x'] as $unsafe) {
        check($renderer->check($unsafe) === 'unsafe', 'unknown scheme: ' . $unsafe);
    }
    check($renderer->check('https: //taskhub.synthetic/notes') === 'safe'
        && $renderer->check('https://taskhub.synthetic/notes') === 'safe'
        && $renderer->check('meeting notes released at noon') === 'safe', 'allowlisted and plain text controls');
    $cases[] = 'F7-unknown-spaced-scheme-allowlist';
    echo json_encode(['ok' => true, 'cases' => $cases], JSON_UNESCAPED_SLASHES), PHP_EOL;
} finally {
    foreach ($journals as $journal) {
        $journal->dispose();
    }
}

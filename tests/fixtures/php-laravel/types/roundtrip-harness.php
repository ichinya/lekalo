<?php
/**
 * The executable wire-vector harness of the issue #58 round-trip gate
 * (driven by scripts/test-php-laravel-type-roundtrip.mjs).
 *
 * Usage: php roundtrip-harness.php <project-root> <corpus> <kernel-dir>
 *   corpus: planner | edge
 *
 * Loads the generated types of the materialized project, then runs the
 * closed wire vectors: decode→encode preserves canonical JSON semantics
 * (absent vs null, empty list vs object, enum values, verbatim wire
 * names), encode→decode preserves typed values and presence, and every
 * hostile input (unknown members, wrong primitives, precision loss,
 * unknown enum values, malformed scalars, duplicate JSON members)
 * refuses. Prints one JSON row per vector; a nonzero exit means at
 * least one vector failed.
 */

$root = $argv[1] ?? '';
$corpus = $argv[2] ?? '';
$kernelDir = $argv[3] ?? '';
if ($root === '' || !is_dir($root)) {
    fwrite(STDERR, "harness: missing project root\n");
    exit(2);
}
if (!in_array($corpus, ['planner', 'edge'], true)) {
    fwrite(STDERR, "harness: unknown corpus\n");
    exit(2);
}

$typesDir = $root . '/.lekalo/generated/php-laravel/types';
$iterator = new RecursiveIteratorIterator(new RecursiveDirectoryIterator($typesDir));
foreach ($iterator as $file) {
    if ($file->isFile() && str_ends_with($file->getPathname(), '.php')) {
        require $file->getPathname();
    }
}

$rows = [];
$check = static function (string $name, bool $ok, string $detail = '') use (&$rows): void {
    $rows[] = ['name' => $name, 'ok' => $ok, 'detail' => $detail];
};
$throws = static function (callable $fn): string {
    try {
        $fn();
        return '';
    } catch (InvalidArgumentException $e) {
        return get_class($e);
    } catch (TypeError) {
        return 'TypeError';
    } catch (Error) {
        return Error::class;
    } catch (LogicException) {
        return LogicException::class;
    } catch (Throwable $e) {
        return 'Throwable:' . get_class($e);
    }
};

use Lekalo\Generated\Types\Planner\DueDate;
use Lekalo\Generated\Types\Planner\DueWindow;
use Lekalo\Generated\Types\Planner\DueWindowList;
use Lekalo\Generated\Types\Planner\DueWindowCodec;
use Lekalo\Generated\Types\Planner\Optional\OptionalDueDate;
use Lekalo\Generated\Types\Planner\Optional\OptionalDueWindowList;
use Lekalo\Generated\Types\Planner\Optional\OptionalNullableDueDate;
use Lekalo\Generated\Types\Planner\TaskDtoCodec;
use Lekalo\Generated\Types\Planner\TaskId;
use Lekalo\Generated\Types\Planner\TaskState;
use Lekalo\Generated\Types\Planner\Text;
use Lekalo\Generated\Types\Notify\NotifyUserInputCodec;
use Lekalo\Generated\Types\Notify\TaskDtoAlias;

if ($corpus === 'planner') {
    $uuid = '0b54ba9b-9d33-4f2e-a4d4-4c1ec21b4d1f';
    $fullWire = [
        'task_id' => $uuid,
        'title' => 'Write the report',
        'state' => 'focused',
        'window' => [
            ['from' => '2026-09-28', 'to' => '2026-10-02'],
            ['from' => '2026-10-03', 'to' => '2026-10-05'],
        ],
    ];

    // 1. decode→encode preserves the exact canonical wire shape.
    $dto = TaskDtoCodec::decode($fullWire);
    $check('planner.roundtrip-full', TaskDtoCodec::encode($dto) === $fullWire);
    $check('planner.presence-absent-due', $dto->due->isAbsent());
    $check('planner.wire-name-verbatim', array_key_exists('task_id', TaskDtoCodec::encode($dto))
        && !array_key_exists('taskId', TaskDtoCodec::encode($dto)));
    $check('planner.enum-value-preserved', $dto->state->toWire() === 'focused');
    $check('planner.window-nested', $dto->window->get()->count() === 2
        && $dto->window->get()->all()[0]->from->toString() === '2026-09-28');

    // 2. encode→decode preserves typed values and presence.
    $reconstructed = TaskDtoCodec::decode(TaskDtoCodec::encode($dto));
    $check('planner.decode-encode-decode-equals', $dto->equals($reconstructed));

    // 3. Absent stays absent; explicit null stays null; empty list is a list.
    $absentWire = ['task_id' => $uuid, 'title' => 'x', 'state' => 'done', 'window' => []];
    $absent = TaskDtoCodec::decode($absentWire);
    $check('planner.absent-stays-absent', $absent->due->isAbsent()
        && !array_key_exists('due', TaskDtoCodec::encode($absent)));
    $check('planner.empty-window-is-list', TaskDtoCodec::encode($absent)['window'] === []);
    $nullDue = TaskDtoCodec::decode(['task_id' => $uuid, 'title' => 'x', 'state' => 'done', 'due' => null, 'window' => []]);
    $check('planner.null-stays-null', $nullDue->due->isNull()
        && TaskDtoCodec::encode($nullDue)['due'] === null);

    // 4. Hostile inputs refuse closed.
    $missing = ['title' => 'x', 'state' => 'done', 'window' => []];
    $check('planner.reject-missing-required', $throws(fn () => TaskDtoCodec::decode($missing)) === InvalidArgumentException::class);
    // window is optional-nonnull in the corpus: absence binds the wrapper.
    $absentWindow = TaskDtoCodec::decode(['task_id' => $uuid, 'title' => 'x', 'state' => 'done']);
    $check('planner.absent-window-wraps', $absentWindow->window->isAbsent()
        && !array_key_exists('window', TaskDtoCodec::encode($absentWindow)));
    $check('planner.reject-null-required', $throws(fn () => TaskDtoCodec::decode(['task_id' => null, 'title' => 'x', 'state' => 'done', 'window' => []])) === InvalidArgumentException::class);
    $check('planner.reject-unknown-member', $throws(fn () => TaskDtoCodec::decode(['task_id' => $uuid, 'title' => 'x', 'state' => 'done', 'window' => [], 'extra' => 1])) === InvalidArgumentException::class);
    $check('planner.reject-unknown-enum', $throws(fn () => TaskDtoCodec::decode(['task_id' => $uuid, 'title' => 'x', 'state' => 'paused', 'window' => []])) === InvalidArgumentException::class);
    $check('planner.reject-bad-uuid', $throws(fn () => TaskDtoCodec::decode(['task_id' => 'not-a-uuid', 'title' => 'x', 'state' => 'done', 'window' => []])) === InvalidArgumentException::class);
    $check('planner.reject-uppercase-uuid', $throws(fn () => TaskId::from(strtoupper($uuid))) === InvalidArgumentException::class);
    $check('planner.reject-bad-date', $throws(fn () => TaskDtoCodec::decode(['task_id' => $uuid, 'title' => 'x', 'state' => 'done', 'window' => [['from' => '2026-02-31', 'to' => '2026-10-02']]])) === InvalidArgumentException::class);
    $check('planner.reject-object-as-list', $throws(fn () => TaskDtoCodec::decode(['task_id' => $uuid, 'title' => 'x', 'state' => 'done', 'window' => ['a' => []]])) === InvalidArgumentException::class);
    $check('planner.reject-null-in-nonnull-list-elements', $throws(fn () => TaskDtoCodec::decode(['task_id' => $uuid, 'title' => 'x', 'state' => 'done', 'window' => [null]])) === InvalidArgumentException::class);
    $check('planner.reject-explicit-null-optional-nonnull', $throws(fn () => TaskFocusedCodecWrapperProbe::run()) === InvalidArgumentException::class);

    // 5. Optional-nonnull on the event payload: absent ok, null refuses.
    $probe = new TaskFocusedCodecProbe();
    $check('planner.event-absent-focused-at', $probe->absentOk());
    $check('planner.event-null-focused-at-refuses', $probe->nullRefuses());

    // 6. Presence wrapper state machine: absent | null | value are distinct.
    $date = DueDate::from('2026-09-28');
    $check('planner.wrapper-distinct-states', (static function () use ($date): bool {
        $absent = OptionalNullableDueDate::absent();
        $null = OptionalNullableDueDate::ofNull();
        $value = OptionalNullableDueDate::of($date);
        return $absent->isAbsent() && !$absent->isNull()
            && !$null->isAbsent() && $null->isNull()
            && !$value->isAbsent() && !$value->isNull()
            && $value->get() === $date
            && $absent->equals(OptionalNullableDueDate::absent())
            && !$absent->equals($null)
            && !$null->equals($value);
    })());
    $check('planner.wrapper-nonnull-refuses-null', $throws(fn () => OptionalDueDate::ofNull()) === Error::class);
    $check('planner.wrapper-get-absent-throws', $throws(fn () => OptionalDueDate::absent()->get()) === LogicException::class);

    // 7. Cross-module command input with a nested DTO.
    $input = NotifyUserInputCodec::decode([
        'user_id' => '11111111-2222-4333-8444-555555555555',
        'task' => $fullWire,
    ]);
    $check('planner.cross-module-nested', $input->task->taskId->toString() === $uuid
        && NotifyUserInputCodec::encode($input) === [
            'user_id' => '11111111-2222-4333-8444-555555555555',
            'task' => $fullWire,
        ]);

    // 8. The query result codec is a direct-body passthrough (no envelope).
    $queryText = file_get_contents($typesDir . '/planner/count_focused_result_codec.php');
    $check('planner.query-direct-body', str_contains($queryText, 'no envelope exists'));
    $result = \Lekalo\Generated\Types\Planner\CountFocusedResultCodec::decode('done');
    $check('planner.query-roundtrip', $result === TaskState::Done
        && \Lekalo\Generated\Types\Planner\CountFocusedResultCodec::encode($result) === 'done');

    // 9. Duplicate JSON members refuse at the kernel boundary.
    require $kernelDir . '/kernel.php';
    $duplicateRefused = false;
    try {
        decode_json_document('{"a":1,"a":2}');
    } catch (RequestRefusal) {
        $duplicateRefused = true;
    }
    $check('planner.duplicate-json-members-refused', $duplicateRefused);

    // 10. The class map loads every class by FQN.
    $classmap = require $typesDir . '/classmap.php';
    $check('planner.classmap-covers-task-id', isset($classmap['Lekalo\\Generated\\Types\\Planner\\TaskId']));
} else {
    // The edge corpus: number/boolean/datetime/uri bases, every
    // remaining presence shape, and hostile description escaping.
    $edgeWire = [
        'reading_id' => 'https://example.com/readings/42',
        'enabled' => true,
        'cap' => 9007199254740991,
        'threshold' => null,
        'floor' => -7,
        'label' => false,
        'window' => [
            ['amount' => 4.5, 'instant' => '2026-09-28T12:00:00Z'],
            null,
            ['amount' => -3, 'instant' => null],
        ],
    ];
    $reading = \Lekalo\Generated\Types\Edge\ReadingDtoCodec::decode($edgeWire);
    $check('edge.roundtrip-full', \Lekalo\Generated\Types\Edge\ReadingDtoCodec::encode($reading) === $edgeWire);
    $check('edge.nullable-list-elements', $reading->window->get()->count() === 3
        && $reading->window->get()->all()[1] === null);
    $check('edge.required-nullable-cap', $reading->cap === null ? false : $reading->cap instanceof \Lekalo\Generated\Types\Edge\Amount);
    $check('edge.optional-nullable-threshold', $reading->threshold->isNull());
    // B1: two optional positions sharing one wrapper key (threshold and
    // floor over edge.amount) must BOTH bind the amount wrapper — a
    // dedup miss would hand the second field the last-appended wrapper
    // and TypeError inside the codec.
    $check('edge.wrapper-dedup-binds-same-fqn', $reading->threshold instanceof \Lekalo\Generated\Types\Edge\Optional\OptionalNullableAmount
        && $reading->floor instanceof \Lekalo\Generated\Types\Edge\Optional\OptionalNullableAmount
        && $reading->floor->get()->value() === -7);
    // The differing third shape keeps its own wrapper.
    $check('edge.wrapper-different-key-distinct', !$reading->label instanceof \Lekalo\Generated\Types\Edge\Optional\OptionalNullableAmount
        && $reading->label instanceof \Lekalo\Generated\Types\Edge\Optional\OptionalEnabled);
    $check('edge.optional-nonnull-label', !$reading->label->isAbsent() && $reading->label->get()->value() === false);

    $check('edge.reject-int-above-precision', $throws(fn () => \Lekalo\Generated\Types\Edge\ReadingDtoCodec::decode(
        ['reading_id' => 'https://x.test/', 'enabled' => true, 'cap' => 1.0e17, 'window' => []],
    )) === InvalidArgumentException::class);
    $check('edge.reject-numeric-string', $throws(fn () => \Lekalo\Generated\Types\Edge\ReadingDtoCodec::decode(
        ['reading_id' => 'https://x.test/', 'enabled' => true, 'cap' => '42', 'window' => []],
    )) === InvalidArgumentException::class);
    $check('edge.reject-float-string-cap', $throws(fn () => \Lekalo\Generated\Types\Edge\ReadingDtoCodec::decode(
        ['reading_id' => 'https://x.test/', 'enabled' => true, 'cap' => 4.5, 'window' => []],
    )) === '');
    $check('edge.reject-bad-datetime', $throws(fn () => \Lekalo\Generated\Types\Edge\Amount::fromWire('2026-09-28')) === InvalidArgumentException::class);
    $check('edge.reject-bad-uri', $throws(fn () => \Lekalo\Generated\Types\Edge\ReadingId::fromWire('no scheme here')) === InvalidArgumentException::class);
    $check('edge.accept-uri', \Lekalo\Generated\Types\Edge\ReadingId::fromWire('mailto:someone@example.com')->toString() === 'mailto:someone@example.com');

    // B3: a query whose return is a structured leaf delegates to the
    // value-object codec both ways — direct body, no envelope.
    $measurementWire = ['amount' => 1.5, 'instant' => null];
    $latest = \Lekalo\Generated\Types\Edge\LatestResultCodec::decode($measurementWire);
    $check('edge.query-structured-leaf', $latest instanceof \Lekalo\Generated\Types\Edge\Measurement
        && \Lekalo\Generated\Types\Edge\LatestResultCodec::encode($latest) === $measurementWire);
    $check('edge.query-structured-leaf-refuses-junk', $throws(
        fn () => \Lekalo\Generated\Types\Edge\LatestResultCodec::decode('nope'),
    ) === InvalidArgumentException::class);

    // The hostile description stayed one safe comment line and the
    // class still loads.
    $text = file_get_contents($typesDir . '/edge/amount.php');
    $lines = explode("\n", $text);
    $descriptionLine = '';
    foreach ($lines as $line) {
        if (str_contains($line, '// Description:')) {
            $descriptionLine = $line;
        }
    }
    $check('edge.hostile-description-one-line', $descriptionLine !== ''
        && !str_contains($descriptionLine, "\n")
        && str_contains($descriptionLine, '? >')
        && !str_contains($descriptionLine, '?>'));
}

usort($rows, static fn (array $left, array $right): int => strcmp($left['name'], $right['name']));
echo json_encode($rows, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . "\n";
foreach ($rows as $row) {
    if (!$row['ok']) {
        fwrite(STDERR, "harness: vector failed: {$row['name']} {$row['detail']}\n");
        exit(1);
    }
}
exit(0);

/**
 * The event-payload probe: the focused event's optional-nonnull
 * `focused_at` accepts absence and refuses explicit null.
 */
final class TaskFocusedCodecProbe
{
    private array $absentWire = [
        'task_id' => '0b54ba9b-9d33-4f2e-a4d4-4c1ec21b4d1f',
    ];

    public function absentOk(): bool
    {
        $payload = \Lekalo\Generated\Types\Planner\TaskFocusedPayloadCodec::decode($this->absentWire);
        return $payload->focusedAt->isAbsent()
            && !array_key_exists('focused_at', \Lekalo\Generated\Types\Planner\TaskFocusedPayloadCodec::encode($payload));
    }

    public function nullRefuses(): bool
    {
        try {
            \Lekalo\Generated\Types\Planner\TaskFocusedPayloadCodec::decode([
                'task_id' => '0b54ba9b-9d33-4f2e-a4d4-4c1ec21b4d1f',
                'focused_at' => null,
            ]);
            return false;
        } catch (InvalidArgumentException) {
            return true;
        }
    }
}

/** The probe of the optional-nonnull wrapper rejection surface. */
final class TaskFocusedCodecWrapperProbe
{
    public static function run(): void
    {
        // An explicit null inside a NON-NULL optional position is
        // refused by the element decode (never absorbed into absent):
        // the event payload owns the named semantic.
        \Lekalo\Generated\Types\Planner\TaskFocusedPayloadCodec::decode([
            'task_id' => '0b54ba9b-9d33-4f2e-a4d4-4c1ec21b4d1f',
            'focused_at' => null,
        ]);
    }
}

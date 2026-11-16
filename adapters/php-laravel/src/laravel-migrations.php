<?php
/**
 * The Laravel migration emitter (issue #57), provider-owned since the
 * stage-4 separation (lekalo-php#1, issue #241 M1): the storage
 * transition input contract and the Illuminate migration rendering are
 * framework knowledge and ride the `migrations.emit` capability of the
 * registered Laravel provider. Moved verbatim from the kernel; the
 * kernel resolves the family through the provider registry and never
 * names Laravel itself.
 */

// ---------------------------------------------------------------------------
// 5a. The Laravel migration emitter (issue #57).
// ---------------------------------------------------------------------------

/** The closed step-kind vocabulary the emitter consumes (the plan v1 set). */
const MIGRATION_OPERATION_KINDS = [
    'create_table', 'drop_table', 'rename_table', 'add_column',
    'drop_column', 'alter_column_type', 'set_column_null',
    'set_column_default', 'add_primary_key', 'rename_constraint',
    'drop_constraint', 'add_unique', 'add_foreign_key', 'add_check',
    'drop_check', 'add_index', 'drop_index', 'create_enum_type',
    'alter_enum_type', 'enable_rls', 'disable_rls', 'create_policy',
    'drop_policy', 'create_sequence', 'alter_sequence',
    'rename_sequence', 'drop_sequence', 'create_extension',
    'backfill', 'rename_column',
];

/** The closed rollback classes (the contract spelling). */
const MIGRATION_ROLLBACK_CLASSES = [
    'reversible', 'data-loss-on-rollback', 'irreversible',
];

/** The closed effective-status vocabulary. */
const MIGRATION_STATUSES = ['ready', 'blocked', 'confirmed'];

/** The declared UTC timestamp base of the fixture pipeline (policy, never the wall clock). */
const MIGRATION_TIMESTAMP_BASE = '2026_09_27_000001';

/**
 * Decode and validate one bounded Laravel migration input document.
 * The closed shape mirrors contracts/laravel-migration-input.schema
 * exactly: unknown members refuse, every closed vocabulary is checked,
 * and every digest pin must be a canonical sha256 spelling.
 */
function decode_storage_input(string $bytes): array
{
    if (strlen($bytes) > MAX_REQUEST_BYTES) {
        throw new RequestRefusal('input-too-large');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('input-syntax');
    }
    if (!is_json_object($document)) {
        throw new RequestRefusal('input-shape');
    }
    $required = [
        'schemaVersion', 'identity', 'engine', 'engineVersion',
        'projectId', 'baseDigest', 'candidateDigest', 'diffDigest',
        'planId', 'gated', 'backfillGated', 'effectiveStatus',
        'operations',
    ];
    $known = array_merge($required, ['historyDigest']);
    foreach (array_keys($document) as $key) {
        if (!in_array($key, $known, true)) {
            throw new RequestRefusal('input-member');
        }
    }
    foreach ($required as $key) {
        if (!array_key_exists($key, $document)) {
            throw new RequestRefusal('input-member');
        }
    }
    if ($document['schemaVersion'] !== 'lekalo/laravel-migration-input/v0.4.0'
        || $document['identity'] !== 'dev.lekalo.laravel-migration-input@0.4.0') {
        throw new RequestRefusal('input-identity');
    }
    if ($document['engine'] !== 'postgres'
        || !is_contract_version($document['engineVersion'])) {
        throw new RequestRefusal('input-engine');
    }
    foreach (['baseDigest', 'candidateDigest', 'diffDigest', 'planId'] as $key) {
        if (!is_sha256_digest($document[$key])) {
            throw new RequestRefusal('input-digest');
        }
    }
    if (array_key_exists('historyDigest', $document)
        && !is_sha256_digest($document['historyDigest'])) {
        throw new RequestRefusal('input-digest');
    }
    if (!is_bool($document['gated']) || !is_bool($document['backfillGated'])) {
        throw new RequestRefusal('input-shape');
    }
    if (!in_array($document['effectiveStatus'], MIGRATION_STATUSES, true)) {
        throw new RequestRefusal('input-status');
    }
    // The effective gate closes: a blocked document generates zero
    // bytes. The refusal is the honest state, never a partial file.
    if ($document['effectiveStatus'] === 'blocked') {
        throw new RequestRefusal('input-gated');
    }
    if (!is_array($document['operations']) || !array_is_list($document['operations'])
        || count($document['operations']) > 1024) {
        throw new RequestRefusal('input-operations');
    }
    $previous = 0;
    foreach ($document['operations'] as $operation) {
        if (!is_json_object($operation)) {
            throw new RequestRefusal('input-operation');
        }
        $operationKnown = ['ordinal', 'kind', 'statement', 'risk', 'requires', 'rollback', 'inverse'];
        $operationRequired = ['ordinal', 'kind', 'statement', 'risk', 'rollback'];
        foreach (array_keys($operation) as $key) {
            if (!in_array($key, $operationKnown, true)) {
                throw new RequestRefusal('input-member');
            }
        }
        foreach ($operationRequired as $key) {
            if (!array_key_exists($key, $operation)) {
                throw new RequestRefusal('input-operation');
            }
        }
        if (!is_int($operation['ordinal']) || $operation['ordinal'] <= $previous) {
            throw new RequestRefusal('input-order');
        }
        $previous = $operation['ordinal'];
        if (!in_array($operation['kind'], MIGRATION_OPERATION_KINDS, true)) {
            throw new RequestRefusal('input-kind');
        }
        if (!is_string($operation['statement']) || $operation['statement'] === ''
            || strlen($operation['statement']) > 4096) {
            throw new RequestRefusal('input-statement');
        }
        if (!in_array($operation['risk'], ['none', 'backfill_required', 'destructive'], true)) {
            throw new RequestRefusal('input-risk');
        }
        if (array_key_exists('requires', $operation)) {
            if (!is_array($operation['requires']) || !array_is_list($operation['requires'])) {
                throw new RequestRefusal('input-operation');
            }
            foreach ($operation['requires'] as $requirement) {
                if (!is_int($requirement) || $requirement < 1
                    || $requirement >= $operation['ordinal']) {
                    throw new RequestRefusal('input-requires');
                }
            }
        }
        if (!in_array($operation['rollback'], MIGRATION_ROLLBACK_CLASSES, true)) {
            throw new RequestRefusal('input-rollback');
        }
        if (array_key_exists('inverse', $operation)
            && (!is_string($operation['inverse']) || $operation['inverse'] === ''
                || strlen($operation['inverse']) > 4096)) {
            throw new RequestRefusal('input-inverse');
        }
        // A reversible operation carries its inverse: the plan v1
        // metadata fully determines it, so the absence would be a
        // truncated document, and an irreversible or data-loss class
        // never carries one.
        $hasInverse = array_key_exists('inverse', $operation);
        if (($operation['rollback'] === 'reversible') !== $hasInverse) {
            throw new RequestRefusal('input-inverse');
        }
    }
    return $document;
}

/**
 * Emit the deterministic Laravel migration artifacts for one bounded
 * input: one PHP class whose up() executes exactly the planned SQL in
 * dependency order and whose down() either inverts a fully reversible
 * plan or refuses before any statement, plus the append-only ledger.
 * Two runs with identical inputs are byte-identical; the artifact set
 * never contains anything but the migration file and the ledger.
 */
function emit_laravel_migrations(array $input, string $target, string $irPath): array
{
    // The effective gate closes here too: emit never renders a blocked
    // transition, defense in depth against a bypassed decode.
    if (($input['effectiveStatus'] ?? '') === 'blocked') {
        throw new RequestRefusal('input-gated');
    }
    $shortDigest = substr(sha256_hex(canonical_json($input)), 0, 12);
    $class = 'Lekalo' . strtoupper(substr(sha256_hex($shortDigest), 0, 8)) . 'Migration';
    $directory = '.lekalo/generated/php-laravel/' . $target . '/migrations';
    $filename = MIGRATION_TIMESTAMP_BASE . '_lekalo_' . $shortDigest . '.php';
    $path = $directory . '/' . $filename;
    $bytes = render_migration_class($input, $class, $target, $irPath);
    $ledgerPath = $directory . '/ledger.json';
    $entry = [
        'filename' => $filename,
        'digest' => sha256_digest($bytes),
        'bytes' => strlen($bytes),
        'planId' => $input['planId'],
        'inputDigest' => sha256_digest(canonical_json($input)),
        'baseDigest' => $input['baseDigest'],
        'candidateDigest' => $input['candidateDigest'],
        'timestampBase' => MIGRATION_TIMESTAMP_BASE,
        'gated' => $input['gated'],
        'backfillGated' => $input['backfillGated'],
        'effectiveStatus' => $input['effectiveStatus'],
    ];
    // The ledger is append-only custody: a published ledger never
    // shrinks and never rewrites an entry. The emitter reads the
    // published document, proves the append with assert_append_only,
    // and writes the merged ledger — so a second generate --apply
    // appends instead of dying write-denied on the existing file, and
    // a byte-identical regeneration replans exactly the published
    // bytes (a true no-op at apply time).
    $published = read_published_ledger($ledgerPath);
    if ($published === null) {
        $ledger = [
            'schemaVersion' => 'lekalo/laravel-migration-ledger/v0.4.0',
            'identity' => 'dev.lekalo.laravel-migration-ledger@0.4.0',
            'projectId' => $input['projectId'],
            'engine' => $input['engine'],
            'engineVersion' => $input['engineVersion'],
            'migrations' => [[
                'ordinal' => 1,
            ] + $entry],
        ];
        $ledgerBytes = canonical_json($ledger) . "\n";
        $ledgerAction = 'create';
    } else {
        $publishedMigrations = $published['migrations'];
        $alreadyPublished = null;
        $highestOrdinal = 0;
        foreach ($publishedMigrations as $publishedEntry) {
            $highestOrdinal = max($highestOrdinal, (int) $publishedEntry['ordinal']);
            if (($publishedEntry['filename'] ?? null) === $filename) {
                $alreadyPublished = $publishedEntry;
            }
        }
        if ($alreadyPublished !== null && $alreadyPublished['digest'] !== $entry['digest']) {
            // The same published filename with different bytes is a
            // custody violation: a migration file is named by its
            // input digest, so the published class must be identical.
            throw new RequestRefusal('ledger-conflict');
        }
        if ($alreadyPublished !== null) {
            // Byte-identical regeneration: the entry is already
            // published, the ledger keeps its published bytes, and the
            // planned write replays them exactly (no-op on disk).
            $ledgerBytes = file_get_contents($ledgerPath);
            if ($ledgerBytes === false) {
                throw new RequestRefusal('ledger-shape');
            }
            $ledgerAction = 'create';
        } else {
            $entry = ['ordinal' => $highestOrdinal + 1] + $entry;
            $merged = $published;
            $merged['migrations'][] = $entry;
            assert_append_only($published, $merged);
            $ledgerBytes = canonical_json($merged) . "\n";
            $ledgerAction = 'replace';
        }
    }
    return [
        'path' => $path,
        'bytes' => $bytes,
        'digest' => sha256_digest($bytes),
        'action' => 'create',
        'ledger' => [
            'path' => $ledgerPath,
            'bytes' => $ledgerBytes,
            'digest' => sha256_digest($ledgerBytes),
            'action' => $ledgerAction,
        ],
        'filename' => $filename,
    ];
}

/**
 * The published ledger of one migrations directory, or null when this
 * adapter has not published one yet. A present-but-invalid ledger
 * refuses: append-only custody cannot be proven over a corrupt
 * document, and silently starting a fresh ledger would orphan every
 * published migration.
 */
function read_published_ledger(string $path): ?array
{
    if (!is_file($path)) {
        return null;
    }
    $bytes = file_get_contents($path);
    if ($bytes === false) {
        throw new RequestRefusal('ledger-shape');
    }
    try {
        $document = json_decode($bytes, true, 64, JSON_THROW_ON_ERROR);
    } catch (JsonException) {
        throw new RequestRefusal('ledger-shape');
    }
    if (!is_json_object($document)
        || !isset($document['migrations'])
        || !is_array($document['migrations'])
        || !array_is_list($document['migrations'])) {
        throw new RequestRefusal('ledger-shape');
    }
    return $document;
}

/**
 * Render one Laravel migration class: strict types, an anonymous
 * class extending Illuminate\Database\Migrations\Migration, up() as
 * sequential DB::statement calls of the planned SQL, and down() from
 * the typed reverse plan or an explicit refusal. The SQL travels as
 * single-quoted PHP literals byte-for-byte — quotes, backslashes,
 * dollar signs, and Unicode survive exactly — never through eval or
 * string interpolation.
 */
function render_migration_class(array $input, string $class, string $target, string $irPath): string
{
    $lines = [];
    $lines[] = '<?php';
    $lines[] = '';
    $lines[] = '// generated by ' . ADAPTER_ID . '@' . ADAPTER_VERSION . ' (issue #57)';
    $lines[] = 'declare(strict_types=1);';
    $lines[] = '';
    $lines[] = '// target: ' . $target;
    $lines[] = '// engine: postgres ' . $input['engineVersion'];
    $lines[] = '// project: ' . $input['projectId'];
    $lines[] = '// plan: ' . $input['planId'];
    $lines[] = '// input: ' . $irPath;
    $lines[] = '';
    $lines[] = 'use Illuminate\Database\Migrations\Migration;';
    $lines[] = 'use Illuminate\Database\Schema\Blueprint;' ;
    $lines[] = 'use Illuminate\Support\Facades\DB;';
    $lines[] = '';
    $lines[] = '/**';
    $lines[] = ' * Lekalo storage transition: ' . count($input['operations']) . ' operation(s),';
    $lines[] = ' * effective gate ' . $input['effectiveStatus'] . '.';
    $lines[] = ' */';
    $lines[] = 'return new class extends Migration';
    $lines[] = '{';
    $lines[] = '    /** Execute the planned transition in dependency order. */';
    $lines[] = '    public function up(): void';
    $lines[] = '    {';
    foreach ($input['operations'] as $operation) {
        $literal = php_single_quote($operation['statement']);
        $lines[] = '        DB::statement(\'' . $literal . '\');';
    }
    $lines[] = '    }';
    $lines[] = '';
    $reverses = [];
    foreach ($input['operations'] as $operation) {
        $reverses[] = $operation['rollback'] === 'reversible'
            ? $operation['inverse']
            : null;
    }
    $fullyReversible = !in_array(null, $reverses, true);
    $lines[] = '    /** Reverse the transition; refuses before any statement when unsafe. */';
    $lines[] = '    public function down(): void';
    $lines[] = '    {';
    if ($fullyReversible) {
        foreach (array_reverse($reverses) as $inverse) {
            $literal = php_single_quote($inverse);
            $lines[] = '        DB::statement(\'' . $literal . '\');';
        }
    } else {
        $lines[] = '        // The plan carries irreversible or data-loss-on-rollback';
        $lines[] = '        // operations: the rollback refuses before the first statement.';
        $lines[] = '        throw new \RuntimeException(\'lekalo: rollback is not safe for this migration\');';
    }
    $lines[] = '    }';
    $lines[] = '};';
    return implode("\n", $lines) . "\n";
}

/**
 * One single-quoted PHP string literal preserving the exact SQL bytes:
 * backslash and single quote escape, everything else travels raw
 * (dollar signs, double quotes, Unicode). No eval, no interpolation.
 */
function php_single_quote(string $sql): string
{
    return str_replace(["\\", "'"], ['\\\\', "\\'"], $sql);
}

/**
 * The append-only custody check: a published migration ledger never
 * loses entries and never rewrites a published file. An existing
 * ledger accepts only a byte-identical regeneration (no-op) or an
 * append with a strictly higher ordinal; any other shape refuses.
 */
function assert_append_only(array $existing, array $next): void
{
    if (!is_json_object($existing)
        || !isset($existing['migrations'])
        || !is_array($existing['migrations'])
        || !array_is_list($existing['migrations'])) {
        throw new RequestRefusal('ledger-shape');
    }
    $published = $existing['migrations'];
    $nextMigrations = $next['migrations'];
    $count = count($published);
    if (count($nextMigrations) < $count) {
        throw new RequestRefusal('ledger-shrunk');
    }
    foreach ($published as $index => $entry) {
        if ($nextMigrations[$index] !== $entry) {
            throw new RequestRefusal('ledger-rewritten');
        }
    }
    $previous = 0;
    foreach ($nextMigrations as $entry) {
        if (!is_int($entry['ordinal']) || $entry['ordinal'] <= $previous) {
            throw new RequestRefusal('ledger-order');
        }
        $previous = $entry['ordinal'];
    }
}

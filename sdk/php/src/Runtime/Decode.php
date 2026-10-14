<?php
declare(strict_types=1);
// Strict wire decoding and semantic validation (issue #106): the
// hand-maintained codec obligation over the generated vocabulary.

namespace Lekalo\AdapterSdk\Runtime;

use Lekalo\AdapterSdk\Generated\Protocol;
use Lekalo\AdapterSdk\Generated\Capabilities;

final class Decode {
    public const LINT_VERSION = '0.6.4';

    /** UTF-8 code-point count without ext-mbstring (issue #106 review
     * r6 #2): production `bounded()` counts `chars().count()`, and the
     * wire is UTF-8, so invalid-byte inputs are already refused at the
     * parse layer and every counted string is well-formed UTF-8. The
     * byte classes below cover exactly the four well-formed sequence
     * shapes, so counting leading bytes equals counting code points.
     * Without mbstring this stays byte-exact and dependency-free. */
    public static function utf8Length(string $text): int {
        if (function_exists('mb_strlen')) {
            return mb_strlen($text, 'UTF-8');
        }
        $count = 0;
        $length = strlen($text);
        for ($index = 0; $index < $length; $index++) {
            $byte = ord($text[$index]);
            if ($byte < 0x80 || ($byte >= 0xC0 && $byte < 0xF8)) {
                $count++;
            }
        }
        return $count;
    }

    /** The first `$length` UTF-8 code points of `$text` (issue #106
     * review r6 #2): mirrors the mbstring spelling that used to be the
     * only implementation. */
    public static function utf8Prefix(string $text, int $length): string {
        if (function_exists('mb_substr')) {
            return mb_substr($text, 0, $length, 'UTF-8');
        }
        $count = 0;
        $byteLength = strlen($text);
        for ($index = 0; $index < $byteLength; $index++) {
            if ($count >= $length) {
                return substr($text, 0, $index);
            }
            $byte = ord($text[$index]);
            if ($byte < 0x80 || ($byte >= 0xC0 && $byte < 0xF8)) {
                $count++;
            }
        }
        return $text;
    }

    /** The UTF-8 code points of `$text` as a one-character list (the
     * mb_str_split spelling without ext-mbstring, issue #106 review r6
     * #2). */
    public static function utf8Chars(string $text): array {
        if (function_exists('mb_str_split')) {
            return mb_str_split($text, 1, 'UTF-8');
        }
        $chars = [];
        $current = '';
        $pending = 0;
        $byteLength = strlen($text);
        for ($index = 0; $index < $byteLength; $index++) {
            $byte = ord($text[$index]);
            if ($pending > 0 && $byte >= 0x80 && $byte < 0xC0) {
                $current .= $text[$index];
                $pending--;
                continue;
            }
            if ($current !== '') {
                $chars[] = $current;
            }
            $current = $text[$index];
            $pending = $byte >= 0xF0 ? 3 : ($byte >= 0xE0 ? 2 : ($byte >= 0xC0 ? 1 : 0));
        }
        if ($current !== '') {
            $chars[] = $current;
        }
        return $chars;
    }

    /** Raw-lexeme audit of the document under admission (issue #106
     * review r4 #3): member paths whose raw JSON value is an object, and
     * whose raw number lexeme is `-0`. json_decode's associative mapping
     * collapses `{}`/`{"0":…}` to list values and `-0` to int 0 — both
     * distinctions production's typed decoder refuses — so admission
     * consults this map at every list-position and u64-member check.
     * Admission is synchronous and single-threaded; the map is scoped
     * to the in-flight admit* call. */
    private static array $objectPaths = [];
    private static array $zeroPaths = [];

    private static function rawObjectAt(string $path): bool {
        return isset(self::$objectPaths[$path]);
    }

    private static function rawNegativeZeroAt(string $path): bool {
        return isset(self::$zeroPaths[$path]);
    }

    private static function refuse(string $stage, string $reason): array {
        return ['ok' => false, 'stage' => $stage, 'reason' => $reason];
    }

    /** Decode and fully validate one request document. */
    public static function admitRequest(string $text): array {
        try {
            $document = Json::readSingleObject($text, Protocol::BOUNDS['MAX_REQUEST_BYTES']);
            if (!$document['ok']) {
                return self::refuse('decode', $document['reason']);
            }
            self::$objectPaths = array_fill_keys($document['objectPaths'] ?? [], true);
            self::$zeroPaths = array_fill_keys($document['zeroPaths'] ?? [], true);
            $value = $document['value'];
            $structural = self::admitRequestValue($value);
            if ($structural !== null) {
                return $structural;
            }
            $semantic = self::validateRequest($value);
            if ($semantic !== null) {
                return $semantic;
            }
            return ['ok' => true, 'value' => $value];
        } catch (\Throwable) {
            return self::refuse('decode', 'shape');
        } finally {
            self::$objectPaths = [];
            self::$zeroPaths = [];
        }
    }

    /** Decode one response document (structural admission). */
    public static function admitResponse(string $text): array {
        try {
            $document = Json::readSingleObject($text, Protocol::BOUNDS['DEFAULT_MAX_OUTPUT_BYTES'], ['capabilities.capabilities' => static fn ($previous, $next): bool => in_array($previous, Protocol::ENUMS['supportState'], true) && in_array($next, Protocol::ENUMS['supportState'], true)]);
            if (!$document['ok']) {
                return self::refuse('decode', $document['reason']);
            }
            self::$objectPaths = array_fill_keys($document['objectPaths'] ?? [], true);
            self::$zeroPaths = array_fill_keys($document['zeroPaths'] ?? [], true);
            $value = $document['value'];
            $structural = self::admitResponseValue($value);
            if ($structural !== null) {
                return $structural;
            }
            return ['ok' => true, 'value' => $value];
        } catch (\Throwable) {
            return self::refuse('decode', 'shape');
        } finally {
            self::$objectPaths = [];
            self::$zeroPaths = [];
        }
    }

    /** Serde's typed-struct decoding accepts BOTH spellings at every
     * closed wire struct: the member map and, via `visit_seq`, the
     * positional sequence in declared field order. A seq shorter than
     * the field list is prefix-tolerant (trailing members are absent or
     * defaulted); a longer one is serde's invalid-length `shape` error.
     * The raw-shape audit paths remap so nested list/`-0` checks stay
     * exact, and the seq identity is preserved through `rawObjectAt`:
     * production's `visit_seq` still runs the typed field decoders, so
     * a seq ELEMENT at a list-typed position is an admission refusal,
     * never a collapse (issue #106 review r6 #1). */
    private static function defaultStruct(mixed &$value, array $fields, string $path): bool {
        if (!is_array($value)) {
            return false;
        }
        if (array_is_list($value) && !self::rawObjectAt($path)) {
            return self::seqAsMembers($value, $fields, $path);
        }
        return $value === [] || self::isClosedObject($value, $fields);
    }

    /** Rewrite one positional sequence onto the declared field names
     * (in place). A seq at a struct position is admissible only when it
     * is not an object spelling and its length is inside the field
     * list; every element path remaps from `<path>.<index>` to
     * `<path>.<field>`. */
    private static function seqAsMembers(array &$seq, array $fields, string $path): bool {
        if (count($seq) > count($fields)) {
            return false;
        }
        $mapped = [];
        foreach ($seq as $index => $member) {
            $name = $fields[$index];
            $mapped[$name] = $member;
            foreach (['objectPaths', 'zeroPaths'] as $audit) {
                $prefix = $path === '' ? (string) $index : "{$path}.{$index}";
                foreach (array_keys(self::${$audit}) as $key) {
                    $key = (string) $key;
                    if ($key === $prefix || str_starts_with($key, $prefix . '.')) {
                        $suffix = $key === $prefix ? '' : substr($key, strlen($prefix));
                        self::${$audit}[($path === '' ? $name : "{$path}.{$name}") . $suffix] = true;
                    }
                }
            }
        }
        $seq = $mapped;
        return true;
    }

    /** The struct-position admission shape: a member map (closed) or a
     * positional sequence prefix of the declared fields — never a
     * scalar, never an object-spelled seq, never an over-long seq
     * (serde's missing-field/invalid-length refusals). */
    private static function structValue(mixed $value, array $fields, string $path): bool {
        if (!is_array($value)) {
            return false;
        }
        if (array_is_list($value)) {
            return !self::rawObjectAt($path) && count($value) <= count($fields);
        }
        return self::isClosedObject($value, $fields);
    }

    /** Rebind one admitted struct-position value onto member names for
     * the per-member checks below: sequences remap positionally with
     * the audit-path rewrite, maps pass through unchanged. Returns the
     * (possibly new) value; callers assign it back. Callers guard the
     * shape with `structValue` first. */
    private static function asMembers(mixed &$value, array $fields, string $path): void {
        if (is_array($value) && array_is_list($value) && !self::rawObjectAt($path)) {
            self::seqAsMembers($value, $fields, $path);
        }
    }

    private static function isClosedObject(mixed $value, array $allowed): bool {
        if (!is_array($value) || array_is_list($value)) {
            return false;
        }
        foreach (array_keys($value) as $key) {
            if (!in_array($key, $allowed, true)) {
                return false;
            }
        }
        return true;
    }

    private static function unknownOrShape(mixed $value, array $allowed): string {
        if (!is_array($value) || array_is_list($value)) {
            return 'shape';
        }
        foreach (array_keys($value) as $key) {
            if (!in_array($key, $allowed, true)) {
                return 'unknown-member';
            }
        }
        return 'shape';
    }

    private static function boundedText(mixed $value, int $max): bool {
        return is_string($value) && self::utf8Length($value) <= $max;
    }

    private static function explicitNull(array $value, string $key): bool {
        return array_key_exists($key, $value) && $value[$key] === null;
    }

    /** The typed integer test (issue #106 review r3 #3): production
     * decodes every wire number as serde `u64`, which refuses float
     * lexemes outright — json_decode maps an integer spelling to int
     * and a float spelling (1.0, 1e2) to float, so `is_int` enforces
     * the exact production syntax boundary. */
    private static function isInt(mixed $value): bool {
        return is_int($value);
    }

    /** The integral-number range test; shared with the serve emit ceiling. */
    public static function numberInRange(mixed $value, int $min, int $max): bool {
        return is_int($value) && $value >= $min && $value <= $max;
    }

    private static function isSorted(array $items): bool {
        $count = count($items);
        for ($index = 1; $index < $count; $index++) {
            if (!is_string($items[$index - 1]) || !is_string($items[$index]) || $items[$index - 1] >= $items[$index]) {
                return false;
            }
        }
        return true;
    }

    // ------------------------------------------------------------------
    // Request admission
    // ------------------------------------------------------------------

    private static function admitRequestValue(array &$value): ?array {
        $allowed = ['dry_run', 'ir_path', 'limits', 'lint_request', 'native_request', 'operation', 'plan_id', 'profile', 'profile_capabilities', 'profile_digest', 'project_root', 'protocol', 'protocol_version', 'request_id', 'target'];
        // The declared field order (wire.rs RequestEnvelope) drives the
        // positional-seq mapping; `$allowed` above stays the closed-set.
        $fields = ['protocol', 'protocol_version', 'operation', 'request_id', 'project_root', 'ir_path', 'target', 'profile', 'profile_digest', 'profile_capabilities', 'dry_run', 'limits', 'plan_id', 'native_request', 'lint_request'];
        if (!self::structValue($value, $allowed, '')) {
            return self::refuse('decode', self::unknownOrShape($value, $allowed));
        }
        self::asMembers($value, $fields, '');
        foreach (['protocol', 'protocol_version', 'operation', 'request_id', 'project_root'] as $member) {
            if (!array_key_exists($member, $value)) {
                return self::refuse('decode', 'shape');
            }
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
        }
        if ($value['protocol'] !== Protocol::WIRE_TOKEN) {
            return self::refuse('decode', 'token');
        }
        $version = $value['protocol_version'];
        if (!in_array($version, Protocol::SUPPORTED_PROTOCOL_VERSIONS, true)) {
            return self::refuse('decode', 'version');
        }
        if (!in_array($value['operation'], Protocol::LATEST_OPERATIONS, true)) {
            return self::refuse('decode', 'operation');
        }
        if (!is_string($value['request_id']) || !Ids::isRequestId($value['request_id'])) {
            return self::refuse('decode', 'request-id');
        }
        if ($value['project_root'] !== '.') {
            return self::refuse('decode', 'grammar');
        }
        foreach (['ir_path', 'target', 'profile'] as $member) {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            if (array_key_exists($member, $value)) {
                // Strict typing at every typed call (issue #106 review r5
                // #2/#6): a wrong-typed member is a refusal, never a
                // TypeError fatal under strict_types.
                if (!is_string($value[$member])) {
                    return self::refuse('decode', 'grammar');
                }
                $ok = $member === 'ir_path'
                    ? Paths::isLogicalPath($value[$member])
                    : Paths::isToken($value[$member]);
                if (!$ok) {
                    return self::refuse('decode', 'grammar');
                }
            }
        }
        if (self::explicitNull($value, 'profile_digest')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['profile_digest']) && (!is_string($value['profile_digest']) || !Ids::isSha256Digest($value['profile_digest']))) {
            return self::refuse('decode', 'bounds');
        }
        if (self::explicitNull($value, 'dry_run')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['dry_run']) && !is_bool($value['dry_run'])) {
            return self::refuse('decode', 'shape');
        }
        if (self::explicitNull($value, 'plan_id')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['plan_id']) && (!is_string($value['plan_id']) || !Ids::isPlanId($value['plan_id']))) {
            return self::refuse('decode', 'bounds');
        }
        if (self::explicitNull($value, 'limits')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['limits'])) {
            $limits = $value['limits'];
            if (!self::structValue($limits, ['max_output_bytes', 'timeout_ms'], 'limits')) {
                return self::refuse('decode', 'shape');
            }
            self::asMembers($limits, ['timeout_ms', 'max_output_bytes'], 'limits');
            // Production requires BOTH members whenever `limits` is
            // present (wire.rs: absent or out-of-range is `shape`).
            foreach (['timeout_ms' => ['LIMITS_TIMEOUT_MIN', 'LIMITS_TIMEOUT_MAX'], 'max_output_bytes' => ['LIMITS_OUTPUT_MIN', 'LIMITS_OUTPUT_MAX']] as $member => $bounds) {
                if (self::explicitNull($limits, $member)) {
                    return self::refuse('decode', 'explicit-null');
                }
                if (!array_key_exists($member, $limits)) {
                    return self::refuse('decode', 'shape');
                }
                if (is_float($limits[$member]) || self::rawNegativeZeroAt("limits.{$member}")) {
                    // A float lexeme — or the `-0` integer lexeme — where
                    // production types `u64` is `shape` (issue #106 review
                    // r4 #3): json_decode collapses -0 to int 0, so the
                    // raw-lexeme audit supplies the signed spelling.
                    return self::refuse('decode', 'shape');
                }
                if (!self::numberInRange($limits[$member], Protocol::BOUNDS[$bounds[0]], Protocol::BOUNDS[$bounds[1]])) {
                    return self::refuse('decode', 'bounds');
                }
            }
        }
        if (self::explicitNull($value, 'profile_capabilities')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['profile_capabilities'])) {
            $capabilities = $value['profile_capabilities'];
            if (!is_array($capabilities) || !array_is_list($capabilities) || self::rawObjectAt('profile_capabilities') || count($capabilities) === 0 || count($capabilities) > Protocol::BOUNDS['MAX_DECLARED_CAPABILITIES']) {
                return self::refuse('decode', 'bounds');
            }
            $previous = null;
            foreach ($capabilities as $capability) {
                if (!self::structValue($capability, ['id', 'support'], 'profile_capabilities')) {
                    return self::refuse('decode', 'shape');
                }
                self::asMembers($capability, ['id', 'support'], 'profile_capabilities');
                if (!Paths::isCapabilityId($capability['id'])) {
                    return self::refuse('decode', 'bounds');
                }
                if (!in_array($capability['support'], Protocol::ENUMS['supportState'], true)) {
                    return self::refuse('decode', 'shape');
                }
                if ($previous !== null && strcmp((string) $capability['id'], $previous) <= 0) {
                    return self::refuse('decode', 'bounds');
                }
                $previous = (string) $capability['id'];
            }
        }
        if (self::explicitNull($value, 'native_request')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['native_request'])) {
            $refusal = self::admitNativeRequest($value['native_request']);
            if ($refusal !== null) {
                return $refusal;
            }
        }
        if (self::explicitNull($value, 'lint_request')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['lint_request'])) {
            $refusal = self::admitLintRequest($value['lint_request']);
            if ($refusal !== null) {
                return $refusal;
            }
        }
        return null;
    }

    private static function admitContentRef(mixed &$value, string $path): ?array {
        if (!self::structValue($value, ['adapter', 'digest', 'revision'], $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, ['digest', 'revision', 'adapter'], $path);
        if (!Ids::isSha256Digest($value['digest'] ?? '')) {
            return self::refuse('decode', 'bounds');
        }
        // Revision is an unbounded wire string; the optional adapter identity carries the production
        // adapter token grammar (issue #106 review r4 #2, core wire.rs
        // validate: `!scopes::is_token(adapter)` refuses
        // `native-request`): a present, non-token identity must never
        // reach dispatch.
        if (self::explicitNull($value, 'adapter')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['adapter']) && !Paths::isToken($value['adapter'])) {
            return self::refuse('decode', 'bounds');
        }
        if (self::explicitNull($value, 'revision')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['revision']) && !is_string($value['revision'])) {
            return self::refuse('decode', 'bounds');
        }
        return null;
    }

    private static function admitNativeRequest(mixed &$value): ?array {
        $allowed = ['capability_snapshot_digest', 'changes', 'execution_policy_ref', 'input_manifest_digest', 'observed_ref', 'scan_ref', 'tool_catalog_digest'];
        $fields = ['changes', 'scan_ref', 'observed_ref', 'execution_policy_ref', 'input_manifest_digest', 'tool_catalog_digest', 'capability_snapshot_digest'];
        if (!self::structValue($value, $allowed, 'native_request')) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, $fields, 'native_request');
        foreach (['input_manifest_digest', 'tool_catalog_digest', 'capability_snapshot_digest'] as $member) {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            if (!Ids::isSha256Digest($value[$member] ?? '')) {
                return self::refuse('decode', 'bounds');
            }
        }
        $changes = $value['changes'] ?? null;
        if (!self::defaultStruct($changes, ['files', 'symbols'], 'native_request.changes')) {
            return self::refuse('decode', 'shape');
        }
        // Optional arrays default ONLY when absent (issue #106 review
        // r4 #3): a present member must be a JSON list — the raw-lexeme
        // audit rejects the object spellings json_decode collapses into
        // list values (`{}`, `{"0":…}`) — and explicit null refuses.
        foreach (['files', 'symbols'] as $member) {
            if (self::explicitNull($changes, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            if (array_key_exists($member, $changes)
                && (!is_array($changes[$member]) || !array_is_list($changes[$member]) || self::rawObjectAt("native_request.changes.{$member}"))) {
                return self::refuse('decode', 'bounds');
            }
        }
        $files = $changes['files'] ?? [];
        $symbols = $changes['symbols'] ?? [];
        if (count($files) > 1024 || count($symbols) > 1024) {
            return self::refuse('decode', 'bounds');
        }
        foreach ($files as $index => $change) {
            if (!self::structValue($change, ['after_digest', 'before_digest', 'change', 'path'], "native_request.changes.files.{$index}")) {
                return self::refuse('decode', 'shape');
            }
            self::asMembers($change, ['path', 'change', 'before_digest', 'after_digest'], "native_request.changes.files.{$index}");
            if (!Paths::isLogicalPath($change['path'] ?? '')) {
                return self::refuse('decode', 'bounds');
            }
            if (!in_array($change['change'] ?? null, Protocol::ENUMS['nativeChangeKind'], true)) {
                return self::refuse('decode', 'shape');
            }
            foreach (['after_digest', 'before_digest'] as $member) {
                if (self::explicitNull($change, $member)) {
                    return self::refuse('decode', 'explicit-null');
                }
                if (isset($change[$member]) && !Ids::isSha256Digest($change[$member])) {
                    return self::refuse('decode', 'bounds');
                }
            }
        }
        foreach ($symbols as $symbol) {
            if (!is_string($symbol)) {
                return self::refuse('decode', 'bounds');
            }
        }
        $value["scan_ref"] ??= null; $refusal = self::admitContentRef($value["scan_ref"], "native_request.scan_ref");
        if ($refusal !== null) {
            return $refusal;
        }
        $value["execution_policy_ref"] ??= null; $refusal = self::admitContentRef($value["execution_policy_ref"], "native_request.execution_policy_ref");
        if ($refusal !== null) {
            return $refusal;
        }
        if (self::explicitNull($value, 'observed_ref')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['observed_ref'])) {
            $refusal = self::admitContentRef($value["observed_ref"], "native_request.observed_ref");
            if ($refusal !== null) {
                return $refusal;
            }
        }
        return null;
    }

    private static function admitLintRequest(mixed &$value): ?array {
        $allowed = ['bindings', 'files', 'pins', 'scope'];
        $fields = ['scope', 'pins', 'bindings', 'files'];
        if (!self::structValue($value, $allowed, 'lint_request')) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, $fields, 'lint_request');
        foreach ($allowed as $member) {
            if (!array_key_exists($member, $value)) {
                return self::refuse('decode', 'shape');
            }
        }
        foreach (['scope', 'files', 'bindings'] as $member) {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            // Present members must be JSON lists (issue #106 review r4
            // #3): object spellings refuse instead of collapsing to the
            // empty default.
            if (array_key_exists($member, $value)
                && (!is_array($value[$member]) || !array_is_list($value[$member]) || self::rawObjectAt("lint_request.{$member}"))) {
                return self::refuse('decode', 'bounds');
            }
        }
        $scope = &$value["scope"]; $scope ??= [];
        $files = &$value["files"]; $files ??= [];
        $bindings = &$value["bindings"]; $bindings ??= [];
        if (count($scope) === 0 || count($scope) > 10000 || count($files) > 4096 || count($bindings) > 10000) {
            return self::refuse("decode", "bounds");
        }
        foreach ($scope as $scopeEntry) {
            if (!self::boundedText($scopeEntry, 128)) {
                return self::refuse('decode', 'bounds');
            }
        }
        foreach ($files as $file) {
            if (!is_string($file) || !Paths::isLogicalPath($file)) {
                return self::refuse('decode', 'bounds');
            }
        }
        foreach ($bindings as $index => $binding) {
            $binding = &$bindings[$index];
            // The pinned 0.6.4 schema spells the members camelCase.
            if (!self::structValue($binding, ['fingerprint', 'kind', 'nativeId', 'path', 'symbol'], "lint_request.bindings.{$index}")) {
                return self::refuse('decode', 'shape');
            }
            self::asMembers($binding, ['path', 'nativeId', 'symbol', 'kind', 'fingerprint'], "lint_request.bindings.{$index}");
            if (!Paths::isLogicalPath($binding['path'])) {
                return self::refuse('decode', 'bounds');
            }
            foreach (['nativeId', 'symbol'] as $member) {
                if (!self::boundedText($binding[$member] ?? null, 128)) {
                    return self::refuse('decode', 'bounds');
                }
            }
            if (!in_array($binding['kind'], ['command', 'entity'], true)) {
                return self::refuse('decode', 'shape');
            }
            if (!Ids::isSha256Digest($binding['fingerprint'] ?? '')) {
                return self::refuse('decode', 'bounds');
            }
            unset($binding);
        }
        $pins = &$value['pins'];
        if (!self::structValue($pins, ['capabilities', 'ir', 'model', 'observed', 'profile', 'revision'], 'lint_request.pins')) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($pins, ['model', 'ir', 'observed', 'revision', 'profile', 'capabilities'], 'lint_request.pins');
        if (self::explicitNull($pins, 'capabilities') || self::explicitNull($pins, 'ir') || self::explicitNull($pins, 'model')
            || self::explicitNull($pins, 'observed') || self::explicitNull($pins, 'profile') || self::explicitNull($pins, 'revision')) {
            return self::refuse('decode', 'shape');
        }
        // The pinned 0.6.4 schema carries every pin as a State-tagged value
        // ({state:"known",value}|{state:"unknown"|"withheld"|"unsupported"}).
        foreach (['capabilities', 'ir', 'model', 'observed', 'profile', 'revision'] as $member) {
            if (!array_key_exists($member, $pins)) {
                return self::refuse('decode', 'shape');
            }
            $state = self::admitStateString($pins[$member] ?? null);
            if ($state !== null) {
                return $state;
            }
        }
        return null;
    }

    /** One State-tagged string value of the pinned 0.6.4 lint schema.
     * The adjacent-tag enum also admits the serde `visit_seq` pair:
     * `["known", value]` and `[tag]` spell positionally (issue #106
     * review r6 #1). A null VALUE element is the serde `None` spelling
     * of the unit variants' absent payload, so `[tag, null]` is legal
     * while `[tag, null, …]` (invalid length) refuses. */
    private static function admitStateString(mixed $value): ?array {
        if (is_array($value) && array_is_list($value) && !self::rawObjectAt('state')) {
            if (count($value) > 2) {
                return self::refuse('decode', 'shape');
            }
            $tag = $value[0] ?? null;
            if ($tag === 'known') {
                if (count($value) !== 2 || !is_string($value[1]) || $value[1] === '' || self::utf8Length($value[1]) > 128) {
                    return self::refuse('decode', 'bounds');
                }
                return null;
            }
            if (in_array($tag, ['unknown', 'withheld', 'unsupported'], true)) {
                return null;
            }
            return self::refuse('decode', 'shape');
        }
        if (!is_array($value) || !self::isClosedObject($value, ['state', 'value'])) {
            return self::refuse('decode', 'shape');
        }
        switch ($value['state'] ?? null) {
            case 'known':
                if (count($value) !== 2 || !isset($value['value']) || !self::boundedText($value['value'], 128)) {
                    return self::refuse('decode', 'bounds');
                }
                return null;
            case 'unknown':
            case 'withheld':
            case 'unsupported':
                if (count($value) !== 1) {
                    return self::refuse('decode', 'shape');
                }
                return null;
            default:
                return self::refuse('decode', 'shape');
        }
    }

    // ------------------------------------------------------------------
    // Response admission
    // ------------------------------------------------------------------

    private static function admitIdentity(mixed &$value, string $path): ?array {
        if (!self::structValue($value, ['digest', 'id', 'version'], $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, ['id', 'version', 'digest'], $path);
        if (!Paths::isToken($value['id'] ?? '') || !Paths::isContractVersion($value['version'] ?? '') || !Ids::isSha256Digest($value['digest'] ?? '')) {
            return self::refuse('decode', 'bounds');
        }
        return null;
    }

    private static function admitEvidence(mixed &$value, string $path): ?array {
        if (!self::structValue($value, ['adapter', 'plan_id'], $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, ['adapter', 'plan_id'], $path);
        $value['adapter'] ??= null; $refusal = self::admitIdentity($value['adapter'], 'profile_capabilities.' . key($value));
        if ($refusal !== null) {
            return $refusal;
        }
        if (self::explicitNull($value, 'plan_id')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['plan_id']) && (!is_string($value['plan_id']) || !Ids::isPlanId($value['plan_id']))) {
            return self::refuse('decode', 'bounds');
        }
        return null;
    }

    private static function admitWriteEntry(mixed &$value, string $path): ?array {
        if (!self::structValue($value, ['action', 'path', 'sha256'], $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, ['path', 'action', 'sha256'], $path);
        if (!Paths::isLogicalPath($value['path'] ?? '')) {
            return self::refuse('decode', 'bounds');
        }
        if (!in_array($value['action'] ?? null, Protocol::ENUMS['writeAction'], true)) {
            return self::refuse('decode', 'shape');
        }
        if (self::explicitNull($value, 'sha256')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['sha256']) && !is_string($value['sha256'])) {
            return self::refuse('decode', 'bounds');
        }
        return null;
    }

    private static function admitScanEntry(mixed &$value, string $path): ?array {
        if (!self::structValue($value, ['detail', 'evidence', 'kind', 'path'], $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, ['path', 'kind', 'detail', 'evidence'], $path);
        if (!Paths::isLogicalPath($value['path'] ?? '') || !self::boundedText($value['kind'] ?? null, 128)) {
            return self::refuse('decode', 'bounds');
        }
        if (self::explicitNull($value, 'detail')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['detail']) && !self::boundedText($value['detail'], 128)) {
            return self::refuse('decode', 'bounds');
        }
        if (self::explicitNull($value, 'evidence')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['evidence'])) {
            $evidence = $value['evidence'];
            if (!self::defaultStruct($evidence, ['signature', 'references'], "{$path}evidence")) {
                return self::refuse('decode', 'shape');
            }
            if (self::explicitNull($evidence, 'signature')) {
                return self::refuse('decode', 'explicit-null');
            }
            if (isset($evidence['signature']) && !Ids::isSha256Digest($evidence['signature'])) {
                return self::refuse('decode', 'bounds');
            }
            if (self::explicitNull($evidence, 'references')) {
                return self::refuse('decode', 'explicit-null');
            }
            $references = $evidence['references'] ?? [];
            if (!is_array($references) || !array_is_list($references) || self::rawObjectAt("{$path}evidence.references") || count($references) > Protocol::BOUNDS['REFERENCES_MAX']) {
                return self::refuse('decode', 'bounds');
            }
            foreach ($references as $index => $reference) {
                if (!self::structValue($reference, ['confidence', 'role', 'target'], "{$path}evidence.references.{$index}")) {
                    return self::refuse('decode', 'shape');
                }
                self::asMembers($reference, ['target', 'role', 'confidence'], "{$path}evidence.references.{$index}");
                if (!self::boundedText($reference['target'] ?? null, 192)) {
                    return self::refuse('decode', 'bounds');
                }
                // The target carries the manifest-local semantic-id
                // grammar (core trace::id::is_semantic_id, issue #106
                // review r3 #8): production's response decoder refuses
                // path-style slash targets, so the SDK must too.
                if (!is_string($reference['target'] ?? null) || !Paths::isSemanticId($reference['target'])) {
                    return self::refuse('decode', 'bounds');
                }
                if (!in_array($reference['role'], Protocol::ENUMS['scanEntryReferenceRole'], true)) {
                    return self::refuse('decode', 'shape');
                }
                if (!in_array($reference['confidence'], Protocol::ENUMS['scanEntryConfidence'], true)) {
                    return self::refuse('decode', 'shape');
                }
            }
        }
        return null;
    }

    private static function admitNativePlan(mixed &$value, string $path): ?array {
        $allowed = ['commands', 'completeness', 'digest', 'entries', 'kind', 'packages', 'run_eligibility', 'units'];
        $fields = ['kind', 'digest', 'commands', 'packages', 'completeness', 'run_eligibility'];
        if (!self::structValue($value, $allowed, $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, $fields, $path);
        if (!in_array($value['kind'] ?? null, Protocol::ENUMS['nativePlanKind'], true)) {
            return self::refuse('decode', 'shape');
        }
        if (!Ids::isSha256Digest($value['digest'] ?? '')) {
            return self::refuse('decode', 'bounds');
        }
        foreach (['commands', 'entries', 'packages', 'units'] as $member) {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            if (isset($value[$member]) && (!self::numberInRange($value[$member], 0, Protocol::BOUNDS['MAX_CONSTRAINT_VALUE']) || self::rawNegativeZeroAt("{$path}.{$member}"))) {
                return self::refuse('decode', 'bounds');
            }
        }
        foreach (['completeness' => 'nativeCompleteness', 'run_eligibility' => 'nativeRunEligibility'] as $member => $vocabulary) {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            if (isset($value[$member]) && !in_array($value[$member], Protocol::ENUMS[$vocabulary], true)) {
                return self::refuse('decode', 'shape');
            }
        }
        return null;
    }

    private static function admitResult(mixed &$value, string $path): ?array {
        $allowed = ['ok', 'truncated', 'entries', 'findings', 'bindings', 'native_plan', 'lint_evidence'];
        if (!self::defaultStruct($value, $allowed, $path)) {
            return self::refuse('decode', 'shape');
        }
        foreach (['ok', 'truncated'] as $member) {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            if (isset($value[$member]) && !is_bool($value[$member])) {
                return self::refuse('decode', 'shape');
            }
        }
        if (self::explicitNull($value, 'entries')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['entries'])) {
            $entries = $value['entries'];
            if (!is_array($entries) || !array_is_list($entries) || self::rawObjectAt('result.entries') || count($entries) > Protocol::BOUNDS['SCAN_ENTRIES_MAX']) {
                return self::refuse('decode', 'bounds');
            }
            foreach ($entries as $index => $entry) {
                $entryRef = &$entries[$index];
                $refusal = self::admitScanEntry($entryRef, "result.entries.{$index}");
                unset($entryRef);
                if ($refusal !== null) {
                    return $refusal;
                }
            }
        }
        if (self::explicitNull($value, 'findings')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['findings'])) {
            $findings = $value['findings'];
            if (!is_array($findings) || !array_is_list($findings) || self::rawObjectAt('result.findings') || count($findings) > Protocol::BOUNDS['FINDINGS_MAX']) {
                return self::refuse('decode', 'bounds');
            }
            foreach ($findings as $index => $finding) {
                if (!self::structValue($finding, ['code', 'detail', 'path'], "result.findings.{$index}")) {
                    return self::refuse('decode', 'shape');
                }
                self::asMembers($finding, ['path', 'code', 'detail'], "result.findings.{$index}");
                if (!Paths::isLogicalPath($finding['path'] ?? '') || !self::boundedText($finding['code'] ?? null, 128)) {
                    return self::refuse('decode', 'bounds');
                }
                if (self::explicitNull($finding, 'detail')) {
                    return self::refuse('decode', 'explicit-null');
                }
                if (isset($finding['detail']) && !self::boundedText($finding['detail'], 128)) {
                    return self::refuse('decode', 'bounds');
                }
            }
        }
        if (self::explicitNull($value, 'bindings')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['bindings'])) {
            $bindings = $value['bindings'];
            if (!is_array($bindings) || !array_is_list($bindings) || self::rawObjectAt('result.bindings') || count($bindings) > Protocol::BOUNDS['BINDINGS_MAX']) {
                return self::refuse('decode', 'bounds');
            }
            foreach ($bindings as $index => $binding) {
                if (!self::structValue($binding, ['module', 'profile', 'target'], "result.bindings.{$index}")) {
                    return self::refuse('decode', 'shape');
                }
                self::asMembers($binding, ['module', 'target', 'profile'], "result.bindings.{$index}");
                foreach (['module', 'profile', 'target'] as $member) {
                    if (!Paths::isToken($binding[$member] ?? '')) {
                        return self::refuse('decode', 'bounds');
                    }
                }
            }
        }
        if (self::explicitNull($value, 'native_plan')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['native_plan'])) {
            $refusal = self::admitNativePlan($value["native_plan"], "result.native_plan");
            if ($refusal !== null) {
                return $refusal;
            }
        }
        if (self::explicitNull($value, 'lint_evidence')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['lint_evidence'])) {
            // The closed LintEvidence subtree is validated before emission
            // (issue #106 review r2 #3): a shallow object check admitted
            // `{}` as `status:"ok"`; the full pinned schema subtree must
            // hold instead.
            $refusal = self::admitLintEvidence($value["lint_evidence"], "result.lint_evidence");
            if ($refusal !== null) {
                return $refusal;
            }
        }
        return null;
    }

    /** The bounded LintEvidence string grammar (first char alnum, 1..=256). */
    private static function lintBounded256(mixed $value): bool {
        if (!is_string($value) || $value === '' || strlen($value) > 256) {
            return false;
        }
        foreach (self::utf8Chars($value) as $index => $char) {
            $alnum = self::isAsciiAlnum($char);
            $extra = $char === '_' || $char === '.' || $char === '/' || $char === ':' || $char === '#' || $char === '@' || $char === '\\' || $char === '-';
            if (!($index === 0 ? $alnum : ($alnum || $extra))) {
                return false;
            }
        }
        return true;
    }

    private static function isAsciiAlnum(string $char): bool {
        return ($char >= 'a' && $char <= 'z') || ($char >= 'A' && $char <= 'Z') || ($char >= '0' && $char <= '9');
    }

    private static function lintStringArray(mixed $value, int $max, bool $unique, ?string $path = null): bool {
        if (!is_array($value) || !array_is_list($value) || ($path !== null && self::rawObjectAt($path)) || count($value) > $max) {
            return false;
        }
        $seen = [];
        foreach ($value as $item) {
            if (!self::lintBounded256($item)) {
                return false;
            }
            if ($unique) {
                if (isset($seen[$item])) {
                    return false;
                }
                $seen[$item] = true;
            }
        }
        return true;
    }

    /** The adjacent-tag State<T> pair also admits the serde `visit_seq`
     * spellings `[[tag, value]]`... i.e. `["known", v]` and `[tag]`
     * positional pairs; `[tag, null]` is the serde `None` payload of the
     * unit variants (issue #106 review r6 #1). */
    private static function lintStateSeq(mixed $value): ?array {
        if (!is_array($value) || !array_is_list($value) || count($value) > 2) {
            return null;
        }
        $tag = $value[0] ?? null;
        if (!in_array($tag, ['known', 'unknown', 'withheld', 'unsupported'], true)) {
            return null;
        }
        return ['state' => $tag, 'value' => $value[1] ?? null, 'seqKnown' => $tag === 'known' && count($value) === 2];
    }

    private static function lintStateString(mixed $value): bool {
        if (($seq = self::lintStateSeq($value)) !== null) {
            if ($seq['seqKnown']) {
                return self::lintBounded256($seq['value']);
            }
            return $seq['state'] !== 'known';
        }
        if (!self::isClosedObject($value, ['state', 'value'])) {
            return false;
        }
        $state = $value['state'];
        if ($state === 'known') {
            return self::lintBounded256($value['value'] ?? null);
        }
        if ($state === 'unknown' || $state === 'withheld' || $state === 'unsupported') {
            return !array_key_exists('value', $value);
        }
        return false;
    }

    private static function lintStateBool(mixed $value): bool {
        if (($seq = self::lintStateSeq($value)) !== null) {
            if ($seq['seqKnown']) {
                return is_bool($seq['value']);
            }
            return $seq['state'] !== 'known';
        }
        if (!self::isClosedObject($value, ['state', 'value'])) {
            return false;
        }
        $state = $value['state'];
        if ($state === 'known') {
            return is_bool($value['value'] ?? null);
        }
        if ($state === 'unknown' || $state === 'withheld' || $state === 'unsupported') {
            return !array_key_exists('value', $value);
        }
        return false;
    }

    private static function lintStateU64(mixed $value, ?string $path = null): bool {
        if (($seq = self::lintStateSeq($value)) !== null) {
            if ($seq['seqKnown']) {
                return self::numberInRange($seq['value'], 0, 10_000_000)
                    && ($path === null || !self::rawNegativeZeroAt("{$path}.value"));
            }
            return $seq['state'] !== 'known';
        }
        if (!self::isClosedObject($value, ['state', 'value'])) {
            return false;
        }
        $state = $value['state'];
        if ($state === 'known') {
            return self::numberInRange($value['value'] ?? null, 0, 10_000_000)
                && ($path === null || !self::rawNegativeZeroAt("{$path}.value"));
        }
        if ($state === 'unknown' || $state === 'withheld' || $state === 'unsupported') {
            return !array_key_exists('value', $value);
        }
        return false;
    }

    private static function lintSha256(mixed $value): bool {
        return is_string($value) && $value !== '' && strlen($value) <= 71 && Ids::isSha256Digest($value);
    }

    private static function lintPins(mixed $value): bool {
        $allowed = ['capabilities', 'ir', 'model', 'observed', 'profile', 'revision'];
        $fields = ['model', 'ir', 'observed', 'revision', 'profile', 'capabilities'];
        if (!self::structValue($value, $allowed, 'result.lint_evidence.pins') || !is_array($value)) {
            return false;
        }
        self::asMembers($value, $fields, 'result.lint_evidence.pins');
        foreach ($allowed as $pin) {
            if (!self::lintStateString($value[$pin] ?? null)) {
                return false;
            }
        }
        return true;
    }

    private static function lintProducer(mixed $value): bool {
        $allowed = ['artifactDigest', 'compiler', 'framework', 'id', 'recipe', 'tool', 'version'];
        $fields = ['id', 'version', 'artifactDigest', 'tool', 'compiler', 'framework', 'recipe'];
        if (!self::structValue($value, $allowed, 'result.lint_evidence.producer') || !is_array($value)) {
            return false;
        }
        self::asMembers($value, $fields, 'result.lint_evidence.producer');
        foreach (['id', 'version', 'tool', 'recipe'] as $member) {
            if (!self::lintBounded256($value[$member] ?? null)) {
                return false;
            }
        }
        return self::lintSha256($value['artifactDigest'] ?? null)
            && self::lintStateString($value['compiler'] ?? null)
            && self::lintStateString($value['framework'] ?? null);
    }

    private static function lintSource(mixed $value, ?string $path = null): bool {
        if (!self::structValue($value, ['bytes', 'fingerprint', 'id', 'path'], $path) || !is_array($value)) {
            return false;
        }
        self::asMembers($value, ['id', 'path', 'fingerprint', 'bytes'], $path);
        $pathValue = $value['path'] ?? null;
        return self::lintBounded256($value['id'] ?? null)
            && is_string($pathValue) && $pathValue !== '' && strlen($pathValue) <= 512 && Paths::isLogicalPath($pathValue)
            && self::lintSha256($value['fingerprint'] ?? null)
            && self::numberInRange($value['bytes'] ?? null, 0, Protocol::BOUNDS['MAX_REQUEST_BYTES'])
            && ($path === null || !self::rawNegativeZeroAt("{$path}.bytes"));
    }

    private static function lintSpan(mixed $value, ?string $path = null): bool {
        if (!self::structValue($value, ['column', 'end', 'endColumn', 'endLine', 'id', 'line', 'source', 'start'], $path) || !is_array($value)) {
            return false;
        }
        self::asMembers($value, ['id', 'source', 'start', 'end', 'line', 'column', 'endLine', 'endColumn'], $path);
        $bounded = static function (mixed $member, int $min, string $name) use ($value, $path): bool {
            return self::numberInRange($member, $min, Protocol::BOUNDS['MAX_REQUEST_BYTES'])
                && ($path === null || !self::rawNegativeZeroAt("{$path}.{$name}"));
        };
        return self::lintBounded256($value['id'] ?? null)
            && self::lintBounded256($value['source'] ?? null)
            && $bounded($value['start'] ?? null, 0, 'start')
            && $bounded($value['end'] ?? null, 0, 'end')
            && $bounded($value['line'] ?? null, 0, 'line')
            && $bounded($value['column'] ?? null, 0, 'column')
            && $bounded($value['endLine'] ?? null, 0, 'endLine')
            && $bounded($value['endColumn'] ?? null, 0, 'endColumn');
    }

    private static function lintCoverage(mixed $value, ?string $path = null): bool {
        if (!self::structValue($value, ['eligible', 'examined', 'limitations', 'rule', 'scope', 'state', 'target'], $path) || !is_array($value)) {
            return false;
        }
        self::asMembers($value, ['rule', 'target', 'scope', 'state', 'eligible', 'examined', 'limitations'], $path);
        if (!in_array($value['state'] ?? null, ['complete', 'partial', 'unknown', 'unsupported', 'withheld', 'disabled'], true)) {
            return false;
        }
        foreach (['rule', 'target', 'scope'] as $member) {
            if (!self::lintBounded256($value[$member] ?? null)) {
                return false;
            }
        }
        return self::lintStateU64($value['eligible'] ?? null, $path === null ? null : "{$path}.eligible")
            && self::lintStateU64($value['examined'] ?? null, $path === null ? null : "{$path}.examined")
            && self::lintStringArray($value['limitations'] ?? null, 10_000, true, $path === null ? null : "{$path}.limitations");
    }

    private static function lintRoute(mixed $value): bool {
        if (!self::structValue($value, ['confidence', 'currency', 'identity', 'origin'], 'result.lint_evidence.records') || !is_array($value)) {
            return false;
        }
        self::asMembers($value, ['identity', 'confidence', 'currency', 'origin'], 'result.lint_evidence.records');
        if (!in_array($value['confidence'] ?? null, ['unknown', 'low', 'medium', 'high', 'exact'], true)
            || !in_array($value['currency'] ?? null, ['current', 'stale', 'unknown'], true)
            || !in_array($value['origin'] ?? null, ['explicit', 'extracted', 'inferred'], true)) {
            return false;
        }
        return self::lintBounded256($value['identity'] ?? null);
    }

    private static function lintStep(mixed $value, ?string $path = null): bool {
        if (!self::structValue($value, ['confidence', 'guards', 'identity', 'locations', 'role'], $path) || !is_array($value)) {
            return false;
        }
        self::asMembers($value, ['role', 'identity', 'locations', 'guards', 'confidence'], $path);
        if (!in_array($value['role'] ?? null, ['binding', 'trigger', 'registration', 'callback', 'effect'], true)
            || !in_array($value['confidence'] ?? null, ['unknown', 'low', 'medium', 'high', 'exact'], true)) {
            return false;
        }
        return self::lintBounded256($value['identity'] ?? null)
            && self::lintStringArray($value['locations'] ?? null, 32, true, $path === null ? null : "{$path}.locations")
            && self::lintStringArray($value['guards'] ?? null, 32, true, $path === null ? null : "{$path}.guards");
    }

    private static function lintRecord(mixed $value, ?string $path = null): bool {
        $allowed = [
            'activation', 'binding', 'candidates', 'claim', 'confidence', 'configuration',
            'currency', 'field', 'guards', 'id', 'key', 'kind', 'locations', 'mechanism',
            'nativeId', 'operation', 'origin', 'ownership', 'resource', 'semanticSymbol',
            'subject', 'trace', 'value',
        ];
        if (!self::structValue($value, $allowed, $path) || !is_array($value)) {
            return false;
        }
        self::asMembers($value, $allowed, $path);
        if (!isset($value['kind']) || !isset($value['mechanism']) || !isset($value['confidence']) || !isset($value['currency']) || !isset($value['origin']) || !isset($value['claim'])) {
            return false;
        }
        if (!in_array($value['kind'], ['binding-candidates', 'dispatch', 'convention', 'effect', 'path', 'reflection', 'string-reference', 'default', 'native-edge', 'field-write'], true)
            || !in_array($value['mechanism'], ['direct', 'observer', 'hook', 'magic'], true)
            || !in_array($value['confidence'], ['unknown', 'low', 'medium', 'high', 'exact'], true)
            || !in_array($value['currency'], ['current', 'stale', 'unknown'], true)
            || !in_array($value['origin'], ['explicit', 'extracted', 'inferred'], true)
            || !in_array($value['claim'], ['structural', 'possible-behavior', 'verified-behavior'], true)) {
            return false;
        }
        foreach (['id', 'subject', 'nativeId'] as $member) {
            if (!self::lintBounded256($value[$member] ?? null)) {
                return false;
            }
        }
        foreach (['semanticSymbol', 'operation', 'resource', 'field', 'value', 'key'] as $member) {
            if (!self::lintStateString($value[$member] ?? null)) {
                return false;
            }
        }
        foreach (['binding', 'configuration', 'ownership', 'trace'] as $member) {
            if (!self::lintStateBool($value[$member] ?? null)) {
                return false;
            }
        }
        if (!is_array($value['candidates']) || !array_is_list($value['candidates']) || ($path !== null && self::rawObjectAt("{$path}.candidates")) || count($value['candidates']) > 32) {
            return false;
        }
        foreach ($value['candidates'] as $candidate) {
            if (!self::lintRoute($candidate)) {
                return false;
            }
        }
        if (!is_array($value['activation']) || !array_is_list($value['activation']) || ($path !== null && self::rawObjectAt("{$path}.activation")) || count($value['activation']) > 32) {
            return false;
        }
        foreach ($value['activation'] as $index => $step) {
            if (!self::lintStep($step, $path === null ? null : "{$path}.activation.{$index}")) {
                return false;
            }
        }
        return self::lintStringArray($value['locations'], 32, true, $path === null ? null : "{$path}.locations")
            && self::lintStringArray($value['guards'], 32, true, $path === null ? null : "{$path}.guards");
    }

    /**
     * The closed `LintEvidence` subtree of a lint result (issue #106
     * review r2 #3): all twelve members required, const identity/version,
     * closed nested objects — shallow or malformed evidence is refused
     * instead of being emitted as success.
     */
    private static function admitLintEvidence(mixed &$value, string $path): ?array {
        $allowed = [
            'coverage', 'identity', 'inputManifestDigest', 'limitations', 'locations',
            'pins', 'producer', 'records', 'schemaVersion', 'scope', 'sources', 'target',
        ];
        if (!self::structValue($value, $allowed, $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, ['schemaVersion', 'identity', 'target', 'scope', 'producer', 'pins', 'inputManifestDigest', 'sources', 'locations', 'coverage', 'records', 'limitations'], $path);
        foreach ($allowed as $member) {
            if (!array_key_exists($member, $value)) {
                return self::refuse('decode', 'shape');
            }
        }
        if ($value['schemaVersion'] !== 'lekalo/ai-lint-evidence/v0.6.4'
            || $value['identity'] !== 'dev.lekalo.ai-lint-evidence@0.6.4') {
            return self::refuse('decode', 'shape');
        }
        if (!self::lintBounded256($value['target'])
            || !self::lintStringArray($value['scope'], 10_000, true, "{$path}.scope")
            || !self::lintProducer($value['producer'])
            || !self::lintPins($value['pins'])
            || !self::lintSha256($value['inputManifestDigest'])) {
            return self::refuse('decode', 'bounds');
        }
        if (!is_array($value['sources']) || !array_is_list($value['sources']) || self::rawObjectAt("{$path}.sources") || count($value['sources']) > 10_000) {
            return self::refuse('decode', 'bounds');
        }
        foreach ($value['sources'] as $index => $source) {
            if (!self::lintSource($source, "{$path}.sources.{$index}")) {
                return self::refuse('decode', 'bounds');
            }
        }
        if (!is_array($value['locations']) || !array_is_list($value['locations']) || self::rawObjectAt("{$path}.locations") || count($value['locations']) > 32) {
            return self::refuse('decode', 'bounds');
        }
        $spanKeys = [];
        foreach ($value['locations'] as $index => $span) {
            if (!self::lintSpan($span, "{$path}.locations.{$index}")) {
                return self::refuse('decode', 'bounds');
            }
            // The uniqueness key is the CANONICAL serialization (recursively
            // sorted member names), not member order (issue #106 review r3
            // #10): json_encode preserves insertion order, so two deep-equal
            // spans with permuted members used to hash differently and a
            // permuted duplicate passed the uniqueItems check.
            $key = Json::canonicalJson($span);
            if (isset($spanKeys[$key])) {
                return self::refuse('decode', 'bounds');
            }
            $spanKeys[$key] = true;
        }
        if (!is_array($value['coverage']) || !array_is_list($value['coverage']) || self::rawObjectAt("{$path}.coverage") || count($value['coverage']) > 10_000) {
            return self::refuse('decode', 'bounds');
        }
        foreach ($value['coverage'] as $index => $coverage) {
            if (!self::lintCoverage($coverage, "{$path}.coverage.{$index}")) {
                return self::refuse('decode', 'bounds');
            }
        }
        if (!is_array($value['records']) || !array_is_list($value['records']) || self::rawObjectAt("{$path}.records") || count($value['records']) > 10_000) {
            return self::refuse('decode', 'bounds');
        }
        foreach ($value['records'] as $index => $record) {
            if (!self::lintRecord($record, "{$path}.records.{$index}")) {
                return self::refuse('decode', 'bounds');
            }
        }
        if (!self::lintStringArray($value['limitations'], 10_000, true, "{$path}.limitations")) {
            return self::refuse('decode', 'bounds');
        }
        return null;
    }

    private static function admitProgress(mixed &$value, string $path): ?array {
        if (!is_array($value) || !array_is_list($value) || self::rawObjectAt($path) || count($value) > Protocol::BOUNDS['PROGRESS_STEPS_MAX']) {
            return self::refuse('decode', 'bounds');
        }
        foreach ($value as $step) {
            if (!self::structValue($step, ['detail', 'state', 'step'], $path)) {
                return self::refuse('decode', 'shape');
            }
            self::asMembers($step, ['step', 'state', 'detail'], $path);
            if (!self::boundedText($step['step'] ?? null, 128)) {
                return self::refuse('decode', 'bounds');
            }
            if (!in_array($step['state'], Protocol::ENUMS['progressState'], true)) {
                return self::refuse('decode', 'shape');
            }
            if (self::explicitNull($step, 'detail')) {
                return self::refuse('decode', 'explicit-null');
            }
            if (isset($step['detail']) && !self::boundedText($step['detail'], 128)) {
                return self::refuse('decode', 'bounds');
            }
        }
        return null;
    }

    private static function admitError(mixed &$value, string $path): ?array {
        $allowed = ['class', 'code', 'detail', 'message', 'partial', 'retryable'];
        if (!self::structValue($value, $allowed, 'error')) {
            return self::refuse('decode', 'shape');
        }
        $fields = ['class', 'code', 'message', 'retryable', 'partial', 'detail'];
        self::asMembers($value, $fields, $path);
        if (!in_array($value['class'] ?? null, Protocol::ENUMS['errorClass'], true)) {
            return self::refuse('decode', 'shape');
        }
        // Production's response validation refuses empty error strings
        // after the bounded check (wire.rs `error.code.is_empty()`), so
        // the SDK admission refuses them too (issue #106 review r6 #3).
        if ($value['code'] === '' || $value['message'] === ''
            || !self::boundedText($value['code'] ?? null, 128) || !self::boundedText($value['message'] ?? null, 256)) {
            return self::refuse('decode', 'bounds');
        }
        foreach (['partial', 'retryable'] as $member) {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
            if (isset($value[$member]) && !is_bool($value[$member])) {
                return self::refuse('decode', 'shape');
            }
        }
        if (self::explicitNull($value, 'detail')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['detail'])) {
            $details = $value['detail'];
            if (!is_array($details) || !array_is_list($details) || self::rawObjectAt('error.detail') || count($details) > Protocol::BOUNDS['ERROR_DETAILS_MAX']) {
                return self::refuse('decode', 'bounds');
            }
            foreach ($details as $detail) {
                if (!self::boundedText($detail, 128)) {
                    return self::refuse('decode', 'bounds');
                }
            }
        }
        return null;
    }

    private static function admitCapabilities(mixed &$value, string $version, string $path): ?array {
        $allowed = ['adapter', 'capabilities', 'constraints', 'ir_versions', 'operations', 'profiles', 'progress', 'protocol_versions', 'read_scopes', 'targets', 'transports', 'write_scopes'];
        if (!self::structValue($value, $allowed, $path)) {
            return self::refuse('decode', 'shape');
        }
        self::asMembers($value, ['adapter', 'protocol_versions', 'operations', 'transports', 'targets', 'profiles', 'read_scopes', 'write_scopes', 'progress', 'ir_versions', 'capabilities', 'constraints'], $path);
        $refusal = self::admitIdentity($value["adapter"], "{$path}.adapter");
        if ($refusal !== null) {
            return $refusal;
        }
        $checkList = function (array $value, string $member, int $max, bool $required, callable $check) use (&$refusal): ?array {
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'bounds');
            }
            if (!array_key_exists($member, $value)) {
                return $required ? self::refuse('decode', 'bounds') : null;
            }
            $items = $value[$member];
            if (!is_array($items) || !array_is_list($items) || self::rawObjectAt("capabilities.{$member}") || count($items) > $max || ($required && count($items) === 0)) {
                return self::refuse('decode', 'bounds');
            }
            $unique = [];
            foreach ($items as $item) {
                if (!is_string($item) || !$check($item)) {
                    return self::refuse('decode', 'bounds');
                }
                if (isset($unique[$item])) {
                    return self::refuse('decode', 'bounds');
                }
                $unique[$item] = true;
            }
            return null;
        };
        $isContract = fn ($item) => Paths::isContractVersion($item);
        $refusal = $checkList($value, 'protocol_versions', 8, true, $isContract);
        if ($refusal !== null) {
            return $refusal;
        }
        $maxOps = $version === self::LINT_VERSION ? Protocol::BOUNDS['OPERATIONS_MAX_LINT'] : Protocol::BOUNDS['OPERATIONS_MAX_BASE'];
        $refusal = $checkList($value, 'operations', $maxOps, true, fn ($op) => in_array($op, Protocol::LATEST_OPERATIONS, true) && ($op !== 'lint' || $version === self::LINT_VERSION));
        if ($refusal !== null) {
            return $refusal;
        }
        $refusal = $checkList($value, 'transports', Protocol::BOUNDS['TRANSPORTS_MAX'], true, fn ($t) => in_array($t, Protocol::ENUMS['transport'], true));
        if ($refusal !== null) {
            return $refusal;
        }
        $refusal = $checkList($value, 'targets', Protocol::BOUNDS['TARGETS_MAX'], false, fn ($t) => Paths::isToken($t));
        if ($refusal !== null) {
            return $refusal;
        }
        $refusal = $checkList($value, 'profiles', Protocol::BOUNDS['PROFILES_MAX'], false, fn ($t) => Paths::isToken($t));
        if ($refusal !== null) {
            return $refusal;
        }
        // Production checks only uniqueness and the count ceiling on the
        // scope lists (wire.rs `unique(&c.read_scopes, 0, 64)`); item
        // content is unconstrained text, so the SDK's extra non-empty /
        // length grammar is an over-strict divergence (issue #106 review
        // r6 #4) and stays out of admission.
        $scopeText = fn ($scope) => is_string($scope);
        $refusal = $checkList($value, 'read_scopes', Protocol::BOUNDS['MAX_SCOPES'], false, $scopeText);
        if ($refusal !== null) {
            return $refusal;
        }
        $refusal = $checkList($value, 'write_scopes', Protocol::BOUNDS['MAX_SCOPES'], false, $scopeText);
        if ($refusal !== null) {
            return $refusal;
        }
        $refusal = $checkList($value, 'ir_versions', Protocol::BOUNDS['MAX_IR_VERSIONS'], false, $isContract);
        if ($refusal !== null) {
            return $refusal;
        }
        if (array_key_exists('progress', $value) && !is_bool($value['progress'])) {
            return self::refuse('decode', 'shape');
        }
        if (self::explicitNull($value, 'capabilities')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['capabilities'])) {
            $capabilityMap = $value['capabilities'];
            if (!is_array($capabilityMap) || !self::rawObjectAt('capabilities.capabilities')) {
                return self::refuse('decode', 'shape');
            }
            if (count($capabilityMap) > Protocol::BOUNDS['MAX_DECLARED_CAPABILITIES']) {
                return self::refuse('decode', 'bounds');
            }
            foreach ($capabilityMap as $id => $support) {
                if (!in_array($id, array_column(Capabilities::DEFINITIONS, 'id'), true)) {
                    return self::refuse('decode', 'bounds');
                }
                if (!in_array($support, Protocol::ENUMS['supportState'], true)) {
                    return self::refuse('decode', 'shape');
                }
            }
        }
        if (isset($value['constraints'])) {
            $constraints = $value['constraints'];
            if (!self::defaultStruct($constraints, ['max_entries', 'max_writes'], 'capabilities.constraints')) {
                return self::refuse('decode', 'shape');
            }
            foreach (['max_entries', 'max_writes'] as $member) {
                if (isset($constraints[$member]) && (!self::numberInRange($constraints[$member], 1, Protocol::BOUNDS['MAX_CONSTRAINT_VALUE']) || self::rawNegativeZeroAt("capabilities.constraints.{$member}"))) {
                    return self::refuse('decode', 'bounds');
                }
            }
        }
        return null;
    }

    private static function admitResponseValue(array &$value): ?array {
        $allowed = ['capabilities', 'error', 'evidence', 'operation', 'progress', 'protocol', 'protocol_version', 'request_id', 'result', 'status', 'writes'];
        if (!self::structValue($value, $allowed, '')) {
            return self::refuse('decode', self::unknownOrShape($value, $allowed));
        }
        self::asMembers($value, ['protocol', 'protocol_version', 'operation', 'request_id', 'status', 'evidence', 'capabilities', 'result', 'writes', 'progress', 'error'], '');
        foreach (['protocol', 'protocol_version', 'operation', 'request_id', 'status', 'evidence'] as $member) {
            if (!array_key_exists($member, $value)) {
                return self::refuse('decode', 'shape');
            }
            if (self::explicitNull($value, $member)) {
                return self::refuse('decode', 'explicit-null');
            }
        }
        if ($value['protocol'] !== Protocol::WIRE_TOKEN) {
            return self::refuse('decode', 'token');
        }
        if (!Paths::isContractVersion($value['protocol_version'])) {
            return self::refuse('decode', 'version');
        }
        if (!in_array($value['operation'], Protocol::LATEST_OPERATIONS, true)) {
            return self::refuse('decode', 'operation');
        }
        if (!is_string($value['request_id']) || !Ids::isRequestId($value['request_id'])) {
            return self::refuse('decode', 'request-id');
        }
        if (!in_array($value['status'], Protocol::ENUMS['responseStatus'], true)) {
            return self::refuse('decode', 'shape');
        }
        $refusal = self::admitEvidence($value["evidence"], "evidence");
        if ($refusal !== null) {
            return $refusal;
        }
        if (self::explicitNull($value, 'capabilities')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['capabilities'])) {
            $refusal = self::admitCapabilities($value["capabilities"], $value["protocol_version"], "capabilities");
            if ($refusal !== null) {
                return $refusal;
            }
        }
        if (self::explicitNull($value, 'result')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['result'])) {
            $refusal = self::admitResult($value["result"], "result");
            if ($refusal !== null) {
                return $refusal;
            }
        }
        if (self::explicitNull($value, 'writes')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['writes'])) {
            $writes = $value['writes'];
            if (!is_array($writes) || !array_is_list($writes) || self::rawObjectAt('writes') || count($writes) > Protocol::BOUNDS['MAX_PLAN_ENTRIES']) {
                return self::refuse('decode', 'bounds');
            }
            foreach ($writes as $index => $entry) {
                $refusal = self::admitWriteEntry($writes[$index], "writes.{$index}");
                if ($refusal !== null) {
                    return $refusal;
                }
            }
        }
        if (self::explicitNull($value, 'progress')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['progress'])) {
            $refusal = self::admitProgress($value["progress"], "progress");
            if ($refusal !== null) {
                return $refusal;
            }
        }
        if (self::explicitNull($value, 'error')) {
            return self::refuse('decode', 'explicit-null');
        }
        if (isset($value['error'])) {
            $refusal = self::admitError($value["error"], "error");
            if ($refusal !== null) {
                return $refusal;
            }
        }
        $result = $value['result'] ?? null;
        $carriesLint = isset($result['lint_evidence']) || (is_array($result) && array_is_list($result) && !self::rawObjectAt('result') && count($result) >= 7);
        if ($carriesLint && ($value['operation'] !== 'lint' || $value['protocol_version'] !== self::LINT_VERSION)) {
            return self::refuse('decode', 'shape');
        }
        return null;
    }

    // ------------------------------------------------------------------
    // Semantic validation (mirrors the host's validate_request)
    // ------------------------------------------------------------------

    public static function validateRequest(array $value): ?array {
        $invalid = fn (string $reason): array => self::refuse('validate', $reason);
        if (($value['protocol'] ?? '') !== Protocol::WIRE_TOKEN) {
            return $invalid('version');
        }
        $operation = $value['operation'];
        $version = $value['protocol_version'];
        $hasLint = array_key_exists('lint_request', $value);
        if (($operation === 'lint') !== $hasLint) {
            return $invalid('lint-request');
        }
        if ($hasLint) {
            $lint = $value['lint_request'];
            if ($version !== self::LINT_VERSION || !array_key_exists('target', $value)) {
                return $invalid('lint-request');
            }
            foreach (['profile', 'profile_digest', 'profile_capabilities', 'ir_path', 'native_request', 'dry_run', 'plan_id'] as $member) {
                if (array_key_exists($member, $value)) {
                    return $invalid('lint-request');
                }
            }
            $scope = is_array($lint['scope'] ?? null) ? $lint['scope'] : [];
            $files = is_array($lint['files'] ?? null) ? $lint['files'] : [];
            if (!self::isSorted($scope) || !self::isSorted($files)) {
                return $invalid('lint-request');
            }
            // Production gates scope items with the ai-lint token grammar,
            // not a length bound (issue #106 review r3 #5).
            foreach ($scope as $item) {
                if (!is_string($item) || !Paths::isWireToken($item)) {
                    return $invalid('lint-request');
                }
            }
            foreach ($files as $file) {
                if (!Paths::isLogicalPath($file)) {
                    return $invalid('lint-request');
                }
            }
            $keys = [];
            foreach ($lint['bindings'] as $binding) {
                $path = $binding['path'];
                if (!Paths::isLogicalPath($path) || !in_array($path, $files, true)) {
                    return $invalid('lint-binding');
                }
                if (!Ids::isSha256Digest($binding['fingerprint'] ?? '')) {
                    return $invalid('lint-binding');
                }
                // nativeId and symbol carry the ai-lint token grammar
                // (issue #106 review r3 #5), not a length bound.
                if (!Paths::isWireToken($binding['nativeId'] ?? '') || !Paths::isWireToken($binding['symbol'] ?? '')) {
                    return $invalid('lint-binding');
                }
                $key = $path . "\0" . $binding['nativeId'] . "\0" . $binding['kind'];
                if (isset($keys[$key])) {
                    return $invalid('lint-binding');
                }
                $keys[$key] = true;
            }
            $pins = $lint['pins'];
            $pinKnown = static function ($pin): ?string {
                return ($pin['state'] ?? null) === 'known' ? ($pin['value'] ?? null) : null;
            };
            foreach (['model', 'ir'] as $pin) {
                // Required known digests (issue #106 review r2 #2):
                // production refuses unknown and malformed spellings of
                // the model/ir pins; mirror it exactly.
                $known = $pinKnown($pins[$pin] ?? null);
                if ($known === null || !Ids::isSha256Digest($known)) {
                    return $invalid('lint-pin');
                }
            }
            foreach (['observed', 'profile', 'capabilities'] as $pin) {
                $known = $pinKnown($pins[$pin] ?? null);
                if ($known !== null && !Ids::isSha256Digest($known)) {
                    return $invalid('lint-pin');
                }
            }
            $revision = $pinKnown($pins['revision'] ?? null);
            if ($revision !== null && !Paths::isWireToken($revision)) {
                return $invalid('lint-pin');
            }
        }
        $hasNative = array_key_exists('native_request', $value);
        if (($operation === 'plan-native') !== $hasNative) {
            return $invalid('native-request');
        }
        $apply = $operation === 'clean' || ($operation === 'generate' && isset($value['dry_run']) && $value['dry_run'] === false);
        $hasPlan = array_key_exists('plan_id', $value);
        if ($apply !== $hasPlan) {
            return $invalid('plan-id');
        }
        if ($hasPlan && !Ids::isPlanId($value['plan_id'])) {
            return $invalid('plan-id');
        }
        if (($operation === 'generate') !== array_key_exists('dry_run', $value)) {
            return $invalid('dry-run');
        }
        if ($operation === 'plan-native' && (array_key_exists('dry_run', $value) || $hasPlan)) {
            return $invalid('plan-id');
        }
        if (in_array($operation, ['validate', 'generate', 'verify'], true) && !array_key_exists('ir_path', $value)) {
            return $invalid('ir-path');
        }
        if (in_array($operation, ['generate', 'bind'], true) && !array_key_exists('target', $value)) {
            return $invalid('target');
        }
        if ($operation === 'bind' && !array_key_exists('profile', $value)) {
            return $invalid('profile');
        }
        if ($operation === 'describe') {
            foreach (['target', 'profile', 'profile_digest', 'profile_capabilities', 'native_request', 'ir_path'] as $member) {
                if (array_key_exists($member, $value)) {
                    return $invalid('member');
                }
            }
        }
        $profileResolution = array_key_exists('profile_digest', $value) || array_key_exists('profile_capabilities', $value);
        if ($profileResolution && (!array_key_exists('profile_digest', $value) || !array_key_exists('profile_capabilities', $value) || !array_key_exists('profile', $value))) {
            return $invalid('profile-capabilities');
        }
        if ($profileResolution && !in_array($version, Protocol::SUPPORTED_PROTOCOL_VERSIONS, true)) {
            return $invalid('member');
        }
        return null;
    }

    /** Response identity echo against the exact negotiated session. */
    public static function validateResponseIdentity(array $response, string $expectedVersion, string $expectedOperation, string $expectedRequestId): ?array {
        if (($response['protocol'] ?? '') !== Protocol::WIRE_TOKEN) {
            return self::refuse('validate', 'version');
        }
        if ($response['protocol_version'] !== $expectedVersion) {
            return self::refuse('validate', 'echo-version');
        }
        if ($response['operation'] !== $expectedOperation) {
            return self::refuse('validate', 'echo-operation');
        }
        if ($response['request_id'] !== $expectedRequestId) {
            return self::refuse('validate', 'echo-request-id');
        }
        if (isset($response['capabilities']) && Json::canonicalJson($response['evidence']['adapter']) !== Json::canonicalJson($response['capabilities']['adapter'])) {
            return self::refuse('validate', 'identity');
        }
        return null;
    }

    /** Operation payload pairing (the validate-before-write obligation). */
    public static function validateResponsePayload(array $request, array $response): ?array {
        $invalid = fn (string $reason): array => self::refuse('validate', $reason);
        $operation = $request['operation'];
        if (array_key_exists('capabilities', $response) && $operation !== 'describe') {
            return $invalid('member');
        }
        if ($operation === 'describe' && ($response['status'] ?? '') === 'ok' && !array_key_exists('capabilities', $response)) {
            return $invalid('member');
        }
        if ($operation === 'describe' && isset($response['capabilities'])) {
            $caps = $response['capabilities'];
            if (Json::canonicalJson($response['evidence']['adapter']) !== Json::canonicalJson($caps['adapter'])) {
                return $invalid('identity');
            }
            if (!in_array('describe', $caps['operations'], true) || !in_array($request['protocol_version'], $caps['protocol_versions'], true)) {
                return $invalid('member');
            }
            if (array_key_exists('progress', $response)) {
                return $invalid('member');
            }
        }
        $apply = $operation === 'clean' || ($operation === 'generate' && isset($request['dry_run']) && $request['dry_run'] === false);
        $evidencePlan = $response['evidence']['plan_id'] ?? null;
        if ($apply && $evidencePlan !== ($request['plan_id'] ?? null)) {
            return $invalid('plan-id');
        }
        if (($response['status'] ?? '') === 'error') {
            if (!array_key_exists('error', $response)) {
                return $invalid('error-pairing');
            }
            if (array_key_exists('result', $response) || array_key_exists('writes', $response) || array_key_exists('progress', $response)) {
                return $invalid('error-pairing');
            }
            return null;
        }
        if (array_key_exists('error', $response)) {
            return $invalid('error-pairing');
        }
        $result = $response['result'] ?? null;
        if (is_array($result)) {
            if (array_key_exists('lint_evidence', $result) && $operation !== 'lint') {
                return $invalid('member');
            }
            if (array_key_exists('entries', $result) && $operation !== 'scan') {
                return $invalid('member');
            }
            if (array_key_exists('truncated', $result) && $operation !== 'scan') {
                return $invalid('member');
            }
            if (array_key_exists('bindings', $result) && $operation !== 'bind') {
                return $invalid('member');
            }
            if (array_key_exists('ok', $result) && !in_array($operation, ['validate', 'verify'], true)) {
                return $invalid('member');
            }
            if (array_key_exists('findings', $result) && !in_array($operation, ['validate', 'verify'], true)) {
                return $invalid('member');
            }
            if (array_key_exists('native_plan', $result) && $operation !== 'plan-native') {
                return $invalid('member');
            }
            if ($operation === 'plan-native' && array_key_exists('entries', $result)) {
                return $invalid('member');
            }
        }
        $complete = match ($operation) {
            'scan' => is_array($result) && array_key_exists('entries', $result),
            'bind' => is_array($result) && array_key_exists('bindings', $result),
            'validate', 'verify' => is_array($result) && array_key_exists('ok', $result),
            'plan-native' => is_array($result) && array_key_exists('native_plan', $result),
            'lint' => is_array($result) && array_key_exists('lint_evidence', $result),
            default => $result === null,
        };
        if (!$complete) {
            return $invalid('member');
        }
        if (in_array($operation, ['generate', 'clean', 'plan-clean'], true)) {
            if (!array_key_exists('writes', $response) || !is_array($response['writes'])) {
                return $invalid('writes-missing');
            }
            foreach ($response['writes'] as $entry) {
                if (($entry['action'] ?? '') === 'delete') {
                    if (array_key_exists('sha256', $entry)) {
                        return $invalid('write-digest');
                    }
                } elseif (!Ids::isSha256Digest($entry['sha256'] ?? null)) {
                    return $invalid('write-digest');
                }
            }
        } elseif (array_key_exists('writes', $response)) {
            return $invalid('member');
        }
        return null;
    }
}

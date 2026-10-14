<?php
declare(strict_types=1);
// Response builders and the validate-before-write boundary (issue #106).

namespace Lekalo\AdapterSdk\Runtime;

use Lekalo\AdapterSdk\Generated\Protocol;
use Lekalo\AdapterSdk\Generated\Capabilities;

final class Build {
    /** Build and validate one adapter identity block. */
    public static function buildAdapterIdentity(string $id, string $version, string $digest): ?array {
        if (!Paths::isToken($id) || !Paths::isContractVersion($version) || !Ids::isSha256Digest($digest)) {
            return self::refuse('identity');
        }
        return ['digest' => $digest, 'id' => $id, 'version' => $version];
    }

    /**
     * Build one describe capability map. Declared scopes and capability
     * states are honest claims the host polices.
     */
    public static function buildCapabilities(
        array $adapter,
        array $protocolVersions,
        array $operations,
        array $transports,
        array $targets,
        array $profiles,
        array $readScopes,
        array $writeScopes,
        array $irVersions,
        bool $progress,
        array $capabilities = [],
        array $constraints = []
    ): ?array {
        $bounded = function (array $values, int $max): ?array {
            if (count($values) > $max) {
                return null;
            }
            $sorted = array_values(array_unique($values));
            sort($sorted, SORT_STRING);
            if (count($sorted) !== count($values)) {
                return null;
            }
            return $sorted;
        };
        if ($protocolVersions === [] || $operations === [] || $transports === []) {
            return self::refuse('bounds');
        }
        $members = [
            'adapter' => $adapter,
            'ir_versions' => $bounded($irVersions, Protocol::BOUNDS['MAX_IR_VERSIONS']),
            'operations' => $bounded($operations, Protocol::BOUNDS['OPERATIONS_MAX_LINT']),
            'profiles' => $bounded($profiles, Protocol::BOUNDS['PROFILES_MAX']),
            'progress' => $progress,
            'protocol_versions' => $bounded($protocolVersions, Protocol::BOUNDS['PROTOCOL_VERSIONS_MAX']),
            'read_scopes' => $bounded($readScopes, Protocol::BOUNDS['MAX_SCOPES']),
            'targets' => $bounded($targets, Protocol::BOUNDS['TARGETS_MAX']),
            'transports' => $bounded($transports, Protocol::BOUNDS['TRANSPORTS_MAX']),
            'write_scopes' => $bounded($writeScopes, Protocol::BOUNDS['MAX_SCOPES']),
        ];
        foreach ($members as $member) {
            if ($member === null) {
                return self::refuse('bounds');
            }
        }
        if (count($capabilities) > 0) {
            if (count($capabilities) > Protocol::BOUNDS['MAX_DECLARED_CAPABILITIES']) {
                return self::refuse('capabilities');
            }
            $sorted = [];
            $keys = array_keys($capabilities);
            sort($keys, SORT_STRING);
            foreach ($keys as $id) {
                $state = $capabilities[$id];
                if (!in_array($id, array_column(Capabilities::DEFINITIONS, 'id'), true) || !in_array($state, Protocol::ENUMS['supportState'], true)) {
                    return self::refuse('capabilities');
                }
                $sorted[$id] = $state;
            }
            $members['capabilities'] = $sorted;
        }
        if (count($constraints) > 0) {
            $members['constraints'] = $constraints;
        }
        return $members;
    }

    /** Build and fully validate one success envelope. */
    public static function buildSuccess(array $request, array $adapter, ?array $capabilities, ?array $payload, ?array $writes, ?array $progress, ?string $planId): ?array {
        $evidence = ['adapter' => $adapter];
        if ($planId !== null) {
            $evidence['plan_id'] = $planId;
        }
        $envelope = [
            'evidence' => $evidence,
            'operation' => $request['operation'],
            'protocol' => Protocol::WIRE_TOKEN,
            'protocol_version' => $request['protocol_version'],
            'request_id' => $request['request_id'],
            'status' => 'ok',
        ];
        // Capabilities ride describe responses only (parity with the host
        // payload rules); other operations must not carry the member.
        if ($capabilities !== null && $request['operation'] === 'describe') {
            $envelope['capabilities'] = $capabilities;
        }
        if ($payload !== null) {
            $envelope['result'] = $payload;
        }
        if ($writes !== null) {
            $envelope['writes'] = $writes;
        }
        if ($progress !== null) {
            $envelope['progress'] = $progress;
        }
        return self::finishEnvelope($envelope, $request);
    }

    /** Build one closed in-envelope error envelope. */
    public static function buildErrorEnvelope(array $request, array $adapter, string $class, string $code, string $message, ?array $detail = null): ?array {
        if (!in_array($class, Protocol::ENUMS['errorClass'], true)) {
            return self::refuse('error-class');
        }
        if ($code === '' || Decode::utf8Length($code) > Protocol::BOUNDS['ERROR_CODE_MAX']) {
            return self::refuse('error-code');
        }
        if ($message === '' || Decode::utf8Length($message) > Protocol::BOUNDS['ERROR_MESSAGE_MAX']) {
            return self::refuse('error-message');
        }
        $error = ['class' => $class, 'code' => $code, 'message' => $message];
        if ($detail !== null) {
            if (count($detail) > Protocol::BOUNDS['ERROR_DETAILS_MAX']) {
                return self::refuse('error-detail');
            }
            foreach ($detail as $item) {
                if (Decode::utf8Length($item) > Protocol::BOUNDS['ERROR_DETAIL_MAX']) {
                    return self::refuse('error-detail');
                }
            }
            $error['detail'] = $detail;
        }
        $envelope = [
            'evidence' => ['adapter' => $adapter],
            'error' => $error,
            'operation' => $request['operation'],
            'protocol' => Protocol::WIRE_TOKEN,
            'protocol_version' => $request['protocol_version'],
            'request_id' => $request['request_id'],
            'status' => 'error',
        ];
        return self::finishEnvelope($envelope, $request);
    }

    private static function refuse(string $reason): array {
        return ['reason' => $reason];
    }

    /** Whether a builder result is a refusal (closed reason token). */
    public static function isRefusal(?array $result): bool {
        return $result !== null && array_key_exists('reason', $result) && !array_key_exists('status', $result);
    }

    private static function finishEnvelope(array $envelope, array $request): ?array {
        if (isset($envelope['writes'])) {
            $sorted = Canonical::sortWritePlan($envelope['writes']);
            if ($sorted === null) {
                return self::refuse('writes');
            }
            $envelope['writes'] = $sorted;
        }
        $admitted = Decode::admitResponse(Json::canonicalJson($envelope));
        if (!$admitted['ok']) {
            return self::refuse($admitted['reason']);
        }
        $identity = Decode::validateResponseIdentity($admitted['value'], $request['protocol_version'], $request['operation'], $request['request_id']);
        if ($identity !== null) {
            return self::refuse($identity['reason']);
        }
        $payload = Decode::validateResponsePayload($request, $admitted['value']);
        if ($payload !== null) {
            return self::refuse($payload['reason']);
        }
        if (isset($envelope['evidence']['plan_id']) && !Ids::isPlanId($envelope['evidence']['plan_id'])) {
            return self::refuse('plan-id');
        }
        return $envelope;
    }

    /** Build one wire Finding row. */
    public static function buildFinding(string $code, string $path, ?string $detail = null): ?array {
        if ($code === '' || Decode::utf8Length($code) > Protocol::BOUNDS['ERROR_CODE_MAX']) {
            return self::refuse('finding-code');
        }
        $finding = ['code' => $code, 'path' => $path];
        if ($detail !== null) {
            if (Decode::utf8Length($detail) > Protocol::BOUNDS['ERROR_DETAIL_MAX']) {
                return self::refuse('finding-detail');
            }
            $finding['detail'] = $detail;
        }
        return $finding;
    }

    /** Build one bounded, registry-checked diagnostic record. */
    public static function buildDiagnostic(array $codes, string $code, string $message, string $rule): ?array {
        $found = false;
        foreach ($codes as $entry) {
            if (($entry['code'] ?? $entry[1] ?? '') === $code) {
                $found = true;
            }
        }
        if (!$found) {
            return self::refuse('diagnostic-code');
        }
        if ($message === '' || Decode::utf8Length($message) > Protocol::BOUNDS['ERROR_MESSAGE_MAX']) {
            return self::refuse('diagnostic-message');
        }
        if (Decode::utf8Length($rule) > Protocol::BOUNDS['ERROR_DETAIL_MAX']) {
            return self::refuse('diagnostic-rule');
        }
        return ['code' => $code, 'message' => $message, 'rule' => $rule];
    }

    /** The canonical bytes of one envelope: the exact stdout form. */
    public static function responseBytes(array $envelope): string {
        return Json::canonicalJson($envelope);
    }
}

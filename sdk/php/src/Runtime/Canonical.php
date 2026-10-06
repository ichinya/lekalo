<?php
// Deterministic sorting, serialization, and result-building helpers
// (issue #106).

namespace Lekalo\AdapterSdk\Runtime;

use Lekalo\AdapterSdk\Generated\Protocol;

final class Canonical {
    private const SCENARIO_STATUSES = ['pass', 'fail', 'unsupported', 'infrastructure', 'degraded'];

    /** @return list<array<string,mixed>>|null */
    public static function sortWritePlan(array $entries): ?array {
        if (count($entries) > Protocol::BOUNDS['MAX_PLAN_ENTRIES']) {
            return null;
        }
        $sorted = [];
        foreach ($entries as $entry) {
            if (!is_array($entry) || array_is_list($entry)) {
                return null;
            }
            $path = $entry['path'] ?? null;
            if (!is_string($path) || !Paths::isLogicalPath($path)) {
                return null;
            }
            $action = $entry['action'] ?? null;
            if (!is_string($action) || !in_array($action, Protocol::ENUMS['writeAction'], true)) {
                return null;
            }
            if ($action === 'delete') {
                if (array_key_exists('sha256', $entry) && $entry['sha256'] !== null) {
                    return null;
                }
            } else {
                $digest = $entry['sha256'] ?? null;
                if (!is_string($digest) || !Ids::isSha256Digest($digest)) {
                    return null;
                }
            }
            $sorted[] = $entry;
        }
        usort($sorted, fn ($a, $b) => $a['path'] <=> $b['path']);
        return $sorted;
    }

    /** @param list<array<string,mixed>> $sorted */
    public static function canonicalPlanId(array $sorted): string {
        return Ids::planId($sorted);
    }

    public static function writeEntryDigest(string $content): string {
        return Ids::sha256($content);
    }

    /**
     * Build one artifact-manifest candidate: path-sorted, digest-addressed.
     *
     * @param list<array{0:string,1:string}> $entries
     * @return array<string,mixed>|null
     */
    public static function buildArtifactManifest(array $entries): ?array {
        if (count($entries) === 0 || count($entries) > Protocol::BOUNDS['MAX_PLAN_ENTRIES']) {
            return null;
        }
        $rows = [];
        foreach ($entries as [$path, $bytes]) {
            if (!Paths::isLogicalPath($path) || Paths::protectedHome($path) !== null) {
                return null;
            }
            $rows[] = ['path' => $path, 'action' => 'create', 'sha256' => Ids::sha256($bytes)];
        }
        usort($rows, fn ($a, $b) => $a['path'] <=> $b['path']);
        $rowsValue = array_map(fn ($row) => ['action' => $row['action'], 'path' => $row['path'], 'sha256' => $row['sha256']], $rows);
        return [
            'algorithm' => 'sha256',
            'canonicalForm' => Json::canonicalJson($rowsValue),
            'entries' => $rowsValue,
        ];
    }

    /** Normalize one scenario result (closed status set, sorted findings). */
    public static function normalizeScenarioResult(array $result): ?array {
        $status = $result['status'] ?? null;
        if (!is_string($status) || !in_array($status, self::SCENARIO_STATUSES, true)) {
            return null;
        }
        $findings = $result['findings'] ?? [];
        if (!is_array($findings) || count($findings) > 10000) {
            return null;
        }
        $rows = [];
        foreach ($findings as $finding) {
            if (!is_array($finding)) {
                return null;
            }
            $code = $finding['code'] ?? null;
            if (!is_string($code) || $code === '' || mb_strlen($code) > 128) {
                return null;
            }
            $path = $finding['path'] ?? null;
            if (!is_string($path) || !Paths::isLogicalPath($path)) {
                return null;
            }
            $detail = null;
            if (array_key_exists('detail', $finding) && $finding['detail'] !== null) {
                if (!is_string($finding['detail']) || mb_strlen($finding['detail']) > 128) {
                    return null;
                }
                $detail = $finding['detail'];
            }
            $rows[] = ['path' => $path, 'code' => $code, 'detail' => $detail];
        }
        usort($rows, fn ($a, $b) => [$a['path'], $a['code']] <=> [$b['path'], $b['code']]);
        $evidenceDigest = null;
        if (array_key_exists('evidenceDigest', $result) && $result['evidenceDigest'] !== null) {
            if (!is_string($result['evidenceDigest']) || !Ids::isSha256Digest($result['evidenceDigest'])) {
                return null;
            }
            $evidenceDigest = $result['evidenceDigest'];
        }
        $findingsValue = array_map(
            fn ($row) => array_filter(
                ['code' => $row['code'], 'path' => $row['path'], 'detail' => $row['detail']],
                fn ($value) => $value !== null
            ),
            $rows
        );
        return [
            'evidenceDigest' => $evidenceDigest,
            'findings' => $findingsValue,
            'status' => $status,
        ];
    }

    /** @param list<string> $writeScopes */
    public static function writePlanViolation(string $path, array $writeScopes): ?string {
        return Paths::planScopeViolation($path, $writeScopes);
    }
}

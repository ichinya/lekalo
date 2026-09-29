<?php

declare(strict_types=1);

namespace App\Http\Controllers;

use Illuminate\Http\JsonResponse;
use Illuminate\Http\Request;

/**
 * The maintained provider webhook of the planner fixture (issue #50):
 * provider deliveries update provider-owned task columns only. The
 * planning fields of `planner.user_task_planning` — planned_for,
 * position, focused_at, paused_at, reorder_version — are never a
 * provider surface; the ownership-separation scenario proves the
 * invariance byte for byte.
 */
final class ProviderWebhookController
{
    public function deliver(Request $request): JsonResponse
    {
        $body = (array) $request->json()->all();
        $taskId = (string) ($body['task_id'] ?? '');
        if ($taskId === '') {
            return response()->json(['ok' => false, 'error' => ['category' => 'validation', 'payload' => []]], 400);
        }
        $updated = \Illuminate\Support\Facades\DB::table('tasks')
            ->where('task_id', $taskId)
            ->update(['provider_synced_at' => (string) ($body['synced_at'] ?? gmdate('Y-m-d\TH:i:s\Z'))]);

        return response()->json(['ok' => true, 'updated' => $updated], 200);
    }
}

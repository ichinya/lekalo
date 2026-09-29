<?php

declare(strict_types=1);

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;
use Lekalo\Generated\Operations\ActorContext;

/**
 * The fixture authentication seam (issue #60): binds the generated
 * ActorContext under the attribute the generated boundary reads, and
 * rejects unauthenticated requests with the DECLARED auth error
 * envelope through the generated map — never an invented id, never an
 * anonymous fallback.
 *
 * Fixture identity seam: `X-Fixture-User` carries the actor id,
 * `X-Fixture-Mode` the declared scope dimensions (mode=bulk denies,
 * fail=store triggers the declared infrastructure failure).
 */
final class RequireActor
{
    public function handle(Request $request, Closure $next): mixed
    {
        $userId = (string) ($request->header('X-Fixture-User') ?? '');
        if ($userId === '') {
            $endpoint = 'planner.endpoint_focus_task_by_id';
            return response()->json(
                \Lekalo\Generated\Routes\ErrorHttpMap::domain($endpoint, 'planner.focus_denied', []),
                \Lekalo\Generated\Routes\ErrorHttpMap::status($endpoint, 'planner.focus_denied'),
            );
        }

        $scopes = ['mode' => (string) $request->header('X-Fixture-Mode', '')];
        $fail = (string) $request->header('X-Fixture-Fail', '');
        if ($fail !== '') {
            $scopes['fail'] = $fail;
        }
        $request->attributes->set(
            \Lekalo\Generated\Routes\HttpEnvelope::ACTOR_ATTRIBUTE,
            new ActorContext($userId, $scopes),
        );

        return $next($request);
    }
}

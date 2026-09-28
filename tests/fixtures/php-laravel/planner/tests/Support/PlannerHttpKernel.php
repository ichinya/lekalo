<?php

declare(strict_types=1);

namespace Tests\Support;

use Illuminate\Foundation\Http\Kernel as HttpKernel;

final class PlannerHttpKernel extends HttpKernel
{
    protected $middleware = [];

    protected $middlewareGroups = [
        'api' => [],
    ];

    protected $middlewarePriority = [];
}

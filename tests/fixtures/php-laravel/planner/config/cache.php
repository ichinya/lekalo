<?php

declare(strict_types=1);

// Cache configuration of the planner fixture: the array store keeps
// cached values inside the process (no host cache is touched).

return [
    'default' => env('CACHE_STORE', 'array'),
    'stores' => [
        'array' => [
            'driver' => 'array',
            'serialize' => false,
        ],
    ],
    'prefix' => 'lekalo_planner_cache',
];

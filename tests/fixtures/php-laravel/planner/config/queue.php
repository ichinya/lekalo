<?php

declare(strict_types=1);

// Queue configuration of the planner fixture: the sync driver runs
// every job inline, so queued work is observable without a host queue.

return [
    'default' => env('QUEUE_CONNECTION', 'sync'),
    'connections' => [
        'sync' => [
            'driver' => 'sync',
        ],
    ],
    'failed' => [
        'driver' => env('QUEUE_FAILED_DRIVER', 'null'),
    ],
];

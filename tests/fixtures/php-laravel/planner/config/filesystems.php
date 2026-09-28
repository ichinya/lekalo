<?php

declare(strict_types=1);

// Filesystem configuration of the planner fixture: local disks rooted
// inside the fixture's own storage tree only.

return [
    'default' => env('FILESYSTEM_DISK', 'local'),
    'disks' => [
        'local' => [
            'driver' => 'local',
            'root' => storage_path('app'),
        ],
    ],
];

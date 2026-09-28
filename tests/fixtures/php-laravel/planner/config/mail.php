<?php

declare(strict_types=1);

// Mail configuration of the planner fixture: the array mailer captures
// messages in-process, so no host mail transport is contacted.

return [
    'default' => env('MAIL_MAILER', 'array'),
    'mailers' => [
        'array' => [
            'transport' => 'array',
        ],
    ],
    'from' => [
        'address' => 'hello@example.com',
        'name' => 'Lekalo Planner Fixture',
    ],
];

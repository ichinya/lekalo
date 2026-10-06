<?php
// The SDK scan example (issue #106): a capability-limited scan-only
// adapter on the PHP adapter SDK. It declares describe and scan honestly
// and refuses everything else with a closed in-envelope error.

declare(strict_types=1);

use Lekalo\AdapterSdk\Runtime\Build;
use Lekalo\AdapterSdk\Runtime\Ids;
use Lekalo\AdapterSdk\Runtime\Serve;

$identity = Build::buildAdapterIdentity('sdk-scan-example', '0.1.0', Ids::sha256('lekalo adapter-sdk scan example'));

$handlers = [
    'describe' => function () use ($identity) {
        return Build::buildCapabilities(
            $identity,
            ['0.3.2'],
            ['describe', 'scan'],
            ['file', 'stdin'],
            ['sdk-scan-example'],
            [],
            ['src/**'],
            [],
            [],
            false,
            ['scan.symbols' => 'full']
        );
    },
    'scan' => function () {
        return ['payload' => ['entries' => [['kind' => 'source', 'path' => 'src/main.ts'], ['kind' => 'source', 'path' => 'src/util.ts']], 'truncated' => false]];
    },
];

exit(Serve::serve(['identity' => $identity, 'handlers' => $handlers]));

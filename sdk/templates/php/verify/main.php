<?php
// The SDK verify example (issue #106): a verification adapter on the
// PHP adapter SDK. It declares describe and verify honestly, reports
// findings through the typed builder, and never widens its declared
// read surface. Read-only; no writes; no network; no shell.

declare(strict_types=1);

use Lekalo\AdapterSdk\Runtime\Build;
use Lekalo\AdapterSdk\Runtime\Ids;
use Lekalo\AdapterSdk\Runtime\Serve;

$identity = Build::buildAdapterIdentity('sdk-verify-example', '0.1.0', Ids::sha256('lekalo adapter-sdk verify example'));

$handlers = [
    'describe' => function () use ($identity) {
        return Build::buildCapabilities(
            $identity,
            ['0.3.2'],
            ['describe', 'verify'],
            ['file', 'stdin'],
            ['sdk-verify-example'],
            [],
            ['.lekalo/ir/**'],
            [],
            ['0.2.16'],
            false,
            ['verify.scenarios' => 'full']
        );
    },
    'verify' => function (array $request) {
        $finding = Build::buildFinding('scenario-missing', 'src/main.ts', 'no scenarios declared');
        if ($finding === null) {
            return ['error' => ['errorClass' => 'invalid', 'errorCode' => 'finding-refused', 'message' => 'the finding builder refused the metadata']];
        }
        return ['payload' => ['findings' => [$finding], 'ok' => true]];
    },
];

exit(Serve::serve(['identity' => $identity, 'handlers' => $handlers]));

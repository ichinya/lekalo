<?php

declare(strict_types=1);

namespace App\Lekalo;

use Illuminate\Support\Facades\DB;
use Lekalo\Generated\Operations\TransactionPort;

/**
 * The maintained transaction adapter behind the generated
 * TransactionPort (issue #60 fixture integration): one transaction of
 * the default connection, commit on a normal return, roll back and
 * rethrow on any throwable — a typed domain failure surfaces as the
 * rolled-back domain failure, never as a committed half-state.
 */
final class DbTransactionPort implements TransactionPort
{
    public function run(callable $body): mixed
    {
        return DB::transaction($body);
    }
}

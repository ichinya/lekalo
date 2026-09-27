<?php

declare(strict_types=1);

namespace Vendor\Core;

use Vendor\Framework\Http;

final class Leaky
{
    public function run(): Http
    {
        return new Http();
    }
}

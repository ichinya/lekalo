<?php

declare(strict_types=1);

namespace Tests\Support;

use Illuminate\Foundation\Console\Kernel as ConsoleKernel;

final class PlannerConsoleKernel extends ConsoleKernel
{
    protected $commands = [];

    protected function bootstrappers(): array
    {
        // The fixture relies on the default framework bootstrappers; the
        // FilesystemServiceProvider binding (`files`) is registered by
        // the standard provider list in `config/app.php`.
        return parent::bootstrappers();
    }
}

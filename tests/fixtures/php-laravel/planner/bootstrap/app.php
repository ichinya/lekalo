<?php

declare(strict_types=1);

use Illuminate\Foundation\Application;

// The Laravel bootstrap of the planner fixture: a minimal application
// that returns the fixture's own Application instance. Mirrors the
// `bootstrap/app.php` contract Laratesto boots through.

/** @var Application $app */
$app = require __DIR__ . '/../app/application.php';

return $app;

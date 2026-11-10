<?php

declare(strict_types=1);

require __DIR__.'/vendor/autoload.php';
require __DIR__.'/src/planner.php';

use Illuminate\Container\Container;
use Illuminate\Database\Capsule\Manager;
use Illuminate\Events\Dispatcher;
use Illuminate\Http\Request;
use Illuminate\Routing\Router;
use Symfony\Component\HttpFoundation\JsonResponse;

$container = new Container();
$container->bind(\Illuminate\Routing\Contracts\CallableDispatcher::class, \Illuminate\Routing\CallableDispatcher::class);
$db = new Manager($container);
$db->addConnection([
    'driver' => 'pgsql', 'host' => getenv('DB_HOST') ?: '127.0.0.1',
    'port' => getenv('DB_PORT') ?: '56329', 'database' => 'lekalo_reference',
    'username' => 'postgres', 'password' => '', 'charset' => 'utf8', 'prefix' => '',
]);
$db->setAsGlobal();
$router = new Router(new Dispatcher($container), $container);
$router->any('/{path?}', function (string $path = '') use ($db): JsonResponse {
    try {
        return new JsonResponse((new ReferencePlanner($db->getConnection()))->handle(Request::capture()));
    } catch (ReferenceRefusal $error) {
        return new JsonResponse(['error' => $error->getMessage()], $error->status);
    } catch (Throwable) {
        return new JsonResponse(['error' => 'store_unavailable'], 503);
    }
})->where('path', '.*');
$router->dispatch(Request::capture())->send();

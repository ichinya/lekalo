FROM php:8.5-cli-alpine@sha256:d1fbbe78850d1eb9aa8f7da652ec5db61eb4c9377c8215cf5e23f3b528ed4f2b
RUN apk add --no-cache libpq-dev && docker-php-ext-install pdo_pgsql
RUN apk add --no-cache oniguruma-dev && docker-php-ext-install mbstring
WORKDIR /workspace/examples/planner-laravel
ENV PHP_CLI_SERVER_WORKERS=4
CMD ["php", "-d", "opcache.enable_cli=1", "-S", "0.0.0.0:8080", "router.php"]

#!/usr/bin/env bash
set -euo pipefail
: "${RUNNER_ROOT:?RUNNER_ROOT is required}"
: "${CI_DOCKER_NAME:?CI_DOCKER_NAME is required}"
for attempt in {1..60}; do
    if docker info >/dev/null 2>&1; then
        break
    fi
    sleep 1
done
test "$(docker info --format '{{.Name}}')" = "$CI_DOCKER_NAME"
cd "$RUNNER_ROOT"
exec ./run.sh

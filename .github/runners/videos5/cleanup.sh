#!/usr/bin/env bash
set -euo pipefail
: "${RUNNER_ROOT:?RUNNER_ROOT is required}"
: "${CI_DOCKER_NAME:?CI_DOCKER_NAME is required}"
# Refuse cleanup unless this is this runner's private Docker daemon.
test "$(docker info --format '{{.Name}}')" = "$CI_DOCKER_NAME"
mapfile -t containers < <(docker ps -aq --filter status=exited)
if ((${#containers[@]})); then docker rm -f "${containers[@]}"; fi
mapfile -t volumes < <(docker volume ls -q --filter dangling=true)
if ((${#volumes[@]})); then docker volume rm "${volumes[@]}"; fi
if [[ -n ${GITHUB_WORKSPACE:-} ]]; then
    case "$GITHUB_WORKSPACE" in
        "$RUNNER_ROOT"/_work/*)
            sudo chown -R "$(id -u):$(id -g)" "$GITHUB_WORKSPACE"
            ;;
        *) printf 'Unexpected workspace path; refusing ownership change\n' >&2; exit 1 ;;
    esac
fi

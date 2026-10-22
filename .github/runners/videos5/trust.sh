#!/usr/bin/env bash
set -euo pipefail
test "${GITHUB_REPOSITORY:-}" = 'ichinya/lekalo'
case "${GITHUB_EVENT_NAME:-}" in
    push|workflow_dispatch) ;;
    pull_request)
        test -f "${GITHUB_EVENT_PATH:?Missing event payload}"
        jq -e '.pull_request.head.repo.full_name == "ichinya/lekalo"' "$GITHUB_EVENT_PATH" >/dev/null
        ;;
    *) printf 'This runner accepts only trusted repository CI events\n' >&2; exit 1 ;;
esac

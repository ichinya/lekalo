#!/usr/bin/env bash
# Load only in a disposable GitHub-hosted VM. Ubuntu 26.04 already attaches
# its packaged bwrap profile to /usr/bin/bwrap; two equally specific
# attachments leave the executable unconfined and userns capabilities denied.
set -euo pipefail

test "${GITHUB_ACTIONS:-}" = true
test "${RUNNER_ENVIRONMENT:-}" = github-hosted
test "$(sysctl -n kernel.apparmor_restrict_unprivileged_userns)" = 1

policy="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/bwrap.apparmor"
system_policy=/etc/apparmor.d/bwrap-userns-restrict
state="${RUNNER_TEMP:?}/lekalo-ci-bwrap-apparmor"
profiles=/sys/kernel/security/apparmor/profiles

cleanup() {
  local result=0
  if sudo grep -Eq '^lekalo-ci-bwrap(-child)? ' "$profiles"; then
    sudo apparmor_parser --skip-cache --remove "$policy" || result=1
  fi
  if test -f "$state/restore-system-profile"; then
    # --replace also recovers a partially removed packaged policy.
    if sudo apparmor_parser --skip-cache --replace "$system_policy"; then
      rm -- "$state/restore-system-profile"
    else
      result=1
    fi
  fi
  if test "$result" = 0 && test -d "$state"; then
    rmdir -- "$state"
  fi
  return "$result"
}

case "${1:-}" in
  load)
    if sudo grep -Eq '^lekalo-ci-bwrap(-child)? ' "$profiles"; then
      printf '%s\n' 'Refusing to replace an existing Lekalo CI policy.' >&2
      exit 1
    fi
    mkdir -- "$state"
    trap 'result=$?; trap - EXIT; if test "$result" != 0; then cleanup || result=1; fi; exit "$result"' EXIT
    if sudo grep -q '^bwrap ' "$profiles"; then
      test -f "$system_policy"
      # Record restoration before unloading, including partial failures.
      touch "$state/restore-system-profile"
      sudo apparmor_parser --skip-cache --remove "$system_policy"
    fi
    sudo apparmor_parser --skip-cache --add "$policy"
    sudo grep -q '^lekalo-ci-bwrap (enforce)$' "$profiles"
    sudo grep -q '^lekalo-ci-bwrap-child (enforce)$' "$profiles"
    trap - EXIT
    ;;
  remove)
    # An earlier provisioning step may have failed before load ran.
    if test -d "$state"; then
      cleanup
    fi
    ;;
  *)
    printf 'Usage: %s load|remove\n' "$0" >&2
    exit 2
    ;;
esac

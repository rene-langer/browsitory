#!/usr/bin/env bash
# Installs Ubuntu packages on a GitHub-hosted runner, with bounded retries (#353).
#
#   scripts/ci-apt-install.sh PACKAGE...
#
# Why: the runner's first mirror (azure.archive.ubuntu.com, from /etc/apt/apt-mirrors.txt)
# sometimes serves packages at about 40 kB/s. Nothing fails: the 61 MB of Tauri build
# dependencies just took 13 to 30 minutes, and apt's own timeouts never fire because bytes keep
# arriving. So this script:
#   1. runs `apt-get update` (each attempt limited to UPDATE_LIMIT seconds),
#   2. downloads the packages only (`--download-only`, each attempt limited to DOWNLOAD_LIMIT
#      seconds). Before the second attempt it drops the first mirror from apt-mirrors.txt, so
#      apt fetches from the next one. Finished and partial downloads stay in apt's cache, so a
#      retry continues where the last attempt stopped,
#   3. installs from the cache. That needs no network and is never cut off, so dpkg is never
#      killed half-way through.
# Gives up after 3 attempts per phase. The workflow step's own timeout-minutes is the backstop.
set -euo pipefail

if [ "$#" -eq 0 ]; then
  echo "usage: $0 PACKAGE..." >&2
  exit 2
fi

ATTEMPTS=3
UPDATE_LIMIT=60
DOWNLOAD_LIMIT=180
MIRRORS=/etc/apt/apt-mirrors.txt

# Retries a connection that fails or goes silent for 30 s; dpkg waits up to 60 s for a lock
# another apt process holds instead of failing at once.
APT_OPTS=(
  -o Acquire::Retries=3
  -o Acquire::http::Timeout=30
  -o Acquire::https::Timeout=30
  -o DPkg::Lock::Timeout=60
)

# Moves on from the first mirror in apt-mirrors.txt, if there is another one to move on to.
drop_first_mirror() {
  [ -f "$MIRRORS" ] || return 0
  local count
  count="$(grep -cE '^[[:space:]]*[a-z]+://' "$MIRRORS" || true)"
  if [ "$count" -gt 1 ]; then
    local first
    first="$(grep -m1 -E '^[[:space:]]*[a-z]+://' "$MIRRORS")"
    echo "Dropping mirror $first from $MIRRORS for the next attempt."
    # Deletes the first mirror line only (sed `0,/re/` is GNU sed; runners are Ubuntu).
    sudo sed -i -E '0,/^[[:space:]]*[a-z]+:\/\//{/^[[:space:]]*[a-z]+:\/\//d}' "$MIRRORS"
  fi
}

# attempt LIMIT LABEL ON_RETRY COMMAND...: runs COMMAND as root, at most ATTEMPTS times, each
# time stopped after LIMIT seconds. Runs ON_RETRY (a function name, or `true`) before each retry.
attempt() {
  local limit=$1 label=$2 on_retry=$3
  shift 3
  local i status
  for ((i = 1; i <= ATTEMPTS; i++)); do
    if [ "$i" -gt 1 ]; then
      "$on_retry"
    fi
    status=0
    sudo timeout --kill-after=10 "$limit" "$@" || status=$?
    if [ "$status" -eq 0 ]; then
      return 0
    fi
    if [ "$status" -eq 124 ] || [ "$status" -eq 137 ]; then
      echo "::warning::$label: attempt $i of $ATTEMPTS ran over ${limit} s and was stopped."
    else
      echo "::warning::$label: attempt $i of $ATTEMPTS failed (exit status $status)."
    fi
  done
  echo "::error::$label failed $ATTEMPTS times; see the apt output above."
  return 1
}

if [ -f "$MIRRORS" ]; then
  echo "apt mirrors ($MIRRORS):"
  cat "$MIRRORS"
fi

attempt "$UPDATE_LIMIT" "apt-get update" true \
  apt-get "${APT_OPTS[@]}" update
attempt "$DOWNLOAD_LIMIT" "Downloading packages" drop_first_mirror \
  apt-get "${APT_OPTS[@]}" install -y --download-only "$@"
sudo apt-get "${APT_OPTS[@]}" install -y "$@"

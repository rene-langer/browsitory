#!/usr/bin/env bash
# Fails if a gpg daemon is still running for a throwaway gpg home dir after the test run (#188).
#
# The Rust tests create keyrings in the system temp dir (`bgpg*`) and must stop the gpg-agent
# that gpg auto-starts for each; a leaked agent holds an inotify instance forever. Matches only
# processes whose program name is gpg-agent/keyboxd/dirmngr (so never this script or an awk/grep
# that has those words in its arguments) and whose `--homedir` is a temp dir or a `bgpg*` dir,
# so a runner's own agent for ~/.gnupg is ignored.
#
# Meant for CI (a fresh runner). Locally, agents leaked by earlier runs also count.
# Usage: scripts/check-gpg-agent-leaks.sh
set -euo pipefail

# gpgconf --kill returns before the agent has fully exited; give stragglers a moment.
attempts=10

list_leaks() {
  ps -eo pid=,args= | awk \
    -v tmp1="/tmp/" \
    -v tmp2="${TMPDIR:-/tmp/}" \
    -v tmp3="${RUNNER_TEMP:-/tmp/}" '
    {
      prog = $2
      sub(/.*\//, "", prog)
      if (prog != "gpg-agent" && prog != "keyboxd" && prog != "dirmngr") next
      home = ""
      for (i = 3; i < NF; i++) if ($i == "--homedir") { home = $(i + 1); break }
      if (home == "") next
      base = home
      sub(/\/+$/, "", base)
      sub(/.*\//, "", base)
      if (base ~ /^bgpg/ || index(home, tmp1) == 1 || index(home, tmp2) == 1 ||
          index(home, tmp3) == 1 || index(home, "/private" tmp2) == 1)
        print
    }'
}

for ((i = 1; i <= attempts; i++)); do
  leaks="$(list_leaks)"
  if [[ -z "$leaks" ]]; then
    echo "No gpg daemons left running for temp gpg home dirs."
    exit 0
  fi
  sleep 1
done

echo "::error::gpg daemons leaked by the test run (see #188):" >&2
echo "$leaks" >&2
exit 1

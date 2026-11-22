#!/usr/bin/env bash
# Update Aurelius on this Linux computer to this checkout's main: the server
# first, then the app. This is the one command for an update; the in-app updater
# only installs Windows releases.
#
#   scripts/update-linux.sh                    update what main has new
#   scripts/update-linux.sh --force            rebuild and replace both even when up to date
#   scripts/update-linux.sh --prefix <dir>     the install of install-linux.sh --prefix <dir>
#
# main is first brought up to date with origin/main when that only moves it
# forward. A running server is restarted on the new version (that stops the
# answers it is working on); a running app is closed and opened again on the
# new version, so it never talks to the old server. Your data and the pairing stay.
# Only one of the two: scripts/update-server.sh or scripts/update-desktop.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

args=()
prefix="$HOME/.local"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --force) args+=(--force) ;;
        --prefix) prefix="${2:?--prefix needs a folder}"; args+=(--prefix "$prefix"); shift ;;
        -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

if [[ ! -x "$prefix/bin/aurelius-daemon" || ! -x "$prefix/bin/app/aurelius" ]]; then
    echo "Aurelius isn't fully installed in $prefix/bin yet: run scripts/install-linux.sh" >&2
    exit 1
fi

# The server before the app: the app restarts last and meets the new server.
scripts/update-server.sh "${args[@]}"
scripts/update-desktop.sh "${args[@]}"

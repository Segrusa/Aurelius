#!/usr/bin/env bash
# Update the installed Linux server (~/.local/bin/aurelius-daemon) to this checkout's main.
# Only the server programs are rebuilt and replaced: aurelius-daemon, the
# approval bridge (aurelius-bridge) and aurelius-ctl. The app is not touched,
# and no tests run (use install-linux.sh for a full install, and
# update-desktop.sh for the app).
#
#   scripts/update-server.sh                    rebuild and replace the server if main has something new
#   scripts/update-server.sh --force            rebuild and replace even when it's up to date
#   scripts/update-server.sh --prefix <dir>     the install of install-linux.sh --prefix <dir>
#
# main is first brought up to date with origin/main when that only moves it
# forward; commits that are only on this computer stay and are built too.
# A running server is restarted on the new version, which stops the answers it
# is working on; a server that wasn't running stays off. Your data and the
# pairing stay.
set -euo pipefail
cd "$(dirname "$0")/.."

prefix="$HOME/.local"
force=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --force) force=1 ;;
        --prefix) prefix="${2:?--prefix needs a folder}"; shift ;;
        -h|--help) sed -n '2,/^set -euo/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

if [[ "$(uname)" != "Linux" ]]; then
    echo "This script is for Linux." >&2
    exit 1
fi
bin="$prefix/bin"
if [[ ! -x "$bin/aurelius-daemon" ]]; then
    echo "The Aurelius server isn't installed in $bin yet: run scripts/install-linux.sh" >&2
    exit 1
fi
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
source scripts/linux-common.sh

commit_file="$bin/aurelius-server.commit"
old_commit="$(cat "$commit_file" 2>/dev/null || true)"

fetch_main
new_commit="$(git rev-parse HEAD)"
new_version="$(workspace_version)"

if [[ "$old_commit" == "$new_commit" && $force -eq 0 ]]; then
    echo "Already up to date: the server is at ${new_commit:0:7}."
    exit 0
fi
was="${old_commit:0:7}"
echo "Updating the server ${was:-unknown} -> $new_version (${new_commit:0:7}) ..."

cargo build --release -p aurelius-daemon -p aurelius-agent-bridge -p aurelius-client \
    || { echo "Build failed; the installed server is unchanged." >&2; exit 1; }

running="$(daemon_pids)"

# Atomic replacement: a running server keeps its old program until restarted.
for program in aurelius-daemon aurelius-bridge aurelius-ctl; do
    install -m 755 "target/release/$program" "$bin/.$program.new"
    mv -f "$bin/.$program.new" "$bin/$program"
done
printf '%s' "$new_commit" > "$commit_file"
remove_windows_leftovers

if [[ -n "$running" ]]; then
    echo "Restarting the running server on the new version ..."
    restart_daemon
    echo "The new server is started. If an app is open and does not reconnect, restart it."
else
    echo "The server wasn't running; it starts on the new version next time."
fi
echo "Aurelius server updated to $new_version (${new_commit:0:7})."

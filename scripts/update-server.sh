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

branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$branch" != "main" ]]; then
    echo "This checkout is on '$branch', not main. Switch first: git switch main" >&2
    exit 1
fi
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "This checkout has local changes. Commit or stash them first (git status)." >&2
    exit 1
fi

commit_file="$bin/aurelius-server.commit"
old_commit="$(cat "$commit_file" 2>/dev/null || true)"

echo "Fetching origin/main ..."
if git fetch -q origin main 2>/dev/null; then
    if git merge-base --is-ancestor HEAD FETCH_HEAD; then
        git merge -q --ff-only FETCH_HEAD
    elif ! git merge-base --is-ancestor FETCH_HEAD HEAD; then
        echo "Note: main and origin/main have both moved on; building the local main as it is."
    fi
else
    echo "Note: origin could not be reached; building the local main as it is."
fi
new_commit="$(git rev-parse HEAD)"
new_version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

if [[ "$old_commit" == "$new_commit" && $force -eq 0 ]]; then
    echo "Already up to date: the server is at ${new_commit:0:7}."
    exit 0
fi
was="${old_commit:0:7}"
echo "Updating the server ${was:-unknown} -> $new_version (${new_commit:0:7}) ..."

cargo build --release -p aurelius-daemon -p aurelius-agent-bridge -p aurelius-client \
    || { echo "Build failed; the installed server is unchanged." >&2; exit 1; }

# The servers of this install only: their program is $bin/aurelius-daemon
# (shown as "... (deleted)" once it has been replaced), not another prefix's.
daemon_pids() {
    local pid exe
    for pid in $(pgrep -u "$(id -u)" -x aurelius-daemon 2>/dev/null || true); do
        exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
        [[ "$exe" == "$bin/aurelius-daemon" || "$exe" == "$bin/aurelius-daemon (deleted)" ]] && echo "$pid"
    done
    return 0
}
running="$(daemon_pids)"

# Atomic replacement: a running server keeps its old program until restarted.
for program in aurelius-daemon aurelius-bridge aurelius-ctl; do
    install -m 755 "target/release/$program" "$bin/.$program.new"
    mv -f "$bin/.$program.new" "$bin/$program"
done
printf '%s' "$new_commit" > "$commit_file"

if [[ -n "$running" ]]; then
    echo "Restarting the running server on the new version ..."
    # `aurelius-ctl restart` needs a signed-in board member; `shutdown` goes over
    # the internal channel and needs none. Whatever is still running after 10 s
    # is stopped, then the new program is started.
    "$bin/aurelius-ctl" shutdown >/dev/null 2>&1 || true
    for _ in $(seq 1 50); do
        [[ -z "$(daemon_pids)" ]] && break
        sleep 0.2
    done
    left="$(daemon_pids)"
    # shellcheck disable=SC2086 # one pid per word
    if [[ -n "$left" ]]; then
        kill -TERM $left 2>/dev/null || true
        sleep 2
        left="$(daemon_pids)"
        if [[ -n "$left" ]]; then kill -KILL $left 2>/dev/null || true; fi
    fi
    setsid "$bin/aurelius-daemon" </dev/null >/dev/null 2>&1 &
    echo "The new server is started. If an app is open and does not reconnect, restart it."
else
    echo "The server wasn't running; it starts on the new version next time."
fi
echo "Aurelius server updated to $new_version (${new_commit:0:7})."

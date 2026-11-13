#!/usr/bin/env bash
# Update the installed Linux desktop app (~/.local/bin/app) to this checkout's main.
# Only the app is rebuilt and replaced: the server (aurelius-daemon), the bridge
# and aurelius-ctl are not touched, and no tests run (use install-linux.sh for
# a full install).
#
#   scripts/update-desktop.sh                    rebuild and replace the app if main has something new
#   scripts/update-desktop.sh --force            rebuild and replace even when it's up to date
#   scripts/update-desktop.sh --prefix <dir>     the install of install-linux.sh --prefix <dir>
#
# main is first brought up to date with origin/main when that only moves it
# forward; commits that are only on this computer stay and are built too.
# The app is started again if it was running. Your data and the pairing stay.
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
if [[ ! -x "$bin/app/aurelius" ]]; then
    echo "Aurelius isn't installed in $bin yet: run scripts/install-linux.sh" >&2
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

old_commit="$(cat "$bin/aurelius.commit" 2>/dev/null || true)"
old_version="$(cat "$bin/aurelius.version" 2>/dev/null || echo "?")"

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
    echo "Already up to date: Aurelius $old_version (${new_commit:0:7})."
    exit 0
fi
echo "Updating the app $old_version (${old_commit:0:7}) -> $new_version (${new_commit:0:7}) ..."

fail() { echo "Build failed; the installed app is unchanged." >&2; exit 1; }
cargo build --release -p aurelius-client || fail
npx -y pnpm@12.16.1 install --frozen-lockfile || fail
npx -y pnpm@12.16.1 build || fail
npx -y pnpm@12.16.1 --filter @aurelius/electron package || fail

# A running app is stopped, replaced and started again. The server keeps running.
running=0
# Started as app/aurelius or through the aurelius link; not aurelius-daemon or the others.
app_pattern="^$bin/(app/)?aurelius( |$)"
if pgrep -u "$(id -u)" -f "$app_pattern" >/dev/null 2>&1; then
    running=1
    pkill -TERM -u "$(id -u)" -f "$app_pattern" || true
    for _ in $(seq 1 50); do
        pgrep -u "$(id -u)" -f "$app_pattern" >/dev/null 2>&1 || break
        sleep 0.2
    done
    pkill -KILL -u "$(id -u)" -f "$app_pattern" 2>/dev/null || true
fi

rm -rf "$bin/.app.new" "$bin/.app.old"
cp -a "clients/electron/release/linux-$(node -p process.arch)" "$bin/.app.new"
mv "$bin/app" "$bin/.app.old"
mv "$bin/.app.new" "$bin/app"
rm -rf "$bin/.app.old"
printf '%s' "$new_commit" > "$bin/aurelius.commit"

if [[ $running -eq 1 ]]; then
    setsid nohup "$bin/aurelius" </dev/null >/dev/null 2>&1 &
fi
echo "Aurelius app updated to $new_version (${new_commit:0:7})."

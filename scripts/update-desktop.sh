#!/usr/bin/env bash
# Update the installed Linux desktop app (~/.local/bin/app) to this checkout's main.
# Only the app is rebuilt and replaced: the server (aurelius-daemon), the bridge
# and aurelius-ctl are not touched, and no tests run (use install-linux.sh for
# a full install, and update-server.sh for only the server).
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
source scripts/linux-common.sh

old_commit="$(cat "$bin/aurelius.commit" 2>/dev/null || true)"
old_version="$(cat "$bin/aurelius.version" 2>/dev/null || echo "?")"

fetch_main
new_commit="$(git rev-parse HEAD)"
new_version="$(workspace_version)"

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
if app_running; then
    running=1
    stop_app
fi

rm -rf "$bin/.app.new" "$bin/.app.old"
cp -a "clients/electron/release/linux-$(node -p process.arch)" "$bin/.app.new"
mv "$bin/app" "$bin/.app.old"
mv "$bin/.app.new" "$bin/app"
rm -rf "$bin/.app.old"
printf '%s' "$new_commit" > "$bin/aurelius.commit"
printf '%s' "$new_version" > "$bin/aurelius.version"

if [[ $running -eq 1 ]]; then
    start_app
fi
echo "Aurelius app updated to $new_version (${new_commit:0:7})."

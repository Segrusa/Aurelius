#!/usr/bin/env bash
# Update the installed Aurelius.app on the Mac to the latest main.
# (The in-app updater installs Windows packages only.)
#
#   scripts/update-mac.sh           pull main, rebuild and replace the app if there is something new
#   scripts/update-mac.sh --force   rebuild and replace even when it's up to date
#
# The app stays where it is (/Applications or ~/Applications) and is started
# again if it was running. Your data and the pairing with the server stay.
set -euo pipefail
cd "$(dirname "$0")/.."

force=0
case "${1:-}" in
    "") ;;
    --force) force=1 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
esac

if [[ "$(uname)" != "Darwin" ]]; then
    echo "This script is for macOS." >&2
    exit 1
fi

installed=""
for dir in /Applications "$HOME/Applications"; do
    if [[ -d "$dir/Aurelius.app" ]]; then installed="$dir/Aurelius.app"; break; fi
done
if [[ -z "$installed" ]]; then
    echo "Aurelius isn't installed yet: run scripts/install-mac.sh" >&2
    exit 1
fi
plist="$installed/Contents/Info.plist"
read_plist() { /usr/libexec/PlistBuddy -c "Print :$1" "$plist" 2>/dev/null || echo "?"; }
old_version="$(read_plist CFBundleShortVersionString)"
old_commit="$(read_plist AureliusCommit)"

branch="$(git rev-parse --abbrev-ref HEAD)"
if [[ "$branch" != "main" ]]; then
    echo "This checkout is on '$branch', not main. Switch first: git switch main" >&2
    exit 1
fi
if ! git diff --quiet || ! git diff --cached --quiet; then
    echo "This checkout has local changes. Commit or stash them first (git status)." >&2
    exit 1
fi

echo "Fetching the latest main ..."
git pull --ff-only
new_version="$(sed -n 's/^version = "\(.*\)"/\1/p' clients/desktop/Cargo.toml | head -1)"
new_commit="$(git rev-parse HEAD)"

if [[ "$old_commit" == "$new_commit" && $force -eq 0 ]]; then
    echo "Already up to date: Aurelius $old_version (${new_commit:0:7})."
    exit 0
fi
echo "Updating Aurelius $old_version (${old_commit:0:7}) → $new_version (${new_commit:0:7}) ..."

args=(--dest "$(dirname "$installed")")
if pgrep -x aurelius >/dev/null 2>&1; then args+=(--open); fi
exec "$(dirname "$0")/install-mac.sh" "${args[@]}"

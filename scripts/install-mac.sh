#!/usr/bin/env bash
# Install Aurelius on the Mac as an app: Aurelius.app with its icon in
# /Applications (or ~/Applications if that isn't writable), so it shows up in
# Launchpad, Spotlight and the Dock. The Mac is a CLIENT of your Aurelius
# server: only the app is installed, no background service.
#
#   scripts/install-mac.sh                 build and install
#   scripts/install-mac.sh --open          ... and start it
#   scripts/install-mac.sh --dest <dir>    install into <dir> instead
#
# Running it again replaces the app in place (that's how scripts/update-mac.sh
# updates); your data in ~/Library/Application Support/Aurelius and the pairing
# with the server stay.
#
# First time: in the app, Settings → Servers (⌘⌥S) → N, paste the pairing text
# from the server, Enter, then "Allow as owner" on the server.
#
# The build is signed ad hoc (no Apple Developer ID yet). After an update macOS
# may ask once whether Aurelius may use its Keychain items: choose "Always Allow".
#
# Needs: Xcode, rustup (rust-toolchain.toml picks
# the toolchain). One-time: chmod +x scripts/*.sh
set -euo pipefail
cd "$(dirname "$0")/.."

dest=""
open_after=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --open) open_after=1 ;;
        --dest) dest="${2:?--dest needs a folder}"; shift ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
    shift
done

if [[ "$(uname)" != "Darwin" ]]; then
    echo "This script is for macOS." >&2
    exit 1
fi
# rustup installs into ~/.cargo; a shell without it on PATH still finds it here.
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust is needed: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    exit 1
fi
if ! xcrun --find metal >/dev/null 2>&1; then
    echo "Xcode (with the Metal toolchain) is needed: install Xcode, then run: sudo xcode-select -s /Applications/Xcode.app" >&2
    exit 1
fi

if [[ -z "$dest" ]]; then
    if [[ -w /Applications ]]; then dest=/Applications; else dest="$HOME/Applications"; fi
fi
mkdir -p "$dest"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
# The commit is kept in Info.plist, so scripts/update-mac.sh knows what's installed.
commit="$(git rev-parse HEAD 2>/dev/null || echo unknown)"

echo "Building Aurelius $version ..."
npx -y pnpm@12.16.1 install --frozen-lockfile
npx -y pnpm@12.16.1 build
npx -y pnpm@12.16.1 --filter @aurelius/electron package

# The packager made the bundle, with the aurelius:// scheme and the icon. The commit goes in its
# Info.plist, then an ad-hoc signature over the whole of it: Apple Silicon only runs signed code.
app="clients/electron/release/darwin-$(node -p process.arch)/Aurelius.app"
cp THIRD-PARTY-LICENSES.md "$app/Contents/Resources/"
/usr/libexec/PlistBuddy -c "Add :AureliusCommit string $commit" "$app/Contents/Info.plist"
codesign --force --deep --sign - "$app"

# Quit a running Aurelius, then swap the app in one move.
if pgrep -x aurelius >/dev/null 2>&1; then
    echo "Quitting the running Aurelius ..."
    osascript -e 'quit app "Aurelius"' >/dev/null 2>&1 || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        pgrep -x aurelius >/dev/null 2>&1 || break
        sleep 0.5
    done
    pkill -x aurelius 2>/dev/null || true
fi
target="$dest/Aurelius.app"
rm -rf "$target.new"
cp -R "$app" "$target.new"
rm -rf "$target"
mv "$target.new" "$target"

# Make Finder, the Dock and Launchpad pick up the (new) icon.
touch "$target"
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
if [[ -x "$lsregister" ]]; then "$lsregister" -f "$target" || true; fi

echo "Aurelius $version is installed: $target"
echo "First time? Settings → Servers (⌘⌥S) → N to pair with your server."
if [[ $open_after -eq 1 ]]; then
    open "$target"
fi

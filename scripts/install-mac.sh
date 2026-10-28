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
# Needs: Xcode (gpui compiles Metal shaders), rustup (rust-toolchain.toml picks
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
version="$(sed -n 's/^version = "\(.*\)"/\1/p' clients/desktop/Cargo.toml | head -1)"
# The commit is kept in Info.plist, so scripts/update-mac.sh knows what's installed.
commit="$(git rev-parse HEAD 2>/dev/null || echo unknown)"

echo "Building Aurelius $version ..."
cargo build -p aurelius-desktop --release

# The bundle: the program, the icon, the licences and Info.plist.
app=target/release/Aurelius.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/aurelius "$app/Contents/MacOS/aurelius"
cp clients/desktop/assets/aurelius.icns "$app/Contents/Resources/aurelius.icns"
cp THIRD-PARTY-LICENSES.md "$app/Contents/Resources/"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Aurelius</string>
    <key>CFBundleDisplayName</key><string>Aurelius</string>
    <key>CFBundleIdentifier</key><string>com.segrusa.aurelius.desktop</string>
    <key>CFBundleExecutable</key><string>aurelius</string>
    <key>CFBundleIconFile</key><string>aurelius</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.productivity</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>AureliusCommit</key><string>$commit</string>
</dict>
</plist>
PLIST
# Apple Silicon only runs signed code; an ad-hoc signature over the whole
# bundle also ties Info.plist and the icon to it.
codesign --force --sign - "$app"

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

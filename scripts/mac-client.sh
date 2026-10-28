#!/usr/bin/env bash
# Aurelius on the Mac as a CLIENT of your Aurelius server.
# The server (Windows PC) keeps projects, keys and runtimes; this app shows it
# over Tailscale. The Mac never starts a background service of its own.
#
#   scripts/mac-client.sh            dev app  → pairs with the dev server (port 8444)
#   scripts/mac-client.sh --release  production app → pairs with the production server (8443)
#
# For an installed app (Aurelius.app with its icon in /Applications) use
# scripts/install-mac.sh, and scripts/update-mac.sh to update it.
#
# First time: in the app, Settings → Servers (⌘⌥S) → N, paste the pairing text
# from the server (Settings → Remote access → N → Copy pairing text), Enter,
# then "Allow as owner" on the server.
#
# Needs: Xcode (gpui compiles Metal shaders), rustup (rust-toolchain.toml picks
# the toolchain), Tailscale signed in to the same tailnet as the server.
# One-time: chmod +x scripts/mac-client.sh
set -euo pipefail
cd "$(dirname "$0")/.."

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

if [[ "${1:-}" == "--release" ]]; then
    export AURELIUS_ENV=production
    exec cargo run -p aurelius-desktop --release
fi
export AURELIUS_ENV=dev
# Unsigned dev builds change with every build; the Keychain would ask each time.
export AURELIUS_SECRETS="${AURELIUS_SECRETS:-file}"
exec cargo run -p aurelius-desktop

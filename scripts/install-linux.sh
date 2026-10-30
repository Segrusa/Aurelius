#!/usr/bin/env bash
# Install Aurelius on this Linux computer, for your user only (no root needed
# unless you ask for --install-deps): the programs go to ~/.local/bin and the
# app shows up in your application menu with its icon.
#
#   scripts/install-linux.sh                    build and install
#   scripts/install-linux.sh --open             ... and start it
#   scripts/install-linux.sh --install-deps     also install the system packages (sudo)
#   scripts/install-linux.sh --prefix <dir>     install into <dir>/bin instead of ~/.local
#
# Running it again replaces the programs in place (that's how you update: git
# pull, then run it again). Your data in ~/.local/share/Aurelius and your
# secrets stay. A running server (aurelius-daemon) is restarted on the new
# version, which stops the answers it is working on; a running app keeps its
# old version until you restart it.
#
# The app starts its own server (aurelius-daemon) in the background. The programs
# must stay together in one folder: the app finds the server and the approval
# bridge next to itself.
#
# Needs: Rust (rustup; rust-toolchain.toml picks the toolchain), a C compiler,
# cmake, clang and the Wayland, fontconfig, GBM/DRM and PipeWire development
# libraries. Use
# --install-deps to install them, or install them yourself with the command the
# script prints. One-time: chmod +x scripts/install-linux.sh
set -euo pipefail
cd "$(dirname "$0")/.."

prefix="$HOME/.local"
open_after=0
install_deps=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --open) open_after=1 ;;
        --install-deps) install_deps=1 ;;
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
# rustup installs into ~/.cargo; a shell without it on PATH still finds it here.
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then
    source "$HOME/.cargo/env"
fi
if ! command -v cargo >/dev/null 2>&1; then
    echo "Rust is needed: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    exit 1
fi

# The system packages, by distribution family (from /etc/os-release).
family="$(. /etc/os-release && echo " $ID ${ID_LIKE:-} ")"
case "$family" in
    *" arch "*|*" omarchy "*|*" cachyos "*|*" manjaro "*)
        manager=pacman
        packages="base-devel pkgconf cmake clang fontconfig wayland mesa libdrm pipewire vulkan-icd-loader" ;;
    *" debian "*|*" ubuntu "*)
        manager=apt
        packages="build-essential pkg-config cmake libclang-dev libfontconfig-dev libwayland-dev libgbm-dev libdrm-dev libpipewire-0.3-dev libspa-0.2-dev libvulkan1" ;;
    *" fedora "*|*" rhel "*)
        manager=dnf
        packages="gcc gcc-c++ make pkgconf-pkg-config cmake clang-devel fontconfig-devel wayland-devel mesa-libgbm-devel libdrm-devel pipewire-devel vulkan-loader" ;;
    *) manager="" ;;
esac

# Missing pieces: a compiler, cmake, clang, and the libraries the build links.
missing=()
for tool in cc cmake clang; do
    command -v "$tool" >/dev/null 2>&1 || missing+=("$tool")
done
if ! command -v pkg-config >/dev/null 2>&1; then
    missing+=("pkg-config")
else
    for module in fontconfig wayland-client gbm libdrm libpipewire-0.3 libspa-0.2; do
        pkg-config --exists "$module" || missing+=("$module")
    done
fi

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "Missing for the build: ${missing[*]}"
    case "$manager" in
        pacman) install="sudo pacman -S --needed $packages" ;;
        apt) install="sudo apt install $packages" ;;
        dnf) install="sudo dnf install $packages" ;;
        *) install="" ;;
    esac
    if [[ -z "$install" ]]; then
        echo "Your distribution isn't recognised: install the development packages for cmake, clang, pkg-config, fontconfig, Wayland, GBM/DRM, PipeWire and SPA yourself." >&2
        exit 1
    fi
    if [[ $install_deps -eq 0 ]]; then
        echo "Install them with:"
        echo "  $install"
        echo "or run this script again with --install-deps."
        exit 1
    fi
    echo "Installing the system packages ..."
    $install
fi

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

echo "Building Aurelius $version ..."
cargo build --release -p aurelius-desktop -p aurelius-daemon -p aurelius-agent-bridge -p aurelius-client

# Atomic replacement: a running Aurelius keeps its old program until restarted.
bin="$prefix/bin"
mkdir -p "$bin"
for program in aurelius aurelius-daemon aurelius-bridge aurelius-ctl; do
    install -m 755 "target/release/$program" "$bin/.$program.new"
    mv -f "$bin/.$program.new" "$bin/$program"
done

# The application menu entry and the icon.
icons="$prefix/share/icons/hicolor/512x512/apps"
apps="$prefix/share/applications"
mkdir -p "$icons" "$apps"
install -m 644 clients/desktop/assets/aurelius-512.png "$icons/aurelius.png"
cat > "$apps/aurelius.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Aurelius
Comment=Your own AI team on Claude Code
Exec=$bin/aurelius
Icon=aurelius
Terminal=false
Categories=Office;Utility;
DESKTOP
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$apps" || true; fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true; fi

echo "Aurelius $version is installed in $bin"
# The app connects to a server that is already running and never restarts it,
# so without this the old server would keep going.
if pgrep -u "$(id -u)" -x aurelius-daemon >/dev/null 2>&1; then
    echo "Restarting the running server on the new version ..."
    "$bin/aurelius-ctl" restart || echo "Could not restart the server: run '$bin/aurelius-ctl restart' yourself." >&2
fi
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "Note: $bin is not on your PATH. The application menu entry works anyway." ;;
esac
echo "First time? Claude Code must be installed and logged in (https://claude.com/claude-code)."
if [[ $open_after -eq 1 ]]; then
    nohup "$bin/aurelius" >/dev/null 2>&1 &
fi

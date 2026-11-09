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
cargo build --release -p aurelius-daemon -p aurelius-agent-bridge -p aurelius-client

# The app is a folder, built from clients/electron (Node and pnpm via npx).
npx -y pnpm@12.16.1 install --frozen-lockfile
npx -y pnpm@12.16.1 build
npx -y pnpm@12.16.1 --filter @aurelius/electron package

# Atomic replacement: a running Aurelius keeps its old program until restarted.
bin="$prefix/bin"
mkdir -p "$bin"
for program in aurelius-daemon aurelius-bridge aurelius-ctl; do
    install -m 755 "target/release/$program" "$bin/.$program.new"
    mv -f "$bin/.$program.new" "$bin/$program"
done
# The app beside the service, which it finds as ../aurelius-daemon from its own folder.
rm -rf "$bin/.app.new" "$bin/.app.old"
cp -a "clients/electron/release/linux-$(node -p process.arch)" "$bin/.app.new"
if [[ -e "$bin/app" ]]; then mv "$bin/app" "$bin/.app.old"; fi
mv "$bin/.app.new" "$bin/app"
rm -rf "$bin/.app.old"
ln -sfn app/aurelius "$bin/.aurelius.new"
mv -Tf "$bin/.aurelius.new" "$bin/aurelius"
printf '%s' "$version" > "$bin/aurelius.version"

# The application menu entry and the icon.
icons="$prefix/share/icons/hicolor/512x512/apps"
apps="$prefix/share/applications"
mkdir -p "$icons" "$apps"
install -m 644 clients/electron/assets/aurelius-512.png "$icons/aurelius.png"
cat > "$apps/aurelius.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Aurelius
Comment=Your own AI team on Claude Code
Exec=$bin/aurelius %u
Icon=aurelius
Terminal=false
Categories=Office;Utility;
MimeType=x-scheme-handler/aurelius;
DESKTOP
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$apps" || true; fi
# aurelius:// links (copied from the app) open Aurelius. Only this user's default is set.
if command -v xdg-mime >/dev/null 2>&1; then xdg-mime default aurelius.desktop x-scheme-handler/aurelius || true; fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" || true; fi

echo "Aurelius $version is installed in $bin"
# The app connects to a server that is already running and never restarts it,
# so without this the old server would keep going.
if pgrep -u "$(id -u)" -x aurelius-daemon >/dev/null 2>&1; then
    echo "Restarting the running server on the new version ..."
    # An older server of another protocol major refuses the new aurelius-ctl
    # ("Update needed"): then stop its process and start the new one ourselves.
    if ! "$bin/aurelius-ctl" restart; then
        echo "The running server did not take the restart; stopping it ..."
        pkill -TERM -u "$(id -u)" -x aurelius-daemon || true
        for _ in $(seq 1 50); do
            pgrep -u "$(id -u)" -x aurelius-daemon >/dev/null 2>&1 || break
            sleep 0.2
        done
        pkill -KILL -u "$(id -u)" -x aurelius-daemon 2>/dev/null || true
        setsid "$bin/aurelius-daemon" </dev/null >/dev/null 2>&1 &
        echo "The new server is started. Restart a running app too."
    fi
fi
case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo "Note: $bin is not on your PATH. The application menu entry works anyway." ;;
esac
echo "First time? Claude Code must be installed and logged in (https://claude.com/claude-code)."
if [[ $open_after -eq 1 ]]; then
    nohup "$bin/aurelius" >/dev/null 2>&1 &
fi

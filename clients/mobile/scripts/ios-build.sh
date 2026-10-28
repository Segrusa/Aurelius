#!/usr/bin/env bash
# Build and install the Aurelius companion on an iPhone from the MacBook.
#
#   clients/mobile/scripts/ios-build.sh                 # list devices, run on the only iPhone found
#   clients/mobile/scripts/ios-build.sh -d my-iphone   # release build, installed on that device
#   clients/mobile/scripts/ios-build.sh --ipa           # build an .ipa (build/ios/ipa/)
#   clients/mobile/scripts/ios-build.sh --dev ...       # same build; reminder about the dev daemon
#
# One-time: chmod +x clients/mobile/scripts/ios-build.sh
# See docs/mobile.md for signing, push and pairing.
set -euo pipefail

usage() {
  sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

device="${IOS_DEVICE:-}"
mode="run"
dev=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    -d|--device)
      [[ $# -ge 2 ]] || { echo "error: $1 needs a device id or name" >&2; exit 2; }
      device="$2"; shift 2 ;;
    --ipa) mode="ipa"; shift ;;
    --dev) dev=1; shift ;;
    -h|--help) usage 0 ;;
    *) echo "error: unknown argument '$1'" >&2; usage 2 ;;
  esac
done

step() { printf '\n==> %s\n' "$*"; }
fail() { printf '\nerror: %s\n' "$*" >&2; exit 1; }

# The Flutter project is the parent of this script's folder.
app_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_dir="$(cd "$app_dir/../.." && pwd)"

step "Checking tools"
command -v flutter >/dev/null 2>&1 || fail "flutter not found. Install Flutter 3.47.5 and add it to PATH."
command -v xcodebuild >/dev/null 2>&1 || fail "xcodebuild not found. Install Xcode and run: sudo xcode-select -s /Applications/Xcode.app"
command -v git >/dev/null 2>&1 || fail "git not found."
echo "flutter:    $(flutter --version 2>/dev/null | head -n 1)"
echo "xcodebuild: $(xcodebuild -version 2>/dev/null | head -n 1)"

step "Updating the checkout ($(git -C "$repo_dir" rev-parse --abbrev-ref HEAD))"
if ! git -C "$repo_dir" pull --ff-only; then
  fail "git pull --ff-only failed. Commit, stash or resolve local changes first."
fi

cd "$app_dir"

step "Fetching packages"
flutter pub get

if [[ $dev -eq 1 ]]; then
  cat <<'EOF'

Note (--dev): the build itself is the same. The environment is chosen by the PC
you pair with: pair against the dev daemon (Tailscale port 8444) to get a host
marked DEV in the app. Push from an Xcode-signed build uses the APNs sandbox.
EOF
fi

if [[ $mode == "ipa" ]]; then
  step "Building the .ipa (release)"
  flutter build ipa --release
  echo
  echo "Done: $app_dir/build/ios/ipa/"
  echo "Install it with Xcode (Window > Devices and Simulators) or upload it with Transporter."
  exit 0
fi

if [[ -z $device ]]; then
  step "Connected devices"
  flutter devices
  # Pick the only physical iOS device, if there is exactly one.
  # (python3 ships with the Xcode command line tools; macOS bash 3.2 has no JSON or mapfile.)
  ios="$(flutter devices --machine 2>/dev/null | python3 -c '
import json, sys
try:
    devices = json.load(sys.stdin)
except ValueError:
    devices = []
for d in devices:
    if str(d.get("targetPlatform", "")).startswith("ios") and not d.get("emulator", False):
        print(d["id"])
' || true)"
  count="$(printf '%s' "$ios" | grep -c . || true)"
  if [[ $count -eq 1 ]]; then
    device="$ios"
    echo "Using the only iOS device: $device"
  else
    echo
    echo "Choose a device and run again, for example:"
    echo "  $0 -d my-iphone"
    echo "(Unlock the iPhone, trust this Mac and turn on Developer Mode if it is missing.)"
    exit 1
  fi
fi

step "Building and installing on $device (release)"
flutter run --release -d "$device"

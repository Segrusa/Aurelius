#!/usr/bin/env bash
# Uninstall Aurelius.app from the Mac. Your data is kept unless you ask for it:
#   --remove-data   also delete ~/Library/Application Support/Aurelius (server
#                   pairing, settings, logs) and the Keychain items "Aurelius/...".
#   --yes           don't ask for confirmation.
# Project folders (~/Documents/Aurelius/...) are yours and are never deleted.
set -euo pipefail

remove_data=0
yes=0
for arg in "$@"; do
    case "$arg" in
        --remove-data) remove_data=1 ;;
        --yes) yes=1 ;;
        *) echo "Unknown option: $arg" >&2; exit 2 ;;
    esac
done

if [[ "$(uname)" != "Darwin" ]]; then
    echo "This script is for macOS." >&2
    exit 1
fi

apps=()
for dir in /Applications "$HOME/Applications"; do
    if [[ -d "$dir/Aurelius.app" ]]; then apps+=("$dir/Aurelius.app"); fi
done
data="$HOME/Library/Application Support/Aurelius"

echo "This removes:"
for app in ${apps[@]+"${apps[@]}"}; do echo "  app            $app"; done
if [[ ${#apps[@]} -eq 0 ]]; then echo "  (no Aurelius.app found)"; fi
if [[ $remove_data -eq 1 ]]; then
    echo "  YOUR DATA      $data"
    echo "  Keychain       API keys 'Aurelius/...'"
else
    echo "Kept: your data in $data and your Keychain items (use --remove-data to delete them too)."
fi
echo "Never touched: your project folders in ~/Documents/Aurelius."
if [[ $yes -eq 0 ]]; then
    read -r -p "Continue? (yes/no) " answer
    if [[ "$answer" != "yes" ]]; then echo "Nothing changed."; exit 0; fi
fi

if pgrep -x aurelius >/dev/null 2>&1; then
    osascript -e 'quit app "Aurelius"' >/dev/null 2>&1 || true
    sleep 1
    pkill -x aurelius 2>/dev/null || true
fi
for app in ${apps[@]+"${apps[@]}"}; do rm -rf "$app"; done
echo "App removed."

if [[ $remove_data -eq 1 ]]; then
    if [[ -d "$data" ]]; then rm -rf "$data"; echo "Data removed."; fi
    # Secrets are generic passwords, service "Aurelius/<ref>", account "aurelius"
    # (crates/platform/src/secrets.rs). Dev items ("Aurelius-Dev/...") stay.
    removed=0
    while IFS= read -r service; do
        if [[ -z "$service" || "$service" == "Aurelius/release-signing-key" ]]; then continue; fi
        if security delete-generic-password -a aurelius -s "$service" >/dev/null 2>&1; then removed=$((removed + 1)); fi
    done < <(security dump-keychain 2>/dev/null | sed -n 's/.*"svce"<blob>="\(Aurelius\/[^"]*\)".*/\1/p' | sort -u)
    echo "Keychain items removed ($removed)."
fi
echo "Aurelius is uninstalled."

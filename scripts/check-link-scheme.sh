#!/usr/bin/env bash
# Check the aurelius:// registration that can be checked without a device
# (AUR-FA-O1): the Linux .desktop entry, the Mac and iOS plists and the Android
# manifest. Needs desktop-file-validate and xmllint; xdg-mime is optional.
# Opening a real link on a phone, a Mac or Windows is not covered (see the report).
set -euo pipefail
cd "$(dirname "$0")/.."
fail=0
bad() { echo "FAIL: $*" >&2; fail=1; }

# Linux: render the heredoc of install-linux.sh and validate it.
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
bin=/usr/bin
sed -n '/^cat > "\$apps\/aurelius.desktop" <<DESKTOP$/,/^DESKTOP$/p' scripts/install-linux.sh | sed '1d;$d' | sed "s|\$bin|$bin|" > "$tmp/aurelius.desktop"
desktop-file-validate "$tmp/aurelius.desktop" || bad "aurelius.desktop is not valid"
grep -q '^MimeType=x-scheme-handler/aurelius;$' "$tmp/aurelius.desktop" || bad "no MimeType for the aurelius scheme"
grep -q '^Exec=.* %u$' "$tmp/aurelius.desktop" || bad "Exec does not take %u"
grep -q 'xdg-mime default aurelius.desktop x-scheme-handler/aurelius' scripts/install-linux.sh || bad "install-linux.sh never sets the default handler"
if command -v xdg-mime >/dev/null 2>&1; then
    # Sandboxed: a throw-away XDG home, so the real defaults stay untouched.
    mkdir -p "$tmp/home/.local/share/applications" "$tmp/home/.config"
    cp "$tmp/aurelius.desktop" "$tmp/home/.local/share/applications/"
    got="$(HOME="$tmp/home" XDG_DATA_HOME="$tmp/home/.local/share" XDG_CONFIG_HOME="$tmp/home/.config" \
        sh -c 'xdg-mime default aurelius.desktop x-scheme-handler/aurelius && xdg-mime query default x-scheme-handler/aurelius')"
    [[ "$got" == "aurelius.desktop" ]] || bad "xdg-mime did not take aurelius.desktop (got '$got')"
fi

# Mac: the packager writes the Info.plist from this declaration.
grep -q 'schemes: \["aurelius"\]' clients/electron/scripts/package.mjs || bad "the Mac bundle does not declare the scheme"

# iOS and Android.
ios=clients/mobile/ios/Runner/Info.plist
xmllint --noout --nonet --nodtdattr "$ios" 2>/dev/null || xmllint --noout "$ios" || bad "iOS Info.plist is not well-formed"
grep -q '<string>aurelius</string>' "$ios" || bad "iOS plist has no aurelius URL scheme"
grep -A1 FlutterDeepLinkingEnabled "$ios" | grep -q '<false/>' || bad "iOS: Flutter deep linking must stay off"
man=clients/mobile/android/app/src/main/AndroidManifest.xml
xmllint --noout "$man" || bad "AndroidManifest is not well-formed"
grep -q 'android:scheme="aurelius"' "$man" || bad "Android manifest has no aurelius scheme"
grep -q 'android.intent.category.BROWSABLE' "$man" || bad "Android intent filter is not BROWSABLE"
grep -q 'flutter_deeplinking_enabled" android:value="false"' "$man" || bad "Android: Flutter deep linking must stay off"

if [[ $fail -eq 0 ]]; then echo "Link scheme packaging: all checks passed."; fi
exit $fail

# Mobile companion (iPhone and Android)

The companion is a native Flutter app (`clients/mobile`) that connects to Aurelius
on your own PC over Tailscale. The PC stays the place where everything runs and
is stored: projects, credentials, memory, runtimes. The phone is one more client
of the same chats, managers and tasks, like a second window of the desktop app.
There is no database sync and no provider login on the phone.

What the phone can do: open chats and manager conversations (start more than one
per manager), send messages and attachments, follow answers live with the commands,
edits and web searches they use, create and view tasks, timeline, memory and usage,
stop or pause work, and answer approvals with the same exact action the desktop shows.
Secrets, updates, backups, folders, connections and shutdown stay desktop-only.

Another **computer** (a second Windows PC, the Mac) connects the same way, but
with the desktop app and full access ("owner"): in Remote access press N, then
**Copy pairing text** (Y); on the other computer open Settings → Servers
(Ctrl Alt S), press N, paste, Enter; back here choose **Allow as owner**.

## Architecture

```text
iPhone / Android (Flutter, BLoC)
  │  HTTPS + WSS over the tailnet (only devices your tailnet allows)
  ▼
Tailscale Serve on the PC          https://my-pc.example-tailnet.ts.net:8443  (dev: 8444)
  │  TLS ends here (tailnet certificate)
  ▼
Remote gateway in the daemon       http://127.0.0.1:47613  (dev: 47614, AURELIUS_REMOTE_PORT overrides)
  │  pairing, sessions, capabilities, method allowlist
  ▼
Same dispatch as the desktop IPC   → same chats, tasks, approvals, policy
  │
  └─► APNs (api.push.apple.com / sandbox)   push for devices without an open connection
```

- The gateway (`crates/remote-gateway`) runs inside the daemon, binds only on
  loopback and is **off after a new install**. Turning it on also runs
  `tailscale serve --bg --yes --https=8443 http://127.0.0.1:47613`; turning it off
  removes that route. No public Funnel route is ever created.
- Endpoints: `POST /v1/pair`, `GET /v1/pair/{id}` (long poll), `POST /v1/session`,
  `GET /v1/ws`, `POST /v1/upload` (25 MB, per-device inbox), `PUT/DELETE /v1/push`,
  `GET /v1/health`. Protocol 1.2; streamed text arrives as `stream_append`.
- Dev and production are separate here too: different ports, different paired
  devices, and the app marks a dev host with **DEV**. A phone paired with one
  environment refuses to talk to the other.

## One-time setup

### 1. Apple Developer account and push key
Needed for installing on the iPhone beyond 7-day free provisioning, and for push.

1. Enrol in the Apple Developer Program with your Apple ID.
2. Certificates, Identifiers & Profiles → **Identifiers** → register the App ID
   `com.segrusa.aurelius` with the **Push Notifications** capability.
3. **Keys** → create a key with **Apple Push Notifications service (APNs)**.
   Download `AuthKey_<KEYID>.p8` (it can be downloaded only once; keep it safe).
   The Key ID is in the file name.
4. Note your **Team ID** (Membership details, 10 characters).

One `.p8` key works for both sandbox and production and for all your apps.

### 2. Xcode signing on the MacBook
1. Install Xcode and Flutter 3.47.5 on your Mac; clone the repo.
2. Xcode → Settings → Accounts → add your Apple ID.
3. Open `clients/mobile/ios/Runner.xcworkspace`, target **Runner** → Signing &
   Capabilities: tick *Automatically manage signing*, choose your **Team** (or put
   `DEVELOPMENT_TEAM = <your Team ID>` in the uncommitted `ios/Flutter/Local.xcconfig`). The
   bundle ID is `com.segrusa.aurelius`; *Push Notifications* comes from
   `ios/Runner/Runner.entitlements`. Packages use Swift Package Manager (no CocoaPods).
4. Connect the iPhone once by cable, trust the Mac, and turn on
   Settings → Privacy & Security → **Developer Mode** on the iPhone.

### 3. Tailscale
- Tailscale on the PC and on the phone, signed in to the same tailnet.
- **MagicDNS** and **HTTPS certificates** must be on (admin console → DNS). They
  are already on for this tailnet; the PC is `my-pc.example-tailnet.ts.net`.
- Optional but recommended: restrict who can reach the PC on port 8443 with a
  grant in the tailnet policy. Tailscale decides which devices can reach the PC;
  Aurelius pairing decides what a paired device may do.

### 4. Turn on Remote access on the desktop
`Ctrl+Alt+R` opens Settings → Remote access (also reachable with `Ctrl+,`, `Tab`).

| Key | Action |
|---|---|
| `O` | Remote access on / off (also starts or stops Tailscale Serve) |
| `N` | Pair a phone: shows a QR code and an 8-character code, valid 5 minutes, single use |
| `Enter` | Allow the phone that is waiting |
| `X` | Refuse it |
| `Delete` | Remove (revoke) the selected device |
| `A` | Choose the APNs key file (`AuthKey_<KEYID>.p8`) |
| `T` | Send a test notification to the selected device |

The screen also shows whether Tailscale is serving the route and the address
phones will use.

### 5. Pair the phone
1. Desktop: `N`. Phone: open the app → **Scan QR code** (or *Enter address and
   code instead*: `my-pc.example-tailnet.ts.net:8443` and the code).
2. The desktop shows the device name and platform. Check it and press `Enter` to
   allow. Nothing is stored for a refused or expired request.
3. The phone receives its device secret once and keeps it in the Keychain
   (Android: encrypted shared preferences). The PC keeps only its SHA-256.

The app can hold several PCs; each is its own host with its own IDs and approvals.

### 6. Push notifications (iPhone)
1. Desktop, Remote access: `A` → pick `AuthKey_<KEYID>.p8`. The Key ID is read from
   the file name. Type the Team ID. The bundle ID is `com.segrusa.aurelius`.
   The key goes to Windows Credential Manager, not to the database.
2. On the phone, allow notifications when asked. The app registers its APNs token
   with the PC (`PUT /v1/push`).
3. Select the device and press `T` for a test notification.

The PC sends push directly to Apple (token auth, ES256), only to devices that
have no open connection at that moment. The payload is discreet: a title, the
task title and routing IDs, never message content. Tapping it opens the item.
Android push (FCM) is not built: Android gets updates only while the app is open.

## Building for the iPhone (MacBook)

```bash
chmod +x clients/mobile/scripts/ios-build.sh      # once
clients/mobile/scripts/ios-build.sh               # lists devices; uses the only iPhone found
clients/mobile/scripts/ios-build.sh -d my-iphone # release build installed on that device (or set IOS_DEVICE)
clients/mobile/scripts/ios-build.sh --ipa         # .ipa in clients/mobile/build/ios/ipa/
```

The script checks `flutter` and `xcodebuild`, runs `git pull --ff-only` and
`flutter pub get`, then builds. `--dev` builds the same app and only prints a
reminder: the environment is chosen by the PC you pair with, not by the build.

## Testing on Windows with the Android emulator (dev)

This needs no Mac, no Tailscale and no Apple account.

1. Start the dev environment: `scripts\dev.ps1`. Turn on Remote access in the dev
   app (`Ctrl+Alt+R`, `O`). The dev gateway listens on `127.0.0.1:47614`.
2. Start an Android emulator (Android Studio → Device Manager) and run the app:
   ```powershell
   cd clients\mobile
   flutter run -d emulator-5554
   ```
3. In the app choose **Enter address and code instead**. Address:
   `http://10.0.2.2:47614` (type the `http://` yourself; without a scheme the
   app assumes `https://`). `10.0.2.2` is the emulator's address for the PC's
   loopback. Press `N` in the desktop app and enter the code, then `Enter` to allow.

Cleartext HTTP is allowed only to `10.0.2.2`, `127.0.0.1` and `localhost`
(Android network security config); everything else must be HTTPS.

Automated tests:
```powershell
cd clients\mobile; flutter analyze; flutter test     # 33 tests
cargo test -p aurelius-daemon --test remote          # gateway end to end
```
The Dart protocol tests read fixtures generated from the Rust types. After a
protocol change, regenerate them:
`$env:AURELIUS_WRITE_FIXTURES=1; cargo test -p aurelius-protocol --test mobile_fixtures`.

## Security model

- **Two gates.** Tailscale decides which devices can reach the PC at all; the
  gateway is bound to loopback and is never exposed on a LAN or the internet.
  Aurelius pairing decides what a reachable device may do.
- **Pairing.** Started on the desktop only. One code at a time, 5 minutes, single
  use, dead after five wrong guesses, rate-limited per minute. Every request needs
  an explicit Allow on the desktop.
- **Credentials.** Device secret (Keychain on the phone) → `POST /v1/session` with
  Basic auth → 12-hour bearer token for the WebSocket and uploads. SQLite stores
  only SHA-256 verifiers (migration 0011: `remote_clients`, `remote_sessions`,
  `push_registrations`). Tokens never go in URLs or logs.
- **Capabilities.** `read`, `chat`, `task_control`, `approve`, always inside the
  existing project and tool policy. Server management (secrets, updates,
  backups, folders, connections, remote access) needs `owner`, which only a
  computer can ask for and only an explicit "Allow as owner" grants; a phone
  never gets it. Shutdown and Claude Code's approval bridge stay on the PC itself.
  Installing a server update needs `approve`, so the phone can do it (Settings →
  Updates): only a release signed with the Aurelius key is ever installed.
- **Approvals.** The daemon checks policy, digest, expiry and single use. Desktop
  and phone approving at the same time run the action once; the other side sees
  that it was already handled. A phone never gets more rights than the desktop.
  Claude's questions (AskUserQuestion) are approvals too: the phone shows the
  options and sends `answer_question`, or skips (a reject). Merging a task's
  Git branch (`merge_task`) needs `approve`, like an approval: it changes the
  checkout on the PC.
- **Audit.** Commands from a phone carry its client ID in the event metadata.
- **Revoke.** Removing a device on the desktop invalidates its secret and tokens
  and closes an open WebSocket at once (close code 4401).
- **Reconnects.** A lost connection never stops work on the PC. On reconnect the
  app resends unanswered commands with the same command ID (applied once).
  Approvals and other decisions are never replayed later; they fail at once while
  offline.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| App says the PC is offline | The PC is asleep or off, the daemon isn't running, Remote access is off, or the phone's Tailscale is disconnected. The app shows when data was last updated and never fakes progress. Check `GET /v1/health` from another tailnet device: `curl https://my-pc.example-tailnet.ts.net:8443/v1/health`. |
| "Paired device was removed" / revoked | The device was removed on the desktop (or its secret no longer matches). Remove the host in the app and pair again. The app stops retrying instead of looping. |
| "Pair again" after an environment change | The PC at that address now runs the other environment (dev vs production). Pair with the right one. |
| Certificate refused / TLS error | Use the full MagicDNS name (`my-pc.example-tailnet.ts.net`), not the IP or short name: the certificate is issued for that name. HTTPS certificates must be on in the tailnet. Run `tailscale serve status` on the PC to check the 8443 route; toggling Remote access off and on recreates it. |
| Pairing code rejected | Codes expire after 5 minutes, work once, and die after five wrong tries. Press `N` again for a new one. |
| Push doesn't arrive | Push is skipped while the app has an open connection (you get live updates instead). Check the key, Key ID and Team ID on the desktop and press `T`. **Sandbox vs production follows the provisioning profile:** builds signed from Xcode (`ios-build.sh`) use the APNs sandbox, TestFlight/App Store builds use production. The app reports which one with its token; after switching build types, open the app once so it re-registers. `BadDeviceToken` or `DeviceTokenNotForTopic` means a mismatch (environment or bundle ID); the PC then drops that token. |
| Android shows no notifications | Expected: FCM is not built yet. |
| Emulator can't connect | Use `http://10.0.2.2:47614` (dev), not `localhost`; make sure the dev app has Remote access on. |

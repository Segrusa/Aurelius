# The mobile app as a Linux build against a test daemon

For the phase 3 smoke run and for screenshots: the same Flutter app, built for
Linux, paired with a test daemon of its own. Nothing here touches `~/.claude`,
the installed daemon or its data (`~/.local/share/Aurelius`). The app keeps its
own pairings in `~/.local/share/eu.aurelius.aurelius_mobile` and the keyring.

Needs: Flutter (`~/development/flutter/bin/flutter`), the Linux toolchain
(clang, cmake, ninja, GTK 3, libsecret), a graphical session, Python 3.

## 1. The test daemon

From the repository root:

```sh
cargo build -p aurelius-daemon
crates/daemon/tests/fixtures/test-daemon.sh /tmp/mu03-app 47792 &
clients/mobile/integration_test/daemon/setup.sh /tmp/mu03-app 47792
```

`setup.sh` makes the board members **alex** (`test-password-1`) and **robin**
(`test-password-2`), the stand-in claude as the Claude Code connection, the
employee Ada, the project Demo, turns remote access on, starts a ticket of
robin that waits for robin's Claude login and a ticket of alex on alex's
(simulated) limit, and prints a pairing code. For 10 minutes it allows every
device that pairs (`allow.py`). Start from an empty folder each time.

Remote access on a dev daemon also adds `tailscale serve` on port 8444 for the
tailnet; the app uses `http://127.0.0.1:<port>` and does not need it.

## 2. The app

From `clients/mobile`, in the graphical session (from a shell without one, set
`XDG_RUNTIME_DIR=/run/user/$(id -u) WAYLAND_DISPLAY=wayland-1
DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$(id -u)/bus`):

- **By hand (screenshots):** `flutter run -d linux`, then *Enter address and
  code instead*: address `http://127.0.0.1:47792`, the code from `setup.sh`,
  *Pair*. Sign in as alex. Connect on More → Account → Claude login accepts
  a whole code `good…#FAKESTATE`, as Claude shows it with its `#state` (another
  code shows the daemon's refusal; one without `#` is refused too).
- **Automatic (the smoke run):** the `flutter test … -d linux` command that
  `setup.sh` prints. It pairs, signs in, opens the session list, connects with
  a wrong and then a good code, checks the `/usage` bars, runs robin's ticket
  on alex's login (Run on my login) and finds the Paused line in Discussion.
  `AUR_RESET=true` forgets the app's earlier pairings first; without it a
  kept session opens Home directly.

The stand-in claude writes every call to `/tmp/mu03-app/bin/runs.log` with the
login folder it ran in: after Run on my login, robin's ticket runs in alex's
folder (`data/logins/<alex's id>`).

## 3. Stop

```sh
python3 -I clients/mobile/integration_test/daemon/aur.py /tmp/mu03-app/data/ipc/daemon.sock alex test-password-1 remote.disable --write
```

Then stop the daemon (Ctrl-C, or `kill` its process). `remote.disable` also
removes the `tailscale serve` on 8444.

## Tools

- `daemon/aur.py <socket> <user> <password> <method> [json params] [--write]`:
  one call over the daemon's IPC socket after a sign-in (`- -` for none: the
  onboarding), then signs out again. `aurelius-ctl` cannot sign in (protocol 5).
- `daemon/allow.py <socket> <user> <password> [seconds]`: allows waiting
  pairings.

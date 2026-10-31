# Access over Tailscale (server setup)

Board members reach the central Aurelius server over a Tailscale tailnet: the
server is never public and is not on the LAN either (MU3-07). This page
covers installing Tailscale, an ACL that admits only your own people,
MagicDNS, connecting a client, and checking that nothing outside the tailnet
gets in. Pairing a phone or a second computer is in [mobile.md](mobile.md).

## How traffic gets in

```text
board member's device (in the tailnet, allowed by the ACL)
  │  HTTPS + WSS to https://<server>.<tailnet>.ts.net:8443   (dev: 8444)
  ▼
tailscaled on the server: Tailscale Serve
  │  listens on the server's tailnet addresses only (100.x.y.z, fd7a:115c:a1e0::…)
  │  ends TLS with the tailnet certificate
  ▼
remote gateway in aurelius-daemon     http://127.0.0.1:47613   (dev: 47614)
  │  pairing (device) → board member login (session) → same dispatch as the desktop
  ▼
desktop on the server itself: local IPC (Unix socket / named pipe), no TCP port
```

- The gateway binds on **loopback only** (`crates/remote-gateway/src/bind.rs`).
  It has no listener on a LAN or tailnet address of its own. The tailnet port is
  opened by tailscaled itself, through the route that **Remote access → on**
  creates: `tailscale serve --bg --yes --https=8443 http://127.0.0.1:47613`.
  Turning remote access off removes the route. Aurelius never creates a Funnel
  route (public internet).
- So a device outside the tailnet finds nothing: the LAN address has no
  Aurelius port, and the tailnet addresses are not routable outside the tailnet.
- Another address is possible only explicitly, with `AURELIUS_REMOTE_BIND=<ip>`
  in the daemon's environment (see [below](#listening-on-another-address)). The
  daemon then logs a warning at every start.

## 1. Install Tailscale on the server

Linux:

```sh
curl -fsSL https://tailscale.com/install.sh | sh
sudo systemctl enable --now tailscaled
sudo tailscale up --advertise-tags=tag:aurelius-server
sudo tailscale set --operator=$USER   # lets the daemon (your user) run `tailscale serve`
```

Windows / macOS: install the app from <https://tailscale.com/download>, sign in,
and tag the machine in the admin console (Machines → … → Edit ACL tags).

The tag must exist in `tagOwners` (step 3) before `--advertise-tags` works.
Check with `tailscale status` (the server is listed and online) and
`tailscale ip` (one 100.x and one fd7a: address).

## 2. MagicDNS and HTTPS certificates

In the admin console under **DNS**:

1. Turn on **MagicDNS**. The server gets the name `<machine>.<tailnet>.ts.net`,
   which resolves only inside the tailnet.
2. Turn on **HTTPS Certificates**. Tailscale Serve then gets a real certificate
   for that name, so the apps connect with HTTPS without warnings.

Aurelius's Remote access page tells you which of the two is still missing.

## 3. ACL: only your own board members

Admin console → **Access controls**. Replace the default "everyone reaches
everything" rule. Example for a company with board members Alex and Robin:

```jsonc
{
  "groups": {
    "group:aurelius": ["alex@example.com", "robin@example.com"],
    // Only whoever maintains the server; not every board member.
    "group:aurelius-admin": ["alex@example.com"]
  },
  "tagOwners": {
    "tag:aurelius-server": ["alex@example.com"]
  },
  "acls": [
    // Board members' devices reach Aurelius (8443, dev 8444). No ssh: Claude login uses Connect in the app.
    { "action": "accept", "src": ["group:aurelius"], "dst": ["tag:aurelius-server:8443,8444"] },
    // ssh only for server admins.
    { "action": "accept", "src": ["group:aurelius-admin"], "dst": ["tag:aurelius-server:22"] }
    // No other rule targets tag:aurelius-server: every other device is dropped.
  ],
  "tests": [
    { "src": "alex@example.com",  "accept": ["tag:aurelius-server:8443", "tag:aurelius-server:22"] },
    { "src": "robin@example.com", "accept": ["tag:aurelius-server:8443"], "deny": ["tag:aurelius-server:47613", "tag:aurelius-server:22"] },
    { "src": "test@example.com",  "deny":   ["tag:aurelius-server:8443", "tag:aurelius-server:22"] }
  ]
}
```

- Add a new board member's Tailscale login to `group:aurelius` first, then
  create the board member in Aurelius.
- If others share the tailnet (family, other projects), they are not in the
  group and do not reach the server, even though they are in the tailnet.
- ssh (22) is open only to `group:aurelius-admin`, never to all board members:
  a shell on the server gives access to the database and to the Claude login
  folders of every board member. Board members don't need it: they sign in to
  Claude with the Connect flow in the app. Leave the admin rule out if nobody
  needs ssh over the tailnet.
- The `tests` block is checked when you save: the policy is refused if a test
  fails. `test@example.com` stands for someone in the tailnet but not in the group.
- The ACL is the outer wall; pairing and the board member login (MU3-02/06) still
  apply to everyone who gets through.

## 4. Turn on remote access

On the server, in the desktop app: Settings → **Remote access** (`Ctrl+Alt+R`) → `O`. Check:

```sh
tailscale serve status
# https://<machine>.<tailnet>.ts.net:8443 (tailnet only)
# |-- / proxy http://127.0.0.1:47613
tailscale funnel status   # the same route, "tailnet only": nothing public
```

## 5. Connect a client and sign in

1. Install Tailscale on the device (App Store, Play Store, or the desktop
   installer) and sign in with an account from `group:aurelius`.
2. Check the server is reachable: in a browser on the device open
   `https://<machine>.<tailnet>.ts.net:8443/v1/health` (it answers with JSON).
3. Pair the device: on the server, Remote access → pair, then scan the QR code
   with the phone, or paste the pairing text in another computer's Settings →
   Servers (details in [mobile.md](mobile.md)). Confirm on the server.
4. Sign in with your board member name and password. From then on the device
   uses its own session; signing out or revoking the session ends it at once.

## 6. Check: only localhost and the tailnet (MU3-07 AC2)

On the server, list what listens on the Aurelius ports:

```sh
ss -ltnp | grep -E ':(47613|47614|8443|8444)\b'      # Linux
# lsof -nP -iTCP -sTCP:LISTEN | grep -E '4761[34]|844[34]'   # macOS
# netstat -ano | findstr "47613 47614 8443 8444"             # Windows
```

Expected (production):

```text
127.0.0.1:47613                    aurelius-daemon   ← loopback only
100.x.y.z%tailscale0:8443          (tailscaled)      ← tailnet only
[fd7a:115c:a1e0::…]%tailscale0:8443 (tailscaled)
```

Nothing on `0.0.0.0`, `[::]` or the LAN address. If the daemon line shows
anything other than `127.0.0.1`, `AURELIUS_REMOTE_BIND` is set; the daemon log
has a `remote gateway listens on …` warning.

## 7. Check: a device outside the tailnet does not get in (MU3-07 AC3)

Use a device that is **not** in the tailnet, or one whose Tailscale is turned
off: a laptop on the same LAN, and a phone on mobile data. Replace
`192.168.1.20` with the server's LAN address and `100.x.y.z` with its
`tailscale ip -4`.

| From outside the tailnet | Command | Expected |
|---|---|---|
| MagicDNS name | `curl -m 5 https://<machine>.<tailnet>.ts.net:8443/v1/health` | `Could not resolve host` (the name exists only in the tailnet) |
| Tailnet address | `curl -m 5 -k https://100.x.y.z:8443/v1/health` | timeout (not routable outside the tailnet) |
| LAN address, gateway port | `curl -m 5 http://192.168.1.20:47613/v1/health` | `Connection refused` |
| LAN address, Serve port | `curl -m 5 -k https://192.168.1.20:8443/v1/health` | `Connection refused` |

Then the opposite, to know the test means something: turn Tailscale on on the
same device (an account in `group:aurelius`) and run the first command again.
It answers with JSON. With an account outside the group (`test@example.com`)
it times out: the ACL drops it.

On the phone without a terminal: turn Tailscale off and open the
`https://…ts.net:8443/v1/health` URL in the browser; the page does not load.
The Aurelius app shows the server as unreachable.

## Listening on another address

Only when you know why (for example a test without Tailscale on a closed
network):

```sh
AURELIUS_REMOTE_BIND=0.0.0.0 aurelius-daemon   # or a specific address
```

- Not an IP address → remote access does not start and says why.
- A loopback address → as the default.
- One of this server's own tailnet addresses → logged: the gateway answers plain
  HTTP to tailnet devices (WireGuard-encrypted, but without Serve's certificate).
- Anything else (`0.0.0.0`, `::`, a LAN address, a 100.x address that is not this
  server's tailnet address) → a **warning** at every start: devices outside the
  tailnet can reach the gateway, and only pairing and login stand in the way.

Unset the variable and restart the daemon to go back to loopback only.

## Test gateway

A test daemon next to the real one needs its own data and port:
`AURELIUS_DATA_DIR=<temp dir> AURELIUS_REMOTE_PORT=0`. Only the default data
folder of this user touches Tailscale: with any other data folder (or one under
the temp folder) Aurelius makes no Tailscale calls and listens on loopback only.
`AURELIUS_TAILSCALE=1` opts such a folder in; `AURELIUS_NO_TAILSCALE=1` turns
Tailscale off for every folder, the default one too.
`cargo test -p aurelius-remote-gateway` covers this
(`tests/bind.rs`, `src/bind.rs`): default loopback, refused on the LAN address,
a bad setting refused, `0.0.0.0` only explicitly.

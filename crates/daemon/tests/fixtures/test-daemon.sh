#!/bin/sh
# Start a test daemon for the phase 3 E2E (impact analysis §7a): its own data
# folder, its own IPC socket and remote port, an environment without any Claude
# login or key, and the stand-in `claude` (fake-claude) next to it. Every board
# member gets their own login folder there once they connect (the stand-in
# accepts a code "good…#<state>"). Never touches ~/.claude, ~/.claude.json,
# ~/.config/claude, the installed daemon or the system's Tailscale
# (AURELIUS_NO_TAILSCALE: remote access stays on loopback, pairing works there).
#
#   crates/daemon/tests/fixtures/test-daemon.sh <folder> [remote port] [daemon binary]
#
# <folder> (made when missing) gets:
#   home/           HOME of the daemon (so nothing of the real one is read)
#   data/           AURELIUS_DATA_DIR: database, logins/<user_id>/, logs/daemon.log
#   bin/claude      the stand-in; set it as the Claude Code connection's path
#   bin/runs.log    one line per call of it: login folder, leaked keys, the call
#   bin/quota-users, bin/expired-users   user ids whose runs hit the limit / a 401
# The daemon's IPC socket is data/ipc/daemon.sock; the remote gateway listens
# on 127.0.0.1:<remote port> (default 47790). Stop it with Ctrl-C or kill <pid>.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
dir=${1:?usage: test-daemon.sh <folder> [remote port] [daemon binary]}
port=${2:-47790}
daemon=${3:-$here/../../../../target/debug/aurelius-daemon}
mkdir -p "$dir/home" "$dir/bin"
dir=$(cd "$dir" && pwd)
mkdir -m 700 -p "$dir/data"
cp "$here/fake-claude" "$dir/bin/claude"
chmod 755 "$dir/bin/claude"
[ -f "$dir/home/claude.json" ] || echo '{}' > "$dir/home/claude.json"
echo "data:    $dir/data"
echo "socket:  $dir/data/ipc/daemon.sock"
echo "remote:  127.0.0.1:$port"
echo "claude:  $dir/bin/claude (add it as the Claude Code connection)"
exec env -i \
    PATH=/usr/bin:/bin \
    HOME="$dir/home" \
    LANG="${LANG:-C.UTF-8}" \
    AURELIUS_DATA_DIR="$dir/data" \
    AURELIUS_REMOTE_PORT="$port" \
    AURELIUS_CLAUDE_JSON="$dir/home/claude.json" \
    AURELIUS_NO_TRAY=1 \
    AURELIUS_NO_AUTO_BACKUP=1 \
    AURELIUS_NO_TAILSCALE=1 \
    AURELIUS_LOG="${AURELIUS_LOG:-info}" \
    "$daemon"

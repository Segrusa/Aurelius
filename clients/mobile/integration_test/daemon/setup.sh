#!/bin/sh
# Fill a fresh test daemon (crates/daemon/tests/fixtures/test-daemon.sh) for the
# phase 3 smoke run of the mobile app, then print the command that runs it as
# the Linux build. See integration_test/README.md.
#
#   integration_test/daemon/setup.sh <test daemon folder> [remote port]
#
# Makes: board members alex and robin, the stand-in claude as the Claude Code
# connection, the employee Ada, the project Demo, remote access on, a ticket of
# robin that waits for robin's Claude login and a ticket of alex that hits
# alex's (simulated) usage limit. Then a pairing code, and allow.py in the
# background allows the app's pairing for 10 minutes.
set -eu
dir=$(cd "${1:?usage: setup.sh <test daemon folder> [remote port]}" && pwd)
port=${2:-47790}
here=$(cd "$(dirname "$0")" && pwd)
sock="$dir/data/ipc/daemon.sock"
alex_pw=test-password-1
robin_pw=test-password-2
aur() { python3 -I "$here/aur.py" "$sock" "$@"; }
alex() { aur alex "$alex_pw" "$@"; }
# The id or field a call answers: field <name>.
field() { python3 -c 'import json,sys; v=json.load(sys.stdin); v=v.get("ok") or sys.exit("error: "+json.dumps(v.get("error"))); v=v["value"]; v=v.get("result",{}).get("value",v); print(v[sys.argv[1]])' "$1"; }

alex_id=$(aur - - user.create '{"username":"alex","password":"'$alex_pw'"}' --write | field user_id)
robin_id=$(alex user.create '{"username":"robin","password":"'$robin_pw'"}' --write | field user_id)
conn=$(alex connection.add '{"provider":"claude_code","display_name":"Stand-in Claude","runtime_path":"'"$dir"'/bin/claude"}' --write | field connection_id)
for purpose in employee memory; do
    alex connection.set_defaults '{"scope":{"kind":"global"},"purpose":"'$purpose'","settings":{"connection":{"mode":"value","value":"'$conn'"},"model":{"mode":"inherit"},"persona":{"mode":"inherit"},"effort":{"mode":"inherit"}}}' --write >/dev/null
done
ada=$(alex employee.create '{"name":"Ada","role":"Developer"}' --write | field employee_id)
mkdir -p "$dir/work/demo"
project=$(alex project.create '{"name":"Demo","path":"'"$dir"'/work/demo"}' --write | field project_id)
alex remote.enable --write >/dev/null
waits=$(aur robin "$robin_pw" ticket.create '{"project_id":"'$project'","title":"Update the changelog","owner_employee_id":"'$ada'","start":true}' --write | field ticket_id)
# alex's runs hit the limit (the stand-in reads bin/quota-users).
echo "$alex_id" > "$dir/bin/quota-users"
limited=$(alex ticket.create '{"project_id":"'$project'","title":"Check the export","owner_employee_id":"'$ada'","start":true}' --write | field ticket_id)
code=$(alex remote.start_pairing --write | field code)
nohup python3 -I "$here/allow.py" "$sock" alex "$alex_pw" 600 > "$dir/allow.log" 2>&1 < /dev/null &

cat <<EOF
alex   $alex_id (password $alex_pw)
robin  $robin_id (password $robin_pw)
ticket waiting for robin's login: $waits
ticket on alex's limit:           $limited
pairing code (5 minutes):         $code

Run the app (from clients/mobile, in a graphical session):
  flutter test integration_test/smoke_test_daemon_test.dart -d linux \\
    --dart-define=AUR_URL=http://127.0.0.1:$port --dart-define=AUR_CODE=$code \\
    --dart-define=AUR_PASSWORD=$alex_pw --dart-define=AUR_TICKET=$waits \\
    --dart-define=AUR_LIMIT_TICKET=$limited --dart-define=AUR_RESET=true
Or by hand: flutter run -d linux, "Enter address and code instead",
  address http://127.0.0.1:$port and code $code; sign in as alex.
EOF

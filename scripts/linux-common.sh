# Shared by install-linux.sh, update-linux.sh, update-server.sh and update-desktop.sh.
# Source it after `bin` (the install folder) is set; it defines functions only.
# shellcheck shell=bash

# Brings this checkout's main up to date with origin/main when that only moves it
# forward; commits that are only on this computer stay. Refuses another branch
# or local changes, because the build would then not be main.
fetch_main() {
    local branch
    branch="$(git rev-parse --abbrev-ref HEAD)"
    if [[ "$branch" != "main" ]]; then
        echo "This checkout is on '$branch', not main. Switch first: git switch main" >&2
        exit 1
    fi
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "This checkout has local changes. Commit or stash them first (git status)." >&2
        exit 1
    fi
    echo "Fetching origin/main ..."
    if git fetch -q origin main 2>/dev/null; then
        if git merge-base --is-ancestor HEAD FETCH_HEAD; then
            git merge -q --ff-only FETCH_HEAD
        elif ! git merge-base --is-ancestor FETCH_HEAD HEAD; then
            echo "Note: main and origin/main have both moved on; building the local main as it is."
        fi
    else
        echo "Note: origin could not be reached; building the local main as it is."
    fi
}

# The servers of this install only: their program is $bin/aurelius-daemon
# (shown as "... (deleted)" once it has been replaced), not another prefix's.
daemon_pids() {
    local pid exe
    for pid in $(pgrep -u "$(id -u)" -x aurelius-daemon 2>/dev/null || true); do
        exe="$(readlink "/proc/$pid/exe" 2>/dev/null || true)"
        [[ "$exe" == "$bin/aurelius-daemon" || "$exe" == "$bin/aurelius-daemon (deleted)" ]] && echo "$pid"
    done
    return 0
}

# Stops this install's server and starts the program now in $bin.
# `aurelius-ctl shutdown` goes over the internal channel and needs no signed-in
# board member (`restart` does); whatever still runs after 10 s is stopped.
restart_daemon() {
    "$bin/aurelius-ctl" shutdown >/dev/null 2>&1 || true
    for _ in $(seq 1 50); do
        [[ -z "$(daemon_pids)" ]] && break
        sleep 0.2
    done
    local left
    left="$(daemon_pids)"
    # shellcheck disable=SC2086 # one pid per word
    if [[ -n "$left" ]]; then
        kill -TERM $left 2>/dev/null || true
        sleep 2
        left="$(daemon_pids)"
        if [[ -n "$left" ]]; then kill -KILL $left 2>/dev/null || true; fi
    fi
    setsid "$bin/aurelius-daemon" </dev/null >/dev/null 2>&1 &
}

# The app of this install: started as app/aurelius or through the aurelius link.
app_pattern() { echo "^$bin/(app/)?aurelius( |$)"; }

app_running() { pgrep -u "$(id -u)" -f "$(app_pattern)" >/dev/null 2>&1; }

# Closes the app. Its folder must not be swapped while it runs: Chromium starts
# helper processes from its own program and would mix two versions.
stop_app() {
    pkill -TERM -u "$(id -u)" -f "$(app_pattern)" || true
    for _ in $(seq 1 50); do
        app_running || break
        sleep 0.2
    done
    pkill -KILL -u "$(id -u)" -f "$(app_pattern)" 2>/dev/null || true
}

start_app() { setsid nohup "$bin/aurelius" </dev/null >/dev/null 2>&1 & }

# Windows programs that the in-app updater of 0.9.5 and older put in a Linux
# install folder: they never ran here.
remove_windows_leftovers() {
    local name
    for name in aurelius.exe aurelius-daemon.exe aurelius-ctl.exe aurelius-bridge.exe; do
        rm -f "$bin/$name" "$bin/$name.old"
    done
}

# The workspace version from the root Cargo.toml.
workspace_version() { sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1; }

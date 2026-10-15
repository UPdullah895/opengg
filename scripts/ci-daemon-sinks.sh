#!/usr/bin/env bash
# Start the real openggd against a real (headless) PipeWire and check that the
# five OpenGG_* virtual sinks appear.
#
#   scripts/ci-daemon-sinks.sh <normal|late-pulse> <path-to-openggd>
#
#   normal      pipewire, wireplumber and pipewire-pulse are up before the daemon.
#   late-pulse  the daemon starts FIRST and pipewire-pulse only 5 s later — what
#               happens on a fresh install, where the daemon is D-Bus activated
#               by the app before the audio session has finished coming up. The
#               sinks must still appear (within 30 s) without anyone restarting
#               the daemon.
#
# Needs on PATH: dbus-run-session pipewire wireplumber pipewire-pulse pactl.
# Everything runs on a private session bus, runtime dir and $HOME, so it is
# safe on a dev machine too (it never touches the real audio session).
# The daemon gets an EMPTY $HOME, which doubles as the fresh-account check:
# it must create ~/.config/opengg etc. itself.
set -uo pipefail

scenario="${1:-}"; daemon="${2:-}"
case "$scenario" in normal|late-pulse) ;; *) echo "usage: $0 <normal|late-pulse> <openggd>" >&2; exit 2 ;; esac
[ -x "$daemon" ] || { echo "not executable: $daemon" >&2; exit 2; }
daemon="$(readlink -f "$daemon")"

work="$(mktemp -d)"
mkdir -m700 "$work/run" "$work/home"
export XDG_RUNTIME_DIR="$work/run" HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" \
       XDG_DATA_HOME="$work/home/.local/share" XDG_STATE_HOME="$work/home/.local/state" \
       XDG_CACHE_HOME="$work/home/.cache" WORK="$work" SCENARIO="$scenario" DAEMON="$daemon"

# shellcheck disable=SC2016  # expanded by the inner shell on purpose
exec dbus-run-session -- bash -c '
set -u
cleanup() { kill $(jobs -p) 2>/dev/null; wait 2>/dev/null; }
trap cleanup EXIT

wait_for() {  # wait_for <seconds> <description> <command...>
    local n=$1 what=$2; shift 2
    for _ in $(seq "$n"); do "$@" >/dev/null 2>&1 && return 0; sleep 1; done
    echo "TIMEOUT after ${n}s waiting for $what" >&2
    return 1
}

pipewire >"$WORK/pipewire.log" 2>&1 &
wait_for 20 "pipewire socket" test -S "$XDG_RUNTIME_DIR/pipewire-0" || { cat "$WORK/pipewire.log"; exit 1; }
wireplumber >"$WORK/wireplumber.log" 2>&1 &

start_pulse() {
    pipewire-pulse >"$WORK/pipewire-pulse.log" 2>&1 &
    wait_for 20 "pipewire-pulse (pactl info)" pactl info || { cat "$WORK/pipewire-pulse.log"; return 1; }
}

if [ "$SCENARIO" = normal ]; then
    start_pulse || exit 1
fi

RUST_LOG=info "$DAEMON" >"$WORK/openggd.log" 2>&1 &
daemon_pid=$!

if [ "$SCENARIO" = late-pulse ]; then
    sleep 5
    kill -0 "$daemon_pid" 2>/dev/null || echo "note: daemon already exited during the 5 s head start" >&2
    start_pulse || exit 1
fi

have_all_sinks() {
    local sinks; sinks="$(pactl list short sinks)" || return 1
    for ch in Game Chat Media Aux Mic; do grep -q "OpenGG_$ch" <<<"$sinks" || return 1; done
}

rc=0
if wait_for 30 "the five OpenGG_* sinks" have_all_sinks; then
    echo "ok  all five OpenGG_* sinks exist ($SCENARIO)"
else
    rc=1
    echo "--- pactl list short sinks"; pactl list short sinks
fi
kill -0 "$daemon_pid" 2>/dev/null || { echo "daemon exited early (status: $(wait "$daemon_pid"; echo $?))" >&2; rc=1; }

if [ "$rc" != 0 ]; then
    for f in openggd pipewire-pulse wireplumber pipewire; do
        [ -f "$WORK/$f.log" ] && { echo "--- $f.log"; tail -n 60 "$WORK/$f.log"; }
    done
fi
exit "$rc"
'

#!/usr/bin/env bash
# Lint packaging/openggd.service with systemd's own parser.
#
#   scripts/verify-unit.sh [unit-file]     (default: packaging/openggd.service)
#
# `systemd-analyze verify` refuses a unit whose ExecStart binary does not
# exist, and the unit points at /usr/bin/openggd, which only a package
# installs. So the unit is verified from a temp copy whose ExecStart is
# redirected to a stub; everything else (section/key names, Type=dbus needing
# BusName=, dependency syntax) is checked as shipped.
#
# Also rejects the filesystem-sandboxing keys that once made the daemon die
# with 226/NAMESPACE on every fresh account (systemd refuses a unit whose
# ReadWritePaths= do not exist yet): see the comment in the unit itself.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
unit="${1:-$ROOT/packaging/openggd.service}"
[ -f "$unit" ] || { echo "no such unit: $unit" >&2; exit 2; }

rc=0
if grep -nE '^(ReadWritePaths|ReadOnlyPaths|InaccessiblePaths|ProtectHome|ProtectSystem|PrivateTmp|PrivateDevices|BindPaths)=' "$unit"; then
    echo "::error::$unit sets filesystem sandboxing — that fails on fresh accounts (226/NAMESPACE) and cannot name a localized/user-chosen clips folder" >&2
    rc=1
fi
grep -qE '^ExecStart=/usr/bin/openggd' "$unit" \
    || { echo "::error::$unit: ExecStart must be /usr/bin/openggd (where every package installs it)" >&2; rc=1; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
printf '#!/bin/sh\nexit 0\n' > "$tmp/openggd"
chmod +x "$tmp/openggd"
sed "s|^ExecStart=.*|ExecStart=$tmp/openggd|" "$unit" > "$tmp/$(basename "$unit")"

if command -v systemd-analyze >/dev/null 2>&1; then
    # --user needs a runtime dir to start its throwaway manager in; a CI
    # container or a root shell has none.
    if [ -z "${XDG_RUNTIME_DIR:-}" ] || [ ! -d "$XDG_RUNTIME_DIR" ]; then
        mkdir -m700 "$tmp/run"
        export XDG_RUNTIME_DIR="$tmp/run"
    fi
    systemd-analyze --user verify "$tmp/$(basename "$unit")" || rc=1
else
    echo "systemd-analyze not found — skipping the parser check" >&2
fi
[ "$rc" = 0 ] && echo "ok  $unit"
exit "$rc"

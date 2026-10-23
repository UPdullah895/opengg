#!/usr/bin/env bash
# OpenGG — First-run setup
# Called via polkit for privileged operations, or directly with sudo.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "╔══════════════════════════════════════╗"
echo "║     OpenGG First-Run Setup           ║"
echo "╚══════════════════════════════════════╝"
echo ""

USER_NAME="${SUDO_USER:-$USER}"
USER_HOME=$(getent passwd "$USER_NAME" | cut -d: -f6)

# ── 1. Group for global hotkeys ─────────────────────────────────
# Only `input` is needed: the daemon reads /dev/input/event* for system-wide
# hotkeys. No udev rules are installed — OpenGG never opens hidraw/usbfs
# itself (ratbagd, headsetcontrol and OpenRGB ship their own rules), and the
# old vendor-wide MODE=0666 rules made every gaming peripheral writable by
# any local process. `audio`, `video` and `i2c` are not needed by OpenGG.
echo "→ Configuring user groups for $USER_NAME..."
if id -nG "$USER_NAME" | tr ' ' '\n' | grep -qx input; then
    echo "  ✓ $USER_NAME already in 'input' group"
else
    usermod -aG input "$USER_NAME"
    echo "  ✓ Added $USER_NAME to 'input' group"
fi

# ── 3. Install systemd user service ─────────────────────────────
echo "→ Installing systemd user service..."
SYSTEMD_DIR="$USER_HOME/.config/systemd/user"
mkdir -p "$SYSTEMD_DIR"

# The packaged unit names /usr/bin/openggd; this source install uses ~/.local/bin.
sed "s|/usr/bin/openggd|$USER_HOME/.local/bin/openggd|" "$PROJECT_DIR/packaging/openggd.service" \
    > "$SYSTEMD_DIR/openggd.service"
echo "  ✓ Service installed to $SYSTEMD_DIR/openggd.service"

# ── 4. Install D-Bus activation file ────────────────────────────
echo "→ Installing D-Bus activation..."
DBUS_DIR="$USER_HOME/.local/share/dbus-1/services"
mkdir -p "$DBUS_DIR"

sed "s|/usr/bin/openggd|$USER_HOME/.local/bin/openggd|" "$PROJECT_DIR/packaging/org.opengg.Daemon.service" \
    > "$DBUS_DIR/org.opengg.Daemon.service"
echo "  ✓ D-Bus activation installed"

# ── 6. Enable the service ───────────────────────────────────────
echo "→ Enabling systemd service..."
# Run as the target user, not root
su - "$USER_NAME" -c "systemctl --user daemon-reload && systemctl --user enable openggd.service" 2>/dev/null || true
echo "  ✓ Service enabled"

echo ""
echo "╔══════════════════════════════════════╗"
echo "║  Setup complete!                     ║"
echo "║                                      ║"
echo "║  ⚠ Log out and back in for group     ║"
echo "║    changes to take effect.           ║"
echo "║                                      ║"
echo "║  Start the daemon:                   ║"
echo "║    systemctl --user start openggd    ║"
echo "║                                      ║"
echo "║  Or it auto-starts when the UI       ║"
echo "║  makes its first D-Bus call.         ║"
echo "╚══════════════════════════════════════╝"

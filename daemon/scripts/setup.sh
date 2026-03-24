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
USER_HOME=$(eval echo "~$USER_NAME")

# ── 1. Install udev rules ───────────────────────────────────────
echo "→ Installing udev rules..."
if [ -f "$PROJECT_DIR/packaging/99-opengg.rules" ]; then
    cp "$PROJECT_DIR/packaging/99-opengg.rules" /etc/udev/rules.d/
    udevadm control --reload-rules
    udevadm trigger
    echo "  ✓ udev rules installed"
else
    echo "  ⚠ udev rules file not found, skipping"
fi

# ── 2. Add user to required groups ──────────────────────────────
echo "→ Configuring user groups for $USER_NAME..."

for group in audio input video; do
    if getent group "$group" > /dev/null 2>&1; then
        if id -nG "$USER_NAME" | grep -qw "$group"; then
            echo "  ✓ $USER_NAME already in '$group' group"
        else
            usermod -aG "$group" "$USER_NAME"
            echo "  ✓ Added $USER_NAME to '$group' group"
        fi
    else
        echo "  ⚠ Group '$group' does not exist, skipping"
    fi
done

# i2c group for OpenRGB (create if missing)
if ! getent group "i2c" > /dev/null 2>&1; then
    groupadd i2c
    echo "  ✓ Created 'i2c' group"
fi
usermod -aG i2c "$USER_NAME" 2>/dev/null || true
echo "  ✓ $USER_NAME added to 'i2c' group"

# ── 3. Install systemd user service ─────────────────────────────
echo "→ Installing systemd user service..."
SYSTEMD_DIR="$USER_HOME/.config/systemd/user"
mkdir -p "$SYSTEMD_DIR"

cp "$PROJECT_DIR/packaging/openggd.service" "$SYSTEMD_DIR/"
echo "  ✓ Service installed to $SYSTEMD_DIR/openggd.service"

# ── 4. Install D-Bus activation file ────────────────────────────
echo "→ Installing D-Bus activation..."
DBUS_DIR="$USER_HOME/.local/share/dbus-1/services"
mkdir -p "$DBUS_DIR"

# Replace %h with actual home directory
sed "s|%h|$USER_HOME|g" "$PROJECT_DIR/packaging/org.opengg.Daemon.service" \
    > "$DBUS_DIR/org.opengg.Daemon.service"
echo "  ✓ D-Bus activation installed"

# ── 5. Install polkit policy ────────────────────────────────────
echo "→ Installing polkit policy..."
if [ -f "$PROJECT_DIR/packaging/org.opengg.setup.policy" ]; then
    cp "$PROJECT_DIR/packaging/org.opengg.setup.policy" /usr/share/polkit-1/actions/ 2>/dev/null || true
    echo "  ✓ polkit policy installed"
fi

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

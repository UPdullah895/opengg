#!/bin/sh
# Shared deb/rpm post-install. Must never fail the transaction, and must never
# touch user accounts: group membership is the admin's call (printed below).
# $1 is "configure" (deb) or the instance count 1/2 (rpm) — not needed here.

udevadm control --reload-rules >/dev/null 2>&1 || true
udevadm trigger >/dev/null 2>&1 || true

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || true
fi

cat <<'MSG'
OpenGG installed.

Post-install steps:
  1. Add your user to the required groups:
       sudo usermod -aG audio,input,video,i2c $USER
     (the i2c group may not exist on your system; skip it if usermod complains)
  2. Log out and back in (or reboot). This applies the group change and starts
     the audio services and user session bus this package may just have installed.
  3. Enable the daemon service:
       systemctl --user enable --now openggd.service
MSG

exit 0

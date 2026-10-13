#!/bin/sh
# Shared deb/rpm post-remove. Cosmetic cache refresh only; never fails.
# (rpm calls this on upgrade too, which is harmless.)

udevadm control --reload-rules >/dev/null 2>&1 || true

if command -v gtk-update-icon-cache >/dev/null 2>&1 && [ -d /usr/share/icons/hicolor ]; then
    gtk-update-icon-cache -q -t -f /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || true
fi

exit 0

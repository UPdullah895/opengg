# OpenGG — Makefile
#
# Usage:
#   make dev        → Full-stack dev mode (daemon + Qt6/QML frontend)
#   make daemon     → Build & run daemon only
#   make ui         → Run Qt6/QML frontend only
#   make build      → Release build (daemon + Qt6/QML frontend)
#   make setup      → First-time dependency install
#   make clean      → Remove all build artifacts
#   make install    → Install daemon + Qt6 binary to ~/.local/bin

SHELL := /bin/bash

ROOT    := $(shell pwd)
DAEMON  := $(ROOT)/daemon
QT_SHELL := $(ROOT)/qt-shell

.PHONY: dev daemon ui build setup clean install install-service install-desktop lint check help new-extension validate-extension ui-shots lint-qml

# ── Default ──────────────────────────────────────────────────────
help:
	@echo ""
	@echo "  OpenGG Development Commands"
	@echo "  ─────────────────────────────"
	@echo "  make dev       Full-stack dev (daemon + Qt6 frontend)"
	@echo "  make daemon    Build & run daemon (debug)"
	@echo "  make ui        Build & run Qt6/QML frontend"
	@echo "  make build     Release build (daemon + Qt6 frontend)"
	@echo "  make setup     Install all dependencies"
	@echo "  make clean     Remove build artifacts"
	@echo "  make install   Install daemon to ~/.local/bin"
	@echo "  make check     Type-check everything"
	@echo "  make lint      Clippy + check-colors.sh"
	@echo "  make new-extension NAME=<id>          Scaffold a new extension"
	@echo "  make validate-extension DIR=<path>   Validate extension manifest & files"
	@echo ""

# ── Full-stack dev ───────────────────────────────────────────────
dev:
	@chmod +x dev.sh && ./dev.sh

# ── Daemon only ──────────────────────────────────────────────────
daemon:
	@chmod +x dev.sh && ./dev.sh daemon

daemon-build:
	cd $(DAEMON) && cargo build

daemon-release:
	cd $(DAEMON) && cargo build --release

# ── Frontend only ────────────────────────────────────────────────
ui:
	@chmod +x dev.sh && ./dev.sh ui

# ── Release build ────────────────────────────────────────────────
build: daemon-release
	cd $(QT_SHELL) && cargo build --release

# ── Scaffold a new extension ─────────────────────────────────────
new-extension:
	@if [ -z "$(NAME)" ]; then echo "usage: make new-extension NAME=<kebab-case-id>"; exit 1; fi
	@chmod +x scripts/new-extension.sh && ./scripts/new-extension.sh "$(NAME)"

# ── Validate an extension ────────────────────────────────────────────
validate-extension:
	@if [ -z "$(DIR)" ]; then echo "usage: make validate-extension DIR=<path/to/extension>"; exit 1; fi
	@chmod +x scripts/validate-extension.sh && ./scripts/validate-extension.sh "$(DIR)"

# ── Setup ────────────────────────────────────────────────────────
setup:
	@chmod +x dev.sh && ./dev.sh setup

# ── Install ──────────────────────────────────────────────────────
install: build install-service
	@mkdir -p $(HOME)/.local/bin
	install -m 755 $(DAEMON)/target/release/openggd $(HOME)/.local/bin/openggd
	install -m 755 $(QT_SHELL)/target/release/opengg-qt $(HOME)/.local/bin/opengg
	@mkdir -p $(HOME)/.local/share/opengg/locales
	install -m 644 $(QT_SHELL)/locales/*.json $(HOME)/.local/share/opengg/locales/
	@echo "✓ Installed openggd to ~/.local/bin/"
	@echo "✓ Installed opengg-qt as ~/.local/bin/opengg"
	@echo "✓ Installed locales to ~/.local/share/opengg/locales/"

# ── systemd user service ─────────────────────────────────────────
# Install + enable the per-user daemon so the virtual audio engine auto-starts at
# login and survives reboots (the sinks are ephemeral and recreated on daemon start).
# No sudo needed — this is a `--user` unit.
install-service:
	@mkdir -p $(HOME)/.config/systemd/user $(HOME)/.local/share/dbus-1/services
	# The packaged units point at /usr/bin/openggd, which is where every
	# package puts it. A from-source install puts it in ~/.local/bin, so
	# rewrite the path rather than shipping a unit that starts the wrong
	# binary (or none at all).
	sed 's|/usr/bin/openggd|$(HOME)/.local/bin/openggd|' \
		$(ROOT)/packaging/openggd.service > $(HOME)/.config/systemd/user/openggd.service
	# Register D-Bus activation too. Without this the daemon only ever starts
	# because the unit below is enabled — `org.opengg.Daemon` could not be
	# activated on demand, which is the whole point of a bus-activated service.
	sed 's|/usr/bin/openggd|$(HOME)/.local/bin/openggd|' \
		$(ROOT)/packaging/org.opengg.Daemon.service \
		> $(HOME)/.local/share/dbus-1/services/org.opengg.Daemon.service
	@systemctl --user daemon-reload
	@systemctl --user enable --now openggd.service && \
		echo "✓ openggd.service enabled (auto-starts at login)" || \
		echo "⚠ Could not enable openggd.service — run: systemctl --user enable --now openggd.service"

# ── Desktop launcher ─────────────────────────────────────────────
install-desktop:
	@mkdir -p $(HOME)/.local/share/applications
	@sed "s|Exec=.*|Exec=$(ROOT)/opengg-launch.sh|" $(ROOT)/opengg.desktop \
		> $(HOME)/.local/share/applications/opengg.desktop
	@chmod +x $(ROOT)/opengg-launch.sh
	@update-desktop-database $(HOME)/.local/share/applications 2>/dev/null || true
	@echo "✓ Installed opengg.desktop to ~/.local/share/applications/"

# ── Code Quality ─────────────────────────────────────────────────
check:
	cd $(DAEMON) && cargo check
	cd $(QT_SHELL) && cargo check

lint:
	cd $(DAEMON) && cargo clippy -- -W clippy::all
	cd $(QT_SHELL) && cargo clippy -- -W clippy::all
	$(MAKE) lint-qml

# Guards qt-shell/qml against bare colour literals, frozen Qt.rgba tints and
# emoji-as-icons — the three defect classes from the UI-fidelity plan.
lint-qml:
	$(ROOT)/qt-shell/tools/check-colors.sh

# ── UI screenshots (qt-shell, headless) ──────────────────────────
# Renders every page + settings panel to PNG under QT_QPA_PLATFORM=offscreen.
# Never opens a window on your desktop. Use ONLY=<page> to capture just one.
ui-shots:
	cd $(ROOT)/qt-shell && cargo build
	$(ROOT)/qt-shell/tools/ui-shots.sh

# ── Clean ────────────────────────────────────────────────────────
clean:
	cd $(DAEMON) && cargo clean
	cd $(QT_SHELL) && cargo clean
	@echo "✓ Cleaned all build artifacts"

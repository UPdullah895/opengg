# OpenGG — Makefile
#
# Usage:
#   make dev        → Full-stack dev mode (daemon + frontend)
#   make daemon     → Build & run daemon only
#   make ui         → Run Tauri frontend only
#   make build      → Release build (daemon + frontend)
#   make setup      → First-time dependency install
#   make clean      → Remove all build artifacts
#   make install    → Install daemon binary to ~/.local/bin

SHELL := /bin/bash

ROOT    := $(shell pwd)
DAEMON  := $(ROOT)/daemon
FRONTEND := $(ROOT)/frontend
QT_SHELL := $(ROOT)/qt-shell

.PHONY: dev daemon ui ui-legacy build setup clean install install-service install-desktop lint check help new-extension validate-extension ui-shots lint-qml

# ── Default ──────────────────────────────────────────────────────
help:
	@echo ""
	@echo "  OpenGG Development Commands"
	@echo "  ─────────────────────────────"
	@echo "  make dev       Full-stack dev (daemon + Qt6 frontend)"
	@echo "  make daemon    Build & run daemon (debug)"
	@echo "  make ui        Build & run Qt6/QML frontend"
	@echo "  make ui-legacy Build & run Tauri/Vue frontend (archived)"
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

ui-legacy:
	@chmod +x dev.sh && ./dev.sh ui-legacy

ui-deps:
	cd $(FRONTEND) && npm install

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
	cp $(DAEMON)/target/release/openggd $(HOME)/.local/bin/
	cp $(QT_SHELL)/target/release/opengg-qt $(HOME)/.local/bin/opengg
	@echo "✓ Installed openggd to ~/.local/bin/"
	@echo "✓ Installed opengg-qt as ~/.local/bin/opengg"

# ── systemd user service ─────────────────────────────────────────
# Install + enable the per-user daemon so the virtual audio engine auto-starts at
# login and survives reboots (the sinks are ephemeral and recreated on daemon start).
# No sudo needed — this is a `--user` unit.
install-service:
	@mkdir -p $(HOME)/.config/systemd/user
	cp $(ROOT)/packaging/openggd.service $(HOME)/.config/systemd/user/
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
	cd $(FRONTEND)/src-tauri && cargo check

lint:
	cd $(DAEMON) && cargo clippy -- -W clippy::all
	cd $(QT_SHELL) && cargo clippy -- -W clippy::all
	cd $(FRONTEND) && npx vue-tsc --noEmit
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
	cd $(FRONTEND)/src-tauri && cargo clean
	rm -rf $(FRONTEND)/node_modules $(FRONTEND)/dist
	@echo "✓ Cleaned all build artifacts"

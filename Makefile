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

.PHONY: dev daemon ui build setup clean install lint check help

# ── Default ──────────────────────────────────────────────────────
help:
	@echo ""
	@echo "  OpenGG Development Commands"
	@echo "  ─────────────────────────────"
	@echo "  make dev       Full-stack dev (daemon + frontend)"
	@echo "  make daemon    Build & run daemon (debug)"
	@echo "  make ui        Run Tauri frontend dev server"
	@echo "  make build     Release build"
	@echo "  make setup     Install all dependencies"
	@echo "  make clean     Remove build artifacts"
	@echo "  make install   Install daemon to ~/.local/bin"
	@echo "  make check     Type-check everything"
	@echo "  make lint      Clippy + vue-tsc"
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

ui-deps:
	cd $(FRONTEND) && npm install

# ── Release build ────────────────────────────────────────────────
build: daemon-release
	cd $(FRONTEND) && npm install && npx tauri build

# ── Setup ────────────────────────────────────────────────────────
setup:
	@chmod +x dev.sh && ./dev.sh setup

# ── Install ──────────────────────────────────────────────────────
install: daemon-release
	@mkdir -p $(HOME)/.local/bin
	cp $(DAEMON)/target/release/openggd $(HOME)/.local/bin/
	@echo "✓ Installed openggd to ~/.local/bin/"
	@echo "  Run: sudo $(DAEMON)/scripts/setup.sh"

# ── Code Quality ─────────────────────────────────────────────────
check:
	cd $(DAEMON) && cargo check
	cd $(FRONTEND)/src-tauri && cargo check

lint:
	cd $(DAEMON) && cargo clippy -- -W clippy::all
	cd $(FRONTEND) && npx vue-tsc --noEmit

# ── Clean ────────────────────────────────────────────────────────
clean:
	cd $(DAEMON) && cargo clean
	cd $(FRONTEND)/src-tauri && cargo clean
	rm -rf $(FRONTEND)/node_modules $(FRONTEND)/dist
	@echo "✓ Cleaned all build artifacts"

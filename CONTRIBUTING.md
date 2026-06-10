# Contributing to OpenGG

Thank you for your interest in contributing to OpenGG! We welcome contributions of all kinds — bug fixes, features, documentation, translations, and more.

**OpenGG** is an open-source Linux gaming hub and a modular alternative to SteelSeries GG. It combines an audio mixer, device/RGB manager, and instant replay system. Licensed under MIT.

## Getting Started

### Prerequisites

- **Rust** (stable) — [rustup.rs](https://rustup.rs/)
- **Node.js 20+** and npm
- **System dependencies**:
  - PipeWire and WirePlumber (audio)
  - webkit2gtk-4.1 (Tauri WebView)
  - Optional: gpu-screen-recorder, FFmpeg, xdg-desktop-portal

For detailed install instructions, see the [README](README.md) "Requirements" section.

### First-Time Setup

```bash
# Clone the repo
git clone https://github.com/UPdullah895/opengg.git
cd opengg

# Run setup once (udev rules, groups, D-Bus policy, data dirs)
./dev.sh setup

# Install frontend dependencies
cd frontend && npm install && cd ..
```

### Running Locally

```bash
# Full stack (daemon + Tauri frontend with unified logs) — recommended
make dev
# or
./dev.sh

# Individual parts
make daemon          # Daemon only (openggd)
make ui              # Frontend only (Tauri + Vue with hot-reload)

# Build for release
make build
```

Each command supports Ctrl+C for graceful shutdown.

## Quality Gates

Before opening a pull request, run these locally:

### Linting & Type Checking

```bash
# Rust (both daemon and Tauri crates)
make lint            # cargo clippy on both crates + npx vue-tsc
make check           # cargo check (fast, no linting)

# Note: CI enforces `cargo clippy -- -D warnings` on both crates
# It's recommended to run this locally too:
cd daemon && cargo clippy -- -D warnings
cd ../frontend/src-tauri && cargo clippy -- -D warnings
```

### Frontend

```bash
cd frontend

# Type checking
npx vue-tsc --noEmit

# Tests
npm test

# Locale key parity (English ↔ Arabic)
npm run check:locales
```

## Code Rules Digest

See [CLAUDE.md](CLAUDE.md) for detailed architecture notes. Key rules:

1. **Two separate Rust crates** — `daemon/` and `frontend/src-tauri/` are not a workspace. Run `cargo` commands from inside each directory.

2. **New Tauri commands** must be registered in **both** places:
   - Add the function to `frontend/src-tauri/src/commands.rs` with `#[command]`
   - List it in `frontend/src-tauri/src/main.rs` in the `invoke_handler!` macro
   - Omitting either silently fails at runtime

3. **No hardcoded user-facing strings** — use i18n. All text in templates must come from `t()` backed by both `en.json` and `ar.json`.

4. **Design tokens, not hardcoded colors** — use CSS custom properties (`var(--accent)`, `var(--text)`, etc.). See [docs/DESIGN_TOKENS.md](docs/DESIGN_TOKENS.md).

5. **Scoped styles only** — all components use `<style scoped>` to prevent global pollution.

6. **Never restart PipeWire** — virtual sinks are stateless. Only recreate them if the user explicitly resets.

7. **Subprocess module for external binaries** — use `spawn()`, not `Command::new()` directly, for proper cleanup.

## Pull Request Workflow

1. Create a branch off `main`
2. Make atomic commits with clear messages
3. Push and open a PR using the [pull request template](.github/pull_request_template.md)
4. Ensure CI is green (ci, security, codeql, locale check, distro matrix)
5. Request review

### PR Checklist

Your PR should pass:

- `make lint` (Clippy + vue-tsc)
- Both crates: `cargo clippy -- -D warnings`
- `npm run check:locales` (if you modified locales)
- `npm test` (if applicable)
- i18n keys present in both `en.json` and `ar.json`
- If you added new Tauri commands: registered in both `commands.rs` and `invoke_handler!`
- If you changed UI: include screenshots
- No hardcoded colors (use design tokens)

## Where to Contribute

### Good First Issues

These are great starting points for new contributors:

- **Internationalization (i18n)** — help translate OpenGG into new languages
  - Copy `frontend/src/locales/en.json` to a new locale file (e.g., `es.json`)
  - Translate all string values (keys stay the same)
  - Add `_meta` for display name and text direction
  - See [docs/TRANSLATING.md](docs/TRANSLATING.md) for details
- **Documentation** — improve README, add guides, clarify constraints
- **Bug reports** — find and report bugs via [bug_report.yml](.github/ISSUE_TEMPLATE/bug_report.yml)
- **UI/UX improvements** — design refinements, accessibility

### Extensions

Want to build a third-party extension? Start with the [extension template](extension-template/README.md) and the [manifest schema](EXTENSION_MANIFEST_SCHEMA.md). Extensions run in a sandbox and can provide UI panels or background daemons.

### Security

If you discover a security vulnerability, **do not open a public issue**. Instead, see [SECURITY.md](SECURITY.md) for private reporting procedures.

## Architecture Overview

- **Frontend** — Tauri 2 + Vue 3 + Pinia + TypeScript
- **Daemon** — Rust background service (D-Bus)
- **IPC** — Tauri commands (frontend ↔ Tauri Rust), D-Bus session bus (Tauri ↔ Daemon)
- **Audio** — PipeWire virtual sinks, app routing, parametric EQ
- **Devices** — ratbagd (mouse/keyboard), OpenRGB SDK (RGB)
- **Replay** — gpu-screen-recorder subprocess, clip gallery, FFmpeg trim/export

See [CLAUDE.md](CLAUDE.md) and [frontend/CLAUDE.md](frontend/CLAUDE.md) for detailed constraints, state management, and IPC patterns.

## Community

Questions? Issues? Ideas?

- **Bug reports** — Use [issue templates](.github/ISSUE_TEMPLATE/)
- **Feature requests** — Describe the problem, proposed solution, and alternatives
- **Security concerns** — See [SECURITY.md](SECURITY.md)
- **Translations** — See [docs/TRANSLATING.md](docs/TRANSLATING.md)

## License

All contributions are licensed under MIT. See [LICENSE](LICENSE) for details.

---

Happy coding! 🎮

# Contributing to OpenGG

Thank you for your interest in contributing to OpenGG! We welcome contributions of all kinds — bug fixes, features, documentation, translations, and more.

**OpenGG** is an open-source Linux gaming hub and a modular alternative to SteelSeries GG. It combines an audio mixer, device/RGB manager, and instant replay system. Licensed under MIT.

## Getting Started

### Prerequisites

- **Rust** (stable) — [rustup.rs](https://rustup.rs/)
- **Qt 6.7+** (`qt6-base`, `qt6-declarative`, `qt6-multimedia`, `qt6-shadertools`)
- **System dependencies**:
  - PipeWire and WirePlumber (audio)
  - GStreamer + `gst-plugin-qml6` (unified clip playback)
  - Optional: gpu-screen-recorder, FFmpeg, xdg-desktop-portal
  - Optional: Node.js 18+ and npm — only needed to scaffold/build third-party extensions via `make new-extension`

For detailed install instructions, see the [README](README.md) "Requirements" section.

### First-Time Setup

```bash
# Clone the repo
git clone https://github.com/UPdullah895/opengg.git
cd opengg

# Run setup once (udev rules, groups, D-Bus policy, data dirs)
./dev.sh setup
```

### Running Locally

```bash
# Full stack (daemon + Qt6/QML frontend with unified logs) — recommended
make dev
# or
./dev.sh

# Individual parts
make daemon          # Daemon only (openggd)
make ui              # Qt6/QML frontend only (debug build + run)

# Build for release
make build
```

Each command supports Ctrl+C for graceful shutdown.

## Quality Gates

Before opening a pull request, run these locally:

### Linting & Type Checking

```bash
make lint            # cargo clippy on daemon + qt-shell + check-colors.sh
make check           # cargo check (fast, no linting)

# Note: CI enforces `cargo clippy -- -D warnings` on daemon and qt-shell.
# It's recommended to run this locally too:
cd daemon && cargo clippy -- -D warnings
cd ../qt-shell && cargo clippy --all-targets -- -D warnings
```

### Qt UI work has an additional mandatory gate — see [AGENTS.md](AGENTS.md)

```bash
qt-shell/tools/check-colors.sh   # no bare hex / frozen tints / emoji icons
qt-shell/tools/ui-shots.sh       # must finish with ZERO QML warnings — capture logs
                                  # with QT_FORCE_STDERR_LOGGING=1, and actually look
                                  # at the generated screenshots under qt-shell/target/ui-shots/
```

## Code Rules Digest

See [CLAUDE.md](CLAUDE.md) and [AGENTS.md](AGENTS.md) for detailed architecture notes and the full list of QML landmines. Key rules:

1. **Three separate Rust crates** — `daemon/`, `core/`, and `qt-shell/` are not a Cargo workspace. Run `cargo` commands from inside each directory.

2. **Never import `daemon/` types into `qt-shell/` or vice versa** — the Qt shell reaches the daemon and the system only through `opengg-core` (blocking API) and D-Bus. See AGENTS.md's "Crate Separation" section.

3. **New QML files must be registered in `qt-shell/build.rs`** — a missing registration causes silent undefined behavior at runtime, not a build error.

4. **No hardcoded user-facing strings** — use i18n. All text must come from `I18n.t()` backed by both `qt-shell/locales/en.json` and `qt-shell/locales/ar.json`.

5. **Theme tokens, not hardcoded colors** — use `Theme.accent`, `Theme.bgCard`, `Theme.text`, etc. from the `Theme` singleton (`qt-shell/qml/Theme.qml`), never bare hex literals or `Qt.rgba()`. `qt-shell/tools/check-colors.sh` enforces this.

6. **Icons from the `Icons.qml` registry** — never embed inline SVG or use emoji as icons.

7. **Never restart PipeWire** — virtual sinks are stateless. Only recreate them if the user explicitly resets.

8. **Subprocess module for external binaries** — use `subprocess::command()`/`spawn()`, not `std::process::Command::new()` directly, for consistent behavior.

## Pull Request Workflow

1. Create a branch off `main`
2. Make atomic commits with clear messages
3. Push and open a PR using the [pull request template](.github/pull_request_template.md)
4. Ensure CI is green (ci, security, distro matrix)
5. Request review

### PR Checklist

Your PR should pass:

- `make lint` (Clippy + check-colors.sh)
- `daemon/` and `qt-shell/`: `cargo clippy -- -D warnings`
- `qt-shell/tools/ui-shots.sh` with zero QML warnings, screenshots visually verified (if you touched QML)
- i18n keys present in both `qt-shell/locales/en.json` and `qt-shell/locales/ar.json` (there is no automated key-parity check yet for the Qt UI — verify by hand)
- If you changed UI: include screenshots
- No hardcoded colors (use `Theme` tokens) or emoji icons

## Where to Contribute

### Good First Issues

These are great starting points for new contributors:

- **Internationalization (i18n)** — help translate OpenGG into new languages
  - Copy `qt-shell/locales/en.json` to a new locale file (e.g., `es.json`)
  - Translate all string values (keys stay the same)
  - See [docs/TRANSLATING.md](docs/TRANSLATING.md) for details
- **Documentation** — improve README, add guides, clarify constraints
- **Bug reports** — find and report bugs via [bug_report.yml](.github/ISSUE_TEMPLATE/bug_report.yml)
- **UI/UX improvements** — design refinements, accessibility

### Extensions

Want to build a third-party extension? Start with the [extension template](extension-template/README.md) and the [manifest schema](EXTENSION_MANIFEST_SCHEMA.md). Extensions run in a sandbox and can provide UI panels or background daemons.

### Security

If you discover a security vulnerability, **do not open a public issue**. Instead, see [SECURITY.md](SECURITY.md) for private reporting procedures.

## Architecture Overview

- **Frontend** — Qt6/QML + cxx-qt (native, `qt-shell/`)
- **Shared Logic** — `opengg-core` crate (`core/`) — all daemon/PipeWire/SQLite/media access
- **Daemon** — Rust background service (D-Bus, `daemon/`)
- **IPC** — cxx-qt controllers call `opengg-core` in-process (no IPC hop); `opengg-core` talks to the daemon over the D-Bus session bus
- **Audio** — PipeWire virtual sinks, app routing, parametric EQ
- **Devices** — ratbagd (mouse/keyboard), OpenRGB SDK (RGB)
- **Replay** — gpu-screen-recorder subprocess, clip gallery, FFmpeg trim/export

See [CLAUDE.md](CLAUDE.md) and [AGENTS.md](AGENTS.md) for detailed constraints, state management, and IPC patterns.

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

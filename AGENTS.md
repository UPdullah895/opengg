# AGENTS.md — AI Contributor Guide

This document is **required reading** for any AI agent implementing features or fixes in OpenGG. It captures hard-won landmines, architecture constraints, and verification protocols discovered across multiple migration sessions.

**Every work session must end by appending an entry to [`docs/AI-CHANGELOG.md`](docs/AI-CHANGELOG.md).**

---

## Architecture Boundaries (Non-Negotiable)

### Crate Separation

OpenGG is **not** a single Cargo workspace. Each crate is built independently:

```
daemon/          ← Runs as background service (openggd)
├── Cargo.toml
├── src/main.rs
└── ...

core/            ← Shared business logic (opengg-core library)
├── Cargo.toml
├── src/lib.rs
└── ...

qt-shell/        ← Native Qt6/QML UI (binary: opengg-qt)
├── Cargo.toml
├── src/main.rs
└── qml/
```

**Rule**: When modifying code, run `cargo clippy` and `cargo test` **from inside each touched crate's directory**, not from the repo root. The Makefile's `lint` and `check` targets handle this orchestration.

### IPC & Access Control

```
┌─ qt-shell (presentation layer)
│  └─ D-Bus invoke ──→ daemon
│                    └─ uses opengg-core for:
│                       • PipeWire routing (pactl calls)
│                       • SQLite clip database
│                       • File I/O (clips, config, thumbnails)
│
├─ daemon (background service)
│  └─ opengg-core (library)
│     ├── audio/ — channel volumes, app→sink routing, EQ state
│     ├── replay/ — clip scanning, metadata, FFmpeg integration
│     └── device/ — ratbagd, OpenRGB, game profiles
```

**Rule**: The Qt shell is **presentation glue only**. All business logic (daemon state queries, settings mutations, file operations) flows through `opengg-core` → daemon D-Bus interface. Never import `daemon/` types into `qt-shell/` or vice versa — use D-Bus bindings only.

---

## Pre-Commit Verification (Mandatory)

Before staging **any** commit, you must run all of these checks and **verify their output**:

### 1. Clippy + Tests (per touched crate)

For each crate you modified (daemon, core, or qt-shell):

```bash
cd <crate>
cargo clippy -- -W clippy::all   # Must pass with zero warnings
cargo test                        # Must pass all tests
```

Clippy warnings are blockers — fix or allow with `#[allow(clippy::foo)]` + a comment explaining why.

### 2. QML Lint

```bash
qt-shell/tools/check-colors.sh
# Must exit with code 0 (no violations)
```

This guards against:
- Bare hex color literals (use Theme tokens only)
- Frozen `Qt.rgba()` tints (use Theme tokens)
- Emoji used as icons (use Icons.qml registry)

### 3. UI Screenshots (with zero QML warnings)

```bash
qt-shell/tools/ui-shots.sh
# Renders every page to PNG under QT_QPA_PLATFORM=offscreen
# Output: qt-shell/target/ui-shots/
# Must complete with NO QML warnings in stderr
```

**Critical**: Qt logs go to journald, not stderr. On a clean build, stderr appears empty even with real warnings present. Always capture logs:

```bash
QT_FORCE_STDERR_LOGGING=1 qt-shell/tools/ui-shots.sh 2>&1 | tee /tmp/qml-build.log
grep -i "warning\|error" /tmp/qml-build.log
# Any hit is a blocker
```

Then **actually look at the generated PNG screenshots** — never sign off UI work based on a clean log alone. Verify:
- Text is readable (font, contrast, no clipping)
- Layout matches design intent (no items off-screen, proper alignment)
- Colors are consistent with theme
- Icons render (no placeholder boxes)

### 4. Full Integration

Before committing, verify the complete flow end-to-end:

```bash
make lint       # Runs clippy on daemon + qt-shell + vue-tsc + check-colors.sh
make check      # Fast: just cargo check (no linting)
make build      # Full release build (may take 5+ minutes)
```

If `make build` succeeds and `make lint` has zero errors, you may commit.

---

## QML Landmines (Critical)

### 1. Integer `pixelSize` Only

**LANDMINE**: Fractional `font.pixelSize` values (e.g., `12.5`) hang the entire app indefinitely.

```qml
// ✗ WRONG — app hangs
Text {
    font.pixelSize: 12.5  // ANY decimal = hang
}

// ✓ CORRECT
Text {
    font.pixelSize: 12    // integers only
}
```

**Why**: Qt's text rendering pipeline on some platforms has a bug with fractional pixel sizes that causes an infinite loop in glyph rasterization.

### 2. QML File Registration in `build.rs`

Every new QML file **must** be registered in `qt-shell/build.rs`. Missing registration causes **silent undefined behavior** — bindings evaluate to `undefined`, imports fail, singletons don't initialize.

```rust
// qt-shell/build.rs (example, do not copy — read the actual file)
cc::build()
    .register_qml_module(
        "com.opengg.app",
        "qml",
    )
    .qml_file_paths(&["src/qml/App.qml", "src/qml/pages/MixerPage.qml", ...])
    .compile("opengg_qml");
```

**Rule**: After adding a new `.qml` file:
1. Add it to the `.qml_file_paths(&[...])` array in `build.rs`
2. For singletons (shared application state), add `.singleton(true)` to the registration
3. Recompile: `cd qt-shell && cargo build`
4. Test: launch the app and verify the new component renders without `undefined` errors

### 3. QML Files in Subdirectories

A QML file in `qml/pages/MixerPage.qml` must declare:

```qml
// MixerPage.qml (in subdirectory)
import com.opengg.app

// Now you can use other registered components
import QtQuick
import QtQuick.Controls as QQC2
// ...
```

Without the explicit `import com.opengg.app`, QML imports from sibling files fail with "module not found".

### 4. Layout Container Width/Height Rules

Two distinct traps, both hit for real in this codebase:

**Inside a `RowLayout` / `ColumnLayout` / `GridLayout`** (Qt Quick Layouts),
a bare `width:` / `height:` on a child is silently overwritten by the
layout's arrange pass — use the `Layout.preferredWidth` /
`Layout.preferredHeight` attached properties instead:

```qml
// ✗ WRONG — arrange pass overwrites this back to a stale value
RowLayout {
    Rectangle { width: 200 }
}

// ✓ CORRECT
RowLayout {
    Rectangle { Layout.preferredWidth: 200 }
}
```

**Inside a plain `Row` / `Column`** (positioners, NOT layouts), the
`Layout.*` attached properties do NOTHING — and a QQC2 control (e.g.
`Slider`) given a bare `width:` there can contribute ZERO width, making it
vanish entirely (this is how the clips-per-row slider disappeared). If a
QQC2 control needs explicit sizing in a row, use a `RowLayout` with
`Layout.preferredWidth`, not a `Row`.

### 5. QQC2 Control Value Bindings Are One-Time

A value binding on a QQC2 control (Slider, SpinBox, etc.) is **permanently severed** the first time the user interacts with it:

```qml
// BAD — binding breaks after first user click
Slider {
    value: myStore.currentVolume  // ← severed after user moves slider
}

// GOOD — seed once, resync externally
Slider {
    Component.onCompleted: { value = myStore.currentVolume }
    Connections {
        target: myStore
        function onCurrentVolumeChanged() { slider.value = myStore.currentVolume }
    }
    onMoved: myStore.currentVolume = value  // sync user edits back
}
```

This is by design in Qt — prevent feedback loops where a user slider movement triggers a binding update that re-sets the slider position.

### 6. Anchors: Parent/Sibling Only

Anchors can only target the parent or siblings, never cross-branch relationships:

```qml
// ✗ WRONG — fails with only a log warning (does not crash)
Column {
    Rectangle { id: sibling }
    Rectangle {
        anchors.top: parent.top   // OK
        anchors.right: sibling.right  // FAIL (different parent branch)
    }
}

// ✓ CORRECT
Column {
    Rectangle { id: rect1 }
    Rectangle {
        anchors.top: rect1.bottom  // sibling in same Column
    }
}
```

**Why**: Qt's anchor system assumes a tree hierarchy. Cross-branch anchors fail to resolve at runtime.

### 7. `mapToItem()` Bindings Go Stale

A declarative binding using `mapToItem()` evaluates once and never re-evaluates:

```qml
// ✗ WRONG — binding computed once, stale after that
Rectangle {
    property point mapped: parent.mapToItem(someOtherItem, 10, 20)
    // ^ evaluates once, never updates
}

// ✓ CORRECT — recompute imperatively
Rectangle {
    function updateMappedPoint() {
        mapped = parent.mapToItem(someOtherItem, 10, 20)
    }
    Component.onCompleted: updateMappedPoint()
    onVisibleChanged: updateMappedPoint()  // re-sync when visibility changes
}
```

**Why**: `mapToItem()` is an imperative method, not a property. Bindings don't re-evaluate when item geometry changes.

### 8. Rust Invokable — QML Cannot See Nested Property Reads

If a Rust invokable returns a struct with nested properties, QML can only see the top-level fields. Nested property changes don't trigger reactive updates:

```rust
// Rust invokable
#[qml_element]
pub struct AudioState {
    pub channels: Vec<Channel>,  // QML sees this
}

pub struct Channel {
    pub volume: f32,  // QML CANNOT see this directly
}
```

```qml
// QML receives AudioState but cannot bind to channels[0].volume
let state = invoke('getAudioState')
state.volume  // OK
state.channels[0].volume  // always undefined in bindings

// FIX: Rust must emit a signal on nested property change
```

**Workaround**: Rust must explicitly emit a signal or Q_PROPERTY when nested state changes, or QML must re-query the entire state via a fresh invokable call.

### 9. QML Debugging Technique: Offscreen Screenshots

For hard-to-reproduce UI bugs, capture screenshots without opening a window:

```bash
# Build in debug mode
cd qt-shell && cargo build

# Capture a single page offscreen (no window decoration, no desktop)
QT_QPA_PLATFORM=offscreen ./target/debug/opengg-qt \
    --page MixerPage \
    --screenshot /tmp/mixer.png \
    --delay 2000  # wait 2s for renders to settle
```

Then inspect `/tmp/mixer.png` in an image viewer. This captures exactly what QML rendered, including any off-screen content.

---

## Theme & Icon Rules

### 1. Theme Tokens Only (No Bare Colors)

**Rule**: Never use bare hex colors, `Qt.rgba()` literals, or CSS color names in QML. Always use Theme tokens:

```qml
// ✗ WRONG
Rectangle { color: "#FF6B6B" }
Rectangle { color: Qt.rgba(1, 0.5, 0.5, 1) }
Rectangle { color: "red" }

// ✓ CORRECT
Rectangle { color: Theme.accent }
Rectangle { color: Theme.bgCard }
Rectangle { color: Theme.textPrimary }
```

The `check-colors.sh` linter enforces this and will block commits with hardcoded colors.

### 2. Icons from `Icons.qml` Registry

All icons are registered in `qml/Icons.qml` and extracted at build time from SVG files. Never embed SVG inline or use emoji as icons:

```qml
// ✗ WRONG
Image { source: "file:///path/to/icon.svg" }
Text { text: "🎵" }  // emoji as icon

// ✓ CORRECT
Image { source: Icons.playIcon }      // pre-registered SVG path
Image { source: Icons.channelVolume }
```

**Before adding a new icon**:
1. Place the SVG file in `qt-shell/assets/icons/`
2. Register it in `qml/Icons.qml`:
   ```qml
   // Icons.qml (singleton)
   readonly property string newIcon: "..." // SVG path data extracted at build
   ```
3. Rebuild: `cd qt-shell && cargo build`

### 3. Per-Channel Identity Colors

Audio channels have assigned identity colors. Use `Theme.channelColor(channelId)` to retrieve them:

```qml
// ✗ WRONG
Rectangle { color: channel.id === "game" ? "#FF6B6B" : "#4A90E2" }

// ✓ CORRECT
Rectangle { color: Theme.channelColor(channel.id) }
```

The Theme singleton manages the full channel→color mapping and respects light/dark theme switching.

---

## Audio & System Rules (Critical)

### 1. NEVER Restart PipeWire

Virtual audio sinks are created via `pactl load-module module-null-sink`. App routing uses `pactl move-sink-input`. These are **stateless CLI calls**.

**Rule**: Do NOT tear down and recreate PipeWire or its connections unless the user explicitly resets from Settings. Never call `systemctl restart --user pipewire` or `pw-cli stop` during development or testing.

**Why**: Restarting PipeWire disconnects all existing audio clients and loses all user routing decisions. Users expect their mixer state to persist across daemon/app restarts.

### 2. Daemon Binary Can Be Stale

After pulling new code, the installed `~/.local/bin/openggd` might be outdated. Before debugging daemon behavior:

```bash
# Compare the installed binary's timestamp against the most recent daemon commit
INSTALLED_MTIME=$(stat -c '%Y' ~/.local/bin/openggd)
CODE_MTIME=$(git log -1 --format=%ct -- daemon/src)

if [ "$INSTALLED_MTIME" -lt "$CODE_MTIME" ]; then
    echo "Daemon binary is stale — run: make install"
    make install
fi
```

This catches cases where a previous session built the daemon but didn't reinstall.

### 3. Replace Running Binaries Safely

If you need to replace a running binary (daemon, Qt app), use atomic move-to-temp-then-replace:

```bash
# ✗ WRONG — fails with ETXTBSY if process still runs
cp new-binary ~/.local/bin/opengg

# ✓ CORRECT — atomic replacement
cp new-binary /tmp/opengg.new
mv /tmp/opengg.new ~/.local/bin/opengg  # atomic rename
```

This ensures the running process doesn't keep the old binary's file handle.

### 4. `ui-settings.json` Top-Level Keys

Some settings are stored at the **top level** of `~/.config/opengg/ui-settings.json`, as siblings of `"settings"`:

```json
{
  "settings": { /* display theme, language, notification style */ },
  "modules": { /* enabled audio/device/replay modules */ },
  "extensionConsents": { /* user approvals for ext permissions */ },
  "mixer": { /* app→channel routing, mutes, volumes */ },
  "macros": { /* custom hotkey sequences */ }
}
```

**Rule**: Before wiring any new top-level data through `SettingsController.setValue()`, inspect the **live file** and the daemon's settings loader to understand the schema. Mutations to `"settings"` go through the normal debounced save; top-level siblings may require different serialization logic.

---

## Process Discipline

### 1. Dev App Runs on Your LIVE Desktop

The `./dev.sh` or `make ui` commands launch the Qt app on your development machine's actual display. It has **real side effects**:

- GPU screen recorder can start/stop and write video files to disk
- Audio devices and virtual sinks are created/destroyed
- User configuration files are read and written
- Hotkeys are registered and may conflict with system shortcuts

**Rule**: 
- Test destructive paths (reset audio, export clips) on **dummy files** only, never production data
- Be aware that the app is live on your desktop — a crash will disrupt your audio session
- Before testing cleanup/reset features, back up `~/.config/opengg/` and `~/.local/share/opengg/`

### 2. Do NOT Use Git Worktrees

A previous migration discovered that worktree isolation has a bug in this codebase where the Rust build resolves the wrong base branch. **Never use `git worktree` in this repo.**

Instead, commit your changes and test on the current branch.

### 3. Relaunch the Dev Binary Safely

If you need to restart the app with fresh state:

```bash
# Kill any existing instance
pkill -9 opengg-qt

# Wait for process group cleanup
sleep 1

# Launch in a new session (prevents the relaunch from dying when your terminal closes)
nohup setsid ./target/debug/opengg-qt > /tmp/opengg.log 2>&1 < /dev/null &
disown
```

Using `setsid` gives the process its own session ID, so killing your terminal doesn't propagate to the app.

---

## Documentation Convention (Mandatory)

After **every work session**, you must append a new entry to [`docs/AI-CHANGELOG.md`](docs/AI-CHANGELOG.md). This is the authoritative session log for all AI contributors.

**Format**:

```markdown
### [YYYY-MM-DD] Agent Name (Claude Model) — Branch

**What Changed:**
- Commit hash: description (e.g., abc1234: Fix Qt pixelSize hang)
- Commit hash: description

**Why:**
A 1–2 sentence explanation of the business/technical reason for the work.

**Landmines & Discoveries:**
- Any hard-won insights (e.g., "Rust invokables don't see nested property changes; QML must re-query")
- Constraints or gotchas uncovered during implementation

**Verification:**
- `make lint` passed
- `make build` succeeded
- `qt-shell/tools/ui-shots.sh` with zero QML warnings
- Manual visual inspection of [screenshot/feature]
- Full end-to-end flow tested: [specific scenario]
```

See [`docs/AI-CHANGELOG.md`](docs/AI-CHANGELOG.md) for examples.

---

## Quick Checklist Before Pushing

- [ ] `make lint` passes with zero warnings
- [ ] `make build` succeeds
- [ ] `cargo test` passes in all touched crates
- [ ] Screenshots captured with `QT_FORCE_STDERR_LOGGING=1` and visually verified
- [ ] No bare hex/emoji in QML; all colors use Theme tokens
- [ ] All new QML files registered in `qt-shell/build.rs`
- [ ] No cross-crate imports between daemon/qt-shell (D-Bus only)
- [ ] Integer `pixelSize` only (no decimals)
- [ ] Destructive tests run on dummy files
- [ ] Commit messages are clear and include reasoning
- [ ] Entry appended to `docs/AI-CHANGELOG.md`

---

## Key References

- **`CLAUDE.md`** (root) — Full-stack architecture, IPC design, data paths
- **`docs/superpowers/plans/2026-08-01-clip-audio-and-design-parity.md`** — Upstream feature plan (GStreamer unification, design requirements)
- **`qt-shell/build.rs`** — QML registration, resource bundling
- **`qt-shell/tools/check-colors.sh`** — Color/emoji/icon linter
- **`qt-shell/tools/ui-shots.sh`** — Offscreen screenshot renderer
- **`Makefile`** — Build orchestration (lint, check, build, install, test)
- **`dev.sh`** — Dev server unified orchestration

---

**Remember**: This guide captures real bugs and constraints. Follow every rule strictly. If you encounter a new landmine or discover a constraint not listed here, add it to this document and file an entry in `docs/AI-CHANGELOG.md` so the next agent learns from it.

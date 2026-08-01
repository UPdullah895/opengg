# AI Contributor Changelog

This is the **append-only session log** for all AI agents working on OpenGG. Every agent must update this after completing work.

**Format**: Newest entry first. See the template below.

---

### [2026-08-01] Claude Fable 5 — qt6-migration

**What Changed:**
- `2c5f8ac`: Archive the Tauri UI in place; qt-shell becomes the default build/dev flows
  - Updated Makefile lint recipe to run clippy on qt-shell + check-colors.sh
  - Fixed dev.sh run_frontend build guard (replaced `& wait $!` pattern with proper `if ! cargo build`)
  - Added ui-legacy flow documentation to dev.sh help text
- `a7e4c1f`: Point the launcher at the Qt shell binary
  - Reordered opengg-launch.sh candidate search: qt-shell/target/release/opengg-qt first
  - Reverted packaging/*.desktop StartupWMClass changes (were incorrect; left unchanged)
- `f3b2d8c`: README: reflect the Qt6/QML architecture and switchover
  - Rewrote architecture section: Qt6/QML + cxx-qt, shared opengg-core, daemon, legacy UI archived
  - Updated Requirements: Qt 6, GStreamer + gst-plugin-qml6, removed Tauri/Node from mandatory build tools
  - Reflected all verified build/run commands from fixed Makefile and dev.sh
  - Kept AUR install, data locations, and troubleshooting sections (still accurate)
- `9c3f7e2`: Add AI contributor guide and session changelog convention
  - Created AGENTS.md: mandatory read for AI agents working on this codebase
    - Architecture boundaries: crate separation, IPC flow, access control
    - Mandatory verification: clippy/test/lint/ui-shots protocol before commits
    - QML landmines: pixelSize integers, file registration, layout rules, binding lifetime, anchors, mapToItem(), Rust invokables
    - Theme/icon rules: Theme tokens only, no bare hex/emoji
    - Audio/system rules: never restart PipeWire, stale daemon binary check, safe binary replacement
    - Process discipline: dev app on live desktop, no git-worktree, safe process relaunch
    - Documentation convention: append to AI-CHANGELOG after each session
  - Created docs/AI-CHANGELOG.md with this header and first entry

**Why:**
The Qt6/QML migration switchover was left half-done by a previous agent (two stalls). This session completes all remaining fixes and infrastructure:
1. The Makefile lint target didn't actually run qt-shell clippy (just referenced it)
2. dev.sh had a dead-code bug where failed builds wouldn't properly abort
3. opengg-launch.sh prioritized old installed binaries over the repo build, launching stale UI
4. README still described the old Tauri architecture
5. No documentation existed for AI agents working in this codebase (high friction for next contributor)

**Landmines & Discoveries:**
- **Git worktree bug**: Previous agent discovered git-worktree isolation doesn't work in this repo (wrong base branch resolution). Documented as non-negotiable rule in AGENTS.md.
- **Qt stderr logging**: Qt logs go to journald, not stderr. Screenshots with `QT_FORCE_STDERR_LOGGING=1` are required to catch real warnings. Build logs often appear clean but hide warnings.
- **Pipeline error handling under `set -euo pipefail`**: The `cargo build | sed &` followed by `wait $!` pattern is a dead code trap. Under strict shell settings, backgrounded pipelines fail silently. Replaced with guarded pipeline `if ! cargo build | sed`.
- **check-colors.sh integration**: The Makefile lint target now correctly invokes it; previous state was incomplete.
- **Desktop file WM_CLASS**: Extensive investigation showed Qt apps set WM_CLASS from the binary name, not `StartupWMClass` in .desktop files. Reverted unnecessary changes to packaging/*.desktop files.
- **GStreamer unified pipeline**: README updated to clarify gst-plugin-qml6 is a runtime dependency (not build-time), replacing Qt Multimedia for clip playback.

**Verification:**
- `bash -n dev.sh` passed
- `bash -n opengg-launch.sh` passed
- `make -n ui`, `make -n ui-legacy`, `make -n build`, `make -n install`, `make -n lint` all passed
- Manual verification of Makefile recipe order and help text consistency
- README.md architecture section cross-checked against CLAUDE.md and qt-shell/ file structure
- AGENTS.md reviewed for completeness against all landmines and rules mentioned in task context

---

## Entry Template

When you complete work on OpenGG, copy this template and fill it in:

```markdown
### [YYYY-MM-DD] Agent Name (Claude Model) — Branch

**What Changed:**
- Commit hash: description
- Commit hash: description

**Why:**
A 1–2 sentence explanation of the business/technical reason.

**Landmines & Discoveries:**
- Any hard-won insights or constraints uncovered

**Verification:**
- `make lint` passed
- `make build` succeeded
- Manual testing of [specific scenario]
```

**Important**: Keep entries **newest first**. Maintain the exact format so agents can scan this file quickly.

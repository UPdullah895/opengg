## Description

Please include a summary of your changes. What problem does this PR solve? Why is this change needed?

Fixes #(issue number, if applicable)

## Type of Change

- [ ] Bug fix (non-breaking)
- [ ] New feature (non-breaking)
- [ ] Breaking change (requires major version bump)
- [ ] Documentation update

## Changes Made

- Change 1
- Change 2
- Change 3

## Testing

How have you tested this change? Please describe:

- [ ] I tested locally with `make dev`
- [ ] I ran `make lint` and all checks pass
- [ ] I manually verified locale key parity between `en.json` and `ar.json` (if modifying locales — no automated checker yet, see `docs/TRANSLATING.md`)
- [ ] I ran `qt-shell/tools/ui-shots.sh` with zero QML warnings (if UI changes)
- [ ] I tested both dark and light themes (if UI changes)

## Checklist

- [ ] `make lint` passes (cargo clippy + check-colors.sh)
- [ ] daemon, core, and qt-shell all pass `cargo clippy -- -D warnings`
- [ ] Locale key parity verified between `en.json` and `ar.json` (if you modified locale files)
- [ ] No hardcoded colors or emoji icons (uses `Theme` tokens and `Icons.qml` — see `AGENTS.md`'s Theme & Icon Rules)
- [ ] No hardcoded user-facing strings (all text uses `I18n.t()` in both `en.json` and `ar.json`)
- [ ] If I added new QML files: registered in `qt-shell/build.rs`
- [ ] If I modified daemon code: considered the constraints in [CLAUDE.md](https://github.com/UPdullah895/opengg/blob/main/CLAUDE.md) (PipeWire restart, subprocess cleanup, IPC validation)
- [ ] If I modified QML: followed the landmines in [AGENTS.md](https://github.com/UPdullah895/opengg/blob/main/AGENTS.md) (integer pixelSize, layout container traps, one-time QQC2 bindings, etc.)
- [ ] Screenshots included (if UI changes)

## Screenshots (if applicable)

If you've made UI changes, please include before/after screenshots.

## Additional Notes

Any other context that might be helpful for reviewers?

---

**By submitting this PR, I confirm that my contributions are licensed under the MIT License.**

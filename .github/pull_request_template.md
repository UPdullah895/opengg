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
- [ ] I ran `npm run check:locales` (if modifying locales)
- [ ] I ran `npm test` (if applicable)
- [ ] I tested both dark and light themes (if UI changes)

## Checklist

- [ ] `make lint` passes (cargo clippy + vue-tsc)
- [ ] Both daemon and Tauri crates pass `cargo clippy -- -D warnings`
- [ ] `npm run check:locales` passes (if you modified locale files)
- [ ] No hardcoded colors (uses CSS custom properties from `docs/DESIGN_TOKENS.md`)
- [ ] No hardcoded user-facing strings (all text uses i18n `t()` in both `en.json` and `ar.json`)
- [ ] All components use `<style scoped>` (no global CSS)
- [ ] If I added new Tauri commands:
  - [ ] Registered in `frontend/src-tauri/src/commands.rs` with `#[command]`
  - [ ] Listed in `frontend/src-tauri/src/main.rs` in the `invoke_handler!` macro
- [ ] If I modified daemon code: considered the constraints in [CLAUDE.md](https://github.com/UPdullah895/opengg/blob/main/CLAUDE.md) (PipeWire restart, subprocess cleanup, IPC validation)
- [ ] If I modified frontend state: followed the reactivity rules in [frontend/CLAUDE.md](https://github.com/UPdullah895/opengg/blob/main/frontend/CLAUDE.md)
- [ ] Screenshots included (if UI changes)

## Screenshots (if applicable)

If you've made UI changes, please include before/after screenshots.

## Additional Notes

Any other context that might be helpful for reviewers?

---

**By submitting this PR, I confirm that my contributions are licensed under the MIT License.**

# Frontend — Archived

**Status:** This Tauri + Vue UI is archived as of the `qt6-migration` branch and is no longer built or launched by default.

## Superseded By

The Qt6/QML interface in [`qt-shell/`](../qt-shell/) is the active UI. All future feature work, bug fixes, and maintenance happen there.

## Why Keep This?

This directory is preserved in the repository for two reasons:

1. **Historical reference** — The Vue components and page structures document the original design intent. The QML port's components cite their Vue counterparts as the source of truth for feature parity.

2. **Reversibility** — Keeping the full Tauri codebase in git history allows reverting the switchover if needed, without re-discovering lost code.

## Will This Compile?

Yes. The `make lint` and `make check` targets still validate the frontend crate to ensure the archived code keeps compiling. However:

- `./dev.sh ui` (and `make ui`) now launch the **Qt6** frontend only.
- The old `./dev.sh ui-legacy` command can be used to launch the Tauri frontend for debugging or historical comparison (see `dev.sh` and `Makefile` for details).

## Data Migration

UI settings from the Tauri app (`~/.config/opengg/ui-settings.json`) are automatically loaded by the Qt6 app on first launch. No manual export/import is needed.

# Third-Party Licenses — Qt6/QML UI Migration

Living inventory of **new runtime dependencies introduced by the Qt6/QML UI
migration** (plan §5.3). It is reviewed once before the Phase 0 gate closes and
updated in the same PR whenever a phase adds a runtime dependency.

Scope: this file tracks the *new* footprint the Qt UI adds. The existing daemon
(`openggd`) and the Tauri app carry their own dependency sets and are unchanged
by the migration until switchover.

## Hard rules (from plan §5.1)

- **Qt6 is linked dynamically in every build and package target** (LGPLv3 —
  users must be able to relink against a replaced Qt). Enforced by
  `qt-shell/scripts/check-qt-linkage.sh` in CI.
- **Codecs and hardware-decode plugins come from system packages, never
  vendored** into anything this project distributes. Missing hardware decode
  degrades to software decode; the project never ships its own codec binaries.
- **Distribution is deb/rpm, AUR, and Flatpak only** (no AppImage / no
  self-contained bundle — plan §5.2).

## Runtime dependencies

### Qt / C++ (dynamic system libraries)

| Component | Version floor | License | How consumed |
|---|---|---|---|
| Qt6 Core, Gui, Qml, Quick, QuickControls2, QuickLayouts, QuickShapes | 6.11 | LGPLv3 | Dynamic system libraries only (§5.1 hard rule; verified by the linkage check) |
| Qt6 Multimedia (+ `qt6-multimedia-gstreamer` backend) | 6.11 | LGPLv3 | Dynamic; video playback (Phase 2+). Not yet a `qt-shell` dependency. |
| Qt6 Wayland platform plugin | 6.11 | LGPLv3 | Dynamic; native Wayland. `xcb` plugin also required for the XWayland fallback (R2). |
| LayerShellQt | — | LGPLv3 | **Only if adopted** for the clip-saved overlay (R6). Adding it requires updating this row first. Not currently a dependency. |

### Rust crates (compiled in; permissive)

| Crate | Version | License | Notes |
|---|---|---|---|
| cxx | 1.x | MIT OR Apache-2.0 | Rust↔C++ FFI |
| cxx-qt | 0.9.x (pinned) | MIT OR Apache-2.0 | Rust QObject bridge to QML |
| cxx-qt-lib | 0.9.x | MIT OR Apache-2.0 | Qt type bindings (feature `full`) |
| cxx-qt-build | 0.9.x | MIT OR Apache-2.0 | Build-time (moc/qmldir/QML module gen) |
| serde_json | 1.x | MIT OR Apache-2.0 | Parses the JSON translation catalogs in `I18n` |
| gstreamer, gstreamer-audio (rs bindings) | 0.25.x | MIT OR Apache-2.0 | Rust bindings to the LGPL GStreamer C libs (consumed dynamically). Multi-track mixer pipeline (Phase 5). Not yet a `qt-shell` dependency. |

### GStreamer (dynamic system packages, declared as package dependencies)

| Component | License | How consumed |
|---|---|---|
| GStreamer core + gst-plugins-base + gst-plugins-good | LGPL | Declared package dependencies (deb/rpm/AUR); provided by the Flatpak runtime |
| Hardware-decode plugins (`nvcodec`, VAAPI — distro-packaged from the per-plugin-licensed `gst-plugins-bad` set) | LGPL (verify per plugin) | **Optional** system dependencies — never bundled |

## GPL watch (plan §5.3)

Any dependency carrying a **GPL** license must be a conscious, recorded
inclusion — an entry here with a one-line justification — never incidental or
transitive.

- Current status: **no GPL runtime dependency.** All crates above are
  MIT/Apache-2.0; Qt and the declared GStreamer plugin sets are LGPL.
- Concretely for GStreamer: the project's own declared package dependencies stay
  within **core + base + good** plus the LGPL hardware-decode plugins. The
  `gst-plugins-bad`/`gst-plugins-ugly` sets are licensed *per plugin* and some
  `-ugly` plugins are GPL; anything beyond the list above is **user-installed**,
  not a project dependency.

## Maintenance

- Update this file in the same PR that adds any new runtime dependency (review
  checklist item, alongside the §2.3 logic-boundary checks).
- Re-review at each phase gate that adds dependencies (notably Phase 2 video and
  Phase 5 mixer, which introduce the GStreamer footprint).

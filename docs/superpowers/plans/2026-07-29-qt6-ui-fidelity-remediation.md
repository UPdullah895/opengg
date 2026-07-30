# Qt6/QML UI Fidelity Remediation Plan

**Created:** 2026-07-29
**Branch:** `qt6-migration`
**Trigger:** Maintainer review — "the elements are not similar to the old ones,
even the icons are not the same as before."

The functional migration (Phases 0–5 of
`2026-07-20-qt6-qml-ui-migration.md`, plus the Guided Tour) is complete and
works. The **visual fidelity is not** — the assessment above is correct and
this plan treats it as a defect list, not a matter of taste.

---

## 0. Root-cause diagnosis (measured, not assumed)

Every number below was measured against the working tree at commit `8cf87be`.

| # | Defect | Evidence | Severity |
|---|---|---|---|
| D1 | **Design-token deficit.** `Theme.qml` exposes 7 tokens; the Vue UI consumes ~18. | `grep var(--*)` in `frontend/src` → 18 distinct tokens, 1 442 uses. `Theme.qml` → 7 `readonly property`. | **Root cause** |
| D2 | **Semantic colors missing → wrong hardcoded values.** `--danger`/`--success` are not exposed, so QML invented its own — and they don't match. | Real palette: `--danger: #dc2626`, `--success: #10b981`. QML uses `#ef4444` (×17) and `#22c55e` (×7) — Tailwind red-500/green-500, not this project's colors. | **High** |
| D3 | **84 hardcoded hex literals** in QML bypass the theme entirely. | `grep -oE '"#[0-9a-f]{6,8}"' qt-shell/qml/` → 84 hits across 23 distinct values. | **High** |
| D4 | **Accent tints frozen to the default accent** — they do not follow the user's chosen accent. This is a **functional bug**, not just fidelity. | 10+ sites use `Qt.rgba(0.914, 0.271, 0.376, α)` — literally `#E94560` baked in. Vue derives all 101 such tints live via `color-mix(in srgb, var(--accent) N%, transparent)`. Change your accent in Settings today and the sidebar highlight, mute pills, and settings nav stay pink. | **High** |
| D5 | **No icon system.** Vue ships 192 inline stroke SVGs; QML substituted ~35 emoji/Unicode glyphs. | `grep -c '<svg'` → 192 in Vue. QML: `🖱️ ⌨️ 🎧 🎮 📦 📁 🗑 ✂ ✎ ⚡ ↻ ↺ ▾ ▲ ▼ ▶ ⏸ 🛡️ ⚠️ ℹ️ 🔋 🎚 ✓ ✕ …`. Emoji render in the system emoji font — wrong weight, wrong color (many ignore `color:`), wrong metrics, and inconsistent across distros. | **High** |
| D6 | **Component structural gaps.** Ported components dropped real UI affordances. | `ClipCard.vue` vs the `ClipsPage.qml` delegate: missing film-strip placeholder, trimmed-badge, created-time badge, heart favorite (uses ★/☆ text), **selection overlay + checkbox (no multi-select at all)**, kebab context menu, inline rename (uses a modal instead), and the size/resolution/date meta pills. 9 gaps in the single most-viewed component. | **High** |
| D7 | **No visual verification loop.** Every feature this session was verified by build + clippy + logs; the UI was screenshotted ~twice all session. | `hyprctl dispatch` is broken in this environment, and the only display has been occupied by the maintainer's real foreground apps (browser, Discord, a fullscreen game). A design defect you cannot see is a design defect you cannot fix. | **Blocker** |

### Why this happened (so it doesn't recur)

D1 is upstream of D2, D3 and D4: with no `danger`/`success`/`bgDeep`/`textMuted`
token to reach for, every panel author (me) reached for a plausible hex literal
instead. D5 and D6 are a different failure: a repeatedly-applied "boring
primitive over fragile fidelity" instinct that was correct for *interaction*
mechanics (not rebuilding DropZone's 725-line pointer-drag system) but was wrong
to extend to *appearance*. An emoji is not a cheaper icon; it is a different
icon. That judgment call was mine and it was applied too broadly.

---

## Phase 0 — Make the UI visible (blocker; do first)

Nothing in Phases 1–4 can be honestly verified without this, and it is the
single highest-leverage item in the plan.

- Add a dev-only `--screenshot <path>[ --page <name>]` flag to `qt-shell`:
  render the window offscreen, `QQuickWindow::grabWindow()` → save PNG, exit.
  Runs under `QT_QPA_PLATFORM=offscreen`, so it **never touches the
  maintainer's desktop** — no focus steal, no window popping over a game.
- Add a sibling script that captures the same page from the **live Tauri app**
  for a side-by-side reference (or, simpler, capture reference PNGs once now
  and commit them under `docs/ui-reference/`).
- Deliverable: `make ui-shots` producing `qt-<page>.png` next to
  `vue-<page>.png` for all five pages plus each settings panel.

**Acceptance:** I can produce a before/after image pair for any page without
asking the maintainer to give up their screen.

### Phase 0 outcome (landed)

`make ui-shots` captures all 16 targets (5 pages + 11 settings panels + the
tour overlay) headlessly. Two implementation notes worth keeping:

- **`Window.contentItem` cannot be grabbed.** It is constructed by
  `QQuickWindow` in C++ and so has no associated `QQmlEngine`;
  `grabToImage()` refuses such items and returns `false` *silently* — no
  `qmlWarning`, which made this look like a platform/scene-graph problem for
  several iterations. `Main.qml` now has an explicit QML-declared
  `Item { id: captureRoot }` wrapping the whole UI. Anything added outside it
  will be invisible to screenshots.
- **`QT_QPA_PLATFORM=offscreen` works** once the above is fixed — the earlier
  "no scene graph" hypothesis was wrong. No Xvfb, no nested compositor, and no
  GPU are needed (this machine has none of the first two available anyway).

The first captures immediately confirmed two defects and found a third:

- **D5 is worse than "wrong style": the emoji do not render at all.** The mixer's
  mute buttons and several other controls draw as tofu boxes (`□`) because no
  installed font covers those codepoints. This is a broken UI, not a stylistic
  difference — it raises Phase 2 from "fidelity" to "correctness".
- D6 confirmed on the Clips grid: no favorite/selection/kebab affordances, and
  metadata renders as plain text rather than the Vue original's pills.
- **New (D8): the clips grid ignored the user's column setting.** Corrected from
  the first draft of this finding: `--clips-grid-cols` is vestigial in the *Vue*
  UI too (defined in `:root`, read by nothing), so the real source is
  `settings.clipsPerRow` (a 2–5 slider). `ClipsPage.qml` hardcoded
  `Math.floor(width / 260)`, which happened to equal the user's `clipsPerRow: 3`
  at 1280px — the defect was invisible precisely because it coincided.

- **New (D9), found while fixing D8 — the most consequential item here: Qt log
  output was never reaching stderr at all.** This Qt build has journald
  support, so `qWarning`/`console.log`/QML `TypeError`s go to the journal and
  stderr looks pristine. Every "verified: clean build, empty runtime log, zero
  QML warnings" claim made across this migration was therefore checking
  nothing. `QT_FORCE_STDERR_LOGGING=1` is required (`QT_LOGGING_TO_CONSOLE` is
  the deprecated spelling). Two live defects were hiding behind it:
  - `DevicesPage.qml:125` threw `TypeError: Cannot read property 'length' of
    undefined` **36 times per session**. `visible:` guarded the undefined
    access but the `text:` binding evaluates regardless of visibility.
  - `GraphicEQ.qml` declared `property bool enabled: false`, shadowing
    `Item.enabled` (`qt.qml.propertyCache: overrides a member of the base
    object`). Renamed to `eqEnabled`, matching `DspControls.qml`'s
    `nrEnabled`/`gateEnabled` convention.

  `ui-shots.sh` now sets that variable and greps each capture for
  TypeError/ReferenceError/binding-loop/shadowing, reporting `WARN` and exiting
  non-zero. A warning can no longer hide behind a successfully written PNG.

### Phase 1 outcome (landed)

`theme.rs` now exposes 19 tokens (was 7), resolved by a pure, unit-tested
`resolve(&Value) -> Resolved` function; 7 tests pin the dark and light palettes
to `App.vue`'s exact values so the two UIs cannot silently drift again, and
cover generic override, px/bare-number parsing, and non-positive/garbage
rejection. `Theme.qml` adds `accentAlpha(pct)` and `scrim(pct)`.

Light mode was verified end-to-end via an isolated `XDG_CONFIG_HOME` (never
touching the real `~/.config/opengg`) with a blue `--accent` override — the full
palette flips correctly. **That capture also proves D4 visually**: with a blue
accent, the active sidebar item and the Mixer tab pill still render *pink*,
because their tints are the hardcoded `Qt.rgba(0.914, 0.271, 0.376, α)`. Phase 3
fixes those.

---

## Phase 1 — Theme token parity (root-cause fix; unblocks 2–4)

Extend `qt-shell/src/theme.rs` + `qml/Theme.qml` to the full Vue token set,
using the exact values from `App.vue`'s `:root` / `html.light` blocks.

Add: `bgDeep`, `bgHover`, `bgInput`, `textMuted`, `danger`, `success`,
`purple`, `radiusLg`. Keep `bg`/`surface` as aliases of `--bg-surface`/
`--bg-card` but document the mapping in the header comment — the current names
silently invert what a reader familiar with the Vue CSS would expect.

Add an accent-tint helper so D4 becomes unrepresentable:

```qml
// Theme.qml — live accent tint, mirrors Vue's
// color-mix(in srgb, var(--accent) N%, transparent)
function accentAlpha(pct) { return Qt.alpha(Theme.accent, pct / 100) }
```

Both the dark and light palettes must be filled in — the Vue app has a real
light mode (`html.light`) and `ThemeController` already carries `darkMode`.

**Acceptance:** every token in `App.vue:618-652` has a `Theme.qml` counterpart
with a byte-identical default value; unit test in `theme.rs` asserts the
palette constants match the CSS.

---

## Phase 2 — Icon system

The correct pattern already exists in this codebase and is proven: `Sidebar.qml`
renders its five nav icons as `Shape { ShapePath { PathSvg { path: ... } } }`
with `strokeColor`, `capStyle: RoundCap`, `joinStyle: RoundJoin` — i.e. exactly
the feather/lucide stroke style the Vue SVGs use. Nothing new needs inventing;
it needs generalizing.

1. `components/Icon.qml` — props: `path` (or `name`), `size` (default 16),
   `color` (default `Theme.text`), `strokeWidth` (default 2), `filled` (bool).
   Integer `size` only (R1 in the migration plan: fractional `font.pixelSize`
   hangs the app — keep icon sizing integral for the same class of reason).
2. `Icons.qml` (`pragma Singleton`) — a name → SVG-path-data registry,
   extracted **mechanically** from the 192 Vue `<svg>` elements so the geometry
   is identical rather than redrawn by hand. A throwaway script over
   `frontend/src/**/*.vue` can emit most of this registry automatically; that
   script belongs in `scratchpad/`, not the repo.
3. Replace all ~35 emoji/glyph icon sites. Two legitimate exceptions to keep as
   text, because the Vue original also uses them as text: the `✓` in
   selection checkboxes and the `✕` in remove-buttons/close-buttons
   (`ClipCard.vue:91`, `StorageSettings.vue:156`, `MouseMacros.vue:211`).
   Everything else becomes an `Icon`.

**Acceptance:** zero emoji in `qt-shell/qml/` outside the two documented
exceptions; every icon's stroke path is traceable to a specific Vue `<svg>`.

### Phase 2 outcome (landed)

`Icons.qml` (31 entries) + `components/Icon.qml`, generalising the
Shape/ShapePath/PathSvg pattern `Sidebar.qml` already proved. 29 emoji sites
were converted programmatically (marker pass, then a brace-matching retype from
`Text` to `Icon` that also strips the now-invalid `font.*` properties) so the
edits stayed uniform rather than 29 hand-edits.

Geometry was extracted mechanically from 193 inline `<svg>`s **plus the 16
named `ICON_*` constants in `frontend/src/assets/deviceAssets.ts`** — a
canonically-named set the initial `*.vue`-only glob had missed entirely.

Three traps worth recording, all of which fail *silently*:

- **`PathSvg` cannot parse SVG's compact arc-flag shorthand.** In
  `a9 9 0 0118 0` the two single-digit flags and the following x run together;
  PathSvg reads `0118` as one number and drops the remainder of the subpath with
  no warning. The mute icon's wave arc simply vanished. A normalizer rewrote all
  31 paths into fully space-separated arc form (flags parsed as single chars,
  never as numbers).
- **SVG attribute names contain digits.** The first extractor used
  `[a-zA-Z-]+` for attribute names, so `x1`/`y1`/`x2`/`y2` never matched and
  every `<line>` collapsed to `M 0 0 L 0 0`.
- **Two icons were misidentified by proximity search.** The path grepped as
  "shield" was actually a duplicate headphones glyph — the Vue UI has no shield
  anywhere, and its Ear Blast header is a bare `<span>` + InfoIcon. Dropped the
  entry, removed the invented 🛡️, and added the real `check-square` that
  `MixerRoutingSettings.vue` uses. Likewise 🎚 for chatmix was invented
  (`DeviceCard.vue` renders it as plain text) and ★/☆ for favourites was wrong
  (`ClipCard.vue` uses a heart that fills).

Because geometry loss is invisible, `--page icons` renders a contact sheet of
every registry entry at 16/24/32px; regenerate it after touching `Icons.qml`.
Kept as text, matching the Vue original: the `✓` in dep-check rows and `•`
bullets in About.

---

## Phase 3 — Color sweep

Mechanical, but do it *after* Phase 1 so there is somewhere correct to sweep to.

- Replace all 84 hex literals with tokens. Note the ones that are semantically
  wrong today, not just untokenized: `#ef4444`→`Theme.danger` (`#dc2626`),
  `#22c55e`/`#10b981`→`Theme.success` (`#10b981`), `#f59e0b` (amber, used for
  the overdrive >100% state — Vue's equivalent needs checking; it may want
  `--purple` or a dedicated token rather than a new invented amber).
- Replace all `Qt.rgba(0.914, 0.271, 0.376, α)` with
  `Theme.accentAlpha(α * 100)`.
- Legitimate literals to keep: pure `#ffffff` on accent-filled buttons (Vue does
  the same), `#000000` video letterboxing, and the `#cc0b0d13`/`#9e000000`
  modal scrims — but route those through a `Theme.scrim` token so the light
  theme can differ.
- Add a CI grep guard (the migration plan already established this pattern for
  R1's fractional-pixelSize lint) failing the build on a new bare hex in
  `qml/` outside an allowlist.

**Acceptance:** `grep -oE '"#[0-9a-fA-F]{6,8}"' qt-shell/qml/` returns only
allowlisted literals; changing the accent in Settings visibly retints every
highlight, verified by a Phase-0 screenshot pair.

---

## Phase 4 — Per-component fidelity pass

Ordered by visibility × measured gap. Each component gets: read the Vue source
→ enumerate concrete visual/affordance deltas → rebuild → Phase-0 screenshot
diff against the reference.

| Order | Component | Known gaps |
|---|---|---|
| 1 | `ClipsPage` delegate → extract a real `ClipCard.qml` | The 9 gaps in D6. **Multi-select is entirely absent** — `replay.ts` has `selectMode`/`toggleSelect` and a bulk toolbar; this is closer to a missing feature than a styling gap and may deserve its own task. |
| 2 | `ChannelStrip` (inline in `MixerPage.qml`) → extract component | Compare against `ChannelStrip.vue` (5 SVGs); check fader track/thumb geometry, mute/solo affordances, VU meter styling. |
| 3 | `DevicesPage` → extract `DeviceCard.qml` | vs `DeviceCard.vue` (4 SVGs); currently `🔋`/`🎚` emoji for battery/chatmix. |
| 4 | `Titlebar.qml` | vs `Titlebar.vue` (4 SVGs); currently `−`/`✕` text glyphs; check the missing maximize/restore control. |
| 5 | `HomePage.qml` | vs `HomePage.vue` (13 SVGs) — the largest icon deficit outside ClipsPage. |
| 6 | Settings panels | `CaptureSoundPanel` (591 ln), `AboutPanel` (566), `ExtensionsPanel` (406), `MixerRoutingPanel` (384) — audit each against its `*Settings.vue` counterpart. |
| 7 | `TourOverlay.qml` | Card/dot/CTA styling vs `GuidedTour.vue`; written before Phase 1 tokens existed, so it is a known offender. |

Deliberately **out of scope** (unchanged from the original plan's judgment,
which was about interaction mechanics and still stands): rebuilding
`DropZone.vue`'s custom pointer-drag-with-ghost-element routing, and the
multi-track `AdvancedEditor.vue`. Those remain click-to-route and trim-only.
This plan is about making what exists *look* right, not re-litigating scope.

---

## Sequencing & risk

Phases 1→2→3 are strictly ordered (each unblocks the next) and are largely
mechanical, low-risk, and independently committable. Phase 4 is the long tail
and is per-component parallelizable once 1–3 land. **Phase 0 gates honest
verification of everything else** and should land before any pixel is changed —
otherwise this plan repeats the exact mistake that produced the defect list.

Nothing here touches `opengg-core`, the daemon, or any Rust logic beyond
`theme.rs`, so the functional migration cannot regress: every acceptance check
is a screenshot diff plus the existing `cargo build`/`clippy`/49-test suite.

---

## Immediate next step

Phase 0, then Phase 1 — in that order. Phase 1 without Phase 0 means changing
colors I still cannot see.

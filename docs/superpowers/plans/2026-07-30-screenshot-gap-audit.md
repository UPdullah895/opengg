# Screenshot Gap Audit — QML shell vs. the Tauri app

**Created:** 2026-07-30
**Method:** four maintainer-supplied screenshots of the shipping Tauri app
(Dashboard, Audio Mixer, Clips, Settings→General), compared against
`make ui-shots` captures and read back against the Vue sources.

## Why this document exists

The UI-fidelity plan (`2026-07-29-…`) fixed the *primitives*: design tokens,
the icon system, colour derivation. After Phase 4 part 1 I reported the original
complaint as "substantially addressed". **That was wrong**, and the maintainer
was right to push back. Tokens and icons were necessary but they are the
smallest part of the gap: two whole pages are *structurally different* from the
original — not styled differently, built differently — and several features
don't exist at all.

The root error was method, not effort: I audited by grepping my own code for
things I knew were wrong (emoji, hex literals), rather than by comparing
against the real app screen by screen. A grep can only find defects you already
suspect. **This audit is screen-by-screen against the actual UI.**

---

## G1 — HomePage is not a port (severity: highest)

The Vue "Dashboard" is a **4-card stat grid with click-to-expand popovers**,
plus a changelog feed:

- Title `dashboard.title` = "Dashboard" (not "Home"), with a review-tutorial
  info button beside it.
- Four cards — AUDIO MIXER / RECORDER / CLIPS / DEVICES — each with a label, a
  coloured icon badge top-right (accent / red / green / purple), a large value
  ("5 Channels", "Active", "80", "Scan") and a sub-line
  ("Game · Chat · Media · Aux · Mic", "Replay active", "Video clips saved",
  "Mouse · Keyboard · RGB").
- Clicking a card opens an inline **popover**: the mixer card shows quick
  per-channel strips with mute buttons and an "Open Mixer" button; the recorder
  card shows recording settings rows plus a gear shortcut to Capture & Sound.
- A dismissible GSR warning banner when the recorder is missing.
- **"Latest Engine Updates"**: a changelog card (version, date, bulleted
  changes) driven by `tm('dashboard.changelog')`, with a "12 more updates"
  expander.

The QML page shares none of this structure. It shows a "Replay Buffer" card
with a Start button and a list of channel sliders — UI I invented rather than
ported. This needs a rewrite, not a restyle.

## G2 — Mixer is missing the Master channel and most of each strip

`MIXER_CHANNELS = ['Master','Game','Chat','Media','Aux','Mic']` — **six**
strips. The QML page renders five; **Master is absent entirely.**

Per strip, the original has, top to bottom: a coloured top border bar in the
channel's identity colour, a circular icon badge (volume / gamepad /
headphones / play / music / mic), the UPPERCASE name, a tick-marked fader
scale, a wide rounded fader thumb, the percentage in large type with a **dB
readout** beneath, a mute button, and a **device-selector dropdown** showing
the bound output ("MZ-630 USB Speaker B…"). Below the strips sits a per-channel
**app list box** ("No apps" / "speech-dis…"), and the tab bar lives top-right
with three trailing icon buttons (overdrive, ear-blast, settings).

The QML strips have the name, percentage, fader and mute — and nothing else.
App routing is a separate card of pills rather than per-strip boxes.

## G3 — Clips: missing right-click menu, list/grouped views, and the toolbar

- **Right-click context menu**: `ClipCard.vue` has
  `@contextmenu.prevent="openMenu"`, and `ClipsPage.vue` drives a singleton
  menu via `replay.activeMenuClipId`. Not implemented in QML at all.
- **View modes**: the original has grid, **list** (`ClipListRow.vue`) and
  **date-grouped** views, plus a grid-size slider (2–5 columns) in the toolbar.
  QML has only the grid, and no size control.
- **Toolbar**: source filter ("Replay ▾"), search field with a magnifier icon,
  sort ("Newest"), game filter ("Games"), a favourites count pill ("♥ 3") and
  four view-mode buttons. QML has search, game filter, sort — plus a "Refresh"
  button the original doesn't have.
- **Multi-select** (already tracked, task #32): selection overlay + checkbox.

## G4 — Fixed this pass

| Defect | Cause |
|---|---|
| Clip card's bottom border/corners shaved off | Arithmetic, not styling: `cellHeight` (+62) was ~6px under the card's real content height, so `clip: true` cut it. Now +80. |
| `142 MB` instead of `142.4 MB` | `fmtSize` didn't port the unit ladder + 1 decimal. |
| ISO `2026-07-20` instead of `Jul 20, 2026` | `fmtDate` wasn't ported at all. |
| 24h `09:45` instead of `9:45 AM` | `fmtTime` wasn't ported at all. |
| Duration showed an hours field | `fmtDur` in the original is `m:ss` only. |
| No maximize/restore button | `Titlebar.vue` has `toggleMaximize`; the port shipped minimize + close. |
| No bottom tip strip in the sidebar | `Sidebar.vue:34-41` never ported. |
| Headings read "Home"/"Mixer" | The Vue pages are titled "Dashboard"/"Audio Mixer". |
| No "Beta" pill on Extensions in the settings nav | Never ported. |

## G5 — Responsive sidebar collapse (net-new, not a regression)

The maintainer asked for the sidebar to shrink when the window is narrow,
noting it stays open under Hyprland. Worth being precise: **this does not exist
in the Tauri app either.** `Sidebar.vue` is a fixed
`width/min-width: var(--sidebar-w)` with no `@media`, no `matchMedia`, and no
`ResizeObserver` anywhere in `Sidebar.vue` or `App.vue`. So this is a new
feature request rather than a port gap — it should be built deliberately
(icon-rail collapse below a width threshold, with the label column hidden),
not "restored".

## G6 — Smaller deltas still open

- Settings: section heading needs the horizontal rule under it; the
  Diagnostics button is an *outlined* accent button in the original, not filled.
- Sidebar icon shapes: the maintainer reports differences; the nav paths in
  `Sidebar.qml` predate the `Icons.qml` registry and were hand-entered, so they
  should be re-extracted from `Sidebar.vue` and moved into the registry.
- Titlebar: the original's language control and overall right-cluster spacing
  differ from the port's.

---

## Ordering

1. **G1 HomePage rewrite** — the landing screen, and the largest structural gap.
2. **G2 Mixer strips + Master** — extract `ChannelStrip.qml` properly.
3. **G3 Clips** — context menu, then list/grouped views, then the toolbar.
4. **G6** — small, cheap, do alongside.
5. **G5** — new feature; schedule separately, after parity.

Verification for every item is a `make ui-shots` capture compared against the
corresponding maintainer screenshot — not a clean build.

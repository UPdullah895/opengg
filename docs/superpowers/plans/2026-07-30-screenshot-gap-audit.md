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

---

# Round 2 — Devices + all nine settings panels

Second batch of maintainer screenshots (Devices, Language, Shortcuts, Audio
Engine, Capture & Sound, Timeline Tracks, Storage, Notifications, Extensions,
About). Audited the same way: screen against screen, then read back against the
Vue source.

## G7 — Devices: the shipping app is a "Coming Soon" placeholder ⚠ DECISION NEEDED

`DevicesPage.vue` renders a **hardcoded placeholder**: a large stroke-1.2
headphones glyph, `devices.comingSoon` ("Devices — Coming Soon") and
`devices.comingSoonDesc`, above an optional device-access banner. There is no
device list.

The QML page renders a **live device list** from the daemon (name, type, model,
battery, chatmix, capability count). So the port is *ahead* of the original
here, and "matching the screenshot" would mean deleting working UI.

**This is a product call, not a defect** — flagged rather than actioned. Options:
(a) keep the live list (QML leads), (b) match the placeholder exactly, (c) keep
the list behind the existing `modules.deviceManager` toggle and show the
placeholder when it is off. (c) preserves both and is probably what the
placeholder was standing in for.

## G8 — Settings panels: per-panel deltas

Fixed this pass (applies to all eleven panels):

- **Heading rule.** Every Vue settings page renders its title above a
  full-width divider; the port had a bare `Text` in each panel. Added
  `components/SettingsHeading.qml` and converted all 12 panels.
- **Ear Blast channel pills** were filled accent chips; the original is an
  accent *outline* with accent text. Also uppercased the sub-labels
  ("PROTECTED CHANNELS", "TRIGGER THRESHOLD", "TARGET VOLUME").

Still open, by panel:

| Panel | Delta |
|---|---|
| **Audio Engine** | Card order is inverted — Danger Zone is **first** in the original. Danger Zone has no descriptive paragraph there (the port invented one) and puts its two actions as label + right-aligned icon button rows. Ear Blast title is preceded by a headphones icon. |
| **Language** | Rows are full-width cards: accent 2-letter code, then the name, then an `LTR`/`RTL` badge right-aligned; selected row gets an accent border + tint. Two icon buttons (open-folder, refresh) sit top-right of the card. |
| **Shortcuts** | Needs a right-aligned **keycap** control per row (bordered, monospace, e.g. `Alt+F10`, `Ctrl+Shift+Z`, `—` when unset), divider lines between rows, and a "Reset to Defaults" button top-right that disables when already default. |
| **Capture & Sound** | 4-column control grid (QUALITY / FPS / REPLAY BUFFER / MONITOR TARGET) with a bitrate spinbox beneath Quality; `Est. RAM … | File: …` top-right; "Run Diagnostics" as an outlined button with a check-circle icon. Audio Capture Devices rows need a drag handle, `TRACK n` label, device dropdown, × remove, and an accent "+ Add Track". |
| **Timeline Tracks** | Rows need eye toggle + colour swatch + name field + icon-type button + × remove; an **icon-picker popup** (3×2 grid, active highlighted); and a **"Live Preview"** card rendering each track as a coloured bar with a coloured left edge and icon + name. |
| **Storage** | Uppercase group labels; folder-icon rows with × ; outlined "+ Add Clip Path"/"+ Add Directory" buttons; a **Disk Usage** card with two accent stat boxes (CLIPS / USED) and a trash action; a **Steam Library** card ("connected — N games", refresh button, scrollable list with per-game icon thumbnails). |
| **Notifications** | Style picker is **five large icon+label option buttons in a row** (globe / monitor / monitor / bell / ban), selected = accent border + accent text + tint. The port uses small rows. Position is a separate card. |
| **Extensions** | **Modules** card (3 toggle rows with bold name + description). GSR card with an accent "▶ How to install?" expander. Extensions list rows need a rounded coloured icon tile, name + dim version, description, and a right-aligned toggle. Footer note "Changes apply immediately — no restart required." |
| **About** | Hero card: accent logo, "OpenGG", a `v0.1.5` accent-tint pill, bold tagline, dim description. "Project Goals" rows prefixed with an accent ▶. "Connect With Us" full-width bordered GitHub/Discord rows. Dependency/device-access cards use a green ✓ badge + monospace text. |

## Method note

Round 1 found 6 defect classes from my own greps. These two rounds of
screenshots found ~20 more, several structural. The lesson is already recorded
in memory, but restating it here because it governs the remaining work:
**parity claims require screen-by-screen comparison against reference images,
not code inspection.** Every remaining task's acceptance criterion is a
`make ui-shots` capture placed next to the corresponding reference screenshot.

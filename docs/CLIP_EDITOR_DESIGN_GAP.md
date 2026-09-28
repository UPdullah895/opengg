# Clip editor: design gap against the Vue original

> **Historical note:** `frontend/` (the Tauri + Vue UI referenced throughout
> this document) was removed from the repository after this comparison was
> written. The file paths below no longer exist; this document is kept as a
> record of the design decisions made during the QML port.

A comparison of `frontend/src/components/AdvancedEditor.vue` (the shipping Vue
editor) against `qt-shell/qml/pages/ClipEditorPage.qml` (the Qt/QML port), written
after the port's first round of user feedback.

The port reproduced the **layout** faithfully — same regions, same information,
same ordering. What it did not reproduce is the **interaction model**. The old
editor treats the timeline as a workspace you manipulate; the new one treats it
as a diagram that reports state. That single difference accounts for most of
"the old design is still far more advanced."

---

## 1. The timeline is illustrative, not instrumental

| | Vue | QML port |
|---|---|---|
| Audio lanes | Real waveforms — `generate_waveform` peaks drawn to a `<canvas>` with a vertical gradient, desaturated to grey when muted | Flat tinted bars |
| Playhead | Draggable (`phDown` → scrub) with a visible grip | Static line; seek by clicking the lane |
| Track icons | Per-type glyph — video / game / mic / chat / media / overlay | One generic note for every audio track |
| Per-track level | Volume slider per track (`volOpen`, `volume: number`) | Mute only |

The waveform is the biggest single loss. It is not decoration: it is how you
find the moment worth keeping without scrubbing blindly. A flat bar tells you a
track exists; a waveform tells you where the shot, the callout or the silence is.
`core::media::generate_waveform` already exists and already returns peaks — the
data is sitting there unused.

**Recommendation, in priority order:** waveforms → draggable playhead → per-track
volume → per-type icons.

---

## 2. Video and audio trim are one control instead of two

The Vue editor keeps **two independent trim ranges** — `trimS`/`trimE` for video
and `audioTrimS`/`audioTrimE` for audio — with a "unified" mode that links them
and a split mode that doesn't. That is four draggable handles, and the audio pair
is clamped inside the video pair.

The port has one pair, applied to everything.

This is almost certainly what "I cannot move the audio track" meant. It reads as
a broken control, but the control was never ported: there is nothing to grab on an
audio lane because audio has no independent range in the new editor.

Being able to offset audio against video is the difference between trimming a clip
and editing one — it is what lets you keep the reaction a half-second after the
play, or cut the picture while the commentary runs on.

---

## 3. There is no history

Vue keeps `undoStack`/`redoStack` of `{s, e, as, ae}` and pushes onto it before
every drag. The port offers a single "reset trim" that throws the whole edit away.

Trimming is iterative — you overshoot, you step back. Without undo, every mistake
costs the entire adjustment, which quietly discourages fine work. The state to
snapshot is four floats; this is cheap to add and disproportionately improves how
the editor *feels*.

---

## 4. The layout is fixed where it used to yield

Vue has a horizontal splitter between preview and timeline (`splitterDown`) and a
draggable info-sidebar edge (`sideResizeDown`). Both persist.

The port hardcodes both. The new full-view toggle helps, but it is a switch
between two states, not a continuum. A six-track clip needs timeline height; a
framing check needs preview height. Only the user knows which, and only in the
moment.

---

## 5. Track names ignore the user's own configuration

Vue resolves a track's label through a deliberate three-step fallback:

1. the **embedded** title, baked in at record time from the actual capture source
2. `trackDefs` — Settings → Timeline Tracks
3. the `captureTracks` override
4. `Audio N`

Step 1 first is a considered choice, and the comment says why: it is correct
across machines and for old recordings, where the positional `trackDefs` would
mismatch.

The port implements step 1 and step 4 only. That is why a clip recorded with
generic device names shows `Device…` in the lanes, while the old editor shows
`noon 1` / `why 2` / `test 3` — the names the user actually set. The port is
reading the file and ignoring the person.

---

## 6. The export has no destination

Vue's export completion turns into a **draggable file row** — you drag the result
straight into Discord, a browser upload, anywhere that takes a file drop
(`startFileDrag`, plus a "Drag to share" toast).

The port's dialog ends at "here is the path". Everything up to the file existing
is now equivalent; the last step — getting it somewhere — isn't there. For a
clipping tool that step is arguably the point of the whole feature.

---

## What the port does better

Worth stating, because the gap list is one-sided:

- **The export dialog is clearer.** Options are grouped, the codec choice is
  disclosed rather than always-on, and the summary line states plainly what will
  be produced. The Vue version buries the same options in a denser modal.
- **The info panel is calmer.** Consistent label/value rhythm, a keyboard legend
  that is actually visible instead of implied.
- **Audio genuinely mixes.** The Vue editor drives one `<video>` element; the QML
  editor runs a real GStreamer graph with a gain element per track. Simultaneous
  multi-track monitoring is a capability the original never had.
- **It is a page, not a modal.** No backdrop, no escape-to-lose-your-work.

---

## Reading of the gap

The port was built by describing the old editor and rebuilding from the
description. That reproduces what a screenshot shows — regions, labels, ordering —
and silently drops what a screenshot cannot: that the playhead is grabbable, that
there are two trim ranges rather than one, that a drag can be undone, that the
panes yield, that the file at the end is meant to be picked up and thrown
somewhere.

Every one of those is a verb. The port has the nouns.

The fix is not a redesign. The layout is right and in several places better. What
is missing is the manipulability underneath it, and it can be restored
incrementally in the order given above — waveforms and split audio trim first,
since those two carry most of the perceived difference.

# AI Contributor Changelog

This is the **append-only session log** for all AI agents working on OpenGG. Every agent must update this after completing work.

**Format**: Newest entry first. See the template below.

---

### [2026-08-30] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Devices roadmap Phase 2: DPI/polling-rate controls UI)

**What Changed:**
- `daemon/src/device/ratbag.rs` — `build_device_info` now populates
  `capabilities` for mice too (`"dpi"` / `"polling_rate"`), derived from
  whether `dpi_options`/`polling_rate_options` are actually non-empty —
  never from `DeviceType`. This is the capability-gating data source the
  roadmap's §2 rule requires; a mouse ratbagd can't currently read
  resolution data for correctly advertises zero capabilities rather than a
  hardcoded "it's a mouse, so it must have DPI" assumption.
- `qt-shell/src/device.rs` — `DeviceController` gained two invokables,
  `setDpi`/`setPollingRate`, following the exact background-thread +
  qt_thread.queue pattern `AudioController::set_mute` already uses:
  background thread does the blocking D-Bus round trip, refreshes
  afterward *unconditionally* (success or failure) so the UI never keeps
  showing a value the write didn't actually achieve. Added `lastError`/
  `lastErrorDeviceId` qproperties — set on a failed write, cleared on the
  next successful one, keyed by device id so two cards mid-write don't
  show each other's error. This is a first for the device/audio
  controllers: no existing write path (`setVolume`, `setMute`,
  `routeApp`, …) surfaces failures to the UI today, they only
  `eprintln!` — Phase 2's explicit requirement to not silently no-op on
  a failed write is genuinely new territory for this codebase's
  controller pattern, not a port of an existing convention.
- `qt-shell/qml/pages/DevicesPage.qml` — added a DPI dropdown and a
  Polling Rate dropdown to the mouse card, gated by
  `hasCapability(modelData, "dpi"/"polling_rate")`, not
  `deviceType === "mouse"`. Both reuse the existing `SelectField`
  component (no new custom control), built from the device's own
  `dpiOptions`/`pollingRateOptions` arrays — never a hardcoded numeric
  range, since a device's real DPI stages are non-linear (confirmed: this
  G502 jumps from 100-step increments below 2600 to 200+ step increments
  above it). Added a per-card error `Text` reading
  `DeviceController.lastError`, shown only when
  `lastErrorDeviceId === modelData.id`.

**Why:** Phase 2 of the Devices Page roadmap — the daemon's write surface
(`SetDpi`/`SetPollingRate`, fixed working in the previous two entries) had
no UI in front of it at all.

**Landmines:**
- Verification gap, stated plainly: there is no tool available in this
  session to drive a native Qt window's mouse/keyboard (the offscreen
  `ui-shots.sh` renderer takes a static screenshot only, it doesn't
  simulate clicks). The actual QML→Rust dropdown-selection→D-Bus-write
  round trip was **not** interactively click-tested end-to-end. What
  *was* verified: (1) the screenshot shows the dropdowns correctly
  reflecting the real device's live 800 DPI / 1000 Hz — proving the read/
  display binding is correct; (2) the exact D-Bus calls
  `setDpi`/`setPollingRate` invoke (`opengg_core::device::set_mouse_dpi`/
  `set_mouse_polling_rate` → `SetDpi`/`SetPollingRate`) were already
  proven to work live against this same hardware via `busctl` in the
  previous entry; (3) the Rust invokable code compiles and follows the
  same background-thread pattern already working for `setVolume`/
  `setMute`. Actually clicking through the dropdown in a running app is
  a reasonable follow-up check for whoever picks up Phase 3/4, or for the
  user to do directly (`./dev.sh ui`).
- The first screenshot attempt (before rebuilding+reinstalling the
  daemon with this session's capability-population change) showed the
  G502 card with no controls at all — not a QML bug, just a stale
  installed daemon binary from an earlier point in this same session
  (AGENTS.md's "Daemon Binary Can Be Stale" landmine, hit for real here).
  Re-verify the installed daemon's mtime against the daemon source
  whenever a screenshot looks like it's missing daemon-sourced data.
- No manual "merge with…"/"not the same device" UI affordance was added
  in Phase 1 or here — still just the D-Bus methods + override file,
  verified via `busctl` only (see that entry's landmine note). Phase 2's
  card now has real controls on it, so this would be a more natural time
  to add that affordance than when the card had nothing else on it — left
  for whoever picks up Phase 3/4 unless the user wants it sooner.

**Verification:** `cargo clippy --all-targets -- -D warnings` → 0
warnings (daemon: 27/27 tests incl. 2 new capability tests; qt-shell:
22/22 tests, all pre-existing/unrelated). `qt-shell/tools/check-colors.sh`
→ clean. `qt-shell/tools/ui-shots.sh` with `QT_FORCE_STDERR_LOGGING=1` →
"no QML warnings" across all 16 pages, twice (once before, once after
catching the stale-daemon issue above). Visually inspected
`devices.png`: DPI/Polling Rate dropdowns render correctly showing
"800 DPI"/"1000 Hz", matching the real connected G502; the headset card
correctly shows neither control.

---

### [2026-08-30] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Fix SetDpi/SetPollingRate: three compounding ratbagd D-Bus bugs)

**What Changed:** `daemon/src/device/ratbag.rs` — `SetDpi`/`SetPollingRate`
were flagged as broken while verifying Phase 1 (previous entry). Fixing them
required finding and fixing **three separate, pre-existing bugs**, each
hiding the next one until the previous was fixed. All confirmed against real
live hardware (this machine's Logitech G502) via `busctl`, not just unit
tests.

1. **`Resolution`'s wrong wire shape.** ratbagd declares this property
   `:type: v` — its value is itself a nested variant (`u` or `(uu)`
   depending on device/profile), confirmed live: `Properties.Get` returns
   `v v u 400` (two levels). The old code's `Value::from(dpi)` only wrapped
   once, which ratbagd rejected outright. Fixed via `extract_dpi_x`/
   `build_resolution_value`, which read the current shape first and
   preserve it — libratbag explicitly documents that changing shape
   mid-flight is invalid.
2. **`ReportRate`/`ReportRates` queried on the wrong D-Bus interface.**
   libratbag's own dbus.rst places both on `Profile`, not `Resolution` — the
   old code queried them from the active `Resolution` object, which simply
   doesn't have them (confirmed via `GetAll` on a live Resolution object:
   no such keys). This meant `SetPollingRate`/read-side polling rate never
   worked for *any* device, on *any* machine, regardless of connection
   mode — not something specific to this session's changes. Read now comes
   from the active `Profile`; `DeviceInfo` gained `polling_rate_options`
   (from `Profile.ReportRates`) alongside the pre-existing `dpi_options`.
3. **`Device.Commit()`'s actual return type.** libratbag's dbus.rst says
   `Commit() → ()`, but this installed ratbagd (0.18-1) really replies with
   a `u` — confirmed via `busctl introspect`. The old `fn commit(&self) ->
   zbus::Result<()>` made zbus fail deserializing every *successful* Commit
   reply with "Signature mismatch: got `u`, expected ``", which is the
   error that first surfaced once bug #1 was fixed and a write finally got
   far enough to call it. Return type corrected to `u32`, logged at debug
   level and otherwise ignored (docs: this call "always succeeds", real
   errors surface via the separate `Resync` signal).
4. **Read/write link-selection drift** (found while manually verifying the
   above): this exact G502 has **both** its wired and wireless links
   simultaneously reporting a real active profile/resolution — not one
   live + one phantom. `build_device_info` (what a merged card displays)
   and `resolve_sysname_for_id` (what a write targets) each had their own
   copy of "prefer the live link" sorting logic, and the two orderings
   disagreed on which link was primary when *both* were live. Confirmed
   live: `SetPollingRate` wrote 250 to the `c08d` link but the merged
   card kept showing `407f`'s untouched 1000. Fixed by extracting one
   `select_primary` function both call — same selection by construction,
   not by hoping two implementations stay in sync.

**Why:** blocks Phase 2 (device controls UI) entirely — there's no point
wiring a DPI slider to a D-Bus call that can never succeed, or that could
succeed while silently displaying a different link's stale value back.

**Landmines:**
- `Device.Commit()`'s `u` return value is logged but not otherwise
  interpreted — if it turns out to matter (e.g. non-zero means "queued,
  not yet applied"), that's still unhandled. The `Resync` D-Bus signal
  (fired async on a real failure) also isn't listened for anywhere in this
  codebase — a failed write currently looks identical to a successful one
  from the daemon's perspective, one commit each way.
- The `Resolution`/`Commit` type mismatches came straight from libratbag's
  own published dbus.rst not matching this installed ratbagd version's
  real behavior (0.18-1) — if that package updates, these assumptions
  should be re-verified against `busctl introspect` again rather than
  trusted from docs alone.
- `select_primary`'s tie-break (lowest `(vid, pid)` wins when multiple
  links are live) is arbitrary — there's no "more correct" link when both
  are equally live. A user changing DPI while both the wireless and wired
  links are simultaneously connected will always land on the same
  deterministic one, which is at least *consistent*, not necessarily the
  one they think they're touching.

**Verification:** `cargo clippy --all-targets -- -D warnings` → 0
warnings; `cargo test` → 26/26 pass (5 new: DPI shape read/write
round-trip, live-both-links selection agreement). Rebuilt release,
reinstalled to `~/.local/bin/openggd`, restarted the live
`openggd.service`, and verified against this machine's real G502 via
`busctl`: `SetDpi`/`SetPollingRate` both now return success *and* the
value actually changes on the device *and* reads back correctly and
consistently afterward (tested with distinct values — 1600/500 — to rule
out a no-op false positive, not just round-tripping the pre-existing
value). Mouse restored to its original 800 DPI / 1000 Hz afterward.

---

### [2026-08-30] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Devices roadmap Phase 1: cross-transport mouse identity merge)

**What Changed:**
- `daemon/src/device/ratbag.rs` — mouse identity is now cross-transport-aware.
  ratbagd's D-Bus API exposes no per-unit serial (`Model` is identical for
  every unit of the same mouse model), and a **live check on this machine**
  confirms a real, present bug: a Logitech G502 LIGHTSPEED reports as two
  entirely separate ratbagd device objects — `usb:046d:c08d:0` ("Logitech
  G502 LIGHTSPEED Wireless Gaming Mouse") over its receiver and
  `usb:046d:407f:0` ("Logitech G502") when wired — both present in ratbagd's
  device list *simultaneously*, with the prior vid:pid-only identity fix
  (previous entry, same file) correctly not merging them since the two ids
  are genuinely different vid:pid pairs.
  - Added a conservative name-similarity heuristic (`merge_key_slug`): same
    vendor ID + matching device name after stripping known connection-mode/
    marketing tokens (wireless, lightspeed, bluetooth, receiver, gaming,
    mouse, …). Deliberately excludes tokens that could denote a genuinely
    different hardware revision (hero, se, plus, …) and refuses to merge on
    a name with no digit-bearing "model designator" token left after
    stripping — merging two different physical mice is a worse failure mode
    than showing a duplicate card, so the heuristic is biased toward false
    negatives, not false positives.
  - Merged devices get a new id form: `ratbag:merged:{vid:pid}+{vid:pid}…`.
    `DeviceInfo` gained `linked_ids: Option<Vec<String>>` (new field, also
    threaded through `headset.rs`'s constructor as `None`) listing every
    member link. Field values (name/model/dpi/polling_rate) are taken from
    whichever member link is currently "live" (reports real resolution
    data), so a present-but-unresponsive link never shadows real data.
  - Added `daemon/src/device/identity_overrides.rs`: a small
    `~/.config/opengg/device-identity-overrides.json` store recording
    explicit user `force_merge`/`force_split` pairs, always re-read from
    disk (no in-memory caching — same freshness-over-caching choice as
    `resolve_sysname_for_id`). These always override the heuristic for that
    exact pair, and setting one clears any conflicting entry in the other
    list.
  - `resolve_sysname_for_id` (renamed from `resolve_sysname`) now parses
    both legacy single-link ids and merged ids, and re-scans ratbagd's live
    device list on every write to pick whichever member link is actually
    responding — never caches a sysname across calls.
  - New `RatbagManager::merge_devices(id_a, id_b)` /
    `split_device(id)`: the manual safety-net override the OpenLogi
    applicability report (§2) called for — pairs every member of one id
    with every member of the other (or every member with every other member,
    for a split), so it works whether either id is already a merged group.
  - New D-Bus methods on `org.opengg.Daemon.Device`:
    `MergeMouseDevices(id_a, id_b)`, `SplitMouseDevice(id)`. Both are
    fire-and-forget writes to the override file — they take effect on the
    next `GetDevices` call, there's no separate "commit" step.
  - `daemon/src/device/dbus.rs` — `SetDpi`/`SetPollingRate` now just strip
    the `"ratbag:"` prefix and hand the rest straight to `RatbagManager`;
    all id-format knowledge (legacy vs. merged) now lives in `ratbag.rs`
    alone, `dbus.rs` no longer parses vid:pid itself for mouse ids.

**Why:** Phase 1 of the Devices Page roadmap. The previous session's
vid:pid identity fix was real progress but explicitly documented as not
fixing this specific case, since the G502's vid:pid genuinely differs
across connection modes. This closes that gap with a heuristic plus a
manual override, rather than leaving users stuck with a duplicate card and
no recourse.

**Landmines:**
- **No real fix exists for two identical-model mice plugged in
  simultaneously** — same limitation as before, name+vid can't disambiguate
  two literal duplicates. Unchanged, still the honest answer given what
  ratbagd exposes.
- **The heuristic can theoretically produce a false negative** (two
  connection-mode names that don't share a stopword-stripped form, e.g. an
  unusually named third-party firmware) — the manual `MergeMouseDevices`
  override exists specifically for this case. It cannot produce a false
  *merge* across different vendor IDs without an explicit override, by
  construction.
- **Discovered, NOT fixed, while verifying against real hardware**:
  `SetDpi` (and by inspection, `SetPollingRate` — same code shape) fails
  live against a real connected mouse: `busctl` call returns
  `org.freedesktop.DBus.Error.InvalidArgs: Incorrect parameters for
  property 'Resolution', expected 'v', got 'u'` — a variant-wrapping
  mismatch in the existing `res.inner().set_property("Resolution",
  zvariant::Value::from(dpi))` call, unrelated to and unchanged by this
  commit. Reproduced identically against both a merged id and a legacy
  single-link id, confirming it predates this change and isn't something
  the identity work introduced. Left unfixed here since fixing the D-Bus
  property write shape is Phase 2's (device-controls-UI) territory, not
  Phase 1's — but it blocks Phase 2's acceptance check ("confirm the device
  actually responds") until addressed, so flagging it now rather than
  letting it surface as a surprise mid-Phase-2.
- Manual QML "merge with…" / "not the same device" UI affordance was **not**
  added in this phase — `qt-shell/src/device.rs`'s `DeviceController` is
  still fully read-only (no invokables beyond `refresh()`), and there is no
  DPI/polling-rate control UI yet for a merge/split action to sit alongside
  (Phase 2). The D-Bus methods and override plumbing exist and are verified
  end-to-end via `busctl`; wiring an actual UI affordance to them is
  reasonable to fold into Phase 2 rather than bolting a standalone menu onto
  a page with no other controls yet.

**Verification:** `cargo clippy --all-targets -- -D warnings` → 0 warnings;
`cargo test` → 21/21 pass (12 new tests: `merge_key_slug` behavior,
grouping with/without overrides, id parsing). Built release, installed to
`~/.local/bin/openggd`, restarted the live `openggd.service`, and confirmed
against this machine's actual connected devices via `busctl --user`:
`GetDevices` collapses the real G502 wired+wireless pair into one
`ratbag:merged:046d:407f+046d:c08d` card; `SplitMouseDevice` on that id
splits it back into two cards and persists a `force_split` entry;
`MergeMouseDevices` on the two split ids re-merges them and persists a
`force_merge` entry, clearing the prior split; removing the override file
falls back to the heuristic alone, which reproduces the same merge
unassisted. Test override file removed after verification, daemon left
running the new binary.

---

### [2026-08-30] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Mouse device identity: key by vid:pid, not by ratbagd sysname)

**What Changed:**
- `daemon/src/device/ratbag.rs` — `DeviceInfo.id` for mice is now
  `"ratbag:{vid:04x}:{pid:04x}"` instead of `"ratbag:{sysname}"`. `sysname`
  is the last path segment of the ratbagd D-Bus object path — a route/
  enumeration-order artifact, not a stable device identity. It changes
  across replugs, USB re-enumeration, or a driver reset, which would
  duplicate the same physical mouse into a "new" device on OpenGG's side
  every time that happens.
- Added `RatbagManager::resolve_sysname(vid, pid)`: re-derives the current
  sysname from ratbagd's live device list on every write, by matching
  `Model` ("usb:VVVV:PPPP:version"). `set_dpi`/`set_polling_rate` now take
  `(vid, pid, ...)` and resolve internally instead of taking a
  caller-supplied sysname — callers were never meant to cache a sysname
  across calls anyway, this just makes the resolution the daemon's job.
- `daemon/src/device/dbus.rs` — generalized `parse_headset_id` into
  `parse_vid_pid_id(id, prefix)`, reused for both `"headset:"` (unchanged)
  and now `"ratbag:"`. `set_dpi`/`set_polling_rate` D-Bus handlers updated
  to parse vid:pid instead of stripping a sysname. Removed the now-unused
  `strip_prefix` helper.

**Why:** flagged in this session's OpenLogi-applicability report (§2) —
OpenLogi's `docs/DECISIONS.md` documents fixing the exact same bug class
(device identity keyed by transport/route instead of physical identity),
after hitting "same device, two cards" bugs on receiver re-enumeration.
OpenGG's `ratbag:{sysname}` scheme had the identical exposure. It wasn't
observable yet only because no per-device settings were persisted anywhere
keyed by that id — the moment any feature does that (button remapping,
per-device RGB profiles, cached device images), a route-scoped id turns
into silent, hard-to-diagnose data loss on replug. Headsets already used a
vid:pid-based id (`headset:{vid}:{pid}` in `headset.rs`) — this makes mice
consistent with that scheme rather than being the odd one out.

**Landmines:**
- **Known, accepted limitation**: vid:pid is *not* a full identity fix.
  ratbagd's D-Bus API exposes no per-unit serial (`Model` is identical for
  every unit of the same mouse model — confirmed against
  `/usr/share/doc/libratbag/html/_sources/dbus.rst.txt`), so two identical-
  model mice plugged in simultaneously will still collide onto the same
  `id`. This is a real gap, not silently swept under the rug — there is
  currently no stronger identity signal available from ratbagd to
  disambiguate them. If/when this needs fixing for real (someone actually
  hits it), the fix has to come from somewhere ratbagd doesn't expose today
  (e.g. reading the kernel `uniq`/serial via udev/hidraw directly, bypassing
  ratbagd for identity purposes only).
- `resolve_sysname` walks ratbagd's live device list once per write call
  rather than caching a vid→sysname map — deliberate, since caching would
  reintroduce exactly the staleness bug this fix removes if a replug happens
  between calls. This makes each `set_dpi`/`set_polling_rate` call slightly
  more expensive (one extra `Manager.Devices` + N `Device.Model` round
  trips), which is a non-issue at the call frequency these see (interactive
  slider commits, not a hot path).
- Nothing outside `daemon/src/device/` constructed or parsed a `"ratbag:"`
  id anywhere in the tree (confirmed via `grep -rn "ratbag:"` before making
  this change) — `qt-shell/src/device.rs`'s `DeviceController` is read-only
  today and `core/src/device.rs` treats `device_id` as an opaque string
  passed straight through to D-Bus. So this change has no client-side
  fallout, but it does mean the write path (`set_dpi`/`set_polling_rate`)
  is still not wired up to any UI yet — that remains a separate gap.

**Verification:** `cargo clippy --all-targets -- -D warnings` → 0 warnings;
`cargo test` → 9/9 pass (daemon crate; no device-specific unit tests exist
yet, this exercises the crate compiles/links cleanly only).

---

### [2026-08-23] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Remove the archived Tauri + Vue frontend entirely)

**What Changed:**
- Deleted `frontend/` in full (Tauri 2 + Vue 3 UI, its Rust backend, Pinia
  stores, locales, and build config) — it had been archived and unbuilt for
  some time; this removes it from the repository rather than just from the
  default build path. Qt6/QML (`qt-shell/`) is now the only UI in the repo.
- Preserved the app icon set (`128x128.png`, `256x256.png`, `32x32.png`,
  `512x512.png`, `logo.svg`) by copying it to a new `packaging/icons/`
  before deleting `frontend/` — it was the only asset under `frontend/`
  still needed by anything outside it (`.desktop` files, AUR packaging,
  the Flatpak manifest).
- Deleted `scripts/fetch_device_assets.sh` and `docs/DESIGN_TOKENS.md` —
  both were entirely about the archived frontend (per-device product
  images that only ever existed in the Vue UI's asset pipeline — `qt-shell`
  has no equivalent yet, this is a real feature gap, not silently dropped
  functionality that existed in the Qt UI; and a CSS-custom-property
  design-token doc for a component tree that no longer exists).
- **Found and fixed the actual release/packaging pipeline was still building
  and shipping the archived Tauri app**, not the Qt6 shell:
  - `.github/workflows/release.yml` — rewritten to build `daemon/` +
    `qt-shell/` (release) on the same Arch container CI already uses for
    the Qt6/QML shell (Qt 6.7+ requirement), package a tarball, and create
    the GitHub Release via `softprops/action-gh-release@v2` (replacing the
    side effect `tauri-apps/tauri-action` used to provide). **Not verified
    against a real tag push** — only YAML-validated and read through
    carefully; flagged for a real dry run before the next release.
  - `.github/workflows/distro-matrix.yml` — dropped the "Cargo check Tauri
    backend" step and its now-pointless webkit2gtk/appindicator/librsvg/
    patchelf system deps (the daemon has zero system-library dependencies
    beyond libc).
  - `.github/workflows/codeql.yml` — Rust autobuild now installs Qt6/
    GStreamer dev packages instead of WebKitGTK/appindicator, so autobuild
    can actually compile `qt-shell` instead of the now-deleted Tauri crate.
  - `.github/dependabot.yml` — replaced the `frontend/src-tauri` and
    `frontend` (npm) entries with `qt-shell` and `core` cargo entries.
  - `packaging/aur/PKGBUILD` — dependencies were still `webkit2gtk-4.1` +
    `libayatana-appindicator` (Tauri runtime) with zero Qt6/GStreamer
    packages listed; replaced with the actual qt-shell/gstreamer runtime
    deps, icon source URL repointed at `packaging/icons/`, `.SRCINFO`
    regenerated via `makepkg --printsrcinfo`.
  - `packaging/flatpak/org.opengg.OpenGG.yml` — replaced the Tauri module
    (GNOME runtime, WebKitGTK finish-args) with a qt-shell module on the
    KDE runtime; rewrote `packaging/flatpak/README.md` and
    `FINISH_ARGS_REFERENCE.md` to match. This manifest was already
    TODO-laden and never fully functional (missing vendored
    `cargo-sources.json`), so this is an on-paper fix, not a verified build.
- `Makefile` and `dev.sh` — removed `ui-legacy`/`ui-deps`/`FRONTEND` var,
  `check`/`lint`/`clean` targets no longer touch `frontend/src-tauri` or
  `vue-tsc`; `do_build`/`do_setup` in `dev.sh` now build/check `qt-shell`
  instead of running `npx tauri build`/`npm install`.
- `opengg.desktop(.template)` — icon path repointed at `packaging/icons/`.
- `scripts/bump-version.sh` — dropped the `frontend/src-tauri/Cargo.toml`,
  `frontend/package.json`, `frontend/src-tauri/tauri.conf.json` sed passes
  and the trailing `npm install --package-lock-only`; added a
  `qt-shell/Cargo.toml` version bump.
- Rewrote the frontend-describing sections of `CLAUDE.md` (repo tree, IPC
  map, "Frontend State (Pinia Stores)", VU counter, media server, file
  watcher, theme system, MixerPage tabs, DSP engine, Key Constraints) to
  describe the actual current `qt-shell`/`core`/`daemon` implementations
  instead of deleted Vue/Tauri code — verified each replacement against
  the real source (`qt-shell/src/{audio,theme,eq,i18n}.rs`,
  `core/src/watcher.rs`) rather than assuming parity.
- Updated `README.md`, `CONTRIBUTING.md`, `SECURITY.md`,
  `docs/TRANSLATING.md` (rewritten for `qt-shell/src/i18n.rs`'s real
  locale-loading mechanism — single directory + `OPENGG_LOCALES_DIR`, no
  separate user-locales merge layer, RTL is a separate toggle from
  language choice, and there's no automated locale-parity checker yet),
  `.github/pull_request_template.md`, `.github/ISSUE_TEMPLATE/
  extension_api_rfc.yml`, `AGENTS.md`, `EXTENSION_MANIFEST_SCHEMA.md`,
  `THIRD_PARTY_LICENSES.md`.
- `docs/ARCHITECTURE.md` and `docs/CLIP_EDITOR_DESIGN_GAP.md` — these
  describe the Tauri/Vue app in detail as historical design records, not
  living documentation; added an explicit "historical, out of date, see
  CLAUDE.md/AGENTS.md instead" banner to each rather than rewriting them
  (`ARCHITECTURE.md` was already stale beyond just the frontend — its
  "Planned (Phase 2)" list includes features that have since shipped).

**Why:**
User asked to remove the archived interface so the Qt6/QML shell is the
only UI in the program. A first pass at just `rm -rf frontend/` would have
left the release pipeline still building and shipping the deleted app
(release.yml built `frontend/src-tauri` via `tauri-action` as the actual
release artifact), so this went further than a straight deletion — every
place that referenced, built, packaged, or documented the old frontend
needed to change for "only one UI" to be true end-to-end, not just true of
`dev.sh`.

**Landmines & Discoveries:**
- `core/src/audio.rs::create_virtual_audio`/`remove_virtual_audio` are
  called directly from `qt-shell/src/audio.rs` (not through the daemon by
  default for creation) — already fixed in the prior session's commit
  `7cb924e`; unrelated to this session but worth remembering when auditing
  what "still touches the old frontend" actually means for audio code.
- `core/src/extensions.rs` still documents a `window.opengg.invoke(...)`
  extension API and `docs/EXTENSION_DEV.md` describes extension UI panels
  as Vue 3 components loaded into *something* — no `WebEngineView` or
  similar embedding was found anywhere in `qt-shell/`. **Not resolved in
  this session** — whether/how third-party extension UI panels are hosted
  in the Qt6 shell at all is an open question that goes beyond removing
  the archived main-app frontend, and needs its own investigation before
  anyone trusts `docs/EXTENSION_DEV.md`'s Vue instructions to produce a
  working panel today.
- `packaging/aur/PKGBUILD`'s dependency list and the Flatpak manifest were
  both silently stale for the Qt6 migration already (still listing Tauri
  runtime deps with zero Qt6 packages) — worth periodically diffing
  packaging manifests against what a crate's `Cargo.toml` actually needs,
  since nothing catches this kind of drift automatically.

**Verification:**
- `cargo check` + `cargo test` in `daemon/`: 9/9 tests pass.
- `cargo check` in `core/`: clean.
- `cargo check` in `qt-shell/`: clean (pre-existing C++ header SFINAE
  warnings only, unrelated to this change).
- `make lint`: passes — `cargo clippy -W clippy::all` on daemon + qt-shell
  (pre-existing, unrelated warnings only), `check-colors.sh` clean.
- `bash -n dev.sh`, `bash -n scripts/bump-version.sh` passed.
- `make -n dev/ui/build/install/lint/check/clean` all passed (dry-run).
- All touched YAML (`release.yml`, `distro-matrix.yml`, `codeql.yml`,
  `security.yml`, `ci.yml`, `dependabot.yml`, `org.opengg.OpenGG.yml`,
  `extension_api_rfc.yml`) parsed successfully with `python3 -c "import
  yaml; yaml.safe_load(...)"`.
- `makepkg --printsrcinfo` regenerated `packaging/aur/.SRCINFO` cleanly.
- **Not verified**: an actual GitHub Actions run of `release.yml` against a
  real tag (no way to trigger that from this session) — this is the
  highest-risk unverified piece of this change. Flag before the next
  release tag is pushed.
- **Not verified**: an actual `flatpak-builder` build of the rewritten
  manifest — it was already non-functional (missing vendored sources)
  before this change, so this is a like-for-like on-paper fix, not a
  regression.

---

### [2026-08-23] Claude Sonnet 5 — qt6-gstreamer-player-b3 (SIGTERM handling + single-owner virtual-sink creation)

**What Changed:**
- Followed up a technical audit's two confirmed findings on virtual-sink
  lifecycle bugs (leaked/orphaned `OpenGG_*` PipeWire sinks):
  1. `daemon/src/main.rs` previously only awaited `tokio::signal::ctrl_c()`
     (SIGINT). `systemctl --user stop`, session logout, and most desktop
     shutdown paths send SIGTERM, which terminates the process without
     unwinding — so `SinkManager`'s `impl Drop` cleanup (the *only* cleanup
     path) never ran. Added a `tokio::signal::unix::signal(SignalKind::terminate())`
     arm alongside `ctrl_c()` in a `tokio::select!`, so either signal now
     reaches the same graceful-shutdown `Ok(())` return.
  2. The Qt UI's "Create Virtual Audio" control (Settings → Danger Zone,
     and the onboarding tour) called `opengg_core::audio::create_virtual_audio()`
     directly from the UI process — a raw `pactl load-module` call with no
     module-ID tracking anywhere, invisible to the daemon's `SinkManager`.
     Refactored `daemon/src/audio/sinks.rs` to extract sink+loopback
     creation into a shared `create_sinks_and_loopbacks()` helper, added an
     instance method `SinkManager::ensure_created()` that folds newly
     created module IDs into the existing `module_ids` vec, wired it up
     through `AudioHub::create_virtual_audio()` → a new
     `org.opengg.Daemon.Audio.CreateVirtualAudio` D-Bus method, and changed
     `core::audio::create_virtual_audio()` to call that D-Bus method first
     (mirroring the existing daemon-preferred pattern already used by
     `remove_virtual_audio()`), falling back to the old direct-`pactl` path
     only when the daemon is unreachable.

**Why:**
Two independent, confirmed root causes for orphaned/duplicate `OpenGG_*`
sinks: an unclean-shutdown gap (SIGTERM never handled) and a second,
untracked sink-creation surface that didn't require any crash at all —
just a user clicking "Create Virtual Audio." Both are now routed through
the single `SinkManager` instance that owns cleanup.

**Landmines & Discoveries:**
- `SinkManager::teardown_all()` drains `module_ids` but does not clear the
  `channels` map — `ensure_created()` re-seeds any missing channel entries
  with `.entry().or_insert_with()` rather than assuming a fresh map, since
  a create→remove→create cycle on the same daemon instance leaves
  `channels` populated the whole time.
- `remove_virtual_audio()`'s D-Bus-first-then-local-fallback pattern in
  `core/src/audio.rs` was the template for `create_virtual_audio()` — same
  `call_dbus_void` helper, same log-and-fall-through structure.
- SIGKILL still cannot be caught by any means; the SIGTERM fix closes the
  much more common real-world gap (service stop / logout), not every
  possible way the daemon can die.

**Verification:**
- `cargo clippy -- -W clippy::all` in `daemon/` and `core/`: zero new
  warnings (both crates had pre-existing, unrelated warnings in
  untouched code — `core/src/audio.rs` doc-comment spacing,
  `core/src/clips/mod.rs` char-comparison style — left as-is, out of scope).
- `cargo test` in `daemon/` (9 passed) and `core/` (50 passed): all green.
- `cargo check` in `qt-shell/` (which depends on `core`): builds clean;
  no QML or qt-shell Rust source changed, since `qt-shell/src/audio.rs`'s
  `create_virtual_audio()` already just delegates to
  `opengg_core::audio::create_virtual_audio()` and needed no edit.
- Not run: a live end-to-end test (kill `-TERM` the daemon, confirm sinks
  are gone; click "Create Virtual Audio" with the daemon up, confirm the
  daemon's own D-Bus method fires instead of the local pactl fallback) —
  this was a static-analysis-driven fix, flagged here for a follow-up
  manual pass before shipping.

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Mixer + GSR-dropdown translation gaps from user screenshots)

**What Changed:**
- User supplied 6 Arabic-mode screenshots showing the Mixer page (tab bar,
  channel strip headers, GraphicEQ/DspControls channel labels) rendering
  fully in English, and the GSR quality/FPS/replay-buffer dropdowns in
  Settings → Capture & Sound and the Clips-page recording popover showing
  stale/English option labels.
- Root cause (Mixer): `MixerPage.qml`'s `tabs` array and every channel
  identifier (`"Master"`, `"Game"`, `"Chat"`, `"Media"`, `"Aux"`, `"Mic"`)
  were rendered directly as display text via `.toUpperCase()` — these
  strings double as functional identifiers passed straight to
  `AudioController`/`EqController` (`setVolume`, `applyEq`,
  `applyNoiseReduction`, etc.), so they can't be swapped for translated
  values; only the *rendered label* could change. Added `mixer.tabs.*`
  and `mixer.channels.*` keys to `en.json`/`ar.json` and a translation
  lookup at each render site — `MixerPage.qml`'s new `tabLabel(id)`
  function, `ChannelStrip.qml`'s new `displayName()` function, and a
  direct `I18n.t("mixer.channels." + channel.toLowerCase())` in
  `GraphicEQ.qml` and `DspControls.qml` — leaving every identifier passed
  to the controllers untouched.
- Root cause (GSR dropdowns): the option-list properties
  (`gsrQualityOptions`, `gsrFpsOptions`, `gsrReplayOptions`, and in
  `RecordingControl.qml` also `gsrTargetOptions`) were `readonly property
  var` array literals built once from `I18n.t()` calls. QML's binding
  dependency tracking only follows *property* reads, not invokable calls
  — so `I18n.t()` inside an array literal never registers a dependency on
  `I18n.language`, and the array never re-evaluates on a live language
  switch (same landmine as the earlier `MixerPage.tabs` fix in the prior
  i18n-scan session). Converted all option lists in both
  `RecordingControl.qml` and `CaptureSoundPanel.qml` from properties to
  functions, and wrapped every call site — `model:`/`options:`,
  `currentIndex:`, `onActivated:` — in the `(I18n.language, ...)`
  comma-trick to force the re-evaluation.
- Investigated `devices.appsShown`/`devices.appsPerRow` (flagged in one
  of the screenshots) and found both already correctly wired through
  `I18n.t()` in `MixerPage.qml` with matching Arabic strings present —
  no change made; likely a reading/rendering ambiguity in the screenshot,
  not a real gap.

**Why:**
Direct user follow-up with screenshots after the prior i18n-scan session:
"I have included some problems with the Arabic language and some pages
that are not covered by the translations."

**Landmines & Discoveries:**
- Confirms the `readonly property var` + `I18n.t()` dependency-tracking
  gap (first found for `MixerPage.tabs` in the previous session) is a
  recurring pattern, not a one-off — it hit two more files
  (`RecordingControl.qml`, `CaptureSoundPanel.qml`) across four option
  lists this round. The fix is always the same: property → function,
  every call site wrapped in `(I18n.language, fn())`.
- Channel-identifier-as-display-text is a distinct trap from the missing
  translation issue: the string itself must stay untranslated (it's an
  API parameter), only the label shown to the user changes — a plain
  key-swap in the underlying property would have broken audio routing.

**Verification:**
- `cargo build` clean
- `cargo test` — 22/22 passing
- `qt-shell/tools/check-colors.sh` — clean
- `qt-shell/tools/ui-shots.sh` — zero QML warnings across all 16 targets
- Manual `OPENGG_LANG=ar` screenshots of Mixer and Settings → Capture &
  Sound confirm the tab bar, channel headers, and GSR quality/FPS/replay
  dropdowns now render translated Arabic text on a fresh launch. The live
  language-switch case for the GSR dropdowns (app already running, user
  flips language) was not separately screenshot-verified — the
  `--screenshot` capture path only supports a fresh single-language
  launch — but is covered by the same `(I18n.language, ...)` fix pattern
  already confirmed working for `MixerPage.tabLabel()`.

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (i18n scan: missing translations + LTR alignment bug)

**What Changed:**
- Root-caused "some texts move to the right even in LTR mode": Qt's `Text`
  element auto-detects RTL-script content (Arabic) via the Unicode BiDi
  algorithm and right-aligns itself *within its own box* whenever
  `horizontalAlignment` isn't explicitly set — independent of
  `LayoutMirroring`/`I18n.rtl`. Any `Text { Layout.fillWidth: true }`
  bound to a translated string was therefore silently right-anchoring
  itself the moment its content happened to be Arabic, even with the
  app's actual layout direction still LTR. Scanned every `qml/**/*.qml`
  file for `Text` blocks combining `Layout.fillWidth: true` (or
  `width: parent.width`) + an `I18n.t()`-sourced `text:` with no explicit
  `horizontalAlignment`, and added `horizontalAlignment: Text.AlignLeft`
  to all 28 matches across `Sidebar.qml`, `DevicesPage.qml`,
  `HomePage.qml`, `MixerPage.qml`, `CaptureSoundPanel.qml`,
  `ExtensionsPanel.qml`, `AboutPanel.qml`, `StoragePanel.qml`,
  `MixerRoutingPanel.qml`, `LanguagePanel.qml`, `DspControls.qml`,
  `RecorderInstallHelper.qml` — explicit `AlignLeft` still mirrors
  correctly to `AlignRight` when true RTL mode (the toggle from the
  previous session) is turned on, since `LayoutMirroring` flips
  *explicit* Left/Right alignment but does nothing for content-based
  auto-alignment.
- Scanned for hardcoded English strings never routed through `I18n.t()`
  (and therefore invisible to translators — the other half of the user's
  report) by diffing every `I18n.t("key")` call site against `en.json`'s
  flattened key set, plus a manual grep for bare `text:`/`placeholderText:`
  literals. Wired ~35 previously-hardcoded strings to the catalog across
  `ClipsPage.qml` (empty states, rename/delete confirmation dialogs,
  search placeholder), `ExportDialog.qml` (target size/codec section
  labels — these already had unused matching keys from an earlier port),
  `RecordingControl.qml`, `DevicesPage.qml`, `ClipEditorPage.qml`,
  `VideoPlayer.qml` (loading label only — no playback logic touched),
  `CaptureSoundPanel.qml`, `ExtensionsPanel.qml`, `Titlebar.qml` (Beta
  badge), `ComingSoonPanel.qml`. Added the genuinely-missing keys (with
  Arabic translations) to both `en.json` and `ar.json`: `common.beta`,
  `common.info`, `common.notAvailableInQt`, `editor.frameSaved`,
  `clips.emptyTitle/emptyHint/noMatchTitle/noMatchHint/deleteConfirm.*/
  bulkDeleteConfirm.*`, `recording.buffer/target`,
  `videoPlayer.loading`, `settings.captureGsr.estUsage`. Deliberately
  left untranslated: brand names (OpenGG/GitHub/Discord), the version
  prefix, single-character icon glyphs, and the RTL toggle's "RTL" badge
  (consistent with the pre-existing untranslated LTR/RTL direction badge
  next to every language row).
- Verified key parity both directions (`en.json` keys ⊆ used keys ⊆
  `ar.json` keys) via a small Python diff script rather than by eye,
  after every edit round.

**Why:**
Direct user follow-up after the RTL/language-toggle session: some UI
text still showed raw untranslated strings, and some text visibly sat
flush-right in the settings while the app was in plain English/LTR mode.

**Landmines & Discoveries:**
- **A one-line `Text { ... }` block edited by a line-based insertion
  script** (used for the 28-file alignment sweep) can silently produce a
  syntactically-valid-looking but semantically-broken file: inserting a
  new property line "after the opening line" lands it as a *sibling*
  property of the block's parent, not inside the `Text{}}`, if the whole
  block was written on one line. This doesn't throw a QML syntax error at
  parse time for some property names, but for `horizontalAlignment` on a
  non-Text parent it throws `Cannot assign to non-existent property` and
  crashes the QML engine at load — caught immediately by `ui-shots.sh`
  (`cargo build` does not catch it; QML type-checking is a runtime
  concern in this cxx-qt/QRC setup, not a compile-time one). Fixed by
  re-scanning every insertion for a preceding single-line `Text { ... }`
  and merging the property into that line instead.
- Confirms the existing AGENTS.md rule that `ui-shots.sh` (with
  `QT_FORCE_STDERR_LOGGING=1`) is the only reliable gate for QML
  correctness — a clean `cargo build` here still shipped a page that
  hard-crashed on load.

**Verification:**
- `cargo build` clean
- `cargo test` — 22/22 passing
- `qt-shell/tools/check-colors.sh` — clean
- `qt-shell/tools/ui-shots.sh` — zero QML warnings across all 16 targets
  (after finding and fixing the single-line-Text insertion bug above)
- Manual `OPENGG_LANG=ar` screenshots of Home and Settings → Language
  confirm every nav/label/heading now sits flush-left with the sidebar
  still on the left, matching the "Arabic defaults to LTR" design intent

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Mixer stuck-duck, theme reset, minimize-to-tray, RTL language toggle)

**What Changed:**
- `core/src/ear_blast.rs`: added `EarBlastState::active_channels()` and
  `release_all()` — force-restores every currently-ducked channel's real
  PipeWire volume, bypassing the normal dB/hold-time gating.
- `qt-shell/src/audio.rs`: `start_vu_stream`'s reader thread now calls
  `ear_blast::release_all()` right after its main loop exits (page
  navigated away or app quitting). Root cause of "Game/Media drop to 60%
  when I open the Mixer": Ear Blast Protection was ducking a channel, then
  the VU stream stopped before its next `check()` pass could release it —
  nothing was left monitoring the level, so PipeWire's real sink volume
  stayed stuck at the duck target indefinitely, surfacing next time as "the
  Mixer opened at 60%."
- `qt-shell/src/theme.rs`: added a real `reset()` qinvokable that discards
  `theme.json`'s saved override and re-derives from built-in defaults.
  `GeneralPanel.qml`'s reset button previously called `reload()`, which
  only re-applies whatever is already on disk — a no-op once a custom
  accent had been saved, since disk had nothing else to fall back to.
- `qt-shell/qml/Main.qml`: built a full minimize-to-tray feature via
  `Qt.labs.platform.SystemTrayIcon` (confirmed present system-wide, no new
  native bindings needed) — a root `s`/`Connections` mirror of
  `SettingsController.settingsJson` reads `runInBackground`; `onClosing`
  intercepts the frameless titlebar's close button and hides instead of
  quitting unless the setting is off or the tray menu's "Quit" was used
  (`reallyQuit` flag). "Start on Boot" was already correct at the backend
  level (verified `~/.config/autostart/opengg.desktop` on disk) — the real
  gap was that Minimize to Tray had no implementation anywhere.
- `qt-shell/src/i18n.rs`: added a `rtlOverride` qproperty (persisted
  independently of the active language) plus `setRtlEnabled`,
  `openLocalesFolder`, and `reloadLocales` qinvokables. Arabic (and any
  future RTL-tagged locale) now defaults to LTR rendering like every other
  language instead of auto-flipping the layout; a separate opt-in toggle
  lets the user switch to RTL, and that choice survives later language
  switches because it's a distinct persisted field, not derived from the
  active language.
- `qt-shell/qml/pages/settings/LanguagePanel.qml`: replaced the plain
  `SettingsCard` title with a bespoke header row (same reasoning as
  MixerRoutingPanel's Ear Blast Protection card — see `SettingsCard.qml`'s
  own header-note) hosting three buttons: open the locales folder
  (`en.json`/`ar.json` side by side, for translating or adding a new
  language pack), reload locales from disk, and an RTL toggle pill that
  only appears when the active language's `_meta.dir` is `"rtl"`.
- `qt-shell/locales/en.json` / `ar.json`: added a `"tray"` block
  (`show`/`quit` strings for the tray menu). The `"language"` block's
  `addLanguage`/`reloadLanguages`/`rtlModeHint` keys already existed in
  both locales (ported earlier, unused until this session).
- `core/src/system.rs`: added `open_path()` — opens any file/dir with the
  desktop's default handler; used by `openLocalesFolder` instead of adding
  a duplicate `open` crate dependency to `qt-shell`.

**Why:**
Direct user report of five issues: Mixer volume unexpectedly dropping,
General's theme reset button doing nothing, Start on Boot/Minimize to Tray
not working, and a request to default Arabic to LTR with an opt-in RTL
toggle plus a way to export the English strings for translation. The
RTL/language design (independent `rtlMode` field, default `false`, header
button layout) was not invented — it mirrors an already-designed-but-never-
ported feature found in the archived `frontend/src/components/settings/
LanguageSettings.vue` and `frontend/src/stores/persistence.ts`.

**Landmines & Discoveries:**
- **cxx-qt qinvokable/qproperty name collisions**: a hand-written
  qinvokable's Rust name or its `#[cxx_name]` cannot reuse the name a
  `#[qproperty(...)]` auto-generates for its setter (`set_<field>` in Rust,
  camelCase `set<Field>` in C++/QML) — collides even if the invokable's own
  QML-facing name differs. `rtl_override`'s qproperty setter is
  `setRtlOverride`; the invokable had to become `apply_rtl_override` /
  `cxx_name = "setRtlEnabled"` to avoid a `defined multiple times` /
  "cannot be overloaded" build error.
- **`self.as_mut()` + reading `self.*` in the same statement** trips
  Rust's borrow checker under cxx-qt's `Pin<&mut Self>` pattern — hoist
  every needed `self.*` read into a `let` binding before any
  `self.as_mut()` mutation call.
- **`use cxx_qt::CxxQtType;`** is required to call `.rust_mut()` on
  `Pin<&mut Self>` for mutating a plain (non-qproperty) struct field after
  construction — not imported by default; found by grepping `eq.rs`/
  `clips.rs` for existing working uses.
- The `open` crate is only a dependency of the `core` crate, not
  `qt-shell` directly — route new "open this path" calls through a
  `core::system` helper rather than adding a second copy of the dependency.

**Verification:**
- `cargo build` clean (qt-shell)
- `cargo test` — 22/22 passing (including a new
  `i18n::tests::language_and_rtl_mode_are_independent`)
- `qt-shell/tools/check-colors.sh` — clean
- `qt-shell/tools/ui-shots.sh` — zero QML warnings

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (fillWidth-wrapper bug, round 2: ShortcutsPanel + MixerRoutingPanel)

**What Changed:**
User's follow-up screenshot showed ShortcutsPanel's key-recorder boxes still
clustered next to their labels instead of right-aligned like the reference
design — the exact same root cause just fixed in `GeneralPanel.qml` (a
nested Layout marked `Layout.fillWidth: true` doesn't stretch the way a
plain `Item` spacer does), just missed there because the earlier sweep only
grepped for `ToggleSwitch` call sites and this row uses a plain `Rectangle`
key-box instead.
- `ShortcutsPanel.qml`: fixed both the header row ("Reset to Defaults" was
  landing next to the title) and every per-shortcut row (key box was landing
  next to the label) — replaced the nested-RowLayout wrappers with
  `Item { Layout.fillWidth: true }` spacers.
- Audited the rest of `qml/` for the same shape (a wrapper Layout whose sole
  child is another Layout, followed by a trailing sibling control) and found
  two more live instances in `MixerRoutingPanel.qml`'s Danger Zone card
  ("Reset Virtual Audio" / "Remove Virtual Audio & Restore OS Defaults" —
  both `ColumnLayout`-wrapped). Fixed the same way.
- Confirmed via screenshot that `DspControls.qml`'s Noise Reduction/Gate/
  Compressor toggles, `GraphicEQ.qml`'s Enabled toggle, and Extensions'
  Modules list do **not** have this bug — they either use a dedicated `Item`
  spacer already, put `Layout.fillWidth` directly on a `Text`, or wrap
  multiple `Text` children directly in a `ColumnLayout` (no intermediate
  nested Layout) — narrowing the actual defect to "a Layout whose *sole*
  child is itself a Layout doesn't propagate `Layout.fillWidth` to its
  parent's stretch calculation," not "any nested Layout is broken."

**Why:**
Direct user follow-up with a labeled before/after crop pinpointing the
Shortcuts key-box position. Given the same bug had just been found once,
did a systematic sweep rather than fixing only the reported instance.

**Verification:**
- `cargo build` succeeded
- `qt-shell/tools/check-colors.sh` passed
- `qt-shell/tools/ui-shots.sh` — all 16 pages, zero QML warnings
- `cargo test` — 21/21 passing
- Screenshotted Shortcuts, Audio Engine (Danger Zone), and Mixer → Chat tab
  (DSP Controls) to confirm right-alignment matches the reference pattern
  and that the DspControls toggles were never actually affected
- Live app relaunched (`QT_FORCE_STDERR_LOGGING=1`) — clean startup

---

### [2026-08-05] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Toggle centering, real toggle-row bug, Shortcuts overcorrection, icon picker, module→nav gating)

**What Changed:**
- `ToggleSwitch.qml`: knob was `y: 3` against a `parent.height-8` knob in a
  22px track — 3px top / 5px bottom, not centered. Fixed to
  `(parent.height - height) / 2`.
- `GeneralPanel.qml` — the real "toggle attached to the title" bug: "Start
  on Boot" / "Minimize to Tray" wrapped their title+info-icon in a *nested*
  `RowLayout` marked `Layout.fillWidth: true`. Unlike a plain `Item` or
  `ColumnLayout` spacer, that nested RowLayout doesn't actually stretch to
  claim the row's remaining space, so the toggle ended up clustered right
  next to the icon instead of pushed to the card's right edge like every
  other toggle row in the app. Found by systematically screenshotting every
  ToggleSwitch call site (7 files) at both 1280px and a narrow 960px test
  width — this was the only one that didn't match the established
  Item-spacer pattern. Replaced with the same dedicated
  `Item { Layout.fillWidth: true }` spacer used everywhere else.
- `ShortcutsPanel.qml`: last session's fix for "crammed" rows (10px/10px
  margins) overcorrected — user's side-by-side comparison showed the
  reference/old design is markedly more compact than what shipped. Reduced
  to 5px/5px, keeping the divider.
- `TrackManagementPanel.qml`: the per-track icon button just cycled blindly
  through a fixed 6-icon array on every click with no way to see or choose
  a specific one. Replaced with an inline icon-choice strip (plain `Row`
  toggled by a `openIconPickerFor` property) — deliberately *not* a
  Popup/ComboBox, since a separate `IconPicker.qml` built on those was
  already documented in this file as hanging the app at startup for
  unknown reasons.
- `Sidebar.qml`: Settings → Extensions → Modules toggles (audio/device/
  replay) called `ExtensionsController.setModule()` but nothing ever read
  `modulesJson` outside the Extensions panel itself — confirmed via grep
  this was never wired even in the archived Vue reference, so it's a new
  feature rather than a porting gap. Added `visibleNavItems`, filtering
  `navItems` by module state (mixer→audio, devices→device, clips→replay;
  home/settings have no backing module and always show), and an
  `ExtensionsController.refresh()` call in `Component.onCompleted` so the
  sidebar reflects saved module state from the first frame rather than only
  after the user visits the Extensions panel.

**Why:**
Follow-up to the previous design-fidelity pass: user reported the toggle
knob still looked off-center, pointed at a toggle sitting flush against a
title in a screenshot, provided a direct before/after comparison showing
Shortcuts had gone too far the other way, and asked for the Timeline Tracks
icon control and Extensions module toggles to actually do something instead
of being decorative. Also asked for a GPU Screen Recorder install
explanation — `RecorderInstallHelper.qml` (distro-aware install command +
installed/missing states) already covers this and is embedded in Capture &
Sound; screenshot-verified it renders the installed-confirmation state
correctly, so no code change was needed there.

**Deliberately not touched:** Timeline Tracks config still doesn't affect
the actual clip editor (`ClipEditorPage.qml`) — that's audio/video editor
scope, out of bounds per the standing agreement that playback/editor work
is handled elsewhere. Only the settings-panel icon-picker UX itself was
fixed.

**Verification:**
- `cargo build` succeeded
- `qt-shell/tools/check-colors.sh` passed
- `qt-shell/tools/ui-shots.sh` — all 16 pages, zero QML warnings, including
  after the TrackManagementPanel delegate restructure (risk of reintroducing
  the documented Popup/ComboBox hang — confirmed clean)
- `cargo test` — 21/21 passing
- Screenshotted every ToggleSwitch call site at 1280px and 960px to find
  the actual "attached to title" offender before writing a fix
- Live app relaunched (`QT_FORCE_STDERR_LOGGING=1`) — clean startup

---

### [2026-08-04] Claude Sonnet 5 — qt6-gstreamer-player-b3 (Toggle/dropdown/shortcuts/storage fidelity fixes)

**What Changed:**
- `ToggleSwitch.qml`: the knob was hardcoded `"#ffffff"` and the track was a
  solid accent fill with no border — user asked why the toggle's white
  didn't match the theme. Read the original `ToggleSwitch.vue`: it was never
  white. Track is an accent-tinted (not solid) fill with an accent border
  when on; knob is `text-muted` off / `accent` on. Rewrote to match exactly.
- `SelectField.qml`: bumped from `implicitHeight: 26, font.pixelSize: 11,
  color: Theme.textDim`, no focus feedback, to `34px/13px/Theme.text` with an
  `activeFocus` border — matching every other ComboBox in the app (Capture &
  Sound's Quality/FPS fields). This was the "dropdown menus are weak/small"
  complaint; SelectField backs Clip Preferences and Notifications'
  Position/Duration fields, so it read as visibly weaker than the plain
  ComboBoxes used elsewhere despite being the more common variant.
- `ShortcutsPanel.qml`: rows had no divider between them and only a 4px top
  margin, so all 9 actions ran together into one dense block ("crammed
  together"). Restructured the Repeater delegate to a `ColumnLayout` with a
  divider after every row except the last, plus real 10px top/bottom margins.
- `StoragePanel.qml`: Clip/Screenshot directory rows were bare text with no
  background, reading as "just text" rather than a file location. Wrapped
  each row in a bordered `Theme.bg` chip (36px, radius, folder icon + path +
  remove icon), same treatment for both `clipDirRow` and `shotDirRow`.

**Why:**
User sent 6 screenshots (toggle switches, Clip Preferences dropdown,
Shortcuts page, Language panel, Timeline Tracks, Storage panel) with four
explicit text complaints: toggle color mismatch, cramped Shortcuts spacing,
storage rows looking like plain text, and weak/small dropdowns. Addressed
each by reading the original Vue source (`ToggleSwitch.vue`) or comparing
against sibling components already in the QML port (other ComboBoxes, other
card dividers) rather than guessing at new styling.

**Deliberately not touched:** two elements visible in the reference images
but not named in the text complaint were left alone — the Language panel's
folder/refresh header icons (no backend command wired for them) and Timeline
Tracks' "Live Preview" section (documented Phase 5/editor scope, not
implemented in the Qt port yet). Both would be net-new feature work, not
part of this design-fidelity pass.

**Verification:**
- `cargo build` succeeded
- `qt-shell/tools/check-colors.sh` passed
- `qt-shell/tools/ui-shots.sh` — all 16 pages captured, zero QML warnings
- `cargo test` — 21/21 passing
- Individual screenshots of General/Shortcuts/Storage/MixerRouting panels
  visually inspected
- Live app relaunched (`QT_FORCE_STDERR_LOGGING=1`) — clean startup, no QML
  errors in the log

---

### [2026-08-04] Claude Opus 5 — qt6-gstreamer-player-b3 (Settings/Mixer consistency pass)

**What Changed:**
- Fixed a settings-panel right-edge overflow: `settingsContentCol` combined
  an explicit `x: 28` offset with `width: parent.width`, pushing its right
  edge 28px past the viewport — a 28px left gutter with none on the right,
  visible as cards bleeding past the window boundary at wide window sizes.
  Moved the gutter to symmetric `Loader` margins instead.
- Mixer EQ/DSP tabs: `GraphicEQ.qml` had no card wrapper at all, unlike its
  `DspControls.qml` sibling on the same Chat tab (Noise Reduction/Gate/
  Compressor, each in a bordered card) — it floated directly on the bare
  page background. Wrapped it in a matching bordered card, added an
  "Enabled" label next to the previously-unlabeled EQ toggle, and gave the
  "Flat" reset button a hover state matching other outlined buttons.
- Settings → Notifications: the "Position" field was the only field in
  Settings still using a plain, unstyled label (14px, not bold) and a raw
  ComboBox instead of the SelectField + uppercase-label pattern Clip
  Preferences established. Gave it a real "Display" card title and moved
  Position/Duration onto that pattern.
- Fixed "clips"/"Used" stat-label casing mismatch in Storage's Disk Usage
  card (English locale only — Arabic has no case distinction).
- Added `--panel <tab>` support to the Mixer page's screenshot path
  (`MixerPage.qml`), matching Settings' existing convention, so EQ/DSP tabs
  can be captured headlessly for review.

**Why:**
User asked for a design consistency pass (spacing, icons, text) across
Settings and Mixer, after flagging the settings right-edge overflow via a
live screenshot at 1920×1080. Rather than guess app-wide, scoped to
Settings + Mixer via AskUserQuestion, then surveyed every panel with
`ui-shots.sh`-adjacent screenshots to find concrete inconsistencies (missing
cards, unlabeled controls, mismatched patterns) instead of restyling things
that were already fine.

**Deliberately not touched:** Capture & Sound's label-left field rows
(Quality/FPS/Replay Buffer/Monitor Target) use a different, but internally
consistent, "label left of control" pattern rather than Clip Preferences'
"uppercase label above control" pattern. Both are legitimate settings-UI
conventions; converting one to the other across a 4-row/2-toggle card
control risked regressions for a stylistic judgment call, not a bug — left
as-is.

---

### [2026-08-04] Claude Opus 5 — qt6-gstreamer-player-b3 (Language panel redesign)

**What Changed:**
- Settings → Language: replaced the ComboBox dropdown with the original
  Vue design's selectable list of language rows (2-letter code badge, name,
  LTR/RTL tag), matching `LanguageSettings.vue` faithfully. Active language
  gets an accent border + tinted background; clicking any row calls
  `I18n.applyLanguage()` directly.
- Added `I18n.languageDir(code)` (`src/i18n.rs`) — the existing
  `available_languages()`/`rtl` surface only exposed the *current* language's
  direction; the per-row LTR/RTL badge needed direction for every language in
  the list, not just the active one.

**Why:**
User supplied side-by-side screenshots of the old Tauri/Vue Language and
Shortcuts pages vs. the current Qt port and asked to close the gap. Shortcuts
already matched closely (header, divider, reset button, keybind pills);
Language was the real gap — a plain dropdown instead of the original's list.

**Deliberately not ported:** the Vue original's two extra header icon
buttons (open a user-locales folder in the file manager; hot-reload
JSON locale files dropped there at runtime). Neither has a backend
equivalent in the Qt port — `qt-shell` only loads locales once at startup
from `OPENGG_LOCALES_DIR`. Porting the buttons faithfully would mean adding
that runtime-reload feature first, which is out of scope for a visual-parity
pass; flagging here rather than shipping dead buttons.

---

### [2026-08-04] Claude Opus 5 — qt6-gstreamer-player-b3

**What Changed:**
- Settings → General: added a native `ColorDialog` color picker (click the
  accent swatch) instead of hex-only entry; dark/light-mode and reload icons
  are now button-shaped (background + border) instead of bare floating icons;
  Clip Preferences' segmented pills became two labeled `SelectField` dropdowns
  (`defaultClickAction`, `dateFormat`) — both were previously write-only
  settings, now wired to actually change clip-card click behavior and enable
  date-based clip search; Diagnostics' "Open Crash Logs Folder" is now an
  outlined button matching the reference design.
- New `SettingsCard.qml` component: title + divider + body, used to give
  every settings-panel card section a consistent underlined title. Migrated
  simple single-title cards across all 11 panels to it, and manually inserted
  the same divider into cards with bespoke header rows (a toggle, a link, a
  reset button) that don't fit the plain-title shape.
- Root-caused and fixed the real settings-card width cap: `SettingsPage.qml`'s
  `Loader` was pinned to `Layout.preferredWidth: 680`; also removed 21
  additional `Layout.preferredWidth: 680/640` caps scattered across individual
  card `Rectangle`s in 10 panel files. Cards now span the available width at
  any window size.
- Reverted a leftover debug-only window-size hook in `Main.qml`.

**Why:**
User feedback on four points: no color picker, non-functional-looking
dark/light toggle buttons, a non-functional Clip Preferences section that
also needed a redesign, and inconsistent/non-underlined section titles across
every settings panel except Ear Blast Protection.

**Landmines & Discoveries:**
- **`Loader` has no `resizeMode` property.** It sizes itself around its
  loaded item, not the other way around — there is no "stretch the loaded
  item to the Loader's width" mode. Assigning a `resizeMode` (a hallucinated
  API) is a hard QML load failure that takes down the whole app; the fix is
  `width: parent.width` on the loaded item's own root, same as
  `settingsContentCol` in `SettingsPage.qml` already does for the identical
  reason (not being a Layout child of anything).
- A QML load failure from a bad property assignment can look exactly like a
  hung headless screenshot run — the process never exits, and truncated
  `2>&1 | tail` output hides the actual `QQmlApplicationEngine failed to load
  component` error. Always re-run with `timeout N` and full untruncated
  output before treating an unexplained hang as a real performance issue.

---

### [2026-08-03] Claude Opus 5 — qt6-gstreamer-player-b3

**What Changed:**
- `21f721e`: `drop_video_pad`'s fakesink now sets `sync=true`; `ChannelStrip`
  clamps its fader height and gains a `compact` mode; `Main.qml` gains minimum
  window dimensions.
- CI now builds and tests the Qt shell (clippy `-D warnings`, `cargo test`,
  `check-colors.sh`, `ui-shots.sh` with screenshot artifacts) in an Arch
  container. The archived Tauri/Vue jobs — the only red checks on the board —
  are removed from `ci.yml` and `security.yml`.
- `SegmentedToggle.qml` replaces the per-corner-radius approach used for the
  Clips grid/list pill.

**Why:**
Two user-reported bugs (playback racing to the end of a clip after a skip;
UI elements overlapping in a quarter-screen window), plus the discovery that
CI gated exclusively on archived code and never built the shipping UI.

**Landmines & Discoveries:**
- **GstBin folds POSITION queries by taking the MAXIMUM across sinks.** A
  `fakesink` defaults to `sync=false` and so consumes decoded video as fast as
  the file reads; that runaway pad, not the audio, then becomes the pipeline's
  reported position. Measured: 5117ms of media per 400ms of wall time, versus
  398-401ms with `sync=true`. Any discard sink in a pipeline whose position is
  ever queried MUST set `sync=true`.
- **A negative height in a QML `Column` does not clamp — it stacks subsequent
  children BACKWARDS.** `ChannelStrip` sized its fader as `strip.height - 168`,
  which inverted below 168px and drew the channel name on top of the volume
  readout. Always `Math.max(0, ...)` a computed height.
- `minimumWidth`/`minimumHeight` on a Window are only hints; tiling
  compositors (Hyprland, sway) size from their own layout and ignore them, so
  components must degrade on their own regardless.
- **Per-corner radii (`topLeftRadius` and friends) require Qt 6.7**, newer than
  Debian stable's Qt, which the distro matrix builds against. A clipping
  rounded parent gets the same result portably.
- The bug was found by measuring, not reading: an earlier plausible-sounding
  theory (that `videoActive` was false, leaving the drift timer running) was
  disproved by checking that `no-more-pads` fires after all pads are added.

**Verification:**
- Regression test `playback_advances_at_wallclock_speed_before_and_after_a_seek`
  asserts ~1x advance; confirmed it FAILS (21516ms) with the fix reverted.
- `cargo test` 16 passed; `cargo clippy --all-targets -- -D warnings` clean
- `./tools/check-colors.sh` clean; `./tools/ui-shots.sh` zero QML warnings
- Before/after offscreen captures of the mixer at 928x492 (the reported
  quarter-screen size) confirm the overlap is gone

---

### [2026-08-02] Claude Sonnet 5 — qt6-gstreamer-player-b3

**What Changed:**
- `728694a`: `ClipAudioMixer.load()` gained a `want_video` param — the editor
  was requesting a GL video branch it had no `ClipVideoSurface` for, stalling
  the pipeline; `VideoPlayer.qml`'s drift-correction timer now skips while
  `videoActive` is true, since the recurring seek was hitting the now-visible
  GStreamer video branch and made playback visibly repeat/jump after a skip;
  the preview volume slider's `value:` binding (severed by the first drag,
  same landmine as the clips-per-row slider) is fixed the same way; list-view
  thumbnails grew 78×44 → 140×79; `IconToggle` gained a `segment` prop so the
  grid/list toggle renders as one fused pill; `RecordingControl`'s dropdown
  now matches the Home dashboard's richer recorder panel (status line,
  Start/Save buttons, Quality/FPS/Buffer/Target grid).

**Why:**
User-reported regressions from B2 (editor audio glitches, video stutter on
seek) plus a batch of Clips-page design-parity feedback, all live-tested
against a reference screenshot of the intended look.

**Landmines & Discoveries:**
- `ClipAudioMixer.load()`'s `want_video` decision cannot be made from
  `gl_video_available()` alone — it has to be per-caller. A video branch with
  no `ClipVideoSurface` to attach to leaves `qml6glsink` with no widget,
  which stalls pipeline state changes and can take the *audio* down with it
  even though the caller only wanted sound.
- The dual-clock drift-correction timer (`ClipAudioMixer.seek()` every 400ms
  on >120ms drift) was written when the mixer only ever owned audio. Once B2
  made it also own the picture, the same timer was periodically re-seeking
  the now-visible video — this is the concrete, user-visible instance of the
  "entire class of bug" B3 is meant to delete outright.

**Verification:**
- `cargo build`, `cargo test` (15 passed)
- `./tools/check-colors.sh` clean
- `./tools/ui-shots.sh` — all pages, zero QML warnings
- Manual offscreen screenshots of Clips grid/list/recording-menu states,
  compared directly against the user's reference screenshots

---

### [2026-08-01] Claude Opus 5 — qt6-migration (Phase B1 spike)

**What Changed:**
- `efe32de`: marked plan slice 2a (icons) as user-owned so contributors start at 2b
- `914861c`: `qt-shell/spikes/qml6glsink/` — standalone C++ harness proving the
  unified GStreamer player, plus the plan's B1 section rewritten with findings

**Why:**
Phase B (one GStreamer pipeline owning both picture and sound) is the committed
direction for clip playback. Its two risks had to be settled before building
anything: the QQuickItem-pointer handoff, and whether it survives the offscreen
screenshot harness.

**Landmines & Discoveries:**
- The QML type is **`GstGLQt6VideoItem`**, not `GstGLVideoItem` — the latter is
  the Qt5 name that essentially every online example still uses. Wrong name
  fails with `"GstGLVideoItem is not a type"`; note it says *is not a type*,
  not *module is not installed*, which is the tell that the module resolved.
- The type registers from inside `gst_element_register_qml6glsink`, not from a
  qmldir (Arch ships only `libgstqml6.so`), so the GStreamer plugin must be
  loaded before the QML engine resolves the import.
- `QQuickWindow::setGraphicsApi(OpenGL)` must precede any window, and the
  pipeline must reach PLAYING only after `sceneGraphInitialized` — otherwise
  `GST_STATE_CHANGE_FAILURE` (0) instead of `ASYNC` (2).
- **`qml6glsink` cannot run under `QT_QPA_PLATFORM=offscreen`** ("Could not
  initialize window system"). It is the window system, not the GL driver, so
  software GL does not rescue it. The real player must degrade to a poster
  frame without GL, or `ui-shots.sh` breaks on the player and editor pages.
- The debugging itself hit this repo's own documented landmine: without
  `QT_FORCE_STDERR_LOGGING=1` the QML error printed *nothing at all*.

**Verification:**
- Spike run against a real 3-track capture on Wayland: `ASYNC` state change,
  `GST_PAD_LINK_OK`, and a screenshot showing actual decoded video frames.
- Same binary under `offscreen`: reproducible bus error, captured verbatim.

---

### [2026-08-01] Claude (main session, Sonnet 5 → Opus 5 → Fable 5) — qt6-migration

**What Changed:**
- `fee433c`: Mixer round 2 — toolbar/borders/ChatMix/Ear-Blast + two real backend bugs
  (stale deployed daemon binary; Mic mute now also mutes @DEFAULT_SOURCE@; Mic
  device list split from the capture-source list)
- `060c54b` / `05cd520` / `99ebf66`: Clips rebuild — multi-select, list/date views,
  stats bar, toolbar, full player transport, dedicated editor page
- `ee94d40` + `3ff1ccd`: GStreamer multi-track audio mixer (all tracks at once,
  per-track gain) + the ownership/sync/trim/z-order regression fixes
- `cfb0368`: Export settings dialog (target size, codec, two-pass encode in core)
- `39ad78e` / `af67442` / `01bb419`: editor design-gap analysis + plan of record —
  **decision: a single GStreamer pipeline (qml6glsink) will replace Qt Multimedia
  for clip playback entirely**; icons fix = Shape.CurveRenderer
- `89c6c71`: review fixes for the switchover-agent's work (fabricated commit
  hashes in this file's first entry, misleading Layout rules in AGENTS.md,
  make install ETXTBSY)

**Why:**
User feedback rounds on the Mixer and Clips pages, then the switchover decision:
qt-shell becomes the launched UI and GStreamer the committed playback direction.

**Landmines & Discoveries:**
- Everything durable was folded into AGENTS.md; the deep narrative lives in the
  session memory and the plan/gap docs under docs/.
- Subagent lesson: an agent that ingests very large files can stall the stream
  watchdog (two 600s stalls); inline the needed facts into the prompt instead.
  And verify agent-written logs against `git log` — the first draft of this
  file's older entry cited four commits that did not exist.

**Verification:**
- Full suite per slice: cargo clippy + test (core/daemon/qt-shell),
  check-colors.sh, ui-shots.sh (zero QML warnings), screenshot review,
  live relaunches; mixer pipeline verified against a real 3-track capture;
  export path verified by a real 1s stream-copy test.

---

### [2026-08-01] Claude Fable 5 — qt6-migration

**What Changed:**
- `a47b6e3`: Archive the Tauri UI in place; qt-shell becomes the default build/dev flows
  - Updated Makefile lint recipe to run clippy on qt-shell + check-colors.sh
  - Fixed dev.sh run_frontend build guard (replaced `& wait $!` pattern with proper `if ! cargo build`)
  - Added ui-legacy flow documentation to dev.sh help text
- `1f38bb7`: Point the launcher at the Qt shell binary
  - Reordered opengg-launch.sh candidate search: qt-shell/target/release/opengg-qt first
  - Reverted packaging/*.desktop StartupWMClass changes (were incorrect; left unchanged)
- `1825dbe`: README: reflect the Qt6/QML architecture and switchover
  - Rewrote architecture section: Qt6/QML + cxx-qt, shared opengg-core, daemon, legacy UI archived
  - Updated Requirements: Qt 6, GStreamer + gst-plugin-qml6, removed Tauri/Node from mandatory build tools
  - Reflected all verified build/run commands from fixed Makefile and dev.sh
  - Kept AUR install, data locations, and troubleshooting sections (still accurate)
- `b31af5f`: Add AI contributor guide and session changelog convention
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

# Learning from Kairos

[Kairos](https://github.com/BasilForge/Kairos) is a Windows instant-replay app (C#/.NET 10, WPF, hosting libobs, about 36k lines of code plus 67 test files). Its scope overlaps OpenGG's "Moments" module almost exactly: a replay buffer, saving clips with a hotkey, a library, an editor, and sharing.

This document records what Kairos does better, what OpenGG adopted, how it was adopted, and what is still worth doing.

## Licence: clean-room only

Kairos is **GPL-2.0-or-later**; OpenGG is **MIT**. Copying Kairos code, or translating it line by line into Rust, would make that part of OpenGG a derivative work under the GPL. So the integration was done clean-room:

1. **Analysis.** Agents read every Kairos source and test file. They wrote **behavioural specs**: rules, thresholds, formulas, ffmpeg filter shapes, edge cases, and the reasons given in comments. They wrote no code.
2. **Implementation.** OpenGG code was written from those specs only. The agents that wrote `core/src/edit` and `core/src/share` were told not to open the Kairos tree at all.
3. **What carries over.** Constants that are facts about behaviour carry over: a 30 s + bytes÷40 MiB/s save timeout, colour-look filter parameters, the platform limits.

The ONNX models Kairos uses for Smart Save (Silero VAD, MIT; openWakeWord, Apache-2.0) and whisper.cpp (MIT) are separately licensed. They would be compatible with OpenGG if adopted later, with attribution in `THIRD_PARTY_LICENSES.md`.

## How the two compare

| Area | Kairos | OpenGG before | Now |
|---|---|---|---|
| Recording engine | libobs inside the app | gpu-screen-recorder subprocess | unchanged (GSR is the right engine on Linux) |
| Saving a clip | save gate (off / warming up / busy), write watchdog, empty-file, full-disk and low-disk checks | fixed 5 s poll for a newer file, could take a half-written file, ran on the UI thread, no reasons given | gate + finished-file detection + verdicts, off the UI thread, every failure notified |
| Clip names | `<App> <date>`, folder per app, " (n)" on collision | spaces → `_`, parsed back at the first `_` ("Elden Ring" → "Elden"); same-second saves overwrote each other | names keep spaces, the lister parses up to the timestamp (old names are repaired), " (n)", optional per-game folders |
| Which game | product → process → title, shells → "Desktop" | X11-only `xdotool` window title; every clip on Wayland was "Unknown" | Hyprland / Sway / kdotool / xdotool, plus `SteamAppId` resolved to the store name through the local Steam manifests, plus a newest-Steam-game fallback for GNOME on Wayland |
| Storage | folder size limit that only touches its own files | none | same rule, enforced via an `owned_clips` table in the clips DB |
| Recorder resilience | retry after a failed build, rebuild after resume, delay right after boot | `gsrAutoStart` / `gsrAutoRestart` saved but never read | supervisor: auto-start (boot delay), crash restarts with a budget, restart after suspend |
| Screenshot hotkey | yes | bound, with no handler | grim / spectacle / gnome-screenshot / maim / import / x11grab, chosen per session |
| Trim / export | stream mapping, keyframe-safe, part files | **no `-map`: one audio track kept, sometimes no video**; slow output-side seek; files written in place | every track kept, input-side seek, part file then rename, `+faststart`, correct multi-track size budget |
| Editor model | multi-part project, per-part speed and levels, looks, undo with gesture merging | one trim window in QML | `core/src/edit`: full model, undo, graph builder, cancellable export (UI uses part of it, see roadmap) |
| Effects on export | looks, speed, per-track levels | preview-only volumes | look, speed, "Use my track mix", optional Mix track |
| Sharing | per-platform planner, 9:16 crop, size-capped encode with retries, captions, web posting | copy path only | `core/src/share` + Share sheet: platforms, crop, resolution ladder, tracks, file-to-clipboard copy, upload page + caption |
| Settings | atomic writes, keeps `.bad.json`, clamps values | plain write; a corrupt file silently became `{}` and was then saved over the user's file | atomic writes + `.bad.json` backup |
| Library performance | incremental refresh, thumbnails only for visible cards | a full rescan + model reset per finished thumbnail; Home and Mixer polled pactl every 2 s forever | thumbnails patched in memory, one view update per batch; polling only while the page is visible |
| Voice trigger (Smart Save) | VAD + wake-word embeddings + matching against the user's recorded phrase | none | roadmap |
| Auto captions | whisper.cpp on the GPU, speech-only decoding | none | roadmap (needs an editor text layer first) |
| i18n / a11y | CLDR plurals, catalogue tests, screen-reader names, contrast-checked tokens | no plurals, some hard-coded English, no `Accessible.*` | roadmap |

Where OpenGG was already ahead, it was kept as is:

- PipeWire per-channel tracks (Kairos has a fixed mix / system / mic layout).
- Waveform lanes in the editor.
- The 2-pass size-target export.
- A multi-select game filter with counts.

## What landed, module by module

### `core`

| Module | Purpose |
|---|---|
| `clips/organizer.rs` | Sanitise names, build `<Game>_<stamp>.<ext>` targets, collision-free moves across filesystems, read the game back from all three naming schemes, choose the group folder. |
| `clips/save_watch.rs` | Pure save gate (`Off` / `Starting` / `Busy` / `Started`), write limit, verdicts, low-disk rate limiting, `statvfs`. |
| `clips/retention.rs` | Owned-file index, oldest-first selection that never picks the newest file, removal through an injectable Trash call. |
| `foreground.rs` | Focused window per compositor, Steam app id from `/proc/<pid>/environ`, store names from `appmanifest_*.acf`, `.desktop` names. |
| `supervisor.rs` | Clock-injected decisions: restart, give up, restart after resume; boot delay. |
| `desktop_shot.rs` | Screenshot tool per session, written through a part file. |
| `gsr.rs` | `save_gsr_replay(restart, &SaveOptions) -> Result<SaveReplayResult, SaveFailure>`; waits until GSR no longer holds the file open (`/proc/<pid>/fd`) and its size is stable. |
| `media.rs` | `trim_clip` / `export_clip_sized` keep every track, seek before the input, write part files. |
| `edit/` | `EditProject` (parts, speed, levels, mutes, looks), `History`, `graph::build` (N tracks, chained `atempo`, look windows, Mix track), `export` (NVENC → VAAPI → x264), `simple` (the export dialog's options as an edit). |
| `share/` | `planner` (platform specs, trim fitting, 9:16 crop, size ladder, bitrate / fps, retry rule), `transcode` (probe, plan, render), `clipboard` (`text/uri-list`, lossless cut), `hashtags`. |
| `settings` | `write_atomic`, `load_json_guarded`. |

### `qt-shell`

- **RecordingController**: asynchronous `save()` with `saving` / `lastSaveJson`; `autoStart()`, `supervise()`, `screenshot()`.
- **EditorController**: `exportEdited`, `cancelExport`, `describeEdit`, `lookIds`.
- **New `ShareController` and `ShareDialog.qml`**, opened from the clip context menu and the editor's top bar.
- **Export dialog**: an Effects section.
- **Storage panel**: "A folder per game" and "Folder size limit".
- **Screenshot harness**: a `--dialog` flag, plus `editor:export` and `clips:share` targets.

### Verification

- **Core tests:** 197 pass. They include real-ffmpeg tests on generated multi-track clips: trims keep every track, effects exports have the right length, size targets are met, cancelling leaves no files, share renders fit their size cap.
- **Screenshot sweep:** 26/26 captures with zero QML warnings.
- **Screenshots on X11:** checked on Xvfb.

## Roadmap

These are ranked by value for the effort, and each builds on what has landed.

1. **Multi-part timeline in the editor.** The engine is done and tested; this is QML work. It needs:
   - split, delete and per-part speed on the timeline (`EditProject::split_at`, `delete_parts`, `set_part_speed`);
   - `History` driving undo instead of the QML trim array;
   - preview that skips cut-out parts (seek at each part boundary).

   Keep the waveform lanes and slice each part's peaks by source time.
2. **Live status on Home.**
   - **Tiles:** buffered seconds, recorder memory (VmRSS of GSR), and codec • height • fps.
   - **Save feedback:** show `lastSaveJson` reasons in the recorder card, not only in notifications.
3. **Hotkey hygiene.**
   - Suspend the daemon's hotkeys while the Shortcuts panel records a new key (today, pressing the current save key there saves a clip).
   - Warn about duplicates between OpenGG's own actions.
   - Read desktop shortcut configs (KDE `kglobalshortcutsrc`, GNOME `gsettings`, `hyprctl binds`) to report keys held by another app.
4. **Full recording with pause**, alongside the buffer (GSR `-ro`, SIGRTMIN / SIGUSR2; check the installed GSR version). Split today's "toggleRecording", which toggles the buffer, into two actions.
5. **Screenshots in the library**, with a Videos / Screenshots filter and an image viewer whose arrows step through screenshots only.
6. **i18n and accessibility.**
   - CLDR plural rules (`I18n.tn`) with catalogue tests: key parity, placeholder parity, plural coverage.
   - `Accessible.name` / `role` on the shared components.
   - A WCAG contrast test on Theme tokens.
7. **Smart Save (voice trigger)** in the daemon.
   - **Audio input:** `pw-record` at 16 kHz mono.
   - **Pipeline:** Silero VAD → speech segmenter → openWakeWord mel + embedding models (`tract-onnx`, or `ort` with dynamic loading) → matching against three enrolled recordings of the user's phrase, with a per-user threshold and a 10 s cooldown.
   - **Trigger:** on a match, emit the existing `HotkeyPressed("saveReplay")` signal, so the app's save path is reused unchanged.
   - Models are about 3 MB, downloaded with consent and verified by SHA-256.
8. **Auto captions** with the `whisper-cli` subprocess.
   - Decode only the speech regions found by the VAD, map times back to the clip, then clean, merge and split lines at 42 characters.
   - Needs a text layer in the editor first.
9. **Quality options.** Bitrate scaled by `(pixel rate ratio)^0.75` per codec, a cap of a quarter of RAM for the buffer, a codec fallback chain via GSR `-k`, and resolutions limited to the monitor's.
10. **Library refresh by diff:** keep unchanged rows and insert / remove rows instead of resetting the model.

Not planned:

- **Kairos's in-app web posting.** It drives the X / TikTok / Instagram pages in a hidden browser, which is fragile and breaks site terms. Opening the upload page with the caption on the clipboard covers most of the value.
- **NVIDIA / OBS hotkey heuristics.** These are Windows-only.
- **libobs.** GSR stays the engine on Linux.

## Landmines found on the way

- **ffmpeg keeps one audio stream by default.** Every OpenGG ffmpeg call that copies or re-encodes clips must pass `-map 0:v:0 -map 0:a?` (or explicit labels). The missing map silently removed tracks from every trimmed clip.
- **`-ss` / `-t` before `-i`.** An output-side seek with `-c copy` can produce no video at all on some inputs. With re-encoding it decodes the file from 0.
- **The MP4 muxer may drop `title=` stream metadata.** It keeps `handler_name`. The edit exporter writes both; readers already fall back to `name` / `handler_name`.
- **Renaming a part file into place is `Modify(Name)`, not `Create`.** The clip watcher now listens for it, and skips dotfiles so part files never appear in the library.
- **QML handlers can see a stale binding.** Inside `onClipChanged`, a property bound to the clip can still hold the previous clip's value. Read from the new object directly.
- **A failed build looks like success to the screenshot harness.** `ui-shots.sh` screenshots whatever binary exists, so always `cargo build` first and check the result. A duplicate `onVisibleChanged` failed qmlcachegen, and the stale binary still produced clean screenshots.

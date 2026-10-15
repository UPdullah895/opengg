import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtMultimedia
import com.opengg.app

// ClipEditorPage — the clip editor as a real page, replacing the TrimEditor
// modal overlay. Port of AdvancedEditor.vue's layout: preview + transport on
// the left, a media INFO panel on the right, and a multi-lane timeline
// (video + one lane per audio track) with draggable trim handles underneath.
//
// AUDIO LANES: the lanes are labelled with the clip's real track names, which
// only exist in the mp4 `name` tag (see core::media's parser). Every track
// plays SIMULTANEOUSLY through ClipAudioMixer's GStreamer pipeline, so each
// lane's speaker button mutes that one track in the live mix. Qt Multimedia
// only ever supplies the picture here — it can decode one audio stream at a
// time and cannot mix. Export is a lossless stream copy and keeps every track
// regardless of what is muted for monitoring.
Rectangle {
    id: page
    color: Theme.bg

    // Media editors stay left-to-right in an RTL language: time runs left to
    // right on the timeline, the transport and trim handles keep their usual
    // places, and the scrub bar fills the same way. Mirroring the whole page
    // put the track labels on the right while the trim overlay still
    // measured from the left, so the handles sat off their marks. Text
    // itself is still shaped and translated as usual.
    LayoutMirroring.enabled: false
    LayoutMirroring.childrenInherit: true

    /// {filepath, title} of the clip being edited, or null.
    property var clip: null
    signal closed()

    readonly property string filepath: page.clip ? page.clip.filepath : ""
    readonly property var info: JSON.parse(EditorController.mediaInfoJson || "{}")
    /// Waveform peaks keyed by ffmpeg stream index, mirrored from the
    /// controller. Each audio lane requests its own; they land one at a time.
    property var waveforms: ({})
    Connections {
        target: EditorController
        function onWaveformsJsonChanged() {
            page.waveforms = JSON.parse(EditorController.waveformsJson || "{}")
        }
    }

    readonly property var audioStreams: (page.info.streams || [])
        .filter(function (s) { return s.codec_type === "audio" })

    // ── Timeline track definitions (Settings → Timeline Tracks) ───────────
    // The lanes below used to label themselves from the ffprobe stream title
    // and colour themselves by guessing a channel from it, so a capture whose
    // streams are named after their PipeWire sources showed three lanes all
    // called "Devices…" in the wrong colours — ignoring the names and colours
    // the user had configured. Read `trackDefs` and use it as the source of
    // truth, falling back to the stream title only when a slot is unset.
    property var trackDefs: (JSON.parse(SettingsController.settingsJson || "{}").trackDefs) || []
    Connections {
        target: SettingsController
        function onSettingsJsonChanged() {
            page.trackDefs = (JSON.parse(SettingsController.settingsJson || "{}").trackDefs) || []
        }
    }

    /// The trackDefs entry for slot `id` ("V1", "A1", "A2", …), or null.
    function trackDef(id) {
        for (var i = 0; i < page.trackDefs.length; i++)
            if (page.trackDefs[i].id === id)
                return page.trackDefs[i]
        return null
    }
    /// A track name for display: the built-in role names ("Game", "Mic", …)
    /// and the empty Overlays/Video defaults are shown translated; anything
    /// the user typed is shown as-is. Track data keeps the English role name
    /// so it stays stable across language switches.
    function trackLabel(def) {
        const n = (def && def.name) || ""
        if (n.length === 0 || n === "Overlays" || n === "Video") {
            if (def && def.id === "O1") return I18n.t("editor.overlayTrack")
            if (def && def.id === "V1") return I18n.t("editor.videoTrack")
        }
        const k = "mixer.channels." + n.toLowerCase()
        const t = I18n.t(k)
        return t !== k ? t : (n || (def ? def.id : ""))
    }

    /// Map a trackDefs icon key onto this shell's icon registry.
    function trackIcon(key, fallback) {
        switch (key) {
        case "video":   return "track-video"
        case "game":    return "track-game"
        case "chat":    return "headphones"
        case "mic":     return "track-mic"
        case "media":   return "track-media"
        case "overlay": return "track-overlay"
        }
        return fallback
    }

    property real trimStart: 0
    property real trimEnd: 0
    readonly property real duration: EditorController.duration

    focus: visible

    // ── Audio ownership ───────────────────────────────────────────────────
    // ClipAudioMixer plays every audio track simultaneously with independent
    // gain, which is the whole point of a multi-track editor; Qt Multimedia is
    // muted down to the picture. Single-track clips fall back to Qt's audio.
    readonly property bool mixed: ClipAudioMixer.active
    /// Ownership token from ClipAudioMixer.load — see the note there.
    property var mixerToken: null
    /// Per-track mute flags. Reassigned wholesale so bindings re-evaluate.
    property var trackMuted: ({})
    /// Per-track gain 0..1, independent of mute. Reassigned wholesale too.
    property var trackVolume: ({})

    function trackGain(index) {
        const v = page.trackVolume[index]
        return v === undefined ? 1.0 : v
    }
    function setTrackGain(index, v) {
        var next = {}
        for (var k in page.trackVolume) next[k] = page.trackVolume[k]
        next[index] = Math.max(0, Math.min(1, v))
        page.trackVolume = next
        // Turning a muted track up is an implicit unmute.
        if (next[index] > 0 && page.trackMuted[index] === true) {
            page.toggleTrack(index)
            return
        }
        if (page.mixed && page.trackMuted[index] !== true)
            ClipAudioMixer.setTrackVolume(index, next[index])
    }
    /// Master mute. Applies to the mix when mixed, to Qt's output otherwise.
    property bool masterMuted: false
    /// Output level, owned by the page rather than read back off audioOut:
    /// while mixed, Qt's output is muted by design and its `volume` says
    /// nothing about what you can actually hear.
    property real masterVolume: 1.0

    function applyVolume() {
        if (page.mixed)
            ClipAudioMixer.setMasterVolume(page.masterMuted ? 0 : page.masterVolume)
        else
            audioOut.volume = page.masterVolume
    }

    // ── Saved editor state ────────────────────────────────────────────────
    // The trim window, volume and per-track gains are saved per clip as they
    // change, and restored when the clip is reopened. Previously the trim
    // was only written on export, so leaving the editor first reset it to
    // the whole clip, and the volume always came back at maximum.
    //
    // A clip with no saved volume starts at the last volume used in the
    // editor rather than at 100%.
    property bool stateLoaded: false
    /// The file the current trim/volume belong to. Kept separately from
    /// `clip`, which has already moved on by the time onClipChanged runs.
    property string stateFilepath: ""
    function saveState() {
        if (!page.stateLoaded || page.stateFilepath.length === 0)
            return
        EditorController.saveState(page.stateFilepath, page.trimStart, page.trimEnd,
                                   page.masterVolume, JSON.stringify(page.trackVolume))
    }
    Timer { id: saveDebounce; interval: 500; onTriggered: page.saveState() }
    onTrimStartChanged: if (page.stateLoaded) saveDebounce.restart()
    onTrimEndChanged: if (page.stateLoaded) saveDebounce.restart()
    onTrackVolumeChanged: if (page.stateLoaded) saveDebounce.restart()
    onMasterVolumeChanged: {
        if (!page.stateLoaded)
            return
        saveDebounce.restart()
        SettingsController.setValue("editorVolume", JSON.stringify(page.masterVolume))
    }
    /// Write any pending change now — before the clip or page goes away.
    function flushState() {
        if (saveDebounce.running) {
            saveDebounce.stop()
            page.saveState()
        }
    }
    /// Hides the info panel and timeline so the picture fills the page.
    property bool theaterMode: false
    /// Collapses the app's nav rail while the editor is open, without going
    /// all the way into full view. Main.qml reads this alongside theaterMode.
    ///
    /// Defaults to HIDDEN: the editor is where horizontal space is scarcest
    /// and the rail is not useful while trimming. The toggle writes the
    /// user's choice to ui-settings, so opting back in sticks.
    property bool navHidden: page.settings.editorNavHidden !== false
    readonly property var settings: JSON.parse(SettingsController.settingsJson || "{}")

    // ── Resizable panes ───────────────────────────────────────────────────
    // The info sidebar's width and the timeline's height are dragged by the
    // user and remembered in ui-settings. A timeline height of 0 means "fit
    // the lanes", which is also the default.
    readonly property int sidebarMinW: 180
    readonly property real sidebarMaxW: Math.max(page.sidebarMinW, Math.min(480, page.width * 0.4))
    property real sidebarW: Math.max(page.sidebarMinW,
                                     Math.min(page.sidebarMaxW, page.settings.editorSidebarW || 232))
    property real timelineUserH: page.settings.editorTimelineH || 0
    readonly property int laneCount: 1 + page.audioStreams.length
    readonly property int laneMinH: 30
    /// Lanes + the pane's margins, at the minimum lane height.
    readonly property real timelineMinH: 30 + page.laneCount * page.laneMinH
                                         + (page.laneCount - 1) * 4
    readonly property real timelineMaxH: Math.max(page.timelineMinH, page.height * 0.65)
    readonly property real timelineH: page.timelineUserH > 0
        ? Math.max(page.timelineMinH, Math.min(page.timelineMaxH, page.timelineUserH))
        : Math.min(196, page.timelineMinH + 20)
    /// Each lane shares whatever height the timeline has been given.
    readonly property real laneH: Math.max(page.laneMinH, Math.min(140,
        (page.timelineH - 30 - (page.laneCount - 1) * 4) / Math.max(1, page.laneCount)))

    function setNavHidden(hidden) {
        page.navHidden = hidden
        SettingsController.setValue("editorNavHidden", JSON.stringify(hidden))
    }

    // ── Full-view chrome auto-hide ────────────────────────────────────────
    /// In full view the top bar and transport fade out once the pointer has
    /// been still for a moment, and come back on the next movement — the
    /// bars used to sit over the picture permanently, which is most of the
    /// point of full view lost.
    property bool chromeHidden: false
    /// Chrome stays put while the pointer is actually on it.
    property bool chromeHovered: false

    Timer {
        id: idleTimer
        interval: 2200
        onTriggered: if (page.theaterMode && !page.chromeHovered) page.chromeHidden = true
    }
    /// Last pointer position seen, so only a REAL move wakes the chrome.
    property point lastPointer: Qt.point(-1, -1)
    HoverHandler {
        // Qt re-delivers a hover event whenever items move under a still
        // cursor — including the bars collapsing — so waking on every event
        // brought the chrome straight back and it never hid while the
        // pointer was inside the window. Compare positions instead.
        onPointChanged: {
            const p = point.position
            if (Math.abs(p.x - page.lastPointer.x) < 3
                    && Math.abs(p.y - page.lastPointer.y) < 3)
                return
            page.lastPointer = Qt.point(p.x, p.y)
            page.chromeHidden = false
            if (page.theaterMode) idleTimer.restart()
        }
    }
    onTheaterModeChanged: {
        page.chromeHidden = false
        if (page.theaterMode) idleTimer.restart()
        else idleTimer.stop()
    }

    // Clicking away from the name/game fields has to actually drop focus.
    // It did not, so the text cursor stayed in the field and I, O and F
    // went to the TextField as characters instead of reaching the editor's
    // keymap. A TapHandler on the root only sees taps no child consumed,
    // which is exactly "clicked an empty area".
    TapHandler {
        onTapped: page.forceActiveFocus()
    }
    /// Current game tag, seeded from the clip row and edited in the top bar.
    property string gameTag: ""
    /// Export settings dialog open?
    property bool exportOpen: false

    function toggleTrack(index) {
        if (!page.mixed)
            return
        var next = {}
        for (var k in page.trackMuted) next[k] = page.trackMuted[k]
        next[index] = !next[index]
        page.trackMuted = next
        ClipAudioMixer.setTrackVolume(index, next[index] ? 0.0 : page.trackGain(index))
    }

    onClipChanged: {
        // Save the outgoing clip first; its values are still in place here.
        page.flushState()
        if (!page.clip)
            return
        // Read straight off `clip` rather than through the `filepath` binding:
        // QML does not guarantee a dependent binding has re-evaluated by the
        // time the property's own onChanged handler runs, and loading with a
        // stale empty path left the whole INFO panel showing dashes.
        page.stateLoaded = false
        page.stateFilepath = page.clip.filepath
        EditorController.loadClip(page.clip.filepath)
        page.trimStart = EditorController.trimStart
        page.trimEnd = EditorController.trimEnd
        page.resetTrimHistory(page.trimStart, page.trimEnd)
        page.gameTag = page.clip.game || ""
        page.trackMuted = ({})
        const saved = JSON.parse(EditorController.savedStateJson || "{}")
        page.trackVolume = saved.trackGains || ({})
        const lastVol = page.settings.editorVolume
        page.masterVolume = saved.volume !== undefined && saved.volume !== null
                            ? saved.volume
                            : (typeof lastVol === "number" ? lastVol : 1.0)
        page.masterMuted = false
        // View state belongs to the clip you were watching, not to the
        // editor: opening a different clip used to inherit the previous
        // one's full view and collapsed rail.
        page.theaterMode = false
        page.navHidden = page.settings.editorNavHidden !== false
        // false: the editor has no ClipVideoSurface yet (its picture still
        // comes from Qt Multimedia below) — see load()'s doc comment.
        page.mixerToken = ClipAudioMixer.load(page.clip.filepath, false)
        page.applyVolume()
        // Restored gains apply to the new mix once it is loaded.
        for (var k in page.trackVolume)
            if (page.mixed) ClipAudioMixer.setTrackVolume(Number(k), page.trackVolume[k])
        page.stateLoaded = true
        // The mix is started by onPlaybackStateChanged, not here — starting it
        // alongside mp.play() ran the audio ahead of the first frame.
        mp.play()
    }

    onVisibleChanged: {
        if (visible) {
            page.forceActiveFocus()
        } else {
            // Leaving with the cursor still in the game field left its
            // suggestion list floating over whatever page came next — a
            // Popup is its own overlay and does not hide with the page.
            page.forceActiveFocus()
            page.flushState()
            mp.stop()
            // Release the audio device; a live pipeline would keep playing
            // over the rest of the app after navigating away. Token-scoped so
            // it can't tear down a mix another view already owns.
            if (page.mixerToken !== null) {
                ClipAudioMixer.release(page.mixerToken)
                page.mixerToken = null
            }
        }
    }

    /// A FLUSHING seek needs a moment before the pipeline reports its new
    /// position. Sampling inside that window reads the OLD position, looks
    /// like drift, and provokes another correction — a loop that replayed the
    /// same stretch of audio. Every seek we issue restarts this.
    Timer { id: seekSettle; interval: 600 }

    // Independent clocks drift; nudge the audio back to the video periodically.
    Timer {
        interval: 400
        running: page.visible && page.mixed
                 && mp.playbackState === MediaPlayer.PlayingState
        repeat: true
        onTriggered: {
            if (seekSettle.running)
                return
            const apos = ClipAudioMixer.positionMs()
            if (apos >= 0 && Math.abs(apos - mp.position) > 120) {
                ClipAudioMixer.seek(mp.position)
                seekSettle.restart()
            }
        }
    }

    function fmt(sec) {
        if (!sec || sec < 0) sec = 0
        var t = Math.floor(sec)
        var m = Math.floor(t / 60)
        var s = t % 60
        return m + ":" + (s < 10 ? "0" : "") + s + "." + Math.floor((sec % 1) * 10)
    }

    function togglePlay() {
        // Only the video is driven here; onPlaybackStateChanged mirrors the
        // resulting state onto the mixer.
        if (mp.playbackState === MediaPlayer.PlayingState) mp.pause()
        else mp.play()
    }
    // Seek BOTH clocks to the same target.
    //
    // This used to assign `mp.position` and then pass `mp.position` straight
    // back to the mixer. MediaPlayer.position is asynchronous: writing it
    // starts a seek, and reading it on the very next line still returns the
    // OLD position. So a skip sent the audio to where the video had just
    // been, not where it was going. The drift timer then corrected it a beat
    // later — audio stutters, then settles — which is exactly the symptom on
    // a short fast-forward. Seek both to the value we computed instead.
    function seekTo(ms) {
        const target = Math.max(0, Math.min(mp.duration, ms))
        mp.position = target
        if (page.mixed) {
            ClipAudioMixer.seek(target)
            seekSettle.restart()
        }
    }
    function skip(ms) {
        page.seekTo(mp.position + ms)
    }
    function resetTrim() {
        page.commitTrim(0, page.duration)
    }

    // ── Trim undo/redo ────────────────────────────────────────────────────
    // Ctrl+Z used to jump straight back to the full clip, which threw the
    // previous trim away with no way back — so there was nothing for a redo
    // to return to. History is an explicit stack of committed trim windows
    // with a cursor, so both directions work.
    property var trimHistory: []
    property int trimCursor: -1
    readonly property bool canUndoTrim: page.trimCursor > 0
    readonly property bool canRedoTrim: page.trimCursor >= 0
                                        && page.trimCursor < page.trimHistory.length - 1

    /// Seed the stack with the clip's loaded trim. Everything after is an edit.
    function resetTrimHistory(start, end) {
        page.trimHistory = [{ s: start, e: end }]
        page.trimCursor = 0
    }

    /// Apply a trim AND record it. Anything the cursor had stepped back past
    /// is dropped, the usual rule for editing after an undo.
    function commitTrim(start, end) {
        page.trimStart = start
        page.trimEnd = end
        const head = page.trimHistory[page.trimCursor]
        if (head && Math.abs(head.s - start) < 0.001 && Math.abs(head.e - end) < 0.001)
            return
        const next = page.trimHistory.slice(0, page.trimCursor + 1)
        next.push({ s: start, e: end })
        // A long drag session would otherwise grow without bound.
        while (next.length > 50) next.shift()
        page.trimHistory = next
        page.trimCursor = next.length - 1
    }

    /// Record wherever the trim currently sits — for drags, which write
    /// trimStart/trimEnd directly on every mouse move and only settle on
    /// release.
    function commitCurrentTrim() { page.commitTrim(page.trimStart, page.trimEnd) }

    function undoTrim() {
        if (!page.canUndoTrim)
            return
        page.trimCursor -= 1
        const h = page.trimHistory[page.trimCursor]
        page.trimStart = h.s
        page.trimEnd = h.e
    }
    function redoTrim() {
        if (!page.canRedoTrim)
            return
        page.trimCursor += 1
        const h = page.trimHistory[page.trimCursor]
        page.trimStart = h.s
        page.trimEnd = h.e
    }

    // Standard editor keymap. Space/arrows/Ctrl+Z were the whole set before;
    // J/K/L, I/O, comma/period and M are what anyone coming from another
    // editor reaches for first.
    Keys.onPressed: (e) => {
        const shift = (e.modifiers & Qt.ShiftModifier) !== 0
        switch (e.key) {
        case Qt.Key_Space:
        case Qt.Key_K:
            page.togglePlay(); e.accepted = true; break
        case Qt.Key_L:
            // Tap to play, tap again to speed up (1x → 1.5x → 2x).
            if (mp.playbackState !== MediaPlayer.PlayingState) mp.play()
            else mp.playbackRate = mp.playbackRate >= 2 ? 2
                                 : mp.playbackRate >= 1.5 ? 2 : 1.5
            e.accepted = true; break
        case Qt.Key_J:
            // No negative-rate playback on this pipeline; step back instead.
            page.skip(-2000); e.accepted = true; break
        case Qt.Key_Right:
            page.skip(shift ? 1000 : 5000); e.accepted = true; break
        case Qt.Key_Left:
            page.skip(shift ? -1000 : -5000); e.accepted = true; break
        case Qt.Key_Comma:
            page.skip(-1000 / Math.max(1, page.info.fps || 30)); e.accepted = true; break
        case Qt.Key_Period:
            page.skip(1000 / Math.max(1, page.info.fps || 30)); e.accepted = true; break
        case Qt.Key_I:
            page.commitTrim(Math.min(mp.position / 1000, page.trimEnd), page.trimEnd)
            e.accepted = true; break
        case Qt.Key_O:
            page.commitTrim(page.trimStart, Math.max(mp.position / 1000, page.trimStart))
            e.accepted = true; break
        case Qt.Key_M:
            page.masterMuted = !page.masterMuted
            if (page.mixed) page.applyVolume()
            else audioOut.muted = page.masterMuted
            e.accepted = true; break
        case Qt.Key_F:
            page.theaterMode = !page.theaterMode; e.accepted = true; break
        case Qt.Key_Home:
            page.seekTo(0); e.accepted = true; break
        case Qt.Key_End:
            page.seekTo(mp.duration); e.accepted = true; break
        case Qt.Key_Escape:
            if (page.theaterMode) page.theaterMode = false
            else page.closed()
            e.accepted = true; break
        case Qt.Key_Z:
            if (e.modifiers & Qt.ControlModifier) {
                // Ctrl+Shift+Z redoes, matching the Ctrl+Y below.
                if (shift) page.redoTrim()
                else page.undoTrim()
                e.accepted = true
            }
            break
        case Qt.Key_Y:
            if (e.modifiers & Qt.ControlModifier) { page.redoTrim(); e.accepted = true }
            break
        case Qt.Key_1: case Qt.Key_2: case Qt.Key_3:
        case Qt.Key_4: case Qt.Key_5: {
            // Number keys mute/unmute the matching audio track.
            const t = e.key - Qt.Key_1
            if (t < page.audioStreams.length) { page.toggleTrack(t); e.accepted = true }
            break
        }
        }
    }

    MediaPlayer {
        id: mp
        source: page.filepath ? "file://" + page.filepath : ""
        videoOutput: videoOut
        audioOutput: AudioOutput {
            id: audioOut
            // Silence Qt's single-track decode while the mixer owns the sound,
            // or track 1 would be heard twice.
            // Muted whenever the mixer owns the sound (its track 1 would
            // otherwise double up), and whenever the user mutes the output.
            muted: page.mixed || page.masterMuted
            volume: page.masterVolume
        }
        onPlaybackRateChanged: if (page.mixed) ClipAudioMixer.setRate(mp.playbackRate)
        // The mix follows the video's ACTUAL state; mp.play() returns long
        // before the first frame is on screen.
        //
        // Resuming used to FLUSH-seek the mix to the video's position every
        // time, even when the two were already together: the flush empties
        // the audio buffers, so every un-pause started with a short dropout,
        // and any small disagreement between the two clocks' readings came
        // out as a skip or a repeated sliver of sound. The mix now simply
        // resumes from where it paused, and only seeks when it is really
        // out of step (first play, or after a seek while paused).
        onPlaybackStateChanged: {
            if (!page.mixed)
                return
            if (mp.playbackState === MediaPlayer.PlayingState) {
                const apos = ClipAudioMixer.positionMs()
                if (apos < 0 || Math.abs(apos - mp.position) > 150)
                    ClipAudioMixer.seek(mp.position)
                // Positions read just after a resume are still settling;
                // give the drift check a moment before it may correct.
                seekSettle.restart()
                ClipAudioMixer.play()
            } else {
                ClipAudioMixer.pause()
            }
        }
        onMediaStatusChanged: {
            if (mediaStatus === MediaPlayer.EndOfMedia) {
                mp.pause()
                page.seekTo(Math.round(page.trimStart * 1000))
            }
        }
        // Playback is clamped to the trim window so the handles preview what
        // the export will actually produce.
        onPositionChanged: {
            if (page.trimEnd > 0 && mp.position > page.trimEnd * 1000 + 50)
                page.seekTo(Math.round(page.trimStart * 1000))
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // ── Top bar ───────────────────────────────────────────────────────
        Rectangle {
            id: topBar
            // 44, not 56: the bar carries a back button, two fields and a few
            // pills, none of which need that much height, and it was eating
            // vertical space the picture wants.
            Layout.fillWidth: true
            // Collapsed rather than merely transparent, so the picture
            // actually reclaims the space in full view.
            // Full view hides it outright, not after an idle delay: only the
            // playback controls stay, so the picture gets the whole window.
            Layout.preferredHeight: page.theaterMode ? 0 : 44
            visible: Layout.preferredHeight > 0
            color: Theme.surface
            border.width: 0
            Behavior on Layout.preferredHeight { NumberAnimation { duration: 120 } }
            HoverHandler { onHoveredChanged: page.chromeHovered = hovered }

            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width; height: 1
                color: Theme.border
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                spacing: 10

                Rectangle {
                    Layout.preferredWidth: 28
                    Layout.preferredHeight: 28
                    radius: Theme.radius
                    color: backArea.containsMouse ? Theme.bgHover : "transparent"
                    border.width: 1
                    border.color: Theme.border
                    Icon {
                        anchors.centerIn: parent
                        name: "chevron-down"
                        size: 14
                        color: Theme.text
                        rotation: 90     // no dedicated back glyph in the set
                    }
                    MouseArea {
                        id: backArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.closed()
                    }
                }

                // Collapse the app's nav rail without going to full view —
                // the editor is where horizontal space is scarcest, and the
                // rail is not useful while you are trimming.
                Rectangle {
                    Layout.preferredWidth: 28
                    Layout.preferredHeight: 28
                    radius: Theme.radius
                    color: page.navHidden ? Theme.accentAlpha(15)
                         : navArea.containsMouse ? Theme.bgHover : "transparent"
                    border.width: 1
                    border.color: page.navHidden ? Theme.accent : Theme.border
                    Icon {
                        anchors.centerIn: parent
                        name: "panel-left"
                        size: 14
                        color: page.navHidden ? Theme.accent : Theme.text
                    }
                    MouseArea {
                        id: navArea
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.setNavHidden(!page.navHidden)
                    }
                    Tip {
                        visible: navArea.containsMouse
                        text: page.navHidden ? (I18n.language, I18n.t("editor.showSidebar"))
                                             : (I18n.language, I18n.t("editor.hideSidebar"))
                    }
                }

                // Name and game are editable here, as in the old editor's
                // header — the Clips grid's inline rename only covers the name.
                EditorField {
                    id: nameField
                    Layout.preferredWidth: 220
                    placeholder: (I18n.language, I18n.t("editor.clipName"))
                    text: page.clip ? page.clip.title : ""
                    onCommitted: (v) => {
                        if (v.length > 0 && page.clip) {
                            ClipsController.setCustomName(page.clip.filepath, v)
                            page.clip = { filepath: page.clip.filepath, title: v }
                        }
                    }
                }
                EditorField {
                    id: gameField
                    Layout.preferredWidth: 170
                    placeholder: (I18n.language, I18n.t("editor.gamePlaceholder"))
                    text: page.gameTag
                    onCommitted: (v) => {
                        if (page.clip) {
                            EditorController.setGameTag(page.clip.filepath, v)
                            page.gameTag = v
                            ClipsController.refresh()
                        }
                    }

                    // Suggestions from the games already in the library. The
                    // field used to accept free text with no check against
                    // existing tags, so a typo silently created a new game
                    // rather than matching one you already had.
                    Popup {
                        id: gameSuggest
                        y: gameField.height + 2
                        width: Math.max(gameField.width, 200)
                        implicitHeight: gameSuggest.rows.length * 26 + 2
                        padding: 1
                        visible: gameField.editing && gameSuggest.rows.length > 0
                        closePolicy: Popup.NoAutoClose

                        /// Known games matching what has been typed, plus an
                        /// "add" row when the text is genuinely new.
                        readonly property string typed: gameField.text.trim()
                        readonly property var rows: {
                            const all = ClipsController.gameList.slice(1)
                            const t = gameSuggest.typed.toLowerCase()
                            var out = []
                            var exact = false
                            for (var i = 0; i < all.length; i++) {
                                const g = String(all[i])
                                if (g.toLowerCase() === t) exact = true
                                if (t.length === 0 || g.toLowerCase().indexOf(t) >= 0)
                                    out.push({ label: g, isNew: false })
                            }
                            out = out.slice(0, 6)
                            if (t.length > 0 && !exact)
                                out.unshift({ label: t, isNew: true })
                            return out
                        }

                        background: Rectangle {
                            radius: Theme.radius
                            color: Theme.surface
                            border.width: 1
                            border.color: Theme.border
                        }
                        contentItem: Column {
                            spacing: 0
                            Repeater {
                                model: gameSuggest.rows
                                Rectangle {
                                    required property var modelData
                                    width: gameSuggest.width - 2
                                    height: 26
                                    color: sugArea.containsMouse
                                           ? Theme.bgHover : "transparent"
                                    Row {
                                        anchors.left: parent.left
                                        anchors.leftMargin: 10
                                        anchors.verticalCenter: parent.verticalCenter
                                        spacing: 6
                                        Icon {
                                            anchors.verticalCenter: parent.verticalCenter
                                            name: modelData.isNew ? "plus" : "gamepad"
                                            size: 11
                                            color: modelData.isNew
                                                   ? Theme.accent : Theme.textDim
                                        }
                                        Text {
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: modelData.isNew
                                                  ? I18n.t("editor.addGame").replace("{name}", modelData.label)
                                                  : modelData.label
                                            color: modelData.isNew
                                                   ? Theme.accent : Theme.text
                                            font.pixelSize: 12
                                        }
                                    }
                                    MouseArea {
                                        id: sugArea
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: gameField.setValue(modelData.label)
                                    }
                                }
                            }
                        }
                    }
                }

                Item { Layout.fillWidth: true }

                // Format badges
                Repeater {
                    model: [
                        page.info.video_codec || "",
                        page.info.width ? page.info.width + "×" + page.info.height : "",
                        page.info.fps ? Math.round(page.info.fps) + "fps" : ""
                    ].filter(function (t) { return !!t })

                    Rectangle {
                        required property string modelData
                        Layout.preferredHeight: 20
                        implicitWidth: badgeText.implicitWidth + 16
                        radius: 4
                        color: Theme.bgDeep
                        Text {
                            id: badgeText
                            anchors.centerIn: parent
                            text: modelData
                            color: Theme.textMuted
                            font.pixelSize: 11
                        }
                    }
                }

                // Export
                Rectangle {
                    Layout.preferredWidth: 108
                    Layout.preferredHeight: 28
                    radius: Theme.radius
                    color: EditorController.exportRunning ? Theme.bgHover : Theme.accent
                    Text {
                        anchors.centerIn: parent
                        text: EditorController.exportRunning
                              ? Math.round(EditorController.exportProgress) + "%"
                              : (I18n.language, I18n.t("editor.exportClip"))
                        color: EditorController.exportRunning ? Theme.text : "#ffffff"
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                    MouseArea {
                        anchors.fill: parent
                        enabled: !EditorController.exportRunning && page.trimEnd > page.trimStart
                        cursorShape: Qt.PointingHandCursor
                        // Opens the settings dialog rather than exporting on the
                        // spot — filename, directory, size and codec all belong
                        // to the user, not to a default.
                        onClicked: page.exportOpen = true
                    }
                }
            }
        }

        // ── Preview + info ────────────────────────────────────────────────
        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            // Video
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                color: "#000000"
                clip: true

                VideoOutput {
                    id: videoOut
                    anchors.fill: parent
                }

                MouseArea {
                    anchors.fill: parent
                    onClicked: page.togglePlay()
                }

                Rectangle {
                    anchors.centerIn: parent
                    width: 60; height: 60; radius: 30
                    visible: mp.playbackState !== MediaPlayer.PlayingState
                    color: Theme.scrim(55)
                    Icon {
                        anchors.centerIn: parent
                        anchors.horizontalCenterOffset: 3
                        name: "play"; size: 24; color: "#ffffff"
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: page.togglePlay()
                    }
                }
            }

            // Info sidebar
            Rectangle {
                id: infoSidebar
                visible: !page.theaterMode
                Layout.preferredWidth: page.theaterMode ? 0 : page.sidebarW
                Layout.fillHeight: true
                color: Theme.surface

                // Splitter on the sidebar's inner edge.
                MouseArea {
                    id: sideSplit
                    z: 5
                    anchors.left: parent.left
                    anchors.leftMargin: -3
                    width: 7
                    height: parent.height
                    hoverEnabled: true
                    preventStealing: true
                    cursorShape: Qt.SplitHCursor
                    onPositionChanged: (m) => {
                        if (!pressed)
                            return
                        const x = mapToItem(page, m.x, 0).x
                        page.sidebarW = Math.max(page.sidebarMinW,
                                                 Math.min(page.sidebarMaxW, page.width - x))
                    }
                    onReleased: SettingsController.setValue("editorSidebarW",
                                                            JSON.stringify(Math.round(page.sidebarW)))
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 2; height: parent.height
                        color: Theme.accent
                        visible: sideSplit.containsMouse || sideSplit.pressed
                    }
                }

                Rectangle {
                    anchors.left: parent.left
                    width: 1; height: parent.height
                    color: Theme.border
                }

                ColumnLayout {
                    anchors.fill: parent
                    anchors.margins: 14
                    spacing: 10

                    Text {
                        text: (I18n.language, I18n.t("common.info").toUpperCase())
                        color: Theme.textMuted
                        font.pixelSize: 11
                        font.weight: Font.Bold
                        font.letterSpacing: 1
                    }

                    Text {
                        Layout.fillWidth: true
                        text: page.filepath
                        color: Theme.textMuted
                        font.pixelSize: 10
                        elide: Text.ElideMiddle
                    }

                    Repeater {
                        model: [
                            { k: I18n.t("editor.info.duration"),   v: page.duration > 0 ? page.duration.toFixed(1) + "s" : "—" },
                            { k: I18n.t("editor.info.resolution"), v: page.info.width ? page.info.width + "×" + page.info.height : "—" },
                            { k: I18n.t("editor.info.fps"),        v: page.info.fps ? page.info.fps.toFixed(1) : "—" },
                            { k: I18n.t("editor.info.codec"),      v: page.info.video_codec || "—" },
                            { k: I18n.t("editor.info.audio"),      v: page.audioStreams.length
                                                  ? I18n.t("editor.info.tracks").replace("{n}", page.audioStreams.length) : "—" },
                            { k: I18n.t("editor.info.trim"),       v: "\u2066" + page.fmt(page.trimStart) + " → " + page.fmt(page.trimEnd) + "\u2069" },
                            { k: I18n.t("editor.info.output"),     v: Math.max(0, page.trimEnd - page.trimStart).toFixed(1) + "s" }
                        ]

                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 6
                            Text {
                                text: modelData.k
                                color: Theme.textMuted
                                font.pixelSize: 11
                            }
                            Item { Layout.fillWidth: true }
                            Text {
                                text: modelData.v
                                color: Theme.text
                                font.pixelSize: 11
                                font.weight: Font.DemiBold
                            }
                        }
                    }

                    Item { Layout.fillHeight: true }

                    Rectangle {
                        Layout.fillWidth: true
                        height: 1
                        color: Theme.border
                    }

                    // Playback and trim only. The full keymap (J/L, frame
                    // step, 1–5, undo/redo) still works and is listed in
                    // Settings → Shortcuts; repeating all of it here turned
                    // the panel into a wall of text nobody reads.
                    Repeater {
                        model: (I18n.language, [
                            { k: "Space", v: I18n.t("editor.keys.playPause") },
                            { k: "← →",   v: I18n.t("editor.keys.skip") },
                            { k: "I / O", v: I18n.t("editor.keys.trim") },
                            { k: "F",     v: I18n.t("editor.keys.fullView") }
                        ])

                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true
                            spacing: 8
                            Rectangle {
                                Layout.preferredWidth: keyText.implicitWidth + 12
                                Layout.preferredHeight: 18
                                radius: 3
                                color: Theme.bgDeep
                                border.width: 1
                                border.color: Theme.border
                                Text {
                                    id: keyText
                                    anchors.centerIn: parent
                                    text: modelData.k
                                    color: Theme.textDim
                                    font.pixelSize: 10
                                }
                            }
                            Text {
                                text: modelData.v
                                color: Theme.textMuted
                                font.pixelSize: 10
                            }
                            Item { Layout.fillWidth: true }
                        }
                    }
                }
            }
        }

        // ── Transport ─────────────────────────────────────────────────────
        Rectangle {
            id: transportBar
            Layout.fillWidth: true
            Layout.preferredHeight: page.theaterMode && page.chromeHidden ? 0 : 44
            visible: Layout.preferredHeight > 0
            color: Theme.surface
            Behavior on Layout.preferredHeight { NumberAnimation { duration: 120 } }
            HoverHandler { onHoveredChanged: page.chromeHovered = hovered }

            Rectangle { width: parent.width; height: 1; color: Theme.border }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 14
                spacing: 4

                EditorButton { icon: "rewind";       tooltip: (I18n.language, I18n.t("editor.back5")); onTriggered: page.skip(-5000) }
                EditorButton {
                    icon: mp.playbackState === MediaPlayer.PlayingState ? "pause" : "play"
                    tooltip: (I18n.language, I18n.t("editor.keys.playPause"))
                    onTriggered: page.togglePlay()
                }
                EditorButton { icon: "fast-forward"; tooltip: (I18n.language, I18n.t("editor.forward5")); onTriggered: page.skip(5000) }

                Text {
                    Layout.leftMargin: 8
                    text: page.fmt(mp.position / 1000) + "  /  " + page.fmt(page.duration)
                    color: Theme.textDim
                    font.pixelSize: 12
                }

                // Scrub bar. In full view the timeline is hidden, which left
                // no way at all to seek — only the ±5s buttons. Shown there,
                // and folded away when the timeline is back on screen.
                Slider {
                    id: seekBar
                    visible: page.theaterMode
                    Layout.fillWidth: true
                    Layout.preferredHeight: 22
                    Layout.leftMargin: 10
                    Layout.rightMargin: 10
                    from: 0
                    to: Math.max(1, mp.duration)

                    // Seeded, not bound: QQC2 severs a value binding on the
                    // first drag (AGENTS.md landmine 5).
                    Component.onCompleted: value = mp.position
                    onMoved: page.seekTo(seekBar.value)
                    Connections {
                        target: mp
                        function onPositionChanged() {
                            if (!seekBar.pressed)
                                seekBar.value = mp.position
                        }
                    }

                    background: Rectangle {
                        x: seekBar.leftPadding
                        y: seekBar.topPadding + seekBar.availableHeight / 2 - height / 2
                        width: seekBar.availableWidth
                        height: 4
                        radius: 2
                        color: Theme.border
                        Rectangle {
                            width: seekBar.visualPosition * parent.width
                            height: parent.height
                            radius: 2
                            color: Theme.accent
                        }
                    }
                    handle: SliderHandle {
                        x: seekBar.leftPadding
                           + seekBar.visualPosition * (seekBar.availableWidth - width)
                        y: seekBar.topPadding + seekBar.availableHeight / 2 - height / 2
                        active: seekBar.pressed || seekBar.hovered
                    }
                }

                Item { Layout.fillWidth: !page.theaterMode }

                EditorButton {
                    icon: masterMuted ? "volume-x" : "volume-2"
                    tooltip: masterMuted ? (I18n.language, I18n.t("editor.unmute"))
                                         : (I18n.language, I18n.t("editor.mute"))
                    onTriggered: {
                        page.masterMuted = !page.masterMuted
                        if (page.mixed)
                            page.applyVolume()
                        else
                            audioOut.muted = page.masterMuted
                    }
                }
                Slider {
                    id: vol
                    Layout.preferredWidth: 84
                    // Custom background/handle delegates set `height`, not
                    // `implicitHeight`, so this Control had no implicit height
                    // and the layout arranged it 0px tall — painted but never
                    // hit-tested, which is why the master volume handle would
                    // not move. See HSlider.qml for the same defect.
                    Layout.preferredHeight: 22
                    Layout.alignment: Qt.AlignVCenter
                    from: 0; to: 1
                    // Seeded once, then owned locally — a binding here would be
                    // severed by QQC2's own writes on the first drag anyway.
                    Component.onCompleted: value = page.masterVolume
                    // A clip's restored volume arrives after the slider exists.
                    Connections {
                        target: page
                        function onMasterVolumeChanged() {
                            if (!vol.pressed) vol.value = page.masterVolume
                        }
                    }
                    onMoved: {
                        page.masterVolume = vol.value
                        if (vol.value > 0) page.masterMuted = false
                        page.applyVolume()
                    }
                    background: Rectangle {
                        x: vol.leftPadding
                        y: vol.topPadding + vol.availableHeight / 2 - height / 2
                        width: vol.availableWidth
                        height: 4
                        radius: 2
                        color: Theme.border
                        Rectangle {
                            width: vol.visualPosition * parent.width
                            height: parent.height
                            radius: 2
                            color: Theme.accent
                        }
                    }
                    handle: SliderHandle {
                        x: vol.leftPadding + vol.visualPosition * (vol.availableWidth - width)
                        y: vol.topPadding + vol.availableHeight / 2 - height / 2
                        active: vol.pressed || vol.hovered
                    }
                }

                EditorButton {
                    icon: "camera"
                    tooltip: (I18n.language, I18n.t("editor.saveFrame"))
                    onTriggered: {
                        if (page.clip)
                            EditorController.grabFrame(page.clip.filepath, mp.position / 1000)
                    }
                }
                EditorButton {
                    icon: page.theaterMode ? "minimize" : "maximize"
                    tooltip: page.theaterMode ? (I18n.language, I18n.t("editor.exitFullView"))
                                              : (I18n.language, I18n.t("editor.keys.fullView"))
                    onTriggered: page.theaterMode = !page.theaterMode
                }
                EditorButton {
                    icon: "rotate-ccw"
                    tooltip: (I18n.language, I18n.t("editor.resetTrim"))
                    onTriggered: page.resetTrim()
                }
            }
        }

        // ── Timeline ──────────────────────────────────────────────────────
        Rectangle {
            id: timelinePane
            visible: !page.theaterMode
            Layout.fillWidth: true
            Layout.preferredHeight: page.theaterMode ? 0 : page.timelineH
            color: Theme.bgDeep

            Rectangle { width: parent.width; height: 1; color: Theme.border }

            // Splitter on the timeline's top edge: taller lanes get more
            // waveform detail, a shorter timeline gives the picture more room.
            MouseArea {
                id: timeSplit
                z: 5
                anchors.top: parent.top
                anchors.topMargin: -3
                width: parent.width
                height: 7
                hoverEnabled: true
                preventStealing: true
                cursorShape: Qt.SplitVCursor
                onPositionChanged: (m) => {
                    if (!pressed)
                        return
                    const y = mapToItem(page, 0, m.y).y
                    page.timelineUserH = Math.max(page.timelineMinH,
                                                  Math.min(page.timelineMaxH, page.height - y))
                }
                onReleased: SettingsController.setValue("editorTimelineH",
                                                        JSON.stringify(Math.round(page.timelineUserH)))
                onDoubleClicked: {
                    // Back to "fit the lanes".
                    page.timelineUserH = 0
                    SettingsController.setValue("editorTimelineH", "0")
                }
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width; height: 2
                    color: Theme.accent
                    visible: timeSplit.containsMouse || timeSplit.pressed
                }
            }

            // Fixed-width label gutter, so every lane's track area starts at
            // the same x and the playhead/trim overlay can span them all.
            readonly property int gutter: 96
            readonly property real trackW: width - gutter - 24

            function xToTime(x) {
                if (page.duration <= 0 || timelinePane.trackW <= 0) return 0
                return Math.max(0, Math.min(page.duration,
                    (x / timelinePane.trackW) * page.duration))
            }
            function timeToX(t) {
                if (page.duration <= 0) return 0
                return (t / page.duration) * timelinePane.trackW
            }

            ColumnLayout {
                id: lanes
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: 12
                anchors.rightMargin: 12
                anchors.topMargin: 10
                spacing: 4

                // Video lane — named and coloured from the V1 slot.
                EditorTrackLane {
                    readonly property var def: page.trackDef("V1")
                    Layout.fillWidth: true
                    Layout.preferredHeight: page.laneH
                    gutter: timelinePane.gutter
                    label: (I18n.language, page.trackLabel(def || { id: "V1" }))
                    accent: def && def.color ? def.color : Theme.accent
                    icon: page.trackIcon(def ? def.icon : "", "track-video")
                    monitorable: false
                }

                // One lane per audio stream, labelled with its real track name.
                Repeater {
                    model: page.audioStreams

                    EditorTrackLane {
                        required property var modelData
                        required property int index
                        // Audio streams map onto the A1..An slots in order.
                        readonly property var def: page.trackDef("A" + (index + 1))
                        Layout.fillWidth: true
                        Layout.preferredHeight: page.laneH
                        gutter: timelinePane.gutter
                        label: def && def.name ? (I18n.language, page.trackLabel(def))
                             : (modelData.title || I18n.t("editor.audioTrack").replace("{n}", index + 1))
                        accent: def && def.color ? def.color
                              : Theme.channelColor(modelData.title)
                        icon: page.trackIcon(def ? def.icon : "", "music")
                        // Every track plays at once; this mutes one of them
                        // rather than switching which single track is audible.
                        monitorable: page.mixed
                        monitoring: page.trackMuted[index] !== true
                        volume: page.trackGain(index)
                        peaks: page.waveforms[modelData.index] || []
                        onMonitorToggled: page.toggleTrack(index)
                        onVolumeRequested: (v) => page.setTrackGain(index, v)

                        // 600 peaks is roughly two per pixel at a typical
                        // timeline width — enough detail to survive the
                        // window being widened without re-decoding. Repeat
                        // requests are dropped controller-side.
                        Component.onCompleted: if (page.filepath)
                            EditorController.requestWaveform(page.filepath, modelData.index, 600)
                    }
                }
            }

            // ── Trim + playhead overlay ───────────────────────────────────
            // One overlay across every lane rather than per-lane handles, so a
            // trim edit visibly applies to the whole clip.
            Item {
                id: overlay
                x: timelinePane.gutter + 12
                y: 10
                width: timelinePane.trackW
                height: lanes.implicitHeight

                // Dim the regions the export will discard.
                Rectangle {
                    x: 0
                    width: Math.max(0, timelinePane.timeToX(page.trimStart))
                    height: parent.height
                    color: Theme.scrim(60)
                }
                Rectangle {
                    x: timelinePane.timeToX(page.trimEnd)
                    width: Math.max(0, parent.width - x)
                    height: parent.height
                    color: Theme.scrim(60)
                }

                // Seek by clicking anywhere on the timeline.
                MouseArea {
                    anchors.fill: parent
                    onClicked: (m) => page.seekTo(Math.round(timelinePane.xToTime(m.x) * 1000))
                }

                // Playhead
                Rectangle {
                    x: timelinePane.timeToX(mp.position / 1000) - width / 2
                    width: 2
                    height: parent.height
                    color: Theme.text
                }

                TrimHandle {
                    id: startHandle
                    x: timelinePane.timeToX(page.trimStart) - width / 2
                    height: parent.height
                    onMovedTo: (px) => {
                        page.trimStart = Math.min(timelinePane.xToTime(px),
                                                  page.trimEnd - 0.1)
                    }
                    // One history entry per drag, not one per mouse move.
                    onDraggingChanged: if (!dragging) page.commitCurrentTrim()
                }
                TrimHandle {
                    id: endHandle
                    x: timelinePane.timeToX(page.trimEnd) - width / 2
                    height: parent.height
                    onMovedTo: (px) => {
                        page.trimEnd = Math.max(timelinePane.xToTime(px),
                                                page.trimStart + 0.1)
                    }
                    onDraggingChanged: if (!dragging) page.commitCurrentTrim()
                }
            }
        }
    }

    // ── Export settings dialog ────────────────────────────────────────────
    ExportDialog {
        // A form, not a media control: it follows the language's direction
        // even though the rest of the editor stays left-to-right.
        LayoutMirroring.enabled: I18n.rtl
        clip: page.exportOpen ? page.clip : null
        trimStart: page.trimStart
        trimEnd: page.trimEnd
        z: 100
        onClosed: page.exportOpen = false
    }

    // A finished frame grab is worth confirming — it lands in a directory the
    // user may not have open.
    Rectangle {
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 90
        z: 90
        visible: EditorController.screenshotPath.length > 0 && shotToast.running
        radius: Theme.radius
        color: Theme.surface
        border.width: 1
        border.color: Theme.accent
        implicitWidth: shotText.implicitWidth + 28
        implicitHeight: 34
        Text {
            id: shotText
            anchors.centerIn: parent
            text: (I18n.language, I18n.t("editor.frameSaved"))
            color: Theme.text
            font.pixelSize: 12
        }
        Timer {
            id: shotToast
            interval: 2600
        }
        Connections {
            target: EditorController
            function onScreenshotPathChanged() {
                if (EditorController.screenshotPath.length > 0)
                    shotToast.restart()
            }
        }
    }
}

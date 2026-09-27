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
    /// Hides the info panel and timeline so the picture fills the page.
    property bool theaterMode: false
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
        if (!page.clip)
            return
        // Read straight off `clip` rather than through the `filepath` binding:
        // QML does not guarantee a dependent binding has re-evaluated by the
        // time the property's own onChanged handler runs, and loading with a
        // stale empty path left the whole INFO panel showing dashes.
        EditorController.loadClip(page.clip.filepath)
        page.trimStart = EditorController.trimStart
        page.trimEnd = EditorController.trimEnd
        page.gameTag = page.clip.game || ""
        page.trackMuted = ({})
        // false: the editor has no ClipVideoSurface yet (its picture still
        // comes from Qt Multimedia below) — see load()'s doc comment.
        page.mixerToken = ClipAudioMixer.load(page.clip.filepath, false)
        page.applyVolume()
        // The mix is started by onPlaybackStateChanged, not here — starting it
        // alongside mp.play() ran the audio ahead of the first frame.
        mp.play()
    }

    onVisibleChanged: {
        if (visible) {
            page.forceActiveFocus()
        } else {
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
    Timer { id: seekSettle; interval: 250 }

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
        page.trimStart = 0
        page.trimEnd = page.duration
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
            page.trimStart = Math.min(mp.position / 1000, page.trimEnd); e.accepted = true; break
        case Qt.Key_O:
            page.trimEnd = Math.max(mp.position / 1000, page.trimStart); e.accepted = true; break
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
            if (e.modifiers & Qt.ControlModifier) { page.resetTrim(); e.accepted = true }
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
        onPlaybackStateChanged: {
            if (!page.mixed)
                return
            if (mp.playbackState === MediaPlayer.PlayingState) {
                ClipAudioMixer.seek(mp.position)
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
            // 44, not 56: the bar carries a back button, two fields and a few
            // pills, none of which need that much height, and it was eating
            // vertical space the picture wants.
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            color: Theme.surface
            border.width: 0

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

                // Name and game are editable here, as in the old editor's
                // header — the Clips grid's inline rename only covers the name.
                EditorField {
                    id: nameField
                    Layout.preferredWidth: 220
                    placeholder: "Clip name"
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
                    placeholder: "Game…"
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
                                                  ? ("Add \"" + modelData.label + "\"")
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
                              : "Export Clip"
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
                visible: !page.theaterMode
                Layout.preferredWidth: page.theaterMode ? 0 : 232
                Layout.fillHeight: true
                color: Theme.surface

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
                            { k: "Duration",   v: page.duration > 0 ? page.duration.toFixed(1) + "s" : "—" },
                            { k: "Resolution", v: page.info.width ? page.info.width + "×" + page.info.height : "—" },
                            { k: "FPS",        v: page.info.fps ? page.info.fps.toFixed(1) : "—" },
                            { k: "Codec",      v: page.info.video_codec || "—" },
                            { k: "Audio",      v: page.audioStreams.length
                                                  ? page.audioStreams.length + " tracks" : "—" },
                            { k: "Trim",       v: page.fmt(page.trimStart) + " → " + page.fmt(page.trimEnd) },
                            { k: "Output",     v: Math.max(0, page.trimEnd - page.trimStart).toFixed(1) + "s" }
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

                    Repeater {
                        model: [
                            { k: "Space / K", v: "Play / Pause" },
                            { k: "J / L",     v: "Back 2s / faster" },
                            { k: "← →",       v: "Skip ±5s (⇧ ±1s)" },
                            { k: ", .",       v: "Frame step" },
                            { k: "I / O",     v: "Set trim in / out" },
                            { k: "M",         v: "Mute" },
                            { k: "1–5",       v: "Mute audio track" },
                            { k: "F",         v: "Full view" },
                            { k: "Ctrl+Z",    v: "Reset trim" }
                        ]

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
            Layout.fillWidth: true
            Layout.preferredHeight: 44
            color: Theme.surface

            Rectangle { width: parent.width; height: 1; color: Theme.border }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 10
                anchors.rightMargin: 14
                spacing: 4

                EditorButton { icon: "rewind";       tooltip: "Back 5s";   onTriggered: page.skip(-5000) }
                EditorButton {
                    icon: mp.playbackState === MediaPlayer.PlayingState ? "pause" : "play"
                    tooltip: "Play / Pause"
                    onTriggered: page.togglePlay()
                }
                EditorButton { icon: "fast-forward"; tooltip: "Forward 5s"; onTriggered: page.skip(5000) }

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
                    tooltip: masterMuted ? "Unmute" : "Mute"
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
                    tooltip: "Save this frame as an image"
                    onTriggered: {
                        if (page.clip)
                            EditorController.grabFrame(page.clip.filepath, mp.position / 1000)
                    }
                }
                EditorButton {
                    icon: page.theaterMode ? "minimize" : "maximize"
                    tooltip: page.theaterMode ? "Exit full view" : "Full view"
                    onTriggered: page.theaterMode = !page.theaterMode
                }
                EditorButton {
                    icon: "rotate-ccw"
                    tooltip: "Reset trim"
                    onTriggered: page.resetTrim()
                }
            }
        }

        // ── Timeline ──────────────────────────────────────────────────────
        Rectangle {
            id: timelinePane
            visible: !page.theaterMode
            Layout.fillWidth: true
            Layout.preferredHeight: page.theaterMode
                                    ? 0 : Math.min(196, 34 + lanes.implicitHeight + 16)
            color: Theme.bgDeep

            Rectangle { width: parent.width; height: 1; color: Theme.border }

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
                    gutter: timelinePane.gutter
                    label: def && def.name ? def.name : "Video"
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
                        gutter: timelinePane.gutter
                        label: def && def.name ? def.name
                             : (modelData.title || ("Audio " + (index + 1)))
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
                    x: timelinePane.timeToX(page.trimStart) - width / 2
                    height: parent.height
                    onMovedTo: (px) => {
                        page.trimStart = Math.min(timelinePane.xToTime(px),
                                                  page.trimEnd - 0.1)
                    }
                }
                TrimHandle {
                    x: timelinePane.timeToX(page.trimEnd) - width / 2
                    height: parent.height
                    onMovedTo: (px) => {
                        page.trimEnd = Math.max(timelinePane.xToTime(px),
                                                page.trimStart + 0.1)
                    }
                }
            }
        }
    }

    // ── Export settings dialog ────────────────────────────────────────────
    ExportDialog {
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

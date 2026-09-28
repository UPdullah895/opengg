import QtQuick
import QtQuick.Controls
import QtMultimedia
import com.opengg.app

// VideoPlayer — full-window clip player. Qt Multimedia MediaPlayer +
// VideoOutput on the GStreamer backend (PoC-validated, Wayland-native).
//
// Rebuilt from a play/pause-and-a-seek-bar stub into a port of
// CustomVideoPlayer.vue: scrub bar with hover-time preview, ±5s skip, volume
// with mute, playback speed, expand-to-window, auto-hiding controls and the
// same keyboard map (Space / F / arrows / Esc).
//
// MULTI-TRACK AUDIO: a clip captured with separate Game/Chat/Mic tracks holds
// several audio streams. Qt Multimedia decodes exactly one at a time — it can
// SWITCH tracks (activeAudioTrack) but cannot mix them, which is why the old
// player silently played only the first and the rest seemed missing. The track
// selector below exposes the switch; simultaneous mixing needs a hand-built
// GStreamer pipeline and belongs to the editor, not this preview.
Rectangle {
    id: root
    color: Theme.scrim(92)
    property string source: ""
    property string title: ""
    /// Cached thumbnail, shown until the pipeline renders its first frame.
    property string posterSource: ""
    signal closed()

    /// Expanded = video fills the whole overlay instead of a centred panel.
    property bool expanded: false

    focus: visible
    onVisibleChanged: {
        if (visible) {
            root.forceActiveFocus()
        } else {
            mp.stop()
            // Release the audio device — a live pipeline left running would
            // keep playing over the rest of the app. Token-scoped, so hiding
            // this player during navigation can't silence the editor.
            if (root.mixerToken !== null) {
                ClipAudioMixer.release(root.mixerToken)
                root.mixerToken = null
            }
            root.expanded = false
            speedMenu.open = false
            trackMenu.open = false
        }
    }

    function fmt(ms) {
        if (!ms || ms < 0)
            ms = 0
        var t = Math.floor(ms / 1000)
        var h = Math.floor(t / 3600)
        var m = Math.floor((t % 3600) / 60)
        var s = t % 60
        var mm = (h > 0 && m < 10 ? "0" : "") + m
        return (h > 0 ? h + ":" : "") + mm + ":" + (s < 10 ? "0" : "") + s
    }

    // ── Audio ownership ───────────────────────────────────────────────────
    // When the clip has several audio tracks, ClipAudioMixer takes the sound
    // (all tracks mixed, independent gains) and Qt Multimedia is muted down to
    // just the picture. Single-track clips keep Qt's own audio path.
    readonly property bool mixed: ClipAudioMixer.active
    /// Ownership token from the last ClipAudioMixer.load — see the note there.
    /// Releasing without it let this player tear down audio the editor page
    /// had just started, which is how the editor ended up silent.
    property var mixerToken: null
    /// Per-track mute flags while `mixed`. Reassigned wholesale, never mutated
    /// in place, so the bindings in the track menu actually re-evaluate.
    property var trackMuted: ({})

    // The user's own output level and mute, owned by this player rather than
    // read back off `audioOut`.
    //
    // They USED to be read off audioOut, and that is why the preview played
    // silently: `audioOut.muted` is forced true whenever the mixer owns the
    // sound, and `ClipAudioMixer.load()` flips `active` synchronously, so the
    // very next line — `setMasterVolume(audioOut.muted ? 0 : ...)` — always
    // saw `muted == true` on a multi-track clip and pinned the mix's master
    // gain to zero. Qt's output muted by design and the mix at 0 dB meant no
    // sound at all. Keeping intent ("the user wants 80%, unmuted") apart from
    // ownership ("Qt must stay quiet because the mixer has the audio") is the
    // fix; ClipEditorPage.qml already models it this way.
    /// Output level the user asked for, 0..1.
    property real userVolume: 1.0
    /// Whether the USER muted the output — never the ownership mute.
    property bool userMuted: false

    /// Push `userVolume`/`userMuted` to whoever currently owns the sound.
    /// While mixed that is the mix's master gain; otherwise Qt's own output
    /// picks it up through the bindings on `audioOut`.
    function applyVolume() {
        if (root.mixed)
            ClipAudioMixer.setMasterVolume(root.userMuted ? 0 : root.userVolume)
    }

    function toggleTrack(index) {
        var next = {}
        for (var k in root.trackMuted) next[k] = root.trackMuted[k]
        next[index] = !next[index]
        root.trackMuted = next
        ClipAudioMixer.setTrackVolume(index, next[index] ? 0.0 : 1.0)
        root.poke()
    }

    function togglePlay() {
        // Only the video is driven here; onPlaybackStateChanged mirrors the
        // resulting state onto the mixer.
        if (mp.playbackState === MediaPlayer.PlayingState) mp.pause()
        else mp.play()
        root.poke()
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
        if (root.mixed) {
            ClipAudioMixer.seek(target)
            seekSettle.restart()
        }
    }
    function skip(ms) {
        root.seekTo(mp.position + ms)
        root.poke()
    }
    function setVolume(v) {
        root.userVolume = Math.max(0, Math.min(1, v))
        // Nudging the level up is an implicit unmute.
        if (root.userVolume > 0) root.userMuted = false
        root.applyVolume()
        root.poke()
    }
    function toggleMute() {
        root.userMuted = !root.userMuted
        root.applyVolume()
        root.poke()
    }

    // The two clocks run independently, so nudge the audio back whenever it
    // drifts more than a frame or two from the video.
    //
    // NOT while ClipAudioMixer.videoActive: once the mixer's own GStreamer
    // pipeline renders the picture, it and the audio share one clock and
    // can't drift — the periodic FLUSHING|KEY_UNIT seek this fires was
    // hitting the now-visible video branch too, which is what made playback
    // visibly repeat a few frames and jump, especially right after a skip.
    Timer {
        interval: 400
        running: root.visible && root.mixed && !ClipAudioMixer.videoActive
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

    /// A FLUSHING seek needs a moment before the pipeline reports its new
    /// position. Sampling inside that window reads the OLD position, looks
    /// like drift, and provokes another correction — a loop that replayed the
    /// same stretch of audio. Every seek we issue restarts this.
    Timer { id: seekSettle; interval: 250 }

    // ── Auto-hiding chrome ────────────────────────────────────────────────
    // Controls fade out after a few idle seconds during playback, and come
    // straight back on any pointer or key activity.
    property bool chromeVisible: true
    function poke() {
        root.chromeVisible = true
        idleTimer.restart()
    }
    Timer {
        id: idleTimer
        interval: 2600
        onTriggered: {
            if (speedMenu.open || trackMenu.open || seek.pressed)
                return
            // Expanded is a viewing mode, so the chrome gets out of the way
            // whether or not the clip happens to be playing — it used to
            // sit over a paused, maximised picture indefinitely. Windowed
            // playback keeps the old rule: chrome stays while paused,
            // because there it frames the panel rather than covering it.
            if (root.expanded || mp.playbackState === MediaPlayer.PlayingState)
                root.chromeVisible = false
        }
    }
    // Entering or leaving expanded restarts the countdown, so maximising a
    // paused clip settles into a clean picture on its own.
    onExpandedChanged: root.poke()

    Keys.onPressed: (e) => {
        switch (e.key) {
        case Qt.Key_Escape:
            if (root.expanded) root.expanded = false
            else root.closed()
            e.accepted = true; break
        case Qt.Key_Space:
            root.togglePlay(); e.accepted = true; break
        case Qt.Key_F:
            root.expanded = !root.expanded; root.poke(); e.accepted = true; break
        case Qt.Key_Right:
            root.skip(5000); e.accepted = true; break
        case Qt.Key_Left:
            root.skip(-5000); e.accepted = true; break
        case Qt.Key_Up:
            root.setVolume(root.userVolume + 0.1); e.accepted = true; break
        case Qt.Key_Down:
            root.setVolume(root.userVolume - 0.1); e.accepted = true; break
        case Qt.Key_M:
            root.toggleMute(); e.accepted = true; break
        }
    }

    MediaPlayer {
        id: mp
        source: root.source
        videoOutput: videoOut
        audioOutput: AudioOutput {
            id: audioOut
            // Silence Qt's single-track decode when the mixer owns the sound,
            // otherwise track 1 would play twice — once here, once in the mix.
            // The user's own mute is a separate term, so the mute button no
            // longer has to overwrite (and thereby sever) this binding.
            muted: root.mixed || root.userMuted
            volume: root.userVolume
        }
        onPlaybackRateChanged: if (root.mixed) ClipAudioMixer.setRate(mp.playbackRate)
        onMediaStatusChanged: {
            // PoC gap #6: Qt Multimedia leaves the pipeline at EndOfMedia; reset
            // to a paused first frame so the clip can be replayed.
            if (mediaStatus === MediaPlayer.EndOfMedia) {
                mp.pause()
                mp.position = 0
                if (root.mixed) {
                    ClipAudioMixer.pause()
                    ClipAudioMixer.seek(0)
                }
                root.poke()
            }
        }
        // The mix follows the video's ACTUAL state rather than being started
        // alongside mp.play(), which returns long before the first frame.
        onPlaybackStateChanged: {
            if (root.mixed) {
                if (mp.playbackState === MediaPlayer.PlayingState) {
                    ClipAudioMixer.seek(mp.position)
                    seekSettle.restart()
                    ClipAudioMixer.play()
                } else {
                    ClipAudioMixer.pause()
                }
            }
            root.poke()
        }
    }
    Component.onDestruction: mp.stop()

    // Restart playback whenever a different clip is opened, and re-probe its
    // audio-track names (see the note on requestAudioTracks in clips.rs).
    onSourceChanged: {
        if (root.source.length > 0) {
            const path = root.source.replace(/^file:\/\//, "")
            ClipsController.requestAudioTracks(path)
            // Build the mix first: `active` decides who owns the audio, and
            // deciding after playback starts would leak a burst of Qt's
            // single-track sound.
            root.trackMuted = ({})
            // true: this view has a ClipVideoSurface (objectName
            // "clipVideoItem") for the pipeline's video branch to attach to.
            root.mixerToken = ClipAudioMixer.load(path, true)
            root.applyVolume()
            // Deliberately NOT starting the mix here: the pipeline would begin
            // instantly while Qt is still opening the file, so the audio ran
            // ahead of the picture. It follows mp's real playback state below.
            mp.play()
            root.poke()
        }
    }

    // Backdrop click closes (only outside the panel — the panel absorbs its own).
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onClicked: root.closed()
        onPositionChanged: root.poke()
    }

    // ── Player panel ──────────────────────────────────────────────────────
    Rectangle {
        id: panel
        anchors.centerIn: parent
        width: root.expanded ? parent.width : Math.min(parent.width - 64, 1280)
        height: root.expanded ? parent.height : Math.min(parent.height - 64, width * 0.62)
        radius: root.expanded ? 0 : Theme.radiusLg
        color: "#000000"
        border.width: root.expanded ? 0 : 1
        border.color: Theme.border
        clip: true

        Behavior on width  { NumberAnimation { duration: 120; easing.type: Easing.OutQuad } }
        Behavior on height { NumberAnimation { duration: 120; easing.type: Easing.OutQuad } }

        // Migration state: both video paths exist. ClipAudioMixer's GStreamer
        // pipeline renders here when it can; Qt Multimedia's VideoOutput is
        // the fallback until B3 removes it entirely.
        ClipVideoSurface {
            id: gstSurface
            anchors.fill: parent
            posterSource: root.posterSource
        }

        VideoOutput {
            id: videoOut
            anchors.fill: parent
            visible: !ClipAudioMixer.videoActive
        }

        // Click the video to toggle playback; movement wakes the chrome.
        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            onClicked: root.togglePlay()
            onDoubleClicked: root.expanded = !root.expanded
            onPositionChanged: root.poke()
        }

        Text {
            anchors.centerIn: parent
            visible: mp.mediaStatus === MediaPlayer.LoadingMedia
                     || mp.mediaStatus === MediaPlayer.BufferingMedia
            text: (I18n.language, I18n.t("videoPlayer.loading"))
            color: Theme.textDim
            font.pixelSize: 14
        }

        // Big centre play badge while paused.
        Rectangle {
            anchors.centerIn: parent
            width: 66; height: 66; radius: 33
            visible: mp.playbackState !== MediaPlayer.PlayingState
                     && mp.mediaStatus !== MediaPlayer.LoadingMedia
            color: Theme.scrim(55)
            Icon {
                anchors.centerIn: parent
                anchors.horizontalCenterOffset: 3   // optical centring of the triangle
                name: "play"; size: 26; color: "#ffffff"
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.togglePlay()
            }
        }

        // ── Top bar: title + close ────────────────────────────────────────
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: 54
            opacity: root.chromeVisible ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 160 } }
            gradient: Gradient {
                GradientStop { position: 0.0; color: Theme.scrim(75) }
                GradientStop { position: 1.0; color: "transparent" }
            }

            Text {
                anchors.left: parent.left
                anchors.leftMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 70
                text: root.title
                color: "#ffffff"
                font.pixelSize: 15
                font.weight: Font.DemiBold
                elide: Text.ElideRight
            }
            Rectangle {
                anchors.right: parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                width: 30; height: 30; radius: 15
                color: closeArea.containsMouse ? Theme.accent : Theme.scrim(45)
                Icon { anchors.centerIn: parent; name: "x"; size: 15; color: "#ffffff" }
                MouseArea {
                    id: closeArea
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.closed()
                }
            }
        }

        // ── Bottom control bar ────────────────────────────────────────────
        Rectangle {
            id: controls
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 86
            opacity: root.chromeVisible ? 1 : 0
            visible: opacity > 0.01
            Behavior on opacity { NumberAnimation { duration: 160 } }
            gradient: Gradient {
                GradientStop { position: 0.0; color: "transparent" }
                GradientStop { position: 0.45; color: Theme.scrim(70) }
                GradientStop { position: 1.0; color: Theme.scrim(90) }
            }

            // Absorb clicks so they don't reach the play/pause surface.
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                onPositionChanged: root.poke()
            }

            // Scrub bar
            Item {
                id: seekRow
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.leftMargin: 16
                anchors.rightMargin: 16
                anchors.top: parent.top
                anchors.topMargin: 14
                height: 18

                Slider {
                    id: seek
                    anchors.fill: parent
                    from: 0
                    to: Math.max(mp.duration, 1)
                    // Binding straight to mp.position would be severed the
                    // first time the user drags (QQC2 writes `value` directly),
                    // so hold the drag value locally and only follow the player
                    // while not pressed — same pattern as the mixer faders.
                    value: seek.pressed ? seek.value : mp.position
                    onMoved: root.seekTo(Math.round(seek.value))
                    onPressedChanged: root.poke()

                    background: Rectangle {
                        x: seek.leftPadding
                        y: seek.topPadding + seek.availableHeight / 2 - height / 2
                        width: seek.availableWidth
                        height: seekHover.hovered || seek.pressed ? 6 : 4
                        radius: height / 2
                        color: Theme.tint(Theme.text, 30)
                        Behavior on height { NumberAnimation { duration: 90 } }

                        Rectangle {
                            width: seek.visualPosition * parent.width
                            height: parent.height
                            radius: parent.radius
                            color: Theme.accent
                        }
                    }
                    handle: Rectangle {
                        x: seek.leftPadding + seek.visualPosition * (seek.availableWidth - width)
                        y: seek.topPadding + seek.availableHeight / 2 - height / 2
                        width: seekHover.hovered || seek.pressed ? 14 : 0
                        height: width
                        radius: width / 2
                        color: Theme.accent
                        Behavior on width { NumberAnimation { duration: 90 } }
                    }
                }

                HoverHandler { id: seekHover }

                // Hover time preview
                Rectangle {
                    visible: seekHover.hovered && mp.duration > 0
                    x: Math.max(0, Math.min(seekRow.width - width,
                                            seekHover.point.position.x - width / 2))
                    y: -26
                    radius: 4
                    color: Theme.scrim(90)
                    implicitWidth: hoverTime.implicitWidth + 14
                    implicitHeight: 20
                    Text {
                        id: hoverTime
                        anchors.centerIn: parent
                        text: root.fmt(Math.max(0, Math.min(1,
                                  seekHover.point.position.x / Math.max(1, seekRow.width)))
                              * mp.duration)
                        color: "#ffffff"
                        font.pixelSize: 11
                    }
                }
            }

            // Transport row
            Item {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.leftMargin: 14
                anchors.rightMargin: 14
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 12
                height: 32

                Row {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4

                    PlayerButton {
                        icon: "rewind"; tooltip: "Back 5s"
                        onTriggered: root.skip(-5000)
                    }
                    PlayerButton {
                        icon: mp.playbackState === MediaPlayer.PlayingState ? "pause" : "play"
                        tooltip: mp.playbackState === MediaPlayer.PlayingState ? "Pause" : "Play"
                        onTriggered: root.togglePlay()
                    }
                    PlayerButton {
                        icon: "fast-forward"; tooltip: "Forward 5s"
                        onTriggered: root.skip(5000)
                    }

                    // Volume: icon toggles mute, slider expands on hover.
                    Item {
                        anchors.verticalCenter: parent.verticalCenter
                        width: volIcon.width + (volHover.hovered ? 78 : 0)
                        height: 32
                        Behavior on width { NumberAnimation { duration: 120 } }
                        HoverHandler { id: volHover }

                        PlayerButton {
                            id: volIcon
                            icon: root.userMuted || root.userVolume <= 0.001 ? "volume-x"
                                : root.userVolume < 0.5 ? "volume-1" : "volume-2"
                            tooltip: root.userMuted ? "Unmute" : "Mute"
                            onTriggered: root.toggleMute()
                        }
                        Slider {
                            id: vol
                            anchors.left: volIcon.right
                            anchors.leftMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            width: 70
                            // Without this the Slider is 0px tall — its
                            // custom background/handle set `height`, not
                            // `implicitHeight`, so the Control had no implicit
                            // height of its own and never hit-tested. See the
                            // same note in HSlider.qml.
                            height: 22
                            visible: volHover.hovered
                            from: 0; to: 1
                            // NOT `value: root.userVolume` — QQC2 writes
                            // `value` directly on the first interactive drag,
                            // which severs a binding on it and leaves the
                            // handle stuck from then on. Seed it once and
                            // re-sync only when the volume changes elsewhere
                            // (mute toggle, keyboard Up/Down).
                            Component.onCompleted: value = root.userMuted ? 0 : root.userVolume
                            onMoved: root.setVolume(vol.value)
                            Connections {
                                target: root
                                function onUserVolumeChanged() {
                                    if (!vol.pressed) vol.value = root.userMuted ? 0 : root.userVolume
                                }
                                function onUserMutedChanged() {
                                    if (!vol.pressed) vol.value = root.userMuted ? 0 : root.userVolume
                                }
                            }

                            background: Rectangle {
                                x: vol.leftPadding
                                y: vol.topPadding + vol.availableHeight / 2 - height / 2
                                width: vol.availableWidth
                                height: 4
                                radius: 2
                                color: Theme.tint(Theme.text, 30)
                                Rectangle {
                                    width: vol.visualPosition * parent.width
                                    height: parent.height
                                    radius: 2
                                    color: "#ffffff"
                                }
                            }
                            handle: SliderHandle {
                                x: vol.leftPadding + vol.visualPosition * (vol.availableWidth - width)
                                y: vol.topPadding + vol.availableHeight / 2 - height / 2
                                active: vol.pressed || vol.hovered
                            }
                        }
                    }

                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        leftPadding: 6
                        text: root.fmt(mp.position) + "  /  " + root.fmt(mp.duration)
                        color: "#ffffff"
                        font.pixelSize: 12
                    }
                }

                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4

                    // Per-track mixer — only meaningful on multi-track clips.
                    PlayerButton {
                        id: trackBtn
                        icon: "music"
                        tooltip: "Audio tracks"
                        visible: root.mixed && ClipAudioMixer.trackCount > 1
                        active: trackMenu.open
                        onTriggered: {
                            trackMenu.open = !trackMenu.open
                            speedMenu.open = false
                            root.poke()
                        }
                    }
                    PlayerButton {
                        id: speedBtn
                        icon: "gear"
                        tooltip: "Playback speed"
                        active: speedMenu.open
                        onTriggered: {
                            speedMenu.open = !speedMenu.open
                            trackMenu.open = false
                            root.poke()
                        }
                    }
                    PlayerButton {
                        icon: root.expanded ? "minimize" : "maximize"
                        tooltip: root.expanded ? "Exit full view" : "Full view"
                        onTriggered: {
                            root.expanded = !root.expanded
                            root.poke()
                        }
                    }
                }
            }
        }

        // ── Speed menu ────────────────────────────────────────────────────
        Rectangle {
            id: speedMenu
            property bool open: false
            visible: open
            anchors.right: parent.right
            anchors.rightMargin: 20
            anchors.bottom: controls.top
            anchors.bottomMargin: -34
            width: 116
            height: speedCol.implicitHeight + 10
            radius: Theme.radius
            color: Theme.scrim(94)
            border.width: 1
            border.color: Theme.tint(Theme.text, 20)

            Column {
                id: speedCol
                anchors.centerIn: parent
                width: parent.width - 8

                Repeater {
                    model: [0.25, 0.5, 1.0, 1.25, 1.5, 2.0]

                    Rectangle {
                        required property real modelData
                        width: speedCol.width
                        height: 26
                        radius: 4
                        color: speedArea.containsMouse ? Theme.accentAlpha(22) : "transparent"

                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            text: modelData === 1.0 ? "Normal" : modelData + "×"
                            color: Math.abs(mp.playbackRate - modelData) < 0.001
                                   ? Theme.accent : "#ffffff"
                            font.pixelSize: 12
                        }
                        Icon {
                            anchors.right: parent.right
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            visible: Math.abs(mp.playbackRate - modelData) < 0.001
                            name: "check"; size: 12; color: Theme.accent
                        }
                        MouseArea {
                            id: speedArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                mp.playbackRate = modelData
                                speedMenu.open = false
                                root.poke()
                            }
                        }
                    }
                }
            }
        }

        // ── Audio-track menu ──────────────────────────────────────────────
        Rectangle {
            id: trackMenu
            property bool open: false
            visible: open
            anchors.right: parent.right
            anchors.rightMargin: 56
            anchors.bottom: controls.top
            anchors.bottomMargin: -34
            width: 190
            height: trackCol.implicitHeight + 10
            radius: Theme.radius
            color: Theme.scrim(94)
            border.width: 1
            border.color: Theme.tint(Theme.text, 20)

            Column {
                id: trackCol
                anchors.centerIn: parent
                width: parent.width - 8

                // Every track is audible at once; these toggle each one's gain
                // rather than choosing between them.
                Repeater {
                    model: ClipAudioMixer.trackCount

                    Rectangle {
                        required property int index
                        readonly property bool muted: root.trackMuted[index] === true
                        width: trackCol.width
                        height: 26
                        radius: 4
                        color: trackArea.containsMouse ? Theme.accentAlpha(22) : "transparent"

                        Icon {
                            id: trackIcon
                            anchors.left: parent.left
                            anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            name: muted ? "volume-x" : "volume-2"
                            size: 13
                            color: muted ? Theme.textMuted : Theme.accent
                        }
                        Text {
                            anchors.left: trackIcon.right
                            anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 40
                            // Real names ("Game"/"Chat"/"Mic") come from the
                            // ffprobe side-channel; Qt's own metadata has none
                            // for GSR captures, hence the positional fallback.
                            text: {
                                const names = ClipsController.audioTrackNames
                                if (names && index < names.length && names[index])
                                    return names[index]
                                return "Track " + (index + 1)
                            }
                            color: muted ? Theme.textMuted : "#ffffff"
                            font.pixelSize: 12
                            elide: Text.ElideRight
                        }
                        MouseArea {
                            id: trackArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.toggleTrack(index)
                        }
                    }
                }
            }
        }
    }
}

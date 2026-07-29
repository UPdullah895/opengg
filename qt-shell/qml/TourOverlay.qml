import QtQuick
import com.opengg.app

// TourOverlay — QML port of GuidedTour.vue's spotlight + card. Sits once at the
// top of Main.qml's tree, above the Titlebar/Sidebar/StackLayout, and reads its
// single Item target per step out of TourController.targets (registered by each
// page's Component.onCompleted).
//
// Vue's version leans on @floating-ui/vue (flip/shift/arrow middleware) to keep
// the card on-screen against a scrolling document. There's no scrolling document
// here — the overlay is always exactly the window's client area — so placement
// is a plain compute-then-clamp against the four sides, no middleware needed
// (this migration's "boring primitive over fragile fidelity" pattern). The
// floating arrow triangle is dropped for the same reason: marginal fidelity for
// real added complexity.
Item {
    id: overlay
    anchors.fill: parent
    visible: TourController.active
    z: 10000
    focus: TourController.active

    Keys.onEscapePressed: TourController.skip()
    Keys.onLeftPressed: TourController.back()
    Keys.onRightPressed: TourController.next()

    property bool showDeep: false
    property bool satisfied: false

    readonly property var step: TourController.current
    readonly property string stepId: step ? step.id : ""
    readonly property string title: step ? I18n.t("tour.steps." + stepId + ".title") : ""
    readonly property string body: step ? I18n.t("tour.steps." + stepId + ".body") : ""
    readonly property bool hasDeep: !!(step && step.deep)
    readonly property string deepText: hasDeep ? I18n.t("tour.steps." + stepId + ".deep") : ""
    readonly property bool hasAction: !!(step && step.action)
    readonly property string actionText: hasAction ? I18n.t("tour.steps." + stepId + ".action") : ""
    readonly property bool showAudioSetupCta: !!(step && step.cta === "audioSetup" && !AudioController.virtualAudioReady)
    readonly property bool showRecorderCta: !!(step && step.cta === "recorderInstall")
    // Once the audio engine is ready, the audioSetup CTA gives way to the split suggestion.
    readonly property bool showAction: hasAction && (!step || step.cta !== "audioSetup" || AudioController.virtualAudioReady)

    onStepChanged: { showDeep = false; satisfied = false }

    Timer {
        interval: 300
        repeat: true
        running: overlay.visible && overlay.hasAction
        onTriggered: {
            if (overlay.stepId !== "mixerAction")
                return
            var apps = []
            try { apps = JSON.parse(AudioController.appsJson || "[]") } catch (e) { apps = [] }
            overlay.satisfied = apps.some(function (a) { return !!a.channel })
        }
    }

    // ── Target geometry (mapped into overlay space; tracks the target's own
    //    geometry changes since mapToItem reads its x/y/width/height chain) ──
    readonly property var targetItem: (step && step.target && TourController.targets[step.target]) || null
    readonly property real targetX: targetItem ? targetItem.mapToItem(overlay, 0, 0).x : 0
    readonly property real targetY: targetItem ? targetItem.mapToItem(overlay, 0, 0).y : 0
    readonly property real targetW: targetItem ? targetItem.width : 0
    readonly property real targetH: targetItem ? targetItem.height : 0
    readonly property bool hasTarget: !!targetItem && targetW > 0
    readonly property bool targetTooBig: hasTarget
        && (targetH > overlay.height * 0.7 || targetW > overlay.width * 0.85)
    readonly property bool anchored: hasTarget && !targetTooBig

    readonly property int pad: 6
    readonly property int gap: 14
    readonly property int edge: 10

    readonly property string placement: {
        var p = (step && step.placement) ? step.placement : "bottom"
        if (!I18n.rtl) return p
        if (p === "left") return "right"
        if (p === "right") return "left"
        return p
    }

    // ── Spotlight: four dim panels around the target, or a full dim ──
    Rectangle {
        visible: overlay.anchored
        color: "#9e000000"
        x: 0; y: 0; width: parent.width; height: Math.max(0, overlay.targetY - overlay.pad)
    }
    Rectangle {
        visible: overlay.anchored
        color: "#9e000000"
        x: 0
        y: overlay.targetY - overlay.pad + overlay.targetH + overlay.pad * 2
        width: parent.width
        height: Math.max(0, parent.height - y)
    }
    Rectangle {
        visible: overlay.anchored
        color: "#9e000000"
        x: 0
        y: overlay.targetY - overlay.pad
        width: Math.max(0, overlay.targetX - overlay.pad)
        height: overlay.targetH + overlay.pad * 2
    }
    Rectangle {
        visible: overlay.anchored
        color: "#9e000000"
        x: overlay.targetX - overlay.pad + overlay.targetW + overlay.pad * 2
        y: overlay.targetY - overlay.pad
        width: Math.max(0, parent.width - x)
        height: overlay.targetH + overlay.pad * 2
    }
    Rectangle {
        visible: !overlay.anchored
        anchors.fill: parent
        color: "#9e000000"
    }

    // Highlight ring around the target (clicks pass through it).
    Rectangle {
        visible: overlay.anchored
        x: overlay.targetX - overlay.pad
        y: overlay.targetY - overlay.pad
        width: overlay.targetW + overlay.pad * 2
        height: overlay.targetH + overlay.pad * 2
        radius: 10
        color: "transparent"
        border.width: 2
        border.color: overlay.satisfied ? "#22c55e" : Theme.accent
    }

    // ── Card ──────────────────────────────────────────────────────────────
    Rectangle {
        id: card
        width: 340
        height: cardCol.implicitHeight + 36
        radius: 14
        color: Theme.surface
        border.width: 1
        border.color: Theme.border

        x: overlay.anchored
            ? Math.min(Math.max(cardRawX, overlay.edge), Math.max(overlay.edge, overlay.width - width - overlay.edge))
            : (overlay.width - width) / 2
        y: overlay.anchored
            ? Math.min(Math.max(cardRawY, overlay.edge), Math.max(overlay.edge, overlay.height - height - overlay.edge))
            : (overlay.height - height) / 2

        readonly property real cardRawX: {
            if (overlay.placement === "left") return overlay.targetX - overlay.pad - overlay.gap - width
            if (overlay.placement === "right") return overlay.targetX + overlay.targetW + overlay.pad + overlay.gap
            return overlay.targetX + overlay.targetW / 2 - width / 2
        }
        readonly property real cardRawY: {
            if (overlay.placement === "top") return overlay.targetY - overlay.pad - overlay.gap - height
            if (overlay.placement === "bottom") return overlay.targetY + overlay.targetH + overlay.pad + overlay.gap
            return overlay.targetY + overlay.targetH / 2 - height / 2
        }

        // Absorb clicks so they don't fall through to whatever's underneath.
        MouseArea { anchors.fill: parent }

        Column {
            id: cardCol
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 18
            spacing: 12

            // Step dots
            Flow {
                width: parent.width
                spacing: 6
                Repeater {
                    model: TourController.steps.length
                    Rectangle {
                        width: 6; height: 6; radius: 3
                        color: index === TourController.stepIndex ? Theme.accent
                             : index < TourController.stepIndex ? Qt.rgba(0.914, 0.271, 0.376, 0.55)
                             : Theme.border
                    }
                }
            }

            Text {
                width: parent.width
                text: overlay.title
                color: Theme.text
                font.pixelSize: 17
                font.weight: Font.Black
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                text: overlay.body
                color: Theme.textDim
                font.pixelSize: 13
                lineHeight: 1.5
                wrapMode: Text.WordWrap
            }

            // Two-tier "More" expander
            Text {
                visible: overlay.hasDeep
                text: overlay.showDeep ? I18n.t("tour.controls.less") : I18n.t("tour.controls.more")
                color: Theme.accent
                font.pixelSize: 12
                font.weight: Font.Bold

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: overlay.showDeep = !overlay.showDeep
                }
            }
            Rectangle {
                visible: overlay.hasDeep && overlay.showDeep
                width: parent.width
                height: deepTextItem.implicitHeight + 20
                radius: 8
                color: Theme.bg
                border.width: 1
                border.color: Theme.border
                Text {
                    id: deepTextItem
                    anchors.fill: parent
                    anchors.margins: 10
                    text: overlay.deepText
                    color: Theme.textDim
                    font.pixelSize: 12
                    lineHeight: 1.5
                    wrapMode: Text.WordWrap
                }
            }

            // State-aware: set up the audio engine (one-click — no wizard in
            // this shell; directly calls the same AudioController invokable
            // MixerRoutingPanel.qml's Danger Zone button already uses).
            Rectangle {
                visible: overlay.showAudioSetupCta
                width: parent.width; height: 36
                radius: 8
                color: Theme.accent
                Text {
                    anchors.centerIn: parent
                    text: I18n.t("tour.controls.setupAudio")
                    color: "#ffffff"
                    font.pixelSize: 13
                    font.weight: Font.Bold
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: AudioController.createVirtualAudio()
                }
            }

            // State-aware: recorder install helper — link to Capture & Sound
            // settings instead of an inline distro-aware installer widget
            // (RecorderInstallHelper.qml already covers that in Settings).
            Rectangle {
                visible: overlay.showRecorderCta
                width: parent.width; height: 32
                radius: 8
                color: "transparent"
                border.width: 1
                border.color: Theme.border
                Text {
                    anchors.centerIn: parent
                    text: I18n.t("settings.captureSound.title")
                    color: Theme.text
                    font.pixelSize: 12
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: TourController.navigateRequested("settings")
                }
            }

            // Audio-split suggestion — ✓ when satisfied, never auto-advances.
            Rectangle {
                visible: overlay.showAction && overlay.actionText.length > 0
                width: parent.width; height: 32
                radius: 8
                color: overlay.satisfied
                    ? Qt.rgba(0.133, 0.773, 0.369, 0.12)
                    : Qt.rgba(0.914, 0.271, 0.376, 0.1)
                border.width: 1
                border.color: overlay.satisfied ? "#22c55e" : Theme.accent
                Text {
                    anchors.centerIn: parent
                    text: overlay.satisfied ? I18n.t("tour.controls.done") : overlay.actionText
                    color: overlay.satisfied ? "#22c55e" : Theme.accent
                    font.pixelSize: 12
                    font.weight: Font.Bold
                }
            }

            Row {
                spacing: 8
                ToggleSwitch {
                    id: dsaToggle
                    checked: TourController.dontShowAgain
                    onToggled: (v) => TourController.dontShowAgain = v
                    scale: 0.8
                    anchors.verticalCenter: parent.verticalCenter
                }
                Text {
                    text: I18n.t("tour.controls.dontShowAgain")
                    color: Theme.textDim
                    font.pixelSize: 11
                    anchors.verticalCenter: parent.verticalCenter
                }
            }

            Item { width: parent.width; height: 4 }

            Item {
                id: controlsRow
                width: parent.width
                height: 30

                Text {
                    text: I18n.t("tour.controls.skip")
                    color: Theme.textDim
                    font.pixelSize: 12
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: TourController.skip() }
                }

                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8

                    Rectangle {
                        visible: !TourController.isFirst
                        width: 64; height: 30; radius: 8
                        color: "transparent"
                        border.width: 1
                        border.color: Theme.border
                        Text { anchors.centerIn: parent; text: I18n.t("tour.controls.back"); color: Theme.textDim; font.pixelSize: 12 }
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: TourController.back() }
                    }
                    Rectangle {
                        width: 72; height: 30; radius: 8
                        color: Theme.accent
                        Text {
                            anchors.centerIn: parent
                            text: TourController.isLast ? I18n.t("tour.controls.finish") : I18n.t("tour.controls.next")
                            color: "#ffffff"
                            font.pixelSize: 12
                            font.weight: Font.Bold
                        }
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: TourController.next() }
                    }
                }
            }
        }
    }
}

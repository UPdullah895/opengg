import QtQuick
import com.opengg.app

// Small wired/wireless/bluetooth pill shown beside a device's name.
//
// `connection` is the daemon's own `connection` field, derived from kernel
// USB/HID topology (`daemon/src/device/connection.rs`) — NOT from matching the
// word "Wireless" in the product name, which describes what the hardware
// supports rather than how it is attached right now. The connected G502
// reports "LIGHTSPEED Wireless Gaming Mouse" as its name while sitting on a
// charging cable, and this badge correctly reads "Wired" for it.
//
// Anything the daemon couldn't determine arrives as an empty string and
// renders nothing at all, rather than defaulting to either claim.
Rectangle {
    id: badge

    property string connection: ""

    readonly property string label: badge.connection === "wired" ? "Wired"
                                  : badge.connection === "wireless" ? "Wireless"
                                  : badge.connection === "bluetooth" ? "Bluetooth"
                                  : ""

    visible: badge.label !== ""
    implicitWidth: labelText.implicitWidth + 16
    implicitHeight: 18
    radius: 9
    color: Theme.bgDeep
    border.width: 1
    border.color: Theme.border

    Text {
        id: labelText
        anchors.centerIn: parent
        text: badge.label
        color: Theme.textDim
        font.pixelSize: 10
        font.weight: Font.DemiBold
    }
}

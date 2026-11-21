import QtQuick
import QtQuick.Controls.Basic

// Share › Nearby Devices: phones and computers running LocalSend (an open
// AirDrop-like app) on the same network. Pick one to send the files.
Sheet {
    id: sh
    property var app
    property var win
    property var paths: []
    property var devices: []
    property bool looking: false
    property string error: ""
    title: "Send " + (paths.length === 1 ? "“" + paths[0].substring(paths[0].lastIndexOf("/") + 1) + "”" : paths.length + " items") + " to a Nearby Device"
    function start(p) { paths = p; devices = []; error = ""; open(); look() }
    function look() {
        looking = true
        Async.call(app.grove, k => app.grove.nearbyDevices(k), r => { sh.looking = false; if (r.error) sh.error = r.error; else sh.devices = r.devices })
    }
    GLabel {
        width: parent.width
        wrapMode: Text.Wrap; elide: Text.ElideNone
        color: Theme.fgSecondary
        text: "Devices running LocalSend (free for Android, iPhone, Windows, Mac and Linux) on this network show up here. On the other device, open LocalSend and accept the files."
    }
    Row {
        spacing: 8
        BusyDots { running: sh.looking; anchors.verticalCenter: parent.verticalCenter }
        GLabel { text: sh.looking ? "Looking for devices…" : sh.devices.length + " found"; color: Theme.fgSecondary }
        GButton { visible: !sh.looking; text: "Look Again"; flat: false; capsule: true; onClicked: sh.look() }
    }
    GLabel { visible: sh.error !== ""; text: sh.error; color: Theme.danger; width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone }
    Flow {
        width: parent.width
        spacing: 10
        Repeater {
            model: sh.devices
            Rectangle {
                required property var modelData
                width: 120; height: 100; radius: 12
                color: dm.containsMouse ? Theme.hover : Theme.field
                Column {
                    anchors.centerIn: parent
                    spacing: 6
                    GIcon { anchors.horizontalCenter: parent.horizontalCenter; size: 40; name: modelData.device_type === "mobile" ? "smartphone|phone" : modelData.device_type === "web" ? "internet-web-browser" : "computer" }
                    GLabel { anchors.horizontalCenter: parent.horizontalCenter; width: 110; horizontalAlignment: Text.AlignHCenter; text: modelData.alias; font.pixelSize: Theme.small }
                }
                MouseArea {
                    id: dm
                    anchors.fill: parent
                    hoverEnabled: true
                    onClicked: { sh.app.grove.shareLocalsend(JSON.stringify(modelData), JSON.stringify(sh.paths)); sh.close(); sh.win.toast("info", "Sending to " + modelData.alias + " — accept on the other device") }
                }
                GTip { visible: dm.containsMouse; text: modelData.alias + " (" + modelData.ip + ", " + modelData.device_model + ")" }
            }
        }
    }
    buttons: [ PushButton { text: "Cancel"; onClicked: sh.close() } ]
}

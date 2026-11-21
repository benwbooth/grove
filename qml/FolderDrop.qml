import QtQuick

// Drop onto a folder item: files go into it. Hovering a while opens the
// folder (spring-loaded, as in Finder).
DropArea {
    id: d
    property var pane
    property string path: ""
    property bool enabledFor: true
    property bool hovering: false
    keys: ["text/uri-list"]
    enabled: enabledFor
    onEntered: (drag) => { hovering = true; spring.restart() }
    onExited: { hovering = false; spring.stop() }
    onDropped: (drop) => { hovering = false; spring.stop(); pane.dropInto(d.path, drop) }
    Timer { id: spring; interval: 1200; onTriggered: if (d.hovering) d.pane.navigate(d.path) }
}

import QtQuick
import QtQuick.Controls.Basic
import Grove

// Grove Preview's sidebar: Thumbnails (every open document with its pages;
// drag pages to reorder them or move them to another document, drop files
// to insert them), Table of Contents, Highlights and Notes, Bookmarks, and
// search results while searching.
Item {
    id: sb
    property var win
    readonly property string mode: win.searchHits !== null ? "search" : win.sidebarMode
    clip: true

    Rectangle { anchors.fill: parent; color: Theme.sidebar }
    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.separator }

    // ---------- thumbnails ----------
    readonly property var rows: {
        const r = []
        const many = win.docs.length > 1
        for (const d of win.docs) {
            if (many) r.push({ t: "doc", doc: d })
            if (many && d.collapsed) continue
            if (d.kind === "image" || d.kind === "text") r.push({ t: "image", doc: d, i: 0 })
            else for (let i = 0; i < d.pages.length; i++) r.push({ t: "page", doc: d, i: i })
        }
        return r
    }
    property var drag: null          // {doc, from: [indices], y}
    property int dropRow: -1
    property bool dropAfter: false

    ListView {
        id: thumbs
        anchors.fill: parent
        anchors.rightMargin: 1
        visible: sb.mode === "thumbs"
        clip: true
        model: sb.rows
        topMargin: 10
        bottomMargin: 10
        ScrollBar.vertical: GScrollBar {}
        KineticWheel { flickable: thumbs }
        readonly property real box: Math.max(60, width - 56)
        delegate: Item {
            id: row
            required property var modelData
            required property int index
            readonly property var r: modelData
            width: thumbs.width
            height: r.t === "doc" ? Theme.sidebarRow + 6 : thumb.height + 30
            // A document's name (several open): click to fold its pages.
            Row {
                visible: row.r.t === "doc"
                x: 10; anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                GLabel { text: row.r.doc && row.r.doc.collapsed ? "▸" : "▾"; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
                GIcon { name: row.r.doc && row.r.doc.kind === "pdf" ? "application-pdf" : "image-x-generic"; size: 16; anchors.verticalCenter: parent.verticalCenter }
                GLabel {
                    text: row.r.doc ? row.r.doc.name + (row.r.doc.dirty ? " — Edited" : "") : ""
                    font.bold: sb.win.cur === row.r.doc
                    width: thumbs.width - 60; elide: Text.ElideMiddle
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
            MouseArea {
                id: headArea
                anchors.fill: parent
                visible: row.r.t === "doc"
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: (m) => {
                    if (m.button === Qt.RightButton) { sb.menuDoc = row.r.doc; docMenu.popup(); return }
                    sb.win.toggleCollapsed(row.r.doc)
                }
                GTip { visible: parent.containsMouse && !closeBtn.hovered; text: "Show or hide this document's pages (right-click to close it)" }
                hoverEnabled: true
            }
            // Close this document (asks to save changes first).
            GButton {
                id: closeBtn
                visible: row.r.t === "doc" && (headArea.containsMouse || hovered)
                anchors.right: parent.right; anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                iconName: "window-close"; iconSize: 10
                tip: "Close “" + (row.r.doc ? row.r.doc.name : "") + "” (other documents stay open)"
                onClicked: sb.win.closeDoc(row.r.doc)
            }
            // A page (or the picture).
            Rectangle {
                visible: row.r.t !== "doc"
                anchors.fill: thumb
                anchors.margins: -4
                radius: 5
                color: sb.win.isPageSelected(row.r.doc, row.r.i) ? Theme.accent : "transparent"
                opacity: sb.win.cur === row.r.doc ? 1 : 0.5
            }
            PvThumb {
                id: thumb
                visible: row.r.t !== "doc"
                anchors.horizontalCenter: parent.horizontalCenter
                y: 8
                win: sb.win
                doc: row.r.doc
                pg: row.r.t === "page" ? row.r.doc.pages[row.r.i] : null
                box: thumbs.box
            }
            Row {
                visible: row.r.t === "page"
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: thumb.bottom
                anchors.topMargin: 5
                spacing: 4
                GLabel { text: row.r.t === "page" ? String(row.r.i + 1) : ""; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                GIcon { visible: row.r.t === "page" && (row.r.doc.bookmarks || []).indexOf(row.r.i) >= 0; name: "bookmarks"; size: 12 }
            }
            // Where dragged pages would go.
            Rectangle {
                visible: sb.dropRow === row.index
                width: parent.width - 24; x: 12; height: 3; radius: 1.5
                color: Theme.accent
                y: sb.dropAfter ? parent.height - 2 : -1
            }
            MouseArea {
                anchors.fill: parent
                visible: row.r.t !== "doc"
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                hoverEnabled: true
                property real sy: 0
                property bool dragging: false
                onPressed: (m) => {
                    sy = m.y; dragging = false
                    if (m.button === Qt.RightButton) {
                        if (!sb.win.isPageSelected(row.r.doc, row.r.i)) sb.win.clickPage(row.r.doc, row.r.i, 0)
                        pageMenu.popup()
                        return
                    }
                    if (!sb.win.isPageSelected(row.r.doc, row.r.i) || (m.modifiers & (Qt.ControlModifier | Qt.ShiftModifier)))
                        sb.win.clickPage(row.r.doc, row.r.i, m.modifiers)
                    else sb.win.setCurrent(row.r.doc, row.r.i)
                }
                onPositionChanged: (m) => {
                    if (!pressed || row.r.t !== "page") return
                    if (!dragging && Math.abs(m.y - sy) > 8) { dragging = true; sb.drag = { doc: row.r.doc, from: sb.win.selectedPages(row.r.doc) } }
                    if (dragging) {
                        const p = mapToItem(thumbs.contentItem, m.x, m.y)
                        const at = thumbs.indexAt(10, p.y)
                        if (at >= 0) {
                            const it = thumbs.itemAtIndex(at)
                            sb.dropRow = at
                            sb.dropAfter = it ? p.y - it.y > it.height / 2 : false
                        }
                        // Near the edges: scroll.
                        const vy = mapToItem(thumbs, m.x, m.y).y
                        if (vy < 30) thumbs.contentY = Math.max(-thumbs.topMargin, thumbs.contentY - 12)
                        else if (vy > thumbs.height - 30) thumbs.contentY = Math.min(thumbs.contentHeight - thumbs.height, thumbs.contentY + 12)
                    }
                }
                onReleased: {
                    if (dragging && sb.drag && sb.dropRow >= 0) {
                        const tr = sb.rows[sb.dropRow]
                        if (tr.t === "page") sb.win.movePages(sb.drag.doc, sb.drag.from, tr.doc, tr.i + (sb.dropAfter ? 1 : 0))
                        else if (tr.t === "doc" && tr.doc.kind === "pdf") sb.win.movePages(sb.drag.doc, sb.drag.from, tr.doc, 0)
                    }
                    dragging = false; sb.drag = null; sb.dropRow = -1
                }
                onDoubleClicked: if (row.r.t === "image") sb.win.setCurrent(row.r.doc, 0)
                GTip { visible: parent.containsMouse && !parent.pressed; text: row.r.t === "page" ? "Page " + (row.r.i + 1) + ". Drag to move it; right-click for more" : row.r.doc.name }
            }
            // Files dropped here go in after this page.
            DropArea {
                anchors.fill: parent
                enabled: row.r.t === "page"
                onEntered: (d) => { sb.dropRow = row.index; sb.dropAfter = true }
                onExited: sb.dropRow = -1
                onDropped: (d) => {
                    sb.dropRow = -1
                    if (d.hasUrls) { sb.win.insertFiles(row.r.doc, row.r.i + 1, d.urls.map(u => decodeURIComponent(String(u).replace("file://", "")))); d.accept() }
                }
            }
        }
        // Files dropped below the pages: opened in this window.
        DropArea {
            anchors.fill: parent
            z: -1
            onDropped: (d) => { if (d.hasUrls) { sb.win.openFiles(d.urls.map(u => decodeURIComponent(String(u).replace("file://", ""))), false); d.accept() } }
        }
    }

    property var menuDoc: null
    GMenu {
        id: docMenu
        GMenuItem { text: "Close Document"; iconName: "document-close"; onTriggered: sb.win.closeDoc(sb.menuDoc) }
        GMenuItem { text: "Close Other Documents"; iconName: "document-close"; onTriggered: { for (const d of sb.win.docs.slice()) if (d.id !== sb.menuDoc.id) sb.win.closeDoc(d) } }
    }
    GMenu {
        id: pageMenu
        GMenuItem { text: "Rotate Left"; iconName: "object-rotate-left"; shortcutText: sb.win.keyFor("pv.rotleft"); onTriggered: sb.win.rotate(-90) }
        GMenuItem { text: "Rotate Right"; iconName: "object-rotate-right"; shortcutText: sb.win.keyFor("pv.rotright"); onTriggered: sb.win.rotate(90) }
        GMenuSeparator {}
        GMenuItem { text: "Insert Blank Page"; iconName: "document-new"; enabled: sb.win.cur && sb.win.cur.kind === "pdf"; onTriggered: sb.win.insertBlank() }
        GMenuItem { text: "Insert Page from File…"; iconName: "document-import"; enabled: sb.win.cur && sb.win.cur.kind === "pdf"; onTriggered: sb.win.insertFromFile() }
        GMenuItem { text: "Export Page as Picture…"; iconName: "document-export"; enabled: sb.win.cur && sb.win.cur.kind === "pdf"; onTriggered: sb.win.exportPage() }
        GMenuItem { text: "Add Bookmark"; iconName: "bookmark-new"; enabled: sb.win.cur && sb.win.cur.kind === "pdf"; onTriggered: sb.win.toggleBookmark() }
        GMenuSeparator {}
        GMenuItem { text: "Delete"; iconName: "edit-delete"; shortcutText: "Delete"; enabled: sb.win.cur && sb.win.cur.kind === "pdf"; onTriggered: sb.win.deletePages() }
    }

    // ---------- table of contents ----------
    property var tocOpen: ({})
    readonly property var tocRows: {
        const out = []
        const d = win.cur
        if (!d || !d.info || !d.info.outline) return out
        const walk = (list, depth, path) => {
            list.forEach((o, i) => {
                const id = path + "/" + i
                out.push({ o: o, depth: depth, id: id, kids: o.kids && o.kids.length > 0 })
                if (o.kids && o.kids.length && (tocOpen[id] !== undefined ? tocOpen[id] : depth < 1)) walk(o.kids, depth + 1, id)
            })
        }
        walk(d.info.outline, 0, "")
        return out
    }
    ListView {
        id: toc
        anchors.fill: parent
        anchors.margins: 6
        visible: sb.mode === "toc"
        clip: true
        model: sb.tocRows
        ScrollBar.vertical: GScrollBar {}
        KineticWheel { flickable: toc }
        delegate: Item {
            required property var modelData
            width: toc.width
            height: Theme.sidebarRow
            Rectangle { anchors.fill: parent; radius: 5; color: tm.containsMouse ? Theme.hover : "transparent" }
            GLabel {
                x: 4 + modelData.depth * 14
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.kids ? ((sb.tocOpen[modelData.id] !== undefined ? sb.tocOpen[modelData.id] : modelData.depth < 1) ? "▾" : "▸") : ""
                color: Theme.fgSecondary
                MouseArea {
                    anchors.fill: parent; anchors.margins: -4
                    onClicked: { const o = Object.assign({}, sb.tocOpen); const id = modelData.id; o[id] = !(o[id] !== undefined ? o[id] : modelData.depth < 1); sb.tocOpen = o }
                }
            }
            GLabel {
                x: 18 + modelData.depth * 14
                width: parent.width - x - 36
                anchors.verticalCenter: parent.verticalCenter
                text: modelData.o.title
                elide: Text.ElideRight
                font.bold: modelData.depth === 0
            }
            GLabel { anchors.right: parent.right; anchors.rightMargin: 6; anchors.verticalCenter: parent.verticalCenter; text: modelData.o.page >= 0 ? String(modelData.o.page + 1) : ""; color: Theme.fgSecondary; font.pixelSize: Theme.small }
            MouseArea {
                id: tm
                anchors.fill: parent; anchors.leftMargin: 16 + modelData.depth * 14
                hoverEnabled: true
                onClicked: sb.win.gotoOutline(modelData.o)
                GTip { visible: parent.containsMouse; text: modelData.o.title }
            }
        }
        GLabel { anchors.centerIn: parent; visible: toc.count === 0; text: "This document has no table of contents."; color: Theme.fgSecondary; width: parent.width - 20; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter }
    }

    // ---------- highlights and notes ----------
    ListView {
        id: notes
        anchors.fill: parent
        anchors.margins: 6
        visible: sb.mode === "notes"
        clip: true
        model: sb.mode === "notes" ? sb.win.notesOf(sb.win.cur) : []
        spacing: 4
        ScrollBar.vertical: GScrollBar {}
        KineticWheel { flickable: notes }
        delegate: Rectangle {
            required property var modelData
            width: notes.width
            height: col.implicitHeight + 12
            radius: 6
            color: nm.containsMouse ? Theme.hover : Theme.field
            Rectangle { x: 0; width: 4; height: parent.height; radius: 2; color: modelData.color || "#ffcc00" }
            Column {
                id: col
                x: 12; y: 6
                width: parent.width - 44
                spacing: 2
                GLabel { text: modelData.kind + " · Page " + (modelData.index + 1) + (modelData.author ? " · " + modelData.author : ""); color: Theme.fgSecondary; font.pixelSize: Theme.small }
                GLabel { width: parent.width; text: modelData.text || "—"; wrapMode: Text.Wrap; maximumLineCount: 4; elide: Text.ElideRight }
            }
            MouseArea { id: nm; anchors.fill: parent; hoverEnabled: true; onClicked: sb.win.gotoNote(modelData) }
            GButton {
                anchors.right: parent.right; anchors.rightMargin: 4; anchors.verticalCenter: parent.verticalCenter
                iconName: "edit-delete"; iconSize: 14
                tip: "Remove this " + modelData.kind.toLowerCase()
                onClicked: sb.win.deleteNote(modelData)
            }
        }
        GLabel { anchors.centerIn: parent; visible: notes.count === 0; text: "No highlights or notes yet. Use the highlighter or Tools › Annotate › Note."; color: Theme.fgSecondary; width: parent.width - 20; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter }
    }

    // ---------- bookmarks ----------
    ListView {
        id: marks
        anchors.fill: parent
        anchors.margins: 6
        visible: sb.mode === "bookmarks"
        clip: true
        model: sb.win.cur ? (sb.win.cur.bookmarks || []) : []
        ScrollBar.vertical: GScrollBar {}
        delegate: Item {
            required property var modelData
            width: marks.width
            height: Theme.sidebarRow + 4
            Rectangle { anchors.fill: parent; radius: 5; color: bm.containsMouse ? Theme.hover : "transparent" }
            GIcon { x: 6; anchors.verticalCenter: parent.verticalCenter; name: "bookmarks"; size: 14 }
            GLabel { x: 28; anchors.verticalCenter: parent.verticalCenter; text: "Page " + (modelData + 1) }
            MouseArea { id: bm; anchors.fill: parent; hoverEnabled: true; onClicked: sb.win.setCurrent(sb.win.cur, modelData) }
            GButton { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; iconName: "edit-delete"; iconSize: 14; tip: "Remove this bookmark"; onClicked: sb.win.toggleBookmark(modelData) }
        }
        GLabel { anchors.centerIn: parent; visible: marks.count === 0; text: "No bookmarks. Tools › Add Bookmark (" + sb.win.keyFor("pv.bookmark") + ") marks the page you're on."; color: Theme.fgSecondary; width: parent.width - 20; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter }
    }

    // ---------- search results ----------
    Column {
        id: shead
        visible: sb.mode === "search"
        x: 10; y: 8
        width: parent.width - 20
        GLabel {
            text: sb.win.searching ? "Searching…" : (sb.win.searchHits || []).length === 0 ? "Not found" : (sb.win.searchHits || []).length + ((sb.win.searchHits || []).length === 1 ? " match" : " matches")
            font.bold: true
        }
    }
    ListView {
        id: results
        anchors.fill: parent
        anchors.topMargin: shead.height + 16
        anchors.margins: 6
        visible: sb.mode === "search"
        clip: true
        model: sb.win.searchHits || []
        ScrollBar.vertical: GScrollBar {}
        KineticWheel { flickable: results }
        currentIndex: sb.win.searchIndex
        onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
        delegate: Rectangle {
            required property var modelData
            required property int index
            width: results.width
            height: rc.implicitHeight + 10
            radius: 5
            color: index === sb.win.searchIndex ? Theme.accent : rm.containsMouse ? Theme.hover : "transparent"
            Column {
                id: rc
                x: 8; y: 5
                width: parent.width - 16
                GLabel { text: (sb.win.docs.length > 1 ? sb.win.docById(modelData.docId).name + " · " : "") + "Page " + (modelData.index + 1); font.pixelSize: Theme.small; color: index === sb.win.searchIndex ? Theme.accentText : Theme.fgSecondary }
                GLabel {
                    width: parent.width
                    textFormat: Text.StyledText
                    text: sb.win.hitSnippet(modelData)
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    color: index === sb.win.searchIndex ? Theme.accentText : Theme.fg
                }
            }
            MouseArea { id: rm; anchors.fill: parent; hoverEnabled: true; onClicked: sb.win.gotoHit(index) }
        }
    }
}

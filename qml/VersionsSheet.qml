import QtQuick
import QtQuick.Controls.Basic

// Browse Previous Versions (Time Machine style) from the hourly btrfs
// snapshots, as a calendar: all years, a year's months, a month's weeks
// and days. Times with snapshots are highlighted (with how many); click a
// year or month to open it, a week or day to list its versions below.
// Show opens a version (read-only) in the pane; Restore copies it back
// (you're asked whether to replace the current one or keep both).
Sheet {
    id: vs
    property var app
    property var win
    property string path: ""
    property var list: []
    property bool loading: false
    property bool hasSnapshots: true
    // "years" | "year" | "month"
    property string level: "month"
    property int year: new Date().getFullYear()
    property int month: new Date().getMonth()
    // What the list below shows: {kind: "year"|"month"|"week"|"day", start, end} (ms).
    property var sel: null
    readonly property var counts: {
        const c = { y: {}, m: {}, d: {} }
        for (const v of list) {
            const d = new Date(v.date * 1000)
            const y = d.getFullYear(), m = d.getMonth(), day = d.getDate()
            c.y[y] = (c.y[y] || 0) + 1
            c.m[y + "-" + m] = (c.m[y + "-" + m] || 0) + 1
            c.d[y + "-" + m + "-" + day] = (c.d[y + "-" + m + "-" + day] || 0) + 1
        }
        return c
    }
    readonly property var shown: sel ? list.filter(v => v.date * 1000 >= sel.start && v.date * 1000 < sel.end) : []
    readonly property var firstYear: list.length ? new Date(list[list.length - 1].date * 1000).getFullYear() : year
    readonly property var lastYear: list.length ? new Date(list[0].date * 1000).getFullYear() : year
    readonly property var monthNames: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map(i => Qt.locale().standaloneMonthName(i, Locale.ShortFormat))
    readonly property int weekStart: Qt.locale().firstDayOfWeek % 7   // 0 = Sunday

    title: "Previous Versions of “" + path.substring(path.lastIndexOf("/") + 1) + "”"
    width: Math.min(parent ? parent.width - 40 : 620, Math.round(560 * Theme.pt))

    function start(p) {
        path = p; list = []; loading = true; sel = null
        open()
        Async.call(app.grove, k => app.grove.versions(k, p), r => {
            vs.list = r.list || []
            vs.hasSnapshots = r.snapshots !== false
            vs.loading = false
            vs.newest()
        })
    }
    // The month of the newest version, its day selected.
    function newest() {
        const d = list.length ? new Date(list[0].date * 1000) : new Date()
        year = d.getFullYear(); month = d.getMonth(); level = "month"
        if (list.length) pickDay(d.getFullYear(), d.getMonth(), d.getDate())
    }
    function pickYear(y) { year = y; level = "year"; sel = { kind: "year", start: new Date(y, 0, 1).getTime(), end: new Date(y + 1, 0, 1).getTime() } }
    function pickMonth(y, m) { year = y; month = m; level = "month"; sel = { kind: "month", start: new Date(y, m, 1).getTime(), end: new Date(y, m + 1, 1).getTime() } }
    function pickDay(y, m, d) { sel = { kind: "day", start: new Date(y, m, d).getTime(), end: new Date(y, m, d + 1).getTime() } }
    function pickWeek(first) { sel = { kind: "week", start: first.getTime(), end: new Date(first.getFullYear(), first.getMonth(), first.getDate() + 7).getTime() } }
    function step(n) {
        if (level === "years") return
        if (level === "year") { year += n; return }
        let m = month + n
        year += Math.floor(m / 12); month = ((m % 12) + 12) % 12
    }
    function countIn(a, b) { let n = 0; for (const v of list) { const t = v.date * 1000; if (t >= a && t < b) n++ } return n }
    // ISO 8601 week number of the week holding `d`.
    function isoWeek(d) {
        const t = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()))
        const day = t.getUTCDay() || 7
        t.setUTCDate(t.getUTCDate() + 4 - day)
        const y0 = new Date(Date.UTC(t.getUTCFullYear(), 0, 1))
        return Math.ceil(((t - y0) / 86400000 + 1) / 7)
    }
    // Test hook (GROVE_TEST vview=years|year|month|newest|week:ROW|day:N).
    function testAction(a) {
        const [c, x] = a.split(":")
        if (c === "years") level = "years"
        else if (c === "year") pickYear(year)
        else if (c === "month") pickMonth(year, month)
        else if (c === "newest") newest()
        else if (c === "week") { const f = gridStart(); pickWeek(new Date(f.getFullYear(), f.getMonth(), f.getDate() + 7 * Number(x))) }
        else if (c === "day") pickDay(year, month, Number(x))
        console.warn("grove test versions", level, year, month, JSON.stringify(sel), shown.length)
    }
    // The first day shown in the month grid (the week holding the 1st).
    function gridStart() {
        const first = new Date(year, month, 1)
        const back = (first.getDay() - weekStart + 7) % 7
        return new Date(year, month, 1 - back)
    }

    component Cell: Rectangle {
        id: cell
        property string label: ""
        property int count: 0
        property bool picked: false
        property bool dim: false
        property string tip: ""
        signal clicked()
        radius: Theme.radius
        color: picked ? Theme.accent
             : count > 0 ? Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, ma.containsMouse ? 0.38 : 0.24)
             : ma.containsMouse ? Theme.hover : "transparent"
        border.width: count > 0 && !picked ? 1 : 0
        border.color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.6)
        GLabel {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: cell.count > 0 && cell.height > 38 ? -6 : 0
            text: cell.label
            color: cell.picked ? Theme.accentText : cell.dim ? Theme.fgTertiary : Theme.fg
            font.bold: cell.count > 0
        }
        GLabel {
            visible: cell.count > 0 && cell.height > 38
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom; anchors.bottomMargin: 3
            text: cell.count
            font.pixelSize: Theme.caption
            color: cell.picked ? Theme.accentText : Theme.fgSecondary
        }
        MouseArea { id: ma; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: cell.clicked() }
        GTip { visible: ma.containsMouse && cell.tip !== ""; text: cell.tip }
    }

    GLabel {
        width: parent.width
        visible: vs.loading || vs.list.length === 0
        wrapMode: Text.Wrap; elide: Text.ElideNone
        color: Theme.fgSecondary
        text: vs.loading ? "Looking through snapshots…"
            : !vs.hasSnapshots ? "This disk has no snapshots, so there are no earlier versions. Snapper takes hourly snapshots only of the disks it's set up for (here: /home)."
            : "No earlier versions: this item hasn't changed since the oldest snapshot, or it's newer than all of them."
    }

    // ---- the calendar ----
    Column {
        id: cal
        visible: vs.list.length > 0
        width: parent.width
        spacing: 6

        Row {
            width: parent.width
            spacing: 6
            GButton {
                iconName: "go-up"
                enabled: vs.level !== "years"
                tip: vs.level === "month" ? "The whole year" : "All years"
                onClicked: vs.level === "month" ? vs.pickYear(vs.year) : vs.level = "years"
            }
            GButton { iconName: "go-previous"; enabled: vs.level !== "years"; tip: vs.level === "year" ? "Previous year" : "Previous month"; onClicked: vs.step(-1) }
            GButton { iconName: "go-next"; enabled: vs.level !== "years"; tip: vs.level === "year" ? "Next year" : "Next month"; onClicked: vs.step(1) }
            GLabel {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - x - newestBtn.width - 6
                font.bold: true
                font.pixelSize: Theme.title
                text: vs.level === "years" ? "All Years" : vs.level === "year" ? String(vs.year) : Qt.locale().standaloneMonthName(vs.month) + " " + vs.year
            }
            GButton { id: newestBtn; text: "Newest"; flat: false; capsule: true; tip: "Go to the newest version"; onClicked: vs.newest() }
        }

        // All years.
        Grid {
            visible: vs.level === "years"
            columns: 4
            spacing: 6
            Repeater {
                model: Math.max(1, vs.lastYear - vs.firstYear + 1)
                Cell {
                    required property int index
                    readonly property int yr: vs.firstYear + index
                    width: (cal.width - 18) / 4; height: Math.round(44 * Theme.pt)
                    label: String(yr)
                    count: vs.counts.y[yr] || 0
                    picked: !!vs.sel && vs.sel.kind === "year" && vs.year === yr
                    tip: count ? count + " versions in " + yr + ". Click to see its months" : "No snapshots in " + yr
                    onClicked: vs.pickYear(yr)
                }
            }
        }

        // A year's months.
        Grid {
            visible: vs.level === "year"
            columns: 4
            spacing: 6
            Repeater {
                model: 12
                Cell {
                    required property int index
                    width: (cal.width - 18) / 4; height: Math.round(44 * Theme.pt)
                    label: vs.monthNames[index]
                    count: vs.counts.m[vs.year + "-" + index] || 0
                    picked: !!vs.sel && vs.sel.kind === "month" && vs.month === index && vs.sel.start === new Date(vs.year, index, 1).getTime()
                    tip: count ? count + " versions in " + Qt.locale().standaloneMonthName(index) + ". Click to see its days" : "No snapshots in " + Qt.locale().standaloneMonthName(index)
                    onClicked: vs.pickMonth(vs.year, index)
                }
            }
        }

        // A month's weeks and days.
        Column {
            visible: vs.level === "month"
            spacing: 3
            readonly property real cw: (cal.width - 7 * 3) / 8
            Row {
                spacing: 3
                GLabel { width: parent.parent.cw; horizontalAlignment: Text.AlignHCenter; text: "Week"; color: Theme.fgSecondary; font.pixelSize: Theme.caption }
                Repeater {
                    model: 7
                    GLabel {
                        required property int index
                        width: parent.parent.cw; horizontalAlignment: Text.AlignHCenter
                        text: Qt.locale().standaloneDayName((vs.weekStart + index) % 7, Locale.ShortFormat)
                        color: Theme.fgSecondary; font.pixelSize: Theme.caption
                    }
                }
            }
            Repeater {
                model: 6
                Row {
                    id: weekRow
                    required property int index
                    readonly property var first: { const g = vs.gridStart(); return new Date(g.getFullYear(), g.getMonth(), g.getDate() + 7 * index) }
                    spacing: 3
                    visible: index < 5 || first.getMonth() === vs.month
                    Cell {
                        width: parent.parent.cw; height: Math.round(36 * Theme.pt)
                        label: String(vs.isoWeek(weekRow.first))
                        count: vs.countIn(weekRow.first.getTime(), weekRow.first.getTime() + 7 * 86400000)
                        dim: true
                        picked: !!vs.sel && vs.sel.kind === "week" && vs.sel.start === weekRow.first.getTime()
                        tip: count ? count + " versions in week " + label + ". Click to list them" : "No snapshots in week " + label
                        onClicked: vs.pickWeek(weekRow.first)
                    }
                    Repeater {
                        model: 7
                        Cell {
                            required property int index
                            readonly property var day: new Date(weekRow.first.getFullYear(), weekRow.first.getMonth(), weekRow.first.getDate() + index)
                            width: parent.parent.cw; height: Math.round(36 * Theme.pt)
                            label: String(day.getDate())
                            dim: day.getMonth() !== vs.month
                            count: vs.counts.d[day.getFullYear() + "-" + day.getMonth() + "-" + day.getDate()] || 0
                            picked: !!vs.sel && vs.sel.kind === "day" && vs.sel.start === day.getTime()
                            tip: count ? count + (count === 1 ? " version" : " versions") + " on " + day.toLocaleDateString(Qt.locale(), "dddd d MMMM") + ". Click to list them" : "No snapshots on " + day.toLocaleDateString(Qt.locale(), "dddd d MMMM")
                            onClicked: { if (day.getMonth() !== vs.month) { vs.year = day.getFullYear(); vs.month = day.getMonth() } vs.pickDay(day.getFullYear(), day.getMonth(), day.getDate()) }
                        }
                    }
                }
            }
        }

        GLabel {
            width: parent.width
            color: Theme.fgSecondary
            wrapMode: Text.Wrap; elide: Text.ElideNone
            text: !vs.sel ? "Pick a day, week, month or year with a highlight."
                : vs.shown.length === 0 ? "No versions then."
                : vs.shown.length + (vs.shown.length === 1 ? " version" : " versions")
                  + (vs.list[0].is_dir ? " (the folder as it was; Show opens it)" : "")
        }
    }

    // ---- the versions in what's picked ----
    Rectangle {
        visible: vs.shown.length > 0
        width: parent.width
        height: Math.min(Math.round(200 * Theme.pt), Math.max(40, lv.contentHeight + 8))
        radius: 8
        color: Theme.field
        KineticWheel { flickable: lv }
        ListView {
            id: lv
            anchors.fill: parent
            anchors.margins: 4
            clip: true
            model: vs.shown
            ScrollBar.vertical: GScrollBar {}
            delegate: Item {
                required property var modelData
                width: lv.width
                height: Theme.rowHeight + 8
                GLabel {
                    x: 8; anchors.verticalCenter: parent.verticalCenter; width: parent.width * 0.45
                    text: new Date(modelData.date * 1000).toLocaleString(Qt.locale(), vs.sel && vs.sel.kind === "day" ? "h:mm AP" : "ddd d MMM yyyy, h:mm AP")
                }
                GLabel { x: parent.width * 0.45; anchors.verticalCenter: parent.verticalCenter; text: modelData.is_dir ? "Folder" : vs.app.size(modelData.size); color: Theme.fgSecondary }
                Row {
                    anchors.right: parent.right; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6
                    GButton {
                        text: "Show"; flat: false; capsule: true
                        tip: "Open this version (read-only) in the window"
                        onClicked: {
                            const p = modelData.path
                            vs.close()
                            if (modelData.is_dir) vs.win.go(p)
                            else vs.win.activePane.navigate(p.substring(0, p.lastIndexOf("/")), { select: [p] })
                        }
                    }
                    GButton {
                        text: "Restore"; flat: false; capsule: true
                        tip: "Copy this version back to where the item is now (you'll be asked about the current one)"
                        onClicked: {
                            const dest = vs.path.substring(0, vs.path.lastIndexOf("/")) || "/"
                            vs.app.grove.runJob("copy", JSON.stringify([modelData.path]), dest)
                            vs.close()
                        }
                    }
                }
            }
        }
    }
    buttons: [ PushButton { text: "Done"; primary: true; tip: "Close the calendar"; onClicked: vs.close() } ]
}

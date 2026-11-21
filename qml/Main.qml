import QtQuick
import Grove

// The application: one Grove controller, any number of browser windows,
// the file operation list, and requests from outside (D-Bus, a second
// launch, the portal's file chooser).
Item {
    id: app

    property Grove grove: Grove {}
    property var windows: []
    property var activeWindow: null
    property var settings: JSON.parse(grove.settingsJson || "{}")
    property ListModel jobs: ListModel {}
    property bool service: grove.env("GROVE_SERVICE") === "1"
    property bool portalMode: grove.env("GROVE_PORTAL") === "1"
    property string screenshot: grove.env("GROVE_SCREENSHOT")
    property int trashCount: 0
    property var closedTabs: []

    Component { id: winComp; BrowserWindow {} }
    Component { id: chooserComp; FileChooser {} }

    function openWindow(paths, selectPaths) {
        const w = winComp.createObject(null, { app: app, initialPaths: paths || [], initialSelect: selectPaths || [] })
        windows.push(w)
        w.closing.connect(function () {
            windows = windows.filter(x => x !== w)
            if (activeWindow === w) activeWindow = windows.length ? windows[windows.length - 1] : null
        })
        w.show()
        w.raise()
        w.requestActivate()
        activeWindow = w
        return w
    }

    function handleOpen(req) {
        const paths = req.paths || []
        if (req.action === "properties") {
            for (const p of paths) {
                const w = activeWindow || openWindow([grove.home()])
                w.showInfo([p])
            }
            return
        }
        if (req.action === "items") {
            // Open each item's folder with the item selected.
            const byDir = {}
            for (const p of paths) {
                const d = p.substring(0, p.lastIndexOf("/")) || "/"
                ;(byDir[d] = byDir[d] || []).push(p)
            }
            for (const d in byDir) openOrTab([d], byDir[d])
            return
        }
        if (req.action === "window" || paths.length === 0) { openWindow([grove.home()]); return }
        openOrTab(paths, [])
    }

    function openOrTab(paths, select) {
        if (activeWindow && !app.settings.new_window_per_request) {
            for (const p of paths) activeWindow.newTab(p, select)
            activeWindow.raise()
            activeWindow.requestActivate()
        } else {
            openWindow(paths, select)
        }
    }

    function findJob(id) {
        for (let i = 0; i < jobs.count; i++) if (jobs.get(i).jobId === id) return i
        return -1
    }

    Connections {
        target: app.grove
        function onOpenRequest(json) { app.handleOpen(JSON.parse(json)) }
        function onSettingsJsonChanged() { app.settings = JSON.parse(app.grove.settingsJson); Theme.mode = app.settings.theme || "auto" }
        function onJobChanged(id, json) {
            const j = JSON.parse(json)
            const row = {
                jobId: id, title: j.title || "", state: j.state || "running",
                fraction: j.totalBytes > 0 ? j.doneBytes / j.totalBytes : (j.totalItems > 0 ? j.doneItems / j.totalItems : 0),
                detail: j.state === "running" || j.state === "paused"
                    ? (j.totalBytes > 0 ? (app.size(j.doneBytes) + " of " + app.size(j.totalBytes)) : (j.indeterminate ? "Working…" : (j.doneItems + " of " + j.totalItems + " items")))
                      + (j.current ? " — " + j.current : "")
                    : j.state === "done" ? "Done" : j.state === "cancelled" ? "Stopped" : (j.errors && j.errors.length ? j.errors[0] : "Failed"),
                indeterminate: !!j.indeterminate, started: Date.now()
            }
            const i = app.findJob(id)
            if (i < 0) app.jobs.append(row)
            else { row.started = app.jobs.get(i).started; app.jobs.set(i, row) }
            if (j.state === "done" || j.state === "cancelled" || j.state === "failed") {
                if (j.errors && j.errors.length) app.notifyAll("error", j.errors.join("\n"))
                if (j.results && j.results.length && app.activeWindow) app.activeWindow.selectAfterJob(j.dest || "", j.results)
                cleanup.restart()
            }
        }
        function onJobConflict(id, json) {
            const w = app.activeWindow || app.windows[0]
            if (w) w.askConflict(id, JSON.parse(json))
            else app.grove.answerConflict(id, "skip", false)
        }
        function onNotify(level, message) { app.notifyAll(level, message) }
        function onFoldersTouched(json) {
            const dirs = JSON.parse(json)
            for (const w of app.windows) w.foldersTouched(dirs)
        }
        function onChooserRequest(json) {
            const req = JSON.parse(json)
            const c = chooserComp.createObject(null, { app: app, request: req })
            c.show(); c.raise(); c.requestActivate()
        }
    }

    // Finished jobs leave the list a few seconds later.
    Timer {
        id: cleanup
        interval: 4000
        onTriggered: {
            for (let i = app.jobs.count - 1; i >= 0; i--) {
                const s = app.jobs.get(i).state
                if (s === "done" || s === "cancelled") app.jobs.remove(i)
            }
        }
    }

    function notifyAll(level, message) {
        const w = activeWindow || windows[0]
        if (w) w.toast(level, message)
        else console.warn("grove:", message)
    }

    function size(n) {
        if (n < 1000) return n + " bytes"
        const u = ["KB", "MB", "GB", "TB"]
        let v = n, i = -1
        while (v >= 1000 && i < u.length - 1) { v /= 1000; i++ }
        return (v < 10 ? v.toFixed(1) : Math.round(v)) + " " + u[i]
    }

    function refreshTrash() { Async.call(grove, k => grove.trashCount(k), r => app.trashCount = r.count) }

    Component.onCompleted: {
        Theme.mode = settings.theme || "auto"
        try { Theme.hearth = JSON.parse(grove.env("GROVE_HEARTH") || "{}") } catch (e) {}
        grove.setIconTheme(Theme.dark)
        grove.start()
        refreshTrash()
        let req = null
        try { req = JSON.parse(grove.env("GROVE_OPEN") || "null") } catch (e) {}
        if (portalMode) return
        if (req && req.paths && req.paths.length) handleOpen(req)
        else if (!service) openWindow([grove.home()])
    }
}

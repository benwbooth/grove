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
    // A drive that writes discs (sysfs, off the UI thread): Burn to Disc
    // shows only when there is one. Checked again when drives come and go.
    property bool hasBurner: false
    function checkBurner() { Async.call(grove, k => grove.opticalDrives(k), r => app.hasBurner = (r || []).some(d => d.canWrite)) }
    Connections { target: app.grove; function onPlacesRevisionChanged() { app.checkBurner() } }
    // Goes up on each Google sign-in: previews of Google documents that
    // couldn't be fetched try again.
    property int googleSignIns: 0
    // hearth's accounts service (Settings > Accounts) is there: saved
    // servers and Google sign-ins come from it, and setting them up opens
    // hearth's Settings instead of grove's own sheets.
    property bool accountsService: false
    property var closedTabs: []
    // The tab being dragged ({win, index}) while a tab drag is under way.
    property var dragTab: null

    Component { id: winComp; BrowserWindow {} }
    Component { id: chooserComp; FileChooser {} }
    function openChooser(req) {
        const c = chooserComp.createObject(null, { app: app, request: req })
        c.show(); c.raise(); c.requestActivate()
        return c
    }

    // Windows and tabs, saved as they change, restored at the next start.
    // Everything, so a restart comes back as it was: each window's tabs
    // (folders, history, views, selection, scroll, open folders, the files
    // being viewed and where in them), size, Quick Look, Get Info and Disk
    // Usage windows. (Grove Player runs as its own process; mpv remembers
    // where each movie was left.)
    // Tests: GROVE_SESSION_TEST=1 saves and restores in screenshot runs too.
    readonly property bool sessionOn: screenshot === "" || grove.env("GROVE_SESSION_TEST") === "1"
    function saveSession() {
        if (!sessionOn || portalMode) return
        const ws = []
        for (const w of windows) { const st = w.windowState(); st.tabs = w.tabPaths(); ws.push(st) }
        if (ws.length) grove.saveSession(JSON.stringify({ windows: ws }))
    }
    Timer { id: sessionTimer; interval: 2000; onTriggered: app.saveSession() }
    // Saved as things change too (scroll, playback): a crash or logout keeps it.
    Timer { interval: 15000; repeat: true; running: app.windows.length > 0 && app.sessionOn; onTriggered: app.saveSession() }
    Connections { target: Qt.application; function onAboutToQuit() { app.saveSession() } }
    property var players: []

    function openWindowWithStates(states) { return openWindow([], [], states) }
    function openWindow(paths, selectPaths, states) {
        const w = winComp.createObject(null, { app: app, initialPaths: paths || [], initialSelect: selectPaths || [], initialStates: states || [] })
        windows.push(w)
        w.closing.connect(function () {
            // The last window: save first, so the session keeps it.
            if (windows.length === 1) saveSession()
            windows = windows.filter(x => x !== w)
            if (activeWindow === w) activeWindow = windows.length ? windows[windows.length - 1] : null
        })
        w.show()
        w.raise()
        w.requestActivate()
        activeWindow = w
        return w
    }

    // Grove Disk Utility: one window, opened from Go › Disk Utility…, the
    // sidebar's Format… sheet, or `grove --disk-utility`.
    Component { id: diskUtilComp; DiskUtilityWindow {} }
    property var diskUtil: null
    function openDiskUtility(diskId) {
        if (!diskUtil) {
            diskUtil = diskUtilComp.createObject(null, { app: app })
            diskUtil.closing.connect(() => Qt.callLater(() => { if (diskUtil && !diskUtil.visible) { diskUtil.destroy(); diskUtil = null } }))
        }
        if (diskId) diskUtil.diskId = diskId
        diskUtil.show(); diskUtil.raise(); diskUtil.requestActivate()
        if (screenshot !== "" && windows.length === 0) duShot.start()
        return diskUtil
    }
    // Tests: grove --disk-utility --screenshot out.png (GROVE_DU_SHOT_MS).
    Timer { id: duShot; interval: Number(grove.env("GROVE_DU_SHOT_MS") || 2500); onTriggered: { grove.grabWindows(app.screenshot); Qt.quit() } }

    // Grove Archiver: archive windows, extract/compress/test flows.
    property Archiver archiver: Archiver { app: app }

    Component { id: playerComp; PlayerWindow {} }
    // Grove Player windows (one per file; none: an empty one with the Open
    // panel, as QuickTime starts). Its own process (grove --play).
    function play(paths) {
        const list = paths.length ? paths : [""]
        let last = null
        for (const p of list) last = newPlayer(p)
        if (screenshot !== "") playShot.start()
        return last
    }
    function newPlayer(p, mode) {
        const w = playerComp.createObject(null, { app: app, path: p || "", mode: mode || "" })
        players = players.concat([w])
        w.closing.connect(() => Qt.callLater(() => { players = players.filter(x => x !== w) }))
        w.show(); w.raise(); w.requestActivate()
        return w
    }
    // Tests: grove --play FILE --screenshot out.png grabs the player and quits.
    Timer { id: playShot; interval: Number(grove.env("GROVE_PLAY_SHOT_MS") || 3500); onTriggered: { grove.grabWindows(app.screenshot); Qt.quit() } }
    // Grove Preview windows (macOS Preview): all the files of one request
    // in one window.
    Component { id: previewComp; PreviewWindow {} }
    property var previews: []
    function preview(paths) {
        const w = previewComp.createObject(null, { app: app, initialPaths: paths })
        previews = previews.concat([w])
        w.closing.connect(() => Qt.callLater(() => { previews = previews.filter(x => x !== w) }))
        w.show(); w.raise(); w.requestActivate()
        return w
    }
    function handleOpen(req) {
        const paths = req.paths || []
        if (req.action === "play") { play(paths); return }
        if (req.action === "preview") { preview(paths); return }
        if (req.action === "diskutility") { openDiskUtility(""); return }
        if (archiver.handle(req)) return
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
        function onSettingsJsonChanged() {
            app.settings = JSON.parse(app.grove.settingsJson)
            Theme.mode = app.settings.theme || "auto"
            Help.custom = app.settings.shortcuts && typeof app.settings.shortcuts === "object" ? app.settings.shortcuts : {}
        }
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
        // Accounts changed (a sign-in, a new server…): Google previews that
        // couldn't be fetched try again.
        function onAccountsChanged(available) { app.accountsService = available; app.googleSignIns++ }
        function onFoldersTouched(json) {
            const dirs = JSON.parse(json)
            for (const w of app.windows) w.foldersTouched(dirs)
        }
        function onChooserRequest(json) { app.openChooser(JSON.parse(json)) }
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

    // Only when the theme really changes: a new iconRev reloads every icon.
    property string appliedIconTheme: ""
    function applyIconTheme() {
        const mac = (settings.icon_style || "macos") === "macos"
        const key = Theme.dark + "/" + mac
        if (key === appliedIconTheme) return
        appliedIconTheme = key
        grove.setIconTheme(Theme.dark, mac)
        Theme.iconRev++
    }
    Connections {
        target: Theme
        function onDarkChanged() { app.applyIconTheme() }
    }
    onSettingsChanged: if (appliedIconTheme !== "") applyIconTheme()

    Component.onCompleted: {
        checkBurner()
        Theme.mode = settings.theme || "auto"
        Help.custom = settings.shortcuts && typeof settings.shortcuts === "object" ? settings.shortcuts : {}
        try { Theme.hearth = JSON.parse(grove.env("GROVE_HEARTH") || "{}") || {} } catch (e) {}
        Theme.wheelDebug = grove.env("GROVE_WHEEL_DEBUG") === "1"
        if (Theme.wheelDebug) console.warn("grove: Screen.devicePixelRatio", Screen.devicePixelRatio)
        applyIconTheme()
        grove.start()
        refreshTrash()
        let req = null
        try { req = JSON.parse(grove.env("GROVE_OPEN") || "null") } catch (e) {}
        if (portalMode) return
        if (req && req.paths && (req.paths.length || req.action === "archive" || req.action === "preview" || req.action === "play" || req.action === "diskutility")) handleOpen(req)
        else if (!service) {
            let sess = null
            try { sess = JSON.parse(grove.session() || "null") } catch (e) {}
            if (settings.restore_session !== false && sess && sess.windows && sess.windows.length && sessionOn)
                for (const w of sess.windows) {
                    // Every place but a search: folders, inside archives, the
                    // Trash, Recents, Computer, servers.
                    const ok = p => !!p && p.indexOf("search:") !== 0
                    const st = (w.states || []).filter(t => t && ok(t.path))
                    const win = st.length ? openWindowWithStates(st) : openWindow((w.tabs || []).filter(ok), [])
                    if (win && win.restoreWindow) win.restoreWindow(w)
                }
            if (!(sess && sess.windows && sess.windows.length && settings.restore_session !== false && sessionOn)) openWindow([grove.home()])
        }
    }
}

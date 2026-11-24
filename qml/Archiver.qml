import QtQuick

// Grove Archiver's flows (7-Zip's features, grove's look): archive windows,
// Extract Here / Extract To / Add to Archive / Test, each with its
// progress window. One per app (Main.qml's `archiver`).
QtObject {
    id: ar
    property var app
    property var windows: []
    readonly property var grove: app.grove
    readonly property var s: app.settings

    property Component winComp: Component { ArchiveWindow {} }
    property Component progComp: Component { ArcProgressWindow {} }
    property Component extractComp: Component { ExtractDialog {} }
    property Component compressComp: Component { CompressDialog {} }

    function show(w) { w.show(); w.raise(); w.requestActivate(); shotLater() }

    // Browse an archive (or an empty window with Open Archive…).
    function openWindow(path) {
        for (const w of windows) if (path && w.path === path) { w.raise(); w.requestActivate(); return w }
        const w = winComp.createObject(null, { app: app, archiver: ar, path: path || "" })
        windows = windows.concat([w])
        w.closing.connect(() => windows = windows.filter(x => x !== w))
        show(w)
        return w
    }

    // A job in its own progress window. done(result) after success.
    function run(kind, args, done) {
        const w = progComp.createObject(null, { app: app, archiver: ar, kind: kind, args: args })
        if (done) w.finished.connect(done)
        show(w)
        return w
    }

    // Extract Here: beside the archive (or "ask" in Settings: Extract To).
    function extractHere(path) {
        if ((s.extract_location || "beside") === "ask") return extractDialog(path, {})
        extract({ archive: path, dest: "", folder: s.extract_folder || "auto", overwrite: "ask" }, true)
    }
    // Extract into "<name>/" beside it (7-Zip's Extract to "name\").
    function extractToFolder(path) {
        extract({ archive: path, dest: "", folder: "always", overwrite: "ask" }, true)
    }
    // The job, then: show the result in grove, the archive to the Trash.
    function extract(args, afterwards, opts) {
        const o = opts || {}
        return run("extract", args, r => {
            const reveal = o.reveal !== undefined ? o.reveal : !!s.archive_reveal
            const trash = o.trash !== undefined ? o.trash : !!s.archive_trash_after
            if (!afterwards && !o.force) return
            if (reveal && r.shown) {
                const shown = r.shown, dest = r.dest
                if (shown === dest) app.openOrTab([dest], (r.created || []))
                else app.openOrTab([dest], [shown])
            }
            if (trash && r.whole && !(r.report && r.report.errors && r.report.errors.length))
                Async.call(grove, k => grove.trashPaths(k, JSON.stringify([args.archive])), x => app.refreshTrash())
        })
    }
    function extractDialog(path, extra) {
        const w = extractComp.createObject(null, { app: app, archiver: ar, archive: path, extra: extra || {} })
        show(w)
        return w
    }
    // Add to Archive (7-Zip's dialog). into: an existing archive to add to.
    function compressDialog(paths) {
        const w = compressComp.createObject(null, { app: app, archiver: ar, items: paths })
        show(w)
        return w
    }
    // Add to "name.7z" / "name.zip" at once, with the default settings.
    function quickCompress(paths, fmt) {
        if (!paths.length) return
        const first = paths[0]
        const dir = first.substring(0, first.lastIndexOf("/")) || "/"
        const base = paths.length === 1 ? first.substring(first.lastIndexOf("/") + 1) : "Archive"
        uniqueOut(dir + "/" + base + "." + fmt, out => run("create", { items: paths, out: out, format: fmt, level: s.archive_level !== undefined ? s.archive_level : 5 }, r => {
            if (s.archive_reveal && r.files && r.files.length) app.openOrTab([dir], r.files)
        }))
    }
    function test(path, password) { run("test", { archive: path, password: password || "" }) }

    // A free name ("Archive.7z", "Archive 2.7z"...): asked off the UI thread.
    function uniqueOut(path, fn) {
        Async.call(grove, k => grove.arcUnique(k, path), r => fn(r.path || path))
    }

    function size(n) { return app.size(n) }

    // grove's own open/save panel. opts: {directory, multiple, currentFolder,
    // acceptLabel, filters}; fn(paths).
    property Component chooserComp: Component { FileChooser {} }
    function choose(title, opts, fn) {
        // Archives are files to pick here, not folders to go into.
        const req = Object.assign({ title: title, mode: "open", archivesAsFiles: true }, opts || {})
        const c = chooserComp.createObject(null, { app: app, request: req, onChosen: fn })
        c.show(); c.raise(); c.requestActivate()
        return c
    }

    // Tests: grove --archive/--extract-here... --screenshot out.png grabs
    // the windows after GROVE_ARC_SHOT_MS and quits.
    property Timer shot: Timer {
        interval: Number(ar.grove.env("GROVE_ARC_SHOT_MS") || 2500)
        onTriggered: { ar.grove.grabWindows(ar.app.screenshot); Qt.quit() }
    }
    function shotLater() { if (app.screenshot !== "" && !shot.running) shot.start() }

    function handle(req) {
        const ps = req.paths || []
        switch (req.action) {
        case "archive": if (!ps.length) openWindow(""); for (const p of ps) openWindow(p); return true
        case "extracthere": for (const p of ps) extractHere(p); return true
        case "extractto": for (const p of ps) extractDialog(p, {}); return true
        case "compress": compressDialog(ps); return true
        case "testarchive": for (const p of ps) test(p, ""); return true
        }
        return false
    }
}

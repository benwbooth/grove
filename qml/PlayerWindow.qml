import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import GlobalMenu
import GroveMpv
import Grove

// Grove Player: macOS QuickTime Player for grove, on libmpv (src/mpv.rs),
// so it plays anything mpv plays: every container and codec, music with
// its cover art, subtitles (embedded and beside the file) and several
// audio tracks, web addresses (yt-dlp), DVD and Blu-ray folders and
// images, with hardware decoding.
//
// The window is all picture: no frame, a floating glass control bar
// (volume, back/forward, play, the scrubber with times, sound output,
// subtitles and audio, picture in picture, full screen) and the title in
// a top strip, both fading in on pointer movement. Edit › Trim (yellow
// handles on a filmstrip), Split Clip, Rotate, Flip; Export As 4K/1080p/
// 720p/480p/Audio Only through mpv's encoder, in the background. File ›
// New Audio Recording; New Screen Recording opens hearth's recorder.
// Window › Movie Inspector. Menus go to the global menu bar and the
// right-click menu.
Window {
    id: pw
    property var app
    // A file, or a web address / dvd:// / bd://.
    property string path: ""
    // "": play; "record": a new audio recording.
    property string mode: ""
    readonly property bool isUrl: path.indexOf("://") > 0 && path.indexOf("file://") !== 0
    readonly property string fileName: {
        if (mp.title && (isUrl || disc !== "")) return mp.title
        const p = path.replace(/\/+$/, "")
        return p ? decodeURIComponent(p.substring(p.lastIndexOf("/") + 1)) : (mode === "record" ? "Audio Recording" : "Grove Player")
    }
    readonly property bool loaded: mp.duration > 0 || mp.hasVideo || mp.albumArt
    // Sound only (maybe with cover art), once the tracks are known.
    readonly property bool music: loaded && trackList.length > 0 && !trackList.some(t => t.type === "video" && !t.albumart && !t.image)
    readonly property var info: { try { return JSON.parse(mp.info || "{}") } catch (e) { return {} } }
    readonly property var meta: info.metadata || {}
    function metaValue(k) {
        for (const key in meta) if (key.toLowerCase() === k) return meta[key]
        return ""
    }
    readonly property var trackList: { try { return JSON.parse(mp.tracks || "[]") || [] } catch (e) { return [] } }
    readonly property var subTracks: trackList.filter(t => t.type === "sub")
    readonly property var audioTracks: trackList.filter(t => t.type === "audio")
    readonly property var outputs: { try { return JSON.parse(mp.audioDevices || "[]") || [] } catch (e) { return [] } }
    readonly property bool fullScreen: visibility === Window.FullScreen

    color: "black"
    title: fileName
    flags: Qt.Window | Qt.FramelessWindowHint | (onTop ? Qt.WindowStaysOnTopHint : 0)
    minimumWidth: trimming || clipEditing ? 520 : music ? 300 : 320
    minimumHeight: music ? 300 : 180
    width: 960
    height: 560

    MpvPlayer {
        id: mp
        onMpvEvent: (name, data) => pw.onMpv(name, data)
        onLogMessage: (level, text) => { if (level === "error" || level === "fatal") pw.lastLog = text }
        onPositionChanged: pw.watchPosition()
        // The movie's size is known: the window takes its shape.
        onVideoWidthChanged: Qt.callLater(pw.fitToVideo)
        onVideoHeightChanged: Qt.callLater(pw.fitToVideo)
        onAlbumArtChanged: Qt.callLater(pw.fitToVideo)
        onTracksChanged: Qt.callLater(pw.fitToVideo)
    }
    property string lastLog: ""
    property string errorText: ""

    // ---------- opening ----------
    // dvd/bluray: the disc being played ("" otherwise) and its device.
    property string disc: ""
    property string discDevice: ""
    property var tryNext: []
    property string waiting: ""
    function open(p) {
        if (!p) return
        if (!video.ready) { waiting = p; return }
        if (path && path !== p && loaded) mp.command(JSON.stringify(["write-watch-later-config"]))
        path = p
        errorText = ""
        sized = false
        resetEdits()
        if (isUrl) { load(p); return }
        app.grove.playerAddRecent(p)
        Async.call(app.grove, k => app.grove.playerProbe(k, p), r => {
            if (r.kind === "dvd" || r.kind === "bluray") playDisc(r.kind, r.device)
            else load(p)
        })
    }
    function playDisc(kind, device) {
        disc = kind
        discDevice = device
        mp.setProp(kind === "dvd" ? "dvd-device" : "bluray-device", device)
        // A disc image that turns out to be the other kind: try that next.
        tryNext = [kind === "dvd" ? "bluray" : "dvd"]
        load(kind === "dvd" ? "dvd://" : "bd://")
    }
    function load(url, start) {
        const opts = start !== undefined && start > 0 ? "start=" + start.toFixed(3) : ""
        mp.command(JSON.stringify(opts ? ["loadfile", url, "replace", "-1", opts] : ["loadfile", url, "replace"]))
        mp.setProp("pause", "no")
    }
    function onMpv(name, data) {
        if (name === "file-loaded") { errorText = ""; fitToVideo(); if (startPaused) { mp.setProp("pause", "yes"); startPaused = false } }
        else if (name === "video-reconfig") fitToVideo()
        else if (name === "end-file") {
            let d = {}
            try { d = JSON.parse(data) } catch (e) {}
            if (d.reason !== "error") return
            if (disc !== "" && tryNext.length) { const k = tryNext.shift(); disc = k; mp.setProp(k === "dvd" ? "dvd-device" : "bluray-device", discDevice); load(k === "dvd" ? "dvd://" : "bd://"); return }
            errorText = "“" + fileName + "” couldn't be opened." + (lastLog ? "\n" + lastLog : d.error ? "\n" + d.error : "")
        }
    }
    property bool startPaused: false

    // The window takes the movie's shape once it's known (at most 80% of
    // the screen); music gets a square for the cover.
    property bool sized: false
    readonly property int videoW: (rotation % 180 ? mp.videoHeight : mp.videoWidth)
    readonly property int videoH: (rotation % 180 ? mp.videoWidth : mp.videoHeight)
    readonly property bool debug: app ? app.grove.env("GROVE_PLAYER_DEBUG") !== "" : false
    function dbg(...a) { if (debug) console.warn("grove-player:", ...a) }
    function fitToVideo(force) {
        dbg("fit", sized, force, music, videoW, videoH, width, height, Screen.devicePixelRatio, Screen.desktopAvailableWidth)
        if ((sized && !force) || fullScreen || pip) return
        if (music) { setGeometry(x, y, 380, 380 + 90); sized = true; return }
        if (!videoW || !videoH) return
        const dpr = Screen.devicePixelRatio || 1
        const maxW = Screen.desktopAvailableWidth * 0.8, maxH = Screen.desktopAvailableHeight * 0.8
        const w = videoW / dpr, h = videoH / dpr
        const s = Math.min(1, maxW / w, maxH / h)
        setGeometry(x, y, Math.max(minimumWidth, Math.round(w * s)), Math.max(minimumHeight, Math.round(h * s)))
        sized = true
    }
    // ---------- resizing ----------
    // Every edge goes through the compositor's own resize. arbor keeps the
    // window's proportions while dragging (window rule keep-aspect-ratio,
    // set for grove-player in hearth's arbor config), so the picture never
    // letterboxes outside maximized and full screen. (Answering each size
    // with one of our own made the window wobble and flicker: the
    // compositor and the player took turns.)
    readonly property real aspect: mp.hasVideo && videoW > 0 && videoH > 0 ? videoW / videoH : 0
    function beginSystemResize(edges) {
        sized = true
        startSystemResize(edges)
    }

    function actualSize() {
        if (!videoW) return
        if (fullScreen) showNormal()
        const dpr = Screen.devicePixelRatio || 1
        setGeometry(x, y, Math.round(videoW / dpr), Math.round(videoH / dpr))
        fill = false
    }

    // ---------- playback ----------
    readonly property bool playing: !mp.paused && loaded
    function toggle() {
        if (!loaded) return
        if (mp.eof || (trimming && mp.position >= trimB - 0.05)) seekTo(trimming ? trimA : 0)
        mp.setProp("pause", mp.paused ? "no" : "yes")
        poke()
    }
    function seekTo(t) { mp.command(JSON.stringify(["seek", Math.max(0, t).toFixed(3), "absolute+exact"])); poke() }
    function seekBy(s) { mp.command(JSON.stringify(["seek", String(s), "relative+exact"])); poke() }
    function frameStep(forward) { mp.command(JSON.stringify([forward ? "frame-step" : "frame-back-step"])); poke() }
    function setVolume(v) { mp.setProp("volume", String(Math.max(0, Math.min(100, Math.round(v))))); if (mp.muted) mp.setProp("mute", "no"); poke() }
    function fmt(s) {
        s = Math.max(0, Math.floor(s || 0))
        const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60
        return (h ? h + ":" + ("0" + m).slice(-2) : m) + ":" + ("0" + (s % 60)).slice(-2)
    }
    function watchPosition() {
        // Trim's Play plays the yellow part only.
        if (trimming && !mp.paused && mp.position >= trimB) mp.setProp("pause", "yes")
    }

    // Controls show on movement and fade while it plays.
    property bool controls: true
    function poke() { controls = true; hideTimer.restart() }
    Timer { id: hideTimer; interval: 2600; onTriggered: if (pw.playing && !barHover.hovered && !topHover.hovered && !pw.trimming && !pw.clipEditing && !pw.menuOpen) pw.controls = false }
    readonly property bool showControls: controls || !playing || music || trimming || clipEditing || !loaded

    // ---------- view ----------
    // Subtitles stay above the control bar while it shows (QuickTime
    // lifts them too).
    readonly property real subPos: {
        if (!showControls || !loaded || music || height <= 0) return 100
        const lift = (trimming || clipEditing ? 96 + 18 : (pip ? 42 + 8 : 72 + 22)) + 8
        return Math.max(50, Math.round(100 * (1 - lift / height)))
    }
    // (Applied a moment later: a resize changes it every pixel.)
    onSubPosChanged: subPosTimer.restart()
    Timer { id: subPosTimer; interval: 120; onTriggered: mp.setProp("sub-pos", String(pw.subPos)) }
    property bool fill: false
    // Fill screen, or a window within 2% of the movie's shape (rounding
    // would leave a hairline of black): no bars.
    readonly property bool panFill: fill || (!fullScreen && aspect > 0 && height > 0 && Math.abs(width / height / aspect - 1) < 0.02)
    onPanFillChanged: mp.setProp("panscan", panFill ? "1.0" : "0.0")
    property bool onTop: false
    property bool subsShown: true
    onSubsShownChanged: mp.setProp("sub-visibility", subsShown ? "yes" : "no")
    function toggleFullScreen() { if (pip) leavePip(); fullScreen ? showNormal() : showFullScreen(); poke() }

    // Picture in picture: a small window that floats above the rest.
    property bool pip: false
    property rect pipSaved
    property bool pipWasOnTop: false
    function enterPip() {
        if (pip || music) return
        if (fullScreen) showNormal()
        pipSaved = Qt.rect(x, y, width, height)
        pipWasOnTop = onTop
        pip = true
        onTop = true
        const w = 400, h = videoW ? Math.round(w * videoH / videoW) : 225
        width = w; height = h
        x = Screen.virtualX + Screen.desktopAvailableWidth - w - 24
        y = Screen.virtualY + Screen.desktopAvailableHeight - h - 24
    }
    function leavePip() {
        if (!pip) return
        pip = false
        onTop = pipWasOnTop
        x = pipSaved.x; y = pipSaved.y; width = pipSaved.width; height = pipSaved.height
    }

    // ---------- edits ----------
    // The movie as clips of the source ([{start, end}] in its seconds),
    // turned and mirrored. Played through mpv's EDL (edl://) so trims and
    // splits show at once; exported the same way.
    property var clips: []
    property int rotation: 0
    property bool flipH: false
    property bool flipV: false
    property var undoStack: []
    property var redoStack: []
    property real sourceDuration: 0
    readonly property bool canEdit: loaded && !isUrl && disc === "" && path !== "" && mode === ""
    readonly property real total: clips.reduce((a, c) => a + (c.end - c.start), 0)
    readonly property bool cut: clips.length > 1 || (clips.length === 1 && (clips[0].start > 0.01 || clips[0].end < sourceDuration - 0.01))
    readonly property bool edited: cut || rotation !== 0 || flipH || flipV
    onRotationChanged: mp.setProp("video-rotate", String(rotation))
    Connections {
        target: mp
        function onDurationChanged() {
            // The source's length, once (an EDL's length is the cut's).
            if (!pw.cut && mp.duration > 0 && pw.clips.length <= 1) { pw.sourceDuration = mp.duration; pw.clips = [{ start: 0, end: mp.duration }] }
        }
    }
    function resetEdits() { clips = []; rotation = 0; flipH = false; flipV = false; undoStack = []; redoStack = []; sourceDuration = 0; trimming = false; clipEditing = false; filmstrip = "" }
    function editState() { return JSON.stringify({ clips: clips, rotation: rotation, flipH: flipH, flipV: flipV }) }
    function pushUndo(label) { undoStack = undoStack.concat([{ label: label, state: editState() }]); redoStack = [] }
    function restore(s, keep) {
        const st = JSON.parse(s)
        const wasCut = cut
        rotation = st.rotation; flipH = st.flipH; flipV = st.flipV
        if (JSON.stringify(st.clips) !== JSON.stringify(clips)) { clips = st.clips; reloadEdits(0) }
    }
    function undo() { if (!undoStack.length) return; const u = undoStack[undoStack.length - 1]; redoStack = redoStack.concat([{ label: u.label, state: editState() }]); undoStack = undoStack.slice(0, -1); restore(u.state) }
    function redo() { if (!redoStack.length) return; const r = redoStack[redoStack.length - 1]; undoStack = undoStack.concat([{ label: r.label, state: editState() }]); redoStack = redoStack.slice(0, -1); restore(r.state) }
    function utf8Len(s) { return unescape(encodeURIComponent(s)).length }
    // mpv's EDL: %length%path,start,length per clip.
    function edlFor(list) {
        return "edl://" + list.map(c => "%" + utf8Len(path) + "%" + path + ",start=" + c.start.toFixed(6) + ",length=" + (c.end - c.start).toFixed(6)).join(";")
    }
    function sourceUrl() { return cut ? edlFor(clips) : path }
    function reloadEdits(at) { load(sourceUrl(), at); if (at === undefined) mp.setProp("pause", "yes") }
    // Timeline (the cut movie's seconds) to [clip index, offset in it].
    function locate(t) {
        let o = 0
        for (let i = 0; i < clips.length; i++) {
            const len = clips[i].end - clips[i].start
            if (t < o + len || i === clips.length - 1) return [i, Math.max(0, Math.min(len, t - o)), o]
            o += len
        }
        return [0, 0, 0]
    }
    // The part of the cut movie between a and b (timeline seconds).
    function slice(a, b) {
        const out = []
        let o = 0
        for (const c of clips) {
            const len = c.end - c.start
            const s = Math.max(a, o), e = Math.min(b, o + len)
            if (e - s > 0.01) out.push({ start: c.start + (s - o), end: c.start + (e - o) })
            o += len
        }
        return out
    }
    function rotate(deg) { if (!canEdit) return; pushUndo(deg < 0 ? "Rotate Left" : "Rotate Right"); rotation = (rotation + deg + 360) % 360; Qt.callLater(() => fitToVideo(true)) }
    function flip(h) { if (!canEdit) return; pushUndo(h ? "Flip Horizontal" : "Flip Vertical"); if (h) flipH = !flipH; else flipV = !flipV }
    function splitClip() {
        if (!canEdit) return
        const t = mp.position
        const [i, off] = locate(t)
        const c = clips[i]
        dbg("split", t, i, off, JSON.stringify(clips))
        if (off < 0.1 || c.end - c.start - off < 0.1) return
        pushUndo("Split Clip")
        const l = clips.slice()
        l.splice(i, 1, { start: c.start, end: c.start + off }, { start: c.start + off, end: c.end })
        clips = l
        reloadEdits(t)
        clipEditing = true
        selectedClip = i + 1
        needFilmstrip()
    }
    function deleteClip(i) {
        if (clips.length < 2 || i < 0 || i >= clips.length) return
        pushUndo("Delete Clip")
        const l = clips.slice(); l.splice(i, 1); clips = l
        selectedClip = Math.min(i, clips.length - 1)
        reloadEdits(locate(0)[2])
    }
    // Trim: the yellow frame's ends, in timeline seconds.
    property bool trimming: false
    property real trimA: 0
    property real trimB: 0
    function startTrim() {
        if (!canEdit) return
        clipEditing = false
        trimA = 0; trimB = total
        trimming = true
        mp.setProp("pause", "yes")
        needFilmstrip()
    }
    function applyTrim() {
        if (trimB - trimA < 0.1) return
        if (trimA > 0.01 || trimB < total - 0.01) {
            pushUndo("Trim")
            clips = slice(trimA, trimB)
            reloadEdits(0)
        }
        trimming = false
    }
    function cancelTrim() { trimming = false }
    property bool clipEditing: false
    property int selectedClip: -1

    // The filmstrip behind Trim and the clips: frames from the source.
    property string filmstrip: ""
    property int filmFrames: 24
    function needFilmstrip() {
        if (filmstrip !== "" || !canEdit) return
        Async.call(app.grove, k => app.grove.playerFilmstrip(k, path, filmFrames, 72), r => { if (r && r.path) pw.filmstrip = "file://" + r.path })
    }

    // ---------- export ----------
    // Each export runs in an MpvJob (mpv's encoder) while playing goes on.
    property var exports: []
    Component { id: jobComp; MpvJob {} }
    readonly property int shortSide: Math.min(mp.videoWidth, mp.videoHeight)
    function presetOk(p) {
        if (p === "audio") return audioTracks.length > 0
        if (!mp.hasVideo) return false
        return shortSide >= ({ "2160": 1500, "1080": 700, "720": 500, "480": 1 })[p]
    }
    function presetName(p) { return p === "audio" ? "Audio Only" : p === "2160" ? "4K" : p + "p" }
    function exportAs(p) {
        if (!canEdit || !presetOk(p)) return
        const dir = path.substring(0, path.lastIndexOf("/"))
        const base = fileName.replace(/\.[^.]*$/, "")
        const ext = p === "audio" ? "m4a" : "mp4"
        chooseFiles("Export As " + presetName(p), { mode: "save", currentFolder: dir, currentName: base + (p === "audio" ? "" : " " + presetName(p)) + "." + ext }, paths => {
            if (paths && paths.length) startExport(p, paths[0])
        })
    }
    function exportSpec(p, out) {
        const o = { "oac": "aac", "oacopts": "b=192k", "ofopts": "movflags=+faststart" }
        if (p === "audio") { o["vid"] = "no"; o["of"] = "ipod" }
        else {
            const portrait = videoH > videoW
            const box = { "2160": [3840, 2160], "1080": [1920, 1080], "720": [1280, 720], "480": [854, 480] }[p]
            const bw = portrait ? box[1] : box[0], bh = portrait ? box[0] : box[1]
            const vf = []
            if (flipH) vf.push("hflip")
            if (flipV) vf.push("vflip")
            vf.push("scale=w=" + bw + ":h=" + bh + ":force_original_aspect_ratio=decrease:force_divisible_by=2")
            // 8-bit 4:2:0: plays everywhere (10-bit and 4:4:4 sources too).
            vf.push("format=yuv420p")
            o["vf"] = vf.join(",")
            o["video-rotate"] = String(rotation)
            o["ovc"] = "libx264"
            o["ovcopts"] = "crf=" + (p === "2160" ? 20 : 21) + ",preset=medium"
            o["of"] = "mp4"
            o["sid"] = "no"
        }
        return { input: sourceUrl(), output: out, length: total, options: o }
    }
    function startExport(p, out) {
        const job = jobComp.createObject(pw)
        const entry = { job: job, name: out.substring(out.lastIndexOf("/") + 1), preset: presetName(p), out: out }
        exports = exports.concat([entry])
        job.finished.connect((ok, err, output) => {
            toastText = ok ? "Exported “" + entry.name + "”" : err ? "Export failed: " + err : "Export cancelled"
            toastTimer.restart()
            if (testExport) console.warn("grove-player-test export", ok ? "ok" : "failed", output, err)
            exports = exports.filter(e => e !== entry)
            job.destroy()
            if (testExport && ok) Qt.quit()
        })
        job.start(JSON.stringify(exportSpec(p, out)))
    }
    property string toastText: ""
    Timer { id: toastTimer; interval: 4000; onTriggered: pw.toastText = "" }

    // ---------- recording ----------
    property var recorder: null
    property string recordFile: ""
    function recordToggle() {
        if (recorder && recorder.running) { recorder.stop(true); return }
        const d = new Date()
        const two = n => ("0" + n).slice(-2)
        const name = "Audio Recording " + d.getFullYear() + "-" + two(d.getMonth() + 1) + "-" + two(d.getDate()) + " at " + two(d.getHours()) + "." + two(d.getMinutes()) + "." + two(d.getSeconds()) + ".m4a"
        recordFile = app.grove.home() + "/Music/" + name
        recorder = jobComp.createObject(pw)
        recorder.finished.connect((ok, err, output) => {
            const r = recorder
            recorder = null
            r.destroy()
            if (ok) { mode = ""; open(output) }
            else { toastText = err ? "Recording failed: " + err : "Recording stopped"; toastTimer.restart() }
        })
        recorder.start(JSON.stringify({ input: "av://pulse:default", output: recordFile, length: 0, options: { "oac": "aac", "oacopts": "b=192k", "of": "ipod", "vid": "no" } }))
    }

    // ---------- files ----------
    Component { id: chooserComp; FileChooser {} }
    function chooseFiles(t, opts, fn) {
        const req = Object.assign({ title: t, mode: "open" }, opts || {})
        const c = chooserComp.createObject(null, { app: app, request: req, onChosen: fn })
        c.show(); c.raise(); c.requestActivate()
        return c
    }
    function openDialog() {
        chooseFiles("Open in Grove Player", {
            mode: "open", multiple: true, currentFolder: path && !isUrl ? path.substring(0, path.lastIndexOf("/")) : app.grove.home() + "/Videos",
            filters: [{ name: "Movies and music", patterns: [{ mime: true, pattern: "video/*" }, { mime: true, pattern: "audio/*" }, { mime: false, pattern: "*.iso" }, { mime: false, pattern: "VIDEO_TS.IFO" }, { mime: false, pattern: "index.bdmv" }] },
                      { name: "All files", patterns: [{ mime: false, pattern: "*" }] }]
        }, paths => openPaths(paths))
    }
    // Like QuickTime: one window per movie; an empty window takes the first.
    function openPaths(paths) {
        for (let i = 0; i < paths.length; i++) {
            if (i === 0 && !path && mode === "") open(paths[0])
            else if (app.newPlayer) app.newPlayer(paths[i])
        }
    }
    property var recents: []
    function refreshRecents() { Async.call(app.grove, k => app.grove.playerRecents(k), r => { if (Array.isArray(r)) pw.recents = r }) }

    // ---------- menus ----------
    function k(act) { return Help.seqs(act)[0] || "" }
    readonly property var menuModel: [
        { title: "Grove Player", icon: "video-x-generic", items: [
            { id: "about", icon: "help-about", text: "About Grove Player" },
            { separator: true },
            { id: "close", icon: "application-exit", text: "Close Window" }
        ] },
        { title: "File", icon: "folder", items: [
            { id: "newmovie", icon: "camera-video", text: "New Screen Recording", shortcut: k("pl.screenrec") },
            { id: "newaudio", icon: "audio-input-microphone", text: "New Audio Recording", shortcut: k("pl.audiorec") },
            { separator: true },
            { id: "open", icon: "document-open", text: "Open File…", shortcut: k("pl.open") },
            { id: "location", icon: "internet-services", text: "Open Location…", shortcut: k("pl.location") },
            { id: "recent", icon: "document-open-recent", text: "Open Recent", items: recents.length
                ? recents.map((r, i) => ({ id: "recent." + i, icon: "video-x-generic", text: r.substring(r.lastIndexOf("/") + 1) })).concat([{ separator: true }, { id: "recentclear", icon: "edit-clear-history", text: "Refresh List" }])
                : [{ id: "recentnone", icon: "document-open-recent", text: "No Recent Movies", enabled: false }] },
            { separator: true },
            { id: "close2", icon: "window-close", text: "Close", shortcut: k("pl.close") },
            { separator: true },
            { id: "export", icon: "document-export", text: "Export As", enabled: canEdit, items: ["2160", "1080", "720", "480", "audio"].map(p => ({ id: "export." + p, icon: p === "audio" ? "audio-x-generic" : "video-x-generic", text: presetName(p) + "…", enabled: presetOk(p) })) },
            { id: "reveal", icon: "folder-open", text: "Show in Grove", enabled: path !== "" && !isUrl }
        ] },
        { title: "Edit", icon: "document-edit", items: [
            { id: "undo", icon: "edit-undo", text: undoStack.length ? "Undo " + undoStack[undoStack.length - 1].label : "Undo", shortcut: k("pl.undo"), enabled: undoStack.length > 0 },
            { id: "redo", icon: "edit-redo", text: redoStack.length ? "Redo " + redoStack[redoStack.length - 1].label : "Redo", shortcut: k("pl.redo"), enabled: redoStack.length > 0 },
            { separator: true },
            { id: "trim", icon: "transform-crop", text: "Trim…", shortcut: k("pl.trim"), enabled: canEdit && !trimming },
            { id: "split", icon: "edit-cut", text: "Split Clip", shortcut: k("pl.split"), enabled: canEdit && mp.hasVideo },
            { id: "deleteclip", icon: "edit-delete", text: "Delete Clip", shortcut: "Delete", enabled: clipEditing && clips.length > 1 && selectedClip >= 0 },
            { id: "clips", icon: "view-list-icons", text: clipEditing ? "Hide Clips" : "Show Clips", enabled: canEdit && clips.length > 1 },
            { separator: true },
            { id: "rotleft", icon: "object-rotate-left", text: "Rotate Left", shortcut: k("pl.rotleft"), enabled: canEdit && mp.hasVideo },
            { id: "rotright", icon: "object-rotate-right", text: "Rotate Right", shortcut: k("pl.rotright"), enabled: canEdit && mp.hasVideo },
            { id: "fliph", icon: "object-flip-horizontal", text: "Flip Horizontal", shortcut: k("pl.fliph"), enabled: canEdit && mp.hasVideo },
            { id: "flipv", icon: "object-flip-vertical", text: "Flip Vertical", shortcut: k("pl.flipv"), enabled: canEdit && mp.hasVideo }
        ] },
        { title: "View", icon: "view-visible", items: [
            { id: "actual", icon: "zoom-original", text: "Actual Size", shortcut: k("pl.actual"), enabled: mp.hasVideo },
            { id: "fit", icon: "zoom-fit-best", text: "Fit to Screen", checkable: true, checked: !fill, shortcut: k("pl.fit"), enabled: mp.hasVideo },
            { id: "fill", icon: "zoom-fit-page", text: "Fill Screen", checkable: true, checked: fill, shortcut: k("pl.fill"), enabled: mp.hasVideo },
            { separator: true },
            { id: "ontop", icon: "window-pin", text: "Float on Top", checkable: true, checked: onTop, shortcut: k("pl.ontop") },
            { id: "pip", icon: "view-restore", text: pip ? "Exit Picture in Picture" : "Enter Picture in Picture", shortcut: k("pl.pip"), enabled: mp.hasVideo },
            { id: "fullscreen", icon: "view-fullscreen", text: fullScreen ? "Exit Full Screen" : "Enter Full Screen", shortcut: k("pl.fullscreen") },
            { separator: true },
            { id: "subs", icon: "media-view-subtitles-symbolic", text: "Subtitles", items: [
                { id: "subs.show", icon: "view-visible", text: "Show Subtitles", checkable: true, checked: subsShown && mp.sid > 0, shortcut: k("pl.subs"), enabled: subTracks.length > 0 },
                { separator: true },
                { id: "sid.0", icon: "dialog-cancel", text: "Off", checkable: true, checked: mp.sid <= 0 }
            ].concat(subTracks.map(t => ({ id: "sid." + t.id, icon: "media-view-subtitles-symbolic", text: trackName(t), checkable: true, checked: mp.sid === t.id })))
             .concat([{ separator: true }, { id: "addsub", icon: "list-add", text: "Add Subtitle File…", enabled: loaded }]) },
            { id: "langs", icon: "audio-x-generic", text: "Languages", enabled: audioTracks.length > 1, items: audioTracks.map(t => ({ id: "aid." + t.id, icon: "audio-x-generic", text: trackName(t), checkable: true, checked: mp.aid === t.id })) },
            { id: "speed", icon: "media-seek-forward", text: "Playback Speed", items: [0.5, 0.75, 1, 1.25, 1.5, 2].map(s => ({ id: "speed." + s, icon: "media-playback-start", text: s === 1 ? "Normal" : s + "×", checkable: true, checked: Math.abs(mp.speed - s) < 0.01 })) },
            { id: "loop", icon: "media-repeat-all", text: "Loop", checkable: true, checked: loopOn, shortcut: k("pl.loop") },
            { id: "output", icon: "audio-speakers", text: "Sound Output", enabled: outputs.length > 0, items: outputs.map((o, i) => ({ id: "out." + i, icon: o.name === "auto" ? "audio-card" : "audio-speakers", text: o.name === "auto" ? "System Default" : o.description, checkable: true, checked: mp.audioDevice === o.name })) }
        ] },
        { title: "Window", icon: "window", items: [
            { id: "minimize", icon: "window-minimize", text: "Minimize", shortcut: k("pl.minimize") },
            { separator: true },
            { id: "inspector", icon: "documentinfo", text: inspector.visible ? "Hide Movie Inspector" : "Show Movie Inspector", shortcut: k("pl.inspector"), enabled: loaded },
            { id: "exports", icon: "document-export", text: "Show Export Progress", enabled: exports.length > 0 }
        ] },
        { title: "Help", icon: "help-contents", items: [
            { id: "help", icon: "help-contents", text: "Grove Player Help", shortcut: k("help") },
            { id: "shortcuts", icon: "preferences-desktop-keyboard", text: "Keyboard Shortcuts" }
        ] }
    ]
    property bool loopOn: false
    onLoopOnChanged: mp.setProp("loop-file", loopOn ? "inf" : "no")
    function trackName(t) {
        const parts = []
        if (t.title) parts.push(t.title)
        if (t.lang) parts.push(t.lang.toUpperCase())
        if (t.codec && !t.title) parts.push(t.codec)
        if (t.external) parts.push("file")
        return (parts.join(" · ") || ("Track " + t.id)) + (t["default"] ? "" : "")
    }
    function menuAction(id) {
        if (id.indexOf("recent.") === 0) { const r = recents[Number(id.substring(7))]; if (r) { if (path) app.newPlayer(r); else open(r) } return }
        if (id.indexOf("export.") === 0) { exportAs(id.substring(7)); return }
        if (id.indexOf("sid.") === 0) { const n = Number(id.substring(4)); mp.setProp("sid", n > 0 ? String(n) : "no"); if (n > 0) subsShown = true; return }
        if (id.indexOf("aid.") === 0) { mp.setProp("aid", id.substring(4)); return }
        if (id.indexOf("speed.") === 0) { mp.setProp("speed", id.substring(6)); return }
        if (id.indexOf("out.") === 0) { const o = outputs[Number(id.substring(4))]; if (o) mp.setProp("audio-device", o.name); return }
        switch (id) {
        case "about": aboutSheet.open(); break
        case "close": case "close2": pw.close(); break
        case "newmovie": app.grove.playerScreenRecording(); break
        case "newaudio": app.newPlayer("", "record"); break
        case "open": openDialog(); break
        case "location": locationSheet.open(); urlField.text = ""; urlField.forceActiveFocus(); break
        case "recentclear": refreshRecents(); break
        case "reveal": Qt.openUrlExternally("file://" + path.substring(0, path.lastIndexOf("/"))); break
        case "undo": undo(); break
        case "redo": redo(); break
        case "trim": startTrim(); break
        case "split": splitClip(); break
        case "deleteclip": deleteClip(selectedClip); break
        case "clips": clipEditing = !clipEditing; if (clipEditing) needFilmstrip(); break
        case "rotleft": rotate(-90); break
        case "rotright": rotate(90); break
        case "fliph": flip(true); break
        case "flipv": flip(false); break
        case "actual": actualSize(); break
        case "fit": fill = false; break
        case "fill": fill = true; break
        case "ontop": onTop = !onTop; break
        case "pip": pip ? leavePip() : enterPip(); break
        case "fullscreen": toggleFullScreen(); break
        case "subs.show": subsShown = !subsShown; break
        case "addsub": chooseFiles("Add Subtitle File", { mode: "open", currentFolder: path && !isUrl ? path.substring(0, path.lastIndexOf("/")) : app.grove.home(),
                                                         filters: [{ name: "Subtitles", patterns: ["*.srt", "*.ass", "*.ssa", "*.vtt", "*.sub", "*.sup", "*.idx"].map(x => ({ mime: false, pattern: x })) }] },
                                   paths => { if (paths && paths.length) { mp.command(JSON.stringify(["sub-add", paths[0], "select"])); subsShown = true } }); break
        case "loop": loopOn = !loopOn; break
        case "minimize": pw.showMinimized(); break
        case "inspector": inspector.visible ? inspector.close() : inspector.show(); break
        case "exports": controls = true; break
        case "help": helpWin.query = "Grove Player"; helpWin.show(); helpWin.raise(); helpWin.requestActivate(); break
        case "shortcuts": helpWin.query = "Grove Player"; helpWin.show(); helpWin.raise(); break
        }
    }
    GlobalMenuBar { id: globalMenu; window: pw; model: pw.menuModel; onTriggered: id => pw.menuAction(id) }
    onVisibleChanged: if (visible) Qt.callLater(globalMenu.attach)
    property bool menuOpen: appMenu.visible
    AppMenu {
        iconUrl: (n, px) => Theme.iconUrl(n) + "&exact=1"
        id: appMenu
        model: pw.menuModel
        onTriggered: id => pw.menuAction(id)
        onAboutToShow: pw.refreshRecents()
        property real closedAt: 0
        onClosed: closedAt = Date.now()
    }
    HelpWindow { id: helpWin; app: pw.app }

    // ---------- keys ----------
    readonly property bool modalOpen: locationSheet.visible || aboutSheet.visible
    component Key: Shortcut { property string act: ""; sequences: Help.seqs(act); context: Qt.WindowShortcut; enabled: !pw.modalOpen && sequences.length > 0 }
    Key { act: "pl.open"; onActivated: pw.openDialog() }
    Key { act: "pl.location"; onActivated: pw.menuAction("location") }
    Key { act: "pl.close"; onActivated: pw.close() }
    Key { act: "pl.audiorec"; onActivated: pw.menuAction("newaudio") }
    Key { act: "pl.screenrec"; onActivated: pw.menuAction("newmovie") }
    Key { act: "pl.undo"; onActivated: pw.undo() }
    Key { act: "pl.redo"; onActivated: pw.redo() }
    Key { act: "pl.trim"; onActivated: pw.startTrim() }
    Key { act: "pl.split"; onActivated: pw.splitClip() }
    Key { act: "pl.rotleft"; onActivated: pw.rotate(-90) }
    Key { act: "pl.rotright"; onActivated: pw.rotate(90) }
    Key { act: "pl.fliph"; onActivated: pw.flip(true) }
    Key { act: "pl.flipv"; onActivated: pw.flip(false) }
    Key { act: "pl.actual"; onActivated: pw.actualSize() }
    Key { act: "pl.fit"; onActivated: pw.fill = false }
    Key { act: "pl.fill"; onActivated: pw.fill = true }
    Key { act: "pl.ontop"; onActivated: pw.onTop = !pw.onTop }
    Key { act: "pl.pip"; onActivated: pw.menuAction("pip") }
    Key { act: "pl.fullscreen"; onActivated: pw.toggleFullScreen() }
    Key { act: "pl.subs"; onActivated: pw.subsShown = !pw.subsShown }
    Key { act: "pl.inspector"; onActivated: pw.menuAction("inspector") }
    Key { act: "pl.loop"; onActivated: pw.loopOn = !pw.loopOn }
    Key { act: "pl.minimize"; onActivated: pw.showMinimized() }
    Key { act: "help"; onActivated: pw.menuAction("help") }

    // ---------- the picture ----------
    Item {
        id: root
        anchors.fill: parent
        focus: true
        transform: Translate { x: 0.001; y: 0.001 }
        Keys.onPressed: (e) => {
            const shift = e.modifiers & Qt.ShiftModifier, alt = e.modifiers & Qt.AltModifier
            if (e.modifiers & Qt.ControlModifier) return
            if (pw.trimming && (e.key === Qt.Key_Return || e.key === Qt.Key_Enter)) pw.applyTrim()
            else if (e.key === Qt.Key_Space || e.key === Qt.Key_K) pw.toggle()
            else if (e.key === Qt.Key_Right || e.key === Qt.Key_Left) {
                const fwd = e.key === Qt.Key_Right
                if (alt) pw.seekTo(fwd ? pw.mp_end() : 0)
                else if (mp.paused && !shift) pw.frameStep(fwd)
                else pw.seekBy((fwd ? 1 : -1) * (shift ? 15 : 5))
            }
            else if (e.key === Qt.Key_L) pw.seekBy(10)
            else if (e.key === Qt.Key_J) pw.seekBy(-10)
            else if (e.key === Qt.Key_Up) pw.setVolume(mp.volume + 5)
            else if (e.key === Qt.Key_Down) pw.setVolume(mp.volume - 5)
            else if (e.key === Qt.Key_M) { mp.setProp("mute", mp.muted ? "no" : "yes"); pw.poke() }
            else if (e.key === Qt.Key_F) pw.toggleFullScreen()
            else if (e.key === Qt.Key_Home) pw.seekTo(0)
            else if (e.key === Qt.Key_End) pw.seekTo(pw.mp_end())
            else if ((e.key === Qt.Key_Delete || e.key === Qt.Key_Backspace) && pw.clipEditing) pw.deleteClip(pw.selectedClip)
            else if (e.key === Qt.Key_Escape) {
                if (pw.trimming) pw.cancelTrim()
                else if (pw.clipEditing) pw.clipEditing = false
                else if (pw.pip) pw.leavePip()
                else if (pw.fullScreen) pw.showNormal()
            }
            else return
            e.accepted = true
        }

        MpvGlView {
            id: video
            handle: mp.handle
            onReadyChanged: if (ready && pw.waiting) { const p = pw.waiting; pw.waiting = ""; pw.open(p) }
            // Music: the cover art in a square above the controls.
            x: pw.music ? (parent.width - width) / 2 : 0
            y: pw.music ? 0 : 0
            width: pw.music ? Math.min(parent.width, parent.height - 90) : parent.width
            height: pw.music ? width : parent.height
            // Always shown: mpv turns the picture on only once this view has
            // made its render context (black while there's nothing).
            // Flip shown as a mirror of the picture (no filter on the decoder).
            transform: Scale { origin.x: video.width / 2; origin.y: video.height / 2; xScale: pw.flipH ? -1 : 1; yScale: pw.flipV ? -1 : 1 }
        }

        // Music without cover art, and the empty window.
        Column {
            anchors.horizontalCenter: parent.horizontalCenter
            y: pw.music ? Math.max(20, (Math.min(parent.width, parent.height - 90) - height) / 2) : (parent.height - height) / 2
            spacing: 14
            visible: (pw.music && !mp.albumArt) || (!pw.loaded && pw.errorText === "" && pw.mode === "")
            GIcon {
                anchors.horizontalCenter: parent.horizontalCenter
                name: pw.music ? "audio-x-generic" : "grove-player|video-x-generic"
                size: pw.music ? 128 : 96
                opacity: pw.music ? 1 : 0.7
            }
            GLabel {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !pw.loaded && pw.path === "" && pw.waiting === ""
                color: Qt.rgba(1, 1, 1, 0.7)
                text: "Open a movie, a song, a disc or a web address"
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 10
                visible: !pw.loaded && pw.path === "" && pw.waiting === ""
                PushButton { text: "Open File…"; onClicked: pw.openDialog(); tip: "Pick movies or music to play (" + pw.k("pl.open") + ")" }
                PushButton { text: "Open Location…"; onClicked: pw.menuAction("location"); tip: "Play from a web address: YouTube and other sites, or a stream (" + pw.k("pl.location") + ")" }
            }
            BusyDots { anchors.horizontalCenter: parent.horizontalCenter; visible: (pw.path !== "" || pw.waiting !== "") && !pw.loaded && pw.errorText === ""; running: visible }
        }
        // Music: title and artist under the cover.
        Column {
            visible: pw.music
            x: 16; width: parent.width - 32
            y: Math.min(parent.width, parent.height - 90) - height - 10
            spacing: 2
            GLabel { width: parent.width; horizontalAlignment: Text.AlignHCenter; color: "white"; font.bold: true; style: Text.Outline; styleColor: Qt.rgba(0, 0, 0, 0.35)
                text: pw.metaValue("title") || pw.fileName; elide: Text.ElideRight }
            GLabel { width: parent.width; horizontalAlignment: Text.AlignHCenter; color: Qt.rgba(1, 1, 1, 0.8); style: Text.Outline; styleColor: Qt.rgba(0, 0, 0, 0.35)
                text: [pw.metaValue("artist") || pw.metaValue("album_artist"), pw.metaValue("album")].filter(x => !!x).join(" — "); visible: text !== ""; elide: Text.ElideRight }
        }

        GLabel {
            anchors.centerIn: parent
            width: Math.min(implicitWidth, parent.width - 40)
            visible: pw.errorText !== ""
            text: pw.errorText
            color: "white"
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }

        // Recording: the big red button, the time, where it goes.
        Column {
            visible: pw.mode === "record"
            anchors.centerIn: parent
            spacing: 16
            GLabel { anchors.horizontalCenter: parent.horizontalCenter; color: "white"; font.pixelSize: Theme.large; font.family: Theme.mono
                text: pw.recorder && pw.recorder.running ? pw.fmt(pw.recorder.position) : "0:00" }
            Rectangle {
                id: recBtn
                anchors.horizontalCenter: parent.horizontalCenter
                width: 64; height: 64; radius: 32
                color: "transparent"; border.color: "white"; border.width: 3
                readonly property bool on: !!pw.recorder && pw.recorder.running
                Rectangle {
                    anchors.centerIn: parent
                    width: recBtn.on ? 24 : 50; height: width; radius: recBtn.on ? 5 : 25
                    color: "#ff3b30"
                    Behavior on width { NumberAnimation { duration: 140 } }
                    Behavior on radius { NumberAnimation { duration: 140 } }
                }
                MouseArea { id: recMouse; anchors.fill: parent; hoverEnabled: true; onClicked: pw.recordToggle() }
                GTip { visible: recMouse.containsMouse; text: recBtn.on ? "Stop and play the recording" : "Record from the microphone (the system's default input)" }
            }
            GLabel { anchors.horizontalCenter: parent.horizontalCenter; color: Qt.rgba(1, 1, 1, 0.6); font.pixelSize: Theme.small
                text: pw.recorder && pw.recorder.running ? "Recording to Music › " + pw.recordFile.substring(pw.recordFile.lastIndexOf("/") + 1) : "Saved in your Music folder" }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: pw.showControls ? Qt.ArrowCursor : Qt.BlankCursor
            onPositionChanged: pw.poke()
            onPressed: (m) => { root.forceActiveFocus(); if (m.button === Qt.RightButton) appMenu.openAt(this, m.x, m.y) }
            onClicked: (m) => { if (m.button === Qt.LeftButton && pw.loaded && !pw.music) pw.toggle() }
            onDoubleClicked: (m) => { if (m.button === Qt.LeftButton && !pw.music) pw.toggleFullScreen() }
            // Drag the window by its picture (it has no title bar).
            property point pressAt
            onPressAndHold: pw.startSystemMove()
        }
        DropArea {
            anchors.fill: parent
            onDropped: (d) => { if (d.hasUrls) pw.openPaths(d.urls.map(u => decodeURIComponent(String(u).replace(/^file:\/\//, "")))) }
        }

        // ---------- top strip: title, window buttons ----------
        Item {
            id: top
            width: parent.width
            height: 40
            opacity: pw.showControls && !pw.pip ? 1 : 0
            visible: opacity > 0
            Behavior on opacity { NumberAnimation { duration: 260 } }
            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.6) }
                    GradientStop { position: 1; color: "transparent" }
                }
            }
            HoverHandler { id: topHover }
            MouseArea { anchors.fill: parent; onPressed: pw.startSystemMove(); onDoubleClicked: pw.toggleFullScreen() }
            GLabel {
                anchors.centerIn: parent
                width: Math.min(implicitWidth, parent.width - 220)
                text: pw.fileName + (pw.edited ? " — Edited" : "")
                color: "white"; font.bold: true
                elide: Text.ElideMiddle
            }
            // The menus, as grove's ☰ (for desktops without a global menu bar).
            GButton {
                id: menuBtn
                x: 10; anchors.verticalCenter: parent.verticalCenter
                iconName: "view-more-horizontal-symbolic|application-menu|open-menu-symbolic|overflow-menu"
                tint: "white"
                tip: "Menus: File, Edit, View, Window, Help (also in the menu bar at the top of the screen, and on a right-click)"
                // A second click closes it: the press outside already closed it.
                onClicked: { if (appMenu.visible || Date.now() - appMenu.closedAt < 250) appMenu.close(); else { pw.refreshRecents(); appMenu.openAt(menuBtn, 0, menuBtn.height + 4) } }
            }
            WindowControls { window: pw; anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; tint: "white" }
        }

        // ---------- the floating control bar ----------
        GlassPanel {
            id: bar
            visible: opacity > 0 && pw.mode === "" && !pw.trimming && !pw.clipEditing
            opacity: pw.showControls && pw.loaded ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 260 } }
            tint: Qt.rgba(0.11, 0.11, 0.12, 0.72)
            radius: 14
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: pw.music ? 10 : pw.pip ? 8 : 22
            width: Math.min(parent.width - (pw.pip ? 16 : 32), 560)
            height: pw.pip ? 42 : 72
            HoverHandler { id: barHover; onHoveredChanged: if (hovered) pw.poke() }
            // What fits: the least important go first as the bar narrows
            // (volume slider, sound output, picture in picture, subtitles,
            // the ±15 s buttons), so nothing overlaps.
            readonly property bool roomSlider: width >= 470
            readonly property bool roomOutput: width >= 430
            readonly property bool roomPip: width >= 395
            readonly property bool roomSubs: width >= 350
            readonly property bool roomSkip: width >= 300

            // Volume, left.
            Row {
                x: 12; y: 8
                visible: !pw.pip
                spacing: 2
                GButton { iconName: mp.muted || mp.volume === 0 ? "audio-volume-muted" : mp.volume < 40 ? "audio-volume-low" : mp.volume < 75 ? "audio-volume-medium" : "audio-volume-high"
                    tint: "white"; iconSize: 16; tip: mp.muted ? "Unmute (M)" : "Mute (M)"; onClicked: mp.setProp("mute", mp.muted ? "no" : "yes"); anchors.verticalCenter: parent.verticalCenter }
                PlayerSlider {
                    width: 78; anchors.verticalCenter: parent.verticalCenter
                    visible: bar.roomSlider
                    value: mp.muted ? 0 : mp.volume / 100
                    onMoved: v => pw.setVolume(v * 100)
                    tip: "Volume " + Math.round(mp.volume) + "% (↑/↓)"
                }
            }
            // Transport, centre.
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                y: pw.pip ? 3 : 4
                spacing: 16
                GButton { iconName: "media-seek-backward"; tint: "white"; iconSize: 20; tip: "Back 15 seconds (Shift+←)"; onClicked: pw.seekBy(-15); anchors.verticalCenter: parent.verticalCenter; visible: bar.roomSkip }
                GButton { iconName: pw.playing ? "media-playback-pause" : "media-playback-start"; tint: "white"; iconSize: 28
                    tip: (pw.playing ? "Pause" : "Play") + " (Space)"; onClicked: pw.toggle(); anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: "media-seek-forward"; tint: "white"; iconSize: 20; tip: "Forward 15 seconds (Shift+→)"; onClicked: pw.seekBy(15); anchors.verticalCenter: parent.verticalCenter; visible: bar.roomSkip }
            }
            // Sound output, subtitles, PiP, full screen, right.
            Row {
                anchors.right: parent.right; anchors.rightMargin: 10
                y: pw.pip ? 5 : 8
                spacing: 2
                GButton { id: outBtn; iconName: "audio-headphones"; tint: "white"; iconSize: 16; visible: !pw.pip && pw.outputs.length > 1 && bar.roomOutput
                    tip: "Play sound through… (like AirPlay: speakers, headphones, HDMI)"; onClicked: outMenu.popup(outBtn, 0, -outMenu.implicitHeight - 4); anchors.verticalCenter: parent.verticalCenter }
                GButton { id: subBtn; iconName: "media-view-subtitles-symbolic"; tint: "white"; iconSize: 16; visible: !pw.pip && pw.loaded && (pw.subTracks.length > 0 || pw.audioTracks.length > 1 || mp.hasVideo) && bar.roomSubs
                    tip: "Subtitles and audio languages"; onClicked: subMenu.popup(subBtn, 0, -subMenu.implicitHeight - 4); anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: pw.pip ? "view-fullscreen" : "view-restore"; tint: "white"; iconSize: 16; visible: mp.hasVideo && (pw.pip || bar.roomPip)
                    tip: pw.pip ? "Back to the full window (Esc)" : "Picture in picture: a small window that stays on top (" + pw.k("pl.pip") + ")"; onClicked: pw.menuAction("pip"); anchors.verticalCenter: parent.verticalCenter }
                GButton { iconName: pw.fullScreen ? "view-restore" : "view-fullscreen"; tint: "white"; iconSize: 16; visible: !pw.pip && !pw.music
                    tip: (pw.fullScreen ? "Exit full screen" : "Full screen") + " (F, double-click)"; onClicked: pw.toggleFullScreen(); anchors.verticalCenter: parent.verticalCenter }
            }
            // Scrubber: time played, the track, time left.
            Item {
                visible: !pw.pip
                x: 14; y: 44
                width: parent.width - 28
                height: 20
                GLabel { id: tNow; text: pw.fmt(mp.position); color: Qt.rgba(1, 1, 1, 0.85); font.pixelSize: Theme.small; font.family: Theme.mono; anchors.verticalCenter: parent.verticalCenter }
                PlayerSlider {
                    x: tNow.width + 10
                    width: parent.width - tNow.width - tLeft.width - 20
                    anchors.verticalCenter: parent.verticalCenter
                    value: mp.duration > 0 ? mp.position / mp.duration : 0
                    // What's buffered ahead, for streams.
                    extra: mp.duration > 0 && pw.info.demuxer_cache_duration ? Math.min(1, (mp.position + pw.info.demuxer_cache_duration) / mp.duration) : 0
                    onMoved: v => pw.seekTo(v * mp.duration)
                    hoverText: f => pw.fmt(f * mp.duration)
                    chapters: { try { return (JSON.parse(mp.chapters || "[]") || []).map(c => mp.duration > 0 ? c.time / mp.duration : 0) } catch (e) { return [] } }
                }
                GLabel { id: tLeft; anchors.right: parent.right; text: "−" + pw.fmt(mp.duration - mp.position); color: Qt.rgba(1, 1, 1, 0.85); font.pixelSize: Theme.small; font.family: Theme.mono; anchors.verticalCenter: parent.verticalCenter }
            }
            GMenu {
                id: outMenu
                Repeater {
                    model: pw.outputs
                    GMenuItem { required property var modelData; text: modelData.name === "auto" ? "System Default" : modelData.description
                        checkable: true; checked: mp.audioDevice === modelData.name; onTriggered: mp.setProp("audio-device", modelData.name) }
                }
            }
            GMenu {
                id: subMenu
                GMenuItem { text: "Subtitles Off"; checkable: true; checked: mp.sid <= 0; onTriggered: mp.setProp("sid", "no") }
                Repeater {
                    model: pw.subTracks
                    GMenuItem { required property var modelData; text: pw.trackName(modelData); checkable: true; checked: mp.sid === modelData.id
                        onTriggered: { mp.setProp("sid", String(modelData.id)); pw.subsShown = true } }
                }
                GMenuItem { text: "Add Subtitle File…"; onTriggered: pw.menuAction("addsub") }
                GMenuSeparator { visible: pw.audioTracks.length > 1 }
                Repeater {
                    model: pw.audioTracks.length > 1 ? pw.audioTracks : []
                    GMenuItem { required property var modelData; text: "Audio: " + pw.trackName(modelData); checkable: true; checked: mp.aid === modelData.id
                        onTriggered: mp.setProp("aid", String(modelData.id)) }
                }
            }
        }

        // ---------- Trim: the filmstrip in a yellow frame ----------
        GlassPanel {
            id: trimBar
            visible: pw.trimming || pw.clipEditing
            tint: Qt.rgba(0.11, 0.11, 0.12, 0.82)
            radius: 12
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 18
            width: parent.width - 32
            height: 96
            GButton {
                id: trimPlay
                x: 10; anchors.verticalCenter: strip.verticalCenter
                iconName: pw.playing ? "media-playback-pause" : "media-playback-start"; tint: "white"; iconSize: 22
                tip: pw.trimming ? "Play the yellow part (Space)" : "Play (Space)"
                onClicked: { if (pw.trimming && !pw.playing && (mp.position < pw.trimA || mp.position >= pw.trimB - 0.05)) pw.seekTo(pw.trimA); pw.toggle() }
            }
            // The clips side by side, each its slice of the filmstrip.
            Item {
                id: strip
                x: trimPlay.x + trimPlay.width + 10
                y: 12
                width: buttons.x - x - 12
                height: 52
                readonly property real total: Math.max(0.001, pw.total)
                function xOf(t) { return t / total * width }
                function tOf(x) { return Math.max(0, Math.min(total, x / width * total)) }
                Row {
                    anchors.fill: parent
                    spacing: 0
                    Repeater {
                        model: pw.clips
                        Item {
                            required property var modelData
                            required property int index
                            width: (modelData.end - modelData.start) / strip.total * strip.width
                            height: strip.height
                            clip: true
                            Rectangle { anchors.fill: parent; color: "#2a2a2c" }
                            Image {
                                // The source's whole strip, shifted so this clip's part shows.
                                visible: pw.filmstrip !== ""
                                source: pw.filmstrip
                                height: parent.height
                                width: pw.sourceDuration > 0 ? pw.sourceDuration / (modelData.end - modelData.start) * parent.width : parent.width
                                x: pw.sourceDuration > 0 ? -modelData.start / pw.sourceDuration * width : 0
                                fillMode: Image.Stretch
                                smooth: true
                                asynchronous: true
                            }
                            // Clips: a gap between them, the chosen one outlined.
                            Rectangle { visible: pw.clipEditing; anchors.fill: parent; color: "transparent"; border.width: pw.selectedClip === index ? 3 : 1
                                border.color: pw.selectedClip === index ? "#ffcc00" : Qt.rgba(0, 0, 0, 0.8); radius: 3 }
                            MouseArea {
                                anchors.fill: parent
                                enabled: pw.clipEditing
                                onClicked: (m) => { pw.selectedClip = index; let o = 0; for (let i = 0; i < index; i++) o += pw.clips[i].end - pw.clips[i].start; pw.seekTo(o + m.x / width * (modelData.end - modelData.start)); root.forceActiveFocus() }
                            }
                        }
                    }
                }
                // Outside the yellow frame: dimmed.
                Rectangle { visible: pw.trimming; x: 0; width: strip.xOf(pw.trimA); height: parent.height; color: Qt.rgba(0, 0, 0, 0.6) }
                Rectangle { visible: pw.trimming; x: strip.xOf(pw.trimB); width: parent.width - x; height: parent.height; color: Qt.rgba(0, 0, 0, 0.6) }
                // QuickTime's yellow frame with its two handles.
                Rectangle {
                    visible: pw.trimming
                    x: strip.xOf(pw.trimA) - 10; y: -3
                    width: strip.xOf(pw.trimB) - strip.xOf(pw.trimA) + 20; height: parent.height + 6
                    color: "transparent"; radius: 5
                    border.color: "#ffcc00"; border.width: 3
                }
                component TrimHandle: Rectangle {
                    id: th
                    property bool isStart: true
                    y: -3; width: 12; height: strip.height + 6
                    radius: 4
                    color: thMouse.pressed ? "#ffd633" : "#ffcc00"
                    visible: pw.trimming
                    // The handle's chevron.
                    Rectangle { anchors.centerIn: parent; width: 2; height: 16; radius: 1; color: Qt.rgba(0, 0, 0, 0.55) }
                    MouseArea {
                        id: thMouse
                        anchors.fill: parent
                        anchors.margins: -6
                        hoverEnabled: true
                        cursorShape: Qt.SizeHorCursor
                        onPositionChanged: (m) => {
                            if (!pressed) return
                            const t = strip.tOf(mapToItem(strip, m.x, 0).x)
                            if (th.isStart) pw.trimA = Math.min(t, pw.trimB - 0.2)
                            else pw.trimB = Math.max(t, pw.trimA + 0.2)
                            pw.seekTo(th.isStart ? pw.trimA : pw.trimB)
                        }
                        onPressed: { if (pw.playing) mp.setProp("pause", "yes") }
                    }
                    GTip { visible: thMouse.containsMouse || thMouse.pressed; text: (th.isStart ? "Start " : "End ") + pw.fmt(th.isStart ? pw.trimA : pw.trimB) + " — drag to trim" }
                }
                TrimHandle { isStart: true; x: strip.xOf(pw.trimA) - width + 2 }
                TrimHandle { isStart: false; x: strip.xOf(pw.trimB) - 2 }
                // The playhead.
                Rectangle { x: strip.xOf(mp.position) - 1; y: -6; width: 2; height: parent.height + 12; radius: 1; color: "white"; visible: mp.duration > 0 }
                MouseArea {
                    anchors.fill: parent
                    z: -1
                    onPressed: (m) => pw.seekTo(strip.tOf(m.x))
                    onPositionChanged: (m) => { if (pressed) pw.seekTo(strip.tOf(m.x)) }
                }
            }
            GLabel {
                x: strip.x; y: strip.y + strip.height + 8
                color: Qt.rgba(1, 1, 1, 0.75); font.pixelSize: Theme.small
                text: pw.trimming ? pw.fmt(pw.trimA) + " – " + pw.fmt(pw.trimB) + "  (" + pw.fmt(pw.trimB - pw.trimA) + ")"
                                  : pw.clips.length + " clips · click one to choose it, Delete removes it, Edit › Split Clip (" + pw.k("pl.split") + ") at the playhead"
            }
            Row {
                id: buttons
                anchors.right: parent.right; anchors.rightMargin: 12
                anchors.verticalCenter: strip.verticalCenter
                spacing: 8
                PushButton { text: "Cancel"; visible: pw.trimming; onClicked: pw.cancelTrim(); tip: "Leave the movie as it was (Esc)" }
                PushButton { text: pw.trimming ? "Trim" : "Done"; primary: true; onClicked: pw.trimming ? pw.applyTrim() : (pw.clipEditing = false); tip: pw.trimming ? "Keep only the yellow part (Return). Undo brings it back" : "Close the clips" }
            }
        }

        // Resize from every edge and corner: the window has no frame, so
        // the window manager draws no borders to drag.
        Repeater {
            model: pw.visibility !== Window.Maximized && !pw.fullScreen ? [
                { e: Qt.LeftEdge, x: 0, y: 8, w: 6, h: -16, c: Qt.SizeHorCursor }, { e: Qt.RightEdge, x: -6, y: 8, w: 6, h: -16, c: Qt.SizeHorCursor },
                { e: Qt.TopEdge, x: 8, y: 0, w: -16, h: 5, c: Qt.SizeVerCursor }, { e: Qt.BottomEdge, x: 8, y: -6, w: -16, h: 6, c: Qt.SizeVerCursor },
                { e: Qt.TopEdge | Qt.LeftEdge, x: 0, y: 0, w: 10, h: 10, c: Qt.SizeFDiagCursor }, { e: Qt.TopEdge | Qt.RightEdge, x: -10, y: 0, w: 10, h: 10, c: Qt.SizeBDiagCursor },
                { e: Qt.BottomEdge | Qt.LeftEdge, x: 0, y: -10, w: 10, h: 10, c: Qt.SizeBDiagCursor }, { e: Qt.BottomEdge | Qt.RightEdge, x: -10, y: -10, w: 10, h: 10, c: Qt.SizeFDiagCursor } ] : []
            MouseArea {
                required property var modelData
                z: 100
                x: modelData.x < 0 ? root.width + modelData.x : modelData.x
                y: modelData.y < 0 ? root.height + modelData.y : modelData.y
                width: modelData.w <= 0 ? root.width + modelData.w : modelData.w
                height: modelData.h <= 0 ? root.height + modelData.h : modelData.h
                hoverEnabled: true
                cursorShape: modelData.c
                onPressed: pw.beginSystemResize(modelData.e)
            }
        }

        // Exports in progress (each with Cancel) and the last result.
        Column {
            anchors.right: parent.right; anchors.rightMargin: 14
            y: top.height + 6
            spacing: 6
            Repeater {
                model: pw.exports
                GlassPanel {
                    required property var modelData
                    width: 260; height: 54
                    tint: Qt.rgba(0.11, 0.11, 0.12, 0.85); radius: 10
                    GLabel { x: 12; y: 8; width: parent.width - 50; color: "white"; font.pixelSize: Theme.small; elide: Text.ElideMiddle
                        text: "Exporting " + modelData.preset + ": " + modelData.name }
                    Rectangle { x: 12; y: 34; width: parent.width - 50; height: 5; radius: 2.5; color: Qt.rgba(1, 1, 1, 0.2)
                        Rectangle { width: parent.width * modelData.job.progress; height: parent.height; radius: 2.5; color: "white" } }
                    GButton { anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; iconName: "dialog-cancel"; tint: "white"; iconSize: 16
                        tip: "Stop exporting (the part made is deleted)"; onClicked: modelData.job.stop(false) }
                }
            }
        }
        Rectangle {
            visible: pw.toastText !== ""
            anchors.horizontalCenter: parent.horizontalCenter
            y: top.height + 8
            width: toastLabel.implicitWidth + 28; height: toastLabel.implicitHeight + 14; radius: height / 2
            color: Qt.rgba(0.11, 0.11, 0.12, 0.85)
            GLabel { id: toastLabel; anchors.centerIn: parent; color: "white"; text: pw.toastText }
        }
    }

    // ---------- Movie Inspector ----------
    Window {
        id: inspector
        title: "Movie Inspector"
        width: 340; height: col.implicitHeight + 32
        flags: Qt.Tool | Qt.WindowStaysOnTopHint
        color: "#1c1c1e"
        transientParent: pw
        readonly property var rows: {
            const i = pw.info, vp = i.video_params || {}, ap = i.audio_params || {}
            const r = []
            const mb = b => b > 0 ? (b >= 1e9 ? (b / 1e9).toFixed(2) + " GB" : (b / 1e6).toFixed(1) + " MB") : ""
            const rate = b => b > 0 ? (b >= 1e6 ? (b / 1e6).toFixed(2) + " Mbit/s" : Math.round(b / 1000) + " kbit/s") : ""
            r.push(["Source", pw.isUrl || pw.disc ? pw.path : pw.path.substring(0, pw.path.lastIndexOf("/"))])
            r.push(["Container", i.file_format || ""])
            r.push(["Video", [i.video_codec || i.video_format, vp.pixelformat].filter(x => !!x).join(", ")])
            r.push(["Resolution", vp.w ? vp.w + " × " + vp.h + (pw.rotation ? " (turned " + pw.rotation + "°)" : "") : ""])
            r.push(["Current Size", pw.videoW ? Math.round(video.width * Screen.devicePixelRatio) + " × " + Math.round(video.height * Screen.devicePixelRatio) : ""])
            r.push(["Frame Rate", i.container_fps ? Number(i.container_fps).toFixed(3).replace(/\.?0+$/, "") + " fps" : ""])
            r.push(["Colour", [vp.colormatrix, vp.primaries, vp.gamma].filter(x => !!x).join(" / ")])
            r.push(["Video Data Rate", rate(i.video_bitrate)])
            r.push(["Audio", [i.audio_codec_name || i.audio_codec, ap.samplerate ? (ap.samplerate / 1000) + " kHz" : "", ap["hr-channels"] || ap.channels].filter(x => !!x).join(", ")])
            r.push(["Audio Data Rate", rate(i.audio_bitrate)])
            r.push(["Data Size", mb(i.file_size)])
            r.push(["Duration", mp.duration > 0 ? pw.fmt(mp.duration) : ""])
            r.push(["Hardware Decoding", i.hwdec_current && i.hwdec_current !== "no" ? i.hwdec_current : "off (software)"])
            return r.filter(x => x[1] !== "")
        }
        Column {
            id: col
            x: 16; y: 16.001
            width: parent.width - 32
            spacing: 6
            GLabel { text: pw.fileName; color: "white"; font.bold: true; width: parent.width; elide: Text.ElideMiddle }
            Repeater {
                model: inspector.rows
                Row {
                    required property var modelData
                    spacing: 10
                    GLabel { width: 118; horizontalAlignment: Text.AlignRight; text: modelData[0]; color: Qt.rgba(1, 1, 1, 0.55); font.pixelSize: Theme.small }
                    GLabel { width: col.width - 128; text: modelData[1]; color: "white"; font.pixelSize: Theme.small; wrapMode: Text.Wrap; elide: Text.ElideNone }
                }
            }
        }
    }

    // ---------- sheets ----------
    Sheet {
        id: locationSheet
        title: "Open Location"
        message: "A web address: YouTube and the other sites yt-dlp knows, a stream (http, rtsp, …), or a disc (dvd://, bd://)."
        GField { id: urlField; width: parent.width; placeholderText: "https://"; onAccepted: okLoc.clicked() }
        buttons: [
            PushButton { text: "Cancel"; onClicked: locationSheet.close() },
            PushButton { id: okLoc; text: "Open"; primary: true; enabled: urlField.text.trim() !== ""
                onClicked: { const u = urlField.text.trim(); locationSheet.close(); if (pw.path && pw.loaded) app.newPlayer(u); else pw.open(u) } }
        ]
    }
    Sheet {
        id: aboutSheet
        title: "Grove Player"
        message: "QuickTime Player for grove. Plays through libmpv: every container and codec FFmpeg knows, subtitles, several audio tracks, web addresses (yt-dlp), DVDs and Blu-rays, with hardware decoding."
        buttons: [PushButton { text: "OK"; primary: true; onClicked: aboutSheet.close() }]
    }

    function mp_end() { return Math.max(0, mp.duration - 0.05) }

    onClosing: {
        if (recorder && recorder.running) recorder.stop(true)
        if (loaded && !isUrl) mp.command(JSON.stringify(["write-watch-later-config"]))
    }

    // ---------- tests ----------
    // GROVE_PLAYER_TEST: "controls", "trim", "clips", "inspector", "pip", "menu", "width:N",
    // "export:<preset>:<file>" (trims to 20%..60%, exports, quits when done).
    readonly property string testSpec: app ? app.grove.env("GROVE_PLAYER_TEST") : ""
    readonly property bool testExport: testSpec.indexOf("export:") === 0
    Timer {
        id: testTimer
        property bool ran: false
        interval: 1500
        onTriggered: {
            const t = pw.testSpec
            if (t === "controls") { pw.controls = true; mp.setProp("pause", "yes"); pw.seekTo(mp.duration * 0.3) }
            else if (t === "trim") { pw.startTrim(); pw.trimA = pw.total * 0.2; pw.trimB = pw.total * 0.7; pw.seekTo(pw.total * 0.35) }
            else if (t === "clips") { pw.seekTo(pw.total * 0.4); Qt.callLater(() => { splitTimer.start() }) }
            else if (t === "inspector") { mp.setProp("pause", "yes"); inspector.show() }
            else if (t === "pip") pw.enterPip()
            else if (t === "menu") { mp.setProp("pause", "yes"); pw.controls = true; appMenu.openAt(menuBtn, 0, menuBtn.height + 4) }
            else if (t.indexOf("width:") === 0) { pw.sized = true; pw.width = Number(t.substring(6)); pw.controls = true; mp.setProp("pause", "yes") }
            else if (pw.testExport) {
                const [, preset, out] = t.split(":")
                pw.startTrim(); pw.trimA = pw.total * 0.2; pw.trimB = pw.total * 0.6; pw.applyTrim()
                Qt.callLater(() => pw.startExport(preset, out))
            }
        }
    }
    Timer { id: splitTimer; interval: 600; onTriggered: { pw.splitClip(); mp.setProp("pause", "yes") } }
    Connections {
        target: mp
        enabled: pw.testSpec !== ""
        function onDurationChanged() { if (mp.duration > 0 && !testTimer.ran) { testTimer.ran = true; testTimer.start() } }
    }
    Component.onCompleted: {
        testTimer.ran = false
        mp.setProp("save-position-on-quit", "yes")
        refreshRecents()
        if (path) { const p = path; path = ""; open(p) }
        // (open() waits for the picture's render context.)
        else if (mode === "" && app.screenshot === "") Qt.callLater(openDialog)
        poke()
    }
}

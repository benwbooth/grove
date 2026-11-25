import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Window
import GlobalMenu

// Grove Preview: macOS Preview for grove. Opens PDFs (and EPUB, XPS,
// comics read-only) and pictures, several in one window with a sidebar of
// thumbnails; search, select and copy text; Markup (sketch, shapes, text,
// notes, signatures, highlights, loupe, Instant Alpha); fill in forms;
// move, insert, delete, rotate and crop pages; Adjust Color and Adjust
// Size for pictures; save, export (any picture format or PDF, optionally
// encrypted), print. Changes stay in the window until saved; Save puts
// the original in the Trash (Undo in grove brings it back).
ApplicationWindow {
    id: pw
    property var app
    property var initialPaths: []
    width: Math.round(1080 * Theme.pt)
    height: Math.round(760 * Theme.pt)
    minimumWidth: 520
    minimumHeight: 360
    color: Theme.window
    readonly property bool clientDecorations: app.settings.client_decorations !== false
    flags: clientDecorations ? (Qt.Window | Qt.FramelessWindowHint) : Qt.Window
    title: cur ? cur.name + (cur.dirty ? " — Edited" : "") : "Grove Preview"

    // ---------- documents ----------
    property var docs: []
    property var cur: null
    property int seq: 0
    property int currentPage: 0
    property var pageOf: ({})
    readonly property bool isPdf: !!cur && cur.kind === "pdf"
    readonly property bool canEdit: !!cur && canEditDoc(cur)
    // Documents shown from a PDF made of them (EPUB, DjVu, PostScript, Office, comics) can be
    // read, searched, printed and exported as PDF, not changed.
    readonly property bool canExport: !!cur && (canEdit || (!!cur.info && !!cur.info.converted))

    // ---------- tools and style ----------
    property string tool: "textsel"
    property var sel: null            // the selected Markup op {key, i}
    property var textSel: null        // {key, docId, index, quads, text}
    property var rectSel: null        // {key, docId, index?, rect: [x0, y0, x1, y1]}
    property var selPages: ({ docId: -1, list: [] })
    property color mkColor: "#ff3b30"
    property string mkFill: ""
    property real mkWidth: 2
    property string mkFont: "Helvetica"
    property real mkFontSize: 14
    property bool mkBold: false
    property bool mkItalic: false
    property string mkAlign: "left"
    property color hlColor: "#ffcc00"
    property var signatures: []
    property int signatureIndex: 0
    property bool markupShown: false
    property bool adjustColorOpen: adjustColor.visible

    // ---------- sidebar and search ----------
    property bool sidebarShown: true
    property string sidebarMode: "thumbs"      // thumbs, toc, notes, bookmarks
    property var searchHits: null
    property bool searching: false
    property int searchIndex: -1
    property string searchText: ""
    readonly property var searchCurrentHit: searchHits && searchIndex >= 0 ? searchHits[searchIndex] : null

    function toast(level, msg) { toastBox.show(level, msg) }
    function keyFor(id) { return Help.keysFor(id) }
    function isDocumentPath(p) { return /\.(pdf|ai|xps|oxps|epub|mobi|azw|azw3|fb2|fbz|cbz|cbr|cb7|cbt|djvu|djv|ps|eps|epsf|odt|ods|odp|odg|ott|ots|otp|docx|xlsx|pptx|doc|dot|xls|ppt|rtf)$/i.test(p) }
    function docById(id) { for (const d of docs) if (d.id === id) return d; return null }
    function passwordFor(src) { for (const d of docs) if (d.path === src && d.password) return d.password; return "" }
    function nameOf(p) { return p.substring(p.lastIndexOf("/") + 1) }

    // Replace a document with a changed copy (bindings see a new object).
    function update(d, patch) {
        const old = docById(d.id)
        if (!old) return null
        const n = Object.assign({}, old, patch)
        docs = docs.map(x => x.id === n.id ? n : x)
        if (cur && cur.id === n.id) cur = n
        return n
    }
    // An undoable change: the editable state before it is kept.
    function state(d) { return JSON.stringify({ pages: d.pages, fields: d.fields, img: d.img, meta: d.meta }) }
    function change(d, label, patch) {
        const old = docById(d.id)
        if (!old || !canEditDoc(old)) return
        const undo = (old.undo || []).slice()
        const now = Date.now()
        const coalesce = undo.length && undo[undo.length - 1].label === label && now - undo[undo.length - 1].t < 1500 && (label === "Adjust Color" || label === "Typing" || label === "Move")
        if (coalesce) undo[undo.length - 1].t = now
        else undo.push({ label: label, t: now, s: state(old) })
        if (undo.length > 200) undo.shift()
        update(old, Object.assign({ undo: undo, redo: [], dirty: true }, patch))
    }
    function canEditDoc(d) { return d.kind === "image" || (d.kind === "pdf" && !!d.info && d.info.isPdf && !d.info.converted) }
    function undo() {
        if (!cur || !(cur.undo || []).length) return
        const u = cur.undo.slice(), last = u.pop()
        const r = (cur.redo || []).concat([{ label: last.label, s: state(cur) }])
        update(cur, Object.assign({ undo: u, redo: r, dirty: u.length > 0 || cur.savedDirty === true }, JSON.parse(last.s)))
        sel = null
    }
    function redo() {
        if (!cur || !(cur.redo || []).length) return
        const r = cur.redo.slice(), last = r.pop()
        const u = (cur.undo || []).concat([{ label: last.label, t: 0, s: state(cur) }])
        update(cur, Object.assign({ undo: u, redo: r, dirty: true }, JSON.parse(last.s)))
        sel = null
    }

    function setPageOps(doc, i, ops, label) {
        const d = docById(doc.id)
        const pages = d.pages.slice()
        pages[i] = Object.assign({}, pages[i], { annots: ops })
        change(d, label, { pages: pages })
    }
    function changeImage(doc, patch, label) {
        const d = docById(doc.id)
        if (!d || !d.img) return
        change(d, label, { img: Object.assign({}, d.img, patch) })
    }
    function setField(doc, key, v, w) {
        const d = docById(doc.id)
        const f = Object.assign({}, d.fields || {})
        f[key] = v
        // Radio buttons: the others of the group go off.
        if (w && w.kind === "radio" && v !== "Off") {
            const [src] = key.split("|")
            for (const o of (d.widgets[src] || [])) if (o.kind === "radio" && o.name === w.name && !(o.page === w.page && o.i === w.i)) f[src + "|" + o.page + "|" + o.i] = "Off"
        }
        change(d, "Fill In", { fields: f })
    }
    function nextField(f, dir) {
        // Tab: the next text field on the page (in reading order).
        Qt.callLater(() => {})
    }
    function setMeta(doc, k, v) {
        const d = docById(doc.id)
        const m = Object.assign({}, d.meta || {})
        m[k] = v
        change(d, "Change Info", { meta: m })
    }

    // ---------- opening ----------
    function openFiles(paths, replace) {
        const fresh = []
        for (const p of paths) {
            if (docs.some(d => d.path === p)) { setCurrent(docs.find(d => d.path === p), 0); continue }
            const d = { id: ++seq, path: p, name: nameOf(p), kind: isDocumentPath(p) ? "pdf" : "image", rev: 0, pages: [], fields: {}, widgets: {},
                        img: null, info: null, undo: [], redo: [], dirty: false, password: "", meta: {}, bookmarks: (app.settings.preview_bookmarks || {})[p] || [], loading: true }
            fresh.push(d)
        }
        if (!fresh.length) return
        docs = docs.concat(fresh)
        if (!cur || replace) { cur = docById(fresh[0].id); currentPage = 0 }
        for (const d of fresh) load(d)
    }
    function load(d, password) {
        const p = d.path, pw0 = password || d.password || ""
        Async.call(app.grove, k => app.grove.pvInfo(k, p, pw0), r => {
            const now = docById(d.id)
            if (!now) return
            if (r.error) { update(now, { loading: false, error: r.error }); if (cur && cur.id === d.id) toast("error", "“" + now.name + "” can't be opened: " + r.error); return }
            if (r.kind === "pdf" && r.locked) { update(now, { loading: false, info: r, locked: true }); if (password) toast("error", "That password isn't right."); askPassword(now); return }
            if (r.kind === "pdf") {
                const src = r.viewPath || p
                const pages = r.sizes.map((s, i) => ({ src: src, page: i, w: s[0], h: s[1], rot: 0, annots: [], delAnnots: [], crop: null, key: "p" + i + "-" + now.rev }))
                const n = update(now, { kind: "pdf", loading: false, locked: false, info: r, pages: pages, password: pw0, encryptedWith: r.encrypted ? pw0 : "" })
                if (r.isPdf && !r.converted) Async.call(app.grove, k => app.grove.pvWidgets(k, p, pw0), w => {
                    const x = docById(d.id)
                    if (!x || !Array.isArray(w) || !w.length) return
                    const ws = Object.assign({}, x.widgets); ws[p] = w
                    update(x, { widgets: ws })
                })
            } else if (r.kind === "text") {
                // Text, code, Markdown, web pages: as grove's Quick Look shows them.
                update(now, { kind: "text", loading: false, info: r })
            } else {
                update(now, { kind: "image", loading: false, info: r, img: { w: r.w, h: r.h, alpha: [], adjust: {}, ops: [], crop: null, rotate: 0, flipH: false, flipV: false, resize: null, dpi: null } })
            }
        })
    }
    // After saving or reverting: everything from the file again.
    function reload(d) {
        const x = docById(d.id)
        update(x, { rev: (x.rev || 0) + 1, dirty: false, undo: [], redo: [], fields: {}, widgets: {}, meta: {}, loading: true })
        load(docById(d.id))
        textSel = null; rectSel = null; sel = null
    }
    function closeDoc(d) {
        const x = docById(d.id)
        if (x && x.dirty) { askSave(x, () => closeDoc(x)); return }
        docs = docs.filter(o => o.id !== d.id)
        if (cur && cur.id === d.id) { cur = docs.length ? docs[0] : null; currentPage = cur ? (pageOf[cur.id] || 0) : 0 }
        if (!docs.length) pw.close()
    }

    // ---------- navigation and selection ----------
    function setCurrent(doc, i) {
        const d = docById(doc.id)
        if (!d) return
        if (!cur || cur.id !== d.id) {
            if (cur) { const m = Object.assign({}, pageOf); m[cur.id] = currentPage; pageOf = m }
            cur = d
            searchHitsForCur()
        }
        const n = d.kind === "pdf" ? Math.max(0, Math.min(d.pages.length - 1, i)) : 0
        currentPage = n
        if (d.kind === "pdf") Qt.callLater(() => docView.go(n))
    }
    function setCurrentQuiet(i) { currentPage = i }
    function clickPage(doc, i, mods) {
        const d = docById(doc.id)
        let list = selPages.docId === d.id ? selPages.list.slice() : []
        if (mods & Qt.ControlModifier) { const at = list.indexOf(i); if (at >= 0) list.splice(at, 1); else list.push(i) }
        else if ((mods & Qt.ShiftModifier) && list.length) { const a = list[list.length - 1]; for (let k = Math.min(a, i); k <= Math.max(a, i); k++) if (list.indexOf(k) < 0) list.push(k) }
        else list = [i]
        selPages = { docId: d.id, list: list }
        setCurrent(d, i)
    }
    function isPageSelected(doc, i) {
        if (!doc) return false
        if (selPages.docId === doc.id && selPages.list.length) return selPages.list.indexOf(i) >= 0
        return cur && cur.id === doc.id && (doc.kind === "image" || currentPage === i)
    }
    function selectedPages(doc) {
        if (selPages.docId === doc.id && selPages.list.length) return selPages.list.slice().sort((a, b) => a - b)
        return [cur && cur.id === doc.id ? currentPage : 0]
    }
    function toggleCollapsed(d) { update(d, { collapsed: !docById(d.id).collapsed }) }
    function select(key, i) { sel = key ? { key: key, i: i } : null }
    function setTool(t) {
        tool = t
        if (t !== "textsel" && t !== "rectsel") { textSel = null }
        if (t !== "textsel" && t !== "rectsel" && t !== "alpha" && t !== "hl" && t !== "ul" && t !== "so") markupShown = true
    }
    function toolDone() { tool = "textsel" }
    function goPage(n) { if (cur && cur.kind === "pdf") setCurrent(cur, n) }
    function followLink(doc, l) {
        if (l.uri) { Qt.openUrlExternally(l.uri); return }
        const d = docById(doc.id)
        const src = l.src || d.path
        let i = d.pages.findIndex(p => p.src === src && p.page === l.page)
        if (i < 0) i = l.page
        if (i >= 0) { setCurrent(d, i); docView.goTo(i, l.y || 0) }
    }
    function gotoOutline(o) {
        if (o.uri) { Qt.openUrlExternally(o.uri); return }
        if (o.page < 0) return
        const i = cur.pages.findIndex(p => p.src === cur.path && p.page === o.page)
        setCurrent(cur, i >= 0 ? i : o.page)
        docView.goTo(i >= 0 ? i : o.page, o.y || 0)
    }

    // ---------- markup ----------
    function setStyle(k, v) {
        if (k === "c") mkColor = v
        else if (k === "fill") mkFill = v
        else if (k === "w") mkWidth = v
        else if (k === "font") mkFont = v
        else if (k === "s") mkFontSize = v
        else if (k === "b") mkBold = v
        else if (k === "i") mkItalic = v
        else if (k === "al") mkAlign = v
        // The selected op takes the style too (as in Preview).
        const t = selTarget()
        if (!t) return
        const o = Object.assign({}, t.ops[t.i])
        if (k === "w") o.w = v * (t.unit || 1)
        else if (k === "s") o.s = v * (t.unit || 1)
        else if (k === "fill") { if (v === "") delete o.fill; else o.fill = v }
        else if (k === "c") o.c = v
        else o[k] = v
        const n = t.ops.slice(); n[t.i] = o
        t.set(n, "Change Style")
    }
    // The selected op's document, page and ops ({ops, i, set(n, label), unit}).
    function selTarget() {
        if (!sel || !cur) return null
        const [id, part] = sel.key.split(":")
        const d = docById(Number(id))
        if (!d) return null
        if (part === "img") return { ops: d.img.ops, i: sel.i, unit: Math.max(1, Math.max(d.img.w, d.img.h) / 1000), set: (n, l) => changeImage(d, { ops: n }, l) }
        const pi = d.pages.findIndex(p => p.key === part)
        if (pi < 0) return null
        return { ops: d.pages[pi].annots || [], i: sel.i, unit: 1, set: (n, l) => setPageOps(d, pi, n, l) }
    }
    function deleteSelectedOp() {
        const t = selTarget()
        if (!t || t.i >= t.ops.length) return false
        const n = t.ops.slice(); n.splice(t.i, 1)
        sel = null
        t.set(n, "Delete")
        return true
    }
    property var noteLayer: null
    property int noteIndex: -1
    function editNote(layer, i) {
        noteLayer = layer; noteIndex = i
        noteText.text = layer.ops[i].text || ""
        notePop.open()
        noteText.forceActiveFocus()
    }
    function currentSignature() { return signatures.length ? signatures[Math.min(signatureIndex, signatures.length - 1)] : null }
    function showSignatures() { sigSheet.open2() }
    function addSignature(s) { signatures = signatures.concat([s]); signatureIndex = signatures.length - 1; saveSignatures() }
    function removeSignature(i) { const n = signatures.slice(); n.splice(i, 1); signatures = n; signatureIndex = 0; saveSignatures() }
    function saveSignatures() { const l = JSON.stringify(signatures); Async.call(app.grove, k => app.grove.pvSetSignatures(k, l), r => { if (r.error) toast("error", r.error) }) }

    // ---------- pages ----------
    function rotate(deg) {
        if (!cur) return
        if (cur.kind === "image") { changeImage(cur, { rotate: ((cur.img.rotate + deg) % 360 + 360) % 360 }, deg < 0 ? "Rotate Left" : "Rotate Right"); return }
        const list = selectedPages(cur)
        const pages = cur.pages.map((p, i) => list.indexOf(i) >= 0 ? Object.assign({}, p, { rot: ((p.rot + deg) % 360 + 360) % 360 }) : p)
        change(cur, deg < 0 ? "Rotate Left" : "Rotate Right", { pages: pages })
    }
    function flip(horizontal) { if (cur && cur.kind === "image") changeImage(cur, horizontal ? { flipH: !cur.img.flipH } : { flipV: !cur.img.flipV }, horizontal ? "Flip Horizontal" : "Flip Vertical") }
    function deletePages() {
        if (!isPdf) return
        const list = selectedPages(cur)
        if (list.length >= cur.pages.length) { toast("error", "A document needs at least one page."); return }
        change(cur, list.length > 1 ? "Delete Pages" : "Delete Page", { pages: cur.pages.filter((p, i) => list.indexOf(i) < 0) })
        selPages = { docId: -1, list: [] }
        currentPage = Math.min(currentPage, cur.pages.length - 1)
    }
    function movePages(from, idx, to, at) {
        const a = docById(from.id), b = docById(to.id)
        if (!a || !b || b.kind !== "pdf") return
        const moving = idx.map(i => a.pages[i]).filter(p => p)
        if (a.id === b.id) {
            const keep = a.pages.filter((p, i) => idx.indexOf(i) < 0)
            const before = idx.filter(i => i < at).length
            const pos = Math.max(0, Math.min(keep.length, at - before))
            const pages = keep.slice(0, pos).concat(moving, keep.slice(pos))
            if (JSON.stringify(pages.map(p => p.key)) === JSON.stringify(a.pages.map(p => p.key))) return
            change(a, "Move Pages", { pages: pages })
            selPages = { docId: a.id, list: moving.map((p, i) => pos + i) }
            return
        }
        // To another document: copied there (fields follow), removed here.
        const copies = moving.map(p => Object.assign({}, p, { key: p.key + "-m" + (++seq) }))
        change(b, "Add Pages", { pages: b.pages.slice(0, at).concat(copies, b.pages.slice(at)) })
        if (a.kind === "pdf" && moving.length < a.pages.length) change(a, "Move Pages", { pages: a.pages.filter((p, i) => idx.indexOf(i) < 0) })
        setCurrent(b, at)
    }
    function insertBlank() {
        if (!isPdf) return
        const at = currentPage + 1, ref = cur.pages[currentPage] || { w: 612, h: 792 }
        const pg = { src: "", page: -1, blank: [ref.w, ref.h], w: ref.w, h: ref.h, rot: 0, annots: [], delAnnots: [], crop: null, key: "b" + (++seq) }
        change(cur, "Insert Blank Page", { pages: cur.pages.slice(0, at).concat([pg], cur.pages.slice(at)) })
        setCurrent(cur, at)
    }
    // Files into a document: PDFs' pages, pictures as pages.
    function insertFiles(doc, at, paths) {
        const d = docById(doc.id)
        if (d.kind !== "pdf") { openFiles(paths, false); return }
        let pending = paths.length
        const add = []
        paths.forEach((p, n) => {
            Async.call(app.grove, k => app.grove.pvInfo(k, p, ""), r => {
                if (r.kind === "pdf" && r.sizes) add[n] = r.sizes.map((s, i) => ({ src: p, page: i, w: s[0], h: s[1], rot: 0, annots: [], delAnnots: [], crop: null, key: "i" + (++seq) }))
                else if (r.kind === "image" && r.w) add[n] = [{ src: "", page: -1, image: p, w: r.w * 0.75, h: r.h * 0.75, rot: 0, annots: [], delAnnots: [], crop: null, key: "g" + (++seq) }]
                else toast("error", "“" + nameOf(p) + "” can't be added" + (r.error ? ": " + r.error : "."))
                if (--pending === 0) {
                    const x = docById(d.id)
                    const pages = [].concat(...add.filter(a => a))
                    if (!pages.length) return
                    change(x, pages.length > 1 ? "Insert Pages" : "Insert Page", { pages: x.pages.slice(0, at).concat(pages, x.pages.slice(at)) })
                    setCurrent(x, at)
                }
            })
        })
    }
    function insertFromFile() {
        if (!isPdf) return
        const d = cur, at = currentPage + 1
        chooseFiles("Insert Pages from", { mode: "open", multiple: true, currentFolder: d.path.substring(0, d.path.lastIndexOf("/")) }, paths => insertFiles(d, at, paths))
    }
    function toggleBookmark(i) {
        if (!isPdf) return
        const n = i === undefined ? currentPage : i
        const b = (cur.bookmarks || []).slice()
        const at = b.indexOf(n)
        if (at >= 0) b.splice(at, 1); else { b.push(n); b.sort((x, y) => x - y) }
        update(cur, { bookmarks: b })
        const all = Object.assign({}, app.settings.preview_bookmarks || {})
        if (b.length) all[cur.path] = b; else delete all[cur.path]
        app.grove.setSetting("preview_bookmarks", JSON.stringify(all))
        toast("info", at >= 0 ? "Bookmark removed" : "Bookmarked page " + (n + 1))
    }
    function crop() {
        const r = rectSel
        if (!r || !cur) return
        const d = docById(r.docId)
        const x0 = Math.min(r.rect[0], r.rect[2]), y0 = Math.min(r.rect[1], r.rect[3]), x1 = Math.max(r.rect[0], r.rect[2]), y1 = Math.max(r.rect[1], r.rect[3])
        if (d.kind === "image") changeImage(d, { crop: [Math.round(x0), Math.round(y0), Math.round(x1 - x0), Math.round(y1 - y0)] }, "Crop")
        else {
            const pages = d.pages.slice()
            pages[r.index] = Object.assign({}, pages[r.index], { crop: [x0, y0, x1, y1] })
            change(d, "Crop", { pages: pages })
        }
        rectSel = null
    }

    // ---------- copy, select ----------
    function copy() {
        if (textSel && textSel.text) { app.grove.copyText(textSel.text); toast("info", "Copied the text"); return }
        if (rectSel || (cur && cur.kind === "image")) {
            const d = rectSel ? docById(rectSel.docId) : cur
            const w = d.kind === "pdf"
                ? { kind: "pdf", path: d.pages[rectSel.index].src, password: passwordFor(d.pages[rectSel.index].src), page: d.pages[rectSel.index].page, pageW: d.pages[rectSel.index].w, rect: rectSel.rect }
                : { kind: "image", path: d.path, spec: imageSpec(d), rect: rectSel ? rectSel.rect : [0, 0, d.img.w, d.img.h] }
            const s = JSON.stringify(w)
            Async.call(app.grove, k => app.grove.pvCopyRegion(k, s), r => toast(r.error ? "error" : "info", r.error || "Copied the picture"))
        }
    }
    function selectAll() {
        if (!cur) return
        if (cur.kind === "image") { rectSel = { key: cur.id + ":img", docId: cur.id, rect: [0, 0, cur.img.w, cur.img.h] }; return }
        const p = cur.pages[currentPage]
        if (!p || p.blank || p.image) return
        const d = cur, i = currentPage
        Async.call(app.grove, k => app.grove.pvSelect(k, p.src, passwordFor(p.src), p.page, 0, 0, 0, 0, 3), r => { if (r.quads) textSel = { key: d.id + ":" + p.key, docId: d.id, index: i, quads: r.quads, text: r.text || "" } })
    }

    // ---------- highlights and notes ----------
    property var existing: ({})       // "docId:pageKey" -> the page's own annotations
    function loadAnnots(d) {
        if (!d || d.kind !== "pdf" || !d.info || !d.info.isPdf) return
        d.pages.forEach(p => {
            if (!p.src || p.blank) return
            const k0 = d.id + ":" + p.key
            if (existing[k0]) return
            Async.call(app.grove, k => app.grove.pvAnnots(k, p.src, passwordFor(p.src), p.page), r => {
                if (!Array.isArray(r)) return
                const m = Object.assign({}, existing); m[k0] = r; existing = m
            })
        })
    }
    onSidebarModeChanged: if (sidebarMode === "notes") loadAnnots(cur)
    onCurChanged: if (sidebarMode === "notes") loadAnnots(cur)
    function notesOf(d) {
        if (!d || d.kind !== "pdf") return []
        const out = []
        const names = { hl: "Highlight", ul: "Underline", so: "Strikethrough", note: "Note", text: "Text" }
        d.pages.forEach((p, i) => {
            (existing[d.id + ":" + p.key] || []).forEach(a => {
                if ((p.delAnnots || []).indexOf(a.i) >= 0) return
                if (["Highlight", "Underline", "StrikeOut", "Squiggly", "Text", "FreeText", "Ink", "Square", "Circle", "Line"].indexOf(a.type) < 0) return
                out.push({ kind: a.type === "Text" ? "Note" : a.type === "StrikeOut" ? "Strikethrough" : a.type === "FreeText" ? "Text" : a.type, index: i, own: true, ai: a.i, color: a.color, text: a.contents, author: a.author, y: a.r[1] })
            })
            ;(p.annots || []).forEach((o, n) => { if (names[o.t]) out.push({ kind: names[o.t], index: i, own: false, n: n, color: o.c, text: o.text || "", y: PvShapes.bounds(o)[1] }) })
        })
        return out
    }
    function gotoNote(n) { setCurrent(cur, n.index); docView.goTo(n.index, n.y || 0); if (!n.own) select(cur.id + ":" + cur.pages[n.index].key, n.n) }
    function deleteNote(n) {
        const pages = cur.pages.slice(), p = Object.assign({}, pages[n.index])
        if (n.own) p.delAnnots = (p.delAnnots || []).concat([n.ai])
        else { p.annots = p.annots.slice(); p.annots.splice(n.n, 1) }
        pages[n.index] = p
        change(cur, "Delete Note", { pages: pages })
    }

    // ---------- search ----------
    function search(text) {
        searchText = text
        if (!text) { searchHits = null; searchIndex = -1; return }
        const ds = docs.filter(d => d.kind === "pdf" && d.pages.length)
        searching = true
        searchHits = []
        let pending = 0
        const found = []
        const token = text
        for (const d of ds) {
            const srcs = [...new Set(d.pages.filter(p => p.src && !p.blank).map(p => p.src))]
            for (const src of srcs) {
                pending++
                Async.call(app.grove, k => app.grove.pvSearch(k, src, passwordFor(src), token), r => {
                    if (searchText !== token) return
                    if (Array.isArray(r)) for (const h of r) d.pages.forEach((p, i) => { if (p.src === src && p.page === h.page) found.push({ docId: d.id, index: i, src: src, page: h.page, quads: h.quads, line: h.line }) })
                    if (--pending === 0) {
                        const order = docs.map(x => x.id)
                        found.sort((a, b) => order.indexOf(a.docId) - order.indexOf(b.docId) || a.index - b.index || a.quads[0][1] - b.quads[0][1])
                        searching = false
                        searchHits = found
                        searchIndex = found.length ? 0 : -1
                        if (found.length) gotoHit(0)
                    }
                })
            }
        }
        if (!pending) { searching = false }
    }
    function searchHitsForCur() {}
    function gotoHit(i) {
        if (!searchHits || !searchHits.length) return
        searchIndex = (i + searchHits.length) % searchHits.length
        const h = searchHits[searchIndex]
        const d = docById(h.docId)
        setCurrent(d, h.index)
        Qt.callLater(() => docView.goTo(h.index, Math.max(0, Math.min(h.quads[0][1], h.quads[0][3]) - 60)))
    }
    function hitSnippet(h) {
        const esc = s => s.replace(/&/g, "&amp;").replace(/</g, "&lt;")
        const line = h.line || "", q = searchText.toLowerCase()
        const at = line.toLowerCase().indexOf(q)
        if (at < 0) return esc(line)
        return esc(line.substring(0, at)) + "<b>" + esc(line.substring(at, at + q.length)) + "</b>" + esc(line.substring(at + q.length))
    }

    // ---------- saving, exporting, printing ----------
    function pdfSpec(d, encrypt) {
        const pw0 = {}
        for (const x of docs) if (x.password) pw0[x.path] = x.password
        const pages = d.pages.map(p => {
            const fields = []
            for (const key in (d.fields || {})) {
                const [src, page, i] = key.split("|")
                if (src === p.src && Number(page) === p.page) fields.push({ i: Number(i), value: d.fields[key] })
            }
            return { src: p.src || "", page: p.page, blank: p.blank || null, image: p.image || null, rot: p.rot, crop: p.crop, fields: fields, delAnnots: p.delAnnots || [], annots: p.annots || [] }
        })
        const enc = encrypt !== undefined ? encrypt : (d.encryptedWith ? { user: d.encryptedWith, owner: d.encryptedWith } : null)
        return { primary: d.info && d.info.isPdf && !d.info.converted ? d.path : "", passwords: pw0, pages: pages, meta: d.meta || {}, encrypt: enc }
    }
    function imageSpec(d) {
        const i = d.img
        return { alpha: i.alpha, adjust: i.adjust, ops: i.ops, crop: i.crop, rotate: i.rotate, flipH: i.flipH, flipV: i.flipV, resize: i.resize, dpi: i.dpi }
    }
    function imageFormatOf(p) {
        const e = (p.match(/\.([a-z0-9]+)$/i) || ["", "png"])[1].toLowerCase()
        return { jpg: "jpeg", jpeg: "jpeg", png: "png", webp: "webp", tif: "tiff", tiff: "tiff", bmp: "bmp", gif: "gif" }[e] || "png"
    }
    property bool saving: false
    function save(d, then) {
        const x = docById((d || cur).id)
        if (!x) return
        if (!canEditDoc(x)) { toast("error", "This kind of document can't be changed here. Use File › Export as PDF… to make a PDF of it."); return }
        saving = true
        const done = r => {
            saving = false
            if (r.error) { toast("error", "Couldn't save: " + r.error); return }
            toast("info", r.replaced ? "Saved. The previous version is in the Trash." : "Saved as “" + nameOf(r.path) + "”")
            if (r.replaced) reload(x)
            if (then) then()
        }
        if (x.kind === "pdf") { const s = JSON.stringify(pdfSpec(x)); Async.call(app.grove, k => app.grove.pvSavePdf(k, x.path, s, "replace", ""), done) }
        else { const s = JSON.stringify(imageSpec(x)), f = imageFormatOf(x.path); Async.call(app.grove, k => app.grove.pvExportImage(k, x.path, s, "replace", "", f, 92), done) }
    }
    function duplicate() {
        const x = cur
        if (!x) return
        const done = r => { if (r.error) toast("error", r.error); else { toast("info", "Made “" + nameOf(r.path) + "”"); openFiles([r.path], true) } }
        if (x.kind === "pdf") { const s = JSON.stringify(pdfSpec(x)); Async.call(app.grove, k => app.grove.pvSavePdf(k, x.path, s, "copy", ""), done) }
        else { const s = JSON.stringify(imageSpec(x)), f = imageFormatOf(x.path); Async.call(app.grove, k => app.grove.pvExportImage(k, x.path, s, "copy", "", f, 92), done) }
    }
    function revert() { if (cur && cur.dirty) reload(cur) }
    function exportTo(d, fmt, quality, dpi, enc) {
        const x = docById(d.id)
        const ext = fmt === "jpeg" ? "jpg" : fmt
        const base = x.name.replace(/\.[^.]+$/, "")
        const dir = x.path.substring(0, x.path.lastIndexOf("/"))
        chooseFiles(fmt === "pdf" ? "Export as PDF" : "Export", { mode: "save", currentFolder: dir, currentName: base + (x.kind === "pdf" && fmt !== "pdf" ? " page " + (currentPage + 1) : "") + "." + ext }, paths => {
            if (!paths.length) return
            let dest = paths[0]
            if (!new RegExp("\\." + ext + "$", "i").test(dest)) dest += "." + ext
            const done = r => toast(r.error ? "error" : "info", r.error ? "Couldn't export: " + r.error : "Exported “" + nameOf(r.path) + "”")
            if (x.kind === "pdf" && fmt === "pdf") { const s = JSON.stringify(pdfSpec(x, enc || null)); Async.call(app.grove, k => app.grove.pvSavePdf(k, x.path, s, "to", dest), done) }
            else if (x.kind === "pdf") { const s = JSON.stringify(pdfSpec(x, null)), pg = currentPage; Async.call(app.grove, k => app.grove.pvExportPage(k, s, pg, dest, fmt, quality, dpi), done) }
            else { const s = JSON.stringify(imageSpec(x)); Async.call(app.grove, k => app.grove.pvExportImage(k, x.path, s, "to", dest, fmt, quality), done) }
        })
    }
    function exportPage() { if (isPdf) { exportSheet.open2(cur, false); exportSheet.format = "png" } }
    // Every open picture, with its edits, in one PDF.
    function picturesToPdf() {
        const items = docs.filter(d => d.kind === "image" && d.img).map(d => ({ path: d.path, spec: imageSpec(d) }))
        if (!items.length) return
        const dir = items[0].path.substring(0, items[0].path.lastIndexOf("/"))
        chooseFiles("Save Pictures as PDF", { mode: "save", currentFolder: dir, currentName: "Pictures.pdf" }, paths => {
            if (!paths.length) return
            const dest = /\.pdf$/i.test(paths[0]) ? paths[0] : paths[0] + ".pdf"
            const s = JSON.stringify(items)
            Async.call(app.grove, k => app.grove.pvImagesPdf(k, s, dest), r => toast(r.error ? "error" : "info", r.error || "Made “" + nameOf(r.path) + "”"))
        })
    }
    function printTo(d, printer, opts) {
        const x = docById(d.id)
        const job = { printer: printer, path: x.path, title: x.name, options: opts }
        if (x.kind === "pdf") job.pdf = pdfSpec(x, null)
        else job.image = imageSpec(x)
        const s = JSON.stringify(job)
        toast("info", "Printing “" + x.name + "”…")
        Async.call(app.grove, k => app.grove.pvPrint(k, s), r => toast(r.error ? "error" : "info", r.error ? "Couldn't print: " + r.error : "Sent to the printer"))
    }

    Component { id: chooserComp; FileChooser {} }
    function chooseFiles(title, opts, fn) {
        const req = Object.assign({ title: title, mode: "open" }, opts || {})
        const c = chooserComp.createObject(null, { app: app, request: req, onChosen: fn })
        c.show(); c.raise(); c.requestActivate()
        return c
    }
    function openDialog() {
        chooseFiles("Open in Preview", { mode: "open", multiple: true, currentFolder: cur ? cur.path.substring(0, cur.path.lastIndexOf("/")) : app.grove.home(),
                                          filters: [{ name: "All files", patterns: [{ mime: false, pattern: "*" }] },
                                                    { name: "PDFs and pictures", patterns: [{ mime: true, pattern: "application/pdf" }, { mime: true, pattern: "image/*" }] },
                                                    { name: "Documents (e-books, comics, DjVu, PostScript, Office, text)", patterns: ["epub", "mobi", "fb2", "xps", "cbz", "cbr", "cb7", "cbt", "djvu", "ps", "eps", "odt", "ods", "odp", "docx", "xlsx", "pptx", "doc", "xls", "ppt", "rtf", "txt", "md"].map(e => ({ mime: false, pattern: "*." + e })) }] },
                    paths => {
                        // As in Preview: a window that has files gets a new one for these.
                        if (docs.length && app.preview) app.preview(paths)
                        else openFiles(paths, true)
                    })
    }
    function toggleAdjustColor() { if (adjustColor.visible) adjustColor.close(); else if (cur && cur.kind === "image") adjustColor.open() }
    function showAdjustSize() { if (cur && cur.kind === "image") sizeSheet.open2(cur) }
    function askPassword(d) { passSheet.doc = d; passField.text = ""; passSheet.open(); passField.forceActiveFocus() }
    property var afterSave: null
    function askSave(d, then) { saveAsk.doc = d; afterSave = then; saveAsk.open() }

    // Closing with unsaved changes asks first.
    property bool closingOk: false
    onClosing: (close) => {
        if (closingOk) return
        const dirty = docs.filter(d => d.dirty)
        if (dirty.length && app.screenshot === "") { close.accepted = false; askSave(dirty[0], () => { closingOk = true; pw.close() }) }
    }

    Component.onCompleted: {
        Async.call(app.grove, k => app.grove.pvSignatures(k), r => { if (Array.isArray(r)) pw.signatures = r })
        if (initialPaths.length) openFiles(initialPaths, true)
        if (app.screenshot !== "") testTimer.start()
    }

    // ---------- menus ----------
    function k(act) { return Help.seqs(act)[0] || "" }
    readonly property var menuModel: {
        const d = cur, pdf = isPdf, img = !!d && d.kind === "image", ed = canEdit
        return [
            { title: "Preview", icon: "image-x-generic", items: [
                { id: "about", icon: "help-about", text: "About Grove Preview" },
                { separator: true },
                { id: "quit", icon: "application-exit", text: "Close Window" }
            ] },
            { title: "File", icon: "folder", items: [
                { id: "open", icon: "document-open", text: "Open…", shortcut: k("pv.open") },
                { id: "closedoc", icon: "document-close", text: "Close Document", enabled: !!d && docs.length > 1 },
                { id: "close", icon: "window-close", text: "Close Window", shortcut: k("pv.close") },
                { separator: true },
                { id: "save", icon: "document-save", text: "Save", shortcut: k("pv.save"), enabled: !!d && d.dirty && ed },
                { id: "duplicate", icon: "edit-copy", text: "Duplicate", shortcut: k("pv.duplicate"), enabled: !!d && ed },
                { id: "revert", icon: "document-revert", text: "Revert to Saved", enabled: !!d && d.dirty },
                { separator: true },
                { id: "export", icon: "document-export", text: "Export…", enabled: pw.canExport },
                { id: "exportpdf", icon: "application-pdf", text: "Export as PDF…", enabled: pw.canExport },
                { id: "picspdf", icon: "application-pdf", text: "Save All Pictures as One PDF…", enabled: docs.filter(x => x.kind === "image").length > 1 },
                { separator: true },
                { id: "print", icon: "document-print", text: "Print…", shortcut: k("pv.print"), enabled: !!d }
            ] },
            { title: "Edit", icon: "document-edit", items: [
                { id: "undo", icon: "edit-undo", text: d && (d.undo || []).length ? "Undo " + d.undo[d.undo.length - 1].label : "Undo", shortcut: k("pv.undo"), enabled: !!d && (d.undo || []).length > 0 },
                { id: "redo", icon: "edit-redo", text: d && (d.redo || []).length ? "Redo " + d.redo[d.redo.length - 1].label : "Redo", shortcut: k("pv.redo"), enabled: !!d && (d.redo || []).length > 0 },
                { separator: true },
                { id: "copy", icon: "edit-copy", text: "Copy", shortcut: k("pv.copy"), enabled: !!textSel || !!rectSel || img },
                { id: "delete", icon: "edit-delete", text: sel ? "Delete" : "Delete Page", shortcut: "Delete", enabled: !!sel || pdf },
                { id: "selectall", icon: "edit-select-all", text: "Select All", shortcut: k("pv.selectall"), enabled: !!d },
                { id: "deselect", icon: "edit-select-none", text: "Deselect All", enabled: !!textSel || !!rectSel || !!sel },
                { separator: true },
                { id: "insert", icon: "list-add", text: "Insert", enabled: pdf && ed, items: [
                    { id: "insertfile", icon: "document-import", text: "Page from File…" },
                    { id: "insertblank", icon: "document-new", text: "Blank Page" }
                ] },
                { separator: true },
                { id: "find", icon: "edit-find", text: "Find…", shortcut: k("pv.find"), enabled: pdf },
                { id: "findnext", icon: "go-down", text: "Find Next", shortcut: k("pv.findnext"), enabled: !!searchHits && searchHits.length > 0 },
                { id: "findprev", icon: "go-up", text: "Find Previous", shortcut: k("pv.findprev"), enabled: !!searchHits && searchHits.length > 0 }
            ] },
            { title: "View", icon: "view-visible", items: [
                { id: "sidebar", icon: "view-split-left-right", text: sidebarShown ? "Hide Sidebar" : "Show Sidebar", shortcut: k("pv.sidebar") },
                { id: "sb.thumbs", icon: "view-preview", text: "Thumbnails", checkable: true, checked: sidebarShown && sidebarMode === "thumbs", shortcut: k("pv.thumbs") },
                { id: "sb.toc", icon: "format-list-unordered", text: "Table of Contents", checkable: true, checked: sidebarShown && sidebarMode === "toc", enabled: pdf, shortcut: k("pv.toc") },
                { id: "sb.notes", icon: "draw-highlight", text: "Highlights and Notes", checkable: true, checked: sidebarShown && sidebarMode === "notes", enabled: pdf, shortcut: k("pv.notes") },
                { id: "sb.bookmarks", icon: "bookmarks", text: "Bookmarks", checkable: true, checked: sidebarShown && sidebarMode === "bookmarks", enabled: pdf, shortcut: k("pv.bookmarks") },
                { id: "contact", icon: "view-grid", text: "Contact Sheet", checkable: true, checked: docView.display === "contact", enabled: pdf, shortcut: k("pv.contact") },
                { separator: true },
                { id: "d.continuous", icon: "view-list-text", text: "Continuous Scroll", checkable: true, checked: docView.display === "continuous", enabled: pdf },
                { id: "d.single", icon: "view-pages-single", text: "Single Page", checkable: true, checked: docView.display === "single", enabled: pdf },
                { id: "d.two", icon: "view-pages-facing", text: "Two Pages", checkable: true, checked: docView.display === "two", enabled: pdf },
                { separator: true },
                { id: "actual", icon: "zoom-original", text: "Actual Size", shortcut: k("pv.actual"), enabled: !!d },
                { id: "fit", icon: "zoom-fit-best", text: "Zoom to Fit", shortcut: k("pv.fit"), enabled: !!d },
                { id: "fitwidth", icon: "zoom-fit-width", text: "Zoom to Width", enabled: pdf },
                { id: "zoomin", icon: "zoom-in", text: "Zoom In", shortcut: k("pv.zoomin"), enabled: !!d },
                { id: "zoomout", icon: "zoom-out", text: "Zoom Out", shortcut: k("pv.zoomout"), enabled: !!d },
                { separator: true },
                { id: "markup", icon: "draw-freehand", text: markupShown ? "Hide Markup Toolbar" : "Show Markup Toolbar", shortcut: k("pv.markup") },
                { id: "slideshow", icon: "media-playback-start", text: "Slideshow", shortcut: k("pv.slideshow"), enabled: !!d },
                { id: "fullscreen", icon: "view-fullscreen", text: pw.visibility === Window.FullScreen ? "Exit Full Screen" : "Enter Full Screen", shortcut: "F11" }
            ] },
            { title: "Go", icon: "go-next", items: [
                { id: "prev", icon: "go-up", text: "Previous Page", shortcut: k("pv.prevpage"), enabled: pdf && currentPage > 0 },
                { id: "next", icon: "go-down", text: "Next Page", shortcut: k("pv.nextpage"), enabled: pdf && currentPage < d.pages.length - 1 },
                { id: "first", icon: "go-top", text: "First Page", shortcut: "Home", enabled: pdf },
                { id: "last", icon: "go-bottom", text: "Last Page", shortcut: "End", enabled: pdf },
                { id: "goto", icon: "go-jump", text: "Go to Page…", shortcut: k("pv.goto"), enabled: pdf },
                { separator: true },
                { id: "prevdoc", icon: "go-previous", text: "Previous Document", shortcut: k("pv.prevdoc"), enabled: docs.length > 1 },
                { id: "nextdoc", icon: "go-next", text: "Next Document", shortcut: k("pv.nextdoc"), enabled: docs.length > 1 }
            ] },
            { title: "Tools", icon: "tools", items: [
                { id: "inspector", icon: "documentinfo", text: "Show Inspector", shortcut: k("pv.inspector"), enabled: !!d },
                { separator: true },
                { id: "adjustcolor", icon: "color-management", text: "Adjust Color…", shortcut: k("pv.adjustcolor"), enabled: img },
                { id: "adjustsize", icon: "transform-scale", text: "Adjust Size…", enabled: img },
                { id: "rotleft", icon: "object-rotate-left", text: "Rotate Left", shortcut: k("pv.rotleft"), enabled: ed },
                { id: "rotright", icon: "object-rotate-right", text: "Rotate Right", shortcut: k("pv.rotright"), enabled: ed },
                { id: "fliph", icon: "object-flip-horizontal", text: "Flip Horizontal", enabled: img },
                { id: "flipv", icon: "object-flip-vertical", text: "Flip Vertical", enabled: img },
                { id: "crop", icon: "transform-crop", text: "Crop", shortcut: k("pv.crop"), enabled: !!rectSel && ed },
                { separator: true },
                { id: "annotate", icon: "draw-freehand", text: "Annotate", enabled: ed, items: [
                    { id: "t.hl", icon: "draw-highlight", text: "Highlight Text", enabled: pdf, shortcut: k("pv.highlight") },
                    { id: "t.ul", icon: "format-text-underline", text: "Underline Text", enabled: pdf },
                    { id: "t.so", icon: "format-text-strikethrough", text: "Strike Through Text", enabled: pdf },
                    { separator: true },
                    { id: "t.rect", icon: "draw-rectangle", text: "Rectangle" },
                    { id: "t.ellipse", icon: "draw-ellipse", text: "Oval" },
                    { id: "t.line", icon: "draw-line", text: "Line" },
                    { id: "t.arrow", icon: "draw-arrow", text: "Arrow" },
                    { id: "t.text", icon: "draw-text", text: "Text" },
                    { id: "t.note", icon: "note", text: "Note", enabled: pdf },
                    { id: "t.sign", icon: "document-sign", text: "Signature" }
                ] },
                { id: "signatures", icon: "document-sign", text: "Signatures…" },
                { id: "bookmark", icon: "bookmark-new", text: pdf && (d.bookmarks || []).indexOf(currentPage) >= 0 ? "Remove Bookmark" : "Add Bookmark", shortcut: k("pv.bookmark"), enabled: pdf },
                { separator: true },
                { id: "t.textsel", icon: "edit-select-text", text: "Text Selection", checkable: true, checked: tool === "textsel" },
                { id: "t.rectsel", icon: "select-rectangular", text: "Rectangular Selection", checkable: true, checked: tool === "rectsel" },
                { id: "t.alpha", icon: "draw-eraser", text: "Instant Alpha", checkable: true, checked: tool === "alpha", enabled: img }
            ] },
            { title: "Help", icon: "help-contents", items: [
                { id: "help", icon: "help-contents", text: "Grove Preview Help", shortcut: k("help") },
                { id: "shortcuts", icon: "preferences-desktop-keyboard", text: "Keyboard Shortcuts" }
            ] }
        ]
    }
    function menuAction(id) {
        const d = cur
        if (id.indexOf("sb.") === 0) { sidebarMode = id.substring(3); sidebarShown = true; if (docView.display === "contact") docView.display = "continuous"; return }
        if (id.indexOf("d.") === 0) { docView.display = id.substring(2); if (id === "d.single" || id === "d.two") docView.fit = "page"; return }
        if (id.indexOf("t.") === 0) { setTool(id.substring(2)); if (id !== "t.textsel" && id !== "t.rectsel") markupShown = true; return }
        switch (id) {
        case "about": aboutSheet.open(); break
        case "quit": case "close": pw.close(); break
        case "open": openDialog(); break
        case "closedoc": closeDoc(d); break
        case "save": save(d); break
        case "duplicate": duplicate(); break
        case "revert": revert(); break
        case "export": exportSheet.open2(d, false); break
        case "exportpdf": exportSheet.open2(d, true); break
        case "picspdf": picturesToPdf(); break
        case "print": printSheet.open2(d); break
        case "undo": undo(); break
        case "redo": redo(); break
        case "copy": copy(); break
        case "delete": if (!deleteSelectedOp()) deletePages(); break
        case "selectall": selectAll(); break
        case "deselect": textSel = null; rectSel = null; sel = null; break
        case "insertfile": insertFromFile(); break
        case "insertblank": insertBlank(); break
        case "find": searchField.forceActiveFocus(); searchField.selectAll(); break
        case "findnext": gotoHit(searchIndex + 1); break
        case "findprev": gotoHit(searchIndex - 1); break
        case "sidebar": sidebarShown = !sidebarShown; break
        case "contact": docView.display = docView.display === "contact" ? "continuous" : "contact"; break
        case "actual": if (isPdf) docView.fit = "actual"; else imageView.fit = "actual"; break
        case "fit": if (isPdf) docView.fit = "page"; else imageView.fit = "fit"; break
        case "fitwidth": docView.fit = "width"; break
        case "zoomin": if (isPdf) docView.zoomBy(1.25); else imageView.zoomBy(1.25); break
        case "zoomout": if (isPdf) docView.zoomBy(0.8); else imageView.zoomBy(0.8); break
        case "markup": markupShown = !markupShown; break
        case "slideshow": slideshow.start(); break
        case "fullscreen": pw.visibility === Window.FullScreen ? pw.showNormal() : pw.showFullScreen(); break
        case "prev": goPage(currentPage - 1); break
        case "next": goPage(currentPage + 1); break
        case "first": goPage(0); break
        case "last": goPage(d.pages.length - 1); break
        case "goto": gotoSheet.open(); gotoField.text = String(currentPage + 1); gotoField.selectAll(); gotoField.forceActiveFocus(); break
        case "prevdoc": case "nextdoc": { const i = docs.findIndex(x => x.id === d.id); const n = docs[(i + (id === "nextdoc" ? 1 : -1) + docs.length) % docs.length]; setCurrent(n, pageOf[n.id] || 0); break }
        case "inspector": inspector.visible ? inspector.close() : inspector.open(); break
        case "adjustcolor": toggleAdjustColor(); break
        case "adjustsize": showAdjustSize(); break
        case "rotleft": rotate(-90); break
        case "rotright": rotate(90); break
        case "fliph": flip(true); break
        case "flipv": flip(false); break
        case "crop": crop(); break
        case "signatures": showSignatures(); break
        case "bookmark": toggleBookmark(); break
        case "help": helpWin.query = "Preview"; helpWin.show(); helpWin.raise(); helpWin.requestActivate(); break
        case "shortcuts": helpWin.query = "Preview"; helpWin.show(); helpWin.raise(); break
        }
    }
    GlobalMenuBar { id: globalMenu; window: pw; model: pw.menuModel; onTriggered: id => pw.menuAction(id) }
    onVisibleChanged: if (visible) Qt.callLater(globalMenu.attach)
    AppMenu {
        iconUrl: (n, px) => Theme.iconUrl(n) + "&exact=1"
        id: appMenu
        model: pw.menuModel
        onTriggered: id => pw.menuAction(id)
        accent: Theme.accent
        dark: Theme.dark
        popupType: Popup.Item
        property real closedAt: 0
        onClosed: closedAt = Date.now()
    }
    HelpWindow { id: helpWin; app: pw.app }

    // ---------- shortcuts ----------
    readonly property bool modalOpen: exportSheet.visible || printSheet.visible || sigSheet.visible || sizeSheet.visible || passSheet.visible || saveAsk.visible || gotoSheet.visible || aboutSheet.visible
    readonly property bool typing: activeFocusItem && (activeFocusItem instanceof TextInput || activeFocusItem instanceof TextEdit)
    component Key: Shortcut { property string act: ""; sequences: Help.seqs(act); context: Qt.WindowShortcut; enabled: !pw.modalOpen && sequences.length > 0 }
    Key { act: "pv.open"; onActivated: pw.openDialog() }
    Key { act: "pv.close"; onActivated: pw.close() }
    Key { act: "pv.save"; onActivated: pw.save(pw.cur) }
    Key { act: "pv.duplicate"; onActivated: pw.duplicate() }
    Key { act: "pv.print"; onActivated: if (pw.cur) printSheet.open2(pw.cur) }
    Key { act: "pv.undo"; enabled: !pw.typing && !pw.modalOpen; onActivated: pw.undo() }
    Key { act: "pv.redo"; enabled: !pw.typing && !pw.modalOpen; onActivated: pw.redo() }
    Key { act: "pv.copy"; enabled: !pw.typing && !pw.modalOpen; onActivated: pw.copy() }
    Key { act: "pv.selectall"; enabled: !pw.typing && !pw.modalOpen; onActivated: pw.selectAll() }
    Key { act: "pv.find"; onActivated: pw.menuAction("find") }
    Key { act: "pv.findnext"; onActivated: pw.gotoHit(pw.searchIndex + 1) }
    Key { act: "pv.findprev"; onActivated: pw.gotoHit(pw.searchIndex - 1) }
    Key { act: "pv.sidebar"; onActivated: pw.sidebarShown = !pw.sidebarShown }
    Key { act: "pv.thumbs"; onActivated: pw.menuAction("sb.thumbs") }
    Key { act: "pv.toc"; onActivated: pw.menuAction("sb.toc") }
    Key { act: "pv.notes"; onActivated: pw.menuAction("sb.notes") }
    Key { act: "pv.bookmarks"; onActivated: pw.menuAction("sb.bookmarks") }
    Key { act: "pv.contact"; onActivated: pw.menuAction("contact") }
    Key { act: "pv.actual"; onActivated: pw.menuAction("actual") }
    Key { act: "pv.fit"; onActivated: pw.menuAction("fit") }
    Key { act: "pv.zoomin"; onActivated: pw.menuAction("zoomin") }
    Key { act: "pv.zoomout"; onActivated: pw.menuAction("zoomout") }
    Key { act: "pv.markup"; onActivated: pw.markupShown = !pw.markupShown }
    Key { act: "pv.slideshow"; onActivated: slideshow.start() }
    Key { act: "pv.goto"; onActivated: pw.menuAction("goto") }
    Key { act: "pv.prevpage"; enabled: !pw.typing && !pw.modalOpen; onActivated: pw.goPage(pw.currentPage - 1) }
    Key { act: "pv.nextpage"; enabled: !pw.typing && !pw.modalOpen; onActivated: pw.goPage(pw.currentPage + 1) }
    Key { act: "pv.prevdoc"; onActivated: pw.menuAction("prevdoc") }
    Key { act: "pv.nextdoc"; onActivated: pw.menuAction("nextdoc") }
    Key { act: "pv.inspector"; onActivated: pw.menuAction("inspector") }
    Key { act: "pv.adjustcolor"; onActivated: pw.toggleAdjustColor() }
    Key { act: "pv.rotleft"; onActivated: pw.rotate(-90) }
    Key { act: "pv.rotright"; onActivated: pw.rotate(90) }
    Key { act: "pv.crop"; onActivated: pw.crop() }
    Key { act: "pv.bookmark"; onActivated: pw.toggleBookmark() }
    Key { act: "pv.highlight"; onActivated: pw.setTool(pw.tool === "hl" ? "textsel" : "hl") }
    Key { act: "help"; onActivated: pw.menuAction("help") }
    Shortcut { sequence: "F11"; context: Qt.WindowShortcut; onActivated: pw.menuAction("fullscreen") }
    Shortcut { sequences: ["Delete", "Backspace"]; context: Qt.WindowShortcut; enabled: !pw.typing && !pw.modalOpen; onActivated: { if (!pw.deleteSelectedOp() && pw.selPages.list.length) pw.deletePages() } }
    Shortcut { sequence: "Escape"; context: Qt.WindowShortcut; enabled: !pw.modalOpen && !pw.typing; onActivated: {
        if (pw.searchHits !== null) { searchField.text = ""; pw.search("") }
        else if (pw.tool !== "textsel") pw.tool = "textsel"
        else { pw.sel = null; pw.textSel = null; pw.rectSel = null }
        if (pw.visibility === Window.FullScreen) pw.showNormal()
    } }
    Shortcut { sequence: "Home"; context: Qt.WindowShortcut; enabled: !pw.typing && pw.isPdf; onActivated: pw.goPage(0) }
    Shortcut { sequence: "End"; context: Qt.WindowShortcut; enabled: !pw.typing && pw.isPdf; onActivated: pw.goPage(pw.cur.pages.length - 1) }
    Shortcut { sequence: "PgDown"; context: Qt.WindowShortcut; enabled: !pw.typing && pw.isPdf; onActivated: docView.pageDown() }
    Shortcut { sequence: "PgUp"; context: Qt.WindowShortcut; enabled: !pw.typing && pw.isPdf; onActivated: docView.pageUp() }
    Shortcut { sequence: "Space"; context: Qt.WindowShortcut; enabled: !pw.typing && pw.isPdf; onActivated: docView.pageDown() }

    // ---------- layout ----------
    Item {
        id: root
        anchors.fill: parent
        transform: Translate { x: 0.001; y: 0.001 }

        // Toolbar
        Item {
            id: toolbar
            width: parent.width
            height: Theme.toolbarHeight
            MouseArea {
                anchors.fill: parent
                onPressed: pw.startSystemMove()
                onDoubleClicked: pw.visibility === Window.Maximized ? pw.showNormal() : pw.showMaximized()
            }
            Row {
                id: leftTools
                x: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                GButton { id: sbBtn; iconName: "view-split-left-right|sidebar-show"; checked: pw.sidebarShown; tip: "Show or hide the sidebar (" + pw.keyFor("pv.sidebar") + "). Right-click to choose what it shows"
                    onClicked: pw.sidebarShown = !pw.sidebarShown
                    TapHandler { acceptedButtons: Qt.RightButton; onTapped: sbMenu.popup(sbBtn, 0, sbBtn.height) } }
                GButton { id: sbModeBtn; iconName: "go-down"; iconSize: 10; width: 18; tip: "Sidebar: thumbnails, table of contents, highlights and notes, bookmarks, contact sheet"; onClicked: sbMenu.popup(sbModeBtn, 0, sbModeBtn.height) }
                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    leftPadding: 6
                    GLabel { text: pw.cur ? pw.cur.name : "Grove Preview"; font.bold: true; width: Math.min(implicitWidth, Math.max(80, toolbar.width - leftTools.x - rightTools.width - 160)); elide: Text.ElideMiddle }
                    GLabel {
                        text: !pw.cur ? "" : (pw.cur.dirty ? "Edited · " : "") + (pw.cur.kind === "pdf" ? (pw.cur.pages.length ? "Page " + (pw.currentPage + 1) + " of " + pw.cur.pages.length : "") : (pw.cur.img ? pw.cur.img.w + " × " + pw.cur.img.h : ""))
                        color: Theme.fgSecondary; font.pixelSize: Theme.small
                    }
                }
            }
            Row {
                id: rightTools
                anchors.right: winControls.visible ? winControls.left : parent.right
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4
                GButton { iconName: "documentinfo|dialog-information"; checked: inspector.visible; tip: "Inspector: about this file (" + pw.keyFor("pv.inspector") + ")"; onClicked: pw.menuAction("inspector") }
                GButton { iconName: "zoom-out"; visible: !pw.cur || pw.cur.kind !== "text"; tip: "Zoom out (" + pw.keyFor("pv.zoomout") + ")"; enabled: !!pw.cur; onClicked: pw.menuAction("zoomout") }
                GLabel {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 44; horizontalAlignment: Text.AlignHCenter
                    text: !pw.cur || pw.cur.kind === "text" ? "" : (pw.isPdf ? docView.percent : imageView.percent) + "%"
                    color: Theme.fgSecondary; font.pixelSize: Theme.small
                    MouseArea { anchors.fill: parent; hoverEnabled: true; onClicked: pw.menuAction("fit"); GTip { visible: parent.containsMouse; text: "Click to zoom to fit (" + pw.keyFor("pv.fit") + ")" } }
                }
                GButton { iconName: "zoom-in"; visible: !pw.cur || pw.cur.kind !== "text"; tip: "Zoom in (" + pw.keyFor("pv.zoomin") + ")"; enabled: !!pw.cur; onClicked: pw.menuAction("zoomin") }
                GButton { iconName: "object-rotate-left"; tip: "Rotate left (" + pw.keyFor("pv.rotleft") + "); right-click to rotate right"; enabled: pw.canEdit; onClicked: pw.rotate(-90)
                    TapHandler { acceptedButtons: Qt.RightButton; onTapped: pw.rotate(90) } }
                GButton { iconName: "draw-highlight"; visible: pw.isPdf; checked: pw.tool === "hl"; enabled: pw.canEdit; tip: "Highlight: select text to highlight it (" + pw.keyFor("pv.highlight") + ")"; onClicked: pw.setTool(pw.tool === "hl" ? "textsel" : "hl") }
                GButton { iconName: "draw-freehand|document-edit"; checked: pw.markupShown; enabled: pw.canEdit; tip: "Show the Markup toolbar: draw, shapes, text, signatures… (" + pw.keyFor("pv.markup") + ")"; onClicked: pw.markupShown = !pw.markupShown }
                // The menus, as grove's ☰ (same icon, look and menu).
                GButton {
                    id: menuBtn
                    iconName: "view-more-horizontal-symbolic|application-menu|open-menu-symbolic|overflow-menu"
                    tip: "Menus: Preview, File, Edit, View, Go, Tools, Help (also in the menu bar at the top of the screen)"
                    // A second click closes it: the press outside already closed it.
                    onClicked: { if (appMenu.visible || Date.now() - appMenu.closedAt < 250) appMenu.close(); else appMenu.openBelow(menuBtn) }
                }
                GField {
                    id: searchField
                    width: Math.round(190 * Theme.pt)
                    visible: pw.isPdf || pw.docs.some(d => d.kind === "pdf")
                    iconName: "edit-find"
                    clearable: true
                    placeholderText: "Search"
                    tip: "Find words in the documents (" + pw.keyFor("pv.find") + "); Return: next match"
                    onAccepted: { if (text === pw.searchText && pw.searchHits && pw.searchHits.length) pw.gotoHit(pw.searchIndex + 1); else pw.search(text) }
                    onTextChanged: { if (text === "") pw.search(""); else searchDelay.restart() }
                    Keys.onEscapePressed: { text = ""; pw.search(""); docView.forceActiveFocus() }
                    Timer { id: searchDelay; interval: 450; onTriggered: if (searchField.text.length >= 2 && searchField.text !== pw.searchText) pw.search(searchField.text) }
                }
            }
            WindowControls {
                id: winControls
                window: pw
                visible: pw.clientDecorations
                anchors.right: parent.right
                anchors.rightMargin: 0
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        GMenu {
            id: sbMenu
            GMenuItem { text: pw.sidebarShown ? "Hide Sidebar" : "Show Sidebar"; iconName: "view-split-left-right"; shortcutText: pw.keyFor("pv.sidebar"); onTriggered: pw.sidebarShown = !pw.sidebarShown }
            GMenuSeparator {}
            GMenuItem { text: "Thumbnails"; iconName: "view-preview"; checkable: true; checked: pw.sidebarMode === "thumbs"; shortcutText: pw.keyFor("pv.thumbs"); onTriggered: pw.menuAction("sb.thumbs") }
            GMenuItem { text: "Table of Contents"; iconName: "format-list-unordered"; checkable: true; checked: pw.sidebarMode === "toc"; enabled: pw.isPdf; shortcutText: pw.keyFor("pv.toc"); onTriggered: pw.menuAction("sb.toc") }
            GMenuItem { text: "Highlights and Notes"; iconName: "draw-highlight"; checkable: true; checked: pw.sidebarMode === "notes"; enabled: pw.isPdf; shortcutText: pw.keyFor("pv.notes"); onTriggered: pw.menuAction("sb.notes") }
            GMenuItem { text: "Bookmarks"; iconName: "bookmarks"; checkable: true; checked: pw.sidebarMode === "bookmarks"; enabled: pw.isPdf; shortcutText: pw.keyFor("pv.bookmarks"); onTriggered: pw.menuAction("sb.bookmarks") }
            GMenuItem { text: "Contact Sheet"; iconName: "view-grid"; checkable: true; checked: docView.display === "contact"; enabled: pw.isPdf; shortcutText: pw.keyFor("pv.contact"); onTriggered: pw.menuAction("contact") }
        }

        // Markup toolbar
        PvMarkupBar {
            id: markupBar
            win: pw
            anchors.top: toolbar.bottom
            width: parent.width
            height: pw.markupShown && pw.canEdit ? implicitHeight : 0
            visible: height > 0
            clip: true
            Behavior on height { NumberAnimation { duration: Theme.reduceMotion ? 0 : 140; easing.type: Easing.OutCubic } }
            Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Theme.separator }
        }
        Rectangle { id: topLine; anchors.top: markupBar.bottom; width: parent.width; height: 1; color: Theme.separator }

        PvSidebar {
            id: sidebar
            win: pw
            anchors.top: topLine.bottom
            anchors.bottom: parent.bottom
            width: pw.sidebarShown ? Math.round(Math.max(150, Math.min(420, pw.app.settings.preview_sidebar_width || 190)) ) : 0
            visible: width > 0
        }
        MouseArea {
            visible: pw.sidebarShown
            x: sidebar.width - 3; width: 6
            anchors.top: sidebar.top; anchors.bottom: parent.bottom
            cursorShape: Qt.SplitHCursor
            property real sx: 0
            property real sw: 0
            onPressed: (m) => { sx = mapToItem(root, m.x, 0).x; sw = sidebar.width }
            onPositionChanged: (m) => { if (pressed) sidebar.width = Math.max(150, Math.min(420, sw + mapToItem(root, m.x, 0).x - sx)) }
            onReleased: pw.app.grove.setSetting("preview_sidebar_width", String(Math.round(sidebar.width)))
        }

        Item {
            id: body
            anchors.top: topLine.bottom
            anchors.left: sidebar.right
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            clip: true
            PvDocView { id: docView; anchors.fill: parent; win: pw; doc: pw.isPdf ? pw.cur : null; visible: pw.isPdf && !pw.cur.locked }
            PvImageView { id: imageView; anchors.fill: parent; win: pw; doc: pw.cur && pw.cur.kind === "image" && pw.cur.img ? pw.cur : null; visible: !!doc }
            DocView {
                id: textView
                anchors.fill: parent
                visible: !!pw.cur && pw.cur.kind === "text" && !!pw.cur.info
                active: visible
                path: visible ? pw.cur.path : ""
                mode: visible ? pw.cur.info.mode : ""
            }
            BusyDots { anchors.centerIn: parent; running: !!pw.cur && pw.cur.loading }
            Column {
                anchors.centerIn: parent
                spacing: 10
                visible: !pw.cur
                GIcon { anchors.horizontalCenter: parent.horizontalCenter; name: "image-x-generic"; size: 64 }
                GLabel { anchors.horizontalCenter: parent.horizontalCenter; text: "Open a PDF or picture (" + pw.keyFor("pv.open") + "), or drop files here."; color: Theme.fgSecondary }
                PushButton { anchors.horizontalCenter: parent.horizontalCenter; text: "Open…"; primary: true; tip: "Choose PDFs and pictures to open"; onClicked: pw.openDialog() }
            }
            Column {
                anchors.centerIn: parent
                spacing: 10
                visible: !!pw.cur && !!pw.cur.locked
                GIcon { anchors.horizontalCenter: parent.horizontalCenter; name: "object-locked|lock"; size: 48 }
                GLabel { anchors.horizontalCenter: parent.horizontalCenter; text: "This document is locked with a password."; color: Theme.fgSecondary }
                PushButton { anchors.horizontalCenter: parent.horizontalCenter; text: "Unlock…"; primary: true; tip: "Type the password to open it"; onClicked: pw.askPassword(pw.cur) }
            }
            DropArea {
                anchors.fill: parent
                onDropped: (d) => { if (d.hasUrls) { pw.openFiles(d.urls.map(u => decodeURIComponent(String(u).replace("file://", ""))), true); d.accept() } }
            }
            // Text selected: a small bar to copy it (or highlight it).
            Rectangle {
                visible: !!pw.textSel && pw.textSel.text !== "" && pw.tool === "textsel"
                anchors.bottom: parent.bottom; anchors.bottomMargin: 18
                anchors.horizontalCenter: parent.horizontalCenter
                width: selRow.implicitWidth + 16; height: Math.round(32 * Theme.pt)
                radius: height / 2
                color: Theme.glass; border.color: Theme.glassEdge
                Row {
                    id: selRow
                    anchors.centerIn: parent
                    spacing: 4
                    GButton { text: "Copy"; tip: "Copy the selected text (" + pw.keyFor("pv.copy") + ")"; onClicked: pw.copy() }
                    GButton { text: "Highlight"; visible: pw.canEdit; tip: "Highlight the selected text"; onClicked: pw.highlightSelection("hl") }
                    GButton { text: "Underline"; visible: pw.canEdit; tip: "Underline the selected text"; onClicked: pw.highlightSelection("ul") }
                    GButton { text: "Strike"; visible: pw.canEdit; tip: "Strike through the selected text"; onClicked: pw.highlightSelection("so") }
                }
            }
            Toast { id: toastBox; anchors.horizontalCenter: parent.horizontalCenter; anchors.bottom: parent.bottom; anchors.bottomMargin: 64 }
        }

        // Window edge resizing for grove's own decorations (the shared
        // ChromeResizeEdges, as every app).
        ChromeResizeEdges { anchors.fill: parent; z: 1000; window: pw; enabled: pw.clientDecorations; visible: pw.clientDecorations && pw.visibility !== Window.Maximized && pw.visibility !== Window.FullScreen }
    }
    function highlightSelection(t) {
        const s = textSel
        if (!s) return
        const d = docById(s.docId)
        setPageOps(d, s.index, (d.pages[s.index].annots || []).concat([{ t: t, c: "" + hlColor, quads: s.quads, text: s.text }]), t === "hl" ? "Highlight" : t === "ul" ? "Underline" : "Strikethrough")
        textSel = null
    }

    // ---------- floating panels and sheets ----------
    PvAdjustColor { id: adjustColor; win: pw; x: pw.width - width - 20; y: Theme.toolbarHeight + markupBar.height + 12 }
    PvInspector { id: inspector; win: pw; x: pw.width - width - 20; y: Theme.toolbarHeight + markupBar.height + 12 }
    PvAdjustSize { id: sizeSheet; win: pw }
    PvExportSheet { id: exportSheet; win: pw }
    PvPrintSheet { id: printSheet; win: pw }
    PvSignatureSheet { id: sigSheet; win: pw }
    Sheet {
        id: passSheet
        property var doc: null
        title: "Password"
        message: doc ? "“" + doc.name + "” is locked. Type its password to open it." : ""
        GField { id: passField; width: parent.width; echoMode: TextInput.Password; placeholderText: "Password"; tip: "The document's password"; onAccepted: unlockBtn.clicked() }
        buttons: [
            PushButton { text: "Cancel"; tip: "Leave it locked"; onClicked: passSheet.close() },
            PushButton { id: unlockBtn; text: "Unlock"; primary: true; tip: "Open it with this password"; onClicked: { const d = passSheet.doc; passSheet.close(); pw.load(pw.docById(d.id), passField.text) } }
        ]
    }
    Sheet {
        id: saveAsk
        property var doc: null
        title: doc ? "Save the changes to “" + doc.name + "”?" : ""
        message: "If you don't save, your changes are lost."
        buttons: [
            PushButton { text: "Don't Save"; destructive: true; tip: "Throw the changes away"; onClicked: { saveAsk.close(); const d = pw.docById(saveAsk.doc.id); if (d) pw.update(d, { dirty: false }); if (pw.afterSave) pw.afterSave() } },
            PushButton { text: "Cancel"; tip: "Keep it open"; onClicked: saveAsk.close() },
            PushButton { text: "Save"; primary: true; tip: "Save, then go on"; onClicked: { saveAsk.close(); pw.save(saveAsk.doc, () => { if (pw.afterSave) pw.afterSave() }) } }
        ]
    }
    Sheet {
        id: gotoSheet
        title: "Go to Page"
        message: pw.isPdf ? "A page from 1 to " + pw.cur.pages.length + "." : ""
        GField {
            id: gotoField
            width: parent.width
            inputMethodHints: Qt.ImhDigitsOnly
            tip: "The page number"
            onAccepted: gotoBtn.clicked()
        }
        buttons: [
            PushButton { text: "Cancel"; tip: "Stay here"; onClicked: gotoSheet.close() },
            PushButton { id: gotoBtn; text: "Go"; primary: true; tip: "Go to that page"; onClicked: { gotoSheet.close(); pw.goPage(Number(gotoField.text) - 1) } }
        ]
    }
    Sheet {
        id: aboutSheet
        title: "Grove Preview"
        message: "Version " + Qt.application.version + "\nView and change PDFs and pictures: Markup, signatures, forms, pages, colours, sizes, export and print."
        buttons: [ PushButton { text: "OK"; primary: true; tip: "Close"; onClicked: aboutSheet.close() } ]
    }
    // A note's text.
    Popup {
        id: notePop
        anchors.centerIn: Overlay.overlay
        width: 300
        padding: 12; topPadding: 12.001; leftPadding: 12.001
        background: GlassPanel { tint: "#fff7c2"; radius: 8 }
        onClosed: {
            if (!pw.noteLayer || pw.noteIndex < 0) return
            const o = pw.noteLayer.ops[pw.noteIndex]
            if (o && o.text !== noteText.text) pw.noteLayer.replaceOp(pw.noteIndex, Object.assign({}, o, { text: noteText.text }), "Edit Note")
            pw.noteLayer = null
        }
        contentItem: Column {
            spacing: 8
            GLabel { text: "Note"; font.bold: true; color: "#5a4a00" }
            TextArea { id: noteText; width: parent.width; height: 120; wrapMode: TextEdit.Wrap; color: "#222"; placeholderText: "Type your note"; background: Rectangle { color: "#fffbe0"; radius: 4 } }
            Row { anchors.right: parent.right; GButton { text: "Done"; flat: false; capsule: true; tip: "Keep the note"; onClicked: notePop.close() } }
        }
    }
    PvSlideshow { id: slideshow; win: pw }

    // ---------- tests: grove --preview FILE --screenshot out.png ----------
    // GROVE_TEST steps: markup, sidebar=toc|notes|bookmarks, contact,
    // search=WORD, page=N, demo (some Markup), adjust, select, sleep=MS.
    Timer { id: testTimer; interval: Number(pw.app.grove.env("GROVE_SHOT_DELAY") || 2500); onTriggered: pw.runSteps((pw.app.grove.env("GROVE_TEST") || "").split(";").filter(s => s)) }
    Timer { id: stepTimer; property var rest: []; onTriggered: pw.runSteps(rest) }
    Timer { id: shotTimer; interval: Number(pw.app.grove.env("GROVE_SHOT_DELAY2") || 1500); onTriggered: { pw.app.grove.grabWindows(pw.app.screenshot); Qt.quit() } }
    function runSteps(steps) {
        for (let i = 0; i < steps.length; i++) {
            if (steps[i].indexOf("sleep=") === 0) { stepTimer.rest = steps.slice(i + 1); stepTimer.interval = Number(steps[i].substring(6)); stepTimer.start(); return }
            testStep(steps[i])
        }
        shotTimer.start()
    }
    function testStep(s) {
        const [cmd, arg] = s.split("=")
        if (cmd === "markup") markupShown = true
        else if (cmd === "sidebar") { sidebarMode = arg; sidebarShown = true }
        else if (cmd === "nosidebar") sidebarShown = false
        else if (cmd === "contact") docView.display = "contact"
        else if (cmd === "two") { docView.display = "two"; docView.fit = "page" }
        else if (cmd === "search") { searchField.text = arg; search(arg) }
        else if (cmd === "page") goPage(Number(arg) - 1)
        else if (cmd === "doc") { const d = docs[Number(arg)]; if (d) setCurrent(d, 0) }
        else if (cmd === "inspector") inspector.open()
        else if (cmd === "adjust") adjustColor.open()
        else if (cmd === "adjustset") { const [kk, v] = arg.split(":"); const a = Object.assign({}, cur.img.adjust); a[kk] = Number(v); changeImage(cur, { adjust: a }, "Adjust Color") }
        else if (cmd === "select") selectAll()
        else if (cmd === "rotate") rotate(90)
        else if (cmd === "print") printSheet.open2(cur)
        else if (cmd === "export") exportSheet.open2(cur, false)
        else if (cmd === "sign") sigSheet.open2()
        else if (cmd === "save") save(cur)
        else if (cmd === "demo") {
            if (cur.kind === "pdf") {
                const p = currentPage
                setPageOps(cur, p, [
                    { t: "pen", c: "#ff3b30", w: 2.5, a: 1, p: [80, 160, 120, 130, 170, 175, 220, 140, 270, 170] },
                    { t: "rect", c: "#007aff", w: 2, x1: 320, y1: 110, x2: 520, y2: 190 },
                    { t: "ellipse", c: "#34c759", w: 2, fill: "#d8f5df", x1: 80, y1: 230, x2: 220, y2: 300 },
                    { t: "arrow", c: "#ff9500", w: 2.5, x1: 250, y1: 300, x2: 330, y2: 240 },
                    { t: "star", c: "#af52de", w: 2, x1: 400, y1: 230, x2: 470, y2: 300, p: PvShapes.outline({ t: "star", x1: 400, y1: 230, x2: 470, y2: 300 }) },
                    { t: "text", c: "#000000", s: 16, font: "Helvetica", x1: 80, y1: 330, x2: 400, y2: 360, text: "Markup in Grove Preview" },
                    { t: "note", c: "#ffcc00", x1: 500, y1: 330, text: "A sticky note" }
                ], "Demo")
            } else {
                const w = cur.img.w, h = cur.img.h, u = Math.max(1, Math.max(w, h) / 1000)
                changeImage(cur, { ops: [
                    { t: "arrow", c: "#ff3b30", w: 4 * u, x1: w * 0.15, y1: h * 0.8, x2: w * 0.4, y2: h * 0.55 },
                    { t: "ellipse", c: "#ffcc00", w: 4 * u, x1: w * 0.42, y1: h * 0.3, x2: w * 0.7, y2: h * 0.6 },
                    { t: "text", c: "#ffffff", s: 36 * u, font: "Helvetica", b: true, x1: w * 0.08, y1: h * 0.08, x2: w * 0.9, y2: h * 0.2, text: "Grove Preview" }
                ] }, "Demo")
            }
        }
    }
}

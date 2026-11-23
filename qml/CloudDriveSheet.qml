import QtQuick
import QtQuick.Controls.Basic

// Add Cloud Drive: a guided setup for any kind of drive rclone knows
// (Google Drive, OneDrive, Dropbox, iCloud, S3, SFTP, WebDAV… 70+).
// 1. pick the kind, 2. name it, 3. answer rclone's own questions one at a
// time (signing in opens the browser when the service needs it),
// 4. open it. Optional questions can be skipped with their defaults.
Sheet {
    id: cd
    property var app
    property var win
    property string page: "pick"       // pick, name, ask, done
    property var providers: []
    property var provider: null
    property string driveName: ""
    property var step: ({})             // {state, option, error}
    property bool busy: false
    property bool waitingLong: false
    property string error: ""
    property bool created: false
    // What's been answered (for guides: S3's provider...).
    property var answers: ({})
    RcloneGuides { id: guides }
    readonly property var guide: page === "ask" && provider ? guides.guide(provider.name, (step.option || {}).Name, answers) : null
    readonly property var signInGuide: busy && waitingLong && provider && guides.oauth.indexOf(provider.name) >= 0 ? guides.signIn(provider.name) : null
    // A guide's page opens when its question comes up (once per question).
    property var openedGuides: ({})
    onGuideChanged: if (guide && guide.url) {
        const k = provider.name + "/" + (step.option || {}).Name
        if (!openedGuides[k]) { openedGuides[k] = true; app.grove.openUrl(guide.url) }
    }
    title: page === "pick" ? "Add Cloud Drive" : provider ? "Add " + shortName(provider) : "Add Cloud Drive"
    // rclone's descriptions can be a whole list ("Amazon S3 Compliant
    // Storage Providers including AWS, Alibaba, …"): keep the name part.
    function shortName(p) {
        const d = p.description || p.name
        const m = d.match(/^(.*?)(\s+including\b|\s*\(|,)/)
        return m && m[1].length > 2 ? m[1] : d
    }
    width: Math.min(parent ? parent.width - 40 : 600, Math.round(600 * Theme.pt))
    closePolicy: busy ? Popup.NoAutoClose : Popup.CloseOnEscape

    function start() {
        page = "pick"; provider = null; error = ""; busy = false; created = false; step = {}; answers = {}; openedGuides = {}
        search.text = ""
        open()
        if (!providers.length)
            Async.call(app.grove, k => app.grove.rcloneProviders(k), r => { if (r.error) cd.error = r.error; else cd.providers = r.providers })
        search.forceActiveFocus()
    }
    function pick(p) {
        provider = p
        nameField.text = shortName(p).replace(/[:\/,]/g, " ").replace(/\s+/g, " ").trim()
        page = "name"
        Qt.callLater(() => nameField.forceActiveFocus())
    }
    // One step of rclone's questions; the answer (if any) to the current one.
    function send(result, opts) {
        opts = opts || {}
        busy = true; error = ""; waitingLong = false
        longWait.restart()
        if (step.option && step.option.Name && result !== undefined) { const a = Object.assign({}, answers); a[step.option.Name] = String(result); answers = a }
        const req = { name: driveName, type: provider.name, state: step.state || "", result: result === undefined ? "" : String(result),
                      password: !!(step.option && step.option.IsPassword), defaults: !!opts.defaults, started: created }
        Async.call(app.grove, k => app.grove.rcloneStep(k, JSON.stringify(req)), r => {
            cd.busy = false; longWait.stop(); cd.waitingLong = false
            if (r.error && !r.state && !r.done) { cd.error = r.error; return }
            cd.created = true
            if (r.done) { cd.page = "done"; cd.error = r.error || ""; return }
            cd.step = r
            cd.error = r.error || ""
            cd.page = "ask"
            cd.loadAnswer()
        })
    }
    Timer { id: longWait; interval: 1500; onTriggered: cd.waitingLong = true }
    function begin() {
        driveName = nameField.text.trim()
        if (!driveName) { error = "Give the drive a name."; return }
        step = {}
        created = false
        send("")
    }
    // Leaving half-way: forget the half-made drive.
    function abandon() {
        if (busy) app.grove.rcloneCancel()
        if (created && page !== "done") Async.call(app.grove, k => app.grove.rcloneDelete(k, cd.driveName), null)
        created = false
        close()
    }
    function startOver() {
        if (busy) app.grove.rcloneCancel()
        if (created) Async.call(app.grove, k => app.grove.rcloneDelete(k, cd.driveName), null)
        created = false; busy = false; step = {}; error = ""
        page = "pick"
    }
    function openDrive() {
        const j = { kind: "rclone", remote: driveName + ":", name: driveName, remember: true, savePassword: false, host: "", user: "", path: "", password: "", port: 0, device: "" }
        busy = true
        Async.call(app.grove, k => app.grove.connectServer(k, JSON.stringify(j)), r => {
            cd.busy = false
            if (r.error) { cd.error = r.error; return }
            cd.close()
            cd.win.go(r.path)
        })
    }

    // ---- the current question
    readonly property var opt: step.option || ({})
    readonly property var examples: opt.Examples || []
    readonly property bool isBool: opt.Type === "bool"
    readonly property bool choiceOnly: examples.length > 0 && !!opt.Exclusive && !isBool
    readonly property bool skippable: (step.state || "").indexOf("*all") === 0 && !opt.Required
    // rclone's help: the first line is the question, the rest explains.
    readonly property var helpLines: (opt.Help || "").split("\n")
    property string answer: ""
    function loadAnswer() {
        const v = opt.ValueStr !== undefined && opt.ValueStr !== "" ? opt.ValueStr : (opt.DefaultStr || "")
        answer = opt.IsPassword ? "" : v
        answerField.text = answer
        if (!choiceOnly && !isBool) Qt.callLater(() => answerField.forceActiveFocus())
    }
    // rclone's help as text with its web addresses as links (opened in the
    // browser); everything else escaped.
    function linkify(t) {
        const esc = String(t || "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        return esc.replace(/(https?:\/\/[^\s<>()"]+[^\s<>()".,;:!?'])/g, '<a href="$1">$1</a>').replace(/\n/g, "<br>")
    }
    function prettyName(n) { if (n === "config_fs_advanced") return "Advanced settings"; return (n || "").replace(/^config_/, "").replace(/_/g, " ").replace(/^./, c => c.toUpperCase()) }

    // ---------------------------------------------------------- pick
    Column {
        width: parent.width
        spacing: 8
        visible: cd.page === "pick"
        GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; color: Theme.fgSecondary
            text: "Choose where your files are. Grove asks rclone's questions for it one at a time, and opens your browser if you need to sign in." }
        GField { id: search; width: parent.width; iconName: "edit-find"; clearable: true; placeholderText: "Search (e.g. “google”, “s3”, “nextcloud”)"; tip: "Filter the kinds of drives"
            Keys.onReturnPressed: { const f = plist.model.find(p => !p.header); if (f) cd.pick(f) } }
        Item {
            width: parent.width
            height: Math.round(330 * Theme.pt)
            KineticWheel { flickable: plist; notch: Theme.rowHeight * 3 }
            ListView {
                id: plist
                anchors.fill: parent
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: GScrollBar {}
                // Matching kinds, with "Popular" / "Everything else" headings.
                model: {
                    const q = search.text.toLowerCase()
                    const m = cd.providers.filter(p => !q || (p.description + " " + p.name).toLowerCase().indexOf(q) >= 0)
                    const pop = m.filter(p => p.popular), rest = m.filter(p => !p.popular)
                    return (pop.length ? [{ header: "Popular" }].concat(pop) : []).concat(rest.length && pop.length ? [{ header: "Everything else" }] : []).concat(rest)
                }
                delegate: Rectangle {
                    required property var modelData
                    width: plist.width
                    height: modelData.header ? Theme.rowHeight + 4 : Theme.rowHeight + 8
                    GLabel { visible: !!parent.modelData.header; anchors.bottom: parent.bottom; anchors.bottomMargin: 3; x: 4; text: parent.modelData.header || ""; font.bold: true; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                    radius: 6
                    color: pm.containsMouse ? Theme.hover : "transparent"
                    Row {
                        visible: !parent.modelData.header
                        anchors.verticalCenter: parent.verticalCenter
                        x: 8
                        spacing: 10
                        GIcon { size: 18; name: "folder-cloud|network-server"; anchors.verticalCenter: parent.verticalCenter }
                        GLabel { text: modelData.description || ""; anchors.verticalCenter: parent.verticalCenter }
                        GLabel { text: modelData.name || ""; color: Theme.fgTertiary; font.pixelSize: Theme.small; anchors.verticalCenter: parent.verticalCenter }
                    }
                    MouseArea { id: pm; anchors.fill: parent; hoverEnabled: true; enabled: !parent.modelData.header; onClicked: cd.pick(parent.modelData) }
                }
            }
            BusyDots { anchors.centerIn: parent; running: cd.providers.length === 0 && cd.error === "" }
        }
    }

    // ---------------------------------------------------------- name
    Column {
        width: parent.width
        spacing: 8
        visible: cd.page === "name"
        GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; text: "What should this drive be called?" }
        GField { id: nameField; width: parent.width; placeholderText: "e.g. My Google Drive"; tip: "Shown in the sidebar; letters, numbers, spaces, - _ . (no “:” or “/”)"
            Keys.onReturnPressed: cd.begin() }
        GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; color: Theme.fgSecondary; font.pixelSize: Theme.small
            text: "Next, rclone asks what it needs for " + (cd.provider ? cd.shortName(cd.provider) : "") + ". Most questions can be left as they are." }
    }

    // ---------------------------------------------------------- ask
    Column {
        width: parent.width
        spacing: 8
        visible: cd.page === "ask"
        GLabel { width: parent.width; text: cd.prettyName(cd.opt.Name) + (cd.opt.Required ? "" : "  (optional)"); color: Theme.fgSecondary; font.pixelSize: Theme.small }
        GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; font.bold: true; textFormat: Text.StyledText; linkColor: Theme.accent
            text: cd.linkify(cd.helpLines[0] || cd.prettyName(cd.opt.Name)); onLinkActivated: (l) => Qt.openUrlExternally(l)
            HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor } }
        Item {
            width: parent.width
            height: Math.min(helpRest.implicitHeight, Math.round(120 * Theme.pt))
            visible: helpRest.text !== ""
            clip: true
            KineticWheel { flickable: helpFlick }
            Flickable {
                id: helpFlick
                anchors.fill: parent
                contentHeight: helpRest.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: GScrollBar {}
                GLabel { id: helpRest; width: helpFlick.width - 12; wrapMode: Text.Wrap; elide: Text.ElideNone; color: Theme.fgSecondary; textFormat: Text.StyledText; linkColor: Theme.accent
                    onLinkActivated: (l) => Qt.openUrlExternally(l)
                    HoverHandler { cursorShape: parent.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor }
                    text: cd.linkify(cd.helpLines.slice(1).join("\n").trim()) }
            }
        }
        // Step-by-step help for this question (its page opens by itself).
        Column {
            width: parent.width
            spacing: 8
            visible: !!cd.guide
            Rectangle {
                width: parent.width
                height: guideSay.implicitHeight + 16
                radius: 8
                color: Qt.rgba(Theme.accent.r, Theme.accent.g, Theme.accent.b, 0.12)
                GLabel { id: guideSay; x: 10; y: 8; width: parent.width - 20; wrapMode: Text.Wrap; elide: Text.ElideNone; text: cd.guide ? cd.guide.say : "" }
            }
            StepPicture {
                width: parent.width
                visible: !!(cd.guide && cd.guide.rows)
                address: cd.guide && cd.guide.address ? cd.guide.address : ""
                heading: cd.guide && cd.guide.heading ? cd.guide.heading : ""
                rows: cd.guide && cd.guide.rows ? cd.guide.rows : []
            }
            PushButton {
                visible: !!(cd.guide && cd.guide.url)
                text: "Open This Page Again"
                tip: cd.guide && cd.guide.url ? cd.guide.url : ""
                onClicked: cd.app.grove.openUrl(cd.guide.url)
            }
        }
        // yes / no
        Row {
            visible: cd.isBool
            spacing: 8
            PushButton { text: "Yes"; primary: cd.answer === "true"; tip: "Answer yes"; enabled: !cd.busy; onClicked: { cd.answer = "true"; cd.send("true") } }
            PushButton { text: "No"; primary: cd.answer !== "true"; tip: "Answer no"; enabled: !cd.busy; onClicked: { cd.answer = "false"; cd.send("false") } }
        }
        // pick one of rclone's choices
        Column {
            width: parent.width
            spacing: 4
            visible: cd.choiceOnly
            Repeater {
                model: cd.choiceOnly ? cd.examples : []
                Rectangle {
                    required property var modelData
                    width: parent.width
                    height: Math.max(Theme.rowHeight + 6, chHelp.implicitHeight + 10)
                    radius: 6
                    color: cd.answer === modelData.Value ? Theme.accent : cm.containsMouse ? Theme.hover : Theme.field
                    GLabel { id: chHelp; x: 10; width: parent.width - 20; anchors.verticalCenter: parent.verticalCenter; wrapMode: Text.Wrap; elide: Text.ElideNone
                        text: (modelData.Help || modelData.Value).split("\n")[0] + (modelData.Help && modelData.Value && modelData.Help.indexOf(modelData.Value) < 0 ? "  —  " + modelData.Value : "")
                        color: cd.answer === modelData.Value ? Theme.accentText : Theme.fg }
                    MouseArea { id: cm; anchors.fill: parent; hoverEnabled: true; enabled: !cd.busy; onClicked: { cd.answer = parent.modelData.Value; cd.send(parent.modelData.Value) } }
                }
            }
        }
        // type an answer (suggestions below when rclone has some)
        GField {
            id: answerField
            width: parent.width
            visible: !cd.isBool && !cd.choiceOnly
            echoMode: cd.opt.IsPassword ? TextInput.Password : TextInput.Normal
            placeholderText: cd.opt.IsPassword ? "password" : cd.opt.DefaultStr ? "default: " + cd.opt.DefaultStr : (cd.opt.Required ? "required" : "leave empty to skip")
            tip: cd.opt.IsPassword ? "Stored obscured in rclone's config" : "Your answer (Return for Next)"
            onTextChanged: cd.answer = text
            Keys.onReturnPressed: if (!cd.busy) cd.send(text)
        }
        Flow {
            width: parent.width
            spacing: 6
            visible: !cd.isBool && !cd.choiceOnly && cd.examples.length > 0
            Repeater {
                model: !cd.choiceOnly && !cd.isBool ? cd.examples : []
                GButton {
                    required property var modelData
                    flat: false; capsule: true
                    text: modelData.Value
                    tip: modelData.Help || modelData.Value
                    onClicked: answerField.text = modelData.Value
                }
            }
        }
        Row {
            spacing: 8
            visible: cd.busy && cd.waitingLong
            BusyDots { running: parent.visible; anchors.verticalCenter: parent.verticalCenter }
            GLabel { text: cd.signInGuide ? "Waiting for you to sign in in the browser…" : "Waiting for rclone… If your browser opened, sign in there, then come back."; color: Theme.fgSecondary; anchors.verticalCenter: parent.verticalCenter }
        }
        // The browser sign-in: what that page looks like and what to click.
        Column {
            width: parent.width
            spacing: 8
            visible: !!cd.signInGuide
            GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; text: cd.signInGuide ? cd.signInGuide.say : "" }
            StepPicture {
                width: parent.width
                address: cd.signInGuide ? cd.signInGuide.address : ""
                heading: cd.signInGuide ? cd.signInGuide.heading : ""
                rows: cd.signInGuide ? cd.signInGuide.rows : []
            }
        }
    }

    // ---------------------------------------------------------- done
    Column {
        width: parent.width
        spacing: 8
        visible: cd.page === "done"
        GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; font.bold: true; text: "“" + cd.driveName + "” is ready." }
        GLabel { width: parent.width; wrapMode: Text.Wrap; elide: Text.ElideNone; color: Theme.fgSecondary
            text: "Open it to browse its files; it's remembered in the sidebar under Network. To remove it later: Connect to Server › Cloud." }
    }

    GLabel { visible: cd.error !== ""; width: parent.width; text: cd.error; color: Theme.danger; wrapMode: Text.Wrap; elide: Text.ElideNone }

    buttons: [
        BusyDots { running: cd.busy && !cd.waitingLong; anchors.verticalCenter: parent ? parent.verticalCenter : undefined },
        PushButton { text: "Start Over"; visible: cd.page === "ask" || cd.page === "name"; tip: "Pick a different kind of drive"; onClicked: cd.startOver() },
        PushButton { text: cd.page === "done" ? "Close" : "Cancel"; tip: cd.page === "done" ? "Close" : "Stop; a half-made drive is removed"; onClicked: cd.page === "done" ? cd.close() : cd.abandon() },
        PushButton { text: "Use Defaults for the Rest"; visible: cd.page === "ask" && cd.skippable; enabled: !cd.busy; tip: "Answer every remaining optional question with its default (sign-in steps still ask)"
            onClicked: cd.send(cd.answer, { defaults: true }) },
        PushButton { text: "Next"; primary: true; visible: cd.page === "name" || (cd.page === "ask" && !cd.isBool && !cd.choiceOnly); enabled: !cd.busy
            onClicked: cd.page === "name" ? cd.begin() : cd.send(answerField.text) },
        PushButton { text: "Open Drive"; primary: true; visible: cd.page === "done"; enabled: !cd.busy; tip: "Connect it and show its files"; onClicked: cd.openDrive() }
    ]
}

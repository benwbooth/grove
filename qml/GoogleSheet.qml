import QtQuick
import QtQuick.Controls.Basic

// Sign in with Google (for Google Docs/Sheets/Slides previews), and the
// one-time setup it needs: Google only lets an app sign people in with an
// OAuth client from its owner's Google Cloud project, so this walks
// through making one: each step opens its page in the browser, shows a
// drawing of that page with the thing to click highlighted, and says what
// to do. Then paste the client's ID and secret and sign in.
Sheet {
    id: gs
    property var app
    property var win
    property int step: 0              // 0: sign in (or the setup's intro), 1..6: setup
    property bool configured: false
    property var accounts: []
    property bool busy: false
    property string error: ""
    property string done: ""
    title: step === 0 ? "Sign in with Google" : "Set up Google sign-in — step " + step + " of 6"
    width: Math.min(parent ? parent.width - 40 : 640, Math.round(640 * Theme.pt))
    closePolicy: busy ? Popup.NoAutoClose : Popup.CloseOnEscape

    function refresh() {
        const s = JSON.parse(app.grove.googleStatus())
        configured = s.configured
        accounts = s.accounts || []
    }
    function start() {
        refresh()
        step = 0; error = ""; done = ""; busy = false
        idField.text = app.settings.google_client_id || ""
        secretField.text = app.settings.google_client_secret || ""
        open()
    }
    // Each step's page: opened as the step is shown (once), and on request.
    readonly property var steps: [
        null,
        { url: "https://console.cloud.google.com/projectcreate",
          say: "Make a Google Cloud project for grove. Type a name (grove is fine), then click Create. It's free.",
          address: "console.cloud.google.com/projectcreate", heading: "New Project",
          rows: [{ kind: "field", label: "Project name *", value: "grove", hot: true }, { kind: "field", label: "Location", value: "No organization" }, { kind: "button", label: "CREATE", hot: true }] },
        { url: "https://console.cloud.google.com/apis/library/drive.googleapis.com",
          say: "Turn on the Google Drive API for it: make sure your new project is picked at the top of the page, then click Enable.",
          address: "console.cloud.google.com/apis/library/drive.googleapis.com", heading: "Google Drive API",
          rows: [{ kind: "text", label: "Google Enterprise API · Create and manage resources in Google Drive." }, { kind: "button", label: "ENABLE", hot: true }] },
        { url: "https://console.cloud.google.com/auth/overview/create",
          say: "Name the sign-in screen: App name grove and your email, then Next. Audience: External, Next. Your email again, Next. Tick the agreement and click Create.",
          address: "console.cloud.google.com/auth/overview/create", heading: "Project configuration",
          rows: [{ kind: "field", label: "App name *", value: "grove", hot: true }, { kind: "field", label: "User support email *", value: "you@gmail.com" },
                 { kind: "choice", label: "Internal" }, { kind: "choice", label: "External", value: true, hot: true }, { kind: "button", label: "CREATE" }] },
        { url: "https://console.cloud.google.com/auth/audience",
          say: "Publish it, so your sign-in lasts (in \"testing\" it ends every 7 days): click Publish app, then Confirm. It's only for you; Google doesn't review it.",
          address: "console.cloud.google.com/auth/audience", heading: "Audience",
          rows: [{ kind: "text", label: "Publishing status: Testing" }, { kind: "button", label: "PUBLISH APP", hot: true }, { kind: "text", label: "User type: External" }] },
        { url: "https://console.cloud.google.com/auth/clients/create",
          say: "Make grove's client: Application type Desktop app, name grove, then Create. A box shows its Client ID and Client secret: keep it open for the next step.",
          address: "console.cloud.google.com/auth/clients/create", heading: "Create OAuth client ID",
          rows: [{ kind: "field", label: "Application type *", value: "Desktop app", hot: true }, { kind: "field", label: "Name *", value: "grove" }, { kind: "button", label: "CREATE", hot: true }] },
        { url: "https://console.cloud.google.com/auth/clients",
          say: "Copy the Client ID and the Client secret from that box (or from the client's page under Clients) into the boxes below, then Save and sign in.",
          address: "console.cloud.google.com/auth/clients", heading: "OAuth client created",
          rows: [{ kind: "field", label: "Client ID", value: "123456789-abc….apps.googleusercontent.com", hot: true }, { kind: "field", label: "Client secret", value: "GOCSPX-…", hot: true }, { kind: "button", label: "OK" }] }
    ]
    property var openedSteps: ({})
    onStepChanged: {
        error = ""
        const s = steps[step]
        if (s && !openedSteps[step]) { openedSteps[step] = true; app.grove.openUrl(s.url) }
    }
    function signIn() {
        busy = true; error = ""; done = ""
        Async.call(app.grove, k => app.grove.googleSignIn(k), r => {
            gs.busy = false
            if (r.error) { gs.error = r.error; return }
            gs.done = "Signed in as " + r.email + ". Google documents now show in previews."
            gs.refresh()
            gs.win.toast("info", "Signed in to Google as " + r.email)
        })
    }

    Column {
        width: parent.width
        spacing: 12

        // ---- signing in (and who's signed in)
        Column {
            visible: gs.step === 0
            width: parent.width
            spacing: 10
            GLabel {
                width: parent.width; wrapMode: Text.Wrap; color: Theme.fgSecondary
                text: gs.configured ? "Sign in to the Google account your Docs, Sheets and Slides are in. Your browser shows Google's own sign-in page; grove only keeps the permission to read your Drive (in your keyring)."
                    : "Google documents in your synced folders are only links. To show what's in them, grove signs in to Google, which first needs a one-time setup in Google Cloud (about 5 minutes, free). Each step opens its page and shows what to click."
            }
            Repeater {
                model: gs.accounts
                Row {
                    required property string modelData
                    spacing: 8
                    GIcon { name: "user-identity|avatar-default"; size: 18; anchors.verticalCenter: parent.verticalCenter }
                    GLabel { text: parent.modelData; anchors.verticalCenter: parent.verticalCenter }
                    PushButton { text: "Sign Out"; tip: "Forget this account (its permission is removed from the keyring)"; onClicked: { gs.app.grove.googleSignOut(parent.modelData); gs.refresh() } }
                }
            }
            GLabel { visible: gs.busy; text: "Finish signing in in your browser…"; color: Theme.fgSecondary }
            GLabel { visible: gs.done !== ""; text: gs.done; wrapMode: Text.Wrap; width: parent.width; color: Theme.good }
            GLabel {
                visible: gs.error !== ""
                width: parent.width; wrapMode: Text.Wrap; color: Theme.danger
                text: gs.error + (gs.error.indexOf("unverified") >= 0 || gs.error.indexOf("access_denied") >= 0 ? "" : "")
            }
            GLabel {
                visible: gs.configured && !gs.busy
                width: parent.width; wrapMode: Text.Wrap; color: Theme.fgTertiary; font.pixelSize: Theme.small
                text: "Google will say \"Google hasn't verified this app\": it's your own app. Click Advanced, then \"Go to grove (unsafe)\", then Continue."
            }
        }

        // ---- the setup steps
        Column {
            visible: gs.step > 0
            width: parent.width
            spacing: 10
            GLabel { width: parent.width; wrapMode: Text.Wrap; text: gs.steps[gs.step] ? gs.steps[gs.step].say : "" }
            StepPicture {
                width: parent.width
                address: gs.steps[gs.step] ? gs.steps[gs.step].address : ""
                heading: gs.steps[gs.step] ? gs.steps[gs.step].heading : ""
                rows: gs.steps[gs.step] ? gs.steps[gs.step].rows : []
            }
            PushButton {
                text: "Open This Page Again"
                tip: gs.steps[gs.step] ? gs.steps[gs.step].url : ""
                onClicked: gs.app.grove.openUrl(gs.steps[gs.step].url)
            }
            Column {
                visible: gs.step === 6
                width: parent.width
                spacing: 6
                GLabel { text: "Client ID"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                GField { id: idField; width: parent.width; placeholderText: "…apps.googleusercontent.com" }
                GLabel { text: "Client secret"; color: Theme.fgSecondary; font.pixelSize: Theme.small }
                GField { id: secretField; width: parent.width; placeholderText: "GOCSPX-…" }
            }
            GLabel { visible: gs.error !== "" && gs.step > 0; text: gs.error; color: Theme.danger; wrapMode: Text.Wrap; width: parent.width }
        }
    }

    buttons: [
        PushButton { text: "Close"; enabled: !gs.busy; onClicked: gs.close() },
        PushButton { visible: gs.step > 0; text: "Back"; onClicked: gs.step = gs.step - 1 },
        PushButton {
            visible: gs.step === 0 && !gs.configured
            text: "Start Setup"
            primary: true
            onClicked: gs.step = 1
        },
        PushButton {
            visible: gs.step === 0 && gs.configured
            text: gs.accounts.length ? "Sign In Another Account" : "Sign In with Google"
            primary: true
            enabled: !gs.busy
            onClicked: gs.signIn()
        },
        PushButton { visible: gs.step > 0 && gs.step < 6; text: "Next"; primary: true; onClicked: gs.step = gs.step + 1 },
        PushButton {
            visible: gs.step === 6
            text: "Save and Sign In"
            primary: true
            enabled: idField.text.trim().indexOf(".apps.googleusercontent.com") > 0 && secretField.text.trim() !== ""
            onClicked: {
                gs.win.setSetting("google_client_id", idField.text.trim())
                gs.win.setSetting("google_client_secret", secretField.text.trim())
                gs.step = 0
                Qt.callLater(() => { gs.refresh(); gs.signIn() })
            }
        }
    ]
}

import QtQuick

// Step-by-step help for the Add Cloud Drive questions that need something
// from the provider's website (access keys, tokens, app passwords) or a
// sign-in in the browser: what to do, the page to open (opened for the
// user when the question comes up), and a drawn picture of that page with
// the thing to click highlighted (StepPicture). guide() answers null when a
// question needs nothing special.
QtObject {
    // Providers whose sign-in rclone does in the browser (OAuth).
    readonly property var oauth: ["drive", "onedrive", "dropbox", "box", "pcloud", "gphotos", "yandex", "zoho", "hidrive", "putio", "premiumizeme", "sharefile", "googlephotos"]

    function s3Keys(provider) {
        const p = (provider || "").toLowerCase()
        if (p === "cloudflare") return { url: "https://dash.cloudflare.com/?to=/:account/r2/api-tokens", address: "dash.cloudflare.com › R2 › API Tokens", heading: "Manage R2 API Tokens",
            say: "In Cloudflare: R2 › Manage API Tokens › Create API Token. Give it Object Read & Write, then Create. Copy the Access Key ID here; the Secret Access Key goes in the next question.",
            rows: [{ kind: "button", label: "Create API token", hot: true }, { kind: "choice", label: "Object Read & Write", value: true }, { kind: "field", label: "Access Key ID", value: "a1b2c3…", hot: true }, { kind: "field", label: "Secret Access Key", value: "••••••" }] }
        if (p === "wasabi") return { url: "https://console.wasabisys.com/access_keys", address: "console.wasabisys.com/access_keys", heading: "Access Keys",
            say: "In Wasabi: Access Keys › Create New Access Key › Root user (or a sub-user) › Create. Copy the Access Key here and the Secret Key into the next question.",
            rows: [{ kind: "button", label: "CREATE NEW ACCESS KEY", hot: true }, { kind: "field", label: "Access Key", value: "ABCD…", hot: true }, { kind: "field", label: "Secret Key", value: "••••••" }] }
        if (p === "digitalocean") return { url: "https://cloud.digitalocean.com/spaces/access_keys", address: "cloud.digitalocean.com/spaces/access_keys", heading: "Spaces Keys",
            say: "In DigitalOcean: Spaces Object Storage › Access Keys › Create Access Key. Copy the Access Key ID here and the Secret into the next question.",
            rows: [{ kind: "button", label: "Create Access Key", hot: true }, { kind: "field", label: "Access Key ID", value: "DO00…", hot: true }, { kind: "field", label: "Secret", value: "••••••" }] }
        if (p === "b2" || p === "backblaze") return b2()
        return { url: "https://console.aws.amazon.com/iam/home#/security_credentials", address: "console.aws.amazon.com › IAM › Security credentials", heading: "Access keys",
            say: "In AWS: Security credentials › Access keys › Create access key (pick \"Application running outside AWS\"). Copy the Access key here; the Secret access key goes in the next question. (Other S3 services have an Access Keys page in their console.)",
            rows: [{ kind: "text", label: "Access keys (1 of 2 used)" }, { kind: "button", label: "Create access key", hot: true }, { kind: "field", label: "Access key", value: "AKIA…", hot: true }, { kind: "field", label: "Secret access key", value: "••••••" }] }
    }
    function b2() {
        return { url: "https://secure.backblaze.com/app_keys.htm", address: "secure.backblaze.com/app_keys.htm", heading: "Application Keys",
            say: "In Backblaze: Application Keys › Add a New Application Key. Name it (grove), allow Read and Write, then Create. Copy keyID here (as the account) and applicationKey into the next question; Backblaze shows it only once.",
            rows: [{ kind: "button", label: "Add a New Application Key", hot: true }, { kind: "field", label: "keyID", value: "0051a2…", hot: true }, { kind: "field", label: "applicationKey", value: "K005…" }] }
    }

    // `provider`: rclone's backend name; `name`: the question (option Name);
    // `answers`: what was answered so far (s3's provider...).
    function guide(provider, name, answers) {
        const n = (name || "").toLowerCase()
        if (oauth.indexOf(provider) >= 0) {
            if (n === "client_id" || n === "client_secret")
                return { say: "Leave this empty: rclone's own sign-in is used (you only need your own if you've made one)." }
            if (n === "config_is_local")
                return { say: "Choose Yes: your web browser opens the sign-in page; sign in and allow access, then come back here." }
        }
        if (provider === "s3" && (n === "access_key_id" || n === "secret_access_key"))
            return n === "access_key_id" ? s3Keys(answers.provider) : { say: "Paste the secret that was shown next to the access key (it's shown only once; if it's gone, make a new key)." }
        if (provider === "b2" && n === "account") return b2()
        if (provider === "b2" && n === "key") return { say: "Paste the applicationKey shown with the keyID (shown only once; if it's gone, add a new key)." }
        if (provider === "internetarchive" && n === "access_key_id")
            return { url: "https://archive.org/account/s3.php", address: "archive.org/account/s3.php", heading: "Your S3-like API keys",
                     say: "Sign in to archive.org: this page shows your keys (Generate New Keys if there are none). Copy the access key here and the secret into the next question.",
                     rows: [{ kind: "field", label: "Your S3 access key", value: "a1B2c3…", hot: true }, { kind: "field", label: "Your S3 secret key", value: "••••••" }, { kind: "link", label: "Generate New Keys" }] }
        if (provider === "jottacloud" && (n.indexOf("token") >= 0 || n === "config_login_token"))
            return { url: "https://www.jottacloud.com/web/secure", address: "jottacloud.com/web/secure", heading: "Security",
                     say: "In Jottacloud: Settings › Security › Personal login token › Generate. Copy the token here.",
                     rows: [{ kind: "text", label: "Personal login token" }, { kind: "button", label: "Generate", hot: true }, { kind: "field", label: "Token", value: "eyJ…", hot: true }] }
        if (provider === "koofr" && n === "password")
            return { url: "https://app.koofr.net/app/admin/preferences/password", address: "app.koofr.net › Preferences › Password", heading: "App passwords",
                     say: "Koofr needs an app password (not your login one): Preferences › Password › App passwords › Generate new password. Paste it here.",
                     rows: [{ kind: "field", label: "Name", value: "grove" }, { kind: "button", label: "Generate new password", hot: true }] }
        if (provider === "mailru" && n === "pass")
            return { url: "https://account.mail.ru/user/2-step-auth/passwords/", address: "account.mail.ru › Passwords for external apps", heading: "Passwords for external applications",
                     say: "Mail.ru needs a password for external apps: add one (any name) and paste it here.",
                     rows: [{ kind: "button", label: "Add", hot: true }, { kind: "field", label: "Password", value: "••••••" }] }
        if (provider === "gofile" && n === "access_token")
            return { url: "https://gofile.io/myProfile", address: "gofile.io/myProfile", heading: "My Profile",
                     say: "Your Gofile profile page shows your account API token: copy it here.", rows: [{ kind: "field", label: "Account API token", value: "AbC123…", hot: true }] }
        if (provider === "pixeldrain" && n === "api_key")
            return { url: "https://pixeldrain.com/user/api_keys", address: "pixeldrain.com/user/api_keys", heading: "API keys",
                     say: "Make an API key on Pixeldrain (API keys › Generate key) and paste it here.", rows: [{ kind: "button", label: "Generate key", hot: true }, { kind: "field", label: "Key", value: "1a2b…", hot: true }] }
        if (provider === "iclouddrive" && n === "password")
            return { say: "Your Apple ID password (an app-specific password won't work here). Apple will ask to approve the sign-in on your iPhone, iPad or Mac next." }
        if (provider === "iclouddrive" && n.indexOf("2fa") >= 0)
            return { say: "Approve the sign-in on your iPhone, iPad or Mac (\"Allow\"), then type the 6-digit code it shows." }
        if (provider === "protondrive" && n === "2fa")
            return { say: "The 6-digit code from your authenticator app (leave empty if two-factor isn't on)." }
        if (provider === "webdav" && n === "url")
            return { say: "Your WebDAV address. Nextcloud and ownCloud show it in Files › Files settings (bottom left) › WebDAV, like https://cloud.example.com/remote.php/dav/files/you/",
                     address: "cloud.example.com › Files", heading: "Files settings",
                     rows: [{ kind: "text", label: "Files settings" }, { kind: "field", label: "WebDAV", value: "https://cloud.example.com/remote.php/dav/files/you/", hot: true }] }
        if ((provider === "sftp" || provider === "ftp" || provider === "smb") && n === "host")
            return { say: "The server's name or address, like nas.local or 192.168.1.10 (no ftp:// or smb:// in front)." }
        return null
    }

    // The browser sign-in, while rclone waits for it.
    function signIn(provider) {
        const pages = {
            drive: { address: "accounts.google.com", heading: "rclone wants to access your Google Account", allow: "Allow" },
            gphotos: { address: "accounts.google.com", heading: "rclone wants to access your Google Account", allow: "Allow" },
            onedrive: { address: "login.microsoftonline.com", heading: "Let this app access your info?", allow: "Accept" },
            dropbox: { address: "www.dropbox.com/oauth2/authorize", heading: "rclone would like access to the files in your Dropbox", allow: "Allow" },
            box: { address: "account.box.com", heading: "Grant access to Box", allow: "Grant access to Box" },
            pcloud: { address: "my.pcloud.com/oauth2/authorize", heading: "rclone wants access to your pCloud", allow: "Allow" }
        }
        const p = pages[provider] || { address: "the service's sign-in page", heading: "Allow rclone to access your account", allow: "Allow" }
        return { say: "Your browser opened the sign-in page. Sign in, then click " + p.allow + ". (It names rclone: that's the tool grove uses for cloud drives.) When the page says Success, come back here.",
                 address: p.address, heading: p.heading,
                 rows: [{ kind: "text", label: "This will allow rclone to see, edit, create and delete your files." }, { kind: "button", label: p.allow, hot: true }] }
    }
}

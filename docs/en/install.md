# Install

## Requirements

| Platform | Artefact |
| --- | --- |
| macOS (Apple Silicon / Intel) | `.app.tar.gz` |
| Windows | `x64-setup.exe` (installer) or `windows-amd64.zip` (portable) |
| Linux | `.AppImage` / `.deb` / `.rpm` |

Download from [GitHub Releases](https://github.com/dengfuping/cursor-byok/releases/latest).

::: tip First launch on macOS
macOS builds are ad-hoc signed. Allow the app in **System Settings → Privacy & Security**, or right-click it and choose **Open**.
:::

## Install the local CA

To decrypt the HTTPS requests Cursor sends upstream, cursor-byok generates a local root certificate (`Cursor BYOK Local CA`).

On first launch the app asks you to initialize the CA. Click **Install local CA** and it will:

1. Generate the certificate and copy the install command
2. Tell you to open a terminal, paste the command and enter your password

Command shapes per platform:

```bash
# macOS
sudo security add-trusted-cert -d -r trustRoot -p ssl -k /Library/Keychains/System.keychain '<cert path>'

# Windows (administrator PowerShell or CMD)
certutil -addstore -f Root "<cert path>"

# Linux
sudo cp '<cert path>' /etc/pki/ca-trust/source/anchors/cursor-byok-local-ca.crt && sudo update-ca-trust extract
```

Certificate and key live in the data directory:

```text
~/.cursor-byok-v3/
└── ca/
    ├── ca.crt    # root certificate
    └── ca.key    # private key, never share
```

::: danger Never share ca.key
The private key can decrypt your local Cursor traffic. When filing an issue, paste error text only — no `ca.key`, API keys or full request logs.
:::

## Enable takeover

Once the CA is ready, click **Enable takeover** on the Cursor page; the sidebar then shows **Taken over**.

Takeover points Cursor at the local service and makes that service forward requests using your model configs. Turning it off restores Cursor's direct connection to the official service.

## First successful run

1. Add a model and make **Test** pass (see [Model channels](./models.md))
2. **Quit Cursor completely** — `Cmd+Q` on macOS, exit from the tray on Windows, confirm the process is gone
3. Reopen Cursor and start a **new conversation**
4. Pick your model in the model selector (the display name is yours to choose)

Changing model configs or takeover state requires the same restart-and-new-conversation step.

## Upgrading from an older version

::: warning 1.0.3 and older will not update themselves
Those builds check the archived `leookun/cursor-byok` repository, which will not publish again, so they cannot see 1.0.4. Install 1.0.4 manually once; from 1.0.4 onward update checks point at this repository.
:::

If Cursor misbehaves after upgrading:

1. Quit both cursor-byok and Cursor
2. Start cursor-byok again and confirm it still shows **Taken over**
3. Reopen Cursor and start a new conversation

## Uninstall and clean up

Quit the app and delete the data directory `~/.cursor-byok-v3` (a same-named hidden directory under your home folder on Windows). The next launch generates a new CA, so you must authorize it again.

If `Cursor BYOK Local CA` is still installed in the system keychain or certificate store, remove it as well — otherwise Cursor keeps trusting a certificate that no longer exists.

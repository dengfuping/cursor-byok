# TAB completion

Cursor's tab completion is served by a separate TAB service and **never goes through your model channels**. Your models affect chat and Agent only; completion keeps using Cursor's TAB API.

Choose a mode under **Settings → TAB settings**:

| Mode | Behaviour | When |
| --- | --- | --- |
| **Direct** (default) | Connect to the official TAB service of the signed-in Cursor account | The account has official quota |
| **Custom** | Forward to the TAB service address you provide | You self-host one |

Restart Cursor and start a new conversation after switching.

## Self-hosting the TAB service

The repository keeps the older Go `cursor-tab-server` on the `archive/v0.0.49` branch:

```bash
git clone --branch archive/v0.0.49 --depth 1 https://github.com/dengfuping/cursor-byok.git
cd cursor-byok/cursor-tab-server
```

It forwards TAB requests (`RefreshTabContext`, `Cpp*`, `FSSync*` and friends) to the official Cursor endpoints and needs your own Cursor access token:

```bash
# Read the token on macOS
sqlite3 "$HOME/Library/Application Support/Cursor/User/globalStorage/state.vscdb" \
  "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken';"

# Windows
sqlite3 "$env:APPDATA\Cursor\User\globalStorage\state.vscdb" \
  "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken';"
```

Put the token in `config.yaml`, start the service with Docker or the binary, then point **TAB settings → Custom** at it (base address only, no query or fragment).

::: warning Scope of a self-hosted service
That token belongs to your Cursor account, so run the service only where you control it. Do not expose it publicly for others, and never commit a config file containing the token.
:::

## Why there is no public service anymore

Earlier versions shipped an author-hosted public TAB service (`tab.leokun.cn`). It went down with the archived upstream, so 1.0.4 removed the option. Configs saved with the public mode fall back to **Direct**.

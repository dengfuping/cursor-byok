# Settings

## Ports

| Field | Meaning |
| --- | --- |
| Proxy port | Port Cursor connects to. `0` or empty picks a free port; an occupied port also falls back automatically |
| Service port | Port the local service listens on, same rules |

Changing either requires an app restart; the UI says so when you save.

## Proxy

| Mode | Behaviour |
| --- | --- |
| Default | Follow the system proxy |
| Custom | Your own outbound proxy address, optionally with username and password |

This affects only how cursor-byok reaches upstream model APIs, not how Cursor reaches the local service. Never point the proxy at cursor-byok itself.

## External API

With **Enable external API** on, local apps and plugins can call your configured models — plugin models included — using the base address plus the API key.

- Every external request must carry that key once enabled
- Use the base address shown in the UI in your client
- Keep it off when unused to reduce exposure

## Pricing

Used to estimate session cost, per **million tokens**:

| Field | Default |
| --- | --- |
| Input price | 5 |
| Output price | 25 |
| Cache read price | 0.5 |
| Cache write price | 6.25 |

Match your provider's price list, then **Save**. **Restore defaults** returns to the values above. Pricing only affects estimates, never actual billing.

## Commit message model

Controls which model generates commit messages inside Cursor:

- **Model**: pick a configured model, or **Direct** to use Cursor's own service
- **Prompt**: custom prompt for commit messages, per interface language

## Application

| Field | Meaning |
| --- | --- |
| Launch at login | Start cursor-byok when you log in |
| Silent start | Start without showing the window |
| Dock icon | Whether macOS shows the icon in the Dock |

The same page shows the current version and checks for updates.

## Plugins

The **Plugins** page manages built-in plugins such as the Codex and Grok account authorizations. Plugins can use your configured models or authorize third-party accounts via OAuth. They download a pinned Deno runtime on first use, which needs network access.

## Statistics

Settings can clear stored statistics or detailed records. Clearing is irreversible: usage records and cost estimates disappear, model configs are unaffected.

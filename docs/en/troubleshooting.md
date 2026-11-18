# Troubleshooting

Start from the symptom, then check your version. The first table lists real faults that are already fixed — upgrade before debugging further.

## Known issues and fixes

| Symptom | Cause and fix |
| --- | --- |
| The conversation dies within seconds and Cursor reports `reconnecting`; logs show `BidiAppend data_binary is not part of the captured protocol`, `400 Bad Request` | Cursor 3.29.x added the `data_binary` field to `BidiAppend` → **1.0.2** ([PR #1](https://github.com/dengfuping/cursor-byok/pull/1)) |
| Conversations with images fail with `failed to prepare Cursor Run` / `[invalid_argument] Cursor message is missing image` | Image parts became object-shaped while the decoder only accepted strings → **1.0.3** ([PR #2](https://github.com/dengfuping/cursor-byok/pull/2)) |
| One conversation fails on every turn with `ConversationStateStructure.file_states: delimited length exceeded` | A checkpoint whose `file_states` length field disagrees with its content → **1.0.4** ([PR #3](https://github.com/dengfuping/cursor-byok/pull/3), [issue #4](https://github.com/dengfuping/cursor-byok/issues/4)) |

See [upgrading](./install.md#upgrading-from-an-older-version). Note that 1.0.3 and older do not receive 1.0.4 automatically.

## Model test failures

| Symptom | Check first |
| --- | --- |
| 401 | Key correctness, stray whitespace, upstream-specific headers |
| 404 | Whether the base address includes the version path (such as `/v1`), or whether **Use full request URL** is needed |
| Model not found | Whether the model name is the identifier the upstream expects; **Fetch models** lists what it offers |
| Empty stream | Upstream streaming support, and the protocol choice (Responses vs Chat Completions) |
| Format errors from an Anthropic service | Whether the model type is set to OpenAI by mistake |

The **raw response** in the test result is the strongest evidence; read it before editing configs.

## Model missing inside Cursor / no response

1. Did you **quit and restart** Cursor rather than just closing the window?
2. Did you start a **new** conversation? Old ones do not pick up new configs
3. Did you select your model manually? **Auto never uses custom models**
4. Does the Cursor page show **Taken over**?

## Certificates

| Symptom | Action |
| --- | --- |
| It asks you to initialize the CA | Run the authorization command from [Install the local CA](./install.md#install-the-local-ca) |
| The system shows a certificate warning | The CA is not in the system trust store; run the install command again |
| Still untrusted after installing | Confirm the certificate landed in the right keychain or store; delete the stale one, restart the app and regenerate |

## Broken after a Cursor update

1. Quit both Cursor and cursor-byok
2. Start cursor-byok and confirm **Taken over**
3. Open Cursor and start a new conversation

Major Cursor updates can change request shapes. If a restart does not help, check that cursor-byok is up to date, then search [Issues](https://github.com/dengfuping/cursor-byok/issues) for the exact error.

## Collecting evidence

Include the OS, cursor-byok version, Cursor version, model type and protocol, a redacted server address, the full error text and reproduction steps.

::: danger Do not paste secrets
Never paste `ca.key`, API keys, access tokens, full request headers or unredacted logs. The data directory is `~/.cursor-byok-v3`; check before posting.
:::

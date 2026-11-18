# cursor-byok guide

cursor-byok is a local model gateway for Cursor. It runs a service on your machine, forwards the Agent requests Cursor emits to the model APIs you configure, and keeps tool calling, Skills, MCP and multi-turn conversations working.

- Your API keys, model configs and conversation data stay on this machine
- Supports OpenAI Responses, OpenAI Chat Completions and Anthropic Messages
- Coexists with your Cursor account: official quota and your own channels switch freely

::: warning Independent project
This project is not affiliated with Cursor or its developers. The model APIs you connect may charge for usage.
:::

## Five-minute setup

1. **Install** the build for your platform from [GitHub Releases](https://github.com/dengfuping/cursor-byok/releases/latest).
2. **Install the local CA** when the app asks you to initialize one, then run the authorization command in a terminal. See [Install](./install.md).
3. **Add a model** under **Cursor → Add model**: server address, API key, model name. Save, then click **Test**.
4. **Enable takeover** once the test passes.
5. **Restart Cursor** completely (quit the process, not just the window), open a new conversation and pick your model.

::: tip Do not pick Auto
Auto only routes to Cursor's official models. Without official quota you must select your own model manually.
:::

## Where to go next

| Your question | Read |
| --- | --- |
| Setup fails, CA won't authorize, model missing in Cursor | [Install](./install.md) |
| Which protocol to pick, what each field means | [Model channels](./models.md) |
| TAB completion ignores my models, or you want to self-host | [TAB completion](./tab.md) |
| Ports, proxy, external API, pricing, startup | [Settings](./settings.md) |
| An error message you cannot read | [Troubleshooting](./troubleshooting.md) |
| Mixing with official quota, where data lives | [FAQ](./faq.md) |

## Version note

This guide describes **1.0.4 and later**. If you still run 1.0.3 or older, read [upgrading from an older version](./install.md#upgrading-from-an-older-version) first: those builds check the archived upstream for updates.

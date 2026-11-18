# FAQ

## Can I use it alongside my Cursor account?

Yes. Sign in to Cursor as usual; official quota and your own channels switch freely:

- Official models spend your Cursor subscription
- Custom models go through your channels and are billed by your upstream provider
- **Auto only uses official models** and never switches to a custom one
- Plugins, codebase indexing and other Cursor features keep working

If an older build created a fake account for you, sign out of it and sign in with your own.

## Why does Auto ignore my models?

That is Cursor's routing: Auto goes through the official service. Select your model manually in the model list.

## Where does my data go?

- API keys, model configs, conversations and usage data live in `~/.cursor-byok-v3` on this machine
- Model requests go to the upstream provider you configured, under that provider's privacy policy
- cursor-byok sends your credentials nowhere else; the code is open for audit

## Why doesn't TAB completion use my models?

Tab completion is served by Cursor's separate TAB service and bypasses model channels. See [TAB completion](./tab.md).

## Why doesn't 1.0.3 offer updates?

Builds up to 1.0.3 check the archived upstream repository, which will not publish again. Install 1.0.4 once manually; update checks then point at this repository. See [upgrading](./install.md#upgrading-from-an-older-version).

## What happened to the public TAB service?

An author-hosted public TAB service existed in earlier versions and went down with the archived upstream. 1.0.4 removed the option and saved configs fall back to **Direct**. You can still self-host one — see [TAB completion](./tab.md).

## Which model providers work?

Anything compatible with OpenAI Responses, OpenAI Chat Completions or Anthropic Messages. Tool calling, multi-turn and caching depend on what your upstream implements.

## Is it cheaper?

It depends on your upstream's pricing. cursor-byok only forwards requests: no markup, no cut. The "estimated value" in the UI uses the pricing you filled in — it is not a bill.

## How do I wipe local data?

Quit Cursor and cursor-byok, delete `~/.cursor-byok-v3`, and remove the `Cursor BYOK Local CA` certificate from the system store. The next launch generates a new CA that needs authorization again.

## How do I report a problem?

Search [Issues](https://github.com/dengfuping/cursor-byok/issues) first. If nothing matches, open one with OS, both version numbers, the error text and reproduction steps. Never paste keys or `ca.key`.

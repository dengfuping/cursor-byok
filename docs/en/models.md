# Model channels

Each model config is an independent upstream channel with its own address, credentials, protocol and generation parameters. Cursor's model selector gains these models.

## Pick a type and protocol

| Your upstream | Model type | Protocol |
| --- | --- | --- |
| Claude family | Anthropic | Messages API |
| GPT / OpenAI family | OpenAI | **Responses API** |
| Other OpenAI-compatible services | OpenAI | Chat Completions API |

::: tip Prefer Responses for GPT
Chat Completions can miss prompt caching, which costs both speed and money. Only fall back to it when the upstream has no Responses support.
:::

**Model type** only selects request and response shape. **Protocol** only decides how paths are appended — it never rewrites the address you entered.

## Fields

| Field | Meaning |
| --- | --- |
| Model type | OpenAI or Anthropic |
| Protocol | For OpenAI, Responses or Chat Completions |
| Server address | Base address; the app appends the standard endpoint. Tick **Use full request URL** to send it verbatim |
| API key | Upstream credential, stored locally |
| Model name | The identifier the upstream accepts; **Fetch models** reads the upstream list |
| Display name | Shown in Cursor only; never changes the identifier sent upstream |
| Note | Appears in Cursor's model description |
| Context window tokens | Leave empty for the default |
| Max output tokens | Leave empty for the default |
| Reasoning / thinking effort | Use values the upstream supports |
| Custom headers | JSON object; only fields the upstream documents |
| Extra parameters | JSON object, merged into the upstream request body |

## Testing

**Test** sends a real request and reports time to first token, generation speed and the raw response. **Test all** runs the batch, **Cancel all tests** stops it.

Use a model only after its test passes. On failure read the raw response first: 401/404 usually means address or key, a missing model means the model name.

## Groups

A **group name** applies to every model in the group and becomes the badge in Cursor's model selector; clearing it falls back to showing the server host. Changing the server address in **Group settings** applies to the whole group — leave it empty to keep each model's own address.

## Ordering and duplication

Models can be reordered, duplicated and batch-edited. Order only affects the UI, not requests.

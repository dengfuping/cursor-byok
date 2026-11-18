# cursor-byok 使用指南

cursor-byok 是一个运行在本机的 Cursor 模型网关：它在你的电脑上开一个服务，把 Cursor 发出的 Agent 请求转到你配置的模型服务，同时保留 Cursor 的工具调用、Skills、MCP 和多轮对话。

- 你的 API Key、模型配置、会话数据只保存在本机
- 支持 OpenAI Responses / Chat Completions 和 Anthropic Messages 三类接口
- 与 Cursor 官方账号并存：官方额度和自定义模型可以随时切换

::: warning 与 Cursor 官方的关系
本项目是独立开源项目，与 Cursor 或其开发者没有关联。你连接的模型服务商可能按用量收费。
:::

## 五分钟上手

1. **下载安装**：从 [GitHub Releases](https://github.com/dengfuping/cursor-byok/releases/latest) 下载对应系统的安装包。
2. **安装本地 CA**：首次启动会提示「请先初始化 CA」，按提示在终端执行授权命令。详见[下载与安装](./install.md)。
3. **添加模型**：打开 **Cursor 配置 → 添加模型**，填写服务器地址、API Key、模型名称，保存后点 **测试**。
4. **开启接管**：测试通过后点 **开启接管 Cursor**。
5. **重启 Cursor**：完全退出 Cursor（不是关窗口），重新打开后新开一个对话，在模型列表里选择你配置的模型。

::: tip 不要选 Auto
Auto 只会使用 Cursor 官方模型，不会走你配置的模型通道。没有官方额度时必须手动选择自定义模型。
:::

## 接下来看什么

| 你的问题 | 去看 |
| --- | --- |
| 装不上、CA 授权失败、Cursor 里看不到模型 | [下载与安装](./install.md) |
| 不知道选哪种协议、字段怎么填 | [模型通道](./models.md) |
| TAB 补全不走我的模型 / 想自建 TAB 服务 | [TAB 补全](./tab.md) |
| 端口、代理、外部 API、定价、开机启动 | [系统设置](./settings.md) |
| 报错信息看不懂 | [报错排查](./troubleshooting.md) |
| 能否和官方账号混用、数据在哪 | [常见问题](./faq.md) |

## 版本说明

当前文档对应 **1.0.4 及以后** 的版本。若你还在 1.0.3 或更早版本，请先看[这一节](./install.md#从旧版本升级)，旧版本的自动更新检查指向已归档的上游仓库。

# 报错排查

先按「症状」定位，再确认版本。下面几条是已修复的真实故障，老版本上遇到请先升级。

## 已知问题与修复版本

| 症状 | 原因与修复版本 |
| --- | --- |
| 会话几秒内断开，Cursor 一直「reconnecting」，日志出现 `BidiAppend data_binary is not part of the captured protocol`、`400 Bad Request` | Cursor 3.29.x 的 `BidiAppend` 新增了 `data_binary` 字段 → **1.0.2**（[PR #1](https://github.com/dengfuping/cursor-byok/pull/1)） |
| 带图片的对话发不出去，报 `failed to prepare Cursor Run` / `[invalid_argument] Cursor message is missing image` | 图片 part 变成了对象形状，解码只认字符串 → **1.0.3**（[PR #2](https://github.com/dengfuping/cursor-byok/pull/2)） |
| 某个对话每一轮都失败，报 `ConversationStateStructure.file_states: delimited length exceeded` | 检查点里的 `file_states` 长度字段与实际内容不一致 → **1.0.4**（[PR #3](https://github.com/dengfuping/cursor-byok/pull/3)，[issue #4](https://github.com/dengfuping/cursor-byok/issues/4)） |

升级方式见[下载与安装](./install.md#从旧版本升级)。注意 1.0.3 及更早版本不会自动收到 1.0.4。

## 模型测试失败

| 现象 | 先查什么 |
| --- | --- |
| 401 | API Key 是否正确、是否带了多余空格、上游是否要求特定 header |
| 404 | 基础地址是否包含正确的版本路径（如 `/v1`）；是否需要勾选「使用完整请求地址」 |
| 模型不存在 | 模型名称是否是上游认的标识；可用 **获取模型** 拉取上游列表 |
| 返回流没有内容 | 上游是否支持流式；协议是否选错（Responses vs Chat Completions） |
| Anthropic 服务报格式错误 | 模型类型是否选成 OpenAI |

测试结果里的**原始响应**是最直接的证据，先看它再改配置。

## Cursor 里看不到模型 / 选了没反应

1. 是否**完全退出**并重启了 Cursor，而不是只关窗口
2. 是否**新开**了对话（旧对话不会加载新配置）
3. 模型选择器里是否手动选了你的模型——**Auto 不会走自定义模型**
4. **Cursor 配置** 页是否显示 **已接管**

## 证书相关

| 现象 | 处理 |
| --- | --- |
| 提示「请先初始化 CA」 | 按[安装本地 CA](./install.md#安装本地-ca)执行授权命令 |
| 系统里出现证书告警 | CA 未安装到系统信任库，重新执行安装命令 |
| 安装过仍提示未信任 | 检查证书是否在正确的钥匙串/证书库；删除旧证书后重启应用重新生成 |

## 升级 Cursor 后失效

1. 完全退出 Cursor 与 cursor-byok
2. 启动 cursor-byok，确认 **已接管**
3. 打开 Cursor，新开对话

Cursor 大版本更新可能改变请求格式。如果重启无效，先确认 cursor-byok 是否是新版本，再去 [Issues](https://github.com/dengfuping/cursor-byok/issues) 搜索对应报错。

## 收集日志

遇到问题请提供：操作系统、cursor-byok 版本、Cursor 版本、模型类型与协议、已脱敏的服务地址、完整报错文本和复现步骤。

::: danger 不要提交敏感信息
不要贴 `ca.key`、API Key、access token、完整请求头或未脱敏的日志。数据目录在 `~/.cursor-byok-v3`，提交前请检查。
:::

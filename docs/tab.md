# TAB 补全

Cursor 的 Tab 补全由独立的 TAB 服务处理，**不经过模型通道**。也就是说：你配的模型只影响对话和 Agent，Tab 补全仍然走 Cursor 自己的 TAB 接口。

在 **系统设置 → TAB 设置** 里选择连接方式：

| 模式 | 行为 | 适用 |
| --- | --- | --- |
| **直连**（默认） | 直接连当前 Cursor 账号对应的官方 TAB 服务 | 账号有官方额度 |
| **自定义** | 转发到你填写的 TAB 服务地址 | 自建 TAB 服务 |

修改后重启 Cursor 并新开对话生效。

## 自建 TAB 服务

仓库保留了老的 Go 版 `cursor-tab-server`（位于 `archive/v0.0.49` 分支）：

```bash
git clone --branch archive/v0.0.49 --depth 1 https://github.com/dengfuping/cursor-byok.git
cd cursor-byok/cursor-tab-server
```

它把 TAB 相关请求（`RefreshTabContext`、`Cpp*`、`FSSync*` 等）转发到 Cursor 官方接口，需要提供你自己的 Cursor access token：

```bash
# macOS 读取 token
sqlite3 "$HOME/Library/Application Support/Cursor/User/globalStorage/state.vscdb" \
  "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken';"

# Windows
sqlite3 "$env:APPDATA\Cursor\User\globalStorage\state.vscdb" \
  "SELECT value FROM ItemTable WHERE key = 'cursorAuth/accessToken';"
```

把 token 写进 `config.yaml` 后用 Docker 或二进制启动，然后在 **TAB 设置 → 自定义** 里填写服务地址（只填基础地址，不带查询参数和 fragment）。

::: warning 自建服务的边界
这个 token 属于你自己的 Cursor 账号，服务只应部署在你能控制的环境里。不要把它做成公开服务共享给他人，也不要提交带 token 的配置文件。
:::

## 为什么默认不是「公共服务」

早期版本提供过一个作者部署的公共 TAB 服务（`tab.leokun.cn`），随上游归档已停止服务，因此 1.0.4 起移除了该选项。曾保存过公共服务的配置会自动回落到 **直连**。

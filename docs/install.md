# 下载与安装

## 系统要求

| 平台 | 产物 |
| --- | --- |
| macOS（Apple Silicon / Intel） | `.app.tar.gz` |
| Windows | `x64-setup.exe`（安装版）或 `windows-amd64.zip`（便携版） |
| Linux | `.AppImage` / `.deb` / `.rpm` |

从 [GitHub Releases](https://github.com/dengfuping/cursor-byok/releases/latest) 下载。

::: tip macOS 首次打开
macOS 构建使用 ad-hoc 签名，首次打开需要在「系统设置 → 隐私与安全性」里允许，或右键应用选择「打开」。
:::

## 安装本地 CA

cursor-byok 要在本机解密 Cursor 发往官方服务的 HTTPS 请求，因此会生成一份仅本机使用的根证书（`Cursor BYOK Local CA`）。

首次启动会提示 **请先初始化 CA**。点击 **安装本地 CA** 后，应用会：

1. 生成证书并复制安装命令
2. 提示「安装命令已自动复制。点击『打开终端』，将命令粘贴到终端中执行，并按提示输入密码」

各系统的命令形态：

```bash
# macOS
sudo security add-trusted-cert -d -r trustRoot -p ssl -k /Library/Keychains/System.keychain '<证书路径>'

# Windows（管理员 PowerShell 或 CMD）
certutil -addstore -f Root "<证书路径>"

# Linux
sudo cp '<证书路径>' /etc/pki/ca-trust/source/anchors/cursor-byok-local-ca.crt && sudo update-ca-trust extract
```

证书和私钥位于数据目录：

```text
~/.cursor-byok-v3/
└── ca/
    ├── ca.crt    # 根证书
    └── ca.key    # 私钥，不要外传
```

::: danger 不要把 ca.key 发给任何人
私钥能解密你本机的 Cursor 流量。提交 issue 时只贴报错文本，不要贴 `ca.key`、API Key 或完整请求日志。
:::

## 开启接管

CA 就绪后，在 **Cursor 配置** 页面点 **开启接管 Cursor**，侧边栏会显示 **已接管**。

接管做两件事：把 Cursor 的连接指向本机服务，并让本机服务按你的模型配置转发请求。关闭接管会恢复 Cursor 直连官方服务。

## 第一次跑通

1. 添加一个模型并 **测试** 通过（见[模型通道](./models.md)）
2. **完全退出 Cursor**：macOS 用 `Cmd+Q`，Windows 从托盘退出，确认进程结束
3. 重新打开 Cursor，**新开一个对话**
4. 在模型选择器里选择你配置的模型（显示名称由你决定）

修改模型配置或切换接管状态后，同样需要重启 Cursor 并新开对话。

## 从旧版本升级

::: warning 1.0.3 及更早不会自动升级
这些版本的更新检查指向已归档的上游仓库 `leookun/cursor-byok`，该仓库不会再发布新版本，所以它们检查不到 1.0.4。请手动下载 1.0.4 覆盖安装一次；从 1.0.4 起更新检查才会指向本仓库。
:::

升级后如果 Cursor 里行为异常：

1. 完全退出 cursor-byok 与 Cursor
2. 重新启动 cursor-byok，确认仍是 **已接管**
3. 重新打开 Cursor 并新开对话

## 卸载与清理

退出应用后删除数据目录即可，路径为 `~/.cursor-byok-v3`（Windows 在用户目录下同名的隐藏目录）。删除后下次启动会重新生成 CA，需要重新执行一次授权命令。

如果你在系统钥匙串或证书管理器里装过 `Cursor BYOK Local CA`，请一并删除，否则 Cursor 仍会信任一个已失效的本地证书。

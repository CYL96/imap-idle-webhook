# imap-idle-webhook

`imap-idle-webhook` 是一个轻量级 IMAP IDLE 新邮件监听器：它持续连接 IMAP 邮箱，发现未读新邮件后解析邮件摘要，并把事件以签名 JSON webhook 转发给下游系统（例如 Hermes、自动化 Agent、告警平台或自建工作流）。

项目使用 Rust/Tokio 实现原生异步 TCP/TLS、IMAP IDLE 与 webhook POST，适合以容器或 systemd 常驻服务方式部署。

## 项目定位

- **把邮箱变成 webhook 事件源**：无需轮询邮件 API，直接通过 IMAP IDLE 接收新邮件通知。
- **面向自动化接入**：输出稳定 JSON payload，并附带 GitHub 风格 HMAC-SHA256 签名。
- **小型常驻服务**：无数据库、无持久状态，配置来自环境变量，易于容器化。
- **安全默认值**：默认不标记邮件已读，不发送完整 RFC822 原文，不在日志中输出密码或 webhook secret。

## 特性

- 基于 IMAP IDLE 监听 `EXISTS` 新邮件事件，并在连接启动时先处理当前 `UNSEEN` 邮件。
- 支持单个文件夹 `IMAP_FOLDER` 或多个文件夹 `IMAP_FOLDERS` 并发监听。
- 使用 TLS 校验 IMAP 服务器证书；`IMAP_HOST` 必须是 DNS 名称，不能是 IP 字面量。
- 解析常见邮件字段：`Message-ID`、`From`、`To`、`Cc`、`Subject`、`Date`、正文文本。
- 优先提取 `text/plain`；没有纯文本时尝试把 `text/html` 转成文本。
- 正文超过 50000 字符时安全截断，并追加截断提示。
- webhook 使用 `X-Hub-Signature-256: sha256=<hex>` 签名，便于下游校验。
- webhook HTTP 非 2xx、IMAP 断线或命令失败后自动重连。
- 可选 `MARK_SEEN=true`：仅在 webhook 发送成功后把邮件标记为已读。
- 可选 `STARTUP_NOTIFICATION=true`：启动时发送一条 marker webhook，便于部署验证。

## 架构与流程

```text
┌──────────────┐     TLS + IMAP IDLE     ┌───────────────────┐
│ IMAP mailbox │ ───────────────────────▶ │ imap-idle-webhook │
└──────────────┘                          └─────────┬─────────┘
                                                     │ signed JSON POST
                                                     ▼
                                           ┌───────────────────┐
                                           │ Webhook receiver  │
                                           │ Hermes / Agent /  │
                                           │ automation system │
                                           └───────────────────┘
```

运行流程：

1. 从环境变量读取 IMAP 账号、监听文件夹、webhook URL 与 secret。
2. 为每个配置的文件夹启动一个独立 async worker。
3. worker 连接 IMAP TLS，先发送 IMAP `ID` 声明客户端身份，再登录并 `SELECT` 对应 mailbox。
4. 启动时执行 `UID SEARCH UNSEEN`，先发送当前未读邮件。
5. 进入 `IDLE`；收到 `EXISTS` 或超时结束后再次搜索 `UNSEEN`。
6. 对每个 UID 使用 `BODY.PEEK[]` 获取邮件内容，避免默认标记已读。
7. 解析邮件摘要，序列化为紧凑 JSON bytes。
8. 基于同一份 JSON bytes 计算 HMAC-SHA256 签名并 POST 到 `WEBHOOK_URL`。
9. 如 `MARK_SEEN=true`，仅在 POST 成功后执行 `UID STORE +FLAGS.SILENT (\Seen)`。
10. 连接异常时等待 `RECONNECT_DELAY_SECONDS` 后重连。

## 配置项

复制示例配置：

```bash
cp .env.example .env
```

示例 `.env`（请替换为自己的占位值，不要提交真实密钥）：

```dotenv
IMAP_HOST=imap.example.com
IMAP_PORT=993
IMAP_USER=user@example.com
IMAP_PASSWORD=<imap-app-password>
IMAP_FOLDER=INBOX
# 可选：并发监听多个文件夹；设置后优先于 IMAP_FOLDER，使用 | 分隔
# IMAP_FOLDERS=INBOX|Alerts|Project Alpha/Inbox

# 如果 webhook 在宿主机上，Podman 容器里通常使用 host.containers.internal
WEBHOOK_URL=https://hermes.example.com/webhooks/<subscription-id>
WEBHOOK_SECRET=<webhook-shared-secret>
GITHUB_EVENT=email.received

# 可选：发送 IMAP ID，兼容 163/188 等要求客户端声明身份的服务商
# IMAP_ID_NAME=imap-idle-webhook
# IMAP_ID_VERSION=0.1.0
# IMAP_ID_VENDOR=imap-idle-webhook
# IMAP_ID_SUPPORT_EMAIL=user@example.com

IDLE_TIMEOUT_SECONDS=1740
RECONNECT_DELAY_SECONDS=10
MARK_SEEN=false
STARTUP_NOTIFICATION=false
LOG_LEVEL=INFO
```

| 变量 | 必填 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `IMAP_HOST` | 是 | 无 | IMAP 服务器 DNS 名称；用于 TLS 证书校验，不支持 IP 字面量。 |
| `IMAP_PORT` | 否 | `993` | IMAP TLS 端口。 |
| `IMAP_USER` | 是 | 无 | IMAP 登录账号，也会出现在 payload 的 `account` 字段。 |
| `IMAP_PASSWORD` | 是 | 无 | IMAP 密码、app password 或服务专用密码。 |
| `IMAP_FOLDER` | 否 | `INBOX` | 单文件夹监听目标；当 `IMAP_FOLDERS` 有有效项时被覆盖。 |
| `IMAP_FOLDERS` | 否 | 未设置 | 多文件夹列表，使用 `|` 分隔；每项会 trim，空项会忽略，文件夹内部空格和斜杠会保留。 |
| `WEBHOOK_URL` | 是 | 无 | 接收 signed JSON POST 的 URL。 |
| `WEBHOOK_SECRET` | 是 | 无 | 计算 `X-Hub-Signature-256` 的共享密钥。 |
| `GITHUB_EVENT` | 否 | `email.received` | webhook header `X-GitHub-Event` 的值；startup marker 的 payload `event` 也使用该值。 |
| `IMAP_ID_NAME` | 否 | `imap-idle-webhook` | IMAP `ID` 命令中的 `name` 字段；连接建立后会主动发送，兼容要求客户端自报身份的 IMAP 服务。 |
| `IMAP_ID_VERSION` | 否 | 当前 crate 版本 | IMAP `ID` 命令中的 `version` 字段。 |
| `IMAP_ID_VENDOR` | 否 | `imap-idle-webhook` | IMAP `ID` 命令中的 `vendor` 字段。 |
| `IMAP_ID_SUPPORT_EMAIL` | 否 | `IMAP_USER` | IMAP `ID` 命令中的 `support-email` 字段。 |
| `IDLE_TIMEOUT_SECONDS` | 否 | `1740` | 每轮 IMAP IDLE 最长等待秒数；超时后会结束本轮并重新搜索未读邮件。 |
| `RECONNECT_DELAY_SECONDS` | 否 | `10` | worker 出错后重连前等待秒数。 |
| `MARK_SEEN` | 否 | `false` | `true`、`yes` 或 `1` 表示 webhook 成功后标记已读。 |
| `STARTUP_NOTIFICATION` | 否 | `false` | `true`、`yes` 或 `1` 表示启动时发送一条 marker webhook。 |
| `LOG_LEVEL` | 否 | 由运行环境决定 | `env_logger` 日志级别，例如 `INFO`、`DEBUG`。 |

### 多文件夹规则

```dotenv
IMAP_FOLDERS=INBOX|Alerts|Project Alpha/Inbox
```

- 只要 `IMAP_FOLDERS` 解析后至少有一个非空文件夹，就优先于 `IMAP_FOLDER`。
- 每个文件夹对应一个独立 async worker 和一条 IMAP 连接。
- payload 的 `folder` 字段保持为当前 worker 选择的文件夹名。

## Webhook payload 与签名

### HTTP headers

服务发送的 webhook headers：

```http
Content-Type: application/json
X-Hub-Signature-256: sha256=<hmac_sha256_hex>
X-GitHub-Event: email.received
```

签名算法兼容 GitHub 风格 webhook：

```bash
payload='{"event":"email.received","account":"user@example.com"}'
secret='<webhook-shared-secret>'
sig="sha256=$(printf '%s' "$payload" | openssl dgst -sha256 -hmac "$secret" -hex | sed 's/^.* //')"
```

下游必须使用收到的**原始 request body bytes** 计算 HMAC-SHA256，并与 `X-Hub-Signature-256` 做常量时间比较；不要先重新格式化 JSON 再验签。

### 邮件事件 payload

```json
{
  "event": "email.received",
  "account": "user@example.com",
  "folder": "INBOX",
  "uid": 123,
  "message_id": "<message-id@example.com>",
  "from": "Sender <sender@example.com>",
  "to": ["Receiver <receiver@example.com>"],
  "cc": [],
  "subject": "Example subject",
  "date": "Tue, 12 May 2026 08:00:00 +0800",
  "text": "plain text body",
  "raw_rfc822_base64": ""
}
```

字段说明：

- `event`：普通邮件事件当前为 `email.received`。
- `account`：IMAP 登录账号。
- `folder`：触发事件的 IMAP 文件夹。
- `uid`：IMAP UID。
- `message_id`、`from`、`subject`、`date`、`text`：可能为 `null`，取决于邮件内容。
- `to`、`cc`：没有对应 header 时为空数组。
- `raw_rfc822_base64`：当前保持为空字符串，避免 HTML 或附件邮件导致 payload 过大。

### Startup marker payload

当 `STARTUP_NOTIFICATION=true` 时，服务启动后会发送一条 marker：

```json
{
  "event": "email.received",
  "account": "user@example.com",
  "folder": "startup",
  "uid": 0,
  "message_id": null,
  "from": "imap-idle-webhook <startup@localhost>",
  "to": ["user@example.com"],
  "cc": [],
  "subject": "imap-idle-webhook started",
  "date": null,
  "text": "imap-idle-webhook started\nfolder_count: 1\nfolders: INBOX\nidle_timeout_seconds: 1740",
  "raw_rfc822_base64": ""
}
```

marker 不包含 IMAP 密码或 webhook secret，适合用于部署验证。

## 部署方式

### Podman 直接运行

```bash
podman build -t imap-idle-webhook:latest -f Containerfile .
podman run -d --name imap-idle-webhook \
  --env-file .env \
  --add-host=host.containers.internal:host-gateway \
  --restart=unless-stopped \
  imap-idle-webhook:latest
```

查看日志与停止：

```bash
podman logs -f imap-idle-webhook
podman stop imap-idle-webhook
podman rm imap-idle-webhook
```

### podman compose

仓库提供 `compose.yaml`：

```bash
cp .env.example .env
# 编辑 .env，替换为占位环境对应的真实运行值；不要提交 .env
podman compose up -d --build
podman compose logs -f
```

### podman kube play

仓库提供 `k8s-podman.yaml` 作为 Podman/Kubernetes 风格示例。文件中只使用占位值，生产部署前应改为由 Secret 管理真实敏感值。

```bash
podman build -t localhost/imap-idle-webhook:latest -f Containerfile .
podman kube play k8s-podman.yaml
```

### Quadlet / systemd

可以把容器交给 systemd 管理。示例仅作模板，请把路径和占位值替换为部署环境值：

```ini
# ~/.config/containers/systemd/imap-idle-webhook.container
[Unit]
Description=IMAP IDLE to signed webhook bridge
After=network-online.target
Wants=network-online.target

[Container]
Image=localhost/imap-idle-webhook:latest
ContainerName=imap-idle-webhook
EnvironmentFile=%h/.config/imap-idle-webhook/imap-idle-webhook.env
AddHost=host.containers.internal:host-gateway

[Service]
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
```

```bash
systemctl --user daemon-reload
systemctl --user enable --now imap-idle-webhook.service
journalctl --user -u imap-idle-webhook.service -f
```

### NixOS 二进制包

仓库提供锁定依赖的 Nix flake，可在 Linux 上构建并运行 `imap-idle-webhook`：

```bash
# 构建 Nix store 中的二进制包
nix build .#imap-idle-webhook
./result/bin/imap-idle-webhook

# 直接运行
nix run .#imap-idle-webhook
```

发布可分发的单文件 Linux 二进制包：

```bash
nix bundle .#imap-idle-webhook --bundler github:NixOS/bundlers#toAppImage \
  --out-link imap-idle-webhook.AppImage
chmod +x imap-idle-webhook.AppImage
```

将生成的 `imap-idle-webhook.AppImage` 上传到 GitHub Release。目标系统不需要预装 Nix；运行时仍须通过环境变量提供 IMAP 与 webhook 配置。

## Hermes Agent 接入

面向 Hermes Agent 的自动部署/接入说明见：[`docs/hermes-agent-integration.md`](docs/hermes-agent-integration.md)。

接入要点：

1. 在 Hermes 中创建 webhook subscription，取得 URL 与共享 secret。
2. 把 URL 写入 `WEBHOOK_URL`，把 secret 写入 `WEBHOOK_SECRET`。
3. 确认 Hermes 使用 `X-Hub-Signature-256` 对原始 body bytes 验签。
4. 开启 `STARTUP_NOTIFICATION=true` 可验证部署链路。

## 开发检查

本地开发建议通过 `devenv` 进入隔离环境，避免依赖系统全局 Rust 工具链：

```bash
devenv shell cargo test
devenv shell cargo clippy --all-targets -- -D warnings
```

如果不使用 `devenv`，请确保本地 Rust 工具链、Cargo 和系统 CA 证书可用。

## 故障排查

### 启动时报缺少环境变量

错误形如：

```text
Missing required environment variables: IMAP_HOST, IMAP_USER, IMAP_PASSWORD, WEBHOOK_URL, WEBHOOK_SECRET
```

检查 `.env` 或 systemd `EnvironmentFile` 是否存在、变量名是否正确、值是否为空。

### TLS 或 IMAP 连接失败

- `IMAP_HOST` 必须是 DNS 名称，不要填 `127.0.0.1` 这类 IP 字面量。
- 确认 IMAP 服务器支持 TLS 端口（通常是 `993`）。
- 确认容器内有可用 CA 证书；仓库 `Containerfile` 的运行镜像已安装 `ca-certificates`。
- 某些邮件服务需要 app password 或服务专用密码，普通网页登录密码可能无法用于 IMAP。

### 没有收到 webhook

- 查看服务日志：`podman logs -f imap-idle-webhook` 或 `journalctl --user -u imap-idle-webhook.service -f`。
- 临时设置 `STARTUP_NOTIFICATION=true`，重启后确认 Hermes 是否收到 startup marker。
- 确认 `WEBHOOK_URL` 从容器内可访问；当接收端在宿主机时，Podman 通常使用 `http://host.containers.internal:<port>/<path>`。
- 确认下游返回 2xx；非 2xx 会被视为发送失败并触发 worker 重连流程。

### 签名校验失败

- 下游必须使用原始 request body bytes 计算 HMAC，不能重新序列化 JSON。
- 确认 `WEBHOOK_SECRET` 两端完全一致，没有多余空格、换行或 shell 转义问题。
- header 名称为 `X-Hub-Signature-256`，值形如 `sha256=<hex>`。

### 邮件被重复发送

- 默认 `MARK_SEEN=false`，服务重启后会再次发送仍处于 `UNSEEN` 的邮件。
- 如果业务允许，可设置 `MARK_SEEN=true`，使 webhook 成功后标记已读。
- 如果下游需要幂等处理，可使用 `account + folder + uid` 或 `message_id` 去重。

### 邮件正文为空或被截断

- 附件不会作为正文发送。
- 优先提取 `text/plain`，没有纯文本时才从 `text/html` 转换。
- `text` 超过 50000 字符会截断；`raw_rfc822_base64` 当前保持为空字符串。

## 安全注意事项

- 不要把 `.env`、真实 IMAP 密码、app password、OAuth token 或 `WEBHOOK_SECRET` 提交到 Git。
- 文档和示例中的 `<...>`、`example.com`、`change-me` 都是占位值，部署时必须替换。
- webhook URL 建议使用 HTTPS；如果只能在内网使用 HTTP，应限制网络访问范围。
- 下游必须校验 `X-Hub-Signature-256`，并拒绝签名缺失或不匹配的请求。
- IMAP 账号建议使用最小权限邮箱或专用账号，不要复用个人主账号密码。
- 日志会包含账号、host、文件夹、UID 和邮件 subject；subject 可能含敏感信息，请按环境要求限制日志访问。
- `MARK_SEEN=true` 会改变邮箱状态；启用前确认业务接受“发送成功后标记已读”的行为。

## 许可证

本项目使用 MIT License，见 [LICENSE](LICENSE)。

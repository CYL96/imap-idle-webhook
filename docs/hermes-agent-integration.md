# Hermes Agent 部署与接入指南

本文面向 Hermes Agent 或其他自动化部署 Agent，说明如何把 `imap-idle-webhook` 部署为常驻服务，并把邮件事件接入 Hermes webhook subscription。

所有示例都使用占位值。不要把真实账号、密码、app password、token、webhook secret 或内部 URL 写入仓库、日志、任务描述或 PR 评论。

## 目标拓扑

```text
IMAP mailbox ──TLS/IDLE──▶ imap-idle-webhook ──signed JSON POST──▶ Hermes webhook subscription
```

Agent 需要完成三件事：

1. 获取或创建 Hermes webhook subscription。
2. 将 Hermes 返回的 `webhook_url` 和 `webhook_secret` 写入部署环境变量。
3. 启动 `imap-idle-webhook`，并通过 startup marker 或测试邮件验证链路。

## 前置条件

- 可访问本仓库工作目录或源码归档。
- 可用容器运行环境：Podman、podman compose、Podman Quadlet，或可直接运行 Rust release binary 的 systemd 主机。
- 可访问目标 IMAP 服务：
  - `IMAP_HOST` 是 DNS 名称。
  - `IMAP_PORT` 通常为 `993`。
  - 账号已启用 IMAP。
  - 已准备 app password、服务专用密码或同等凭据。
- Hermes 已提供 webhook subscription 能力，或已有一个可接收 GitHub 风格签名 webhook 的 endpoint。
- 部署环境能出站访问 IMAP host 和 Hermes webhook URL。
- Secret 管理方式已确定：环境文件、systemd credentials、Podman secret、Kubernetes Secret、外部 secret manager 等。

## 环境变量清单

最小必填：

```dotenv
IMAP_HOST=imap.example.com
IMAP_USER=user@example.com
IMAP_PASSWORD=<imap-app-password>
WEBHOOK_URL=https://hermes.example.com/webhooks/<subscription-id>
WEBHOOK_SECRET=<webhook-shared-secret>
```

推荐完整配置：

```dotenv
IMAP_HOST=imap.example.com
IMAP_PORT=993
IMAP_USER=user@example.com
IMAP_PASSWORD=<imap-app-password>
IMAP_FOLDER=INBOX
# IMAP_FOLDERS=INBOX|Alerts|Project Alpha/Inbox

WEBHOOK_URL=https://hermes.example.com/webhooks/<subscription-id>
WEBHOOK_SECRET=<webhook-shared-secret>
GITHUB_EVENT=email.received

IDLE_TIMEOUT_SECONDS=1740
RECONNECT_DELAY_SECONDS=10
MARK_SEEN=false
STARTUP_NOTIFICATION=true
LOG_LEVEL=INFO
```

Agent 处理规则：

- `IMAP_PASSWORD` 和 `WEBHOOK_SECRET` 必须来自 secret store 或部署时注入，不要硬编码进镜像或 Git 跟踪文件。
- `STARTUP_NOTIFICATION=true` 适合首次部署验证；验证完成后可按需改回 `false`。
- 多文件夹监听使用 `IMAP_FOLDERS`，以 `|` 分隔；设置后优先于 `IMAP_FOLDER`。
- 如果下游需要避免重复处理，建议 Hermes 侧用 `account + folder + uid` 或 `message_id` 做幂等键。

## Hermes webhook subscribe / secret / URL 接入

### 1. 创建或读取 subscription

在 Hermes 主机上使用官方 CLI 创建动态 webhook subscription。示例把邮件 payload 交给 Hermes Agent 总结，并投递到 Feishu；按实际环境替换 `--deliver`。

```bash
hermes webhook subscribe imap-email \
  --events email.received \
  --description "Summarize inbound IMAP emails" \
  --skills email-summary-template \
  --deliver feishu \
  --secret '<webhook-shared-secret>' \
  --prompt '请使用 email-summary-template，总结下面这封邮件：{payload}'
```

如果不传 `--secret`，Hermes 会自动生成 secret；Agent 必须把生成结果保存到 secret store，不要写入 Git、普通日志或任务摘要。

查看 subscription：

```bash
hermes webhook list
```

Hermes 会显示 route URL，通常形如：

```text
http://localhost:8644/webhooks/imap-email
```

如果 `imap-idle-webhook` 在 Podman 容器内，而 Hermes gateway 在同一宿主机监听 `8644`，部署环境中通常写：

```dotenv
WEBHOOK_URL=http://host.containers.internal:8644/webhooks/imap-email
```

### 2. 写入部署环境

把 Hermes route 和 secret 映射为：

```dotenv
WEBHOOK_URL=http://host.containers.internal:8644/webhooks/imap-email
WEBHOOK_SECRET=<webhook-shared-secret>
GITHUB_EVENT=email.received
```

不要把 secret 输出到普通日志。若 Agent 必须记录操作结果，只记录 subscription 名称、URL host/path 摘要和 secret 存储位置，不记录 secret 原文。

### 3. 验签约定

`imap-idle-webhook` 会发送：

```http
Content-Type: application/json
X-Hub-Signature-256: sha256=<hmac_sha256_hex>
X-GitHub-Event: email.received
```

Hermes 应使用 `WEBHOOK_SECRET` 对收到的原始 request body bytes 做 HMAC-SHA256，转为 hex 后拼接 `sha256=`，再与 `X-Hub-Signature-256` 常量时间比较。

## 部署步骤：Podman 直接运行

适合单机快速部署。

```bash
podman build -t localhost/imap-idle-webhook:latest -f Containerfile .
```

将环境变量写入仅部署用户可读的文件，例如 `/opt/imap-idle-webhook/imap-idle-webhook.env`：

```dotenv
IMAP_HOST=imap.example.com
IMAP_PORT=993
IMAP_USER=user@example.com
IMAP_PASSWORD=<imap-app-password>
IMAP_FOLDER=INBOX
WEBHOOK_URL=https://hermes.example.com/webhooks/<subscription-id>
WEBHOOK_SECRET=<webhook-shared-secret>
GITHUB_EVENT=email.received
STARTUP_NOTIFICATION=true
LOG_LEVEL=INFO
```

建议权限：

```bash
chmod 600 /opt/imap-idle-webhook/imap-idle-webhook.env
```

启动容器：

```bash
podman run -d --name imap-idle-webhook \
  --env-file /opt/imap-idle-webhook/imap-idle-webhook.env \
  --add-host=host.containers.internal:host-gateway \
  --restart=unless-stopped \
  localhost/imap-idle-webhook:latest
```

验证：

```bash
podman ps --filter name=imap-idle-webhook
podman logs -f imap-idle-webhook
```

## 部署步骤：podman compose

适合由 Agent 管理项目目录中的 compose 服务。

1. 准备环境文件：

   ```bash
   cp .env.example .env
   chmod 600 .env
   ```

2. 写入占位值对应的真实部署值（不要提交 `.env`）。

3. 启动：

   ```bash
   podman compose up -d --build
   podman compose logs -f
   ```

4. 更新部署：

   ```bash
   podman compose up -d --build
   ```

## 部署步骤：Quadlet

适合 rootless Podman + systemd user service。

1. 构建镜像：

   ```bash
   podman build -t localhost/imap-idle-webhook:latest -f Containerfile .
   ```

2. 准备环境文件：

   ```bash
   mkdir -p "$HOME/.config/imap-idle-webhook"
   install -m 600 /dev/null "$HOME/.config/imap-idle-webhook/imap-idle-webhook.env"
   ```

   写入示例：

   ```dotenv
   IMAP_HOST=imap.example.com
   IMAP_PORT=993
   IMAP_USER=user@example.com
   IMAP_PASSWORD=<imap-app-password>
   IMAP_FOLDER=INBOX
   WEBHOOK_URL=https://hermes.example.com/webhooks/<subscription-id>
   WEBHOOK_SECRET=<webhook-shared-secret>
   GITHUB_EVENT=email.received
   STARTUP_NOTIFICATION=true
   LOG_LEVEL=INFO
   ```

3. 创建 Quadlet 文件：

   ```ini
   # ~/.config/containers/systemd/imap-idle-webhook.container
   [Unit]
   Description=IMAP IDLE to Hermes webhook bridge
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

4. 启动服务：

   ```bash
   systemctl --user daemon-reload
   systemctl --user enable --now imap-idle-webhook.service
   systemctl --user status imap-idle-webhook.service
   journalctl --user -u imap-idle-webhook.service -f
   ```

## 部署步骤：systemd 直接运行 binary

适合不使用容器的主机。Agent 应先构建 release binary 并放到部署路径。

```bash
cargo build --release
install -m 0755 target/release/imap-idle-webhook /usr/local/bin/imap-idle-webhook
```

环境文件示例：

```dotenv
# /etc/imap-idle-webhook/imap-idle-webhook.env
IMAP_HOST=imap.example.com
IMAP_PORT=993
IMAP_USER=user@example.com
IMAP_PASSWORD=<imap-app-password>
IMAP_FOLDER=INBOX
WEBHOOK_URL=https://hermes.example.com/webhooks/<subscription-id>
WEBHOOK_SECRET=<webhook-shared-secret>
GITHUB_EVENT=email.received
STARTUP_NOTIFICATION=true
LOG_LEVEL=INFO
```

systemd unit 模板：

```ini
# /etc/systemd/system/imap-idle-webhook.service
[Unit]
Description=IMAP IDLE to Hermes webhook bridge
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=/etc/imap-idle-webhook/imap-idle-webhook.env
ExecStart=/usr/local/bin/imap-idle-webhook
Restart=always
RestartSec=10
User=<service-user>
Group=<service-group>
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

启动：

```bash
systemctl daemon-reload
systemctl enable --now imap-idle-webhook.service
systemctl status imap-idle-webhook.service
journalctl -u imap-idle-webhook.service -f
```

## 验证命令

### 1. 验证服务启动

Podman：

```bash
podman ps --filter name=imap-idle-webhook
podman logs --since 10m imap-idle-webhook
```

systemd：

```bash
systemctl --user status imap-idle-webhook.service
journalctl --user -u imap-idle-webhook.service --since "10 minutes ago"
```

典型启动日志会包含 host、account、folder_count、folders、idle timeout、`MARK_SEEN` 和 `GITHUB_EVENT`，不会包含密码或 webhook secret。

### 2. 验证 Hermes 收到 startup marker

首次部署建议设置：

```dotenv
STARTUP_NOTIFICATION=true
```

重启服务后，Hermes 应收到 `folder=startup`、`uid=0`、`subject=imap-idle-webhook started` 的 signed webhook。

### 3. 验证签名算法

如果 Hermes 提供请求回放或调试 body，可用同一 secret 计算：

```bash
payload_file=/tmp/hermes-webhook-body.json
secret='<webhook-shared-secret>'
printf 'sha256=%s\n' "$(openssl dgst -sha256 -hmac "$secret" -hex "$payload_file" | sed 's/^.* //')"
```

结果应与请求 header `X-Hub-Signature-256` 一致。

### 4. 验证真实邮件事件

向被监听邮箱发送一封测试邮件，确认 Hermes 收到：

```json
{
  "event": "email.received",
  "account": "user@example.com",
  "folder": "INBOX",
  "uid": 123,
  "subject": "<test-subject>",
  "raw_rfc822_base64": ""
}
```

## 常见故障

### Hermes 没有收到任何请求

- 检查 `WEBHOOK_URL` 是否为 Hermes 返回的完整 URL。
- 从容器或服务所在主机测试 DNS 与网络连通性。
- 如果 Hermes 在同一宿主机，Podman 容器内通常使用 `host.containers.internal` 访问宿主机端口。
- 开启 `STARTUP_NOTIFICATION=true`，通过启动 marker 排除“没有新邮件”的情况。

### Hermes 返回 401 / 403 / 签名错误

- 确认 Hermes 侧保存的 secret 与 `WEBHOOK_SECRET` 完全一致。
- Hermes 必须用原始 body bytes 验签，不要重新格式化 JSON。
- 确认 header 名称是 `X-Hub-Signature-256`，值包含 `sha256=` 前缀。
- 检查 secret 是否被 shell、YAML 或环境文件意外截断、转义或追加换行。

### IMAP 登录失败

- 确认账号已启用 IMAP。
- 使用 app password 或服务专用密码，不要假设网页登录密码可用。
- 检查 `IMAP_USER` 是否需要完整邮箱地址。
- 检查邮箱服务是否限制来源 IP、地区或安全策略。

### TLS 校验失败

- `IMAP_HOST` 使用 DNS 名称，不要使用 IP。
- 确认服务器证书链被系统 CA 信任。
- 容器镜像需要 CA 证书；仓库 `Containerfile` 已安装 `ca-certificates`。

### 重复事件

- 默认 `MARK_SEEN=false`，未读邮件在服务重启后可能再次发送。
- 可在 Hermes 侧按 `account + folder + uid` 做幂等。
- 如果业务允许，可设置 `MARK_SEEN=true`，让发送成功的邮件标记为已读。

### 多文件夹未生效

- `IMAP_FOLDERS` 使用 `|` 分隔，不是逗号。
- 只要 `IMAP_FOLDERS` 有至少一个非空项，就会覆盖 `IMAP_FOLDER`。
- 文件夹名带空格或斜杠可以保留，例如 `Project Alpha/Inbox`。

## 密钥与日志注意事项

- 不要在 Agent prompt、任务日志、Git diff、issue、PR、README 示例中写入真实 secret。
- 不要提交 `.env`、systemd 环境文件、Hermes API token 或 IMAP app password。
- 若需要输出调试信息，请脱敏：只展示 secret 的存储引用或 hash 摘要，不展示原文。
- 限制环境文件权限：单机部署建议 `chmod 600`。
- 日志可能包含邮件 subject、账号和文件夹名；如 subject 可能包含敏感信息，应限制日志访问。
- 轮换 `WEBHOOK_SECRET` 时，应先更新 Hermes subscription，再重启 `imap-idle-webhook` 使用新 secret；避免两端不一致导致丢事件。

## Agent 完成条件

Agent 自动部署后应返回以下非敏感摘要：

- 部署方式：`podman` / `compose` / `quadlet` / `systemd`。
- 服务状态：running / failed。
- Hermes subscription id 或脱敏 URL（不要包含 secret）。
- 是否收到 startup marker。
- 如失败，返回错误类别和下一步建议，不返回密钥原文。

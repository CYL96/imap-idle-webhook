# AGENTS.md

## 仓库结构
- 单一 Rust crate。二进制入口是 `src/main.rs`，库模块在 `src/lib.rs`。
- 主执行流：`Config::from_env()` → `log_config_summary()` → 可选 `send_startup_notification()` → `listener::run_forever()`。
- 模块分工：
  - `src/listener.rs`：IMAP TLS 连接、`IDLE` 循环、抓取邮件、`MARK_SEEN`、重连。
  - `src/webhook.rs`：紧凑 JSON 序列化、HMAC-SHA256 签名、HTTP POST。
  - `src/config.rs`、`src/email_parse.rs`：纯配置/解析逻辑，适合优先补测试。
- `docs/hermes-agent-integration.md` 是 Hermes/Podman/Quadlet 部署接入手册，不是源码行为规范。

## 事实来源
- 仓库内没有 CI workflow、pre-commit 配置、`opencode.json`，也没有其他 instruction 文件。
- 以 `README.md`、`devenv.nix`、`Cargo.toml` 和现有测试为准；文档与代码冲突时以代码/测试为准。

## 常用命令
- 优先：`devenv shell` 后运行 `test` 或 `check`。
- 直接运行：
  - `cargo test`
  - `cargo fmt --check`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo build --release`
- 完整校验顺序按 `devenv.nix` 的 `check`：`cargo fmt --check` → `cargo clippy --all-targets -- -D warnings` → `cargo test`。
- 聚焦验证：
  - 单个 integration test 文件：`cargo test --test native_async`
  - 单个测试：`cargo test config_from_env_uses_existing_defaults_and_parses_mark_seen`

## 测试与改动陷阱
- `tests/native_async.rs` 是静态守卫：它会扫描 `src/listener.rs`、`src/webhook.rs` 和 `Cargo.toml`，确保 live I/O 仍然是原生异步实现。
- `src/listener.rs` 自带单元测试；其中 `tls_config_builds_with_single_crypto_provider` 依赖系统 CA 证书，精简环境里可能失败。
- 需要改环境变量相关测试时，沿用 `temp-env` 做隔离，不要让测试互相污染环境。

## 硬约束
- 不要引入 `spawn_blocking`、`std::net::TcpStream`、`std::thread`、阻塞式 std I/O adapter 或 `reqwest::blocking`。
- `reqwest` 当前固定为 `default-features = false, features = ["rustls"]`；不要顺手打开 blocking 或默认 TLS 特性。
- `IMAP_HOST` 必须保持为 DNS 名称，不能改成 IP literal；TLS `ServerName` 校验依赖这一点。
- `IMAP_FOLDERS` 使用 `|` 分隔，`trim` 后忽略空项；只要解析后还有值，就覆盖 `IMAP_FOLDER`。
- 连接建立后会先发送 IMAP `ID` 命令；163/188 这类要求客户端自报身份的服务依赖它，默认值来自 crate 名称/版本和 `IMAP_USER`，也可用 `IMAP_ID_*` 环境变量覆盖。
- `IMAP_PASSWORD`、`WEBHOOK_SECRET` 不能进入日志、startup marker、示例提交内容或任务回执。

## 修改时的最小验证建议
- 改 IMAP 命令流、`IDLE` 行为、异步 I/O 或 Cargo feature：先跑 `cargo test --test native_async`，再跑完整校验。
- 改 webhook payload、签名或 startup marker：至少补/跑 `tests/webhook.rs` 与 `tests/startup.rs`。
- `Cargo.toml` 的 release profile 是体积优先配置（`opt-level = "z"`、`lto = true`、`panic = "abort"`、`strip = "symbols"`）；除非任务明确涉及构建产物权衡，否则不要改。

## 安全与部署备注
- `.env` 不应提交；部署时用 `.env.example` 作为模板。
- 首次联调可用 `STARTUP_NOTIFICATION=true` 验证链路，完成后再按需关闭。

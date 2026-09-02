# Claude Code 与 Codex 的凭据处理调研

- 调研日期：2026-09-01
- 适用项目：Vortex CLI Agent
- 状态：已采纳，对应设计见 `docs/design/credentials.md`
- 调研对象版本：Claude Code 2.1.251（本机 `--help`）、Codex 官方配置与认证文档

## 方法与可信度

结论来自两方官方文档与本机 Claude Code 的帮助输出。**未做源码审计**——"两者都不读 `.env`"这一条的依据是：两份官方配置参考中均无 dotenv 相关字段，且 Claude Code 文档中 `.env` 唯一一次出现是作为建议禁止读取的路径。这是文档级证据，强度低于源码验证，引用时应如实说明。

## 结论

两个参照物都不把 `.env` 当作凭据来源，都以"环境变量 + 配置中只写变量名"为主路径。但两者**都提供了凭据落盘或进钥匙串的登录路径**，这一点与 Vortex 当前设计不同。

调研过程中发现 Vortex 现有文档把两条不同强度的约束混写为一句，需要在设计中拆开（见末节）。

## 三方对照

| | 密钥来源 | 读 `.env` | 凭据落盘 | 配置中指定变量名 |
| --- | --- | --- | --- | --- |
| Codex | `OPENAI_API_KEY` 环境变量，或 ChatGPT 登录 | 否 | 是：`~/.codex/auth.json` 或系统钥匙串 | 是：`env_key` |
| Claude Code | `ANTHROPIC_API_KEY`、`apiKeyHelper`、OAuth | 否 | 是：钥匙串 | 有 `env` 设置项 |
| Vortex 现设计 | 仅进程环境变量 | 否 | 否（已否决 `vortex login`） | 计划中的 `api_key_env` |

## `.env` 的处理

两份官方配置参考均未出现 dotenv 相关字段。

Claude Code 文档中 `.env` 的唯一一次出场，是在权限配置示例里作为**建议禁止 Agent 读取**的路径：

```json
"deny": ["Read(./.env)", "Read(./.env.*)"]
```

即在其模型中，`.env` 是需要防护的秘密文件，而非凭据入口。这与 Agent 类工具的运行方式一致：它们要在任意工作区中运行，自动加载当前目录的 `.env` 会让陌生仓库得以注入凭据与 `base_url`。

## Codex：`env_key` 与凭据存储

配置参考原文：

> `model_providers.<id>.env_key` —— "Environment variable supplying the provider API key."
>
> `model_providers.<id>.env_http_headers` —— "HTTP headers populated from environment variables when present."
>
> `model_providers.<id>.experimental_bearer_token` —— "Direct bearer token for the provider (discouraged; use `env_key`)."

配置文件中写的是**环境变量名**而非密钥值。直接写入 token 的字段虽然存在，但被官方标注为不推荐。

认证路径有两条：`codex login` 走浏览器 ChatGPT 登录；API key 则通过 stdin 传入：

```bash
printenv OPENAI_API_KEY | codex login --with-api-key
```

凭据缓存位置由 `cli_auth_credentials_store` 控制，可选文件（`CODEX_HOME` 下的 `auth.json`）、系统钥匙串或自动选择。文档强调 `~/.codex/auth.json` "contains access tokens"，应当"like a password"对待，不得提交版本控制。

## Claude Code：`apiKeyHelper` 与 `env`

设置参考原文：

> `apiKeyHelper` —— "Generate the API credential with your own command"
>
> `env` —— "Set environment variables for every session and its subprocesses"

`apiKeyHelper` 是一条 Codex 没有的路径：由用户自定义命令**动态生成**凭据，适合短期令牌与轮换场景。`env` 则是给会话及其子进程统一设置环境变量的通道，不是密钥入口。

本机 `claude --help` 中 `--bare` 的说明可佐证入口范围：

> "Anthropic auth is strictly ANTHROPIC_API_KEY or apiKeyHelper via --settings (OAuth and keychain are never read)."

## 两条容易混淆的约束

调研澄清了一处此前被混写的区别：

1. **密钥不写进项目配置文件** —— 两个参照物都遵守。项目配置会被提交、被 review、被同步，写入密钥等于泄露。这是通用安全底线。
2. **密钥完全不落盘** —— 两个参照物都**不**遵守。它们把凭据放进系统钥匙串或受保护的用户级文件，只是不放在工作区里。

Vortex 现有文档（`README.md`、`AGENTS.md`、`docs/design/cli-surface.md`）将两者表述为一句"API 密钥只能从环境变量读取"，实际执行的是更严格的第 2 条。

该选择本身自洽——约束在设计之前就已确立——但代价需要记录：**OAuth 订阅登录依赖凭据落盘**，Codex 的 ChatGPT 登录与 Claude Code 的 OAuth 都是如此。只要 Vortex 仅使用 API key，代价为零；若将来要接订阅账号，这条约束必须重新评估。

## 对 Vortex 的影响

1. `.env` 定位为开发者本地注入手段，产品二进制不读取——与两个参照物一致。
2. Provider 配置中记录环境变量名而非密钥值，形状照 Codex 的 `env_key`。
3. 在设计文档中拆开上述两条约束，明确当前采用更严格的一条及其代价。
4. `apiKeyHelper` 式的凭据生成命令是一条值得记录的备选路径，当前不实现。

## 主要资料

- [Codex 配置参考](https://learn.chatgpt.com/docs/config-file/config-reference)
- [Codex 认证文档](https://learn.chatgpt.com/docs/auth)
- [Claude Code settings](https://code.claude.com/docs/en/settings)
- [Claude Code settings reference](https://code.claude.com/docs/en/settings-reference)
- 本机 `claude --help`（Claude Code 2.1.251），2026-09-01 实测

# 密钥与凭据访问设计

本文档定义 Vortex 如何获取 Provider 的 API 密钥：密钥从哪里来、`.env` 的定位、配置文件中记录什么、日志如何脱敏。

架构层面的 crate 划分见 [`../architecture/overview.md`](../architecture/overview.md)；命令行参数契约见 [`cli-surface.md`](cli-surface.md)；本文档取舍的依据见 [`../research/credential-handling-research.md`](../research/credential-handling-research.md)。

## 设计前提

`README.md` 与 `AGENTS.md` 中的原有约定："API 密钥只能从环境变量读取，日志和错误信息必须脱敏。"

该表述实际包含两条强度不同的约束，本设计将其拆开并分别表态：

| 约束 | 本设计 | 说明 |
| --- | --- | --- |
| 密钥不写进项目配置文件 | **采纳** | 项目配置会被提交、review、同步，写入密钥等于泄露。通用安全底线。 |
| 密钥完全不落盘 | **采纳（更严格）** | Claude Code 与 Codex 均不遵守此条，它们把凭据存入系统钥匙串或用户级文件。见「代价」一节。 |

## 密钥来源

**进程环境变量是唯一来源。** `vortex` 通过 `std::env::var` 读取，不接受其他任何输入路径：

- 不从 TOML 配置读取密钥值
- 不接受命令行参数传入密钥（会进入 shell 历史与进程列表）
- 不提供 `vortex login` 一类的密钥管理子命令（见 `cli-surface.md` 的已否决方案）
- 不读取 `.env` 文件（下节详述）

## `.env` 的定位

**`.env` 是开发者本地把密钥注入进程环境的手段，不是 Vortex 的凭据来源。产品二进制在任何情况下都不查找、不读取 `.env`。**

理由是 Agent 类工具的运行方式：它天生要在任意工作区中运行。若二进制自动加载当前目录的 `.env`，那么在陌生仓库中运行 `vortex` 就会加载该仓库携带的 `.env`；而 Provider 配置含 `base_url`，恶意 `.env` 可将请求连同对话内容一并导向第三方，全程无提示。

Claude Code 的官方权限示例把 `.env` 列为**建议禁止 Agent 读取**的路径，与此判断一致。

约定的用法：

```bash
# .env.example —— 提交进仓库，只有变量名没有值
DEEPSEEK_API_KEY=

# .env —— 本地文件，不提交（.gitignore 已覆盖 .env 与 .env.*）
DEEPSEEK_API_KEY=sk-...
```

```bash
# 注入：-a 使 source 进来的变量自动 export
set -a; . ./.env; set +a
```

`.env` 中**只放密钥**。`base_url`、模型名、超时等非秘密配置一律走分层 TOML——它们需要被版本化和 review，散落在 `.env` 中既无必要也难以追踪。

## Provider 配置形状

配置文件记录**环境变量名**，不记录密钥值：

```toml
[providers.deepseek]
kind         = "openai-chat"          # OpenAI 兼容的 Chat Completions
base_url     = "https://api.deepseek.com"
model        = "deepseek-v4-flash"
api_key_env  = "DEEPSEEK_API_KEY"     # 变量名，不是值
```

命名说明：Codex 中的对应字段为 `model_providers.<id>.env_key`；本项目用 `api_key_env`，语义相同，取其在脱离上下文时更自解释。

配置解析阶段**不读取环境变量**，只记录变量名；实际取值推迟到构造 Provider 时。这样 `vortex config` 一类的配置检查、以及配置的单元测试，都不需要真实密钥存在。

密钥缺失时的错误信息必须指出**缺少哪个环境变量名**，而非笼统的"认证失败"——变量名不是秘密，且这是最常见的上手障碍。

## 脱敏要求

`AGENTS.md` 要求日志和错误信息脱敏，落到具体规则：

- 密钥值不得出现在任何日志级别、任何错误链、任何 `--json` 事件中
- `Authorization` 等认证头在记录请求时必须掩码，只保留前缀（如 `Bearer sk-***`）
- 持有密钥的类型自行实现 `Debug`，输出固定占位符，**不要 `#[derive(Debug)]`**——派生实现会在 `dbg!` 或错误链中原样打印密钥
- 相应地需要一条测试：断言持密类型的 `Debug` 与 `Display` 输出中不含密钥原文

第三条是最容易在调试时被绕过的：临时加一句 `dbg!(&request)` 就会泄露，而派生的 `Debug` 让它看起来无害。用类型层面的实现堵住，比依赖纪律可靠。

## 代价

采纳"密钥完全不落盘"意味着 **OAuth 订阅登录不可实现**。Codex 的 ChatGPT 登录与 Claude Code 的 OAuth 都依赖凭据落盘（系统钥匙串或受保护的用户级文件）。

只要 Vortex 仅使用 API key，此代价为零。若将来要支持订阅账号登录，本约束必须重新评估——届时应参照两个参照物的做法：凭据进系统钥匙串或用户级受保护文件，而**不是**放宽"不写进项目配置文件"那一条。

## 分阶段落地

| 阶段 | 内容 |
| --- | --- |
| 1 打通闭环 | `std::env::var` 读取密钥、`api_key_env` 配置字段、密钥缺失时报出变量名、持密类型的自定义 `Debug` |
| 2 工具与沙箱 | 请求头掩码、`--json` 事件流的脱敏校验 |
| 待定 | 凭据生成命令（`apiKeyHelper` 式）、`env_http_headers` 式的自定义请求头 |

## 待定项

- **凭据生成命令**：Claude Code 的 `apiKeyHelper` 由用户自定义命令动态生成凭据，适合短期令牌与轮换。它不违反"不落盘"约束，但需要先明确子进程的执行与超时策略。
- **`env_http_headers`**：Codex 支持从环境变量填充自定义请求头。接入需要认证头的自建网关时会用到，当前无用例。
- **多 Provider 同时配置**：目前设想是 `--provider <NAME>` 选择其一，未考虑同一次会话中混用多个 Provider 的情形。

## 已否决的方案

| 方案 | 否决理由 |
| --- | --- |
| 产品二进制读取 `.env` | 工作区中的 `.env` 可被陌生仓库携带，配合 `base_url` 构成静默劫持通道 |
| 配置文件中直接写密钥值 | 项目配置会被提交与同步；Codex 虽保留 `experimental_bearer_token`，官方亦标注为不推荐 |
| 命令行参数传入密钥 | 进入 shell 历史与进程列表，任何同机用户可见 |
| `vortex login` 子命令 | 与"密钥不落盘"约束冲突，详见 `cli-surface.md` |

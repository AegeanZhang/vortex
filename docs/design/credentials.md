# 密钥与凭据访问设计

本文档定义 Vortex 如何获取模型 Provider 与搜索工具的 API 密钥：密钥从哪里来、`.env` 的定位、配置文件中记录什么、日志如何脱敏。

架构层面的 crate 划分见 [`../architecture/overview.md`](../architecture/overview.md)；配置与路径契约见 [配置设计](configuration.md) 和 [安装设计](installation.md)，命令行参数见 [`cli-surface.md`](cli-surface.md)；本文档取舍的依据见 [`../research/credential-handling-research.md`](../research/credential-handling-research.md)。

## 设计前提

README 与 [架构总览](../architecture/overview.md) 约定："API 密钥只能从环境变量读取，
日志和错误信息必须脱敏。" AGENTS.md 只保留修改边界与规范入口。

该表述实际包含两条强度不同的约束，本设计将其拆开并分别表态：

| 约束 | 本设计 | 说明 |
| --- | --- | --- |
| 密钥不写进项目配置文件 | **采纳** | 项目配置会被提交、review、同步，写入密钥等于泄露。通用安全底线。 |
| Vortex 不持久化凭据 | **采纳（更严格）** | 产品不保存 API key 或登录令牌；开发者自行注入环境变量与产品凭据存储是不同边界。见「代价」一节。 |

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

目标是 `.env` 只注入密钥，base_url、模型名、超时等普通配置走分层 TOML。当前 TOML
加载尚未接入，Provider 地址/模型固定在 CLI；运行时日志路径由 VORTEX_HOME 环境变量
提供，可以在被 Git 忽略的本地启动脚本中导出。不要把下面的目标配置示例当作现有解析能力。

待实现的目标 HOME 默认是系统用户主目录下 `.vortex`，只有显式覆盖时才需要 VORTEX_HOME；不会因此自动读取 `.env`、新增凭据文件或搬迁已有开发目录。项目配置只在启动 cwd 下检查，不能改变 Provider 地址或密钥来源。

## 搜索服务凭据

当前 TUI 装配秘塔搜索时，CLI 从进程环境读取 `METASO_API_KEY`，与模型的
`QWEN_API_KEY` 分开管理。缺失或无效时明确报错，不静默回退到 Mock；旧 `exec` 路径
未接入搜索工具，不要求该搜索密钥。

搜索工具不自行查找 `.env`，也不接受命令行或配置文件中的密钥值。秘塔认证头标记为
sensitive，持密工具使用手写脱敏 `Debug`；搜索工具日志不输出密钥、认证头、查询正文或原始
响应正文，错误不透传可能含敏感信息的 HTTP 错误链或服务端正文。

搜索地址与请求边界当前固定在工具实现中，尚未接入 TOML 配置。未来若增加搜索 profile，
同样只保存密钥的环境变量名。请求与输出限制见 [Agent loop 与搜索工具](agent-loop.md)。

## Provider 配置形状

以下是 [配置设计](configuration.md) 约定的目标片段，尚未接入当前代码。Provider 只定义连接和凭据变量名；模型与请求选项单独定义，不记录密钥值：

```toml
[providers.deepseek]
kind         = "openai-chat"          # OpenAI 兼容的 Chat Completions
base_url     = "https://api.deepseek.com"
api_key_env  = "DEEPSEEK_API_KEY"     # 变量名，不是值

[models.deepseek-pro]
provider = "deepseek"
model = "deepseek-v4-pro"             # 示例，须确认账户实际可用名称
display_name = "DeepSeek Pro"
supports_tools = true
request_options = "plain"
```

命名说明：Codex 中的对应字段为 `model_providers.<id>.env_key`；本项目用 `api_key_env`，语义相同，取其在脱离上下文时更自解释。

配置解析阶段**不读取环境变量**，只记录变量名；实际取值推迟到构造 Provider 时。这样 `vortex config` 一类的配置检查、以及配置的单元测试，都不需要真实密钥存在。

本轮不新增 `vortex config` 或登录命令。可以在用户层并列定义多个 Provider，通过所选模型 profile 引用其一；只读取实际选中模型的密钥。受限项目文件不能修改连接和凭据变量名，普通配置解析与会话内模型选择不承担认证流程。

密钥缺失时的错误信息必须指出**缺少哪个环境变量名**，而非笼统的"认证失败"——变量名不是秘密，且这是最常见的上手障碍。

## 脱敏要求

架构要求日志和错误信息脱敏，落到具体规则：

- 密钥值不得出现在任何日志级别、任何错误链、任何 `--json` 事件中
- `Authorization` 等认证头在记录请求时必须掩码，只保留前缀（如 `Bearer sk-***`）
- 持有密钥的类型自行实现 `Debug`，输出固定占位符，**不要 `#[derive(Debug)]`**——派生实现会在 `dbg!` 或错误链中原样打印密钥
- 相应地需要一条测试：断言持密类型的 `Debug` 与 `Display` 输出中不含密钥原文

第三条是最容易在调试时被绕过的：临时加一句 `dbg!(&request)` 就会泄露，而派生的 `Debug` 让它看起来无害。用类型层面的实现堵住，比依赖纪律可靠。

当前 SecretString 使用手写脱敏 Debug，TUI slash 接入时已移除提交分支的输入正文 TRACE。
这只移除了一个已知风险点，不是全链路隐私审计通过；启用详细日志前仍应确认错误链和
新增诊断不会输出正文或凭据，不能把持密类型脱敏等同于 Prompt/工具结果的日志安全。
实际日志入口见 [指南](../guide/getting-started.md#日志与本地配置)。

## 代价

当前 Vortex 只实现从进程环境读取 API key，不实现 OAuth 或订阅登录，也不持久化登录凭据。**OAuth 本身不要求凭据落盘**：可以仅在当前进程内保存令牌，但无法因此自动获得跨进程的登录复用能力。

若将来要提供持久化订阅登录，应独立设计授权流程、令牌刷新与凭据存储，重新评估当前凭据策略；可参考系统钥匙串或受保护的用户级凭据文件，而**不是**放宽"不写进项目配置文件"那一条。模型配置与会话中的模型选择不承担登录职责；能否使用订阅权益还取决于服务方支持的授权接口与账号权限，不是增加一个配置字段就能实现。

## 分阶段落地

| 阶段 | 内容 |
| --- | --- |
| 1 打通闭环 | `std::env::var` 读取密钥、`api_key_env` 配置字段、密钥缺失时报出变量名、持密类型的自定义 `Debug` |
| 2 工具与沙箱 | 请求头掩码、`--json` 事件流的脱敏校验 |
| 待定 | 凭据生成命令（`apiKeyHelper` 式）、`env_http_headers` 式的自定义请求头 |

## 待定项

- **凭据生成命令**：Claude Code 的 `apiKeyHelper` 由用户自定义命令动态生成凭据，适合短期令牌与轮换。它不违反"不落盘"约束，但需要先明确子进程的执行与超时策略。
- **`env_http_headers`**：Codex 支持从环境变量填充自定义请求头。接入需要认证头的自建网关时会用到，当前无用例。
- **独立 Provider 选择参数**：目标 CLI 中的 `--provider` 与模型 profile 引用的组合规则以后再定；本轮仅通过 `-m` 和 `/model` 选择已有模型，不同时提供第二套 Provider 选择入口。

## 已否决的方案

| 方案 | 否决理由 |
| --- | --- |
| 产品二进制读取 `.env` | 工作区中的 `.env` 可被陌生仓库携带，配合 `base_url` 构成静默劫持通道 |
| 配置文件中直接写密钥值 | 项目配置会被提交与同步；Codex 虽保留 `experimental_bearer_token`，官方亦标注为不推荐 |
| 命令行参数传入密钥 | 进入 shell 历史与进程列表，任何同机用户可见 |
| 本轮实现 `vortex login` 子命令 | 当前仅支持环境变量 API Key，不设计登录与凭据管理；不是断言 OAuth 技术上必须落盘 |

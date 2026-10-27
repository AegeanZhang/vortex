# Web Search 能力与服务选型调研

- 调研日期：2026-09-27
- 补充调研：2026-10-02，新增秘塔 AI 搜索、中国大陆直连条件、当前 Qwen 的模型侧搜索、DeepSeek V4 Pro 搜索实测，以及 Codex/Claude Code/DeepSeek Harness/WorkBuddy 的多模型搜索设计；除明确补充的内容外，价格与额度仍为原调研基线
- 接入反馈：2026-10-05，作者反馈编译后的程序已成功执行秘塔搜索；证据边界见 §5.5
- 源码补充：2026-10-21，只读核对用户提供的 Claude Code 历史快照，区分云端 WebSearch 与本地 Grep/Glob；未运行快照或调用真实搜索，见 §3.4 的快照核对
- 范围：Agent Web Search 的执行形态、国内外服务候选、可靠性与安全边界
- 定位：保留实现前调研与候选建议；后续接入状态单独注明，以设计文档和根 README 为准
- 关联设计：[`Message 与上下文构建`](../design/runtime/message-building.md)、[`Session Runtime`](../design/runtime/session-runtime.md)
- 后续执行契约：[Agent loop](../design/runtime/agent-loop.md)、[网页搜索工具](../design/tools/web-search.md)
- 关联架构：[`架构总览`](../architecture/overview.md)、[`关键依赖与技术选型`](../architecture/technology-stack.md)

本文保留 2026-09-27 调研时的基线与候选建议，“当前一次 Prompt 对应一次模型流”等描述
是当时的问题背景，不是最新完成状态。工具版契约见上述设计，进度与验证结果见根 README；
秘塔现已接入 TUI 的 Client Tool 路径，正常搜索由作者反馈跑通；Tavily 仍是候选。
此状态不代表所有异常路径、费用或中国大陆无代理条件已验证。
除明确标注的补充内容外，价格和额度不因文档更新而重新核验。

## 1. 结论

1. Web Search 不只是一次 HTTP 请求，而是 Agent Tool loop 的第一个真实用例。Vortex 当前“一次 Prompt 对应一次模型流”的运行方式，需要先演进为“一个 Turn 包含多个模型步骤和工具步骤”。
2. 搜索存在两种不同执行形态：Vortex 调用独立搜索 API 的 Client Tool，以及模型厂商在一次请求内部完成的 Provider-hosted Search。两者可以共享内部事件语义，但不能共用同一执行实现。
3. 原调研推荐 Tavily，依据接口与免费额度；2026-10-02 补充“国内运行、不依赖代理”的约束后，建议优先验证秘塔和腾讯云，百度/博查作为同类候选；Tavily 保留为已有账户的对照，直连通过后再提高优先级。任何后端首版都只使用检索，不购买服务端生成答案；当时是候选建议，2026-10-05 的秘塔接入反馈见 §5.5。
4. 中国大陆已有可直接使用的同类服务。腾讯云联网搜索 API 和百度 AI 搜索可作为后续独立后端；阿里云百炼联网搜索更适合作为 Qwen Provider 的托管能力，而不是通用搜索后端。2026-10-02 补充将秘塔加入优先评测名单；后续项目已采用其独立搜索 API，不等于采用秘塔问答或 MCP。
5. 首版必须同时限制搜索次数、结果数量、输出大小和超时，并把查询隐私、Prompt Injection、来源引用和真实 API 测试费用纳入设计。
6. “海外 API”不等于“一定需要代理”，“国内产品”也不等于所有网络均可达。国内候选通常按直连路线验证；海外候选未取得目标网络实测前，不应作为无代理首版默认依赖。分类与验证方法见 §5.6。
7. 当前 `qwen3.8-max` 可尝试百炼内置搜索：基础 Chat Completions 路线最接近现有实现，适合先验证“联网回答”；但它不替代 Vortex 自己的工具循环，且搜索来源、预算和可观测性有区别，见 §5.3。
8. 当前 DeepSeek 官方 `deepseek-v4-pro` 在 Responses 与 Anthropic 接口中实测返回了服务端搜索结果；现有 Chat Completions 路径未启用该能力。Responses 实测与官方兼容表存在冲突，应作为已验证样本、尚待稳定契约确认的能力，而非直接切换产品接口，见 §5.7。

## 2. 能力边界

三个容易混淆的能力应分开建模：

| 能力 | 输入与输出 | 首版范围 |
| --- | --- | --- |
| `web_search` | 查询词 → 标题、URL、相关片段及元数据 | 是 |
| `web_fetch` | URL → 提取后的网页正文 | 否，后续单独设计 SSRF 与内容限制 |
| Deep Research | 任务 → 多轮检索、阅读、分析和报告 | 否，依赖完整 Tool loop、预算与后台任务 |

搜索服务只负责发现和检索；最终回答仍由 Vortex 当前选定的模型生成。若直接购买搜索厂商的答案或研究接口，会掩盖 Tool call、Tool result、上下文回填和多步调度等本项目希望学习的实现细节。

## 3. 编码与办公 Agent 的搜索实现对照

本节于 2026-10-02 补充官方资料与源码核对；没有运行这些产品、切换用户配置、安装客户端
或执行新的付费搜索。本地 Codex 基线仍为 `5bf132cd527311eb61bbec46562e3890eb49df80`
（2026-09-16）；Harness 为查询时官方文档及公开 `master` 源码，不是固定发行版验收。
§3.4 的历史快照核对于 2026-10-21 另行补充，不代表重新验证本节其他产品或最新平台能力。

### 3.1 先区分“本地”的两种含义

- **本地文件搜索**：Grep、Glob、Shell、代码索引等，搜索仓库或工作目录，不是 Web Search。
- **客户端执行的 Web Search**：模型发出 Function ToolCall，Agent 在本机或执行宿主调度
  工具，再访问远端搜索 API；客户端执行不意味着索引、检索也在本地。
- **Provider-hosted Search**：模型 API 收到内置搜索工具后，在服务端检索并继续生成，
  客户端接收搜索事件、来源和正文。

“产品自有云端搜索”描述谁提供后端，“Client Tool/Hosted Tool”描述谁执行调用循环，
它们不是互斥分类。一个 Client Tool 也可以调用另一个模型的 hosted search，再只把来源
交回主模型；Harness 的 DeepSeek backend 就是此类组合。浏览器搜索页面自动化、MCP
搜索与上述默认工具也应分别标注，不能看到 `WebSearch` 名称就判断实际网络路径。

### 3.2 总览：切换模型不等于保留搜索

| 产品/形态 | 可确认的 Web Search 设计 | 自定义模型/API Key 下的边界 |
| --- | --- | --- |
| Codex CLI、桌面本地会话 | 常规 hosted search；另有开发中的 standalone 工具执行路径 | Provider 要支持相应搜索协议；不会因有第三方 Key 就自动获得 OpenAI 搜索 |
| Claude Code CLI、Desktop Code | 内置 WebSearch 调用 Anthropic 搜索后端；外部搜索可经 MCP | 取决于目标平台的 server-tool 支持；任意兼容对话接口不保证可搜索 |
| DeepSeek Harness CLI/Web/桌面 | 统一 `web_search` 工具加可替换 backend；Exa、Perplexity、DeepSeek | 主 LLM 与搜索 backend 分开；各 backend 需要自己的可用凭据与配置 |
| WorkBuddy 桌面 | 官方更新日志确认元宝搜索；支持自定义模型与连接器 | 未公开搜索路由/鉴权完整契约，不能保证所有自定义模型下免费、无需产品登录可用 |
| CodeBuddy Code CLI（另列） | 内置 WebSearch；发布说明确认工具访问平台服务 | 模型 URL/Key 与平台工具可用性应分别验收，不能直接套用到 WorkBuddy |

下文给出证据；前三项可确认执行边界，后两项的底层索引、具体 API 与认证方式仍有空白。

### 3.3 Codex：hosted 与 standalone 两条路径

官方说明：本地会话默认 cached search 使用 OpenAI 维护的索引，不是本机浏览器缓存；
live 模式允许实时联网。搜索是 hosted 工具，与本地命令的网络权限隔离。
[Codex 搜索说明][codex-search]

本地源码确认：

- `codex-rs/core/src/tools/hosted_spec.rs` 构造 `ToolSpec::WebSearch`；
  `core/src/tools/spec_plan.rs` 根据 Provider 能力和当前模式选择 hosted 或 standalone，
  不是无条件同时提供两条工具。
- standalone 的 `ext/web-search/src/tool.rs` 在客户端执行 `web.run`，通过
  `codex-api/src/endpoint/search.rs` 的 `SearchClient` 请求 `alpha/search`；它从当前
  Provider 取得 API 地址与认证，不直接抓 Google，也不是给所有第三方模型固定配一个
  无需授权的 OpenAI Search Key。
- `features/src/lib.rs` 将 `standalone_web_search` 标为开发中、默认关闭；第三方 Provider
  还需声明 `supports_standalone_web_search`。官方同样提醒单独设置此字段不会启用功能，
  还要求兼容 endpoint、模型与 runtime 支持。[自定义 Provider 配置][codex-provider-config]

因此，官方 OpenAI Key、转发 OpenAI 完整 Responses 的网关，以及只兼容模型推理的第三方
接口是三种情况。第三方提供 hosted search 时可由对应服务执行；只有对话/函数调用兼容
时不能保证内置搜索，MCP 可作为独立扩展，但不会凭空提供搜索账户。
当前官方文档要求自定义 Provider 兼容 Responses，不应把 Chat Completions-only 服务
当作完整 Codex backend。[模型与协议说明][codex-models]

桌面本地 Codex 与 CLI 共用 `config.toml`，不是桌面自动另选 Google 的证据；Cloud、
浏览器及其他工具有各自边界，不能从本地配置推断云端同样生效。公开资料也不支持从
“OpenAI 维护索引”推断其所有底层数据来源。

### 3.4 Claude Code：客户端入口，受服务端搜索能力约束

相比原调研，当前官方工具文档已明确：WebSearch 请求 Anthropic 搜索 backend，返回标题
与 URL；阅读网页需后续 WebFetch。搜索 backend 不可配置成任意厂商，其他搜索后端应通过
MCP 工具接入。这取代原来“无法确认其 backend”的笼统表述，但不意味着底层搜索引擎
厂商已公开。[Claude Code 工具说明][claude-tools]

同一文档列出了平台差异：Claude API 与 Claude Platform on AWS 支持搜索；Microsoft
Foundry 要求 Anthropic 托管的 deployment，Azure 托管不支持 server tools；Google Cloud
Agent Platform 的适用 Claude 模型支持；Amazon Bedrock 不暴露该服务端工具。
不能把 Claude Platform on AWS 与 Amazon Bedrock 混为一谈。

切换 Key/endpoint 时：

1. 仍使用支持搜索的 Claude 平台：内置工具依赖该平台的服务端能力、权限和计费。
2. 自建网关/第三方 Messages 兼容接口：除正文、函数调用外，还须支持实际发出的搜索
   请求与结果块。仅宣称“Anthropic-compatible”不能保证搜索；本次未抓包验证任意网关。
3. DeepSeek 官方兼容接口：DeepSeek 明确声明支持 Claude Code Web Search，并由其 API
   完成搜索。§5.7 已直接验证 DeepSeek 的服务端搜索，但没有运行 Claude Code 来验收
   完整链路；不把这个例子推广到所有第三方服务。[DeepSeek 集成说明][deepseek-claude-code]
4. 任意主模型加独立检索：可通过 MCP 搜索工具解耦，须另行配置服务/凭据；它不是把内置
   WebSearch 的 backend 开关改掉。

Desktop 的 **Code tab** 与 CLI 使用同一底层引擎。当前 Desktop 文档提供第三方平台/
网关部署入口，但不是任意个人 Key 都有相同 GUI 与功能的保证；也不能把 Code tab 的
结论直接套到 Chat/Cowork。[桌面功能与部署边界][claude-desktop]

#### 历史源码快照核对（2026-10-21）

证据来自用户指定的本地源码快照。用户说明它是 2026 年 3 月流出版本，快照随附 README
自述公开暴露日期为 2026-03-31；真实性、完整性和授权未独立核实，不视为官方开源仓库
或最新实现。本轮只读源码，不安装依赖、不执行快照、不调用真实 API，也不复制实现代码。
本机绝对路径只保留在未入库的 `AGENTS.local.md`；下表路径相对于包含 `src/` 的快照根目录。

**结论：网页搜索由云端执行，本地文件搜索主要调用本机成熟搜索工具。** “搜索发生在
哪里”与“结果是否会发送给模型”需要分别判断。

| 能力 | 执行位置 | 快照中的常规实现 |
| --- | --- | --- |
| `WebSearch` | 模型服务端 | CLI 另发一次携带服务端搜索工具定义的模型 API 请求 |
| `Grep` | 本地 | 将正则、范围、上下文和输出模式转换为 ripgrep 参数 |
| `Glob` | 本地 | 通过 ripgrep 的 `--files --glob` 查找文件路径 |

WebSearch 的调用链并非 CLI 直接请求 Google、Brave 或 Tavily：

1. 外层主模型选择调用 `WebSearch`，CLI 构造只包含搜索查询的辅助用户消息。
2. `makeToolSchema()` 生成 `web_search_20250305` 定义，携带域名过滤与 `max_uses=8`。
   `call()` 通过 `queryModelWithStreaming()` 另发辅助请求，将定义放入 `extraToolSchemas`；
   普通客户端工具列表为空，不等于 API 请求没有工具，传输层会合并额外的服务端工具定义。
3. 未开启对应 feature flag 时使用主循环模型；开启后选择小模型并要求使用 `web_search`，
   不能据此推断所有部署都固定使用同一个搜索模型。
4. 云端返回 `server_tool_use`、查询参数增量、`web_search_tool_result` 与文本等内容块。
   CLI 跟踪查询和结果数量以展示进度，再归一化标题、URL 和文字。
5. `mapToolResultToToolResultBlockParam()` 将结果封装为外层调用对应的 `tool_result`，
   回填主会话，并提示最终回答附来源链接。

这是“客户端 Tool 封装服务端搜索”的组合，而不是本地检索网页或本机浏览器自动化。
客户端可确认服务端搜索协议，但无法据此识别云端最终使用的搜索引擎、索引供应商或全部
数据来源；该内置入口也未展示独立搜索 API 的自动回退路径。

快照的 `isEnabled()` 对 firstParty、Foundry 返回启用，对 Vertex 检查指定 Claude 4
模型名，其余返回禁用。这只是当时的客户端分支，不是今日平台支持表，也不是服务端能力
探测。替换 API 地址或 Key 后仍须由目标端点实现所发搜索协议；不能由工具显示为可用
推断远端实际支持，更不能用旧快照否定前文较晚的 DeepSeek 接入证据。

Grep/Glob 则在本机执行搜索。Grep 构造 rg 参数数组，经 `ripGrep()` 调用；Glob 使用
`--files --glob`，再整理路径与截断信息。ripgrep 可能来自系统安装、随包 vendor 文件，
或原生程序内嵌版本；常规调用使用 `spawn` / `execFile`，不是任意字符串拼接的 Shell 命令。
复用搜索引擎不代表可省略权限、忽略规则、取消、进程生命周期和输出大小治理。

另有特殊构建分支：`hasEmbeddedSearchTools()` 为真时移除独立 Grep/Glob，在 Bash 环境
注入调用内嵌 bfs/ugrep 的 find/grep 包装。这仍是本地搜索，不应把 ripgrep 描述为所有
构建、所有搜索入口的唯一实现。是否实际启用，仍需检查具体构建与运行条件。

可复核的源码入口如下，行号仅对应本次读取的快照：

| 入口 | 核对位置 |
| --- | --- |
| 服务端搜索定义与启用条件 | `src/tools/WebSearchTool/WebSearchTool.ts:76`、`:168` |
| 辅助请求、流式结果与外层回填 | `src/tools/WebSearchTool/WebSearchTool.ts:254`、`:299`、`:401` |
| 合并额外工具定义与调用 Messages API | `src/services/api/claude.ts:1385`、`:1822` |
| Grep 参数与本机搜索调用 | `src/tools/GrepTool/GrepTool.ts:310`、`:441` |
| Glob 工具入口与 rg 文件枚举 | `src/tools/GlobTool/GlobTool.ts:160`、`src/utils/glob.ts:91` |
| ripgrep 来源与进程启动 | `src/utils/ripgrep.ts:31`、`:135`、`:217` |
| 特殊构建的搜索入口替换 | `src/tools.ts:193`、`src/utils/embeddedTools.ts:15`、`src/utils/bash/ShellSnapshot.ts:153` |

本地搜索不等于离线 Agent：匹配内容和文件名仍会转成工具结果，后续发送给所用模型服务。
对 Vortex 可借鉴的是将 `web_search`、文件路径匹配和文件内容搜索分成独立 Tool；模型
决定调用意图，Tool 决定执行位置与访问边界。当前 `list_directory` 只列一层，不等同于
Glob 或 Grep；本节不自动授权接入新工具，也不改变 Vortex 已选用秘塔独立搜索 API 的方案。

### 3.5 DeepSeek Harness：模型与搜索后端独立，最值得参考的抽象

官方 `dsh-tool-web` 定义模型可见的 `web_search`/`web_fetch`；`dsh-web` 的 `ctx.web`
统一检索契约和 backend 选择，具体 provider 插件负责网络调用。主模型走 `ctx.llm`，
搜索走 `ctx.web`，替换主 LLM 不会直接改变搜索工具 schema。
[Web 子系统][harness-web]、[工具消费层][harness-web-tool]

| Search backend | 实际方式 | 凭据/结果 |
| --- | --- | --- |
| Exa | 独立搜索 API | `EXA_API_KEY`；来源、片段、日期，不生成答案 [Exa adapter][harness-exa] |
| Perplexity | 辅助搜索模型 `sonar` 的 Chat Completions | `PERPLEXITY_API_KEY`；生成文本及结构化来源/引用 [Perplexity adapter][harness-perplexity] |
| DeepSeek | 单独的 Anthropic Messages 请求启用 `web_search_20250305` | 可用 `DEEPSEEK_API_KEY`；从搜索结果块抽取来源，不信任辅助模型正文作检索证据 [DeepSeek adapter][harness-deepseek] |

DeepSeek backend 的搜索 endpoint 与主 LLM endpoint 分开配置，不复用
`DEEPSEEK_BASE_URL`；源码直接发辅助 HTTP 请求，不经过主 `ctx.llm`。
这意味着“主模型 A + DeepSeek 搜索模型 B”是架构支持的组合，并非 A 自带了搜索。
一个搜索调用会产生辅助模型请求，不能按独立检索 API 的成本估计。
[DeepSeek provider 源码][harness-deepseek-source]

`searchProvider` 明确选 backend；只有一个可用候选时可自动选择，多个可用候选但未指定
时报歧义错误，不按注册顺序静默 fallback。[Web 子系统][harness-web]
CLI 与 Electron Desktop 复用运行时实现，但 profile、插件启用和安装状态独立；共享数据
不代表两边必然启用相同搜索 backend，也不是发行版可用性验收。
[Harness 架构][harness-architecture]

### 3.6 WorkBuddy 与 CodeBuddy Code：平台工具证据与未公开边界

WorkBuddy 是桌面办公产品；CLI 对照另列 CodeBuddy Code，不将两个产品名称或海外/
国内版本当作完全相同实现。WorkBuddy 模型文档确认可配置第三方 URL、Key 与模型；
其更新日志确认加入元宝搜索、识别元宝 Provider、展示搜索来源品牌。这支持“产品层存在
搜索 Provider”的判断，不足以证明所有模型都在自身 API 请求里开启 native search。
[WorkBuddy 模型配置][workbuddy-models]、[官方更新日志][workbuddy-changelog]

CodeBuddy Code 工具文档列出 WebSearch 与 WebFetch；`v2.137.0` 发布说明明确提到这些
内置工具访问平台服务，且把此类请求与直连第三方服务区分。这是“平台工具后端”的直接
证据，不应解释为只依赖主模型的 `enable_search`。
[CLI 工具参考][codebuddy-tools]、[平台请求说明][codebuddy-platform-tools]

CodeBuddy 的 `models.json` 支持独立 URL/Key 和 `supportsToolCall`；WorkBuddy 也支持
自定义模型，但两者都不能由“模型可正常聊天”推导“平台搜索认证也已成功”。
[CLI 模型配置][codebuddy-models]

目前公开证据不足以确认：

- 搜索工具具体 endpoint、实际底层搜索引擎与返回契约；元宝品牌不等于自建完整索引。
- 自定义模型时，搜索是否始终走平台、是否某些 Provider 会走模型端，以及具体优先级。
- 搜索凭据是否复用产品登录态、单独 Key 或其他服务认证；仅有第三方模型 Key 能否运行。
- 搜索与辅助摘要的计费、企业/国内外版本差异；不能宣称换模型后仍免费无限搜索。

这些须在具体版本中，分别用“产品登录 + 默认模型”“产品登录 + 自定义模型”以及文档
允许的独立 Key 模式验证。优先观察工具事件与脱敏请求域名，不记录认证头/正文，不擅自
解包闭源产品或使用未公开接口。这里没有把推测当成源码事实。

### 3.7 对 API Key 与模型切换的通用结论

**模型身份、模型协议和搜索身份是三个不同维度。** 同厂商换模型名、同模型换托管厂商、
第三方网关换 endpoint 不应共用一个“支持搜索”判断。

- hosted search：通常使用目标模型服务的认证；要确认 endpoint/model/tools 的组合。
- 独立搜索 Tool：用搜索服务自己的认证，主模型只需支持所需 Tool call 与结果回填。
- 产品平台搜索：可能使用单独产品账户/权限；主模型 Key 不自动授权它。
- 辅助搜索模型：搜索本身是一轮额外模型调用，其模型、Key、预算可独立于主模型。

对 Vortex，建议继续将主模型 Provider 与 `WebSearchBackend` 分开配置，并显式区分
独立检索、辅助搜索模型和主请求 hosted search。首版选择一个明确后端，不自动混用；
`/model` 只切换主模型时，应保留独立搜索配置，重新验证 Tool call 能力。若切换 hosted
路线，则明确提示搜索可用性、数据出口与费用变化。这是后续建议，不是本次实现或 ADR。

## 4. 海外独立搜索 API

以下价格和额度为 2026-09-27 查询结果，正式使用前需重新核对。

| 服务 | 主要特点 | 免费与公开价格 | 适合方向 |
| --- | --- | --- | --- |
| Tavily | 面向 Agent；结果直接包含相关内容片段、URL 和相关性分数；另有 Extract/Crawl | 免费 1,000 credits/月；基础搜索 1 credit，高级搜索 2 credits；按量价 $0.008/credit | 快速跑通 Tool loop |
| Brave Search | 独立网页索引；同时提供传统 Web Search 和面向模型的 LLM Context | Search $5/千次，每月赠送 $5 credits；开通计划需要支付信息 | 独立索引、通用搜索与后续对照 |
| Exa | 偏语义检索；可返回网页文本、highlights，并提供 Contents/Deep Search | Search $7/千次（含最多 10 条）；提供每月免费 credits | RAG、技术研究、语义召回 |
| Serper | 实时返回 Google SERP 的结构化数据 | 2,500 次免费试用；最低付费档约 $1/千次但需购买 50,000 credits | 明确需要 Google 排名与本地化结果 |

Tavily 的 Search endpoint 是普通 Bearer 鉴权的 JSON POST；结果包含 `title`、`url`、`content`、`score` 和可选发布时间。`basic` 已返回每个来源的相关内容片段，足以支撑首版。[Tavily Search API][tavily-search]、[Tavily 定价][tavily-pricing]

Brave 的优势是搜索索引独立于 Google/Bing，并提供预提取内容的 LLM Context endpoint；Exa 更适合搜索与正文提取紧密结合的语义检索；Serper 本质上是 Google SERP API，而不是完整的 Agent 上下文层。[Brave 文档][brave-docs]、[Exa 定价][exa-pricing]、[Serper 官网][serper]

## 5. 中国大陆服务

### 5.1 百度 AI 搜索

百度已经提供面向生成式 AI 的正式搜索产品，而不只是传统站长或站内搜索接口。“百度搜索”模式可以返回 URL、网页标题、摘要、发布时间和图片等，并支持结果数量、指定站点及发布时间过滤，可通过 OpenAPI、SDK、MCP 或智能体使用。当前公开目录价约为人民币 0.036 元/次，并提供每日免费额度。[百度 AI 搜索][baidu-ai-search]、[百度搜索能力与价格][baidu-search-pricing]

它适合作为中文互联网和百度生态后端，但应通过 Vortex 自己的统一结果类型隔离百度字段。

### 5.2 腾讯云联网搜索 API

腾讯云联网搜索来源于搜狗搜索，叠加腾讯内容生态，以 JSON 返回排序结果。它同时支持腾讯云 AK/SK 和简单 Bearer API Key，后者更适合 CLI Agent。个人实名用户可以开通轻量版或标准版；当前按量价格分别为人民币 18 元/千次和 46 元/千次。[产品说明][tencent-wsa]、[API Key 接入][tencent-wsa-key]、[计费说明][tencent-wsa-pricing]

腾讯轻量版是较合适的国内实现：它是独立 Search API，而不是绑定某个模型的一次回答增强。
原调研把它放在 Tavily 之后；考虑无代理约束后，可与秘塔一起优先验证，见 §5.6。

### 5.3 阿里云百炼联网搜索

以下为 2026-10-02 对当前 Qwen 模型与代码路径的补充核对；没有调用百炼真实 API，也没有修改代码。
应区分模型托管搜索、独立搜索工具和联网搜索 MCP 的接口与费用，不从相似名称推断等价。

#### 当前模型与最小路线

[CLI 装配](../../crates/vortex-cli/src/main.rs) 使用 `qwen3.8-max`，请求地址为
`https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions`；开启流式，关闭 Thinking，
调研时注册一个 `MockWebSearch`，2026-10-05 默认实现已换为 `MetasoWebSearch`。
当前 [ChatRequest](../../crates/vortex-provider/src/openai_chat/request.rs) **没有**
`enable_search` 或 `search_options`；秘塔是 Vortex 执行的 Client Tool，不等于 Qwen 模型侧搜索。

官方将当前模型列为支持联网搜索。保留 Chat Completions，使用 `turbo` 基础策略是最小探索
路线；不要直接迁移到服务端多轮 Agent 搜索。[百炼联网搜索][bailian-web-search]

待验证的 HTTP 请求体示例：

```json
{
  "model": "qwen3.8-max",
  "messages": [{"role": "user", "content": "请联网查询杭州明天的天气"}],
  "stream": true,
  "enable_thinking": false,
  "enable_search": true,
  "search_options": {
    "search_strategy": "turbo",
    "forced_search": true
  }
}
```

这是原始 HTTP JSON：扩展参数放在请求体顶层。Python SDK 示例中的 `extra_body` 是 SDK
传参方式，不是 Rust/Reqwest 需要发送的 JSON 包装层。`enable_search` 只允许搜索；模型
仍可能判断无需搜索，测试时可用 `forced_search`，日常是否强制则应另行配置。
[Chat Completions 参数参考][bailian-chat-api]

网络方面，客户端仍请求当前百炼域名，检索由服务端执行；不会因为开启搜索就要求 Vortex
直连 Google 或新增海外搜索 API。这是根据执行形态与代码请求地址作出的判断，仍需在
作者实际大陆网络验证，不能替代 §5.6 的无代理验收。

#### 与现有 Agent loop 的区别

```text
Client Tool：Vortex → 模型 → ToolCall → Vortex 调用搜索 API → ToolResult → 再请求模型
Hosted Search：Vortex → 百炼（内部检索与回答）→ 模型正文流 → Vortex
```

| 关注点 | 当前 Chat Completions 模型侧搜索 | Vortex 自己执行 Client Tool |
| --- | --- | --- |
| 执行与历史 | 检索留在百炼内部；本地主要接收 Assistant 正文 | Core 执行工具，并保留调用与结果 |
| TUI 状态 | 不自动触发本地 ToolStarted/ToolFinished | 已有工具进度投影路径 |
| 搜索预算 | 本地 `MAX_TOOL_ROUNDS` 不能限制服务端内置检索 | 本地可控制调用次数，后续补超时/大小预算 |
| 后端替换 | 绑定百炼模型能力 | 可以替换秘塔、腾讯云等实现 |

官方能力表注明 Chat Completions 不支持结构化搜索来源返回与角标标注；不能把模型自行
写出的 URL 当成已验证引用，也不能凭回答自称“已搜索”就判定成功。当前 decoder 没有
解析 usage 或搜索元数据，TUI 更不能伪造本地搜索完成事件。[百炼协议能力对照][bailian-web-search]

最小探索预计可复用现有 HTTP/SSE 传输和正文投影，但这是代码层推断，不是通过记录。
需保存并脱敏验证真实 SSE：包括结束包、usage-only 包、空值字段（如 `tool_calls: null`）
和中断。当前 `tool_calls` 是带 `#[serde(default)]` 的 Vec，默认值只处理字段缺失，不能
自动把显式 JSON null 转成空数组；若实测出现该格式，需要单独处理并加 fixture 测试。

#### 服务端多轮检索是另一条路线

当前联网搜索专题明确说明 Qwen3.8 的 Chat Completions/DashScope 不支持
`search_strategy: "agent"`；要多轮检索，应通过 Responses API 挂载 `{"type":"web_search"}`。
个别 DashScope/网页抓取参考页的支持列表仍有不同描述，不应跨协议照抄；首轮探索仅验证
上面的 `turbo`，高级能力以具体协议说明及实测为准。[联网搜索专题][bailian-web-search]、
[DashScope 参考][bailian-dashscope-api]

Responses 的请求、SSE 事件与终态不等于现有 Chat Completions，不能只更换 URL 并复用
`choices[].delta`/`[DONE]` decoder。需独立 adapter，把服务端搜索过程与来源映射到明确的
内部事件；它返回的搜索动作不是要求 ToolRegistry 再执行一遍的 Client ToolCall。
官方示例可从 `web_search_call.action.sources` 提取来源，但不自动插入角标。
[Responses API 参考][bailian-responses-api]

保留当前 `enable_thinking: false`，不为基础搜索顺手开启思考；Core 遇到 ReasoningDelta
仍会失败。如果以后选择需要思考的策略，必须先解决推理事件与历史策略，再进行组合验证。

#### 费用与探索顺序

官方当前列出的北京地域基础搜索费用为 `turbo` 3 元/千次、`max` 4 元/千次，另计检索内容
增加的模型 Token；内置搜索不提供自身免费调用额度。联网搜索 MCP 的前 2,000 次免费与
后续 29 元/千次是另一个服务，不能套用到模型内置搜索。以上为 2026-10-02 查询记录，
授权测试前仍应核对账户地域、计费与额度。[百炼计费说明][bailian-web-search]

建议把两条目标分开：**最快获得联网回答，可先探索百炼基础 hosted search；学习并控制
Agent 工具循环，继续推进秘塔/腾讯云等 Client Tool**。不因为内置搜索可用就废弃已实现的 loop。

后续实现授权范围建议：

1. 在 Provider 的 `mod.rs` 保存可选 Qwen 搜索设置，在 `request.rs` 映射扩展字段；只对
   支持的百炼 Provider 启用，不把它作为所有兼容模型的默认字段，不混进 ToolSpec。
2. 探索时新开 Session，使用空 ToolRegistry，不同时提供同名 Mock 搜索，避免模型选择
   假工具使结果混淆；是否同时使用 hosted search 与其他函数工具后续专门验证。
3. 在作者授权消费额度后，用同一模型比较关闭/开启搜索的少量中文时效问题，记录请求
   设置、延迟、Token/实际费用与错误；对回答事实独立核对，不以文风判断已执行搜索。
4. 先验证正文流，不宣称已有搜索来源卡片；按真实响应补 decoder fixture。搜索可能增加
   首字等待，检查现有 30 秒响应头/120 秒 chunk 空闲超时，不无依据地放宽。
5. 若验收证明必须获取可靠来源与搜索过程，再选择独立搜索 API、DashScope 或 Responses
   adapter；据此补 Provider capability 设计，不把三条协议一次性并入首版。

### 5.4 博查

博查提供面向大模型的 Web Search 和 AI Search，Web Search 返回网页与图片，公开目录价约人民币 0.036 元/次。它适合列入国内候选，但当前项目没有必要在首版同时接入多个同质后端；待有统一评测后再决定是否实现。[博查产品文档][bocha-doc]

### 5.5 秘塔 AI 搜索（2026-10-02 补充）

#### 产品与 API 边界

用户提供的 `metasota.ai/#/metasoai` 是公司产品介绍入口；实际搜索产品与 API 实验室位于
`metaso.cn`。公司官网介绍的是带答案、引用和学术检索的用户产品，不能直接推断 API 返回
相同内容或享有相同免费额度。[秘塔科技官网][metaso-company]

官方实验室区分三种能力，搜索可以独立使用，不要求把最终回答交给秘塔生成：

| 能力 | 官方页面请求路径 | Vortex 建议 |
| --- | --- | --- |
| 搜索 | `POST /api/v1/search` | 可作为 `web_search` 的真实后端 |
| 读取网页 | `POST /api/v1/reader` | 后续单独设计 `web_fetch` |
| 问答 | `POST /api/v1/chat/completions` | 首版不使用，避免额外购买答案生成 |

上述路径、Bearer 请求头及下面的参数来自官方实验室和其公开请求模板；2026-10-02 调研只读取页面，
没有登录账户、使用 Key 或执行收费请求。[秘塔 API 实验室][metaso-playground]

#### 最小接入形态

在 [API Keys 控制台][metaso-api-keys] 获取密钥后，可由具体后端发送 HTTPS JSON 请求。
以下为 2026-10-02 整理的公开请求模板；后续实现沿用此形态，作者反馈正常搜索已成功，
但本段不是原始响应或完整 API 契约验证记录：

```http
POST https://metaso.cn/api/v1/search
Authorization: Bearer <API_KEY>
Content-Type: application/json
```

```json
{
  "q": "Rust 官方异步编程文档",
  "scope": "webpage",
  "size": "5",
  "includeSummary": false,
  "includeRawContent": false,
  "conciseSnippet": false
}
```

`size` 使用官方模板的字符串形态；当前不假定数字类型也受支持。`includeSummary` 的页面
说明是利用网页摘要增强召回，不是问答接口的“生成最终答案”；是否开启应通过结果与成本
对照决定。关闭 `includeRawContent` 避免一次搜索隐式抓取所有来源正文。页面还提供分页
与学术等搜索范围，首版仅使用网页检索。[秘塔 API 实验室][metaso-playground]

不要直接套用网上所有名为“秘塔搜索”的代码：官方另有以 `question`、`searchTopicId`
调用 `/api/open/search/v2` 的专题检索文档，输出含生成文本与来源；官方 Python SDK 也展示
问答、追问及专题上传。这些不是上述独立网页搜索契约，不应混用参数、响应或 Key 的适用
范围；更不建议使用网页 Cookie、逆向接口或不明代理。[专题 API 文档][metaso-topic-doc]、
[官方 Python SDK][metaso-sdk]

#### 费用、可用性与证据限制

- 官方用户协议将 API 计费指向 API 实验室，而非直接沿用网页搜索积分规则。本次公开页面
  未核实基础搜索单价、赠送额度和周期，不把历史宣传中的免费次数当作当前承诺。
  原文 Tavily 的 1,000 credits/月仍为 2026-09-27 基线，不作本次价格对比依据。
  [秘塔用户协议][metaso-user-policy]
- 原文读取开关的官方提示为每个网页消耗 3 点积分；这不能换算成基础搜索每次价格，
  也不能假设所有可选能力均包含在一个固定单价中。[秘塔 API 实验室][metaso-playground]
- 国内运营产品值得纳入大陆网络环境评测，但本次没有从中国大陆执行真实 API 请求，
  不能保证无代理可用、特定延迟或 SLA；海外访问、中文与英文覆盖也需要分别测量。
- 官方隐私政策说明个人信息境内存储，但这不是“API 查询零留存”或“不用于训练”的
  专项承诺。接入前确认 API 查询/正文的保存期限、删除方式、训练用途和展示归因要求，
  仍不得发送仓库敏感信息。[秘塔隐私政策][metaso-private-policy]

2026-10-02 调研未核实实际响应字段；后续代码按 `webpages` 数组中的 `link`、可选
`title`/`snippet` 解析，作者反馈正常搜索已成功。尚未提供脱敏原始响应，空结果结构、
业务错误、QPS/并发限制、摘要增强成本和域名/日期过滤仍待确认；不能将一次成功
推广为所有响应形态的稳定契约，也不从品牌名称断言其底层索引来源。

#### 对 Vortex 的建议（2026-10-02 调研基线）

秘塔可以与 Tavily 放在同一层比较：都是由 Vortex 调用外部检索，再把结果交还当前模型。
同事的桌面助手经验是有价值的候选线索，但没有提供其接口路径、时间和测试集，不能据此
判断用的是独立搜索、问答、MCP 还是历史接口，也不能据此判定质量更好。

1. 保留现有 Mock 和 Agent loop；首先核对控制台的开通条件、试用额度与扣费规则。
2. 在作者授权实测后，用同一组中文时效、Rust 官方文档、中英混合查询对照秘塔与 Tavily，
   比较前五条的权威性、可用片段、时效、延迟、失败率及实际扣费；网页体验不能替代 API 评测。
3. 在“国内运行、不依赖代理”的约束下，优先验证秘塔与腾讯云；若秘塔的网络可达性、
   结果与费用符合预期，可作为首个真实后端候选。Tavily 先作为对照，最终选择仍待验收。
4. 接入时由 `vortex-tools` 隔离秘塔 DTO 并转换为统一结果；Core 不增加秘塔字段，
   不改模型 Provider。统一 `WebSearchBackend` 仍是后续设计，不是当前已有类型。
5. 先用 Reqwest 直接调用搜索 API，补 fixture、超时、大小限制、取消和错误处理；
   页面虽提供 MCP 入口，但本项目还未实现通用 MCP 客户端，无需为首个搜索后端扩大范围。
   [官方 MCP 入口][metaso-mcp]

#### 项目接入反馈（2026-10-05）

- TUI 默认注册 `MetasoWebSearch`，直接调用 `/api/v1/search`；秘塔 DTO 与 HTTP 客户端
  位于 `vortex-tools`，Core 与模型 Provider 无需增加厂商字段。Mock 保留，但不默认注册。
- 作者反馈编译后的程序已成功执行搜索，可确认作者环境的正常路径可用；本次文档更新
  未由 Agent 再次调用真实 API，也未收到脱敏响应、扣费或大陆无代理记录。
- 不要求重复正常路径才能继续开发。后续回归应补空结果、认证/限流、超时、响应过大、
  退出取消与预算限制；可保存一份脱敏成功样本作为离线 fixture，避免默认测试消耗额度。
- 请求与输出边界见 [搜索工具设计](../design/tools/web-search.md)，最新验证状态见
  [根 README](../../README.md)。通用 `WebSearchBackend` 与来源引用验证仍未实现。

### 5.6 中国大陆直连、代理与候选优先级（2026-10-02 补充）

#### 证据范围与分类

本次核对官方 API 域名、国内地域说明和项目使用的 HTTP 客户端，**没有在已确认的中国大陆
出口执行实测**。浏览工具的出口未知，本机终端也未确认网络地域与透明代理状态；它们能
打开页面，不足以证明大陆直连。官方接口文档证明“请求哪个地址”，不自动证明“每个运营商
都能访问”。以下国内直连预期属于基于产品部署与接入方式的推断，不是 SLA 或验收记录。

将状态分成三类，不强行做缺乏证据的二分：

- **直连优先候选**：国内服务，通常无需境外代理，先在 Vortex 的目标网络验收。
- **直连待验证候选**：海外 API，尚不能证明目标网络稳定直连，也不能断言全国必须代理。
- **该网络下依赖代理/其他获准网络出口**：只有同一环境中直连持续失败、替代出口成功，
  并排除配置、鉴权和限流等原因后，才能记录这一结论；结论必须附网络与日期。

| 候选 | 需验证的 API 地址/域名 | 无代理首版判断 | 建议优先级与限制 |
| --- | --- | --- | --- |
| 秘塔 | `metaso.cn/api/v1/search` | 直连优先候选；国内产品接入，目标网络待验收 | 第一组；先核实 API 额度、收费和真实响应 [官方实验室][metaso-playground] |
| 腾讯云 | `api.wsa.cloud.tencent.com/SearchPro` | 直连优先候选；国内云服务接入，目标网络待验收 | 第一组；API Key 接入明确，需实名认证与开通 [官方接入][tencent-wsa-key] |
| 百度 AI 搜索 | 千帆接入域名 `qianfan.baidubce.com`；具体搜索路径以所选 API 文档为准 | 直连优先候选；目标网络待验收 | 国内备选；不要混用智能搜索生成与独立百度搜索 [服务域名][baidu-service-domains]、[产品文档][baidu-ai-search] |
| 博查 | `api.bochaai.com/v1/web-search` | 直连优先候选；国内产品接入，目标网络待验收 | 国内备选；可用统一评测决定与百度的顺序 [官方示例][bocha-platform] |
| 阿里云百炼 | 中国内地域名 `dashscope.aliyuncs.com`；或北京业务空间专属域名 | 国内地域直连优先候选；不要混用境外地域域名/Key | 网络适配好，但本次默认不选 hosted search，保持 Client Tool 学习目标 [地域说明][bailian-region] |
| Tavily | `api.tavily.com/search` | 直连待验证；不标成“已证实必须代理” | 海外对照优先，因已有账户；直连通过再提升 [官方接口][tavily-search] |
| Brave Search | `api.search.brave.com/res/v1/web/search` | 直连待验证；需关注超时/连通风险 | 后置；独立索引优势不能代替网络验收 [官方接口][brave-search-api] |
| Exa | `api.exa.ai/search` | 直连待验证；需关注跨境链路风险 | 后置；需要语义检索后再比较 [官方接口][exa-search-api] |
| Serper | `google.serper.dev/search` | 直连待验证；不能因返回 Google 结果就断言需代理 | 后置；明确需要 Google SERP 时再比较 [Haystack 官方适配器源码][serper-adapter] |

目前没有足够证据将上述海外服务列入“大陆所有网络必须代理”。开发者项目中确有风险线索：
XnoBot 的发布说明提醒 Brave API 在国内可能限速/超时；OpenBiliClaw 的维护文档记录过 Exa
国内直连超时。这些是具体项目的使用报告，不是厂商承诺，也不是全国各运营商的当前测量。
它们可用于提高验收优先级，不能替代 Vortex 的测试。[XnoBot 发布说明][brave-network-report]、
[OpenBiliClaw 网络配置记录][exa-network-report]

#### 四件事必须分别验证

1. **官网与控制台**：注册、登录、获取 Key、充值是否可完成；已有账户只解决部分开通问题。
2. **API 连通**：实际 API 域名的 DNS、TCP、TLS 与 HTTP，而非官网首页或 Google 页面。
3. **业务成功**：合法请求得到可用 JSON、正确结果与可接受的延迟；`401` 不是搜索成功，
   `403` 也不能直接归因于网络封锁，可能是鉴权、WAF、账户或地域策略。
4. **结果来源访问**：API 能返回某个境外 URL，不代表本地浏览器或未来 `web_fetch` 能读取它。
   只把已有结果片段回填模型时，不应隐式要求客户端再访问所有来源网站。

对于 Serper，Google 检索由服务端完成，Vortex 连接的是 Serper；服务端代检索与本地使用
网络代理是两回事。查询参数中的国家/语言，也只影响结果本地化，不证明 API 在该国家可达。
账户地域或服务条款限制同样不能靠网络连通测试解决，不应通过代理规避厂商使用条件。

#### Vortex 当前并非完全没有环境代理支持

仓库尚无应用级代理配置、每后端分流或代理状态展示，但 [根 Cargo.toml](../../Cargo.toml)
没有关闭 Reqwest 默认 features；锁定的 `0.13.4` 默认含 `system-proxy`。
[Provider](../../crates/vortex-provider/src/openai_chat/mod.rs) 用 `reqwest::Client::new()`，
没有调用 `no_proxy()` 或设置显式代理。根据该版本官方文档与本地依赖源码，Client 默认可读取
`HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` 及大小写变体，`NO_PROXY` 可排除域名。
[Reqwest 0.13.4 代理文档][reqwest-proxies]

因此，“没有 Vortex 代理配置模块”不等于“必须先开发该模块才能使用代理”。若用户已有
可用的 HTTP 代理，启动前导出环境变量可以作为开发期方案。当前秘塔客户端未调用
`no_proxy()`，沿用 Reqwest 的系统代理策略；Mock 不发网络请求。这个结论是客户端行为核对，未验证
某个本地代理可用，也不应据此把代理变成首版必需依赖。

环境代理会影响符合条件的其他请求，包括现有 Qwen Provider；以后配置时应考虑 `NO_PROXY`
或每客户端策略，不能把所有国内请求一起绕到海外。浏览器使用的代理设置也不保证终端
进程继承；TUN/透明代理则可能让“不设置代理变量”的请求仍经过代理。

#### 无代理验收方法与记录

由作者在实际运行 Vortex 的大陆网络执行，先确认未启用 TUN、透明代理或其他改道；本文
不代为修改系统网络设置。下面只是不带 Key 的 HTTP 连通性探测示例，不能证明业务成功：

```sh
curl -q --proxy '' --noproxy '*' \
  --connect-timeout 5 --max-time 15 \
  -sS -o /dev/null \
  -w 'http=%{http_code} ip=%{remote_ip} tcp=%{time_connect} tls=%{time_appconnect} total=%{time_total}\n' \
  https://api.tavily.com/search
```

`-q` 禁用 curl 默认配置文件；显式空代理与 `--noproxy '*'` 排除 curl 的应用层代理，
**不能排除系统 TUN/透明代理**。对其他服务替换为表中的准确 API 地址，不自动跟随重定向，
不关闭 TLS 验证。来自真实服务的 `401/405` 等响应只能支持“HTTP 服务可达”；若拿到网关/
防火墙页面仍需确认来源。`http=000` 表示未取得 HTTP 响应，不足以判定必须代理。

随后在授权消费额度的前提下按各厂商正确的 POST/GET 方法做少量真实检索，敏感 Key 不进
命令历史、日志和调研文档。至少分两个时间段复测，记录日期、地区/运营商或云地域、出口
类型、IPv4/IPv6、DNS、是否关闭透明代理、成功次数/总次数、错误类型与延迟；公开文档无需
记录个人公网 IP。单次成功不代表稳定，单次超时也不能判定被封锁。

在“国内运行、首版不依赖代理”的前提下，更新后的推荐顺序为：**先验收秘塔与腾讯云，
百度/博查备选；Tavily 用已有账户做直连对照，通过后再决定是否前移；Brave/Exa/Serper
在有明确需要且网络验收通过后接入**。百炼保持独立的 hosted search 研究方向。
该顺序针对 Client Tool 后端；如果目标只是先获得联网回答，可优先探索 §5.3 的百炼基础
hosted search，不需要为此先接入独立搜索 API。当前没有任何候选被本文标记为
“已通过 Vortex 大陆无代理验收”；最终选择仍需记录决定。

### 5.7 当前 DeepSeek V4 Pro 的模型端搜索（2026-10-02 调研与实测）

#### 项目使用路径与结论

[exec](../../crates/vortex-cli/src/exec.rs) 使用 `DEEPSEEK_API_KEY`、
`deepseek-v4-pro` 和官方 `https://api.deepseek.com/chat/completions`，不是百炼托管的
DeepSeek。当前 TUI 实际装配的仍是 Qwen；[CLI](../../crates/vortex-cli/src/main.rs) 中的
DeepSeek TUI 装配是注释示例。不能从同一个模型名称推断不同厂商、不同协议的搜索支持相同。

**当前账户实测：官方 V4 Pro 的 Responses 和 Anthropic 接口支持服务端搜索，现有项目
Chat Completions 路径没有接入它。** 本次未改源码、配置或依赖，未通过 Vortex TUI 调用；
使用临时诊断进程直接请求官方 API，不等于产品功能已经实现。

| 接口/形态 | 官方材料 | 本次观察与适用结论 |
| --- | --- | --- |
| 官方 Chat Completions | 工具定义只列 `function`，没有文档化的 `enable_search` 开关 [Chat API][deepseek-chat-api] | 普通请求与添加百炼式搜索参数的请求均未出现搜索证据；不要复用 Qwen 开关 |
| 官方 Responses | 兼容表写 `web_search` 等内置工具被忽略 [兼容说明][deepseek-responses-guide] | 同一模型的非流式与 SSE 请求均返回 `web_search_call`；与文档冲突，需确认支持范围与长期契约 |
| 官方 Anthropic Messages | 集成页声明支持 Claude Code 搜索；兼容表接受服务端搜索内容块 [集成说明][deepseek-claude-code]、[Anthropic 兼容说明][deepseek-anthropic-guide] | 返回 `server_tool_use`、`web_search_tool_result`、来源及最终文本，单个样本完成服务端搜索 |
| 百炼托管 DeepSeek | 属于百炼搜索能力与计费体系 [百炼搜索][bailian-web-search] | 本次没有调用；不用于证明官方 DeepSeek Chat API 的能力 |

Responses 的 `200` 本身不是证据：官方文档明确允许静默忽略不支持的参数。本次依据是
结构化搜索调用、搜索阶段事件及最终响应，而非模型自称“已联网”。但两次样本也不能证明
所有账户、模型别名、地域和工具参数均受支持；正式接入前应向 DeepSeek 确认文档差异。

#### 实验方法与结果

使用本地已有 DeepSeek Key，仅发送到 `api.deepseek.com`，验证 TLS 且禁止携带认证头跟随
重定向。不输出密钥、认证头、完整搜索正文或本地环境文件；没有发送仓库内容。共完成
**5 次手动请求**，关闭 Thinking、每次输出上限 256 Token、不重试。它们是本次用户要求的
真实 API 实验，会消耗额度，不进入默认自动化测试。

统一问题为“实际联网检索 Rust 官方博客最新文章，给出标题、日期、来源链接；不能联网就
明确说明，不猜测，限 100 字”。实验沿用进程的环境代理；出口地域、TUN 状态未确认，
**不能作为中国大陆无代理验收**。

| 实验 | 请求差异 | 响应/耗时 | 关键观察 |
| --- | --- | --- | --- |
| A：Chat 基线 | 不提供工具或搜索字段 | `200`，1.35 秒 | 返回无法联网的文本，无工具调用 |
| B：Chat 搜索参数探测 | 加 `enable_search: true`、`search_options.forced_search: true` | `200`，1.68 秒 | 仍返回无法联网，无搜索结构；只能确认此请求未启用搜索，不能仅据此判断字段是忽略还是其他处理 |
| C：Responses 非流式 | `tools: [{"type":"web_search"}]` | `200`，2.60 秒 | 搜索调用完成，返回 Rust 1.99.0 的标题、日期、链接；最终文本 `annotations: []` |
| D：Responses SSE | C 改为 `stream: true` | `200`，13.40 秒 | 一次搜索、一次打开 feed 失败、一次打开首页完成；最终 `response.completed`，没有 `[DONE]` |
| E：Anthropic 非流式 | `web_search_20250305`，`max_uses: 1` | `200`，3.57 秒 | 一次服务端搜索，10 条来源、最终答案；`usage.server_tool_use.web_search_requests: 1` |

C 和 E 的答案均指向 [2026-10-01 发布的 Rust 1.99.0][rust-199-release]，标题与日期已由
Rust 官方页面独立核对。D 的最终答案却称最新文章为“Demoting i686 Windows targets to
std-only”（2026-10-02），仅给博客首页；本次独立检索未确认该标题，不能当作已验证事实。
这说明结构化搜索执行成功与最终回答的时效、引用准确性需要分别验收。

返回用量如下，仅记录服务端报告，不替代账单：

| 实验 | 输入 Token | 输出 Token | 缓存与限制 |
| --- | ---: | ---: | --- |
| A | 39 | 34 | 缓存命中 0 |
| B | 39 | 41 | 缓存命中 0 |
| C | 3,397 | 119 | 输入中缓存命中 512；reasoning 0 |
| D | 15,687 | 240 | 输入中缓存命中 7,168；reasoning 0 |
| E | 9,901 | 119 | 另报告 `cache_read_input_tokens: 256`，勿直接按 Responses 的统计口径相加 |

输出上限不限制服务端搜索回填的输入量，也不保证内部搜索只发生一次。D 的多个搜索/
打开动作不受 Vortex 的四轮 Client Tool 上限约束；本次没有验证 Responses
`max_tool_calls` 的实际效果，官方兼容表称其被忽略。E 的 `max_uses: 1` 只观察到一次搜索，
没有测试超过上限是否被拒绝。官方集成说明提醒搜索摘要会产生额外模型 Token 费用；实际
扣费以控制台和 [当前模型价格][deepseek-pricing] 为准，不用普通短问答价格估计搜索总成本。

#### 已验证的最小请求形态

本次成功的 Responses 请求：

```http
POST https://api.deepseek.com/responses
Authorization: Bearer <DEEPSEEK_API_KEY>
Content-Type: application/json
```

```json
{
  "model": "deepseek-v4-pro",
  "input": "请实际联网检索 Rust 官方博客最新文章，提供标题、日期和来源链接。",
  "tools": [{"type": "web_search"}],
  "reasoning": {"effort": "none"},
  "max_output_tokens": 256,
  "stream": true
}
```

非流式只改 `stream: false`。关闭思考使用 Responses 的 `reasoning.effort: none`，不是
Qwen 的 `enable_thinking`。[DeepSeek 思考参数][deepseek-thinking]

Anthropic 实验使用 `POST https://api.deepseek.com/anthropic/v1/messages`，携带 Bearer
认证与 `anthropic-version: 2023-06-01`；请求字段为 `model`、`messages`、`max_tokens: 256`、
`thinking: {"type":"disabled"}`、`stream: false`，工具定义如下：

```json
{"type":"web_search_20250305","name":"web_search","max_uses":1}
```

工具定义参照 [Anthropic 搜索工具][anthropic-web-search]，并在 DeepSeek 端实际验证；不因
此样本推断 DeepSeek 同时支持 Anthropic 的所有工具版本、过滤参数或收费规则。

#### Vortex 接入影响与建议

1. **不能只改 URL 或在现有 ChatRequest 加开关。** 当前 Provider 发送 `messages`，按
   `choices[].delta` 解码且要求 `[DONE]`；Responses 使用 `input`、语义事件与
   `response.completed/incomplete/failed`。应新增独立 Responses adapter，复用 HTTP/SSE
   传输层，不把 Responses 数据交给 `OpenAiChatDecoder`。
2. **服务端搜索不交给 ToolRegistry 执行。** `web_search_call` 是已经由厂商处理的动作，
   不是要本地再次执行的 Function ToolCall。初版可先映射最终正文和结束状态；需要进度/
   来源时新增中立的 hosted-tool 事件，并明确与 Client Tool 事件、历史项的区别。
3. **处理文本阶段与引用。** D 的流包含英文过程文本和中文最终答案，不能把所有
   `output_text.delta` 不加区分拼成最终历史。应按输出 item/phase 管理过程与最终回答；
   C 没有 URL annotations，E 也没有文本 citations。E 的结构化来源可用于展示，但不能
   宣称已有自动引用编号或完整证据映射。
4. **保持显式关闭 Thinking。** 当前 Core 拒绝 `ReasoningDelta`；将模型切到 DeepSeek
   并不会自动使用 Qwen 开关关闭思考。请求 DTO 应采用目标协议支持的开关，不静默丢弃
   思考来掩盖接口不匹配。
5. **先单独实验，不替换学习用工具循环。** 若目标是尽快获得联网回答，官方 Key 可继续
   使用，但需要新增 Provider 适配与预算/取消验证；若目标是学习搜索 Tool 调度，继续
   秘塔/腾讯云等 Client Tool 路线。两者不要隐式 fallback，避免费用和数据出口变化。
6. 正式选型前补齐多轮上下文、SSE 提前断流/失败、取消、来源获取、搜索次数限制、
   搜索加 Function Tool 的混用，以及目标大陆网络测试。此次没有验证这些内容，也没有
   验证其他 DeepSeek 模型或百炼托管版本。

## 6. Vortex 架构影响

### 6.1 运行时模型

当前运行路径接近：

```text
Prompt → ModelProvider::stream → 最终回答
```

工具版需要变成：

```text
Turn
  → Model step
  → ToolCall(web_search)
  → Tool 执行与 ToolResult
  → 下一次 Model step
  → 最终回答
```

Session 在上述所有步骤中保持 Running。模型流中可能被拆分的厂商工具参数应由 Provider adapter 组装为完整内部 ToolCall，Core 不应理解 OpenAI 或 Anthropic 的增量字段。

### 6.2 后端边界

建议的依赖方向为：

```text
WebSearchTool
    └── WebSearchBackend
          ├── TavilySearchBackend
          ├── MetasoSearchBackend
          ├── TencentSearchBackend
          └── BaiduSearchBackend

ModelProvider capability
    └── ProviderHostedWebSearch
```

`WebSearchBackend` 只抽象查询和搜索结果；Provider-hosted Search 仍属于 Provider adapter。厂商请求和响应类型不得进入 Core。

内部最小结果至少保留稳定 ID、标题、URL、片段、可选发布时间和可选分数。对 URL 做规范化和去重，并为来源分配 `S1`、`S2` 等 ID。模型引用来源 ID 后，Core 才能验证引用确实来自本次搜索，TUI 再把它渲染成可点击链接；不能完全依赖模型自由生成 URL。

## 7. 容易遗漏的工程问题

### 7.1 隐私与不可信内容

- 搜索查询会离开本机；默认不得把仓库文件正文、内部域名、凭据或大段错误日志自动拼进查询。
- 搜索结果属于不可信 ToolResult，即使包含“忽略此前指令”也不能提升到 Instruction。
- 日志默认不记录 API Key、完整查询和搜索正文；诊断模式也应限制长度并显式开启。
- 生产使用前需分别复核厂商的数据保留、缓存、展示归因和适用地区条款。本文不作法律结论。

### 7.2 预算、取消与错误

- 一轮最多搜索 3 次，每次最多返回 5 条，是合理的首版默认值。
- 同时限制总字符数或 Token 数，不能只限制结果条数。
- Turn 取消应向下取消搜索 HTTP 请求。
- `401/403` 不重试；`429` 尊重 `Retry-After` 或做有上限退避；超时和额度不足应使用不同结构化错误。
- 搜索失败通常返回错误 ToolResult，让模型决定是否改写查询或在信息不足时回答；不应无条件让整个 Session 失败。
- 不静默切换备用厂商。不同索引会改变结果、费用与隐私边界，fallback 必须是显式配置和可观察事件。

### 7.3 测试与评测

单元测试使用 Fake backend 和固定 JSON fixture，不访问真实服务。真实 API 测试必须通过显式环境开关启用，并与默认 CI 分离，避免消耗 credits 或因网络波动失败。

建立小型评测集后再比较后端：中文时效信息、Rust/软件问题、官方文档查找、中英文混合及限定站点/日期各若干条。记录前五条相关性、权威来源比例、发布时间准确性、延迟、响应大小和单次成本。没有这组对照，不应根据一两个查询宣称某后端质量更好。

## 8. 首版建议

原 2026-09-27 路线以 Tavily 为首个后端；以下保留 2026-10-02 加入无代理约束后的顺序建议，
不是当前待办清单。2026-10-05 秘塔接入反馈见 §5.5；统一后端抽象尚未实现。
百炼最小 hosted search 探索与此路线分开，见 §5.3；DeepSeek 官方搜索实测与接口限制见 §5.7。
前两项的当前实现状态见 Agent loop 设计与根 README，不应理解为尚未开始。

1. 先定义 Provider 无关的 ToolSpec、ToolCall、ToolResult 和 ConversationItem。
2. 让一个 Turn 支持多个 Model step，并使用 Fake Search 跑通闭环。
3. 建立 WebSearchBackend 和统一 SearchResult，不依赖具体服务字段。
4. 先按 §5.6 完成大陆无代理验收，再从秘塔/腾讯云等候选中选择一个真实后端；不要先接入再发现网络是硬前置。若实测决定使用 Tavily，固定 `search_depth = basic`、`max_results = 5`、`include_answer = false`、`include_raw_content = false`、`include_images = false`。
5. 增加搜索开始、完成、失败事件，以及 TUI 中简洁的查询和来源展示。
6. 增加每轮调用次数、超时、结果大小、取消和引用验证。
7. 使用统一评测集比较 Tavily 与秘塔/腾讯/百度，再决定真实后端顺序；秘塔的补充评测建议见 §5.5。
8. Tool loop 稳定后，单独设计 `web_fetch`；随后再研究百炼等 Provider-hosted Search。

这一路线让搜索厂商只承担检索，Vortex 自己承担 Agent 调度、上下文、错误、引用与展示，形成以后文件、Shell 和 MCP 工具都能复用的基础。

## 参考资料

- [OpenAI Web Search][openai-web-search]
- [Claude Code Agent SDK][claude-code-sdk]
- [Anthropic Web Search][anthropic-web-search]
- [Codex 搜索][codex-search]、[自定义 Provider][codex-provider-config]、[模型与协议][codex-models]
- [Claude Code WebSearch 行为][claude-tools]、[Desktop 部署与功能][claude-desktop]
- [Harness Web 子系统][harness-web]、[架构与桌面运行时][harness-architecture]、[Web 工具层][harness-web-tool]
- [Harness Exa][harness-exa]、[Perplexity][harness-perplexity]、[DeepSeek][harness-deepseek]、[DeepSeek adapter 源码][harness-deepseek-source]
- [WorkBuddy 模型][workbuddy-models]、[更新日志][workbuddy-changelog]；[CodeBuddy CLI 工具][codebuddy-tools]、[模型][codebuddy-models]、[平台工具请求][codebuddy-platform-tools]
- [Tavily Search API][tavily-search]、[Tavily 定价][tavily-pricing]
- [Brave Search API][brave-docs]
- [Exa API 定价][exa-pricing]
- [Serper][serper]
- [百度 AI 搜索][baidu-ai-search]
- [腾讯云联网搜索 API][tencent-wsa]
- [阿里云百炼联网搜索][bailian-web-search]
- [百炼 Chat Completions 参数][bailian-chat-api]、[DashScope 参数][bailian-dashscope-api]、[Responses 接口与迁移][bailian-responses-api]
- [DeepSeek Chat API][deepseek-chat-api]、[Responses 兼容说明][deepseek-responses-guide]、[Anthropic 兼容说明][deepseek-anthropic-guide]
- [DeepSeek Claude Code 搜索说明][deepseek-claude-code]、[思考参数][deepseek-thinking]、[模型价格][deepseek-pricing]
- [Rust 1.99.0 官方发布文章][rust-199-release]（搜索答案的独立核对来源）
- [博查搜索 API 产品文档][bocha-doc]
- [秘塔 API 实验室][metaso-playground]、[API Keys][metaso-api-keys]、[官方 MCP 入口][metaso-mcp]
- [秘塔专题 API 文档][metaso-topic-doc]、[官方 Python SDK][metaso-sdk]
- [秘塔用户协议][metaso-user-policy]、[隐私政策][metaso-private-policy]
- [百炼中国内地接入域名][bailian-region]、[百度千帆服务域名][baidu-service-domains]、[博查官方示例][bocha-platform]
- [Brave API][brave-search-api]、[Exa Search API][exa-search-api]、[Haystack Serper 适配器源码][serper-adapter]
- [Reqwest 0.13.4 代理文档][reqwest-proxies]
- [XnoBot 的 Brave 网络提示][brave-network-report]、[OpenBiliClaw 的 Exa 网络记录][exa-network-report]（项目使用报告，非厂商保证）

[openai-web-search]: https://developers.openai.com/api/docs/guides/tools-web-search
[claude-code-sdk]: https://code.claude.com/docs/en/agent-sdk/python
[anthropic-web-search]: https://platform.claude.com/docs/zh-CN/agents-and-tools/tool-use/web-search-tool
[codex-search]: https://learn.chatgpt.com/docs/web-search
[codex-provider-config]: https://learn.chatgpt.com/docs/config-file/config-advanced
[codex-models]: https://learn.chatgpt.com/docs/models
[claude-tools]: https://code.claude.com/docs/en/tools-reference
[claude-desktop]: https://code.claude.com/docs/en/desktop
[harness-web]: https://deepseek-harness.github.io/deepseek-harness/en/reference/subsystems/web
[harness-architecture]: https://deepseek-harness.github.io/deepseek-harness/en/reference/
[harness-web-tool]: https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/web/tool-web
[harness-exa]: https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/web/web-search-exa
[harness-perplexity]: https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/web/web-search-perplexity
[harness-deepseek]: https://github.com/deepseek-ai/deepseek-harness/tree/master/packages/web/web-search-deepseek
[harness-deepseek-source]: https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/web/web-search-deepseek/src/provider.ts
[workbuddy-models]: https://www.workbuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Model
[workbuddy-changelog]: https://www.workbuddy.cn/docs/workbuddy/Changelog
[codebuddy-tools]: https://www.codebuddy.ai/docs/cli/tools-reference
[codebuddy-models]: https://cloud.tencent.com/document/product/1831/137010
[codebuddy-platform-tools]: https://www.codebuddy.ai/docs/cli/release-notes/v2.137.0
[tavily-search]: https://docs.tavily.com/documentation/api-reference/endpoint/search
[tavily-pricing]: https://www.tavily.com/pricing
[brave-docs]: https://api-dashboard.search.brave.com/documentation
[exa-pricing]: https://exa.ai/pricing?tab=api
[serper]: https://serper.dev/
[baidu-ai-search]: https://cloud.baidu.com/doc/BAIDU_AI_SEARCH/index.html
[baidu-search-pricing]: https://cloud.baidu.com/theme/B/57977-75
[tencent-wsa]: https://cloud.tencent.com/product/wsa
[tencent-wsa-key]: https://cloud.tencent.com/document/product/1806/130615
[tencent-wsa-pricing]: https://cloud.tencent.com/document/product/1806/121798
[bailian-web-search]: https://help.aliyun.com/zh/model-studio/web-search
[bailian-chat-api]: https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions
[bailian-dashscope-api]: https://help.aliyun.com/zh/model-studio/qwen-api-via-dashscope
[bailian-responses-api]: https://help.aliyun.com/zh/model-studio/compatibility-with-openai-responses-api
[deepseek-chat-api]: https://api-docs.deepseek.com/api/create-chat-completion/
[deepseek-responses-guide]: https://api-docs.deepseek.com/guides/responses_api/
[deepseek-anthropic-guide]: https://api-docs.deepseek.com/guides/anthropic_api/
[deepseek-claude-code]: https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/
[deepseek-thinking]: https://api-docs.deepseek.com/guides/thinking_mode/
[deepseek-pricing]: https://api-docs.deepseek.com/quick_start/pricing/
[rust-199-release]: https://blog.rust-lang.org/2026/10/01/Rust-1.99.0/
[bocha-doc]: https://mkp-res.hc-cdn.com/marketplace/public/appv2/attachment/FE5/A96/96F/0000000000FE5A9696F.20250527042304.feac985174234808a69f64e899bd6792.pdf
[metaso-company]: https://metasota.ai/#/metasoai
[metaso-playground]: https://metaso.cn/search-api/playground
[metaso-api-keys]: https://metaso.cn/search-api/api-keys
[metaso-mcp]: https://www.modelscope.cn/mcp/servers/metasota/metaso-search
[metaso-topic-doc]: https://metaso.cn/subject/8547516269457154048?ssi=hXHmJkx
[metaso-sdk]: https://github.com/meta-sota/metaso-sdk
[metaso-user-policy]: https://metaso.cn/meta-user-policy
[metaso-private-policy]: https://metaso.cn/meta-private-policy
[baidu-service-domains]: https://cloud.baidu.com/doc/qianfan/s/8mh4sv3sb
[bailian-region]: https://help.aliyun.com/zh/model-studio/beijing-access-information
[bocha-platform]: https://open.bochaai.com/
[brave-search-api]: https://api-dashboard.search.brave.com/api-reference/web/search/get
[exa-search-api]: https://exa.ai/docs/reference/search
[serper-adapter]: https://github.com/deepset-ai/haystack/blob/main/haystack/components/websearch/serper_dev.py
[reqwest-proxies]: https://docs.rs/reqwest/0.13.4/reqwest/#proxies
[brave-network-report]: https://pypi.org/project/xnobot-ai/
[exa-network-report]: https://github.com/whiteguo233/OpenBiliClaw/blob/main/docs/modules/config.md

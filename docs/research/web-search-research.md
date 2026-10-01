# Web Search 能力与服务选型调研

- 调研日期：2026-09-27
- 范围：Agent Web Search 的执行形态、国内外服务候选、可靠性与安全边界
- 定位：实现前调研与首版建议，不代表已经完成依赖或接口决策
- 关联设计：[`Message 与上下文构建`](../design/message-building.md)、[`Session Runtime`](../design/session-runtime.md)
- 关联架构：[`架构总览`](../architecture/overview.md)、[`关键依赖与技术选型`](../architecture/technology-stack.md)

## 1. 结论

1. Web Search 不只是一次 HTTP 请求，而是 Agent Tool loop 的第一个真实用例。Vortex 当前“一次 Prompt 对应一次模型流”的运行方式，需要先演进为“一个 Turn 包含多个模型步骤和工具步骤”。
2. 搜索存在两种不同执行形态：Vortex 调用独立搜索 API 的 Client Tool，以及模型厂商在一次请求内部完成的 Provider-hosted Search。两者可以共享内部事件语义，但不能共用同一执行实现。
3. 首个 Client Tool 后端建议使用 Tavily：接口简单、结果面向 Agent、免费账户每月提供 1,000 credits，适合开发验证。首版应只使用基础检索，不购买服务端生成答案。
4. 中国大陆已有可直接使用的同类服务。腾讯云联网搜索 API 和百度 AI 搜索可作为后续独立后端；阿里云百炼联网搜索更适合作为 Qwen Provider 的托管能力，而不是通用搜索后端。
5. 首版必须同时限制搜索次数、结果数量、输出大小和超时，并把查询隐私、Prompt Injection、来源引用和真实 API 测试费用纳入设计。

## 2. 能力边界

三个容易混淆的能力应分开建模：

| 能力 | 输入与输出 | 首版范围 |
| --- | --- | --- |
| `web_search` | 查询词 → 标题、URL、相关片段及元数据 | 是 |
| `web_fetch` | URL → 提取后的网页正文 | 否，后续单独设计 SSRF 与内容限制 |
| Deep Research | 任务 → 多轮检索、阅读、分析和报告 | 否，依赖完整 Tool loop、预算与后台任务 |

搜索服务只负责发现和检索；最终回答仍由 Vortex 当前选定的模型生成。若直接购买搜索厂商的答案或研究接口，会掩盖 Tool call、Tool result、上下文回填和多步调度等本项目希望学习的实现细节。

## 3. Codex 与 Claude Code 对照

### 3.1 Codex

本地 Codex 源码基线为提交 `5bf132cd52`（2026-09-16）。默认路径会构造 hosted `WebSearch` ToolSpec，随 Responses 请求交给服务端执行；客户端接收 `web_search_call`、动作和引用并投影到事件与 TUI。OpenAI 官方 Responses API 同样以 `{ "type": "web_search" }` 开启托管搜索，并返回搜索调用、文本及 URL citation。[OpenAI Web Search 文档][openai-web-search]

源码还包含开发中的 standalone `web.run` 扩展：客户端通过 `SearchClient` 请求所配置 Provider 的 `alpha/search`，而不是直接请求 Google、Bing 或 Brave。该功能在上述基线中为 `UnderDevelopment` 且默认关闭。两条路径都没有公开底层搜索引擎；可确认的是 Codex 自己负责工具装配、事件和界面，检索基础设施由服务端提供。

### 3.2 Claude Code

Claude Code Agent SDK 把 `WebSearch` 列为内置工具，输入包含查询及允许/禁止域名，输出包含标题、URL、摘要和元数据。[Claude Code Agent SDK][claude-code-sdk]

Anthropic Messages API 还提供版本化的服务端 `web_search_*` 工具：Claude 决定何时搜索，API 执行搜索并在同一请求中把结果交还模型，最后生成带引用的回答。[Anthropic Web Search 文档][anthropic-web-search]

Claude Code 不是完整开源项目，公开材料没有证明其客户端内置工具与 Messages API 服务端工具的内部实现完全相同，也没有披露实际使用 Google、Bing、Brave 或自有混合索引。因此不应从工具名称推断底层搜索厂商。

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

若需要验证国内后端可替换性，腾讯轻量版是 Tavily 之后较合适的第二实现：它是独立 Search API，而不是绑定某个模型的一次回答增强。

### 5.3 阿里云百炼联网搜索

百炼提供两种模型托管接入：Chat Completions 使用 `enable_search: true`，Responses API 在 `tools` 中声明 `web_search`。它还提供独立的联网搜索 MCP；当前文档注明前 2,000 次调用免费，之后约人民币 29 元/千次。[百炼联网搜索][bailian-web-search]

Vortex 当前使用 Qwen 兼容接口，因此这条路径接入最快，但会绕过通用 Client Tool 的执行过程。建议以后将其建模为 Provider capability，用来研究 hosted search；不要用它替代首个 Tool loop。

### 5.4 博查

博查提供面向大模型的 Web Search 和 AI Search，Web Search 返回网页与图片，公开目录价约人民币 0.036 元/次。它适合列入国内候选，但当前项目没有必要在首版同时接入多个同质后端；待有统一评测后再决定是否实现。[博查产品文档][bocha-doc]

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

实施顺序建议如下：

1. 先定义 Provider 无关的 ToolSpec、ToolCall、ToolResult 和 ConversationItem。
2. 让一个 Turn 支持多个 Model step，并使用 Fake Search 跑通闭环。
3. 建立 WebSearchBackend 和统一 SearchResult，不依赖具体服务字段。
4. 接入 Tavily，固定 `search_depth = basic`、`max_results = 5`、`include_answer = false`、`include_raw_content = false`、`include_images = false`。
5. 增加搜索开始、完成、失败事件，以及 TUI 中简洁的查询和来源展示。
6. 增加每轮调用次数、超时、结果大小、取消和引用验证。
7. 使用统一评测集比较 Tavily 与腾讯/百度，再决定第二个 Client Tool 后端。
8. Tool loop 稳定后，单独设计 `web_fetch`；随后再研究百炼等 Provider-hosted Search。

这一路线让 Tavily 只承担检索，Vortex 自己承担 Agent 调度、上下文、错误、引用与展示，形成以后文件、Shell 和 MCP 工具都能复用的基础。

## 参考资料

- [OpenAI Web Search][openai-web-search]
- [Claude Code Agent SDK][claude-code-sdk]
- [Anthropic Web Search][anthropic-web-search]
- [Tavily Search API][tavily-search]、[Tavily 定价][tavily-pricing]
- [Brave Search API][brave-docs]
- [Exa API 定价][exa-pricing]
- [Serper][serper]
- [百度 AI 搜索][baidu-ai-search]
- [腾讯云联网搜索 API][tencent-wsa]
- [阿里云百炼联网搜索][bailian-web-search]
- [博查搜索 API 产品文档][bocha-doc]

[openai-web-search]: https://developers.openai.com/api/docs/guides/tools-web-search
[claude-code-sdk]: https://code.claude.com/docs/en/agent-sdk/python
[anthropic-web-search]: https://platform.claude.com/docs/zh-CN/agents-and-tools/tool-use/web-search-tool
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
[bocha-doc]: https://mkp-res.hc-cdn.com/marketplace/public/appv2/attachment/FE5/A96/96F/0000000000FE5A9696F.20250527042304.feac985174234808a69f64e899bd6792.pdf

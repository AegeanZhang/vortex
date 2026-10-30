# 网页搜索工具设计

核对日期：2026-10-30。状态：当前实现契约。架构归属：工具执行与资源访问。
源码入口：[metaso_web_search.rs](../../../crates/vortex-tools/src/metaso_web_search.rs)、[mock_web_search.rs](../../../crates/vortex-tools/src/mock_web_search.rs)。实际检查结果见根 [README](../../../README.md)，验收要求不代表已经通过。

当前提供同名 web_search 的秘塔与 Mock 两种实现，按配置只装配其一；默认秘塔，
不接入 Tavily。公共接口见 [工具注册表](tool-registry.md)，配置字段见 [配置设计](../configuration/configuration.md)。

## Mock 搜索（保留的闭环基线）

`MockWebSearch` 注册名为 `web_search`，Schema 要求字符串 `query`。实现手动检查该字段
并去除首尾空白，拒绝空查询；没有校验 Schema 的所有规则（如额外字段）。成功输出包含
`mock: true`、规范化后的查询和一条固定结果，URL 为 `https://example.invalid/result-1`。
它不会因查询变化而检索网页，也不代表模型侧托管搜索。显式 `search.backend="mock"`
才注册 Mock，不读搜索 key，不能与同名 MetasoWebSearch 同时注册；数量/超时设置不使
固定 Mock 变成真实检索或完整秘塔模拟。

## 秘塔搜索（默认实现）

`MetasoWebSearch` 仍注册为 `web_search`，只接受对象中的字符串 `query`，拒绝额外字段、
空查询和去除首尾空白后超过 512 个 Unicode 字符的查询。HTTP 留在 Tools，Core、Provider
和 TUI 不认识厂商 DTO；CLI 从 `search.api_key_env` 指定的环境变量读取独立凭据，默认
`METASO_API_KEY`，并把配置映射成工具选项。

工具向固定 HTTPS 搜索地址发送 `q`，范围为 `webpage`，size 为配置条数的字符串（默认
`"5"`，允许 `"1"`～`"5"`），关闭摘要
增强与来源全文抓取，不调用秘塔问答接口。认证头标记 sensitive，类型使用脱敏 Debug；
普通日志只记录后端、状态码、耗时、字节数和脱敏错误，不记录查询或响应正文。
显式 Debug wire TRACE 可记录请求和原始正文，见 [开发期诊断](#开发期诊断)。

当前适配器读取 `webpages` 数组以及 `link/title/snippet`；缺少数组或非法 JSON 是协议
错误，合法空数组是成功的空结果。成功 ToolResult.output 是 JSON 字符串，包含
`backend: "metaso"`、`query`、`results`、`truncated`，结果仅有 `title/url/snippet`。
仅接受无内嵌凭据的 HTTP(S) URL，去重保留首条；不自动抓取返回 URL。片段是不可信数据，
不是 Agent 指令，链接是否支持最终回答仍需核对。

具体工具的边界如下，不改变注册表公共接口：

- 连接超时默认 5 秒，配置允许 1～30 秒；请求与正文读取总超时默认 20 秒，允许 1～120
  秒，连接超时不得大于总超时；不自动重试或跟随重定向。
- HTTP 成功正文最多 256 KiB，逐 chunk 检查；成功响应须为 JSON 类型。
- 按配置返回最多 1～5 条，硬上限 5；即使后端多返回也按配置裁剪并标记 truncated。
  标题最多 160 字符、片段最多 800 字符、URL 最多 2,048 字节。
- 最终序列化 JSON 最多 16 KiB；超预算移除末尾结果并标记截断，不切断 JSON 字符串。
- HTTP、传输、协议及超时错误通过现有 ToolError 转成错误 ToolResult，并回填模型。
- 正常业务请求直接 await；诊断错误预览可短暂独立运行，见下文。Shutdown 中止 `run_turn` 时丢弃等待中的 future，
  不承诺远端停止工作或不扣费。

正常搜索与完整响应契约的验证范围见根 README。HTTP 200 内的业务错误、空结果和可空
字段应以后续脱敏样例确认，不凭正常搜索一次成功认定所有异常响应均已覆盖。

## 开发期诊断

秘塔接入共享 [WireTrace](../cross-cutting/diagnostics.md)，仅 Debug + `vortex_wire=trace`
开启。记录与发送使用同一请求 DTO；状态后记录有界读取完成、规范化前的原始正文，因此
坏 JSON 也可排查。读取失败或超出 256 KiB 时记录 body_unavailable，不为日志重读正文。
HTTP 非成功或 MIME 错误启动最多 16 KiB/300 ms 的后台预览，业务仍立即返回原错误。

请求/响应诊断共享一个本地 request_id，已知 key 与 Bearer 字面值在副本中替换；原业务
请求与工具输出不变。正文可含查询和检索片段，仍需遵守诊断设计的隐私边界。
诊断不放宽正文/输出预算和工具总超时；后台预览不属于 Turn 取消集合，退出可能丢失其日志。
Mock 不发 HTTP，也没有 search wire 请求记录。

## 接入决策与限制解释

首版直接使用 HTTP API，而非 MCP：已有 ToolRegistry 与执行循环，只需新增 Tool
适配器；项目尚无 MCP client、传输、认证和生命周期治理。MCP 是工具接入协议，不自动
提高搜索质量或降低费用；未来需要复用外部 MCP 工具时再独立设计，不改 Core 的工具契约。

同名 web_search 每次只装配一个实现；disabled/Mock 不读秘塔 Key，启用秘塔却缺 key 时
在进入 TUI 前明确失败，不退回 Mock。旧 exec 不新增搜索认证要求，并拒绝 TUI 专用
`-m/-c`。CLI 不自动加载 .env，本地脚本负责导出。
模型不看到凭据或厂商端点，也不能通过工具参数指定任意 endpoint。

响应映射补充规则：title 缺失/空白回退 URL，snippet 缺失/null 为空；规范化后的 URL
去重，不编造网页正文。非空来源列表中若仅部分 URL 无效则丢弃并标记 truncated，全部
无效则失败。文字裁剪按字符，最终输出预算按序列化 UTF-8 字节计算。控制字符过滤只在
搜索映射局部生效，不代表最终模型文本或整条 TUI 链路已有完整防护。

256 KiB 是正文消费上限，不承诺 HTTP/TLS 缓冲完全不占额外内存。可配置的异步 timeout（默认 20 秒）
包围请求和正文读取，但不能抢占同步 JSON 解析；映射仍依赖小体积预算控制工作量。
web_search 默认每用户 Turn 4 次尝试，由 Core 计数，秘塔与 Mock 规则相同，可用用户
tools.call_limits 覆盖。批次先预检、执行前扣额，参数或网络错误不退额；它与单请求超时、
结果条数、响应大小、输出大小及实际收费统计不同，不能相互替代。

只有第二个真实搜索后端出现且确有重复时，再提取 WebSearchBackend 和共用结果类型；
通用网络预算在多个工具有共同需求时下沉。TOML 已承载搜索开关、后端、结果数量和超时，
密钥值仍来自环境。工具构造入口独立校验数量/超时，256 KiB/16 KiB 等硬限制不开放配置。
搜索、抓取网页与问答是不同能力，不因返回 URL 就增加来源抓取或 SSRF 风险。
若以后引入抓取，须另设路径/网络/重定向和输出边界。

## 验收要求

验证参数、空结果、坏响应、URL 去重/裁剪、数量与字节预算、超时、取消、认证及日志脱敏。
使用本地 HTTP fixture 与假凭据，不访问付费 API；Mock 搭配真实模型不属于完全离线测试。
字段可空性与业务错误需要脱敏真实样例确认；作者反馈正常搜索可用不等于全部契约已验收。
具体矩阵见 [回归测试方案](../../engineering/regression-test-plan.md)，候选服务和证据边界见
[Web Search 调研](../../research/web-search-research.md)。

本文回收原 Agent loop 中的秘塔/Mock 契约及本地 `20261002-01.md` 的稳定设计；
搜索失败不能通过静默回退 Mock 伪造成功。

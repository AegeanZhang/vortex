# Agent loop 与搜索工具设计

本文描述工具版 v1 的执行契约。运行时所有权见 [Session](session-runtime.md)，流式协议见
[SSE](streaming-response.md)，完整上下文构建目标见 [Message](message-building.md)。
实现与验收进度统一记录在根 [README](../../README.md)。

## 范围与职责

一个 Turn 从有效用户输入开始，可包含多个模型步骤和工具步骤，直到得到最终回答或失败。
v1 串行执行：每个模型步骤最多一个 ToolCall，每个 Turn 最多执行 4 次工具调用；可以在
第四次工具完成后再请求模型生成最终回答，第五次调用在执行前被拒绝。普通文本回答无需工具。

| 模块 | 职责 |
| --- | --- |
| `vortex-core/src/tool.rs` | Tool trait、规范化调用/结果、ToolRegistry |
| `vortex-core/src/agent_loop.rs` | 模型与工具交替执行，校验事件序列与调用 ID |
| `vortex-core/src/session.rs` | 命令串行化、权威历史、进度事件投影和 Turn 收尾 |
| `vortex-provider/src/openai_chat/` | 请求映射、SSE 工具增量组装；不执行工具 |
| `vortex-tools/src/mock_web_search.rs` | 固定搜索结果，用于验证执行路径 |
| `vortex-tools/src/metaso_web_search.rs` 及子模块 | 秘塔 HTTP 搜索、请求/响应映射与工具内边界限制 |
| `vortex-cli/src/main.rs` | 具体 Provider/Tool 装配；TUI 不认识它们 |

Mock v1 是最初的闭环基线；当前 CLI 默认注册秘塔工具，复用同一执行契约，不接入 Tavily。
仍不实现并行工具、审批、沙箱、通用 JSON Schema 校验、注册表统一超时或总 Token 预算。
秘塔工具单独实现参数校验、请求超时与输出限制；不能把这些推断为所有 Tool 都已受治理。

## 工具接口与注册表

Core 定义以下协议无关类型：

```rust
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub struct ToolResult {
    pub call_id: String,
    pub output: String,
    pub is_error: bool,
}

pub type ToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + 'a>>;

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, arguments: serde_json::Value) -> ToolFuture<'_>;
}
```

`ToolRegistry` 持有 `Vec<Arc<dyn Tool>>`：`empty()` 供无工具路径使用；`new()` 拒绝重名
工具；`specs()` 按注册顺序返回定义；`execute(&ToolCall)` 按名字查找并执行。
未知工具、参数错误和执行错误统一转换为保留原 `call_id` 的错误 ToolResult，不直接结束
Turn。工具只处理参数，不负责生成调用 ID；注册表不自动执行 Schema 校验。

### Mock 搜索（保留的闭环基线）

`MockWebSearch` 注册名为 `web_search`，Schema 要求字符串 `query`。实现手动检查该字段
并去除首尾空白，拒绝空查询；没有校验 Schema 的所有规则（如额外字段）。成功输出包含
`mock: true`、规范化后的查询和一条固定结果，URL 为 `https://example.invalid/result-1`。
它不会因查询变化而检索网页，也不代表模型侧托管搜索。CLI 默认不注册 Mock，不能与
同名的 `MetasoWebSearch` 同时加入注册表。

### 秘塔搜索（默认实现）

`MetasoWebSearch` 仍注册为 `web_search`，只接受对象中的字符串 `query`，拒绝额外字段、
空查询和去除首尾空白后超过 512 个 Unicode 字符的查询。HTTP 留在 Tools，Core、Provider
和 TUI 不认识厂商 DTO；CLI 从 `METASO_API_KEY` 读取独立凭据并装配工具。

工具向固定 HTTPS 搜索地址发送 `q`，范围为 `webpage`，数量为字符串 `"5"`，关闭摘要
增强与来源全文抓取，不调用秘塔问答接口。认证头标记 sensitive，类型使用脱敏 Debug；
日志只记录后端、状态码、耗时、字节数和脱敏错误，不记录查询或响应正文。

当前适配器读取 `webpages` 数组以及 `link/title/snippet`；缺少数组或非法 JSON 是协议
错误，合法空数组是成功的空结果。成功 ToolResult.output 是 JSON 字符串，包含
`backend: "metaso"`、`query`、`results`、`truncated`，结果仅有 `title/url/snippet`。
仅接受无内嵌凭据的 HTTP(S) URL，去重保留首条；不自动抓取返回 URL。片段是不可信数据，
不是 Agent 指令，链接是否支持最终回答仍需核对。

具体工具的边界如下，不改变注册表公共接口：

- 连接最多 5 秒，单次请求与正文读取总超时 20 秒；不自动重试或跟随重定向。
- HTTP 成功正文最多 256 KiB，逐 chunk 检查；成功响应须为 JSON 类型。
- 最多返回 5 条，标题最多 160 字符、片段最多 800 字符、URL 最多 2,048 字节。
- 最终序列化 JSON 最多 16 KiB；超预算移除末尾结果并标记截断，不切断 JSON 字符串。
- HTTP、传输、协议及超时错误通过现有 ToolError 转成错误 ToolResult，并回填模型。
- 请求直接 await，不另起后台任务；Shutdown 中止 `run_turn` 时丢弃等待中的 future，
  不承诺远端停止工作或不扣费。

正常搜索与完整响应契约的验证范围见根 README。HTTP 200 内的业务错误、空结果和可空
字段应以后续脱敏样例确认，不凭正常搜索一次成功认定所有异常响应均已覆盖。

#### 接入决策与限制解释

首版直接使用 HTTP API，而非 MCP：已有 ToolRegistry 与执行循环，只需新增 Tool
适配器；项目尚无 MCP client、传输、认证和生命周期治理。MCP 是工具接入协议，不自动
提高搜索质量或降低费用；未来需要复用外部 MCP 工具时再独立设计，不改 Core 的工具契约。

同名 web_search 每次只装配一个实现。缺少秘塔 Key 时在进入 TUI 前明确失败，不偷偷
退回 Mock；旧 exec 不因此新增秘塔认证要求。CLI 不自动加载 .env，本地脚本负责导出。
模型不看到凭据或厂商端点，也不能通过工具参数指定任意 endpoint。

响应映射补充规则：title 缺失/空白回退 URL，snippet 缺失/null 为空；规范化后的 URL
去重，不编造网页正文。非空来源列表中若仅部分 URL 无效则丢弃并标记 truncated，全部
无效则失败。文字裁剪按字符，最终输出预算按序列化 UTF-8 字节计算。控制字符过滤只在
搜索映射局部生效，不代表最终模型文本或整条 TUI 链路已有完整防护。

256 KiB 是正文消费上限，不承诺 HTTP/TLS 缓冲完全不占额外内存。20 秒异步 timeout
包围请求和正文读取，但不能抢占同步 JSON 解析；映射仍依赖小体积预算控制工作量。
四次工具轮数、单请求超时、响应大小与输出大小是不同限制，不能相互替代。

只有第二个真实搜索后端出现且确有重复时，再提取 WebSearchBackend 和共用结果类型；
通用网络预算在多个工具有共同需求时下沉。TOML 配置未来承载非密钥选项，密钥仍保持
独立来源。搜索、抓取网页与问答是不同能力，不因返回 URL 就增加来源抓取或 SSRF 风险。
若以后引入抓取，须另设路径/网络/重定向和输出边界。

## 多步执行与历史

```text
User → ModelRequest(items, tools)
     → ToolCall + Finished(ToolCalls)
     → ToolStarted → ToolRegistry::execute → ToolFinished
     → 回填 Assistant 前缀（如有）、ToolCall、ToolResult
     → 再次 ModelRequest
     → TextDelta… + Finished(Stop)
     → TurnOutput → Session 提交最终 Assistant → TurnCompleted
```

`run_turn(provider, tools, history, progress)` 返回 `Result<TurnOutput, TurnError>`。
它拥有权威历史的副本 `context`，每个模型步骤构造 `ModelRequest { items, tools }`。
模型正文一边累积、一边作为 `TurnProgress::TextDelta` 发给 Session；思考增量在阶段 A
直接返回协议错误。每一步必须有结束事件，不能把 EOF 当作成功。

- `ToolCalls`：要求恰好一个调用，校验轮数和 ID 后才执行工具；结果包括错误结果均回填，
  再次调用模型。当前步骤的非空 Assistant 前缀随调用/结果一起提交给 Session。
- `Stop`：不得携带 ToolCall，最终正文必须非空；返回完整 Assistant，由 Session 只写入一次。
- `Length`、`ContentFilter`、Provider 错误、缺少 Finished 或不合法事件组合：返回 TurnError。

`ToolFinished` 同时携带前缀、调用和结果，Session 接收时按同样顺序写入权威历史。
如果后续模型步骤失败，已经完成的工具步骤及其 Assistant 前缀保留；失败模型步骤的部分
正文不进入历史，TUI 则可保留已经显示的片段。已接受的 User 也不回滚。

## 调用 ID 与错误边界

`run_turn` 在循环外从历史 ToolCall 收集 `BTreeSet<String>`，执行前拒绝空白 ID，并用
`insert()` 拒绝重复 ID。集合跨模型步骤保留，工具完成或返回错误也不释放 ID；不修改
Provider 的原始 ID。非法 ID 在 ToolStarted、工具执行和历史写入之前被拒绝，不生成带
重复 ID 的错误 ToolResult。以后裁剪历史时应将去重状态提升到 Session，避免遗忘已使用 ID。

轮数上限是防止无限工具循环的首版限制，不是总时长限制；等待模型或工具仍可能很长。
Shutdown 关闭整个 Session 并中止 `run_turn`，不承诺生成取消 ToolResult 或额外 Turn 终态。
未来带副作用工具必须另行设计超时、取消、安全边界与审批，不能直接复用 Mock 的安全假设。

## Provider 与 TUI 映射

Chat Completions 将 ToolSpec 映射为 `tools[].function`；ToolCall 映射为 Assistant 的
`tool_calls`；结果映射为 `role: "tool"`、`tool_call_id` 和文本 `content`。相邻的 Assistant
前缀与 ToolCall 合并进同一条 wire message；结构化 arguments 仅在协议边界序列化为 JSON
字符串。`is_error` 保留于内部类型和 UI 事件，wire 层通过结果正文表达错误。

Provider 按 index 累积工具参数，到 `[DONE]` 才输出完整 ToolCall 和 Finished；当前只接受
index 0。流中的空 ID 不覆盖已保存的 ID，不同非空 ID 仍报错，详见 SSE 设计。

TUI 在 ToolCallStarted/ToolCallFinished 时只更新 notice 并重绘，Session 仍为 Running。
所有模型步骤的文本增量追加到同一条 Assistant entry；没有工具卡片、步骤分隔或结果来源
展示。TurnCompleted/TurnFailed 才修改终态。Mock 工具很快，开始状态可能只短暂出现。

## 测试契约与后续接入

以下是待覆盖的契约，不是通过记录：

- FakeProvider 依次给出调用和最终回答：第二次请求含正确的调用/结果，事件顺序稳定。
- 工具错误仍回填并再次调用模型；未知工具、重名工具、参数错误分别可观测。
- 第五次调用被拒绝；空白/历史重复/当前 Turn 重复 ID 都不执行工具。
- SSE arguments 跨包拼接、后续空 ID、真实非空 ID 变化、非法 JSON 和缺少 DONE。
- Session 保留已完成工具步骤，失败部分回复不入历史；退出和 UI 背压不阻塞取消。
- TUI 工具事件不新增 Transcript entry、不改变 Running 状态，完成/失败后清除 notice。

测试使用受控 fake 和本地 fixture，不访问付费 API。秘塔已通过替换 CLI 注册的 Tool
接入；后续服务适配仍应在 Tools 内转换请求/结果，不在 Agent loop 中加入厂商字段。
搜索的空结果、错误响应、超时、取消与输出预算应补专项回归。服务候选及证据边界见
[Web Search 调研](../research/web-search-research.md)。

参数、映射、HTTP 异常、取消和 fixture 组织的具体步骤见
[回归测试方案](../engineering/regression-test-plan.md)。本文回收了本地
`20260927-01.md` 与 `20261002-01.md` 的稳定契约；旧迁移桥接代码不再作为当前接口参考。

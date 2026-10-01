# Agent loop 与 Mock 工具设计

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
| `vortex-cli/src/main.rs` | 具体 Provider/Tool 装配；TUI 不认识它们 |

v1 不接入 Tavily，不实现并行工具、审批、沙箱、工具超时、通用 JSON Schema 校验、输出
截断或总 Token 预算。模型请求仍访问真实 Qwen 服务；只有搜索工具的数据是 Mock。

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

`MockWebSearch` 注册名为 `web_search`，Schema 要求字符串 `query`。实现手动检查该字段
并去除首尾空白，拒绝空查询；没有校验 Schema 的所有规则（如额外字段）。成功输出包含
`mock: true`、规范化后的查询和一条固定结果，URL 为 `https://example.invalid/result-1`。
它不会因查询变化而检索网页，不代表真实搜索服务已经可用。

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

测试使用受控 fake 和本地 fixture，不访问付费 API。接入真实搜索时替换 CLI 注册的 Tool
实现，并补统一结果结构、超时、输出限制、隐私与引用策略；不要在 Agent loop 中加入
Tavily 特有字段。服务候选见 [Web Search 调研](../research/web-search-research.md)。

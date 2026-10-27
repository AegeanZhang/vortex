# Agent loop 与单轮执行设计

核对日期：2026-10-26。状态：当前实现契约。架构归属：Turn 执行与历史构建。
源码入口：[agent_loop.rs](../../../crates/vortex-core/src/agent_loop.rs)、[conversation.rs](../../../crates/vortex-core/src/conversation.rs)。实际检查结果见根 [README](../../../README.md)，验收要求不代表已经通过。

本文件维护单轮执行、不变量和局部上下文；会话所有权见 [Session](session-runtime.md)，
工具接口见 [注册表](../tools/tool-registry.md)，流协议见 [SSE](../provider/streaming-response.md)。

## 范围与职责

一个 Turn 从有效用户输入开始，可包含多个模型步骤和工具步骤，直到得到最终回答或失败。
v1 串行执行：每个模型步骤最多一个 ToolCall，每个 Turn 最多执行 4 次工具调用；可以在
第四次工具完成后再请求模型生成最终回答，第五次调用在执行前被拒绝。普通文本回答无需工具。

Core 通过 ModelProvider 和 ToolRegistry 调用注入能力，不认识具体搜索厂商或终端控件。
整个 Turn 固定 Provider 与工具注册表，不引入可变 ModelManager。尚无并行工具、
通用 Schema 校验、审批、统一工具超时或 Token 预算。
单次响应的多个工具调用仍不支持，与整个 Turn 四次工具预算不同；请求未发送
parallel_tool_calls=false，已知问题见 [Issue #3](https://github.com/AegeanZhang/vortex/issues/3)。

## 多步执行与历史

```text
User → ModelStepStarted(1) → ModelRequest(items, tools)
     → ReasoningDelta*（仅 DisplayOnly）、TextDelta*（可选前缀）
     → ToolCall + Finished(ToolCalls)
     → ToolStarted → ToolRegistry::execute → ToolFinished
     → 回填 Assistant 前缀（如有）、ToolCall、ToolResult
     → ModelStepStarted(2) → 再次 ModelRequest
     → ReasoningDelta*（仅 DisplayOnly）、TextDelta… + Finished(Stop)
     → TurnOutput → Session 提交最终 Assistant → TurnCompleted
```

`run_turn(provider, tools, history, progress)` 返回 `Result<TurnOutput, TurnError>`。
它拥有权威历史的副本 `context`，每个模型步骤构造 `ModelRequest { items, tools }`。
每次请求前发布 ModelStepStarted，步骤从 1 开始。模型正文一边累积、一边作为
`TurnProgress::TextDelta` 发给 Session。Provider 的 reasoning_output_policy 在本 Turn
开始时读取一次：Reject 收到思考即失败；DisplayOnly 只计字节并转发独立思考增量，
不写 Assistant 前缀、最终回答或请求历史。每模型步骤上限 256 KiB UTF-8 字节，超限
增量不发布并使 Turn 失败；新步骤重新计数，详见 [Thinking 展示](../tui/thinking-display.md)。
每一步必须有结束事件，不能把 EOF 当作成功；只有思考、没有最终正文的 Stop 也失败。

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

## 与其他模块的衔接

Provider 的工具 wire 映射与增量组装见 [SSE 设计](../provider/streaming-response.md)；注册表将工具
业务错误转为结果，具体目录与搜索边界分别见 [目录工具](../tools/list-directory.md)、[网页搜索](../tools/web-search.md)。
TUI 根据 ModelStepStarted 划分或复用 Assistant 展示条目，同一步追加正文/思考；
工具开始/完成只更新状态，不自行创建条目。具体投影见 [Thinking 展示](../tui/thinking-display.md)。

## 测试契约与后续接入

- FakeProvider 的第二次请求应包含前缀、ToolCall、ToolResult，最终回答只提交一次。
- 未知工具及参数/执行错误可回填并继续；第五次调用、空白/历史重复/跨步骤重复 ID 在执行前拒绝。
- 缺少 Finished、非法结束原因组合、Provider 失败、仅思考无正文与思考超限不能转为成功。
- Session 保留已完成工具步骤，失败部分正文不入历史；退出与 UI 背压不阻塞关闭。
- 使用受控流、FakeTool 和假凭据，不要求真实模型确定地选择工具。

具体矩阵和覆盖边界见 [回归测试方案](../../engineering/regression-test-plan.md)，扩展历史模型见
[消息构建](message-building.md)。新增具体工具通过注册表接入，不在 loop 中添加厂商字段。
本文保留原 `20260927-01.md` 的稳定执行契约，旧桥接代码不再维护。

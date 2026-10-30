# Agent loop 与单轮执行设计

核对日期：2026-10-30。状态：当前实现契约。架构归属：Turn 执行与历史构建。
源码入口：[agent_loop.rs](../../../crates/vortex-core/src/agent_loop.rs)、[tool_budget.rs](../../../crates/vortex-core/src/tool_budget.rs)、[conversation.rs](../../../crates/vortex-core/src/conversation.rs)。实际检查结果见根 [README](../../../README.md)，验收要求不代表已经通过。

本文件维护单轮执行、不变量和局部上下文；会话所有权见 [Session](session-runtime.md)，
工具接口见 [注册表](../tools/tool-registry.md)，流协议见 [SSE](../provider/streaming-response.md)。

## 范围与职责

一个 Turn 从有效用户输入开始，可包含多个模型步骤和工具步骤，直到得到最终回答或失败。
每个模型步骤可以返回多个 ToolCall，组成一个批次；本地按批次中的顺序逐个 await，
全部结果回填后再请求模型。普通文本回答无需工具，工具执行不是新的用户 Turn。

Core 通过 ModelProvider 和 ToolRegistry 调用注入能力，不认识具体搜索厂商或终端控件。
整个 Turn 固定 Provider 与工具注册表，不引入可变 ModelManager。尚无并行工具、
通用 Schema 校验、审批、统一工具超时或 Token 预算。
请求不发送 `parallel_tool_calls=false`；模型返回多个调用与本地并行执行是两件事。
不增加工具类型或并发调度元数据，也不把调用次数当作模型请求次数或精确费用。

## 多步执行与历史

```text
User → ModelStepStarted(1) → ModelRequest(items, tools)
     → ReasoningDelta*（仅 DisplayOnly）、TextDelta*（可选前缀）
     → ToolCall* + Finished(ToolCalls)（至少一个调用）
     → 全批 ID 校验与预算预检 → ToolBatchStarted(content, calls)
     → 对每个调用：扣额 → ToolStarted → execute → ToolFinished → 回填 ToolResult
     → ModelStepStarted(2) → 再次 ModelRequest
     → ReasoningDelta*（仅 DisplayOnly）、TextDelta… + Finished(Stop)
     → TurnOutput → Session 提交最终 Assistant → TurnCompleted
```

`run_turn(provider, tools, history, progress, limits)` 返回 `Result<TurnOutput, TurnError>`。
它拥有权威历史的副本 `context`，每个模型步骤构造 `ModelRequest { items, tools }`。
每次请求前发布 ModelStepStarted，步骤从 1 开始。模型正文一边累积、一边作为
`TurnProgress::TextDelta` 发给 Session。Provider 的 reasoning_output_policy 在本 Turn
开始时读取一次：Reject 收到思考即失败；DisplayOnly 只计字节并转发独立思考增量，
不写 Assistant 前缀、最终回答或请求历史。每模型步骤上限 256 KiB UTF-8 字节，超限
增量不发布并使 Turn 失败；新步骤重新计数，详见 [Thinking 展示](../tui/thinking-display.md)。
每一步必须有结束事件，不能把 EOF 当作成功；只有思考、没有最终正文的 Stop 也失败。

- `ToolCalls`：要求非空批次，全部 ID 校验与预算预检通过后才声明及执行；错误结果也回填，
  整批完成后再次调用模型。非空 Assistant 前缀与全部调用组成一个历史项。
- `Stop`：不得携带 ToolCall，最终正文必须非空；返回完整 Assistant，由 Session 只写入一次。
- `Length`、`ContentFilter`、Provider 错误、缺少 Finished 或不合法事件组合：返回 TurnError。

`ToolBatchStarted { content, calls }` 让 Session 一次写入 `AssistantToolCalls`；随后
`ToolFinished { call, result }` 逐项提交真实结果，不重复写前缀或声明。Turn 的局部 context
以同一结构更新；两者不是共享 Vec。后续模型失败时已完成工具历史保留，失败步骤的部分
正文不入历史。批次中途异常由 Session 补齐未完成结果，见 [Session](session-runtime.md)。

## 每工具、每 Turn 的调用额度

`ToolCallLimits` 是可克隆规则，`TurnToolBudget` 是每次 run_turn 新建的使用计数。
规则在 Session 内固定，切换模型不重置当前任务或改配置；新用户 Turn 重新计数。

| 规则 | 当前行为 |
| --- | --- |
| 内置搜索 | `web_search=4`，秘塔与 Mock 同名共用该默认规则 |
| 未显式配置的工具 | 每种工具每 Turn 默认 50 次，常量 `DEFAULT_TOOL_CALL_LIMIT_PER_TURN` |
| 用户覆盖 | `tools.call_limits` 按工具名覆盖；正 u32，可大于 50，不支持零值或无限制 |
| 批次预检 | 聚合已注册工具的本批次数；任一剩余额度不足，整批不执行、不写声明 |
| 扣额 | 每个已注册工具进入执行前扣一次；参数、业务或网络错误均不退还 |
| 未知工具 | 仍生成错误 ToolResult，不扣实际执行额度 |

`preflight` 不扣额；通过后才记录批次，再逐项 `consume`。超限返回
`TurnError::ToolBudgetExceeded`，包含工具名、上限、已用次数和本批需求，本轮失败；
不挑选部分调用继续执行。额度耗尽后仍可请求模型给最终正文，不能再执行同名工具。
用户字段、合并与重启规则见 [配置设计](../configuration/configuration.md#工具调用额度)。

这不是完整防失控系统：尚无全局模型步骤、Token、时长或费用预算；反复调用未知工具
也可能继续循环。一次 ToolCall 内的 HTTP 重试或未来 Bash 的内部命令不逐个计数。

## 调用 ID 与错误边界

`run_turn` 在循环外从历史 AssistantToolCalls 收集 `BTreeSet<String>`，整批执行前拒绝
空白 ID、批内重复及历史/本轮跨步骤重复 ID。集合跨模型步骤保留，错误也不释放 ID；不修改
Provider 的原始 ID。非法 ID 在 ToolStarted、工具执行和历史写入之前被拒绝，不生成带
重复 ID 的错误 ToolResult。以后裁剪历史时应将去重状态提升到 Session，避免遗忘已使用 ID。

调用额度不是总时长限制；等待模型或工具仍可能很长。
Shutdown 关闭整个 Session 并中止 `run_turn`，不承诺生成取消 ToolResult 或额外 Turn 终态。
未来带副作用工具必须另行设计超时、取消、安全边界与审批，不能直接复用 Mock 的安全假设。

## 与其他模块的衔接

Provider 的工具 wire 映射与增量组装见 [SSE 设计](../provider/streaming-response.md)；注册表将工具
业务错误转为结果，具体目录与搜索边界分别见 [目录工具](../tools/list-directory.md)、[网页搜索](../tools/web-search.md)。
TUI 根据 ModelStepStarted 划分或复用 Assistant 展示条目，同一步追加正文/思考；
工具开始/完成只更新状态，不自行创建条目。具体投影见 [Thinking 展示](../tui/thinking-display.md)。

## 测试契约与后续接入

- 第二次请求应含一条 AssistantToolCalls 和全部对应结果，前缀、最终回答不重复；执行严格顺序。
- 验证整批超限不执行、每工具独立、默认 50/51 边界、失败不退额及新 Turn 重置。
- 未知工具返回错误并继续；空白、批内/跨步骤/历史重复 ID 在整批执行前拒绝。
- 缺少 Finished、非法结束原因组合、Provider 失败、仅思考无正文与思考超限不能转为成功。
- Session 保留成功结果，异常补齐已开始但未知与未开始的调用；退出与 UI 背压不阻塞关闭。
- 使用受控流、FakeTool 和假凭据，不要求真实模型确定地选择工具。

具体矩阵和覆盖边界见 [回归测试方案](../../engineering/regression-test-plan.md)，扩展历史模型见
[消息构建](message-building.md)。新增具体工具通过注册表接入，不在 loop 中添加厂商字段。
本文保留原 `20260927-01.md` 的稳定执行契约，旧桥接代码不再维护。

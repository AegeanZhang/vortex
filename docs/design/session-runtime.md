# Session Runtime 设计

本文档描述文本流式版本中 `vortex-core` 的 Session runtime，以及它与
`vortex-provider` 的边界。模型上下文未来如何裁剪和压缩，见
[`message-building.md`](message-building.md)；TUI 如何消费 Session，见
[`tui-interface.md`](tui-interface.md)。SSE 协议、Thinking 阶段和后续刷新策略见
[`streaming-response.md`](streaming-response.md)；本文只描述阶段 A 的运行时契约，
实现与验收进度以根 [`README.md`](../../README.md) 为准。

## 当前范围

Session runtime 负责保存对话事实、串行化命令、调用模型并发布语义事件。它不知道
HTTP、OpenAI wire format 或终端控件。每次只运行一个模型流，支持增量正文和整个 Session
关闭；不包含 Thinking 展示、工具调用、保留会话的 Turn 取消、持久化、恢复和模型切换。

```text
CLI：装配 Arc<dyn ModelProvider>，调用 start_session
                       │ SessionConnection
                       v
TUI ──AgentHandle（命令 / 关闭信号）──> Session actor
TUI <────────CoreEventStream───────── Session actor
                                          │ ModelRequest
                                          v
                                     Provider task
                                          │ ModelEvent / Err
                                          v
                                     Session actor
```

## 核心接口

| 类型 | 当前职责 |
| --- | --- |
| `ModelProvider` | `stream(ModelRequest)` 返回惰性 `ModelStream`，流元素为 `Result<ModelEvent, ProviderError>` |
| `ModelEvent` | `TextDelta`、`ReasoningDelta` 和携带 `FinishReason` 的 `Finished`；阶段 A 拒绝思考增量 |
| `SessionConnection` | 一次性交付初始 `SessionSnapshot`、命令句柄和独占事件流 |
| `AgentHandle` | 可克隆的命令入口；业务命令走有界 channel，Shutdown 走独立 watch 信号 |
| `CoreEventStream` | 单消费者事件入口；隐藏接收端实现 |
| `AgentCommand` | 当前只有 `SubmitPrompt` 和 `Shutdown` |
| `CoreEvent` | 描述用户消息、Assistant 生命周期、完成、失败和命令拒绝 |

`start_session(Arc<dyn ModelProvider>)` 创建容量为 32 的命令 channel 和容量为 128 的
事件 channel，并启动一个 Tokio task；每个 Turn 另建容量为 32 的模型事件 channel。
初始 Snapshot 固定为空消息和 `Idle`；当前尚未实现
历史恢复，因此这里还不承担“快照与订阅原子建立”的完整语义。

## 所有权与并发

Session actor 独占 `messages`、`status`、`ActiveTurn` 和 `JoinSet<()>`，其他模块不能
直接修改这些状态。事件循环使用 `tokio::select!` 等待命令、单轮模型事件、关闭信号和
UI 消费者关闭；先得到 `RuntimeInput`，再更新状态，避免跨分支借用活跃 Turn。

- `messages` 是模型请求的事实来源；只有已接受的用户消息和成功回复会写入。
- `status` 为 `Running` 时拒绝新的 Prompt，因此逻辑上最多只有一个活跃 Turn。
- `ActiveTurn` 保存接收端、累积正文和待确认 outcome；思考缓冲仅预留，不参与阶段 A。
- Provider task 只转发事件，发送 Finished/Err 后返回；它独占发送端，不直接修改历史。
- Finished/Err 只记录 outcome；actor 等通道排空并关闭，再回收 task、校验并发布终态。
- `Shutdown`、所有命令句柄关闭或事件消费者消失时，actor 中止活跃 task 并退出。

向 UI 发布事件统一经过 `publish_event`，同时监听关闭信号。即使 UI 队列已满，Shutdown
也不必排在业务命令后等待。`AgentHandle::send(Shutdown)` 成功只代表通知成功，不代表任务
已回收；已有入队事件仍可能被读到。关闭 Session 不承诺额外发送 Turn 终态。

## Turn 状态与事件顺序

有效 Prompt 的成功路径固定为：

```text
SubmitPrompt
  -> UserMessageAdded
  -> AssistantMessageStarted
  -> ModelProvider::stream
  -> AssistantTextDelta（一次或多次）
  -> Finished(Stop)，内部通道排空并关闭，task 正常结束
  -> TurnCompleted
```

收到命令后先用 `trim()` 判断是否为空，但保留原始内容。空 Prompt 发出
`CommandRejected`；`Running` 期间再次提交也发出 `CommandRejected`。两种拒绝都不修改
消息历史和 Session 状态。

图中 Finished 是 Provider 事件，不直接发给 TUI。仅在 task 正常结束、结束原因为 `Stop`
且累积正文非空时成功；完整 Assistant 只写入历史一次，不重发完整正文 Delta。

Provider 错误、缺少终态、task 异常、`Length`、`ContentFilter` 或空正文均导致 `Failed`
和一次 `TurnFailed`。阶段 A 收到 `ReasoningDelta` 时记录协议错误，关闭本轮接收端并
中止 task；忽略已排队的后续内容，再统一收尾，不等远端继续生成完毕。

本轮已接受的 User 消息保留，但失败 Assistant 不进入 `messages`；TUI 可保留已显示的
片段并标记错误。`Failed` 不阻止下一次提交：下一条有效 Prompt 重新进入 `Running`，
成功后回到 `Idle`。下一轮可能包含连续 User 消息，当前不回滚也不自动重试。

## Provider 适配边界

`vortex-provider::OpenAiChatProvider` 是当前唯一实现。它把 `MessageRole` 映射为
OpenAI-compatible Chat Completions 的 `user`、`assistant`、`system`，发送
`stream: true`，将 SSE choice 0 的正文转换为 `TextDelta`。泛用构造器省略千问专用参数；
CLI 用 `QwenThinkingOptions` 显式关闭思考及思考历史。仅改变该开关不能完成 Thinking 接入。

Provider 负责 HTTP 状态、SSE Content-Type、分帧与 JSON 校验，并要求合法 `finish_reason`
加 `[DONE]` 才产生 Finished。等待响应头上限为 30 秒，每次等待下一个 body chunk 的空闲
上限为 120 秒；没有总时长限制或自动重连。空 choices 统计包可忽略，不能当作空回复错误。

网络故障映射为 `Transport`，非成功 HTTP 状态映射为 `HttpStatus`，JSON 或回复内容不合法
映射为 `InvalidResponse`。API key 由 CLI 从指定进程环境变量注入，Provider 的
`SecretString` 调试输出固定为 `***`；产品本身不读取 `.env`。

## 单元测试矩阵

通用组织方式和执行命令见 [`../engineering/testing.md`](../engineering/testing.md)。测试不得
调用真实付费 API。下表是应覆盖的契约，不代表全部已有测试；覆盖现状见根 README。

| 模块与位置 | 场景 | 关键断言 |
| --- | --- | --- |
| `vortex-core/src/session.rs` | 成功 Turn | Started 后多次 Delta，最后一次终态；首段先于模型结束到达 |
| 同上 | 连续两个成功 Turn | 第二次请求包含此前 User/Assistant 和本轮 User，顺序不变 |
| 同上 | 空白 Prompt | 只收到 `CommandRejected`，Provider 未调用 |
| 同上 | Turn 运行中再次提交 | 第二条命令被拒绝，第一条仍可完成 |
| 同上 | Provider 返回错误后重试 | 收到 `TurnFailed`；失败 User 保留、无失败 Assistant，随后可再次提交 |
| 同上 | 缺少终态、异常结束、非 Stop 或空正文 | 发一次失败，不提交 Assistant 历史 |
| 同上 | 阶段 A 收到 ReasoningDelta | 关闭接收端并中止持续生成的流，不再转发后续正文 |
| 同上 | `Shutdown` 与背压 | 活跃流及阻塞的事件发送可被中断，事件流最终关闭 |
| `vortex-provider/src/lib.rs` | 请求映射 | 三种 role/content 正确，`stream` 为 true，千问开关按显式选项写入 |
| 同上及 `src/sse.rs` | HTTP/SSE 异常与结束 | 状态码、Content-Type、坏 JSON、缺 DONE、超时映射正确；统计包不结束 Turn |
| 同上 | 字节分帧与增量 | 中文跨 chunk、多行 data、同包多事件不丢内容，结束包可携带最后一段正文 |
| 同上 | 密钥脱敏 | `SecretString` 的 Debug 不包含原始密钥 |

并发测试应使用 `oneshot`、`Notify` 或受控 fake Provider 明确控制完成时机，不依赖
`sleep` 猜测调度。Core fake 测试绕过 HTTP/SSE，不能替代 Provider 测试；后者优先测试
纯转换和固定 SSE fixture，必须覆盖 HTTP 时使用本地 mock server。

## 后续演进

Thinking 需要增加 Core 的思考事件及 TUI 投影；思考历史还需单独扩展消息结构，具体规则
见流式响应设计。`CancelTurn`、恢复历史、工具事件和模型切换也应先扩展 Core
契约，再由 CLI 或 TUI 使用，不能把厂商响应或界面状态塞进 `CoreEvent`。

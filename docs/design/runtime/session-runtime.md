# Session Runtime 设计

架构归属：Session 调度。状态：当前实现契约。源码入口：[session.rs](../../../crates/vortex-core/src/session.rs)。
详见 [逻辑模块](../../architecture/logical-modules.md)；实现与验证结果以根 [README](../../../README.md) 为准。

本文档描述文本流式与工具版 v1 中 `vortex-core` 的 Session runtime，以及它与
`vortex-provider` 的边界。模型上下文未来如何裁剪和压缩，见
[`message-building.md`](message-building.md)；TUI 如何消费 Session，见
[`../tui/interface.md`](../tui/interface.md)。SSE 协议见
[Provider 流接口](../provider/streaming-response.md)，思考投影与后续刷新见
[Thinking 展示](../tui/thinking-display.md)；本文描述正文及展示-only 思考的运行时契约，
实现与验收进度以根 [`README.md`](../../../README.md) 为准。模型/工具交替执行的详细规则见
[`agent-loop.md`](agent-loop.md)。

## 当前范围

Session runtime 负责保存对话事实、串行化命令、调度模型/工具并发布语义事件。它不知道
HTTP、OpenAI wire format 或终端控件。每次只运行一个 Turn，Turn 内可以串行经历多个
模型请求和工具调用，支持步骤/正文/思考事件、空闲/失败时的会话内模型切换和整个 Session 关闭；
不包含思考历史回传、保留会话的 Turn 取消、持久化或恢复。源码契约与完整验收不同，
模型切换的完整专项矩阵仍需补齐，已补覆盖以根 README 为准。

```text
CLI：加载配置，装配 ModelManager 和 ToolRegistry，调用 start_session
                       │ SessionConnection
                       v
TUI ──AgentHandle（命令 / 关闭信号）──> Session actor
TUI <────────CoreEventStream───────── Session actor
                                          │ history 副本、Provider、Tools
                                          v
                                     run_turn task
                                     ├── ModelProvider::stream
                                     └── ToolRegistry::execute
                                          │ TurnProgress
                                          v
                                     Session actor
```

## 核心接口

| 类型 | 当前职责 |
| --- | --- |
| `ModelProvider` | 返回惰性 `ModelStream`，另以 reasoning_output_policy 声明 Reject/DisplayOnly 消费策略 |
| `ModelInfo` | 可安全显示的 profile ID、显示名、Provider/host、工具能力与不可选原因，不含凭据 |
| `ModelFactory` | 返回模型目录，按 ID 本地构造 Provider；具体实现由 CLI 提供，不执行网络请求 |
| `ModelManager` | 持有目录、当前模型和 Provider，构造成功后才原子替换绑定 |
| `ModelRequest` | `items: Vec<ConversationItem>` 和 `tools: Vec<ToolSpec>`，不含厂商协议类型 |
| `ModelEvent` | `TextDelta`、`ReasoningDelta`、完整 `ToolCall` 和携带 `FinishReason` 的 `Finished` |
| `ToolRegistry` | 持有可用工具，稳定返回定义，执行并归一化结果 |
| `SessionConnection` | 一次性交付初始 `SessionSnapshot`、命令句柄和独占事件流 |
| `SessionSnapshot` | 初始 messages/status，以及已选 model 和 models 目录；不是实时更新对象 |
| `AgentHandle` | 可克隆的命令入口；业务命令走有界 channel，Shutdown 走独立 watch 信号 |
| `CoreEventStream` | 单消费者事件入口；隐藏接收端实现 |
| `AgentCommand` | `SubmitPrompt`、`SelectModel { model_id }` 和 `Shutdown` |
| `CoreEvent` | 用户/Assistant、模型步骤、思考增量、工具和 Turn 事件；普通拒绝与模型选择确认/拒绝分别表达 |

`start_session(ModelManager, ToolRegistry)` 创建容量为 32 的命令 channel 和容量为
128 的事件 channel，并启动一个 Tokio task；每个 Turn 另建容量为 32 的 `TurnProgress`
channel。无工具调用方使用 `ToolRegistry::empty()`；思考是否可消费由本 Turn Provider
的 Reject/DisplayOnly 策略决定，与注册表是否为空无关。
初始 Snapshot 保存空消息、`Idle`、`models.current()` 与模型目录的副本；manager 本体随后
移入 actor，后续变化通过事件通知。当前尚未实现历史恢复，因此这里还不承担恢复场景下
“快照与订阅原子建立”的完整语义。

## 启动顺序与三个执行角色

CLI 的 TUI 分支先经 bootstrap 调用 `start_session(models, tools)`，再执行
`run(connection, tui_options).await`。两者不是“Session 执行完才启动 TUI”：

1. `start_session` 创建 channel，通过 `tokio::spawn` 启动后台 Session loop，随后返回
   `SessionConnection`，不等待这个 loop 结束。
2. TUI 的 `run` 初始化终端，再 `.await run_event_loop`。该循环在调用它的异步任务中
   执行；调用 async 函数并 `.await` 不会自动创建一个新 task 或独立线程。
3. Session 接受有效 Prompt 后，通过 `turns.spawn(run_turn(...))` 创建本轮任务。它消费
   Provider 流、执行工具并在必要时再次请求模型；Turn 结束后退出，不是随 Session
   一直运行的第三个长期循环。

进入 Session 前，CLI 已完成路径/日志初始化、阻塞池中的 TOML 加载、来源报告、工具装配
和初始模型构造；需要的 API key 在本地装配时读取。TUI 不读取配置或密钥。

| 执行角色 | 消费什么 | 生产什么 |
| --- | --- | --- |
| TUI loop | 键盘/鼠标事件、CoreEvent | 用户操作对应的 AgentCommand；本地状态更新与绘制 |
| Session loop | AgentCommand、关闭信号、TurnProgress | CoreEvent；维护历史并调度本轮任务 |
| 单轮 run_turn 任务 | ModelStream、工具执行结果 | TurnProgress；任务返回 TurnOutput 或 TurnError |

因此 TUI 和 Session 是双向通信：TUI 是命令生产者、会话事件消费者；Session 是命令
消费者、会话事件生产者。Channel 传递的是命令或事件数据，不是 Tokio task，也不会
自动执行命令。异步任务由 Tokio 调度，不意味着每个循环独占一个操作系统线程。

### 从 send 到实际处理

`mpsc::channel` 创建时返回相互关联的发送端与接收端。`start_session` 把
`command_sender` 保存到 `AgentHandle.sender`，把 `command_receiver` 移入后台任务。
因此 `send` 根据持有的发送端对象确定目标 channel，不按命令类型或名称查找接收者。
每次调用 `start_session` 都创建新的通道；克隆已有 `AgentHandle` 则仍连接原来的通道。

以普通 `SubmitPrompt` 为例，源码中的衔接顺序如下：

1. TUI 的 `execute_effects` 处理 `Effect::SendCommand`，调用 `agent.send(command).await`。
2. `AgentHandle::send` 使用 `self.sender.send(command).await` 发送命令。队列满时异步
   等待容量；返回成功只表示完成发送，不是业务接受、执行完成或模型回复的确认。
3. Session loop 的 `tokio::select!` 通过 `command_receiver.recv()` 接收命令，包装为
   `RuntimeInput::Command`，再交给下面的 `match input` 分派处理。
4. `SubmitPrompt` 分支校验输入和 Session 状态。拒绝时发布 `CommandRejected`；接受时
   保存 User、发布开始事件，通过 `models.provider()` 获取当前 Provider 的 Arc 克隆，
   再将它、历史副本和工具注册表交给本轮 `run_turn`。
5. `run_turn` 构建每个模型步骤的请求，通过 `stream.next().await` 读取增量、必要时执行
   工具，用 `turn_sender` 发送 TurnProgress。Session 在 `active.receiver.recv()` 分支
   接收，记录完成的工具步骤并发布对应 CoreEvent；最终 Assistant 由任务返回值提交。
6. TUI 在 `events.recv()` 分支接收 CoreEvent，经 `update` 更新界面状态并请求重绘。
   业务结果通过这条事件通道返回，不通过最初 `send` 的返回值返回。

`Shutdown` 不走普通命令队列：`AgentHandle::send` 将独立 `watch<bool>` 设为 true，
Session 通过关闭标志检查或 `shutdown_receiver.changed()` 响应。这样关闭通知不必等待
普通命令排队，且能打断阻塞的 UI 事件发送；具体清理语义见下一节。

阅读源码时，从 [`main.rs`](../../../crates/vortex-cli/src/main.rs) 的 TUI 分支开始，再看
[`event_loop.rs`](../../../crates/vortex-tui/src/event_loop.rs) 的 `execute_effects` 和
[`session.rs`](../../../crates/vortex-core/src/session.rs) 的 `AgentHandle::send`、
`start_session`、`match input`，即可沿上述路径定位。

## 模型切换

Session 串行处理 SelectModel，Running 或 ActiveTurn 尚未回收时发布专用拒绝事件，
不自动延期；成功或失败分别发布 ModelSelected/ModelSelectionRejected。
目录、工厂装配、原子替换与凭据时机以 [模型管理](model-management.md) 为准。
切换不改变历史或 SessionStatus，不写配置；每个 Turn 使用固定 Provider。

## 所有权与并发

### start_session 返回后的任务归属

`start_session` 分为创建 channel、启动后台任务、构造并返回 `SessionConnection` 三步。
函数返回不会结束后台任务：`tokio::spawn` 已将任务交给 Tokio runtime 持有并调度，
任务的生命周期不再依附于这次函数调用的栈帧。

- `async move` 将 `command_receiver`、`event_sender`、`shutdown_receiver`、`models`
  和 `tools` 等捕获值移入任务；内部创建的 `history`、`status`、`active` 和 `turns` 也由任务
  持有。等待 channel 时这些状态仍保留，不是继续借用已经返回的函数栈。
- `tokio::spawn` 返回 `JoinHandle`，可用于等待任务结果或请求中止。当前代码没有保存它；
  丢弃 JoinHandle 只会使任务脱离该句柄的等待与控制，不会取消任务。
- `SessionConnection` 只持有初始快照、命令/关闭信号发送端和事件接收端，不持有后台
  Session 的 JoinHandle，也不直接拥有其历史或模型任务。它是通信接口，不是任务本体。

正常运行时，Session 通过 Shutdown、所有 AgentHandle 被丢弃或事件接收端关闭来结束
循环。释放一个 AgentHandle 克隆不等于关闭所有发送端；释放事件接收端则会让 Session
检测到消费者消失。丢弃这些端点是通过通道状态促使任务退出，不是同步等待任务结束。

正常离开循环会调用 `turns.abort_all()` 请求中止活跃 Turn 任务（包括等待中的模型/工具
future）；它本身不等待取消完成，也不保证未来外部进程工具自动终止。
`JoinSet` 与 `JoinHandle` 的 Drop 行为不同：丢弃 JoinSet 会中止其中尚未完成的任务。
Session task 若 panic 并展开栈，JoinSet 的析构也会请求中止子任务；Tokio runtime 关闭
同样可能取消未结束的异步任务，不能把这些异常退出等同于完整走完业务收尾或发布终态。

当前没有对外暴露“等待 Session task 完全退出”的 join 接口。若以后需要确认资源清理
完成或观测 Session task 的 panic，应另行设计完成通知或任务句柄，不能把 `send(Shutdown)`
成功误认为任务已回收。

### Session 内部状态与并发边界

Session actor 独占 `ModelManager`、`history`、`status`、`ActiveTurn` 和
`JoinSet<Result<TurnOutput, TurnError>>`，其他模块不能直接修改这些状态。
事件循环使用 `tokio::select!` 等待命令、TurnProgress、关闭信号和
UI 消费者关闭；先得到 `RuntimeInput`，再更新状态，避免跨分支借用活跃 Turn。

- `history: Vec<ConversationItem>` 是权威请求历史，保存已接受的 User、已完成工具步骤
  的 Assistant 前缀/调用/结果，以及最终成功 Assistant。
- `status` 为 `Running` 时拒绝新的 Prompt，因此逻辑上最多只有一个活跃 Turn。
- `ActiveTurn` 只保存 TurnProgress 接收端；不再累积正文或保存模型 outcome。
- 初始模型和目录先克隆到 snapshot；manager 的所有后续选择只能由 actor 串行处理。
- `run_turn` 独占进度发送端，累积每个模型步骤的正文并维护本地 `context`；不直接修改
  Session 权威历史。工具步骤通过 ToolFinished 回填，最终回答通过 TurnOutput 返回。
- 每次模型请求前发送 ModelStepStarted；按 Provider 输出策略转发思考增量。展示-only
  思考不在 Session 累积，不进入 history 或 snapshot，事件发送仍监听 Shutdown。
- actor 等进度通道排空并关闭，再 `join_next()` 回收任务；只有正常返回 `Ok(TurnOutput)`
  才提交最终 Assistant 并发布 TurnCompleted，任务错误或 panic 统一失败。
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
  -> ModelStepStarted(1) -> run_turn：ModelProvider::stream
  -> [AssistantReasoningDelta（仅 DisplayOnly，可选）、AssistantTextDelta（可选前缀）
      -> ToolCallStarted -> ToolCallFinished
      -> ModelStepStarted(n+1) -> 再次 ModelProvider::stream]（零到多次）
  -> AssistantReasoningDelta（仅 DisplayOnly，可选）
  -> AssistantTextDelta（最终回答，一次或多次）
  -> run_turn 返回，内部进度通道排空并关闭，task 正常结束
  -> TurnCompleted
```

收到命令后先用 `trim()` 判断是否为空，但保留原始内容。空 Prompt 发出
`CommandRejected`；`Running` 期间再次提交也发出 `CommandRejected`。两种拒绝都不修改
消息历史和 Session 状态。

Finished 是每个模型步骤的 Provider 事件，不直接发给 TUI。ToolCalls 结束当前模型步骤，
不是结束整个 Turn；仅在最终步骤以 Stop 结束、正文非空且任务正常返回时成功。
最终 Assistant 只写入历史一次，不重发完整正文 Delta。

Provider 错误、缺少终态、task 异常、Length、ContentFilter、空最终正文、工具轮数超限
或非法调用 ID 均导致 Failed 和一次 TurnFailed。Reject 策略收到 ReasoningDelta 或
DisplayOnly 思考超过每步字节上限时，run_turn 失败并释放流；Session 排空已发送进度后
统一收尾，不等远端生成完毕。只有思考而无最终正文也不能以 Stop 成功。
未知工具、工具参数/执行错误则成为错误 ToolResult，交给下一步模型处理，不直接失败。

本轮已接受的 User 及已完成的工具步骤保留，但失败模型步骤的部分 Assistant 不进入
`history`；TUI 可保留已显示片段并标记错误。Failed 不阻止下一次提交：有效 Prompt
重新进入 Running，
成功后回到 `Idle`。下一轮可能包含连续 User 消息，当前不回滚也不自动重试。

## Provider 适配边界

Session 只消费 ModelProvider 的语义流，不使用 Reqwest/SSE/厂商 DTO。
具体请求/响应映射、超时与 Decoder 规则见 [SSE 设计](../provider/streaming-response.md)，
工厂职责见 [模型管理](model-management.md)，Thinking 请求选择见 [思考选项](../provider/thinking-options.md)。
步骤与思考增量按本文件事件契约转发；展示-only 不进入权威历史，UI 投影见
[Thinking 展示](../tui/thinking-display.md)。

## 进度排空与发布背压

每个 Turn 的 `run_turn` 任务由 `JoinSet<Result<TurnOutput, TurnError>>` 管理，使用独占的
有界 TurnProgress 通道（容量 32）。任务处理多个模型步骤，进度发送端只由该任务持有。
actor 的 ActiveTurn 只保存接收端；正文、步骤编号、思考字节计数和每步结束原因由 run_turn 管理。它维护历史
副本，不直接修改权威历史，也不把完整回答再次发送给 TUI。

| 输入 | 处理位置与行为 |
| --- | --- |
| 有效 SubmitPrompt | 记录 User，发 UserMessageAdded、AssistantMessageStarted，启动本轮任务 |
| TurnProgress::TextDelta | 发布 AssistantTextDelta；仍为 Running |
| ModelStepStarted / 合法 ReasoningDelta | 转发步骤/思考事件；思考仅展示，不提交历史；仍为 Running |
| ToolStarted / ToolFinished | 发布工具事件；完成步骤写入前缀/调用/结果，仍为 Running |
| 模型 Finished(ToolCalls) | run_turn 执行工具并再次调用模型，不结束 Turn |
| Reject 策略收到 ReasoningDelta、思考超限或 Provider Err | run_turn 返回错误并释放流；此前进度排空后统一失败 |
| 通道关闭 | 回收本轮 task，依据 TurnOutput/TurnError 或 JoinError 提交成功/失败 |
| Running 时再次提交 | CommandRejected；不改当前缓冲、状态或历史 |
| Shutdown / UI 消失 | 中止任务、丢弃活跃流并关闭 Session；不自动重试 |

**不能用 task 提前完成作为 TurnCompleted 的依据。** 通道关闭时此前 Delta 已按 FIFO
读完，再检查 task 返回值，避免最后一段丢失、终态越过 Delta 或污染下一轮。每轮使用
新通道，上一轮完全回收前不接受下一轮；以后允许重叠/取消重启时再引入 TurnId。

Core → UI 仍使用有界通道，完整保留文本事件，不用 `try_send` 丢 Delta，也不改成无限队列。
仅把网络放进 task 不能保证退出：actor 若阻塞在 UI channel 的 `send().await`，仍可能收不到
Shutdown。因此使用内部 `watch<bool>` 关闭信号：`AgentHandle::send(Shutdown)` 通知它，
actor 的主 select 和每一次事件发送均监听关闭；外部 AgentCommand 形状不变。所有句柄
关闭、消费者消失、错误提前返回也必须释放 JoinSet，从而中止网络任务。通知发送成功不代表
取消已经完成，测试需要另外观察活跃流释放及事件通道关闭。

## 单元测试矩阵

通用组织方式和执行命令见 [`../../engineering/testing.md`](../../engineering/testing.md)。测试不得
调用真实付费 API。下表是应覆盖的契约，不代表全部已有测试；覆盖现状见根 README。

| 模块与位置 | 场景 | 关键断言 |
| --- | --- | --- |
| `vortex-core/src/session.rs` | 成功 Turn | Started 后多次 Delta，最后一次终态；首段先于模型结束到达 |
| 同上 | 连续两个成功 Turn | 第二次请求包含此前 User/Assistant 和本轮 User，顺序不变 |
| 同上 | 空白 Prompt | 只收到 `CommandRejected`，Provider 未调用 |
| 同上 | Turn 运行中再次提交 | 第二条命令被拒绝，第一条仍可完成 |
| 同上 | Provider 返回错误后重试 | 收到 `TurnFailed`；失败 User 保留、无失败 Assistant，随后可再次提交 |
| 同上 | 缺少终态、异常结束、Length/ContentFilter 或空最终正文 | 发一次失败，不提交失败步骤的 Assistant |
| 同上 | Reject 收到 ReasoningDelta | run_turn 立即返回错误，释放流，不再读取后续正文 |
| 同上 | DisplayOnly 思考流 | 步骤先于增量，思考先于流完成到达；下一轮历史不含思考 |
| 同上 | Thinking 中失败/退出/满队列 | 保留已完成工具历史，失败 Assistant 不入历史；Shutdown 释放活跃流 |
| 同上 | `Shutdown` 与背压 | 活跃流及阻塞的事件发送可被中断，事件流最终关闭 |
| 同上 | 工具步骤后模型失败 | 保留完成的前缀/调用/结果，失败部分正文不入历史 |
| 同上 | 空闲/Failed 切换后再提交 | 下一轮使用新 Provider，历史只带一次，不新增 slash 消息 |
| 同上 | Running 拒绝、未知/不可用/缺密钥、相同 ID | 专用结果事件，失败保留原绑定；同 ID 也确认 |
| 同上 | 模型确认发布遇到背压 | Shutdown 仍能结束；选择不改 status 或配置文件 |
| `vortex-core/src/model.rs` | 目录与实例绑定 | 重复 ID 拒绝、同 ID 幂等、失败不部分更新 |
| `vortex-core/src/agent_loop.rs`、`tool.rs` | 工具闭环与异常 | 结果回填、错误恢复、轮数和 ID 校验；详见 agent-loop.md |


模型目录/工厂的专项以 [模型管理](model-management.md) 为准；HTTP/SSE 映射矩阵见
[Provider 验收](../provider/streaming-response.md#provider-专项矩阵)。

并发测试应使用 `oneshot`、`Notify` 或受控 fake Provider 明确控制完成时机，不依赖
`sleep` 猜测调度。Core fake 测试绕过 HTTP/SSE，不能替代 Provider 测试；后者优先测试
纯转换和固定 SSE fixture，必须覆盖 HTTP 时使用本地 mock server。

## 后续演进

Thinking 展示已通过 Core 事件投影到 TUI，思考历史仍需单独扩展消息结构，具体规则
见 [消息构建目标](message-building.md#思考历史扩展目标)。CancelTurn、恢复历史、工具详情/审批事件等仍应先扩展 Core
契约，再由 CLI 或 TUI 使用，不能把厂商响应或界面状态塞进 `CoreEvent`。

# SSE 流式回答与 Thinking 展示设计

- 日期：2026-09-19；代码契约更新：2026-10-01；临时稿回收：2026-10-06
- 定位：文本/工具 SSE 契约及后续 Thinking/绘制节流设计；阶段 A 与阶段 B/C 不可混为一谈。
- 范围：TUI → Session → 当前 Chat Completions 兼容 Provider；本轮不迁移 exec。

文本流运行时见 [Session runtime](session-runtime.md)，显示边界见
[TUI](tui-interface.md) 与 [滚动设计](transcript-scrolling.md)。工具循环见
[Agent loop](agent-loop.md)。本地实施稿不作为长期契约依赖，本文保留流式接口规则。
实现与验收进度以根 [README](../../README.md) 为准，不以阶段表或测试矩阵代替完成记录。

## 1. 目标与阶段

模型生成期间即可看到增量内容，不等待完整回复后再逐字播放。一个增量可能包含多个字，
不承诺字符级节奏。Thinking 指 API 主动公开的思考文本或摘要，不代表获取完整内部推理。

| 阶段 | 请求与展示 | 历史策略 |
| --- | --- | --- |
| A：文本/工具 SSE | 按接口方言与模型能力显式关闭思考，正文增量与单个工具调用分别处理 | 完成的工具步骤与最终成功回答进入历史 |
| B：Thinking 展示 | 显式开启思考，思考纯文本与 Markdown 回答分区 | 显式关闭思考回传，思考仅保留于本次 UI |
| C：思考历史（可选后续） | 启用前先扩展语义消息及请求映射 | 成功回答的思考独立保存，按支持的协议完整回传 |

阶段 A 的协议允许完整 ToolCall 结束一个模型步骤，再由 Core 决定执行和下一步请求；
一个模型流完成不等于整个 Turn 完成。阶段 B 的目标是范围明确的思考展示，不假装支持
默认开启的思考历史。阶段 C 不是显示 Thinking 的前置条件。CancelTurn、重连续传、
usage UI、菜单折叠和持久化不属于此版接口；阶段与测试覆盖现状见根 README。
Shutdown 仍表示退出整个 Session，而非“取消本轮后继续聊天”。

## 2. 分层与接口

```text
HTTP bytes → SSE event → Provider ModelEvent
                           ↓ run_turn：累积正文、执行工具、再次请求
                           ↓ 有界 TurnProgress 通道
                     Session actor：权威历史、事件、终态
                           ↓ CoreEvent
                     TUI：投影、Markdown、滚动、绘制
```

Core 不依赖 Reqwest/SSE/厂商字段。Provider 不直接发送 `CoreEvent`，也不决定会话历史。
TUI 不持有 HTTP response，不解析 JSON，不向网络任务提供可变 widget 引用。

Provider 边界采用以下惰性流接口，替代旧 `complete()` / `ProviderFuture` 契约：

```rust
pub type ModelStream =
    Pin<Box<dyn Stream<Item = Result<ModelEvent, ProviderError>> + Send + 'static>>;

pub trait ModelProvider: Send + Sync {
    fn stream(&self, request: ModelRequest) -> ModelStream;
}

pub enum ModelEvent {
    TextDelta { delta: String },
    ReasoningDelta { delta: String },
    ToolCall { call: ToolCall },
    Finished { reason: FinishReason },
}

pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
}
```

`stream()` 本身不等待网络；返回流拥有 HTTP client、请求和必要配置，第一次 poll 才发起
请求。`'static` 表示不借用短生命周期的 `&self`，不是让数据泄漏或永久存活。错误使用
`Result::Err`，与正常结束原因区分。未知结束原因、不支持的多调用或 legacy function_call
返回协议错误，不猜测成功。

## 3. Provider 的 SSE 解析

使用 Reqwest 字节流和 `eventsource-stream` 做 SSE 分帧，`serde_json` 解析单个 data。
使用 `async-stream` 构造惰性 Stream；不自行按网络 chunk 拆 UTF-8/JSON，也不引入自动
重连客户端。依赖归属见 [技术选型](../architecture/technology-stack.md)。

当前适配器只请求一个 choice：

- HTTP 非成功状态先返回错误；成功状态核对 `Content-Type: text/event-stream`（允许参数）。
- SSE 注释/心跳与空 data 不产生文本；`choices: []` 的统计包不结束模型步骤。
- choice 0 的 `delta.content` 和 `delta.reasoning_content` 分别产生事件；同一个包有两者
  时分别处理，不能使用互斥 else if。工具参数增量独立累积；首版拒绝非零 choice、
  多候选、非零 tool index 和 legacy function_call。
- 先处理包内 Delta，再记录 `finish_reason`。没有内容的终止包不报“空回复”。
- 严格完成策略：收到合法 `finish_reason` 后继续消费统计包，直到 `[DONE]` 才输出一次
  Finished；ToolCalls 结束时先输出完整 ToolCall，再输出 Finished，随后流结束。
  `[DONE]` 没有结束原因、EOF 缺少 `[DONE]`、结束原因重复或
  终止后继续输出正文均视为协议异常。其他供应商需要不同结束规则时在其 adapter 内明确支持。
- 错误 JSON、API 错误对象、HTTP/body 读取错误均输出一次 Err 后结束，错误文案不含原始
  响应体、认证头或完整请求。禁止把失败事件序列转换为空字符串成功结果。

工具增量由 `openai_chat/decoder.rs` 的 PendingToolCall 保存 id、name 和 arguments。
name/arguments 片段按到达顺序拼接，arguments 在 DONE 时解析为 JSON；缺失 ID/名称、
非法 JSON 或工具信息与结束原因不匹配都失败。后续 `id: ""` 不表示 ID 改变，应忽略；
首次非空 ID 保存，相同非空 ID 接受，不同非空 ID 仍报错。跨模型步骤的重复 ID 则由
Core 校验，不能在 Decoder 内用上一条模型流的状态判断。

模块边界：`sse_transport.rs` 处理 HTTP/超时/字节分帧，`openai_chat/stream.rs` 连接传输
与解码，`request.rs` 映射请求，`decoder.rs` 负责协议状态；不是把全部逻辑放在 lib.rs。

选择严格完成是本项目的首版策略，不是所有 SSE 服务的通用要求。请求发起至响应头使用
30 秒超时，每次等待下一个 body chunk 使用 120 秒空闲超时；不是整个回答限时 120 秒，
也不是等待下一个正文 Delta 的超时。不能把“Thinking 没产生正文”当作网络空闲。总时长/大小
上限、原始未闭合 SSE 事件缓冲上限应独立评估，不能认为有界 channel 已限制全部内存。

### 跨协议复用与第一轮等价拆分

SSE 分帧可复用，不代表所有厂商的模型协议可共用一个 Decoder：

| 层 | 可复用边界 | 不应负责 |
| --- | --- | --- |
| sse_transport | 响应状态/MIME、超时、字节流到完整 SSE Event、传输错误 | 工具参数、Thinking、DONE 或模型完成语义 |
| 请求适配器 | 语义 ModelRequest → 厂商 DTO、认证与模型选项装配 | 会话历史提交和工具执行 |
| 协议 Decoder | 事件类型、增量状态、工具组装、协议终态/EOF 校验 | 通用 HTTP 和 UI 状态 |
| stream 驱动 | 惰性请求、传输到 Decoder 的连接、错误后结束 | 重试、自动恢复会话 |

按实际端点/协议选择适配器，不按模型品牌字符串猜测。经 OpenAI-compatible 端点访问
某模型时仍走该端点协议；原生接口需要独立适配。非 SSE 的流不能直接套用此传输实现。
当前 Chat Decoder 使用 Event.data，但通用传输保留完整 Event；后续命名事件协议可能
需要 event 类型，不应提前只保留 data。具体终态规则必须按对应协议核对，不能复制 DONE。

第一轮拆分保持等价：request 拥有请求 DTO，stream 的上下文拥有 Client、认证及选项，
第一次 poll 才发请求；正常传输 EOF 仍交给 Decoder 判定，不自动算成功。
SecretString 如需 Clone 仍保留手写脱敏 Debug，不为打印日志开放字段或派生原文 Debug。
只记录有语义的元数据，不把 Debug 当成字段访问权限或新增 TRACE trait。

出现第二个真实协议、确认共有驱动重复后，再考虑内部 SseProtocolDecoder 抽象；不先
新增公开 SseProvider，也不在 Core 暴露厂商 event/signature。第一轮拆分不改变超时、
错误分类、惰性、背压或取消语义。原稿的“拒绝所有工具调用”已被工具版 v1 取代。

## 4. Session 并发、顺序与背压

每个 Turn 的 `run_turn` 任务由 `JoinSet<Result<TurnOutput, TurnError>>` 管理，使用独占的
有界 TurnProgress 通道（容量 32）。任务处理多个模型步骤，进度发送端只由该任务持有。
actor 的 ActiveTurn 只保存接收端；正文和每一步结束原因由 run_turn 管理。它维护历史
副本，不直接修改权威历史，也不把完整回答再次发送给 TUI。

| 输入 | 处理位置与行为 |
| --- | --- |
| 有效 SubmitPrompt | 记录 User，发 UserMessageAdded、AssistantMessageStarted，启动本轮任务 |
| TurnProgress::TextDelta | 发布 AssistantTextDelta；仍为 Running |
| ToolStarted / ToolFinished | 发布工具事件；完成步骤写入前缀/调用/结果，仍为 Running |
| 模型 Finished(ToolCalls) | run_turn 执行工具并再次调用模型，不结束 Turn |
| ReasoningDelta / Provider Err | run_turn 返回错误并释放流；此前进度排空后统一失败 |
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

## 5. 完成、失败和历史

阶段 A 的成功序列为：

```text
UserMessageAdded → AssistantMessageStarted
→ [AssistantTextDelta（可选）、ToolCallStarted、ToolCallFinished]（零到多组）
→ 一个或多个 AssistantTextDelta
→ TurnCompleted
```

阶段 B 才增加零到多个 `AssistantReasoningDelta`；它目前不在 `CoreEvent` 中。
接入后不依赖所有服务严格“先思考、后正文”，两类事件分别累积即可。

首版只有最终模型步骤以 Stop 结束且回答非空才算 Turn 成功；ToolCalls 表示继续执行。
Length、ContentFilter、不支持的响应、空最终回答、非法 ID、工具轮数超限、
中断、超时或 task panic 均发一个 `TurnFailed`，保留 UI 已显示片段并标注中断，但不将失败
步骤的部分 Assistant 写入请求历史。对应 User、此前完成工具步骤及其 Assistant 前缀
保留；工具执行错误也作为 ToolResult 保留并继续交给模型。暂不回滚或自动重试。
未来支持带终态的不完整历史时另改该契约。

成功时只把缓冲写入历史一次，不再次发整段 TextDelta。Thinking 不是新 MessageRole，
不能将其并入回答。阶段 C 增加 `SessionMessage.reasoning: Option<String>`；User/System
为 None，成功 Assistant 独立保留。Snapshot 必须同步投影，不因恢复丢失该字段。

## 6. Thinking 参数与能力边界

请求侧的跨供应商装配已按 [思考选项设计](provider-thinking-options.md) 接入并补离线测试：
统一策略不等于统一厂商 JSON，不发送参数也不等于关闭思考。此次不修改当前
ReasoningDelta 的拒绝行为。后续阶段 B/C 的拆分必须按协议能力判断；对于带 tools 时
要求回传思考历史的服务，不能只启用展示而遗漏请求历史支持。

CLI 将配置的接口方言、模型策略与能力装配成 `OpenAiChatOptions`，不按模型名推断。
Provider 通过统一 `with_options` 校验并保存转换结果：DashScope 按能力发送关闭字段，
DeepSeek 官方发送 thinking.type=disabled。Generic 只支持 ProviderDefault；默认省略
控制字段，Disabled 不可表达时本地失败。旧千问专用 builder 已移除。

下表仅是千问后续阶段的参数参考，不是当前配置支持的策略列表；B/C 尚未实现：

| 阶段 | stream | enable_thinking | preserve_thinking |
| --- | --- | --- | --- |
| A | true | false | false |
| B | true | true | false |
| C | true | true | true |

千问公开思考通过 `reasoning_content` 返回，正文为 `content`，同一次流请求即可获取；
直接 HTTP 请求的扩展参数放在 body 顶层，不套 SDK 的 `extra_body`。[思考说明][thinking]

核对时 `qwen3.8-max` 默认启用思考历史；启用后要分别回传历史 reasoning_content，不能
拼入 content；其 Token 计入输入。默认行为不应替代显式策略。[接口说明][chat-api]

阶段 C 仅支持本会话、同一模型的可展示/可回传文本。跨 Provider 的摘要、签名和 opaque
reasoning item 不是同一种数据；未来 `/model` 必须做能力检查和明确转换/拒绝策略，不能
把千问字段照搬到其他服务。API 没返回思考时只显示等待状态，不能补造思考文本。

实施顺序应是请求选项 → run_turn 分别累积思考/正文 → TurnProgress 与 CoreEvent
增量投影 → TUI 状态/布局 → 回归测试。不要恢复旧 ActiveTurn.content/reasoning 缓冲，
当前 actor 只负责进度与权威历史。阶段 B 验证通过后，才单独决定是否实施阶段 C 的
历史字段、Snapshot 和请求回传；思考签名与 opaque 数据须留在正确的协议边界。

## 7. TUI 投影与刷新

阶段 A 复用已有 `AssistantTextDelta`：追加原文并请求重绘，不需要 TUI 解析 SSE。
以下 Thinking 投影属于阶段 B，尚未实现：新增 `CoreEvent::AssistantReasoningDelta`，
同一个 Assistant entry 保存 `reasoning` 与
`content` 两段，前者弱化颜色、按纯文本渲染，后者仍由 `markdown.rs` 处理。标签和间距
由 Transcript 添加，合并后的同一个 Paragraph 同时负责测高与绘制。

- Started 时显示“等待模型”；收到思考时“思考中”；收到正文时“回答中”。阶段仅为 UI
  状态，Core 的 Running 不必拆分。已进入回答阶段后不因晚来的思考增量退回等待。
- 思考可见但正文为空时，失败清理不能删掉整条 Assistant；只有两者都空才删除占位。
- 阶段 B 保留已显示思考直至当前 TUI 结束，但不写入模型历史；阶段 C Snapshot 同步支持。
- 第一版不自动折叠，避免完成时高度突变；人工折叠和语义阅读锚点留到后续。

绘制节流是独立的待实施步骤。当前每个 Delta 请求重绘，仅合并同批 Effect，不跨事件合并。
目标是接收与绘制解耦：收到事件立即更新数据并置 dirty；每约 33 ms 最多绘制一次，空闲不绘制。
Tokio interval 使用 `MissedTickBehavior::Skip`，结束/失败可强制刷新最后一帧。退出优先清理，
不要求再画终态。禁止逐字 sleep，也不合并丢弃生命周期事件。

每次按当前整条 Assistant 原文解析 Markdown，不逐 delta 独立解析。沿用 follow_tail，
历史浏览时不强制到底；未闭合代码围栏和 resize 的回流不承诺完全稳定阅读锚点。先节流，
再按测量结果缓存完成消息；不改变已选的 Markdown 渲染器，不顺手修复滚动条视觉问题。

## 8. 日志、安全与测试

日志契约要求只记录必要元数据，不记录请求/响应正文；Thinking 的显示权限不等于日志
落盘权限，API key 在任何级别都禁止记录。Provider 流路径没有整段 ChatRequest trace，
TUI 提交分支的输入原文 TRACE 也已在 slash 接入时移除；这不代表全链路日志/错误路径
已经完成脱敏验收，不应为排障默认开启正文诊断。
模型文本不执行、不作为命令，控制字符过滤仍是独立工作，不能声称 SSE/Markdown 已提供防护。

下表是验收目标，Thinking 与节流相关用例随对应阶段补齐，不表示已有这些能力或完整覆盖。

| 层 | 必须验证 |
| --- | --- |
| Provider | 任意字节拆包、中文跨包、多行 data、统计包、双字段 delta、坏 JSON、缺 DONE、非成功 HTTP；工具参数拼接与空 ID 兼容 |
| Core | 首段提前到达、最后 Delta 不丢、只发一个 Turn 终态；工具结果回填、轮数/ID 校验；失败部分正文不进历史，完成工具步骤保留 |
| 生命周期 | 正文前/Thinking 中/输出中 Shutdown；UI 通道满时退出；task panic；事件消费者关闭；读流超时 |
| TUI | 思考与回答分离、只有思考时失败仍保留、Markdown 围栏跨 delta、手动滚动不自动跳底、定时刷新和终态末帧 |
| 阶段 C | 第二轮正确回传独立 reasoning；User/System 无该字段；Snapshot 保留；日志无正文/密钥 |

使用受控 fake、oneshot/Notify 和本地 HTTP fixture，不依赖付费模型或 sleep 猜调度。
TestBackend 验证布局，真实终端/PTY 验证退出恢复。已有 Core fake 测试不能证明 HTTP/SSE
解析正确，具体覆盖与验证结果见根 README，不因文档存在而算通过。

受控流、取消同步和本地 HTTP 分帧的具体方法见
[回归测试方案](../engineering/regression-test-plan.md)。本文同时回收了本地
`20260919-01.md`、`20260922-01.md` 与 `2026-09-22-SSE抽象问题.md` 的长期设计约束，
不保留过时接口的逐行迁移代码。

## 9. 参考与核对日期

2026-09-19 核对以下上游资料。行为随模型/服务变化，新增适配器必须重新验证：

- [千问流式输出][stream-doc]：兼容端点的 SSE、结束和统计包。
- [千问深度思考][thinking]：公开思考字段与开关。
- [Chat Completions 参数][chat-api]：思考历史与原始 HTTP 参数位置。
- [eventsource-stream 0.2.3](https://docs.rs/eventsource-stream/0.2.3/eventsource_stream/)：字节流到 SSE 事件。
- [async-stream 0.3.6](https://docs.rs/async-stream/0.3.6/async_stream/)：以 async 代码构造 Stream。

[thinking]: https://help.aliyun.com/zh/model-studio/deep-thinking
[chat-api]: https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions
[stream-doc]: https://help.aliyun.com/zh/model-studio/stream

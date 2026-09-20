# SSE 流式回答与 Thinking 展示设计

- 日期：2026-09-19
- 定位：文本 SSE 契约及后续 Thinking/绘制节流设计；阶段 A 与阶段 B/C 不可混为一谈。
- 范围：TUI → Session → 当前 Chat Completions 兼容 Provider；本轮不迁移 exec。

文本流运行时见 [Session runtime](session-runtime.md)，显示边界见
[TUI](tui-interface.md) 与 [滚动设计](transcript-scrolling.md)。实现步骤保存在本地
`md.local.temp/20260919-01.md`；它被 Git 忽略，本文独立保留全部长期契约。
实现与验收进度以根 [README](../../README.md) 为准，不以阶段表或测试矩阵代替完成记录。

## 1. 目标与阶段

模型生成期间即可看到增量内容，不等待完整回复后再逐字播放。一个增量可能包含多个字，
不承诺字符级节奏。Thinking 指 API 主动公开的思考文本或摘要，不代表获取完整内部推理。

| 阶段 | 请求与展示 | 历史策略 |
| --- | --- | --- |
| A：文本 SSE | 当前千问显式关闭思考，打通真实文本流、错误和退出 | 仅完整成功回答进入历史 |
| B：Thinking 展示 | 显式开启思考，思考纯文本与 Markdown 回答分区 | 显式关闭思考回传，思考仅保留于本次 UI |
| C：思考历史（可选后续） | 启用前先扩展语义消息及请求映射 | 成功回答的思考独立保存，按支持的协议完整回传 |

阶段 A 的接口已接入代码，但仍需补充 Provider 测试及真实服务验收。阶段 B/C 尚未实现；
阶段 B 的目标是范围明确的思考展示，不假装支持默认开启的思考历史。阶段 C 不是显示 Thinking
的前置条件。暂不实现 Tool call、CancelTurn、重连续传、usage UI、菜单折叠或持久化。
Shutdown 仍表示退出整个 Session，而非“取消本轮后继续聊天”。

## 2. 分层与接口

```text
HTTP bytes → SSE event → Provider ModelEvent
                           ↓ 有界的单轮内部通道
                     Session actor：累积、历史、终态
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
    Finished { reason: FinishReason },
}

pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
}
```

`stream()` 本身不等待网络；返回流拥有 HTTP client、请求和必要配置，第一次 poll 才发起
请求。`'static` 表示不借用短生命周期的 `&self`，不是让数据泄漏或永久存活。错误使用
`Result::Err`，与正常结束原因区分。未知结束原因/不支持的工具调用返回协议错误，不猜测成功。

## 3. Provider 的 SSE 解析

使用 Reqwest 字节流和 `eventsource-stream` 做 SSE 分帧，`serde_json` 解析单个 data。
使用 `async-stream` 构造惰性 Stream；不自行按网络 chunk 拆 UTF-8/JSON，也不引入自动
重连客户端。依赖归属见 [技术选型](../architecture/technology-stack.md)。

当前适配器只请求一个 choice：

- HTTP 非成功状态先返回错误；成功状态核对 `Content-Type: text/event-stream`（允许参数）。
- SSE 注释/心跳与空 data 不产生文本；`choices: []` 的统计包不结束 Turn。
- choice 0 的 `delta.content` 和 `delta.reasoning_content` 分别产生事件；同一个包有两者
  时分别处理，不能使用互斥 `else if`。首版拒绝非零 choice、多候选和工具增量。
- 先处理包内 Delta，再记录 `finish_reason`。没有内容的终止包不报“空回复”。
- 严格完成策略：收到合法 `finish_reason` 后继续消费统计包，直到 `[DONE]` 才输出一次
  `Finished`，随后流结束。`[DONE]` 没有结束原因、EOF 缺少 `[DONE]`、结束原因重复或
  终止后继续输出正文均视为协议异常。其他供应商需要不同结束规则时在其 adapter 内明确支持。
- 错误 JSON、API 错误对象、HTTP/body 读取错误均输出一次 Err 后结束，错误文案不含原始
  响应体、认证头或完整请求。禁止把失败事件序列转换为空字符串成功结果。

选择严格完成是本项目的首版策略，不是所有 SSE 服务的通用要求。请求发起至响应头使用
30 秒超时，每次等待下一个 body chunk 使用 120 秒空闲超时；不是整个回答限时 120 秒，
也不是等待下一个正文 Delta 的超时。不能把“Thinking 没产生正文”当作网络空闲。总时长/大小
上限、原始未闭合 SSE 事件缓冲上限应独立评估，不能认为有界 channel 已限制全部内存。

## 4. Session 并发、顺序与背压

后台模型任务由 `JoinSet<()>` 管理。每轮创建独占的
有界 ModelEvent 通道（容量 32）；任务逐条转发 Stream，发送 Finished/Err 后立即
返回。发送端只由该任务持有，不留在 actor 或其他 clone 中。

actor 保存 `ActiveTurn`：receiver、回答缓冲和待确认 outcome；思考缓冲在阶段 A 仅预留。
网络任务不修改历史，不把完整回答再次发送给 TUI。阶段 A 的事件状态转换如下：

| 输入 | actor 行为 |
| --- | --- |
| 有效 SubmitPrompt | 记录 User，发 UserMessageAdded、AssistantMessageStarted，启动本轮任务 |
| TextDelta | 追加正文缓冲，发布 AssistantTextDelta；仍为 Running |
| ReasoningDelta | 记录不支持的响应错误，关闭接收端并中止模型任务；排空后统一失败 |
| Finished / Err | 保存 outcome，仍为 Running，等待内部通道关闭 |
| 通道关闭 | 回收本轮 task，校验无 panic 且 outcome 存在，再统一提交成功或失败 |
| Running 时再次提交 | CommandRejected；不改当前缓冲、状态或历史 |
| Shutdown / UI 消失 | 中止任务、丢弃活跃流并关闭 Session；不自动重试 |

**不能用 task 提前完成作为 TurnCompleted 的依据。** 通道关闭时此前 Delta 已按 FIFO
读完，再检查 task 和 outcome，避免最后一段丢失、终态越过 Delta 或污染下一轮。每轮使用
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
→ 一个或多个 AssistantTextDelta
→ TurnCompleted
```

阶段 B 才增加零到多个 `AssistantReasoningDelta`；它目前不在 `CoreEvent` 中。
接入后不依赖所有服务严格“先思考、后正文”，两类事件分别累积即可。

首版只有 `Stop` 且回答不为空才算成功。`Length`、`ContentFilter`、不支持的响应、空回答、
中断、超时或 task panic 均发一个 `TurnFailed`，保留 UI 已显示片段并标注中断，但不将失败
Assistant 写入请求历史。对应 User 沿用当前规则保留；下一次请求可能包含连续 User，暂不
实现回滚或自动重试。未来支持带终态的不完整历史时另改该契约。

成功时只把缓冲写入历史一次，不再次发整段 TextDelta。Thinking 不是新 MessageRole，
不能将其并入回答。阶段 C 增加 `SessionMessage.reasoning: Option<String>`；User/System
为 None，成功 Assistant 独立保留。Snapshot 必须同步投影，不因恢复丢失该字段。

## 6. Thinking 参数与能力边界

对当前千问使用显式配置，不根据模型名字符串暗中推断。CLI 在装配时给 Provider 传入
千问专用选项；泛用 OpenAI-compatible 请求默认省略这些厂商参数。推荐阶段参数：

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

默认日志只记录模型、事件类别、长度、耗时、结束原因和错误类别，不记录请求/响应正文。
流式请求路径只记录元数据，不启用旧实现的整个 `ChatRequest` trace 输出；后续阶段同样
遵守此边界。Thinking 的显示权限不等于日志落盘权限。显式正文诊断以后单独设计，
API key 在任何级别都禁止记录。
模型文本不执行、不作为命令，控制字符过滤仍是独立工作，不能声称 SSE/Markdown 已提供防护。

下表是验收目标，Thinking 与节流相关用例随对应阶段补齐，不表示已有这些能力或完整覆盖。

| 层 | 必须验证 |
| --- | --- |
| Provider | 按任意字节拆包、中文跨包、CRLF/多行 data、同包多事件、空统计包、双字段 delta、坏 JSON、错误对象、缺 DONE、非成功 HTTP |
| Core | 第一段先于请求结束到达；最后 Delta 不丢；只发一个终态；历史不重复；部分失败不进历史；第二轮顺序正确 |
| 生命周期 | 正文前/Thinking 中/输出中 Shutdown；UI 通道满时退出；task panic；事件消费者关闭；读流超时 |
| TUI | 思考与回答分离、只有思考时失败仍保留、Markdown 围栏跨 delta、手动滚动不自动跳底、定时刷新和终态末帧 |
| 阶段 C | 第二轮正确回传独立 reasoning；User/System 无该字段；Snapshot 保留；日志无正文/密钥 |

使用受控 fake、oneshot/Notify 和本地 HTTP fixture，不依赖付费模型或 sleep 猜调度。
TestBackend 验证布局，真实终端/PTY 验证退出恢复。已有 Core fake 测试不能证明 HTTP/SSE
解析正确，具体覆盖与验证结果见根 README，不因文档存在而算通过。

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

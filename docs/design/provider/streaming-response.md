# 模型流接口与 SSE 协议适配设计

核对日期：2026-10-30。状态：当前实现契约。架构归属：模型协议适配。
源码入口：[provider.rs](../../../crates/vortex-core/src/provider.rs)、[openai_chat](../../../crates/vortex-provider/src/openai_chat)、[sse_transport.rs](../../../crates/vortex-provider/src/sse_transport.rs)。实际检查结果见根 [README](../../../README.md)，验收要求不代表已经通过。

本文件维护模型流契约、wire 映射与 HTTP/SSE/Decoder 行为，不维护 Session 调度或 TUI 布局。
当前只有 OpenAI-compatible Chat Completions 适配器；思考选项见 [请求装配](thinking-options.md)。

## 分层与接口

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
    fn reasoning_output_policy(&self) -> ReasoningOutputPolicy {
        ReasoningOutputPolicy::Reject
    }
}

pub enum ReasoningOutputPolicy {
    Reject,
    DisplayOnly,
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
`Result::Err`，与正常结束原因区分。未知结束原因、多候选或 legacy function_call
返回协议错误，不猜测成功。

## Provider 的 SSE 解析

使用 Reqwest 字节流和 `eventsource-stream` 做 SSE 分帧，`serde_json` 解析单个 data。
使用 `async-stream` 构造惰性 Stream；不自行按网络 chunk 拆 UTF-8/JSON，也不引入自动
重连客户端。依赖归属见 [技术选型](../../architecture/technology-stack.md)。

当前适配器只请求一个 choice：

- HTTP 非成功状态先返回错误；成功状态核对 `Content-Type: text/event-stream`（允许参数）。
- SSE 注释/心跳与空 data 不产生文本；`choices: []` 的统计包不结束模型步骤。
- choice 0 的 `delta.content` 和 `delta.reasoning_content` 分别产生事件；同一个包有两者
  时分别处理，不能使用互斥 else if。多个工具的参数增量独立累积；仍拒绝非零 choice、
  多候选和 legacy function_call，非零工具 index 则属于同一响应的其他调用。
- 先处理包内 Delta，再记录 `finish_reason`。没有内容的终止包不报“空回复”。
- 严格完成策略：收到合法 `finish_reason` 后继续消费统计包，直到 `[DONE]` 才输出一次
  Finished；ToolCalls 结束时整批校验，成功后按工具 index 升序输出全部完整 ToolCall，
  最后输出 Finished，随后流结束。任一调用不合法则整批不发布 ToolCall。
  `[DONE]` 没有结束原因、EOF 缺少 `[DONE]`、结束原因重复或
  终止后继续输出正文均视为协议异常。其他供应商需要不同结束规则时在其 adapter 内明确支持。
- 错误 JSON、API 错误对象、HTTP/body 读取错误均输出一次 Err 后结束，错误文案不含原始
  响应体、认证头或完整请求。禁止把失败事件序列转换为空字符串成功结果。

工具增量由 `openai_chat/decoder.rs` 的 `BTreeMap<u32, PendingToolCall>` 按 index 保存
各自 id、name 和 arguments，支持交错到达。每个调用的 name/arguments 按到达顺序拼接；
首块 arguments 为空合法，须等后续片段与 DONE。DONE 时逐项解析完整 JSON，缺失 ID/名称、
非法 JSON 或工具信息与结束原因不匹配都失败，不提前发布已校验的部分调用。
后续 `id: ""` 不表示 ID 改变，应忽略；
首次非空 ID 保存，相同非空 ID 接受，不同非空 ID 仍报错。跨模型步骤的重复 ID 则由
Core 整批校验，批内不同 index 使用相同 ID 也由 Core 拒绝；Decoder 不持有跨请求的 ID 集合。

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
普通日志只记录有语义的元数据；显式 Debug 正文诊断走共享 WireTrace，不把 Debug 当成字段访问权限或新增 TRACE trait。

出现第二个真实协议、确认共有驱动重复后，再考虑内部 SseProtocolDecoder 抽象；不先
新增公开 SseProvider，也不在 Core 暴露厂商 event/signature。第一轮拆分不改变超时、
错误分类、惰性、背压或取消语义。原稿的“拒绝所有工具调用”已被工具版 v1 取代。

## 请求消息与工具映射

Chat Completions 将 ToolSpec 映射为 `tools[].function`。每个
`AssistantToolCalls { content, calls }` 映射为一条 Assistant 消息：前缀只写一次，无前缀时
content 为 null，tool_calls 数组保留全部调用及原顺序。结构化 arguments 仅在协议边界
序列化为 JSON 字符串；不再通过相邻 Message/ToolCall 猜测批次。
每个结果映射为独立的 `role: "tool"`、匹配的 `tool_call_id` 和文本 content；真实结果和
异常补齐结果使用相同映射。`is_error` 保留于内部类型和 UI 事件，wire 通过结果正文表达错误。

请求不设置 parallel_tool_calls=false；Decoder 接受多个调用不代表本地并行执行，调度
与预算归 Core。展示-only reasoning 仍不写入这些历史消息。

## 消费方与终态边界

ModelEvent::Finished 只结束一次模型步骤，不直接向 TUI 发布 TurnCompleted。
run_turn 决定工具继续与最终回答，Session 排空内部进度后提交历史并发送终态；
对应主契约分别见 [Agent loop](../runtime/agent-loop.md)、[Session](../runtime/session-runtime.md)。
输出策略与实际请求字段一起装配：ProviderDefault/Disabled 为 Reject，合法 Enabled 为
DisplayOnly。Decoder 仍解析 reasoning_content，是否允许消费由 Turn 检查，不能吞掉字段。
UI 的模型步骤、思考分区与待实施节流见 [Thinking 展示](../tui/thinking-display.md)。

## 日志、安全与验收

普通 Provider 日志只记录必要元数据，不记录请求/响应正文、思考或密钥；SecretString 保留手写脱敏
Debug。显式 Debug wire TRACE 是 [诊断设计](../cross-cutting/diagnostics.md) 规定的正文例外：
首次 poll 记录最终 ChatRequest；响应头到达后记录状态；完整 SSE Event.data 在 Decoder 前记录，
含公开思考、工具参数和 DONE，但不含 SSE id/event/retry、注释或尚未解析的原始字节。
正常流仍按事件 yield，不重复读取或等待完整回答。HTTP/MIME 异常由独立任务有界预览，
业务立即返回原错误；取消与进程退出不保证错误预览完整。

错误不能透传认证头、原始服务错误体或 Reqwest 敏感链。模型控制字符过滤是独立问题，
不能因 SSE/Markdown 可用就宣称已防护，通用凭据规则见 [凭据设计](../cross-cutting/credentials.md)。

离线验证 DTO、双字段 delta、参数跨包拼接、ID、终态/EOF 与错误分类；本地 HTTP fixture
另验任意字节分帧、中文跨包、CRLF、多行 data、MIME、状态码和超时。创建流不发请求、
首次 poll 才发送，Drop 停止本地消费；受控流测试不替代真实 HTTP 字节解析或终端验收。
方法和待补矩阵见 [回归测试方案](../../engineering/regression-test-plan.md)。

只有第二个真实协议出现后，才评估共有 Decoder 驱动；不预建公开 SseProvider。
思考历史回传须扩展语义和协议模型，目标见 [消息构建](../runtime/message-building.md)，不是放宽 EOF 校验。
本文保留本地 `20260919-01.md`、`20260922-01.md` 与 `2026-09-22-SSE抽象问题.md` 的稳定约束。

### Provider 专项矩阵

以下是验收要求，已有覆盖及运行结果仍只在根 README 记录：

| 模块与位置 | 场景 | 关键断言 |
| --- | --- | --- |
| `vortex-provider/src/openai_chat/request.rs` | 请求映射 | role/content、工具定义与结果正确，stream 为 true，千问开关按显式选项写入 |
| `src/openai_chat/decoder.rs`、`src/sse_transport.rs` | HTTP/SSE 异常与结束 | 状态码、Content-Type、坏 JSON、缺 DONE、超时映射正确；统计包不结束模型步骤 |
| `src/openai_chat/decoder.rs` | 多调用交错与整批失败 | 首块空参数合法、按 index 独立累积并排序输出；第二项非法时第一项也不发布 |
| `src/openai_chat/request.rs` | 批次映射 | 一条 Assistant 含完整数组、前缀一次，全部结果 ID/正文匹配；不回填 reasoning |
| `src/openai_chat/decoder.rs`、`src/sse_transport.rs` | 字节分帧与工具增量 | 中文跨 chunk、多行 data 不丢内容；参数拼接、空 ID 和非空 ID 变化正确处理 |
| `src/openai_chat/mod.rs` | 密钥脱敏 | SecretString 的 Debug 不包含原始密钥 |

## 参考与核对日期

2026-09-19 核对以下上游资料。行为随模型/服务变化，新增适配器必须重新验证：

- [千问流式输出][stream-doc]：兼容端点的 SSE、结束和统计包。
- [千问深度思考][thinking]：公开思考字段与开关。
- [Chat Completions 参数][chat-api]：思考历史与原始 HTTP 参数位置。
- [eventsource-stream 0.2.3](https://docs.rs/eventsource-stream/0.2.3/eventsource_stream/)：字节流到 SSE 事件。
- [async-stream 0.3.6](https://docs.rs/async-stream/0.3.6/async_stream/)：以 async 代码构造 Stream。

[thinking]: https://help.aliyun.com/zh/model-studio/deep-thinking
[chat-api]: https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions
[stream-doc]: https://help.aliyun.com/zh/model-studio/stream

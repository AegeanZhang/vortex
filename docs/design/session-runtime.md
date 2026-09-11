# Session Runtime 设计

本文档描述当前 v1 中 `vortex-core` 的 Session runtime，以及它与
`vortex-provider` 的边界。模型上下文未来如何裁剪和压缩，见
[`message-building.md`](message-building.md)；TUI 如何消费 Session，见
[`tui-interface.md`](tui-interface.md)。

## 当前范围

Session runtime 负责保存对话事实、串行化命令、调用模型并发布语义事件。它不知道
HTTP、OpenAI wire format 或终端控件。当前只支持单个非流式模型请求，不包含工具调用、
取消、持久化、恢复和模型切换。

```text
vortex-cli
    │  装配 Arc<dyn ModelProvider>
    v
start_session ──> AgentHandle ──command──> Session actor
                     TUI                    │
                     ^                      │ ModelRequest
                     │                      v
                CoreEventStream <──event── Provider
```

## 核心接口

| 类型 | 当前职责 |
| --- | --- |
| `ModelProvider` | 接收与厂商无关的 `ModelRequest`，异步返回完整回复或 `ProviderError` |
| `SessionConnection` | 一次性交付初始 `SessionSnapshot`、命令句柄和独占事件流 |
| `AgentHandle` | 可克隆的命令入口；隐藏有界 Tokio channel |
| `CoreEventStream` | 单消费者事件入口；隐藏接收端实现 |
| `AgentCommand` | 当前只有 `SubmitPrompt` 和 `Shutdown` |
| `CoreEvent` | 描述用户消息、Assistant 生命周期、完成、失败和命令拒绝 |

`start_session(Arc<dyn ModelProvider>)` 创建容量为 32 的命令 channel 和容量为 128 的
事件 channel，并启动一个 Tokio task。初始 Snapshot 固定为空消息和 `Idle`；当前尚未实现
历史恢复，因此这里还不承担“快照与订阅原子建立”的完整语义。

## 所有权与并发

Session actor 独占 `messages`、`status` 和 `JoinSet`，其他模块不能直接修改这些状态。
事件循环使用 `tokio::select!` 同时等待下一条命令和当前 Provider task 的结果：

- `messages` 是模型请求的事实来源；只有已接受的用户消息和成功回复会写入。
- `status` 为 `Running` 时拒绝新的 Prompt，因此逻辑上最多只有一个活跃 Turn。
- Provider 调用放进 `JoinSet`，使 actor 等待网络时仍能接收 `Shutdown`。
- `Shutdown`、所有命令发送端关闭或事件消费者消失时，actor 中止活跃 task 并退出。

## Turn 状态与事件顺序

有效 Prompt 的成功路径固定为：

```text
SubmitPrompt
  -> UserMessageAdded
  -> AssistantMessageStarted
  -> ModelProvider::complete
  -> AssistantTextDelta（当前一次发送完整回复）
  -> TurnCompleted
```

收到命令后先用 `trim()` 判断是否为空，但保留原始内容。空 Prompt 发出
`CommandRejected`；`Running` 期间再次提交也发出 `CommandRejected`。两种拒绝都不修改
消息历史和 Session 状态。

Provider 返回错误、task 异常结束或没有结果时，状态改为 `Failed` 并发出
`TurnFailed`。本轮已接受的 User 消息会保留，但失败的 Assistant 内容不会进入
`messages`。`Failed` 不阻止下一次提交：下一条有效 Prompt 会重新进入 `Running`，成功后
回到 `Idle`。

## Provider 适配边界

`vortex-provider::OpenAiChatProvider` 是当前唯一实现。它把 `MessageRole` 映射为
OpenAI-compatible Chat Completions 的 `user`、`assistant`、`system`，发送
`stream: false`，并读取第一个 choice 的非空 content。

网络故障映射为 `Transport`，非成功 HTTP 状态映射为 `HttpStatus`，JSON 或回复内容不合法
映射为 `InvalidResponse`。API key 由 CLI 从 `DEEPSEEK_API_KEY` 注入，Provider 的
`SecretString` 调试输出固定为 `***`；产品本身不读取 `.env`。

## 单元测试矩阵

通用组织方式和执行命令见 [`../engineering/testing.md`](../engineering/testing.md)。测试不得
调用真实付费 API。

| 模块与位置 | 场景 | 关键断言 |
| --- | --- | --- |
| `vortex-core/src/session.rs` | 成功 Turn | 四个事件顺序固定，请求包含用户消息 |
| 同上 | 空白 Prompt | 只收到 `CommandRejected`，Provider 未调用 |
| 同上 | Turn 运行中再次提交 | 第二条命令被拒绝，第一条仍可完成 |
| 同上 | Provider 返回错误 | 收到 `TurnFailed`，随后可再次提交 |
| 同上 | `Shutdown` | 受控 Provider task 被中止，事件流最终关闭 |
| `vortex-provider/src/lib.rs` | 三种角色映射 | 请求 JSON 的 role 与 content 正确，`stream` 为 false |
| 同上 | HTTP 与响应异常 | 状态码、坏 JSON、空 choices、空 content 分别映射正确 |
| 同上 | 密钥脱敏 | `SecretString` 的 Debug 不包含原始密钥 |

并发测试应使用 `oneshot`、`Notify` 或受控 fake Provider 明确控制完成时机，不依赖
`sleep` 猜测调度。Provider wire 测试优先测试纯转换和固定 JSON；必须覆盖 HTTP 时使用
本地 mock server。

## 后续演进

真正流式输出时，Provider 边界需要返回增量流，Core 再产生多次
`AssistantTextDelta`。`CancelTurn`、恢复历史、工具事件和模型切换也应先扩展 Core
契约，再由 CLI 或 TUI 使用，不能把厂商响应或界面状态塞进 `CoreEvent`。

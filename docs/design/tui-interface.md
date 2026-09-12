# TUI 接口设计

本文档以当前非流式 v1 为基线，说明 `vortex-tui` 如何消费
`vortex-core::SessionConnection`。Session actor 和 Provider 调用链见
[`session-runtime.md`](session-runtime.md)，技术选型见
[`../research/rust-tui-research.md`](../research/rust-tui-research.md)。

## 职责边界

`vortex-tui` 是由 `vortex-cli` 启动的 library crate，负责终端生命周期、输入映射、
界面状态和渲染。它依赖 Core 的语义类型，但不持有具体 Provider，不构造模型请求，也不
保存权威会话历史。

- CLI 读取环境变量、装配 Provider、启动 Session，再把连接交给 TUI。
- Core 保存对话事实、执行 Turn 并发布 `CoreEvent`。
- TUI 保存 Prompt、Transcript、notice 和用于展示的 Session 状态。
- Ratatui、Crossterm 和 `ratatui-textarea` 类型不得进入 `vortex-core`。

当前入口为：

```rust
pub async fn run(
    connection: SessionConnection,
    options: TuiOptions,
) -> Result<TuiOutcome, TuiError>;
```

`TuiOutcome::UserExit` 是当前唯一正常终态。`TuiOptions::color` 已预留但尚未生效。
终端 I/O、Session channel 和事件流关闭通过 `TuiError` 返回；TUI 不调用
`process::exit`。

## 模块与数据流

```text
Crossterm Event ──> map_event ─┐
                               ├─> UiAction ─> update(AppState) ─> Effect
CoreEventStream ────────────────┘                  │                 │
                                                   v                 v
                                              下一次 render     event loop 执行
```

| 模块 | 当前职责 |
| --- | --- |
| `lib.rs` | 公开入口、键盘事件映射和 `TuiOutcome` |
| `terminal.rs` | 初始化 raw mode/alternate screen，并用 guard 恢复终端 |
| `event_loop.rs` | 等待终端/Core 事件、调用 reducer、顺序执行 Effect |
| `app.rs` | `AppState` 和唯一状态修改入口 `update` |
| `action.rs` | `UiAction` 与 `Effect` |
| `ui.rs` | 四区布局和只读渲染 |
| `widgets/` | `PromptEditor` 与 `Transcript` 的局部状态和渲染 |

后台任务不能直接修改 UI 或绘制终端。`update` 是同步纯状态转换；发送异步命令和退出循环
由 event loop 执行。

当前模块只按已经出现的职责拆分，不建立通用 `Component`、`Screen`、router 或内部
event bus。等第二个页面、焦点切换或真正可复用的控件出现后，再从重复行为中提取抽象，
避免隐藏现有的单向控制流。

## 输入、Action 与 Effect

当前按键映射如下：

- `Ctrl+C` → `UiAction::Quit`。
- `Enter` → `UiAction::SubmitPrompt`。
- 其他 `KeyEvent` → `UiAction::EditPrompt`，交给 `PromptEditor`。
- 非键盘事件当前忽略。

提交时，TUI 只生成 `SendCommand(SubmitPrompt)`，不会乐观地把用户文本写入 Transcript。
显示更新以 Core 随后发送的 `UserMessageAdded` 为准。空输入会清空编辑器但不发送命令；
`Running` 时提交会保留编辑器内容，并在状态栏显示拒绝原因。

`Effect` 当前只有三种：发送 `AgentCommand`、请求重绘和退出。Effect 按返回顺序执行；退出
会先发送 `Shutdown`，成功后才结束 event loop。命令 channel 关闭时返回 `TuiError`。

## PromptEditor 边界

`PromptEditor` 是项目对 `ratatui-textarea::TextArea<'static>` 的薄封装。`PromptInput`
包装第三方 `Input`，使 `action.rs` 和 `AppState` 不需要了解 TextArea API；组件不暴露
`textarea()` 或可变 getter。使用 `'static` 避免把控件的生命周期传播到整个 AppState。

Enter 在全局映射阶段转换为 `SubmitPrompt`，`PromptEditor::handle_input` 也拒绝 Enter，
防止它意外插入换行。`take_text()` 使用 `std::mem::take` 取出文本，并以默认编辑器替换旧
实例，因此内容、光标、选择和 undo history 会一起重置，同时恢复边框与 placeholder。

当前 Prompt 区高度固定为 3，只支持 Enter 提交的单行交互。以后增加多行输入时，需要同时
确定插入换行的组合键和输入区高度策略，不能只放开 TextArea 的 Enter。

## Core 事件投影

| `CoreEvent` | `AppState` 变化 |
| --- | --- |
| `UserMessageAdded` | 追加 User Transcript entry |
| `AssistantMessageStarted` | 状态设为 `Running`，清除 notice，创建空 Assistant entry |
| `AssistantTextDelta` | 追加到最后一个 Assistant entry；不存在时创建一个 |
| `TurnCompleted` | 状态设为 `Idle`，清除 notice |
| `TurnFailed` | 状态设为 `Failed`，清除 notice 并追加 Error entry |
| `CommandRejected` | 只更新 notice，不改状态和 Transcript |

Turn 失败时，如果最后一项是空 Assistant 占位，`Transcript::fail_assistant` 会先删除它；
如果已存在部分回复，则保留部分内容并另加 Error entry。Snapshot 转换会恢复 User 和
Assistant 消息，System 消息当前不显示。

## Event loop 与绘制

Event loop 使用 `tokio::select!` 等待 `EventStream` 和 `CoreEventStream`。Action 经过
reducer 产生 Effect，所有 Effect 执行完后再决定退出或重绘。`redraw` 标志会合并同一批
Effect 中的重复请求，但当前没有定时 tick 或跨事件的刷新窗口。

页面固定分为 header、transcript、status 和 prompt 四区。Transcript 使用
`Paragraph` 换行渲染；空会话显示输入提示。当前没有滚动、Markdown、resize 专用状态或
窄窗口降级布局。

`TerminalGuard` 从 raw mode 开启后立即接管清理。正常退出显式调用 `cleanup`；初始化后
发生错误或 unwind 时由 `Drop` 尽力关闭 raw mode、离开 alternate screen 并显示光标。
TUI 运行期间不得使用 `println!` 破坏屏幕。

## 单元测试矩阵

通用规范见 [`../engineering/testing.md`](../engineering/testing.md)。优先测试 reducer 和
widget 的确定性状态，不通过真实键盘或真实 Provider 制造条件。

| 模块与位置 | 场景 | 关键断言 |
| --- | --- | --- |
| `app.rs` | Quit | `Shutdown` 位于 `Exit` 之前 |
| 同上 | 空输入、正常提交、Running 时提交 | 是否保留输入、发送命令和设置 notice 均正确 |
| 同上 | 每一种 `CoreEvent` | status、notice、Transcript 与 Redraw 一致 |
| `widgets/transcript.rs` | 多段 delta | 内容追加到同一 Assistant entry |
| 同上 | 失败前无内容/已有部分内容 | 分别删除空占位/保留部分回复，并追加 Error |
| 同上 | Snapshot 投影 | User/Assistant 保留，System 隐藏，顺序不变 |
| `widgets/prompt_editor.rs` | 普通输入、Backspace、Enter、提交重置 | `q` 等字符可输入，Enter 不新增行，取值后编辑状态恢复默认 |
| `ui.rs` | 标准布局与基本边框 | `TestBackend` 中四区位置正确 |
| 同上 | 空会话、消息、notice、窄窗口 | 关键文本可见且渲染不 panic |
| `event_loop.rs` | Effect 执行 | 命令发送、Redraw/Exit 聚合及 channel 错误正确 |
| `lib.rs` | 输入映射 | `Ctrl+C`、Enter、普通键和非键盘事件映射正确 |

终端恢复还需要人工或 PTY 集成测试，不能由 `TestBackend` 证明。event loop 测试应使用
fake Session/channel，禁止访问真实模型服务。

## 后续设计

以下能力尚未实现，不能作为当前代码的既有契约：

- `/model` 与 `AgentCommand::SelectModel`；模型只能在 Core 确认后更新显示。
- `CancelTurn`；未来 `Ctrl+C` 在 Running 时应先取消，Idle 时才退出。
- 真正流式 Provider、跨事件重绘节流和 Transcript 自动跟随/手动滚动。
- 工具状态、审批弹窗、Markdown、多行提交策略和会话恢复。
- 模型文本的终端控制字符过滤。

这些能力应先扩展 Core 的语义命令或事件，再投影到 TUI；不得把 Provider wire 类型或
Ratatui 控件作为跨 crate 接口。

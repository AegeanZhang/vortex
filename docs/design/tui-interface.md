# TUI 接口设计

本文档定义 `vortex-tui` 与 CLI、Agent core 之间的接口和状态边界。技术选型及外部实现对照见 [`../research/rust-tui-research.md`](../research/rust-tui-research.md)，命令行模式见 [`cli-surface.md`](cli-surface.md)。

## 目标与边界

`vortex-tui` 是由 `vortex-cli` 启动的 library crate，负责终端生命周期、输入映射、界面状态和渲染。它依赖 `vortex-core` 的语义类型，但不直接持有 Provider、Tool、配置加载器或会话存储实现。

- CLI 负责读取配置、装配具体实现、创建或恢复 Session，再启动 TUI。
- Core 负责 Agent loop、对话事实、当前模型、审批和任务状态。
- TUI 只保存输入框、光标、滚动、焦点、弹窗等展示状态。
- Ratatui、Crossterm 及控件类型不得出现在 `vortex-core` 的公共接口中。
- 非 TTY 和 `vortex exec` 不进入 TUI，继续使用 Core 的同一事件流生成纯文本或 JSONL 输出。

## 公共入口

`vortex-tui` 对外保持一个主入口：

```rust
pub async fn run(
    connection: SessionConnection,
    options: TuiOptions,
) -> Result<TuiOutcome, TuiError>;
```

`TuiOptions` 只包含颜色、全屏模式等 UI 偏好，不重复保存模型、Provider 或权限配置。`TuiOutcome` 表示用户退出或信号中断等正常终态；终端初始化、绘制及事件通道异常通过结构化 `TuiError` 返回，最终由 CLI 转换为中文提示和退出码。

`run` 内部拥有终端，从进入 raw mode 到恢复终端形成完整生命周期。调用方不应操作 Ratatui `Terminal`，TUI 也不调用 `process::exit`。

## Core 连接契约

`SessionConnection` 属于 `vortex-core`，由 Core 在启动 Agent runtime 时创建：

```rust
pub struct SessionConnection {
    pub snapshot: SessionSnapshot,
    pub agent: AgentHandle,
    pub events: CoreEventStream,
}
```

- `SessionSnapshot` 提供订阅建立时的一致状态，包括历史投影、当前模型、模型目录和任务状态。
- `AgentHandle` 是可克隆的命令入口，隐藏 Tokio channel 的具体类型。
- `CoreEventStream` 是当前消费者独占的有界事件流。

Snapshot 与事件订阅必须原子建立，避免读取快照后、开始收事件前丢失状态变化。TUI 不读取 JSONL 文件来补齐事件，也不自行调用 Provider 恢复历史。

TUI 可以发送的核心命令至少包括：

```rust
pub enum AgentCommand {
    SubmitPrompt { content: String },
    CancelTurn,
    SelectModel { target: ModelTarget },
    DecideApproval {
        request_id: ApprovalId,
        decision: ApprovalDecision,
    },
    Shutdown,
}
```

Core 通过 `CoreEvent` 返回用户消息、文本增量、工具状态、审批请求、模型切换、任务完成和结构化错误。Provider 的 SSE chunk、HTTP 响应体和 Ratatui/Crossterm 事件都不能成为 `CoreEvent`。

## 单向状态流

TUI 内部遵循单向数据流：

```text
Terminal event / CoreEvent / Tick / Signal
                    │
                    v
                 UiAction
                    │
                    v
             update(AppState)
                    │
                    v
                  Effect
          ┌─────────┼─────────┐
          v         v         v
 AgentCommand    Redraw      Exit
```

核心接口为：

```rust
fn update(state: &mut AppState, action: UiAction) -> Vec<Effect>;

fn render(frame: &mut Frame<'_>, state: &AppState);
```

只有 `update` 修改 `AppState`。输入映射把 `KeyEvent` 转成 `UiAction`；渲染函数只读取状态；event loop 负责执行 Effect。后台任务只能发送事件，不直接修改 UI 状态或绘制终端。

事件通道必须有界。收到连续文本增量时先更新状态并标记 dirty，在短时间窗口内合并重绘；空闲时不固定高频刷新。用户向上滚动后暂停自动跟随，回到底部时恢复。

## `/model` 契约

`/model` 是 UI Intent，不是 Provider 调用。无参数时打开选择器，带目标时可以直接请求切换：

```text
/model
/model deepseek/deepseek-v4-pro
```

处理顺序固定为：

1. TUI 从 Snapshot 或后续 Core event 中读取模型目录。
2. 用户选择后，TUI 发送 `AgentCommand::SelectModel`。
3. Core 校验 Provider、凭据、模型能力和当前 Turn 状态。
4. 成功时 Core 写入会话事件并发出 `CoreEvent::ModelChanged`；失败时发出结构化拒绝事件。
5. TUI 仅在收到确认事件后更新当前模型显示，不做乐观切换。

切换默认只影响下一次模型请求，不修改用户级或项目级 TOML。正在流式响应或存在未闭合 Tool call 时，由 Core 明确拒绝或延后切换；TUI 不自行决定。模型切换后的 Token 预算重算、Provider 选择和协议缓存失效也由 Core 与 Provider 层处理。

## 终端与故障处理

- 使用 RAII guard 管理 raw mode、alternate screen、光标和鼠标捕获，正常返回、错误及 panic 路径都必须恢复终端。
- TUI 运行期间禁止 `println!` 和直接写 stdout；终端绘制使用 stderr，诊断信息交给 `tracing`。
- 第一次 `Ctrl+C` 在任务运行时发送 `CancelTurn`；空闲时触发退出。取消应由 Core 继续传播到 Provider、工具和子进程。
- Agent 的单次任务失败通常显示在会话中，不结束 TUI；终端故障或 Core 通道意外关闭才结束 `run`。
- 渲染模型和工具文本前过滤控制字符，不把未经处理的 ANSI escape sequence 写入终端。

## 内部模块建议

```text
vortex-tui/src/
├── lib.rs          # run 与公开类型
├── app.rs          # AppState 与 update
├── action.rs       # UiAction 与 Effect
├── event_loop.rs   # Tokio select 与 Effect 执行
├── input.rs        # 终端输入映射
├── terminal.rs     # TerminalGuard 与恢复
├── ui.rs           # 页面布局和纯渲染
└── widgets/        # transcript、prompt、model picker
```

模块应在对应职责出现时再建立。首版先完成 transcript、单行输入、取消和文本流；审批弹窗、`/model`、Markdown 与多行编辑器随后按真实需求加入。

## 测试要求

- reducer 单元测试覆盖输入、提交、滚动、取消、Core event 投影和模型切换确认。
- Ratatui `TestBackend` 覆盖常规、窄窗口和 resize 后的关键布局。
- 使用 fake `SessionConnection` 测试命令发送、事件顺序、通道关闭和背压，不调用真实 Provider。
- PTY 集成测试覆盖正常退出、`Ctrl+C`、错误和 panic 后的终端恢复。
- `/model` 测试必须确认请求发出后不会提前更新状态，只有 `ModelChanged` 才使选择生效。

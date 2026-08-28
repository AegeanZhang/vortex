# TUI 手写实现方案

## 目标与边界

本练习用于理解 Ratatui 应用的核心机制，而不是直接复制 Codex 的完整实现。最终做出一个简化的 Agent 对话界面：上方显示可滚动的消息记录，中间是单行输入框，底部显示状态和快捷键；程序能够模拟流式回复，并在退出或异常时恢复终端。

首轮只使用 **Ratatui + Crossterm**。完成同步版本后再引入 Tokio，避免同时学习渲染、终端控制和异步调度。暂不实现 Markdown、高亮、鼠标、自定义 Terminal、多行编辑器和真实模型调用。

## 目标界面

```text
┌ Vortex TUI Demo ───────────────────────────┐
│ User: 请解释 Cargo Workspace               │
│ Agent: Cargo Workspace 可以统一管理……       │
│                                            │
├ Prompt ────────────────────────────────────┤
│ 输入内容_                                  │
├────────────────────────────────────────────┤
│ Ready  Enter:发送  ↑/↓:滚动  Ctrl+C:退出    │
└────────────────────────────────────────────┘
```

建议布局为 `Constraint::Min(1)`、`Constraint::Length(3)`、`Constraint::Length(1)`。窗口过小时优先保留状态栏和输入框，消息区允许缩小到零。

## 建议结构

先在 `main.rs` 中完成第一个可运行版本，理解完整调用链后再拆分：

```text
src/
├── main.rs       # 启动、主循环和错误返回
├── app.rs        # AppState、Action 和状态更新
├── event.rs      # 终端、时钟及模拟 Agent 事件
├── terminal.rs   # raw mode、alternate screen 和恢复
└── ui.rs         # 纯渲染函数
```

状态变化遵循单向数据流：

```text
Crossterm/Tokio event -> Action -> update(AppState) -> draw(AppState)
```

`ui.rs` 只读取状态；按键判断、消息追加和退出逻辑不要写进渲染函数。

## 分阶段实施

### 1. 终端生命周期与静态布局

添加 `ratatui`、`crossterm` 依赖。进入 raw mode 和 alternate screen，创建 Terminal，绘制三个区域；同步读取按键，按 `q` 退出。使用 guard 或统一清理函数确保任何 `Result::Err` 都会恢复 raw mode、显示光标并离开 alternate screen。

验收：界面可随终端 resize 重绘；正常退出后 shell 输入和回显正常；TUI 运行期间不使用 `println!`。

### 2. AppState 与 Action

定义最小状态：`running`、`messages`、`input`、`cursor`、`scroll` 和 `status`。把终端事件映射成 `Action`，再由单独的 `update` 函数修改状态。建议至少包含 `Insert`、`Backspace`、`Submit`、`ScrollUp`、`ScrollDown`、`Resize` 和 `Quit`。

验收：输入、删除、提交和退出均只通过 Action 改变状态；渲染函数不持有可变业务状态。

### 3. 输入框与消息记录

先实现单行输入：字符插入、Backspace、左右移动、Enter 提交。提交后把内容加入消息记录并清空输入。光标位置必须按终端显示宽度计算，至少手工验证中文和 emoji；不要直接假设“字符数等于显示列数”。

消息区域支持上下滚动，并在新消息到达且用户位于底部时自动跟随。用户已向上滚动时，不要强制跳回底部。

验收：空输入不会提交；中文输入不 panic；长消息、窄窗口和 resize 后仍可阅读。

### 4. 模拟流式 Agent

提交问题后生成一段固定回复，每次 tick 追加一个或几个字符。增加 `Idle`、`Streaming`、`Cancelled` 三种运行状态；流式期间允许继续滚动，并支持 `Esc` 取消。先用 `event::poll(timeout)` 同时承担输入等待和 tick，不急于引入线程。

验收：回复逐步出现且输入仍有响应；取消后停止追加；空闲时不进行无意义的高频重绘。

### 5. 迁移到异步事件循环

同步版稳定后再加入 Tokio，并为 Crossterm 启用 `event-stream` feature。将事件统一为应用自己的枚举，例如 `Terminal`、`Tick`、`AgentChunk`、`AgentDone` 和 `AgentFailed`；通过有界 channel 传递模拟 Agent 输出，在主循环中用 `tokio::select!` 合并事件。

UI 状态仍只在主循环中修改，后台任务只发送事件。取消流式任务时保存并触发对应的 cancellation handle，避免留下孤立任务。

验收：快速连续输入时界面不冻结；退出后后台任务全部结束；慢消费者不会造成 channel 无限增长。

### 6. 测试与收尾

先测试无终端依赖的 `update`：字符编辑、提交、滚动边界、取消和任务结束。再使用 Ratatui `TestBackend` 检查 `80x24`、`40x10` 两种尺寸的关键文本与边框位置。最后手工测试正常退出、`Ctrl+C`、主动返回错误和 resize 后的终端恢复。

每完成一个阶段运行：

```bash
cargo fmt --all -- --check
cargo clippy -p tui-demo --all-targets -- -D warnings
cargo test -p tui-demo
cargo run -p tui-demo
```

## 推荐按键

| 按键 | 行为 |
|---|---|
| `Enter` | 提交输入 |
| `Left` / `Right` | 移动输入光标 |
| `Up` / `Down` | 滚动消息记录 |
| `PageUp` / `PageDown` | 按页滚动 |
| `Esc` | 取消当前流式回复 |
| `Ctrl+C` | 退出程序 |

第一阶段可临时使用 `q` 退出；输入框启用后，普通字母都应作为文本，退出统一改为 `Ctrl+C`。

## 学习重点与暂缓项

实现过程中重点回答三个问题：谁拥有终端、谁可以修改状态、一个异步事件如何触发一次必要的重绘。每阶段完成后再拆文件，避免用模块数量掩盖尚未理解的控制流。

暂缓 `tui-textarea`、Markdown renderer、snapshot 框架和真实 LLM 流。手写单行编辑器是为了理解光标与 Unicode；掌握后再用 `PromptEditor` 接口替换为成熟控件。真实 Agent 接入前，先保证模拟流的取消、背压和终端恢复都可靠。

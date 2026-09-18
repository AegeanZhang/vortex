# Rust TUI 技术调研

- 调研日期：2026-08-26
- 适用项目：Vortex CLI Agent
- 状态：建议采用，待原型验证

## 结论

首版建议采用 **Ratatui + Crossterm + Tokio**：Ratatui 负责布局、组件和差量绘制，Crossterm 负责 raw mode、alternate screen、键鼠及终端事件，Tokio 统一承载模型流、工具执行和 UI 事件。多行输入优先使用 `tui-textarea`，但应封装为项目自己的 `PromptEditor`，避免业务状态依赖第三方控件 API。

首版使用全屏 alternate screen，内部维护消息滚动；渲染层保留 viewport 抽象，以便以后增加 inline 模式。交互 TUI 写入 `stderr`，`vortex exec` 的最终结果或结构化输出写入 `stdout`，保证管道调用可用。

## 需求侧重点

Vortex 的 TUI 不是普通表单，而是长时间运行的流式界面，需要同时处理：

- 模型文本、计划和工具输出持续追加；
- 用户输入、滚动、取消及审批弹窗；
- 长会话、Markdown、代码块、CJK 和 emoji；
- 异步任务失败后界面仍能响应，并可靠恢复终端状态；
- TTY 与非 TTY、窄窗口、SSH/tmux 及不同终端能力。

## 方案比较

| 方案 | 优点 | 主要问题 | 结论 |
|---|---|---|---|
| Ratatui + Crossterm | 立即模式、布局灵活、生态完整，支持测试 backend；Crossterm 是默认跨平台 backend | 应用需自行设计状态、事件循环和焦点管理 | **采用** |
| tui-realm | 在 Ratatui 上提供组件、Message/Event 和 Elm 风格更新 | 与 Vortex 自身事件模型重叠，引入额外框架约束 | 暂不采用 |
| iocraft | React/SwiftUI 风格声明式 API、Flexbox 和 hooks，上手直观 | 仍为 0.x；异步示例偏向 smol，输出流能力仍标为 unstable | 保留观察 |
| Cursive | retained-mode View 和 callback 适合表单类应用 | 高频流式 transcript、自定义渲染和 Tokio 协调不如 Ratatui 直接 | 不采用 |
| 仅 Crossterm | 依赖少、控制最直接 | 需自行实现布局、裁剪、滚动、样式和 diff | 不采用 |

调研时最新版为 Ratatui 0.30.2、Crossterm 0.29.0、`tui-textarea` 0.7.0、tui-realm 4.1.0、iocraft 0.8.4 和 Cursive 0.21.1。实现时应重新确认兼容版本并提交 `Cargo.lock`，不要使用宽泛的 `*` 依赖。

## Codex TUI 实现对照

本节基于 OpenAI Codex 本地源码提交 [`068c49f075`](https://github.com/openai/codex/tree/068c49f075cf287a1fe7d1ee36cf005efac922e7)（2026-08-24）。结论是：**Codex 使用成熟库作为底座，但产品级 TUI 的大部分状态管理和高级行为均为自研**。

### 成熟库底座

Codex 直接使用 Ratatui 0.30.2、Crossterm 0.29.0 和 Tokio；另外使用 pulldown-cmark、syntect、textwrap、Unicode 处理库、arboard、insta 与 vt100 分别承担 Markdown 解析、代码高亮、换行、剪贴板及测试。其 TUI 没有直接依赖 `tui-textarea`、tui-realm、iocraft 或 Cursive。[TUI 依赖清单](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/Cargo.toml#L73-L165)

Codex 启用了 Ratatui 的 scrolling region 和多个 unstable feature；Crossterm 还被替换为 OpenAI 维护的固定 revision fork。这说明成熟库仍是基础，但终端边界行为需要可控版本。[workspace 依赖与 patch](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/Cargo.toml#L388-L393)

### 自研层

| 模块 | Codex 做法 | 对 Vortex 的启示 |
|---|---|---|
| Terminal | 从 `ratatui::Terminal` 派生并维护自有实现，处理 viewport、buffer diff、cursor、超链接和 scrollback | 首版全屏模式先用标准 Terminal；出现 inline/原生 scrollback 需求后再扩展 |
| 输入编辑器 | 自研 `TextArea`，覆盖 Unicode、软换行、kill buffer、文本元素、Vim 模式和运行时 keymap | 首版可用 `tui-textarea`，但必须隔离在 `PromptEditor` 接口之后 |
| Markdown | 以 pulldown-cmark 为 parser，自研 Ratatui renderer、流式块缓存、表格布局、文件链接和代码高亮 | 保留原始 Markdown，先做当前消息重解析，再按稳定块缓存 |
| 事件 | 在 Crossterm `EventStream` 外增加 `EventBroker`，暂停时真正释放 stdin，避免外部编辑器抢输入 | 终端输入生命周期必须独立于 Agent core，并可暂停/恢复 |
| Transcript | 完成的 history 通过 ANSI scrolling region 写入终端 scrollback | v0.1 不复制该复杂度；先维护内部滚动视图 |
| 测试 | 自研 Crossterm + vt100 backend，并大量使用 insta snapshot；该提交包含约 694 个 `.snap` 文件 | snapshot 覆盖布局，PTY 测试覆盖 escape sequence 与恢复流程 |

Codex 同时支持 alternate screen 和 inline 模式；`auto` 允许使用 alternate screen，`--no-alt-screen` 禁止切换。[模式选择逻辑](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/src/lib.rs#L1776-L1790)

2026-09-15 补充：允许 alternate screen 不等于普通聊天持续占用备用屏幕。较新源码的聊天
采用 inline 区域，Diff 等界面另行切换；调用链与菜单、多行输入的详细证据见
[`tui-layout-and-input-research.md`](tui-layout-and-input-research.md)。本节其余选型结论保留原调研日期的语境。

相关源码：[`custom_terminal.rs`](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/src/custom_terminal.rs)、[`textarea.rs`](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/src/bottom_pane/textarea.rs)、[`markdown_render.rs`](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/src/markdown_render.rs)、[`event_stream.rs`](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/src/tui/event_stream.rs)、[`insert_history.rs`](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/src/insert_history.rs) 和 [`test_backend.rs`](https://github.com/openai/codex/blob/068c49f075cf287a1fe7d1ee36cf005efac922e7/codex-rs/tui/src/test_backend.rs)。

### 对当前结论的影响

Codex 的实现验证了 Ratatui + Crossterm + Tokio 路线，但不应把其多年演进后的自研层当作 Vortex v0.1 的起点。Vortex 应先使用标准 Ratatui 全屏渲染、官方 Crossterm 和适配后的 `tui-textarea`；事件、UI 状态与 Agent core 从第一天解耦。当实际需求证明标准组件不足时，再分别替换输入编辑器、Markdown renderer 或 Terminal，而不是一次性复制 Codex 架构。

## 建议架构

```text
Terminal events ──> Input mapping ──> UiIntent ──> Agent core
                                             │
Provider / tools / approvals ──> CoreEvent ──┘
                                  │
                                  v
                           reducer(AppState)
                                  │
                                  v
                          Ratatui render(Frame)
```

- 只有 reducer 修改 `AppState`；widget 仅根据状态渲染，不直接调用 Provider 或工具。
- Tokio 主循环使用 `select!` 合并终端输入、CoreEvent、信号和渲染节拍。事件通过有界 channel 传递，避免工具输出无限占用内存。
- 收到事件只标记 dirty，并在约 16–33 ms 窗口内合并重绘；空闲时不固定刷新。长 transcript 只布局可见窗口，禁止每帧复制全部历史。
- CoreEvent 使用稳定语义，如文本增量、工具开始/输出/结束、审批请求、计划变化和任务结束；不要把 Provider 原始 SSE 事件泄漏给 UI。

## 页面与交互

默认布局由 transcript、动态高度输入框和单行状态栏组成。审批、错误详情和帮助使用 overlay，不切换 Agent core 状态。输入框支持 bracketed paste、多行编辑、历史和取消；快捷键先通过内部 `Action` 映射，再交给状态机，避免在 widget 内散落按键判断。

流式消息应保存原始 Markdown，渲染时生成 Ratatui `Text/Line/Span`。首版可以在当前消息变化后重新解析该消息，而不是做不完整的增量 Markdown parser；后续再按块缓存。模型和工具文本必须过滤控制字符，禁止把未经处理的 ANSI escape sequence 直接写入终端。

## 终端生命周期与故障处理

- 使用 RAII guard 管理 raw mode、alternate screen、鼠标捕获和光标；正常退出、错误、panic 及信号路径都必须恢复。
- UI 运行期间禁止 `println!` 与直接写终端，诊断信息通过 `tracing` 写文件或内存缓冲。
- 第一次 `Ctrl+C` 取消当前 Agent 任务并保留会话；任务空闲时再次触发才退出。取消动作必须传播到 Provider 流、工具子进程和 MCP 调用。
- 非 TTY 环境不启动 Ratatui，自动进入纯文本/结构化输出路径；该路径不得依赖 UI crate。

## 测试与原型验收

1. reducer 单元测试覆盖输入、滚动、审批、任务切换、取消和错误状态。
2. 使用 Ratatui `TestBackend` 与 snapshot 测试 `80x24`、`120x40` 和极窄窗口。
3. 验证每秒 50 次文本增量时输入无明显卡顿，10,000 行 transcript 可平滑滚动。
4. 验证多行粘贴、中文、组合字符、emoji、超长单词和终端 resize。
5. 用 PTY 集成测试正常退出、`Ctrl+C`、panic 和子任务失败后终端模式均被恢复。
6. 审批弹窗期间后台事件继续接收，但危险工具在用户决策前不得执行。

## 风险与应对

- **依赖兼容**：Ratatui 警告不要同时引入不兼容的 Crossterm 大版本，否则事件队列和 raw mode 状态可能分裂。统一由 workspace 管理版本，并隔离 `tui-textarea`。
- **长会话性能**：采用可见区虚拟化、换行结果缓存和限量工具输出；不以高频 tick 掩盖低效渲染。
- **流式 Markdown 抖动**：当前消息允许重新解析，完成后冻结为缓存块；代码围栏未闭合时按普通代码块展示。
- **终端差异**：首版保证基础 16 色和键盘操作，真彩、鼠标、OSC 52 剪贴板及增强键盘协议仅作能力探测后的渐进增强。

## 主要资料

- [Ratatui 0.30.2 API 与应用结构](https://docs.rs/ratatui/latest/ratatui/)
- [Ratatui async event loop](https://ratatui.rs/tutorials/counter-async-app/full-async-events/)
- [Ratatui backend 与 TestBackend](https://ratatui.rs/concepts/backends/)
- [Ratatui inline viewport 示例](https://ratatui.rs/examples/apps/inline/)
- [Ratatui snapshot testing](https://www.ratatui.rs/recipes/testing/snapshots/)
- [Crossterm 0.29.0 API 与 EventStream](https://docs.rs/crossterm/latest/crossterm/)
- [`tui-textarea` 0.7.0 文档](https://docs.rs/tui-textarea/latest/tui_textarea/)
- [tui-realm 4.1.0 文档](https://docs.rs/tuirealm/latest/tuirealm/)
- [iocraft 0.8.4 文档](https://docs.rs/iocraft/latest/iocraft/)
- [Cursive 0.21.1 文档](https://docs.rs/cursive/latest/cursive/)

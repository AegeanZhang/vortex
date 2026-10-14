# 文档索引

Vortex 的文档分为架构、工程规范、详细设计、调研和用户手册五类。下面写清每份是什么、什么时候查。

## 架构

- [`architecture/README.md`](architecture/README.md) —— 架构文档入口，包含系统总览、仓库组织、技术选型与术语表。
- [`architecture/decisions/0001-transcript-markdown-rendering.md`](architecture/decisions/0001-transcript-markdown-rendering.md) —— 首版使用 tui-markdown 的决策逻辑、依赖边界与后续优化条件。

  **什么时候查**：新建 crate、决定某段代码放进哪个 crate、拿不准某处能否依赖另一个 crate 时。

## 工程规范

- [`engineering/coding-style.md`](engineering/coding-style.md) —— Rust 格式与命名约定。
- [`engineering/testing.md`](engineering/testing.md) —— 测试组织和提交前检查命令。
- [`engineering/regression-test-plan.md`](engineering/regression-test-plan.md) —— Fake/受控流、本地 HTTP 与搜索工具的回归测试步骤；区分待补覆盖和真实验证。
- [`engineering/code-cleanup.md`](engineering/code-cleanup.md) —— 旧实现注释、占位模板的清理方法，以及仍应保留的执行链路。
- [`engineering/git-workflow.md`](engineering/git-workflow.md) —— Commit、分支与 Pull Request 约定。
- [`engineering/review-checklist.md`](engineering/review-checklist.md) —— Review 时检查架构、正确性与安全性的清单。

  **什么时候查**：编写、检查或提交代码与文档时。

## 详细设计

单个子系统的对外契约，是写代码时的直接依据。

理解工具版 TUI 时，建议依次阅读 `session-runtime.md`、`agent-loop.md`、
`streaming-response.md`、`tui-interface.md` 和 `message-building.md`：先看调度与工具循环，
再看协议和界面投影，最后区分最小请求模型与后续上下文构建目标。

- [`design/cli-surface.md`](design/cli-surface.md) —— `vortex` 命令行接口：模式、参数、输出契约、退出码，以及各部分归属哪个落地阶段。

  **什么时候查**：实现或修改 CLI 时。参数命名与语义以此为准，不要在代码里临时决定。

- [`design/credentials.md`](design/credentials.md) —— 密钥从哪里来、`.env` 的定位、Provider 配置形状、日志脱敏规则。

  **什么时候查**：接入新 Provider、处理 API 密钥、写任何会记录请求内容的日志时。

- [`design/session-runtime.md`](design/session-runtime.md) —— 当前 Session actor、Provider 抽象、Turn 状态和事件顺序。

  **什么时候查**：阅读 `vortex-core`、排查 TUI 为何收到某个事件，或补充 Session/Provider 测试时。

- [`design/streaming-response.md`](design/streaming-response.md) —— SSE 增量接口与任务终态，以及后续 Thinking 展示/历史策略和绘制节流。

  **什么时候查**：理解流式 Provider、增加思考显示或验证流中断与退出时。

- [`design/agent-loop.md`](design/agent-loop.md) —— 工具版 v1 的 Tool/Registry、模型与工具循环、历史回填、ID/轮数限制、秘塔与 Mock 搜索及测试契约。

  **什么时候查**：阅读 run_turn、接入真实工具，或补充工具调用与多步失败测试时。

- [`design/message-building.md`](design/message-building.md) —— 会话事件如何形成模型上下文，以及 Tool call、Token 预算和 Provider 映射规则。

  **什么时候查**：实现 Agent loop、上下文压缩、工具调用或 Provider 请求转换时。

- [`design/tui-interface.md`](design/tui-interface.md) —— TUI 状态流、slash v1 的解析/补全菜单与 /exit、增量事件投影、终端生命周期及后续交互边界。

  **什么时候查**：阅读 `vortex-tui`、核对 slash 的 Enter/Tab/Esc 行为、补充 reducer/widget 测试，或增加终端交互时。

- [`design/transcript-scrolling.md`](design/transcript-scrolling.md) —— Transcript 的滚动状态、自动跟随、按键映射、滚动条渲染和测试边界。

  **什么时候查**：实现长对话浏览、滚动条或 Transcript resize 行为时。

- [`design/terminal-redraw-recovery.md`](design/terminal-redraw-recovery.md) —— Kitty 中边框/状态栏丢失的证据、排查步骤和强制重绘候选设计；尚未确认根因或实现修复。

  **什么时候查**：排查休眠恢复后的画面异常，或设计 Resize/Ctrl+L 重绘时。

- [`design/development-directions.md`](design/development-directions.md) —— 后续输入、取消、配置、上下文、工具与会话能力的候选方向和依赖，不作为完成状态或固定排期。

  **什么时候查**：讨论下一项能力的范围与前置条件时。

## 调研

技术选型与外部对照，回答"这个取舍为什么是这样"。

- [`research/README.md`](research/README.md) —— 调研文档入口，按 Agent、TUI、CLI 与配置主题索引现有调研。
- [`research/web-search-research.md`](research/web-search-research.md) —— Web Search 服务形态、国内外候选、Codex/Claude Code 对照，以及 Vortex 的接入边界与首版建议。
- [`research/rust-tui-research.md`](research/rust-tui-research.md) —— TUI 技术选型，结论是 Ratatui + Crossterm + Tokio。
- [`research/tui-layout-and-input-research.md`](research/tui-layout-and-input-research.md) —— Codex/Claude Code 屏幕模式与输入交互对照，以及 Vortex slash 菜单、多行输入和粘贴方案。
- [`research/transcript-markdown-research.md`](research/transcript-markdown-research.md) —— Markdown 渲染库选型、Codex/Claude Code 实现对照，以及 Transcript 排版、滚动和流式展示的候选方案。
- [`research/cli-design-research.md`](research/cli-design-research.md) —— Claude Code 与 Codex CLI 的接口对照，`design/cli-surface.md` 中若干决策的依据。
- [`research/credential-handling-research.md`](research/credential-handling-research.md) —— 两者如何处理 API 密钥与 `.env`，`design/credentials.md` 的依据。

  **什么时候查**：想改动某个已定的设计、觉得现有方案别扭时。先看调研里有没有已经权衡过的反例。

## 用户手册

- [`guide/getting-started.md`](guide/getting-started.md) —— 当前真实可运行的命令：构建、检查、运行实验。

  **什么时候查**：刚接触这个仓库、想跑起来看看时。

## 两条约定

**设计文档区分基线契约与后续目标，不维护独立进度表。** 当前完成到哪一步及验证结果
以仓库根目录的 `README.md` 为准，
各实验的进度记录在自己的计划文档里（如 `playground/tui-demo/docs/implementation-plan.md`）。
描述代码接口时同步相关设计，并明确标注未实现的目标类型，避免把规划误当成现有 API。

**用户手册只写实际验证过的内容。** 尚未实现的命令属于设计，写在 `design/` 下并标明目标形态，
不要写进手册——手册描述不存在的行为，比没有手册更糟。

## 临时稿回收记录（2026-10-06）

以下本地稿的长期有效内容已合并到正式文档；旧 API、已完成迁移的逐行代码和重复内容
不继续维护。待实现项仍明确标为方案，回收不代表对应功能或测试已完成。

| 原临时稿 | 正式文档去向 |
| --- | --- |
| `20260919-01.md`、`20260919-02-test.md` | [SSE 设计](design/streaming-response.md)、[回归测试方案](engineering/regression-test-plan.md) |
| `20260922-01.md`、`2026-09-22-SSE抽象问题.md` | [SSE 的跨协议边界与等价拆分](design/streaming-response.md) |
| `20260927-01.md` | [Agent loop](design/agent-loop.md)、[回归测试方案](engineering/regression-test-plan.md) |
| `20261002-01.md` | [秘塔接入契约](design/agent-loop.md)、[搜索专项回归](engineering/regression-test-plan.md) |
| `20261006-01.md` | [旧代码清理方案](engineering/code-cleanup.md) |
| `2026-09-22-tui-display-loss.md` | [终端画面恢复待验证方案](design/terminal-redraw-recovery.md) |
| `codex-tui.md` | [TUI 布局与输入调研](research/tui-layout-and-input-research.md) 已包含对应观察，不重复追加 |

`rust-learn.md` 是个人学习笔记，问题尚未整理为项目规范，本次保留，不作为工程事实引用。

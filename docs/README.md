# 文档索引

Vortex 的文档分为架构、工程规范、详细设计、调研和用户手册五类。下面写清每份是什么、什么时候查。

## 架构

- [`architecture/README.md`](architecture/README.md) —— 架构阅读导航、文档职责与维护规则。
- [`architecture/overview.md`](architecture/overview.md) —— 当前系统边界、整体架构与未接入能力。
- [`architecture/logical-modules.md`](architecture/logical-modules.md) —— 八个业务逻辑模块及横切诊断的职责、接口、状态归属及源码映射。
- [`architecture/runtime-flows.md`](architecture/runtime-flows.md) —— 启动、Prompt/工具循环、事件边界、模型切换和退出时序。
- [`architecture/repository-structure.md`](architecture/repository-structure.md) —— 实际目录、crate 依赖和源码阅读入口。
- [`architecture/technology-stack.md`](architecture/technology-stack.md) —— 当前技术选择及尚未接入的目标选型。
- [`architecture/evolution.md`](architecture/evolution.md) —— 配置、Session 与 TUI 的后续内部拆分建议及功能演进条件，不代表已经重构。
- [`architecture/glossary.md`](architecture/glossary.md) —— 逻辑模块、运行时与协议相关术语。
- [`architecture/decisions/0001-transcript-markdown-rendering.md`](architecture/decisions/0001-transcript-markdown-rendering.md) —— 首版使用 tui-markdown 的决策逻辑、依赖边界与后续优化条件。

  **什么时候查**：理解整体系统、梳理模块职责、跟踪调用链，或判断代码应放在哪里、是否需要拆分时。

## 工程规范

- [`engineering/coding-style.md`](engineering/coding-style.md) —— Rust 格式与命名约定。
- [`engineering/testing.md`](engineering/testing.md) —— 测试组织和提交前检查命令。
- [`engineering/regression-test-plan.md`](engineering/regression-test-plan.md) —— Fake/受控流、本地 HTTP、搜索与目录工具的回归测试方法；区分待补覆盖和真实验证。
- [`engineering/code-cleanup.md`](engineering/code-cleanup.md) —— 旧实现注释、占位模板的清理方法，以及仍应保留的执行链路。
- [`engineering/git-workflow.md`](engineering/git-workflow.md) —— Commit、分支与 Pull Request 约定。
- [`engineering/review-checklist.md`](engineering/review-checklist.md) —— Review 时检查架构、正确性与安全性的清单。

  **什么时候查**：编写、检查或提交代码与文档时。

## 详细设计

[设计导航](design/README.md) 按架构职责索引接口、状态、不变量及验收要求，并记录旧文档的迁移去向。

- 应用与配置：[启动装配](design/application/startup.md)、[CLI](design/application/cli-surface.md)、
  [安装路径](design/application/installation.md)、[配置](design/configuration/configuration.md)。
- 运行时：[模型管理](design/runtime/model-management.md)、[Session](design/runtime/session-runtime.md)、
  [Agent loop](design/runtime/agent-loop.md)、[消息构建](design/runtime/message-building.md)。
- 模型协议：[SSE/流接口](design/provider/streaming-response.md)、[思考选项](design/provider/thinking-options.md)。
- 工具：[注册表](design/tools/tool-registry.md)、[目录列表](design/tools/list-directory.md)、[网页搜索](design/tools/web-search.md)。
- 交互：[TUI](design/tui/interface.md)、[Thinking 展示](design/tui/thinking-display.md)、
  [滚动](design/tui/transcript-scrolling.md)、[文本选择与复制](design/tui/text-selection.md)、
  [终端画面恢复](design/tui/terminal-redraw-recovery.md)。
- 横切与候选：[凭据](design/cross-cutting/credentials.md)、[开发期 HTTP 诊断](design/cross-cutting/diagnostics.md)、
  [开发方向](design/development-directions.md)。

**什么时候查**：编写或 Review 某个子系统时，从其主文档查契约；阅读当前链路按
启动 → 模型管理 → Session → Agent loop → Provider/工具 → TUI。目标 API 与待验证项
分别标注，不将完整上下文装配、审批、MCP 或持久化当作已实现。

## 调研

技术选型与外部对照，回答"这个取舍为什么是这样"。

- [`research/README.md`](research/README.md) —— 调研文档入口，按 Agent、TUI、CLI 与配置主题索引现有调研。
- [`research/agent-loop-research.md`](research/agent-loop-research.md) —— Codex 与 Claude Code 历史快照的 Agent loop 对照；ReAct、Prompt/上下文装配、Plan–Execute 工程编排与 Plan Schema 样例。
- [`research/web-search-research.md`](research/web-search-research.md) —— Web Search 服务形态、国内外候选、Codex/Claude Code 对照，以及 Vortex 的接入边界与首版建议。
- [`research/agent-evaluation-benchmarks-research.md`](research/agent-evaluation-benchmarks-research.md) —— SWE-bench 系列的行业采用与边界、其他 Agent 基准、模型/CLI 评测口径，以及 Vortex 的分阶段接入与数据管理建议。
- [`research/rust-tui-research.md`](research/rust-tui-research.md) —— TUI 技术选型，结论是 Ratatui + Crossterm + Tokio。
- [`research/tui-layout-and-input-research.md`](research/tui-layout-and-input-research.md) —— Codex/Claude Code 屏幕模式与输入交互对照，以及 Vortex slash 菜单、多行输入和粘贴方案。
- [`research/transcript-markdown-research.md`](research/transcript-markdown-research.md) —— Markdown 渲染库选型、Codex/Claude Code 实现对照，以及 Transcript 排版、滚动和流式展示的候选方案。
- [`research/cli-design-research.md`](research/cli-design-research.md) —— Claude Code 与 Codex CLI 的接口对照，`design/application/cli-surface.md` 中若干决策的依据。
- [`research/credential-handling-research.md`](research/credential-handling-research.md) —— 两者如何处理 API 密钥与 `.env`，`design/cross-cutting/credentials.md` 的依据。

  **什么时候查**：想改动某个已定的设计、觉得现有方案别扭时。先看调研里有没有已经权衡过的反例。

## 用户手册

- [`guide/getting-started.md`](guide/getting-started.md) —— 当前真实可运行的命令：构建、检查、运行产品 TUI。

  **什么时候查**：刚接触这个仓库、想跑起来看看时。

## 两条约定

**设计文档区分基线契约与后续目标，不维护独立进度表。** 当前完成到哪一步及验证结果
以仓库根目录的 `README.md` 为准；编程语言练习由独立项目维护。
描述代码接口时同步相关设计，并明确标注未实现的目标类型，避免把规划误当成现有 API。

**用户手册只写实际验证过的内容。** 尚未实现的命令属于设计，写在 `design/` 下并标明目标形态，
不要写进手册——手册描述不存在的行为，比没有手册更糟。

## 临时稿回收记录（2026-10-06）

以下本地稿的长期有效内容已合并到正式文档；旧 API、已完成迁移的逐行代码和重复内容
不继续维护。待实现项仍明确标为方案，回收不代表对应功能或测试已完成。

| 原临时稿 | 正式文档去向 |
| --- | --- |
| `20260919-01.md`、`20260919-02-test.md` | [SSE 设计](design/provider/streaming-response.md)、[回归测试方案](engineering/regression-test-plan.md) |
| `20260922-01.md`、`2026-09-22-SSE抽象问题.md` | [SSE 的跨协议边界与等价拆分](design/provider/streaming-response.md) |
| `20260927-01.md` | [Agent loop](design/runtime/agent-loop.md)、[回归测试方案](engineering/regression-test-plan.md) |
| `20261002-01.md` | [秘塔接入契约](design/tools/web-search.md)、[搜索专项回归](engineering/regression-test-plan.md) |
| `20261006-01.md` | [旧代码清理方案](engineering/code-cleanup.md) |
| `2026-09-22-tui-display-loss.md` | [终端画面恢复待验证方案](design/tui/terminal-redraw-recovery.md) |
| `codex-tui.md` | [TUI 布局与输入调研](research/tui-layout-and-input-research.md) 已包含对应观察，不重复追加 |

`rust-learn.md` 是个人学习笔记，问题尚未整理为项目规范，本次保留，不作为工程事实引用。

## 临时稿回收记录（2026-10-19）

| 原临时稿 | 正式文档去向 |
| --- | --- |
| `20261007-01.md` | [TUI 的 slash 解析、补全、菜单与退出契约](design/tui/interface.md)、[回归测试方案](engineering/regression-test-plan.md) |
| `20261014-01.md` | [安装](design/application/installation.md)、[配置](design/configuration/configuration.md)、[Session](design/runtime/session-runtime.md)、[TUI](design/tui/interface.md)、[凭据](design/cross-cutting/credentials.md) 已收录主要契约；剩余路径/配置/模型/搜索专项测试及人工验收回收到 [回归测试方案](engineering/regression-test-plan.md) |

上述两份本地原稿在核对后删除，旧阶段编译缺口、临时副本路径和被当前代码取代的
逐行迁移代码不继续维护。原稿中的待补测试仍为待补，不因回收或旧测试通过而变成
已验证能力。保留 `rust-learn.md`；新的实施稿不作为正式文档的永久依赖。

## 临时稿回收记录（2026-10-26）

| 原临时稿 | 正式文档去向 |
| --- | --- |
| `20261019-01.md` | [Provider 思考选项](design/provider/thinking-options.md)、[配置设计](design/configuration/configuration.md) 已覆盖现行装配与迁移契约；三种接入的人工验收与失败排查补入 [回归测试方案](engineering/regression-test-plan.md) |
| `20261020-01.md` | [Agent loop 与目录工具](design/tools/list-directory.md) 已覆盖句柄路径、预算、错误和取消边界；真实调用链与平台专项验收补入 [回归测试方案](engineering/regression-test-plan.md) |
| `20261023-01.md` | [Thinking 展示](design/tui/thinking-display.md)、[Provider 思考选项](design/provider/thinking-options.md)、[配置设计](design/configuration/configuration.md) 已覆盖预设、输出策略和事件链；工具往返验收顺序与失败分类补入 [回归测试方案](engineering/regression-test-plan.md) |

三份实施稿已核对并删除，不重复保存大段逐行迁移代码；早期仅支持关闭思考、目录工具
待实现等描述由当前正式契约取代。[后续方向](design/development-directions.md) 同步区分
已接入能力与待验证、待开发项。归档不新增测试通过记录，也不代表真实工具往返、
Windows 路径边界或 HTTP/SSE 分帧已完成验收。

`test-env.md` 的个人测试环境信息已迁入不入库的本地 Agent 补充说明；原文件删除后，
空的 `md.local.temp` 目录一并清理。该目录可在后续编写新实施稿时重新创建。

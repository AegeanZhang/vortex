# 文档索引

Vortex 的文档分为架构、工程规范、详细设计、调研和用户手册五类。下面写清每份是什么、什么时候查。

## 架构

- [`architecture/README.md`](architecture/README.md) —— 架构文档入口，包含系统总览、仓库组织、技术选型与术语表。

  **什么时候查**：新建 crate、决定某段代码放进哪个 crate、拿不准某处能否依赖另一个 crate 时。

## 工程规范

- [`engineering/coding-style.md`](engineering/coding-style.md) —— Rust 格式与命名约定。
- [`engineering/testing.md`](engineering/testing.md) —— 测试组织和提交前检查命令。
- [`engineering/git-workflow.md`](engineering/git-workflow.md) —— Commit、分支与 Pull Request 约定。
- [`engineering/review-checklist.md`](engineering/review-checklist.md) —— Review 时检查架构、正确性与安全性的清单。

  **什么时候查**：编写、检查或提交代码与文档时。

## 详细设计

单个子系统的对外契约，是写代码时的直接依据。

- [`design/cli-surface.md`](design/cli-surface.md) —— `vortex` 命令行接口：模式、参数、输出契约、退出码，以及各部分归属哪个落地阶段。

  **什么时候查**：实现或修改 CLI 时。参数命名与语义以此为准，不要在代码里临时决定。

- [`design/credentials.md`](design/credentials.md) —— 密钥从哪里来、`.env` 的定位、Provider 配置形状、日志脱敏规则。

  **什么时候查**：接入新 Provider、处理 API 密钥、写任何会记录请求内容的日志时。

- [`design/message-building.md`](design/message-building.md) —— 会话事件如何形成模型上下文，以及 Tool call、Token 预算和 Provider 映射规则。

  **什么时候查**：实现 Agent loop、上下文压缩、工具调用或 Provider 请求转换时。

## 调研

技术选型与外部对照，回答"这个取舍为什么是这样"。

- [`research/rust-tui-research.md`](research/rust-tui-research.md) —— TUI 技术选型，结论是 Ratatui + Crossterm + Tokio。
- [`research/cli-design-research.md`](research/cli-design-research.md) —— Claude Code 与 Codex CLI 的接口对照，`design/cli-surface.md` 中若干决策的依据。
- [`research/credential-handling-research.md`](research/credential-handling-research.md) —— 两者如何处理 API 密钥与 `.env`，`design/credentials.md` 的依据。

  **什么时候查**：想改动某个已定的设计、觉得现有方案别扭时。先看调研里有没有已经权衡过的反例。

## 用户手册

- [`guide/getting-started.md`](guide/getting-started.md) —— 当前真实可运行的命令：构建、检查、运行实验。

  **什么时候查**：刚接触这个仓库、想跑起来看看时。

## 两条约定

**文档只写设计与约定，不写进度。** 当前完成到哪一步以仓库根目录的 `README.md` 为准，
各实验的进度记录在自己的计划文档里（如 `playground/tui-demo/docs/implementation-plan.md`）。
把状态叙述写进设计文档，过期后会主动误导读者。

**用户手册只写实际验证过的内容。** 尚未实现的命令属于设计，写在 `design/` 下并标明目标形态，
不要写进手册——手册描述不存在的行为，比没有手册更糟。

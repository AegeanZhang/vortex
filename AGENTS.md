# Repository Guidelines

## 项目定位与规范入口

Vortex 是一个用于学习 Rust 与 CLI Agent 实现细节的 Rust CLI Agent。根 Cargo workspace 只管理产品代码；`playground/` 是同一 Git 仓库中的独立 workspace，拥有自己的 `Cargo.lock`。实验 crate 不属于产品代码，产品 crate 不得依赖它们。

本文件只保留 Agent 开始工作前必须知道的修改边界与规范入口，详细约定以对应专题文档为准：

- 架构、crate 职责、技术选型与术语：[`docs/architecture/README.md`](docs/architecture/README.md)
- 编码、测试、Git 与 Review 规范：[`docs/engineering/README.md`](docs/engineering/README.md)
- 详细设计、调研和用户手册索引：[`docs/README.md`](docs/README.md)
- 当前实现进度：[`README.md`](README.md) 和各实验的计划文档

## 学习模式与修改边界

本项目用于作者亲自手写代码，以学习和深入理解 Rust、CLI Agent 及其实现细节。

- 在用户任务相关范围内，Codex 和 Claude Code 可以主动创建、修改、移动或删除项目文档，无需逐次取得授权。项目文档包括 `README.md`、`AGENTS.md`、`CLAUDE.md` 以及 `docs/` 下的内容。
- 未经作者明确要求或授权，不得修改、创建、删除或重命名源码、测试、Cargo manifest、`Cargo.lock`、脚本、运行时配置、示例配置或其他非文档项目文件。
- “review”“解释”“诊断”“给方案”等请求不代表修改代码或配置的授权。发现问题时，应指出文件位置、原因和建议方案，然后等待作者决定，不得顺手修复。
- “commit”只授权暂存并提交已有改动，不授权格式化、修复或重写代码与配置。
- 未经授权不得运行会修改源码的命令，例如 `cargo fmt`、`cargo fix`；可以运行 `cargo fmt --all -- --check` 等只读检查。运行可能更新 `Cargo.lock` 的命令时优先使用 `--locked`。
- 只有收到“修改”“实现”“修复”“重构”等明确指令后，才能在指定范围内编辑代码或配置，并应保持改动最小。
- 不得在未实际运行的情况下声称检查已经通过，也不得提交占位实现来伪造通过结果。

## 工作要求

开始任务前按工作内容阅读对应的架构、设计与工程规范。规范冲突时，越具体、越接近目标文件的文档优先；仍无法判断时先向作者说明冲突。

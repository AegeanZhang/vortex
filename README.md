# Vortex

Vortex 是一个计划使用 Rust 编写的通用终端 Agent，目标体验类似 Codex CLI 和 Claude Code：理解自然语言任务，在用户控制下读取工作区、调用工具、执行命令并持续完成任务。

> 当前状态：设计与仓库准备阶段。仓库尚未包含可执行程序、Cargo workspace 或可用命令。

## 首版方向

- Linux 优先，提供交互式 TUI；同时规划 `vortex run "<task>"` 单次执行模式。
- 采用可插拔 Provider 架构，计划支持 OpenAI Responses API、OpenAI-compatible Chat Completions 和 Anthropic Messages API。
- 内置文件读取、内容搜索、补丁编辑和 Shell 工具，并通过 stdio 接入 MCP Server。
- 默认使用 `bubblewrap` 限制写入当前工作区并关闭网络；越界操作由用户按需审批。
- 使用 JSONL 事件日志保存会话，支持恢复、审计和异常退出后的继续执行。
- 分层读取工作区内的 `AGENTS.md`，为 Agent 提供项目级约束。
- 首版采用单 Agent loop，包含计划状态、取消执行和上下文自动压缩，不包含多 Agent 调度。

工具定义与 Provider 响应将归一化为内部事件模型，使 TUI、单次模式、会话记录和后续 Provider 共用同一执行核心。

## 配置与安全

普通配置计划从用户级和项目级 TOML 分层加载。API 密钥只允许通过环境变量提供，例如 `OPENAI_API_KEY` 和 `ANTHROPIC_API_KEY`，不得写入仓库或配置文件。非交互模式遇到需要审批的操作时应安全失败，除非用户显式选择相应权限策略。

## Roadmap

1. 初始化 Rust 2024 Cargo workspace，定义 Agent、Provider、Tool 和事件接口。
2. 实现模型流式响应、工具循环、内置工具及自动化测试。
3. 加入 TUI、JSONL 会话恢复、`bubblewrap` 沙箱和 stdio MCP client。
4. 完善安装、CI、发布包、可观测性与跨平台抽象。

在 Cargo 工程建立前，本仓库没有可运行的构建或测试命令。

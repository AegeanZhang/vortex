# Vortex

Vortex 是一个计划使用 Rust 编写的通用终端 Agent，目标体验类似 Codex CLI 和 Claude Code：理解自然语言任务，在用户控制下读取工作区、调用工具、执行命令并持续完成任务。

> 当前状态：设计阶段 + 技术预研。Cargo workspace 已建立，但其中只有 `playground/` 下的学习实验，产品 crate（CLI/TUI、Agent 核心、Provider、工具与沙箱）尚未开始实现。

## 首版方向

- Linux 优先，提供交互式 TUI；同时规划 `vortex exec "<task>"` 单次执行模式。
- 采用可插拔 Provider 架构，计划支持 OpenAI Responses API、OpenAI-compatible Chat Completions 和 Anthropic Messages API。
- 内置文件读取、内容搜索、补丁编辑和 Shell 工具，并通过 stdio 接入 MCP Server。
- 默认使用 `bubblewrap` 限制写入当前工作区并关闭网络；越界操作由用户按需审批。
- 使用 JSONL 事件日志保存会话，支持恢复、审计和异常退出后的继续执行。
- 分层读取工作区内的 `AGENTS.md`，为 Agent 提供项目级约束。
- 首版采用单 Agent loop，包含计划状态、取消执行和上下文自动压缩，不包含多 Agent 调度。

工具定义与 Provider 响应将归一化为内部事件模型，使 TUI、单次模式、会话记录和后续 Provider 共用同一执行核心。

## 仓库结构

```
Cargo.toml                 # Rust 2024 workspace（resolver = "3"）
README.md                  # 本文件：产品设计与仓库现状
AGENTS.md                  # 协作规范（CLAUDE.md 为其指针）
docs/                      # 文档，分类索引见 docs/README.md
├── architecture/          # 架构、仓库组织、技术选型与术语
├── design/                # 详细设计（命令行接口等）
├── engineering/           # 编码、测试、Git 与 Review 规范
├── research/              # 技术调研
└── guide/                 # 用户手册
playground/                # 学习与验证用实验，不属于产品代码
├── hello-rust/            # Rust 入门练习
└── tui-demo/              # ratatui + crossterm 终端界面实验
    └── docs/implementation-plan.md   # 分阶段方案与进度
```

`playground/` 下的 crate 只用于熟悉技术栈和验证方案，接口不做兼容承诺，产品实现开始后可能被替换或删除；各实验的进度记录在自己的计划文档里。

## 配置与安全

普通配置计划从用户级和项目级 TOML 分层加载。API 密钥只允许通过环境变量提供，例如 `OPENAI_API_KEY` 和 `ANTHROPIC_API_KEY`，不得写入仓库或配置文件。非交互模式遇到需要审批的操作时应安全失败，除非用户显式选择相应权限策略。

## 常用命令

当前 workspace 内只有实验 crate，以下命令可用于检查它们：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo run -p tui-demo          # 运行 TUI 实验，按 q 退出
```

## Roadmap

1. 初始化 Rust 2024 Cargo workspace，定义 Agent、Provider、Tool 和事件接口。（workspace 已建立，接口定义待开始）
2. 实现模型流式响应、工具循环、内置工具及自动化测试。
3. 加入 TUI、JSONL 会话恢复、`bubblewrap` 沙箱和 stdio MCP client。
4. 完善安装、CI、发布包、可观测性与跨平台抽象。

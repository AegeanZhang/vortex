# Vortex

Vortex 是一个计划使用 Rust 编写的通用终端 Agent，目标体验类似 Codex CLI 和 Claude Code：理解自然语言任务，在用户控制下读取工作区、调用工具、执行命令并持续完成任务。

> 当前状态：非流式 TUI v1 已跑通。CLI 可装配 OpenAI-compatible Provider，Core Session
> 可接受 Prompt、维护消息历史并发布事件，TUI 可编辑输入、提交请求和展示回复。工具调用、
> 真正流式输出、会话恢复、模型切换和沙箱尚未实现。

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
Cargo.toml                 # 产品 Rust 2024 workspace（resolver = "3"）
Cargo.lock                 # 产品依赖锁文件
README.md                  # 本文件：产品设计与仓库现状
AGENTS.md                  # 协作规范（CLAUDE.md 为其指针）
docs/                      # 文档，分类索引见 docs/README.md
├── architecture/          # 架构、仓库组织、技术选型与术语
├── design/                # 详细设计（命令行接口等）
├── engineering/           # 编码、测试、Git 与 Review 规范
├── research/              # 技术调研
└── guide/                 # 用户手册
crates/                    # 产品 crate
├── vortex-cli/            # 二进制入口与依赖装配
├── vortex-core/           # Session runtime、语义命令与事件
├── vortex-provider/       # OpenAI-compatible Provider 适配
├── vortex-tui/            # 交互界面、状态更新与渲染
├── vortex-tools/          # 工具层占位 crate
└── vortex-sandbox/        # 沙箱层占位 crate
playground/                # 独立实验 workspace，不属于产品代码
├── Cargo.toml             # 实验成员与共享依赖
├── Cargo.lock             # 实验依赖锁文件
├── hello-rust/            # Rust 入门练习
└── tui-demo/              # ratatui + crossterm 终端界面实验
    └── docs/implementation-plan.md   # 分阶段方案与进度
```

`playground/` 下的 crate 只用于熟悉技术栈和验证方案，接口不做兼容承诺，产品实现开始后可能被替换或删除；各实验的进度记录在自己的计划文档里。

产品和实验保留在同一 Git 仓库，但分别维护锁文件和构建目录。根目录的 `--workspace`
只覆盖产品 crate；运行或检查实验时使用 `--manifest-path playground/Cargo.toml`。

## 配置与安全

普通配置计划从用户级和项目级 TOML 分层加载。API 密钥只允许通过环境变量提供，例如 `OPENAI_API_KEY` 和 `ANTHROPIC_API_KEY`，不得写入仓库或配置文件。非交互模式遇到需要审批的操作时应安全失败，除非用户显式选择相应权限策略。

## 常用命令

以下命令检查产品 workspace，并分别运行产品 TUI 和实验：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
DEEPSEEK_API_KEY=<key> cargo run -p vortex-cli  # 运行产品 TUI，Ctrl+C 退出
cargo run --manifest-path playground/Cargo.toml -p tui-demo # 运行 TUI 实验，按 q 退出
```

产品不会主动读取 `.env`；本地开发脚本如需加载它，应先把变量导出到进程环境。当前 Provider
地址和模型仍由 CLI 固定装配，尚未接入 TOML 配置。

## Roadmap

1. 初始化 Rust 2024 Cargo workspace，完成 Provider 抽象、Session 事件接口和非流式 TUI v1。（已完成）
2. 补齐当前 Core、Provider 与 TUI 的单元测试，再实现模型流式响应和取消。
3. 实现工具循环、内置工具、JSONL 会话恢复、`bubblewrap` 沙箱和 stdio MCP client。
4. 完善安装、CI、发布包、可观测性与跨平台抽象。

# Vortex

Vortex 是一个使用 Rust 编写、以学习实现细节为目的的终端 Agent，目标体验类似 Codex CLI 和 Claude Code：理解自然语言任务，在用户控制下读取工作区、调用工具、执行命令并持续完成任务。

> 当前状态：TUI 已接入文本 SSE 和 Mock 工具闭环。一个 Turn 可经历多次模型请求与
> 工具执行，Provider 组装完整 ToolCall，Core 回填工具结果，TUI 流式展示 Assistant Markdown。
> `web_search` 仅返回固定 Mock 数据，不访问搜索服务，也不能查询真实天气或新闻。

Transcript 已接入 Assistant Markdown、键盘翻页与鼠标滚轮；相关回归测试仍需补充，
滑块视觉断裂问题暂缓处理。当前处于文本 SSE 阶段 A，CLI 显式关闭千问思考及思考历史；
阶段 B/C 的 Thinking 契约见 [`docs/design/streaming-response.md`](docs/design/streaming-response.md)。
Shutdown 可通知 Session 中止活跃流，但尚无保留会话的 `CancelTurn`；exec 未迁移到流式链路。

工具版 v1 每个模型步骤只接受一个调用，一个 Turn 最多执行 4 次工具调用；第五次会失败。
工具错误作为 `is_error = true` 的结果回填给模型，空白或重复调用 ID 则直接使 Turn 失败。
TUI 仅用状态栏展示工具开始/完成，不显示工具卡片或来源引用。设计见
[`Agent loop 与 Mock 工具`](docs/design/agent-loop.md)。Thinking、真实搜索、通用 Schema
校验、工具超时/输出限制、会话恢复、模型切换和沙箱均未实现。

验证边界（2026-10-01）：产品测试共 15 个并通过（含 1 个沙箱占位测试），release 构建通过。
Core 的 7 个测试覆盖无工具文本流的增量顺序、
历史和退出背压；Provider、工具注册表及新 Agent loop 尚无专项测试，不能据此认定工具
闭环或 HTTP/SSE 已充分验证。真实模型工具调用、流式表现与终端恢复仍需人工验收。
当前格式检查通过；严格 Clippy 仍被 TUI 的 `Quit` / `SlashCommand` 未使用警告阻挡。

## 首版方向

- Linux 优先，提供交互式 TUI；同时规划 `vortex exec "<task>"` 单次执行模式。
- 采用可插拔 Provider 架构，计划支持 OpenAI Responses API、OpenAI-compatible Chat Completions 和 Anthropic Messages API。
- 内置文件读取、内容搜索、补丁编辑和 Shell 工具，并通过 stdio 接入 MCP Server。
- 默认使用 `bubblewrap` 限制写入当前工作区并关闭网络；越界操作由用户按需审批。
- 使用 JSONL 事件日志保存会话，支持恢复、审计和异常退出后的继续执行。
- 分层读取工作区内的 `AGENTS.md`，为 Agent 提供项目级约束。
- 首版采用单 Agent loop，包含计划状态、取消执行和上下文自动压缩，不包含多 Agent 调度。

TUI 链路已使用协议无关的工具定义和模型事件；后续单次模式、会话记录与多 Provider
应复用这一执行核心，而不是复制工具循环。

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
├── vortex-core/           # Session、Agent loop、语义历史与 Provider/Tool 接口
├── vortex-provider/       # OpenAI-compatible Provider 适配
├── vortex-tui/            # 交互界面、状态更新与渲染
├── vortex-tools/          # MockWebSearch 实现，不发起真实搜索
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
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
export VORTEX_HOME="$PWD/.vortex"          # 必须为绝对路径，日志写入 logs/vortex.log
export QWEN_API_KEY='<your-api-key>'        # 仅在本地设置，不提交真实值
cargo run -p vortex-cli --locked           # 产品 TUI，输入 /exit 后 Enter 退出
cargo run --manifest-path playground/Cargo.toml -p tui-demo # 运行 TUI 实验，按 q 退出
```

产品不会主动读取 `.env`；本地开发脚本如需加载它，应先把变量导出到进程环境。当前 Provider
地址和模型仍由 CLI 固定装配，尚未接入 TOML 配置。

日志以 JSON 行追加到 `$VORTEX_HOME/logs/vortex.log`，默认级别为 `info`；未使用 `-v`
时可用 `RUST_LOG` 调整，单个 `-v` 强制 `debug`。注意：TUI 仍有记录输入原文的 TRACE，
全局 `RUST_LOG=trace` 或 `vortex_tui=trace` 会使 Prompt 落盘；日志脱敏尚未完整验证。

## Roadmap

1. 初始化 Rust 2024 Cargo workspace，完成 Provider 抽象、Session 事件接口和非流式 TUI v1。（已完成）
2. 补齐文本 SSE、Mock 工具循环、Provider 转换和 TUI 工具状态测试，完成真实终端验收。
3. 在现有工具循环上接入真实搜索及其他内置工具，补参数校验、超时和输出预算；独立推进 Thinking、重绘节流和 Turn 取消。
4. 实现 JSONL 会话恢复、`bubblewrap` 沙箱和 stdio MCP client，并将 exec 迁移到共享 Session。
5. 完善安装、CI、发布包、可观测性与跨平台抽象。

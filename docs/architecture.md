# Vortex 架构设计

本文档描述产品代码的**目标架构**：crate 如何划分、依赖朝哪个方向走、按什么顺序落地。它只记录设计决策，不记录进度——当前实现到哪一步见 `README.md`，`playground/` 下各实验的进度见其各自的计划文档。

## crate 划分

```text
vortex/
├── Cargo.toml                  # workspace + [workspace.dependencies] 统一版本
├── rust-toolchain.toml         # 固定稳定版，本地与 CI 一致
├── docs/
│   ├── architecture.md         # 本文件
│   └── research/               # 技术调研
├── crates/
│   ├── vortex-core/            # 抽象与执行核心，不依赖任何兄弟 crate
│   │   └── src/
│   │       ├── event.rs        # 内部事件模型：UI、会话、工具共用的唯一真相
│   │       ├── agent.rs        # 单 Agent loop：turn、取消、上下文压缩
│   │       ├── conversation.rs # 消息历史与压缩策略
│   │       ├── session.rs      # JSONL 事件日志与恢复
│   │       ├── config.rs       # 用户级 / 项目级 TOML 分层加载
│   │       ├── instructions.rs # 分层读取 AGENTS.md
│   │       ├── provider.rs     # trait Provider，只有抽象
│   │       ├── tool.rs         # trait Tool 与 JSON Schema 描述
│   │       ├── approval.rs     # 权限策略：所有副作用的唯一入口
│   │       └── error.rs
│   ├── vortex-provider/        # OpenAI Responses、OpenAI-compatible、Anthropic
│   │   └── src/{openai_responses,openai_chat,anthropic,sse}.rs
│   ├── vortex-tools/           # 内置工具与 stdio MCP client
│   │   └── src/{fs_read,search,apply_patch,shell,mcp/}.rs
│   ├── vortex-sandbox/         # bubblewrap 封装、可写边界与网络开关
│   ├── vortex-tui/             # app / ui / event / terminal 四层
│   └── vortex-cli/             # [[bin]] name = "vortex"
│       └── src/{main,args,run,wiring}.rs
└── playground/                 # 学习与验证用实验，不属于产品代码
```

## 依赖方向

```text
vortex-cli ──┬─> vortex-tui ─────┐
             ├─> vortex-provider ├─> vortex-core   （trait 与事件模型，零具体实现）
             ├─> vortex-tools ───┤
             └─> vortex-sandbox ─┘

vortex-tools ──> vortex-sandbox        # Shell 与文件写入必须过沙箱
```

三条硬规则：

1. **`vortex-core` 不依赖任何兄弟 crate**，也不依赖 `ratatui`、`reqwest` 这类具体实现库。它只定义事件、trait、配置和错误。
2. **只有 `vortex-cli` 认识全部具体实现。** provider、tools、sandbox 和 UI 在 `wiring.rs` 中组装；更换 Provider 或更换 UI 只应改动这一个文件。
3. **TUI 与单次模式共用同一份事件流。** `vortex run "<task>"` 是把 `event.rs` 的事件渲染成纯文本输出，而不是另写一套执行循环。这是"共用同一执行核心"能否成立的关键，任何绕过事件模型的捷径都会让两种模式行为漂移。

## 各 crate 职责

| crate | 负责 | 明确不负责 |
| --- | --- | --- |
| `vortex-core` | 事件模型、Agent loop、会话持久化、配置、权限策略、trait 定义 | HTTP、终端、进程管理 |
| `vortex-provider` | 三种 Provider 的请求构造与流式响应解析，归一化为内部事件 | 决定何时调用模型 |
| `vortex-tools` | 内置工具实现与 JSON Schema、MCP server 接入 | 决定工具是否被批准 |
| `vortex-sandbox` | 进程隔离、可写边界、网络开关 | 判断路径是否越界之外的业务逻辑 |
| `vortex-tui` | 状态机与纯渲染 | 直接发起模型或工具调用 |
| `vortex-cli` | 参数解析、单次模式、依赖组装、面向用户的中文提示 | 承载可复用逻辑 |

## 落地顺序

一次建齐六个 crate 没有收益，只有 `Cargo.toml` 的维护成本。分三步推进，每步都有明确的拆分触发条件：

| 阶段 | 新建 crate | 暂时作为模块留在哪 |
| --- | --- | --- |
| 1. 打通闭环 | `vortex-core`、`vortex-provider`、`vortex-cli` | 工具放在 core 内，先只做只读工具 |
| 2. 工具与沙箱 | 拆出 `vortex-tools` | sandbox 作 `vortex-tools::sandbox` 模块 |
| 3. 交互 | 拆出 `vortex-tui`、`vortex-sandbox` | — |

拆分触发条件：

- `vortex-sandbox` 在引入 Linux-only 依赖时拆出，避免整个 `vortex-tools` 变得不可跨平台编译。
- `vortex-tui` 在需要给状态机写独立测试时拆出。

## 工程约定

- workspace 设 `default-members = ["crates/*"]`，使 `cargo build` 不连带编译 `playground/`，而 `cargo test --workspace` 仍覆盖它们。
- crate 目录与 package 名统一为 `vortex-*`（kebab-case），代码中对应 `vortex_core::` 等；只有 `vortex-cli` 产出名为 `vortex` 的二进制。
- 依赖版本集中在 `[workspace.dependencies]`，各 crate 用 `dep.workspace = true` 引用。

## 待定项

- **MCP client 的归属**：暂定放在 `vortex-tools`，理由是它对外表现就是一批工具；若 server 生命周期管理复杂到影响工具实现的可读性，再拆为独立 crate。
- **是否需要 `vortex-protocol`**：暂不设独立的类型 crate。若 `vortex-core` 因引入运行时依赖而变重、导致纯类型使用方被迫编译大量依赖，再把事件与协议类型拆出去。
- **跨平台沙箱抽象**：首版仅 Linux + `bubblewrap`。抽象层在有第二个真实后端之前不预先设计。

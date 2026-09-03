# 架构总览

Vortex 将 Agent 执行核心、Provider、工具、沙箱、终端 UI 与命令行入口拆分为职责清晰的 crate。共享接口不得依赖具体终端 UI 或模型服务，产品二进制名称统一为 `vortex`。

## 依赖方向

```text
vortex-cli ──┬─> vortex-tui ─────┐
             ├─> vortex-provider ├─> vortex-core   （trait 与事件模型）
             ├─> vortex-tools ───┤
             └─> vortex-sandbox ─┘

vortex-tools ──> vortex-sandbox
```

三条硬规则：

1. `vortex-core` 不依赖任何兄弟 crate，也不依赖 Ratatui、Reqwest 等具体实现库；它只定义事件、trait、配置和结构化错误。
2. 只有 `vortex-cli` 认识全部具体实现，并在组合层完成装配。更换 Provider 或 UI 不应迫使核心逻辑改变。
3. TUI 与单次模式共用同一事件流。`vortex exec "<task>"` 只是把核心事件渲染成纯文本输出，不得另写一套执行循环。

## crate 职责

| crate | 负责 | 明确不负责 |
| --- | --- | --- |
| `vortex-core` | 事件模型、Agent loop、会话持久化、配置、权限策略和 trait | HTTP、终端、进程管理 |
| `vortex-provider` | 构造 Provider 请求、解析流式响应并转换为内部事件 | 决定何时调用模型 |
| `vortex-tools` | 内置工具、JSON Schema 与 stdio MCP client | 决定工具是否获批 |
| `vortex-sandbox` | 进程隔离、可写边界和网络开关 | 工具业务逻辑 |
| `vortex-tui` | 状态机与纯渲染 | 直接调用 Provider 或工具 |
| `vortex-cli` | 参数解析、单次模式、依赖装配和面向用户的中文错误提示 | 承载可复用业务逻辑 |

库层返回保留上下文的结构化错误，CLI 层负责将其转换为面向用户的中文提示和稳定退出码。

## 横切约束

- Provider、Tool、会话存储和 UI 通过内部事件模型解耦。
- 会话事件、内部语义消息和 Provider 请求保持分层，消息构建规则见 [`../design/message-building.md`](../design/message-building.md)。
- 工具参数使用 JSON Schema 描述；文件修改、Shell 和 MCP 工具经过同一权限策略。
- Linux 默认以当前工作区为可写边界，并通过 Bubblewrap 禁止网络；不得绕过审批执行越界操作。
- API 密钥只从进程环境读取，日志和错误信息必须脱敏。详细规则见 [`../design/credentials.md`](../design/credentials.md)。
- 项目指令按目录层级读取 `AGENTS.md`，越接近目标文件的规则优先。

## 演进顺序

| 阶段 | 新建 crate | 暂时作为模块保留的位置 |
| --- | --- | --- |
| 1. 打通闭环 | `vortex-core`、`vortex-provider`、`vortex-cli` | 工具先放在 core，只实现只读能力 |
| 2. 工具与沙箱 | 拆出 `vortex-tools` | Sandbox 暂作 `vortex-tools::sandbox` 模块 |
| 3. 交互 | 拆出 `vortex-tui`、`vortex-sandbox` | 无 |

`vortex-sandbox` 在引入 Linux-only 依赖时拆出，避免整个工具 crate 失去跨平台编译能力；`vortex-tui` 在状态机需要独立测试时拆出。

## 待定项

- MCP client 暂归 `vortex-tools`；若生命周期管理明显干扰工具实现，再拆为独立 crate。
- 暂不创建 `vortex-protocol`；只有核心运行时依赖让纯类型使用方负担过重时才拆分。
- 首版只实现 Linux + Bubblewrap；在出现第二个真实后端前不预设跨平台沙箱抽象。

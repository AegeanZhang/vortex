# 架构总览

Vortex 将 Agent 执行核心、Provider、工具、沙箱、终端 UI 与命令行入口拆分为职责清晰的 crate。共享接口不得依赖具体终端 UI 或模型服务，产品二进制名称统一为 `vortex`。

## 依赖方向

以下是目标依赖边界，不表示所有能力已接入；沙箱/MCP、持久化和 exec 共享 Session
均需按设计演进，当前实现与验证进度见根 README。

```text
vortex-cli ──┬─> vortex-tui ─────┐
             ├─> vortex-provider ├─> vortex-core   （trait 与事件模型）
             ├─> vortex-tools ───┤
             └─> vortex-sandbox ─┘

vortex-tools ──> vortex-sandbox
```

三条硬规则：

1. `vortex-core` 不依赖任何兄弟 crate，也不依赖 Ratatui、Reqwest 等具体实现库；它承载协议无关的 Session/Agent loop、语义类型、trait、配置和结构化错误。
2. 只有 `vortex-cli` 认识全部具体实现，并在组合层完成装配。更换 Provider 或 UI 不应迫使核心逻辑改变。
3. 目标是 TUI 与单次模式共用同一事件流：exec 应将核心事件渲染成纯文本，不另写执行循环。现有 exec 是尚待迁移的旧 HTTP 路径，不代表目标边界已满足。

## crate 职责

| crate | 负责 | 明确不负责 |
| --- | --- | --- |
| `vortex-core` | 事件模型、Agent loop、会话持久化、配置、权限策略和 trait | HTTP、终端、进程管理 |
| `vortex-provider` | 构造 Provider 请求、解析流式响应并转换为内部事件 | 决定何时调用模型 |
| `vortex-tools` | 内置工具、JSON Schema 与 stdio MCP client | 决定工具是否获批 |
| `vortex-sandbox` | 进程隔离、可写边界和网络开关 | 工具业务逻辑 |
| `vortex-tui` | 状态机与纯渲染 | 直接调用 Provider 或工具 |
| `vortex-cli` | 参数解析、单次模式、依赖装配和面向用户的中文错误提示 | 承载可复用业务逻辑 |

Core 的 Tool trait/ToolRegistry 不依赖具体工具；MetasoWebSearch 与 MockWebSearch 位于
vortex-tools，由 CLI 选择、注册并传给 Session，不同时注册同名工具。一个 Turn 的
模型/工具交替执行位于 Core 的 run_turn，
Provider 只转换协议，TUI 只消费进度。接口见 [Agent loop](../design/agent-loop.md)。

安装、HOME 和日志/配置路径由 CLI 统一解析，Core config 接收明确文件路径并负责加载、来源记录、合并与校验；不自行推导安装位置或系统用户目录。用户 HOME 统一为用户主目录下 `.vortex`，项目配置仅检查启动 cwd，不自动查找 Git 根；首版不增加系统配置文件层。以上为待实现契约，见 [安装设计](../design/installation.md) 与 [配置设计](../design/configuration.md)。

库层返回保留上下文的结构化错误，CLI 层负责将其转换为面向用户的中文提示和稳定退出码。

## 横切约束

- Provider、Tool、会话存储和 UI 通过内部事件模型解耦。
- 会话事件、内部语义消息和 Provider 请求保持分层，消息构建规则见 [`../design/message-building.md`](../design/message-building.md)。
- TUI 只通过 Session snapshot、Agent command 和 Core event 与运行时交互，接口契约见 [`../design/tui-interface.md`](../design/tui-interface.md)。
- 工具参数使用 JSON Schema 描述；文件修改、Shell 和 MCP 工具经过同一权限策略。
- Linux 默认以当前工作区为可写边界，并通过 Bubblewrap 禁止网络；不得绕过审批执行越界操作。
- API 密钥只从进程环境读取，日志和错误信息必须脱敏。详细规则见 [`../design/credentials.md`](../design/credentials.md)。
- 项目指令按目录层级读取 `AGENTS.md`，越接近目标文件的规则优先。

## 演进边界

crate 已按职责分开，是否完成能力以根 README 为准。后续按以下边界演进，不以创建
空 crate 作为完成标志：

| 能力 | 所在 crate | 前置约束 |
| --- | --- | --- |
| 执行核心 | vortex-core | 不依赖厂商或终端类型，先用受控 fake 验证 |
| 具体工具 | vortex-tools | 复用 Tool trait；真实搜索与文件访问先补参数/超时/输出边界 |
| 副作用与隔离 | vortex-sandbox 与 Core 权限策略 | 审批与隔离先于补丁/Shell 启用 |
| 界面与单次入口 | vortex-tui、vortex-cli | 共享 Session 契约，不复制工具执行逻辑 |

Linux-only 依赖应留在沙箱实现层，避免整个工具 crate 失去跨平台编译能力；TUI 的
状态机与 widget 独立测试，不把终端控件传播到 Core。

## 待定项

- MCP client 暂归 `vortex-tools`；若生命周期管理明显干扰工具实现，再拆为独立 crate。
- 暂不创建 `vortex-protocol`；只有核心运行时依赖让纯类型使用方负担过重时才拆分。
- 首版只实现 Linux + Bubblewrap；在出现第二个真实后端前不预设跨平台沙箱抽象。

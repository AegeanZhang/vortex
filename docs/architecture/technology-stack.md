# 关键技术选型

本文件汇总已经确定、会约束实现的技术选择。候选方案比较和外部项目对照保留在 [`../research/`](../research/)；若将来改变选型，应先更新相应设计决策和本文件。

| 领域 | 当前选择 | 使用边界 |
| --- | --- | --- |
| 语言与构建 | 稳定版 Rust、2024 edition、Cargo workspace | 本地与 CI 使用同一稳定工具链 |
| 异步运行时 | Tokio | 异步代码不得阻塞 executor；阻塞操作需要隔离到专用线程或进程 |
| 配置格式 | TOML | 用户级与项目级分层加载；密钥值不进入配置文件 |
| TUI | Ratatui + Crossterm | Ratatui 负责布局渲染，Crossterm 负责终端与事件；Tokio 统一事件循环 |
| Transcript Markdown | tui-markdown | 仅 vortex-tui 使用，首版关闭默认 features，Assistant 正文按消息转换，User/Error 保持纯文本 |
| HTTP | Reqwest | 只位于 Provider 实现层，不进入 `vortex-core` 公共接口 |
| SSE | eventsource-stream + async-stream | Provider 内分帧并构造惰性模型流，Core 只消费语义事件 |
| 会话格式 | JSONL 事件日志 | 便于追加、审计与异常退出恢复 |
| 工具协议 | JSON Schema、stdio MCP | 所有带副作用的调用统一经过权限策略 |
| Linux 沙箱 | Bubblewrap | 默认限制写入当前工作区并禁止网络 |

TUI 选型依据见 [`../research/rust-tui-research.md`](../research/rust-tui-research.md)，CLI 和凭据相关决策分别见 [`../design/cli-surface.md`](../design/cli-surface.md) 与 [`../design/credentials.md`](../design/credentials.md)。

## Markdown 依赖与演进边界

选型与取舍见 [`ADR 0001`](decisions/0001-transcript-markdown-rendering.md)：当前优先搭建可用展示，
为工具、记忆等核心能力保留开发精力；通过 TUI 内部薄适配层保留后续优化和替换空间。

在 Workspace 声明 `tui-markdown = { version = "0.3.9", default-features = false }`，
由 `vortex-tui` 通过 `workspace = true` 引用；版本最终由根目录 `Cargo.lock` 固定。
首版不直接增加 `pulldown-cmark`、`syntect` 或 `ansi-to-tui` 依赖。若需要代码语法高亮，
在 TUI 的依赖声明中按需开启 `highlight-code`，并重新验证性能与依赖树。

升级时检查与现有 Ratatui 的类型兼容性；原始消息、滚动状态和 Session 接口不因 Markdown
渲染改变。实际实现进度与验证结果以代码、测试及根 README 为准。

## 流式输出依赖与边界

[`流式响应设计`](../design/streaming-response.md) 使用 `eventsource-stream` 解析 SSE、
`async-stream` 构造惰性事件流、`serde_json` 解析事件载荷。Workspace 声明前两者的版本
要求分别为 `0.2.3` 和 `0.3.6`，Provider 通过 `workspace = true` 引用；实际版本由根
`Cargo.lock` 固定。Reqwest 启用 `json` 和 `stream` features。

Core 使用 `futures-util` 的通用 Stream 接口，不依赖 HTTP/SSE 类型。Tokio `time` feature
由需要网络超时、测试超时或绘制定时器的 crate 按需启用；启用 feature 不代表 TUI 已实现
定时刷新。不得用网络 chunk 边界代替 SSE 分帧，也不默认引入自动重连。

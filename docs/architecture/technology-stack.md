# 关键技术选型

本文件汇总已经确定、会约束实现的技术选择。候选方案比较和外部项目对照保留在 [`../research/`](../research/)；若将来改变选型，应先更新相应设计决策和本文件。

| 领域 | 当前选择 | 使用边界 |
| --- | --- | --- |
| 语言与构建 | 稳定版 Rust、2024 edition、Cargo workspace | 本地与 CI 使用同一稳定工具链 |
| 异步运行时 | Tokio | 异步代码不得阻塞 executor；阻塞操作需要隔离到专用线程或进程 |
| 配置格式 | TOML | 用户级与项目级分层加载；密钥值不进入配置文件 |
| TUI | Ratatui + Crossterm | Ratatui 负责布局渲染，Crossterm 负责终端与事件；Tokio 统一事件循环 |
| HTTP | Reqwest | 只位于 Provider 实现层，不进入 `vortex-core` 公共接口 |
| 会话格式 | JSONL 事件日志 | 便于追加、审计与异常退出恢复 |
| 工具协议 | JSON Schema、stdio MCP | 所有带副作用的调用统一经过权限策略 |
| Linux 沙箱 | Bubblewrap | 默认限制写入当前工作区并禁止网络 |

TUI 选型依据见 [`../research/rust-tui-research.md`](../research/rust-tui-research.md)，CLI 和凭据相关决策分别见 [`../design/cli-surface.md`](../design/cli-surface.md) 与 [`../design/credentials.md`](../design/credentials.md)。

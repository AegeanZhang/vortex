# 详细设计文档

本目录按逻辑职责组织接口与行为契约，对齐 [架构模块](../architecture/logical-modules.md)。
目录不是 crate 列表：运行时目录聚合模型管理、Session 与 Turn，凭据属于跨模块约束。
架构解释整体协作；本目录说明具体接口、不变量、状态、错误和验收；实际结果见根
[README](../../README.md)，逐行手写步骤留在本地实施稿。

## 模块导航

| 架构职责 | 主文档 | 内容边界 |
| --- | --- | --- |
| 应用入口与装配 | [启动](application/startup.md)、[安装路径](application/installation.md)、[CLI](application/cli-surface.md) | 装配顺序、目录生命周期、当前参数及目标接口 |
| 配置管理 | [配置](configuration/configuration.md) | TOML 字段、合并、来源、白名单和迁移 |
| 模型管理 | [模型目录与切换](runtime/model-management.md) | 工厂、当前绑定、能力校验与确认/拒绝 |
| Session 调度 | [Session](runtime/session-runtime.md) | 公共连接、actor、权威历史、通道与生命周期 |
| Turn 执行与历史构建 | [Agent loop](runtime/agent-loop.md)、[消息构建](runtime/message-building.md) | 单轮循环、ID/预算、局部历史与未来上下文装配 |
| 模型协议适配 | [模型流与 SSE](provider/streaming-response.md)、[思考选项](provider/thinking-options.md) | wire 映射、传输、Decoder、厂商字段及能力/历史契约 |
| 工具执行与资源访问 | [注册表](tools/tool-registry.md)、[目录列表](tools/list-directory.md)、[网页搜索](tools/web-search.md) | 公共调用/结果、具体参数及资源边界 |
| 终端交互与展示 | [接口](tui/interface.md)、[Thinking](tui/thinking-display.md)、[滚动](tui/transcript-scrolling.md)、[画面恢复](tui/terminal-redraw-recovery.md) | 状态投影、输入、绘制与待验证终端问题 |
| 横切约束 | [凭据](cross-cutting/credentials.md) | 环境 key、.env 定位、按需读取与脱敏 |
| 功能候选 | [开发方向](development-directions.md) | 依赖与候选功能，不作为现状或固定排期 |

## 推荐阅读顺序

理解现有执行链：启动 → 模型管理 → Session → Agent loop → 模型流/SSE → 工具与 TUI。
配置与凭据按需穿插；准备上下文能力时再读消息构建的目标章节。Thinking 先读
Provider 的请求/消费策略，再读 TUI 的事件投影。绘制节流、完整思考历史、恢复与
终端异常修复仍按相应章节标注为目标或待验证，文档存在不等于功能完成。

## 主契约与维护规则

- 配置字段与完整 TOML 只在配置文档维护；能力转换/预设归 Provider，模型绑定归运行时。
- Session 维护权威历史、终态顺序和背压；Agent loop 维护执行预算与 ID；工具各管资源边界。
- Provider 不调度模型/工具，TUI 不解析 SSE；展示步骤、滚动与节流由 TUI 文档维护。
- 验收要求留在各设计，具体方法集中于 [回归测试方案](../engineering/regression-test-plan.md)，
  测试通过记录只在根 README 维护。不改写历史记录为本次已验证。
- 新功能及接口变更更新其主文档，通过链接引用，避免复制整段规则；重构建议见
  [架构演进](../architecture/evolution.md)，不在这里再维护一套拆分排期。

## 本次迁移与内容去向（2026-10-26）

下表记录原平铺文档的去向，不保留第二份正文。配置示例注释仍引用旧 credentials.md，
因此仅此路径保留跳转页；本轮不改源码、配置或本地临时文件。

| 原文件名 | 当前主文档与拆分去向 |
| --- | --- |
| `installation.md` | [application/installation.md](application/installation.md) |
| `cli-surface.md` | [application/cli-surface.md](application/cli-surface.md) |
| `configuration.md` | [configuration/configuration.md](configuration/configuration.md) |
| `session-runtime.md` | [runtime/session-runtime.md](runtime/session-runtime.md)、[runtime/model-management.md](runtime/model-management.md)、[application/startup.md](application/startup.md) |
| `agent-loop.md` | [runtime/agent-loop.md](runtime/agent-loop.md)、[tools/tool-registry.md](tools/tool-registry.md)、[tools/list-directory.md](tools/list-directory.md)、[tools/web-search.md](tools/web-search.md)、[provider/streaming-response.md](provider/streaming-response.md) |
| `message-building.md` | [runtime/message-building.md](runtime/message-building.md) |
| `streaming-response.md` | [provider/streaming-response.md](provider/streaming-response.md)、[runtime/session-runtime.md](runtime/session-runtime.md)、[runtime/message-building.md](runtime/message-building.md)、[tui/thinking-display.md](tui/thinking-display.md) |
| `provider-thinking-options.md` | [provider/thinking-options.md](provider/thinking-options.md)、[configuration/configuration.md](configuration/configuration.md) |
| `tui-interface.md` | [tui/interface.md](tui/interface.md) |
| `thinking-display.md` | [tui/thinking-display.md](tui/thinking-display.md) |
| `transcript-scrolling.md` | [tui/transcript-scrolling.md](tui/transcript-scrolling.md) |
| `terminal-redraw-recovery.md` | [tui/terminal-redraw-recovery.md](tui/terminal-redraw-recovery.md) |
| `credentials.md` | [cross-cutting/credentials.md](cross-cutting/credentials.md)，完整 TOML 示例集中于 [configuration/configuration.md](configuration/configuration.md) |
| `development-directions.md` | [development-directions.md](development-directions.md)，保留功能候选导航 |

# 后续能力与设计依赖

本文回收本地头脑风暴中的候选方向，不是已实现功能清单，也不是固定排期。实现进度以
根目录 README 为准；每项动工前补充具体设计与验收范围。

## 近期闭环

| 方向 | 依赖与边界 | 设计入口 |
| --- | --- | --- |
| 输入与退出 | exit/model 已接入；后续补 Paste 只编辑、动态多行、普通 Prompt 提交确认/草稿恢复和更多命令 | [TUI 接口](tui/interface.md#slash-命令与菜单)、[输入调研](../research/tui-layout-and-input-research.md) |
| Transcript 稳定性 | 鼠标/键盘同一滚动模型；resize、Unicode、极小窗口与终端恢复测试 | [滚动设计](tui/transcript-scrolling.md) |
| 终端画面恢复 | Resize 已接入普通重绘；Kitty 合盖后画面丢失待定位，强制重绘仍是候选 | [画面恢复](tui/terminal-redraw-recovery.md) |
| 流式输出与 Thinking | 展示-only 思考、步骤事件与离线回归已接入；先补真实工具往返验收，再做折叠、缓存和其他接入的思考历史适配 | [Thinking 展示](tui/thinking-display.md)、[流式响应](provider/streaming-response.md) |
| 工具闭环验证 | 秘塔搜索与目录工具已接入，已有部分 FakeProvider/Tool 回归；继续补调用 ID、异常、HTTP/SSE 分帧及真实工具往返验收 | [Agent loop](runtime/agent-loop.md)、[回归测试方案](../engineering/regression-test-plan.md) |
| Turn 取消 | 区分取消、失败和整个 Session 退出；候选为 Running 时 Ctrl+C 取消、Idle 时退出 | [Session](runtime/session-runtime.md) |

补充测试与实现可以交错，复用已有可控 FakeProvider 与 FakeTool。按缺口继续覆盖多轮
历史、并发提交拒绝、失败后再次提交、关闭活跃请求、HTTP/响应异常和密钥脱敏。
自动化测试不调用真实模型，真实服务与终端行为另作人工验收。

## 从聊天走向 Agent

1. 配置与模型选择的验证和演进：分层 TOML、Provider/模型 profile、`-m/-c`、`/model`
   首版已接入，配置保留在 Core。当前切换仅本会话，以 Core 确认为准，运行中明确拒绝，
   不延后执行或自动写盘；先补专项回归和人工验收，再讨论目录过滤、显式保存或新协议。
2. 文本 ContextAssembler：分离指令与会话，加载分层项目指令，确定顺序与预算接口。
   参见 [消息构建](runtime/message-building.md)。请求日志只记录必要元数据，正文诊断另行授权。
3. 从搜索与目录工具继续扩展：秘塔已有超时和输出限制，目录工具已有句柄路径边界与
   扫描/输出预算，但已开始的阻塞 I/O 没有硬超时。下一步设计有界 `read_file`，复用
   多调用顺序执行与结果回填循环；按工具额度已接入，通用校验、并发调度及模型步骤/Token/费用
   预算另行设计，见 [Agent loop](runtime/agent-loop.md)。
   文件访问仍先检查路径与数据外发边界；出现第二个文件工具时再提炼共享规则，不能
   因目录工具可用就推定任意文件读写已获授权。
4. 有副作用工具：补丁与 Shell **先建立权限、审批和隔离再启用**；需要超时、取消、输出
   截断、网络边界与非交互安全失败。沙箱不是上线后的附加功能。
5. 会话与产品化：JSONL 事件、恢复与摘要、exec 共用 Session 流、稳定退出码，随后完善
   CI、安装与发布。参见 [CLI 契约](application/cli-surface.md) 和 [架构](../architecture/overview.md)。

这些方向存在依赖，但不要求所有 TUI 优化完成后才做工具。新增能力不得反向依赖 TUI，
也不因提前创建了 crate 或枚举就宣称功能已完成。

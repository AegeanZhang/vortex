# 后续能力与设计依赖

本文回收本地头脑风暴中的候选方向，不是已实现功能清单，也不是固定排期。实现进度以
根目录 README 为准；每项动工前补充具体设计与验收范围。

## 近期闭环

| 方向 | 依赖与边界 | 设计入口 |
| --- | --- | --- |
| 输入与退出 | 正式 slash 解析、菜单、Paste 只编辑；不要把临时 trim 匹配当成最终规则 | [输入调研](../research/tui-layout-and-input-research.md) |
| Transcript 稳定性 | 鼠标/键盘同一滚动模型；resize、Unicode、极小窗口与终端恢复测试 | [滚动设计](transcript-scrolling.md) |
| 流式输出与 Thinking | 增量流、终态顺序、失败历史策略，再做折叠与缓存 | [流式响应](streaming-response.md) |
| Turn 取消 | 区分取消、失败和整个 Session 退出；候选为 Running 时 Ctrl+C 取消、Idle 时退出 | [Session](session-runtime.md) |

补充测试与实现可以交错，但必须先建立可控 fake Provider。重点覆盖多轮历史、并发提交
拒绝、失败重试、关闭活跃请求、HTTP/响应异常和密钥脱敏。不要把真实模型请求当作测试。

## 从聊天走向 Agent

1. 配置与模型选择：用户/项目级 TOML、Provider profile、模型能力与环境变量名；配置
   先放 Core，复杂度确有需要时再拆 crate。`/model` 以 Core 确认结果为准，运行中拒绝
   还是延后切换要先定契约，不能仅更换 TUI 标签。
2. 文本 ContextAssembler：分离指令与会话，加载分层项目指令，确定顺序与预算接口。
   参见 [消息构建](message-building.md)。请求日志只记录必要元数据，正文诊断另行授权。
3. 工具闭环：Tool trait、Schema、只读文件/目录/搜索、调用与结果回填，直到最终回答。
   即使只读工具也先检查工作区路径边界，不能等可写工具出现才考虑安全。
4. 有副作用工具：补丁与 Shell **先建立权限、审批和隔离再启用**；需要超时、取消、输出
   截断、网络边界与非交互安全失败。沙箱不是上线后的附加功能。
5. 会话与产品化：JSONL 事件、恢复与摘要、exec 共用 Session 流、稳定退出码，随后完善
   CI、安装与发布。参见 [CLI 契约](cli-surface.md) 和 [架构](../architecture/overview.md)。

这些方向存在依赖，但不要求所有 TUI 优化完成后才做工具。新增能力不得反向依赖 TUI，
也不因提前创建了 crate 或枚举就宣称功能已完成。

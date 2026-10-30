# Thinking 流式展示 v1 设计

架构归属：终端交互与展示及跨层思考消费。状态：当前展示契约；绘制节流另标目标。源码入口：[transcript.rs](../../../crates/vortex-tui/src/widgets/transcript.rs)、[agent_loop.rs](../../../crates/vortex-core/src/agent_loop.rs)。
详见 [逻辑模块](../../architecture/logical-modules.md)；实现与验证结果以根 [README](../../../README.md) 为准。

- 日期：2026-10-23。
- 修订：2026-10-26，同步请求装配、事件链、TUI 投影与离线测试边界。
- 修订：2026-10-30，同步多调用顺序执行、按工具额度与异常历史补齐。
- 状态：描述当前展示-only 契约；实现及离线/人工验收记录以根 README 为准。
- 目标：先完整观察一次模型步骤的公开思考与正文，以及工具前后的步骤边界，再推进上下文装配。
- 关联：[SSE](../provider/streaming-response.md)、[思考选项](../provider/thinking-options.md)、
  [Session](../runtime/session-runtime.md)、[TUI](interface.md)。

## 1. 范围与接入选择

首版选择当前默认模型所用的百炼 OpenAI-compatible 接入，以单独的 Qwen Thinking
profile 显式开启：`enable_thinking=true`、`preserve_thinking=false`。保留内置默认
profile 的关闭行为，不自动改用户配置、不新增网络探测，不按模型名猜测参数。
普通用户显式选择 `thinking_preset="dashscope-qwen-display-only"` 并设置 thinking.mode；
预设针对当前 `qwen3.8-max` 候选接入，不是全部 Qwen/百炼模型的通用能力表。

这里展示的是 API 返回的文本或摘要，不是完整内部推理；没有返回时不补造，也不使用
Prompt 强迫模型输出 Thought 标签。思考不是新 MessageRole，不拼入 Assistant 正文。

接入预设、可选覆盖、独立历史适用标记与方言矩阵由
[Provider 思考选项](../provider/thinking-options.md#8-能力预设与覆盖) 统一维护；配置输入见
[配置设计](../configuration/configuration.md)。当前仅特定 Qwen 展示-only 接入允许 Enabled，不支持 DeepSeek
思考历史回传。静态能力声明不证明远端兼容，实际工具往返仍需人工验收。

## 2. 请求策略与运行时策略分开

Provider 根据已校验请求选择 Reject/DisplayOnly，Turn 对照固定实例的策略决定是否转发；
Session 只发布步骤与思考事件，不保存思考。UI 不从模型名称或当前标签推断消费权限。
ProviderDefault 不等于关闭，Disabled 意外返回思考仍拒绝，不能以显示能力绕过协议限制。

## 3. 增量事件与模型步骤

步骤与思考事件：

```rust
// agent_loop.rs 内部 TurnProgress，session.rs 公共 CoreEvent 同形转发。
ModelStepStarted { step: u32 },
AssistantReasoningDelta { delta: String }, // CoreEvent 名称
// 内部 TurnProgress 对应 ReasoningDelta { delta: String }。
```

每次 provider.stream 前发送 ModelStepStarted，从 1 开始。同一步可返回多个调用，
本地逐个执行完再进入下一模型步骤；step 不是工具次数，也不是新的 TurnId。
每工具额度独立，耗尽后仍可请求最终回答；没有新增全局模型步骤上限。

```text
UserMessageAdded → AssistantMessageStarted
  → ModelStepStarted(1) → Reasoning* / Text*
  → (ToolCallStarted → ToolCallFinished) × 本批调用数
  → ModelStepStarted(2) → Reasoning* / Text*
  → TurnCompleted 或 TurnFailed
```

思考与正文不强制互斥或严格先后；同一个包的两者都处理。run_turn 只对展示-only 思考
计数并发增量，不在权威历史累积它。新增每模型步骤 256 KiB UTF-8 思考字节上限，
超过时失败并保留此前可见片段，不截断为成功；它不是模型 Token、费用或整会话内存上限。

保留有界通道、FIFO 排空后发单一 Turn 终态、Shutdown 与背压取消规则。不要把思考
发送改成 try_send，也不要恢复旧 ActiveTurn 正文/思考缓冲。新事件不改变消息/工具历史
提交规则；只有思考而无最终正文，Stop 仍失败。

## 4. TUI 投影与历史

首版让 Transcript 的每个 Assistant entry 对应一个可见模型步骤，包含独立 reasoning
与 content 字符串。用户和错误条目的 reasoning 为空；SessionMessage / ConversationItem
保持不变。AssistantMessageStarted 创建本 Turn 占位，首次 ModelStepStarted 复用空占位；
后续步骤在前一条已有内容时新建 Assistant entry，避免把工具前后的思考拼成同一段。

reasoning 纯文本、弱化颜色、明确 Thinking 标签；content 继续按整段 Markdown 渲染。
两者合并成同一个 Paragraph，按实际内容宽度测高与绘制，复用原滚动模型。第一版不自动
折叠、不改布局框架、不顺手更换 Markdown 库或修复已暂缓的滑块视觉问题。

AppState 增加 UI 专属阶段：Waiting / Reasoning / Answering / Tool。Core SessionStatus
保持 Idle / Running / Failed。新模型步骤重置为 Waiting；同一步进入 Answering 后，
晚到思考不让状态倒退。工具完成后等待下一步。Turn 成功/失败时清空阶段，保持模型切换
pending、草稿与 /exit 的原规则。

失败仅删除 reasoning 和 content 都为空的最后占位，只有思考也须保留并追加错误。
DisplayOnly 思考的业务展示仅存当前 TUI 内存，不写模型历史、Snapshot、会话文件或普通日志。
显式开启 Debug wire TRACE 时，原始 SSE data 中的公开思考会另存诊断文件，见
[诊断边界](../cross-cutting/diagnostics.md)；这不增加会话恢复或思考回传能力。
未来重连/恢复会丢失这些展示信息是本版显式限制，不因此修改 SessionSnapshot。

绘制节流不阻挡首版正确性验收；先沿用增量绘制。若真实终端出现明显卡顿，再单独实施
[绘制节流目标](#绘制节流目标) 中的 dirty/tick 节流候选方案，不能用
丢弃增量或逐字 sleep 代替。纯文本渲染不自动等同于终端控制字符防护，后者仍需专门验收。

## 5. 分批验收

1. 请求装配：预设展开、显式 false 覆盖、未知预设/方言拒绝；有能力但无历史契约仍拒绝；
   Enabled JSON、旧 TOML 和旧模式不回归，内置默认仍关闭。
2. Core：思考提前到达、双字段顺序、多模型步骤、不回填思考、无正文失败、大小上限。
3. Session：先步骤事件，再增量，最后唯一终态；Thinking 中 Shutdown 和满队列退出。
4. TUI：分区、步骤不混合、晚到思考、失败片段保留、滚动/resize 与 slash/模型切换回归。
5. 人工：明确选择 Thinking profile，验证文本、目录/Mock 工具、两次用户输入及 /exit。

自动化测试使用 fake 和固定 SSE JSON，不访问真实模型；预设/请求、Decoder、Core、Session
和 TUI 已有离线回归，实际结果只在根 README 记录。原实施稿已回收并清理，人工验收
顺序与失败分类见 [回归测试方案](../../engineering/regression-test-plan.md#thinking-展示与工具往返)。
本文件与 [配置设计](../configuration/configuration.md) 保留当前契约，不依赖本地临时文件。

### 人工验收与已知限制

作者已反馈诗词类纯文本请求可显示 Thinking，不能由此认定工具往返已通过。此前搜索的
`multiple tool calls are not supported in v1` 记录于
[Issue #3](https://github.com/AegeanZhang/vortex/issues/3)。当前 Decoder 已按索引组装多个
调用，Core 支持批次与顺序执行，不再保留单调用限制；请求仍不发送 parallel_tool_calls=false。
离线修复不等于真实服务验收或 Issue 已关闭。

离线工具脚本已验证分步事件、不回填思考、批次/额度及异常补齐；公开 CoreEvent 未新增
批次事件，TUI 仍逐项更新工具状态。真实 Qwen 工具往返仍待验收。
DeepSeek Enabled 在本地选项校验时拒绝；添加 can_enable 或套用 Qwen 预设不能接通它。

## 6. 后续边界

DeepSeek 的工具思考历史支持须另立方案：完整保留工具步骤 Assistant 的正文、reasoning
与调用关系，适配 wire 请求，解决跨模型切换和数据出口，不只是给最终回答加一个字段。
签名、opaque reasoning 与文本 reasoning 不混为一类。上下文组装、长期记忆、持久化、
CancelTurn、工具卡片、思考折叠与 Token 统计不纳入本版。

## 绘制节流（目标）

绘制节流是独立的待实施步骤。当前每个 Delta 请求重绘，仅合并同批 Effect，不跨事件合并。
目标是接收与绘制解耦：收到事件立即更新数据并置 dirty；每约 33 ms 最多绘制一次，空闲不绘制。
Tokio interval 使用 `MissedTickBehavior::Skip`，结束/失败可强制刷新最后一帧。退出优先清理，
不要求再画终态。禁止逐字 sleep，也不合并丢弃生命周期事件。

每次按当前整条 Assistant 原文解析 Markdown，不逐 delta 独立解析。沿用 follow_tail，
历史浏览时不强制到底；未闭合代码围栏和 resize 的回流不承诺完全稳定阅读锚点。先节流，
再按测量结果缓存完成消息；不改变已选的 Markdown 渲染器，不顺手修复滚动条视觉问题。

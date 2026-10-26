# Thinking 流式展示 v1 设计

- 日期：2026-10-23。
- 修订：2026-10-26，同步请求装配、事件链、TUI 投影与离线测试边界。
- 状态：描述当前展示-only 契约；实现及离线/人工验收记录以根 README 为准。
- 目标：先完整观察一次模型步骤的公开思考与正文，以及工具前后的步骤边界，再推进上下文装配。
- 关联：[SSE](streaming-response.md)、[思考选项](provider-thinking-options.md)、
  [Session](session-runtime.md)、[TUI](tui-interface.md)。

## 1. 范围与接入选择

首版选择当前默认模型所用的百炼 OpenAI-compatible 接入，以单独的 Qwen Thinking
profile 显式开启：`enable_thinking=true`、`preserve_thinking=false`。保留内置默认
profile 的关闭行为，不自动改用户配置、不新增网络探测，不按模型名猜测参数。
普通用户显式选择 `thinking_preset="dashscope-qwen-display-only"` 并设置 thinking.mode；
预设针对当前 `qwen3.8-max` 候选接入，不是全部 Qwen/百炼模型的通用能力表。

这里展示的是 API 返回的文本或摘要，不是完整内部推理；没有返回时不补造，也不使用
Prompt 强迫模型输出 Thought 标签。思考不是新 MessageRole，不拼入 Assistant 正文。

首版只接受选中上述预设、解析后 `can_enable=true` 且支持关闭思考历史控制的 DashScope 接入。
`can_enable` 表示此模型在该接入下允许显式开启，不是“模型具有推理能力”的通用声明。
“可以设置历史控制参数”不等于“工具循环允许省略思考回传”。Provider 的预设还提供
独立的 `DisplayOnlyWithHistoryDisabled` 历史契约标记；无预设时为 Unverified。
用户能力覆盖不能创造这个标记，只有三个 true 的自定义 Enabled 仍拒绝。
DeepSeek 官方、百炼托管 DeepSeek 和 Generic 的显式 Enabled 在本版均拒绝；它们的
Disabled / ProviderDefault 继续保持现有请求装配行为。

2026-10-23 核对的官方资料说明：

- [百炼深度思考](https://help.aliyun.com/zh/model-studio/deep-thinking)：
  通过 enable_thinking 控制适用模型，公开思考与正文分别返回。
- [百炼参数](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions)：
  preserve_thinking 控制历史 reasoning 是否作为输入；文档对部分型号默认值有例外，
  本版一律显式关闭，不依赖默认值。
- [DeepSeek 思考模式](https://api-docs.deepseek.com/guides/thinking_mode/)：
  请求携带 tools 时，后续请求须回传此前 reasoning，不能套用本版仅展示策略。

这些是协议选择依据，不是已完成的真实 API 验收。Qwen 的思考＋工具往返仍需作者人工
验证；若具体端点要求回传 reasoning，本版应停止将它作为展示-only 候选，不能靠吞字段
或静默关闭工具凑通。没有外部服务切换或付费调用的默认授权。

预设只记录静态适用契约，不验证远端身份、路由或实际响应。显式使用预设表示选择相应
契约，不按域名或型号自动猜测，也不拒绝用户明确配置的兼容路由别名；不适用的接入
不能仅靠选择该 ID 被视为已支持。实际服务商、型号、工具往返必须纳入人工验收。
Kimi/GLM 不是免适配的替代品，它们的工具思考历史同样需单独设计，见
[Kimi Thinking](https://platform.kimi.ai/docs/guide/use-thinking-models) 和
[GLM 思考模式](https://docs.bigmodel.cn/cn/guide/capabilities/thinking-mode)
（资料核对：2026-10-23；本次修订未执行远端实验）。

## 2. 请求策略与运行时策略分开

当前分层接口：

| 层 | 当前职责 |
| --- | --- |
| Core ThinkingMode | 增加 Enabled，保留 ProviderDefault 默认值 |
| Core ThinkingCapabilities | 增加默认 false 的 can_enable |
| Core 配置输入 | thinking_preset 可选字符串；ThinkingCapabilitiesOverride 三个 Option<bool> |
| Provider 预设解析 | 保守基线 → 显式预设 → 逐字段覆盖；另保存预设的历史适用契约 |
| Provider 请求选项 | 校验能力和历史契约；Enabled 仅映射为 DashScope 的 true/false 两个字段 |
| ModelProvider | 增加默认 Reject 的 reasoning_output_policy 方法 |
| ReasoningOutputPolicy | Reject / DisplayOnly，不包含 HTTP、方言或 TUI 类型 |

`DisplayOnly` 仅由成功装配的显式 Enabled 得到。ProviderDefault 即使远端返回思考，
在本版仍拒绝；Disabled 返回思考也不能偷偷接收。这样保留当前严格边界，不把支持 UI
展示误变成所有模型都开启或接受思考。策略取自本 Turn 固定的 Provider 实例，不从
TUI 当前标签、日志或模型名称反推。

工厂的目录不可用原因与 create 入口使用相同选项校验，非法组合在读取 key 之前失败。
重复 with_options 必须同时替换请求字段和输出策略，不能留下前一组模型选项。
项目配置与 -c 白名单不扩展，开启由用户级完整模型 profile 明确声明。

配置输入与运行时能力分开：覆盖中的 None 表示未填写，Some(false) 必须覆盖预设 true；
不能在反序列化时提前变成 false。未知预设或方言不匹配本地拒绝，未选中的非法模型标记
不可用但不读取其 key。预设不包含连接、模型路由或 thinking 开关，不引入完整模型目录。
详细配置、旧 TOML 兼容和装配接口见
[Provider 选项契约](provider-thinking-options.md#8-能力预设与覆盖)。

## 3. 增量事件与模型步骤

步骤与思考事件：

```rust
// agent_loop.rs 内部 TurnProgress，session.rs 公共 CoreEvent 同形转发。
ModelStepStarted { step: u32 },
AssistantReasoningDelta { delta: String }, // CoreEvent 名称
// 内部 TurnProgress 对应 ReasoningDelta { delta: String }。
```

每次 provider.stream 前发送 ModelStepStarted，从 1 开始。现有四次工具执行上限下，
可有第五个模型步骤生成最终回答；step 不是工具次数，也不是新的 TurnId。

```text
UserMessageAdded → AssistantMessageStarted
  → ModelStepStarted(1) → Reasoning* / Text*
  → ToolCallStarted → ToolCallFinished
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
DisplayOnly 思考仅存当前 TUI 内存，不写模型历史、Snapshot、会话文件或普通日志。
未来重连/恢复会丢失这些展示信息是本版显式限制，不因此修改 SessionSnapshot。

绘制节流不阻挡首版正确性验收；先沿用增量绘制。若真实终端出现明显卡顿，再单独实施
[SSE 文档](streaming-response.md#7-tui-投影与刷新) 中的 dirty/tick 节流候选方案，不能用
丢弃增量或逐字 sleep 代替。纯文本渲染不自动等同于终端控制字符防护，后者仍需专门验收。

## 5. 分批验收

1. 请求装配：预设展开、显式 false 覆盖、未知预设/方言拒绝；有能力但无历史契约仍拒绝；
   Enabled JSON、旧 TOML 和旧模式不回归，内置默认仍关闭。
2. Core：思考提前到达、双字段顺序、多模型步骤、不回填思考、无正文失败、大小上限。
3. Session：先步骤事件，再增量，最后唯一终态；Thinking 中 Shutdown 和满队列退出。
4. TUI：分区、步骤不混合、晚到思考、失败片段保留、滚动/resize 与 slash/模型切换回归。
5. 人工：明确选择 Thinking profile，验证文本、目录/Mock 工具、两次用户输入及 /exit。

自动化测试使用 fake 和固定 SSE JSON，不访问真实模型；预设/请求、Decoder、Core、Session
和 TUI 已有离线回归，实际结果只在根 README 记录。原手写步骤保存在本地实施稿，
本文件与 [配置设计](configuration.md) 不依赖该未入库文件来解释当前契约。

### 人工验收与已知限制

作者已反馈诗词类纯文本请求可显示 Thinking，不能由此认定工具往返已通过。搜索时
观察到 `multiple tool calls are not supported in v1`，见
[Issue #3](https://github.com/AegeanZhang/vortex/issues/3)：当前单次模型响应仅接受一个调用，
Decoder 拒绝非零工具索引，Core 同样拒绝第二个调用；请求尚未发送 parallel_tool_calls=false。
该限制与整个 Turn 四次工具预算不同，不在本轮修复。

离线工具脚本已验证分步事件、不回填思考及预算边界，真实 Qwen 工具往返仍待验收。
DeepSeek Enabled 在本地选项校验时拒绝；添加 can_enable 或套用 Qwen 预设不能接通它。

## 6. 后续边界

DeepSeek 的工具思考历史支持须另立方案：完整保留工具步骤 Assistant 的正文、reasoning
与调用关系，适配 wire 请求，解决跨模型切换和数据出口，不只是给最终回答加一个字段。
签名、opaque reasoning 与文本 reasoning 不混为一类。上下文组装、长期记忆、持久化、
CancelTurn、工具卡片、思考折叠与 Token 统计不纳入本版。

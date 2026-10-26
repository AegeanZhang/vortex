# Provider 思考选项装配设计

- 日期：2026-10-19。
- 修订：2026-10-26，同步能力预设、可选覆盖、Enabled 请求与输出策略。
- 状态：描述当前请求装配与展示-only 契约；实现进度及离线/人工验收结果见根 README。
- 范围：TUI 的模型配置、CLI 工厂与 OpenAI-compatible 请求装配；不修改旧 exec。

当前支持 ProviderDefault、Disabled，以及指定 Qwen 候选的 DashScope 展示-only Enabled。
能力预设、可选覆盖和独立历史契约见第 8 节；响应消费和 UI 规则见
[Thinking 展示 v1](thinking-display.md)。DeepSeek/Generic Enabled 尚未适配。

当前源码使用接口方言、模型思考策略与能力声明，Provider 通过统一 `with_options` 装配。
默认千问显式关闭思考；其他服务省略参数时可能返回思考文本，触发现有 Agent loop
的拒绝分支；只有成功装配的显式 Enabled 允许展示思考。完整配置见
[配置设计](configuration.md)，响应和历史边界见 [SSE 设计](streaming-response.md)。

## 1. 策略、能力与适配分层

```text
模型 profile 的 ThinkingOptions（表达意图）
  + 显式 thinking_preset 与 ThinkingCapabilitiesOverride（能力与独立历史契约）
  + Provider profile 的 api_flavor（声明接口方言）
  → CLI Factory 装配 OpenAiChatOptions
  → Provider 校验并转换为请求字段与 ReasoningOutputPolicy
  → ChatRequest 序列化，复用原来的 SSE 接收与解析
```

Core 提供通用 `ThinkingMode` / `ThinkingOptions` 与 `ThinkingCapabilities`，不包含厂商 HTTP DTO。
Core config 的 `ApiFlavor` 只是显式配置声明，CLI 将其映射为 Provider 内的
`OpenAiChatDialect`；具体字段转换留在 `vortex-provider`。
TUI、Session、Agent loop 不识别厂商选项。

使用枚举匹配当前有限的方言，不新增动态策略 trait，不传任意 JSON 作为扩展参数。
以后增加新方言只扩展配置声明、装配映射与 Provider 适配，不修改统一思考策略。
模型能力声明不等于在线探测结果，也不是仅由模型原始名称决定的属性；同名模型在
不同供应商下分别维护能力。本轮不重构现有 supports_tools 或引入完整模型目录。

## 2. 配置契约

Provider 的 `kind` 仍为 `openai-chat`，`api_flavor` 支持：`generic`（缺省）、`dashscope`、
`deepseek`。它表示实际调用接口的参数格式，不代表模型所属公司。
通过百炼调用 DeepSeek 时应按百炼实际接口选择方言，而不是看到模型名就选 DeepSeek
官方参数；不按 ID、域名或模型名称进行隐式识别。
DashScope 在本文仅表示百炼 OpenAI-compatible Chat Completions 扩展，仍调用
`/chat/completions`；不是 DashScope 原生 API 或 Responses API。

模型使用 `thinking`、可选 `thinking_preset` 与高级 `thinking_capabilities` 覆盖表。
内置 Qwen 默认关闭，等价用户配置为：

```toml
[providers.qwen]
kind = "openai-chat"
api_flavor = "dashscope"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
api_key_env = "QWEN_API_KEY"

[models.qwen-max]
provider = "qwen"
model = "qwen3.8-max"
display_name = "Qwen Max"
supports_tools = true
thinking_preset = "dashscope-qwen-display-only"

[models.qwen-max.thinking]
mode = "disabled"
```

`mode` 支持：

- `provider-default`（缺省）：省略全部思考控制字段，使用服务端行为；不等于关闭思考。
- `disabled`：必须显式表达关闭；当前方言不支持时本地报错，不静默降级为省略参数。
- `enabled`：仅接受已适配的展示-only 接入；须满足方言、能力与独立历史契约检查。

本轮不接受思考强度或通用历史回传开关。内置 Qwen profile 使用 DashScope 方言、
Disabled、`dashscope-qwen-display-only` 预设与空覆盖，保持原关闭请求；
新定义或整条替换的模型省略 thinking 时回到 ProviderDefault，不继承旧模型策略。

配置输入 `ThinkingCapabilitiesOverride` 有三个 `Option<bool>`，省略为 None。
运行时 `ThinkingCapabilities` 则有三个 bool，无预设且无覆盖时均为 false：

- `can_enable`：声明在此接入下允许显式开启，不单独证明展示或工具历史兼容。
- `can_disable`：声明该模型在此接口下允许关闭思考；false 表示未声明支持。
- `supports_history_control`：声明支持独立历史思考控制参数。本轮只实现 DashScope
  的 preserve_thinking 映射，不等于模型能推理、会返回思考或协议要求回传历史。

预设提供默认能力，用户可依据服务文档逐字段覆盖，不从模型名推断，不保证远端实际能力。
ProviderDefault 不使用这些标志生成控制参数；Disabled 必须声明 can_disable=true。
当前千问预设的三个能力为 true；DeepSeek 官方及百炼 DeepSeek 关闭配置的 can_disable
为 true、supports_history_control 为 false。不能把千问能力复制到所有百炼模型。

Provider/模型仍按 ID 整条替换，不对 thinking 子表做跨层字段拼接。
省略预设和覆盖时回到全 false。项目配置和 `-c` 白名单保持不变，不能通过项目文件
新增或修改接口方言、思考策略、预设或能力声明。

## 3. 请求转换

ProviderDefault 在任意方言与能力声明下都不发送控制字段。Disabled 先检查 can_disable，
未声明时返回能力错误；已声明时再按下表处理：

| 方言 | supports_history_control | Disabled |
| --- | --- | --- |
| Generic | 任意 | 本地方言不支持错误 |
| DashScope | false | 只发送 `enable_thinking=false` |
| DashScope | true | `enable_thinking=false`、`preserve_thinking=false` |
| DeepSeek 官方 | false | `thinking={"type":"disabled"}` |
| DeepSeek 官方 | true | 本地能力与适配器不匹配错误，不忽略声明 |

Enabled 只接受 DashScope、can_enable=true、supports_history_control=true 和
DisplayOnlyWithHistoryDisabled 历史契约，发送 `enable_thinking=true`、
`preserve_thinking=false`。不把开启与历史控制视为同一语义，不读取或回传思考历史。

这些字段直接位于 HTTP JSON 顶层，不套 SDK 的 `extra_body`。
字段不适用时必须省略，而不是发送 null、其他厂商字段或空对象。
千问控制参数与历史开关见 [百炼深度思考说明](https://help.aliyun.com/zh/model-studio/deep-thinking)，
DeepSeek 参数见 [官方思考模式](https://api-docs.deepseek.com/zh-cn/guides/thinking_mode/)，
托管参数见 [百炼 DeepSeek](https://help.aliyun.com/zh/model-studio/deepseek-api)，
历史参数适用范围见 [百炼 Chat Completions](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions)
（核对日期：2026-10-19）。

Provider 内的选项模块负责纯转换，输出类型化字段；请求 DTO 用 flatten 合并
这些受控字段。统一 builder `with_options` 校验后保存转换结果，构造请求时不再决定
策略。只有 Enabled 的输出策略为 DisplayOnly，其余为 Reject；重复 builder 调用同时
替换请求字段与输出策略。现有消息、工具定义、ToolCall 和 ToolResult 映射保持不变。

## 4. 装配和失败边界

工厂按顺序处理：查找模型与连接 → 检查工具能力 → 映射策略/能力/方言并校验 → 读取选中
密钥 → 构造 Provider。非法组合在读取密钥、创建请求和进入 TUI 之前失败。
`/model` 复用工厂；失败沿现有 ModelSelectionRejected 保留原绑定和草稿。
尚未选中的 profile 不读取密钥。目录与 create 复用同一 `chat_options` 校验预设、能力、
方言和历史契约；不合法的闲置模型标为不可用，不阻止其他合法模型构造。

`OpenAiChatProvider::new/from_client` 的默认行为仍为 Generic + ProviderDefault。
旧 `QwenThinkingOptions` / `with_qwen_thinking` 移除，避免长期保留两套入口。
错误区分未声明开启/关闭能力、方言不支持策略、不支持历史控制、未知预设、方言不匹配
和历史契约缺失。错误只包含安全枚举信息，不包含密钥、Prompt、原始 TOML 或响应正文。

本地校验只能证明该适配器能够表达参数，不能证明某个远端模型接受参数或已关闭思考。
不支持关闭的模型仍可能远端失败；即使显式关闭后返回 ReasoningDelta，现有拒绝分支也
继续生效，不吞掉事件、不把思考拼入正文、不自动重试或切换策略。

## 5. 配置迁移

本次明确迁移，不保留兼容别名或自动转换：

| 旧配置 | 新配置 |
| --- | --- |
| `request_options="plain"` | 删除旧字段；thinking 省略或 mode 为 provider-default |
| `request_options="qwen-stage-a"` | 删除旧字段；api_flavor=dashscope；模型 Disabled，原千问模型的两个能力均为 true |
| DeepSeek 官方接口需要关闭思考 | api_flavor=deepseek；模型 Disabled，can_disable=true、历史控制为 false |
| 百炼托管 DeepSeek 需要关闭思考 | api_flavor=dashscope；模型 Disabled，can_disable=true、历史控制为 false |

旧字段继续通过 deny_unknown_fields 报错。程序不改写本地文件、不读 `.env`、不迁移
用户目录；同 ID Provider 整条替换时，用户需要在新条目中显式保留正确方言。
上一版未实现草稿中的 api_flavor=qwen 不保留别名，新配置继续严格拒绝未知值。

## 6. 验收与后续

离线测试验证默认值、旧字段拒绝、方言/策略/能力组合、三种接入 profile 的精确 JSON、
不支持历史控制时字段省略而非 null、无厂商参数串用、
消息/工具映射不变，以及工厂在非法组合下不读取 key。使用固定假数据，自动化测试
不调用真实 API。远端参数接受情况、正文流和工具往返由作者另做人工验收。

2026-10-19 已补单元测试，覆盖 Core 配置、Provider 转换/DTO/builder 和 CLI 工厂；
还验证同 Provider 模型能力互不串用、切换装配失败保留原绑定。完整检查记录见根 README。
后续已补预设/覆盖、Enabled JSON 与消费策略回归，实际范围和结果见根 README；
纯转换和 fake 测试不能代替 HTTP 字节分帧或真实 Thinking 工具往返验收。

`ModelRequest`、`ModelEvent` 和 SessionMessage 的形状保持不变；TurnProgress/CoreEvent
已增加步骤与思考增量，消费和 UI 按 Thinking v1 设计执行。DeepSeek 等接入的工具思考
历史仍须独立适配，不能假定所有协议都能先只展示、以后再补回传。
通用抽象统一意图与装配入口，不承诺各供应商具有相同能力。

## 7. 调研依据与采纳边界

[OpenCode 模型文档](https://opencode.ai/v2/docs/models) 将 reasoning 的支持、toggle、
力度和预算与 Provider 选项分开；本文采纳能力与适配分层，不引入完整目录或 variants。
[Aider reasoning 文档](https://aider.chat/docs/config/reasoning.html) 用 accepts_settings
声明模型可接受的设置；本文采纳显式声明，但对不支持的关闭意图采用报错而非忽略。
来源核对日期为 2026-10-19，外部行为不是 Vortex 已实现能力的证明。

## 8. 能力预设与覆盖

本节定义当前预设与覆盖规则，普通用户无需逐项填写能力。运行时展示契约以
[Thinking 展示 v1](thinking-display.md) 为准。

### 用户配置与解析顺序

用户级模型支持 `thinking_preset: Option<String>`，显式选择，不根据模型名、域名或
Provider ID 自动匹配。预设 ID 复用 1–64 字节 kebab-case 约束；Core 仅验证形式，
Provider 维护小型预设表、识别 ID 并检查 api_flavor 是否匹配。
预设不含模型路由、地址、凭据或 thinking.mode，不修改用户意图。

模型原来的 `thinking_capabilities` 表保留名字，改用配置类型 ThinkingCapabilitiesOverride：
can_enable、can_disable、supports_history_control 均为 Option<bool>，缺省 None。
运行时 ThinkingCapabilities 则保留三个 bool，缺省 false。

```text
保守基线（全部 false）
→ 显式预设提供能力默认值和独立历史适用契约
→ 用户逐字段覆盖能力（None 不覆盖，Some(false) 覆盖 true）
→ 适配器校验策略、能力、方言及历史契约
→ 类型化请求字段 + ReasoningOutputPolicy
```

只有预设内部默认值与同一条目的覆盖进行合并；不同配置层的同 ID 模型仍整条替换，
不继承旧条目的预设、能力或策略。项目配置与 -c 白名单不扩展。
无预设时，旧 TOML 的布尔能力仍生效，未提供的能力解析后为 false。
内置 Qwen 使用预设加空覆盖，mode 保持 Disabled，旧请求 JSON 不变。

### 首版预设与历史契约

只维护 `dashscope-qwen-display-only`：适用于本文所用的百炼 OpenAI-compatible
`qwen3.8-max` 候选；要求 api_flavor=dashscope；三个能力均为 true。
不把相同能力复制给全部 Qwen、百炼托管 DeepSeek 或其他供应商型号。

Provider 另持有 ReasoningHistoryPolicy，首版仅含 Unverified（默认）和
DisplayOnlyWithHistoryDisabled。只有此预设提供后一标记，能力覆盖不能生成或修改它，
配置不开放 history_policy 字段。该标记表示当前适配器的静态适用声明，不能证明远端支持，
也不是安全权限或在线验证机制。具体端点若要求思考回传，应停止该候选验收并修订预设，
不得把 reasoning 拼进正文、吞掉字段或静默关闭工具。

supports_history_control 只说明能表达独立历史控制参数，不证明工具循环允许省略回传。
Enabled 需要 DashScope 方言、can_enable=true、supports_history_control=true 和上述
独立历史契约；请求仍为 enable_thinking=true、preserve_thinking=false。
无预设但三个覆盖均为 true 仍拒绝 Enabled。DeepSeek/Generic Enabled 继续拒绝；
覆盖不能绕过尚未实现的协议和历史处理。
ProviderDefault 不生成控制字段且输出策略为 Reject；Disabled 维持旧映射且为 Reject。
只有成功装配的 Enabled 为 DisplayOnly，重复 with_options 原子替换字段与输出策略。

### 装配与兼容性

Core 承载统一意图、能力值及配置覆盖类型，不保存厂商预设表或 HTTP DTO。
Provider 的纯函数 resolve_thinking_profile(dialect, preset, overrides) 返回能力与历史
标记；CLI 的 chat_options 同时用于目录和 create，再调用 OpenAiChatOptions::validate。
全流程不联网，未知预设和方言不匹配即使在 ProviderDefault 下也不静默忽略。
合法形式但无效的闲置 profile 标为不可用，不阻止其他合法模型构造；选中失败沿原拒绝链
保留绑定和草稿，读取 key 之前完成校验，错误不回显原始配置或任意预设字符串。

验收新增 None/false 区分、旧布尔 TOML、预设/覆盖、未知 ID、方言不匹配、缺历史契约、
整条替换与跨模型不串用；保留旧 JSON 精确断言和非法组合不读 key 测试。
预设元数据与 FakeProvider 测试不代替真实工具往返验收。

### 选型依据与不做的事情

本轮不新增中转站、在线能力探测、完整模型目录或 supports_tools 重构；协议适配继续
位于 vortex-provider。[Codex 网关说明](https://learn.chatgpt.com/docs/enterprise/gateway-compatibility)
说明元数据不会创造能力；[Claude Code 模型配置](https://code.claude.com/docs/en/model-config)
采用已知能力与显式覆盖。本文采纳这种分层，不承诺与这些产品的接入范围一致。

资料核对于 2026-10-23：当前 [百炼参数](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions)
也为 Qwen 候选提供 reasoning_effort（none 映射为关闭）；首版沿用已有两个显式字段，
不同时新增强度、预算或第二套转换入口。是否支持 OpenAI SDK、字段名是否相同，均不足以
证明历史语义相同。[Kimi](https://platform.kimi.ai/docs/guide/use-thinking-models)、
[GLM](https://docs.bigmodel.cn/cn/guide/capabilities/thinking-mode) 和
[DeepSeek](https://api-docs.deepseek.com/guides/thinking_mode/) 均有独立的工具步骤或跨轮思考
回传要求，不能把它们视为免适配的展示-only 替代品。本次修订没有新增真实 API 验收记录。

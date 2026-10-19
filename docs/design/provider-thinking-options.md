# Provider 思考选项装配设计

- 日期：2026-10-19。
- 修订：结合多供应商 Agent 调研，拆分策略、模型能力与接口方言，保留按能力关闭历史控制。
- 状态：请求侧装配已实现，离线回归通过；真实 API 参数接受情况与终端交互待人工验收。
- 范围：TUI 的模型配置、CLI 工厂与 OpenAI-compatible 请求装配；不修改旧 exec。

当前源码使用接口方言、模型思考策略与能力声明，Provider 通过统一 `with_options` 装配。
默认千问显式关闭思考；其他服务省略参数时可能返回思考文本，触发现有 Agent loop
的拒绝分支。本文解决请求装配，不启用思考内容处理。完整配置见
[配置设计](configuration.md)，响应和历史边界见 [SSE 设计](streaming-response.md)。

## 1. 策略、能力与适配分层

```text
模型 profile 的 ThinkingOptions（表达意图）
  + 模型 profile 的 ThinkingCapabilities（此供应商下该模型的能力）
  + Provider profile 的 api_flavor（声明接口方言）
  → CLI Factory 装配 OpenAiChatOptions
  → Provider 校验并转换为请求字段
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

模型使用 `thinking` 与 `thinking_capabilities` 表：

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

[models.qwen-max.thinking]
mode = "disabled"

[models.qwen-max.thinking_capabilities]
can_disable = true
supports_history_control = true
```

`mode` 首版只有：

- `provider-default`（缺省）：省略全部思考控制字段，使用服务端行为；不等于关闭思考。
- `disabled`：必须显式表达关闭；当前方言不支持时本地报错，不静默降级为省略参数。

本轮不接受 `enabled`、思考强度或通用历史回传开关，防止配置宣称支持尚未完成的能力。
内置 Qwen profile 显式配置 DashScope 方言、Disabled 与两个 true 能力，保持原行为；
新定义或整条替换的模型省略 thinking 时回到 ProviderDefault，不继承旧模型策略。

`ThinkingCapabilities` 的两个布尔字段均缺省为 false：

- `can_disable`：声明该模型在此接口下允许关闭思考；false 表示未声明支持。
- `supports_history_control`：声明支持独立历史思考控制参数。本轮只实现 DashScope
  的 preserve_thinking 映射，不等于模型能推理、会返回思考或协议要求回传历史。

这些字段由用户依据服务文档维护，不从模型名推断，不保证远端实际能力。
ProviderDefault 不使用这些标志生成控制参数；Disabled 必须声明 can_disable=true。
百炼千问示例的两个能力为 true；DeepSeek 官方及百炼 DeepSeek 示例的 can_disable
为 true、supports_history_control 为 false。不能把千问能力复制到所有百炼模型。

Provider/模型仍按 ID 整条替换，不对 thinking 子表做跨层字段拼接。
能力省略时也回到两个 false。项目配置和 `-c` 白名单保持不变，不能通过项目文件
新增或修改接口方言、思考策略或能力声明。

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

千问显式关闭历史控制是本轮文本阶段的兼容规则，不把两个开关视为同一语义；将来
开放 Enabled 时独立设计历史策略。本轮不新增通用历史配置或读取思考历史。

这些字段直接位于 HTTP JSON 顶层，不套 SDK 的 `extra_body`。
字段不适用时必须省略，而不是发送 null、其他厂商字段或空对象。
千问控制参数与历史开关见 [百炼深度思考说明](https://help.aliyun.com/zh/model-studio/deep-thinking)，
DeepSeek 参数见 [官方思考模式](https://api-docs.deepseek.com/zh-cn/guides/thinking_mode/)，
托管参数见 [百炼 DeepSeek](https://help.aliyun.com/zh/model-studio/deepseek-api)，
历史参数适用范围见 [百炼 Chat Completions](https://help.aliyun.com/zh/model-studio/qwen-api-via-openai-chat-completions)
（核对日期：2026-10-19）。

Provider 内的选项模块负责纯转换，输出类型化字段；请求 DTO 用 flatten 合并
这些受控字段。统一 builder `with_options` 校验后保存转换结果，构造请求时不再决定
策略。现有消息、工具定义、ToolCall 和 ToolResult 映射保持不变。

## 4. 装配和失败边界

工厂按顺序处理：查找模型与连接 → 检查工具能力 → 映射策略/能力/方言并校验 → 读取选中
密钥 → 构造 Provider。非法组合在读取密钥、创建请求和进入 TUI 之前失败。
`/model` 复用工厂；失败沿现有 ModelSelectionRejected 保留原绑定和草稿。
尚未选中的 profile 不读取密钥，能力与方言兼容性在构建该模型时检查。
目录禁用原因本轮仍只包含原工具能力判断，装配失败复用已有拒绝链，不扩展 TUI 状态。

`OpenAiChatProvider::new/from_client` 的默认行为仍为 Generic + ProviderDefault。
旧 `QwenThinkingOptions` / `with_qwen_thinking` 移除，避免长期保留两套入口。
错误区分未声明关闭能力、方言不支持策略与不支持历史控制；只包含安全枚举信息，
不包含密钥、Prompt、原始 TOML 或响应正文。

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
这些测试不等于 HTTP 分帧、完整工具循环或 Thinking 内容处理已完成验收。

本轮不改变 `ModelRequest`、`ModelEvent`、TurnProgress、CoreEvent 或 SessionMessage。
后续支持 Enabled 时，必须同时设计内容消费、UI、模型能力及工具历史策略；DeepSeek
带 tools 的思考请求有历史回传要求，不能假定所有协议都能先只展示、以后再补回传。
通用抽象统一意图与装配入口，不承诺各供应商具有相同能力。

## 7. 调研依据与采纳边界

[OpenCode 模型文档](https://opencode.ai/v2/docs/models) 将 reasoning 的支持、toggle、
力度和预算与 Provider 选项分开；本文采纳能力与适配分层，不引入完整目录或 variants。
[Aider reasoning 文档](https://aider.chat/docs/config/reasoning.html) 用 accepts_settings
声明模型可接受的设置；本文采纳显式声明，但对不支持的关闭意图采用报错而非忽略。
来源核对日期为 2026-10-19，外部行为不是 Vortex 已实现能力的证明。

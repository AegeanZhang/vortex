# 配置模块产品设计

架构归属：配置管理。状态：当前实现契约。源码入口：[config.rs](../../../crates/vortex-core/src/config.rs)。
详见 [逻辑模块](../../architecture/logical-modules.md)；实现与验证结果以根 [README](../../../README.md) 为准。

本文定义已接入的模型与搜索配置首版契约，是实施稿的依据；运行目录见 [安装设计](../application/installation.md)，密钥处理见 [凭据设计](../cross-cutting/credentials.md)。CLI/Core/TUI 已接通 TOML、默认用户 HOME、`-m/-c` 与 `/model`。思考选项的配置、装配和请求 JSON 已补离线测试；完整配置优先级、路径与 TUI 切换专项及真实交互验收仍未完成，验证结果以根 README 为准。下文验收要求不是通过记录。

Thinking 预设、可选覆盖及显式 Enabled 的配置已接入；当前仅开放指定 DashScope Qwen
展示-only 接入，DeepSeek 开启思考尚未适配。用法见第 3 节，消费与历史边界见
[Thinking 展示设计](../tui/thinking-display.md)。

## 1. 目标与职责

用户应能在不同项目复用 Provider/模型定义，按项目收紧搜索行为，并通过启动参数或 `/model` 选择模型，而不必修改源码。

| 层 | 负责 | 不负责 |
| --- | --- | --- |
| CLI 路径解析 | HOME、cwd、配置文件和日志目录的位置 | 解析 TOML、创建模型请求 |
| Core config | 文件读取、来源记录、解析、合并和校验 | 查询环境密钥、依赖具体 HTTP/UI 实现 |
| CLI 装配 | 凭据按需读取，将模型/搜索设置映射到具体实现 | 保存会话历史、在 render 中读取配置 |
| Session/TUI | 命令确认与当前模型显示 | 登录、自动写回配置 |

首版不新增 config crate、OAuth、配置热重载、远程模型发现、配置写回、系统配置或管理员策略。本轮模型/搜索配置仅接入 TUI；旧 exec 不静默接受这些未接入的参数，未传时保持旧执行路径。HOME 与日志位置解析适用于所有本地模式，不等于 exec 已迁移到共享 Session。

## 2. 来源与优先级

```text
内置默认值
  < <vortex-home>/config.toml
  < <startup-cwd>/.vortex/project.toml
  < 重复的 -c KEY=VALUE（后者优先）
  < -m MODEL_ID
```

`<vortex-home>` 默认是用户主目录下 `.vortex`；非空绝对路径的 `VORTEX_HOME` 可覆盖它。项目文件只检查启动 cwd，不向上查找，不识别 Git 根，不改变 cwd。

例如在 `demo/src` 启动时，不会读取 `demo/.vortex/project.toml`。此行为有意保持简单、明确，未来是否增加自动发现需作为独立产品决策，不能在实现时顺便加入。

### 合并规则

- 不同 Provider/模型 ID 合并；同 ID 的条目整条替换，不逐字段拼接旧连接或旧请求选项。
- 用户/项目搜索设置只覆盖显式出现的字段；`false` 是有效值，不是缺省。
- 用户工具调用额度按工具名合并，未覆盖的内置规则保留；项目和 `-c` 不支持额度字段。
- Provider 未写 `api_flavor` 时为 `generic`；模型未写 `thinking` 时为 `provider-default`，未写预设为 None、能力覆盖逐字段为 None。运行时从全 false 基线解析预设与覆盖；整条替换不继承旧策略、预设或能力。
- 首版内置 Qwen 模型与秘塔默认搜索以兼容现有 TUI。添加其他模型不删除内置条目，不读取未选中模型的密钥。
- 配置在启动时读取一次；编辑文件需重启，不影响已经运行的 Session。

## 3. 用户配置

用户配置定义连接、模型及搜索选项，不保存密钥值。以下模型名称沿用当前源码示例，不保证远端账户可用，应由使用者确认。

```toml
default_model = "qwen-max"

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

[search]
enabled = true
backend = "metaso"
api_key_env = "METASO_API_KEY"
max_results = 5
connect_timeout_secs = 5
request_timeout_secs = 20

[tools.call_limits]
web_search = 4
list_directory = 50
```

Provider 定义协议、接口方言、地址和凭据变量名；模型 profile 定义 Provider 引用、服务端模型名、显示名、工具能力、思考策略及思考能力。同一 Provider 可供多个模型复用，但模型请求选项独立。

CLI 将方言、策略和能力装配为 `OpenAiChatOptions`，Provider 校验并转换成厂商字段。
上例发送 `enable_thinking=false`、`preserve_thinking=false`，不启用 Thinking 内容处理。
`provider-default` 省略参数，不等于关闭思考；Core 不依赖厂商 DTO，不按模型名猜测能力。
`request_options` 旧字段已拒绝，新字段与迁移规则见 [思考选项装配设计](../provider/thinking-options.md)。

`openai-chat` 明确表示 Chat Completions 协议，并使用现有 SSE/工具调用语义。仅支持 Responses 或 Anthropic Messages 的服务不能仅靠改地址接入；不支持的 kind 明确报错。

### Thinking 预设与高级覆盖

普通用户在完整模型条目中选择能力预设并设置策略，不必填写能力布尔值。例如在已有
providers.qwen 连接下新增独立模型：

```toml
[models.qwen-thinking]
provider = "qwen"
model = "qwen3.8-max"
display_name = "Qwen Thinking"
supports_tools = true
thinking_preset = "dashscope-qwen-display-only"

[models.qwen-thinking.thinking]
mode = "enabled"
```

该预设仅针对指定百炼 Qwen 候选接入，三个能力均为 true，并提供独立的展示-only
历史适用契约；不按型号或域名自动匹配，也不是远端验收记录。
api_flavor 仍须显式选对，不由预设修改；model、URL、key 和 mode 同样不由预设生成。

高级用户可以在同一模型条目下声明覆盖，例如：

```toml
[models.qwen-thinking.thinking_capabilities]
can_enable = false
```

此值明确覆盖预设的 true，因此本例 Enabled 会被标记不可用，不静默降级。
输入类型为 ThinkingCapabilitiesOverride，三个字段均为 Option<bool>：None 不覆盖、
Some(false) 覆盖 true；最终运行时能力仍为 bool。无预设时从全 false 开始应用覆盖，
旧 can_disable / supports_history_control 布尔 TOML 仍兼容。

模型新增 thinking_preset 为可选字符串，形式复用 profile ID 规则；Provider 检查已知 ID
和方言。未知预设不回退，无效闲置模型在目录标记不可用；create 在读取 key 之前拒绝。
同 ID 整条替换后不继承原预设或覆盖，只有显式选中的预设与当前条目覆盖逐字段合并。
内置默认模型使用预设，但保持 Disabled 和原请求 JSON。

能力覆盖不接受 history_policy，也不能使尚未适配的接入支持展示。即使三个 bool 全 true，
无独立历史契约的 Enabled 仍拒绝；supports_history_control 不证明工具步骤可省略回传。
项目文件和 -c 白名单不增加预设、能力或历史字段。完整契约见
[Provider 选项第 8 节](../provider/thinking-options.md#8-能力预设与覆盖)。

保存后重启，并用 `-m qwen-thinking` 或 `/model qwen-thinking` 选择该 profile；新增条目
不会自动改变 default_model。界面中思考以纯文本单独显示，正文仍按 Markdown 渲染。
ProviderDefault 不发送控制参数，但当前输出策略仍为 Reject，不能用它绕过 Enabled
校验。远端参数接受情况与真实工具往返的验收边界见根 README。

### 搜索配置

- `enabled=false`：不注册搜索工具，也不读取搜索密钥。
- `backend="mock"`：显式注册固定 Mock，不读取秘塔密钥；不是网络或认证失败的回退。
- `backend="metaso"`：读取指定环境变量，按配置构造秘塔工具。
- 秘塔请求的结果数量、归一化上限与连接/请求超时都必须使用实际配置值；Mock 仍是固定示例，不承诺模拟全部秘塔行为。
- 响应体 256 KiB、工具输出 16 KiB、来源字段与 URL 校验等现有安全边界不因此开放配置或放宽。

搜索开关只控制 `web_search`。TUI 始终注册 `list_directory`，关闭搜索后仍可查看启动
工作区的单层列表，不需要搜索 key，但仍需要模型 key 与模型工具调用能力。本轮没有
`files.enabled` 或可配置根目录；路径、隐藏项与输出限制是工具硬边界，
见 [目录工具契约](../tools/list-directory.md#目录列表list_directory-v1)。

### 工具调用额度

用户 `<vortex-home>/config.toml` 可设置 `[tools.call_limits]`，例如上例；不设置即可使用
内置 `web_search=4`，其他工具每种每 Turn 默认 50 次。`list_directory=50` 只是显式写出
默认值，不是必填字段；只覆盖它也不会移除搜索默认额度。秘塔与 Mock 都使用 web_search 名称。

每个值必须是 `1..=4294967295` 的整数；50 是可覆盖的默认值，不是显式配置的硬上限。
工具名为 1–128 个 ASCII 字母、数字、下划线或连字符；允许预先配置尚未注册的工具名，
但不会因此注册工具。零、负数、字符串、越界或非法名称均拒绝，不提供无限制语义。

额度是每工具、每用户 Turn 的尝试次数，跨模型步骤累计；新 Turn 重置，错误尝试不退额。
执行前先预检整批，超限整批不执行；未知工具不扣实际执行额度。它与搜索结果条数、
目录扫描量、超时、Token 或收费统计不同，详见 [Agent loop](../runtime/agent-loop.md)。
此字段只允许用户配置，不允许项目配置或 `-c`；修改后重启，不热重载或写回文件。
CLI 在移动 AppConfig 到模型工厂之前构造 ToolCallLimits，再注入 Session。

### 字段约束

| 项目 | 首版约束 |
| --- | --- |
| profile ID | 1–64 字节，`[a-z0-9][a-z0-9-]*` |
| Provider kind | 仅 `openai-chat` |
| base_url | 最多 2048 字节的 HTTPS API base，有 host，无空白/控制字符、userinfo、query、fragment；不含 `/chat/completions` 后缀 |
| api_key_env | 1–128 字节，`[A-Z_][A-Z0-9_]*`，不是实际密钥 |
| 服务端 model | 非空、最多 128 字节，无空白或控制字符 |
| display_name | 非空、最多 80 字符，无控制字符 |
| Provider 引用 | 必须引用已定义条目 |
| api_flavor | Provider 字段，`generic`（缺省）、`dashscope` 或 `deepseek` |
| thinking.mode | `provider-default`（缺省）、`disabled`、`enabled`；Enabled 须通过适配器与历史契约校验 |
| thinking_preset | 可选模型字段，形式同 profile ID；当前仅识别 `dashscope-qwen-display-only`，并要求 dashscope 方言 |
| thinking_capabilities | 可选 can_enable、can_disable、supports_history_control 布尔覆盖；省略不覆盖，显式 false 覆盖预设 true |
| 搜索数量/超时 | 1–5 条、连接 1–30 秒、总请求 1–120 秒；连接超时不大于总超时 |
| tools.call_limits | 仅用户配置；工具名 1–128 个 ASCII 字母/数字/下划线/连字符，值为正 u32 |
| 配置体积/数量 | 每文件最多 64 KiB，Provider 最多 16 个，模型最多 64 个且非空 |

`supports_tools` 是配置声明，不是远程探测结果。CLI 按实际注册表计算 `requires_tools`，
模型目录的不可用原因与 `create()` 使用同一条件；不兼容时先于模型 key 读取拒绝。
当前 TUI 始终有目录工具，因此关闭搜索也必须选择 `supports_tools=true` 的模型，
不静默移除工具来兼容模型。其他模型可以展示为不可选择，并说明能力不匹配。
选中模型构造时才读取其 key，不因闲置 profile 缺密钥阻止启动。

## 4. 受限项目配置

```toml
default_model = "qwen-max"

[search]
enabled = false
max_results = 3
```

只允许：

- `default_model`：引用内置/用户层已有的模型 ID。
- `search.enabled`：允许关闭；不能重新开启用户层已关闭的搜索。
- `search.max_results`：允许降低已有条数，不得提高或设为零。

项目不能新增或修改 Provider、模型定义、接口方言、思考策略或能力、密钥变量名、服务地址、搜索后端、超时和工具调用额度。未知或禁止字段必须报错，不能静默忽略。用户显式 CLI 覆盖仍可改变搜索开关/数量，但必须满足产品硬限制。

这种字段限制不等于完整 workspace trust：用户层默认可信，项目仍可能从已有 profile 中改变本次请求目标，因此启动信息必须显示最终模型与 Provider。开启搜索会将查询发送给搜索服务；切换 Provider 后，后续请求会把当前历史发送给新服务。

项目目录和文件不得为符号链接；存在时确认 canonical 目标仍在启动目录边界内，Windows 另测 junction。首版只支持普通配置文件，不接受目录、设备或管道，不声称具备完整 TOCTOU 防护。

用户配置可来自用户主动管理的位置，但程序不验证作者身份。将 HOME 指向仓库内 `.vortex` 仅适用于自有开发环境，不应使陌生仓库文件获得用户层信任。

## 5. CLI 覆盖与会话切换

`-m` 与 `/model` 接受模型 profile ID，而不是临时服务端模型字符串。`-c` 使用 TOML 字面量，白名单仅包括：

```text
default_model
search.enabled
search.backend
search.max_results
search.connect_timeout_secs
search.request_timeout_secs
```

例如 `-c 'default_model="qwen-max"'`、`-c 'search.enabled=false'`；字符串需 TOML 引号。当前 CLI 仅在 TUI 分支将这些参数交给加载器；exec 收到任意 `-m/-c` 返回非零错误，不静默忽略。完整目标参数见 [CLI 设计](../application/cli-surface.md)。

`/model` 只改变当前会话绑定，不修改配置或加载时的默认模型；重启仍按配置与启动覆盖
选择。它接受已有 profile ID，不创建临时模型定义，不承担登录或密钥管理。
目录/工厂/选择与确认以 [模型管理](../runtime/model-management.md) 为准，窗口与草稿规则见
[TUI 接口](../tui/interface.md#模型窗口与确认链)。

## 6. 配置来源与可观测性

路径由 CLI 解析一次，Core 加载结果同时返回配置与来源状态。准备启动 TUI 时在进入 raw mode 前，将以下信息写到 stderr，并记录结构化日志：

```text
工作目录: /work/demo/src
Vortex HOME: /home/ming/.vortex
用户配置: /home/ming/.vortex/config.toml（已加载或不存在）
项目配置: /work/demo/src/.vortex/project.toml（已加载或不存在）
模型: qwen-max | Provider: qwen | 目标: dashscope.aliyuncs.com
```

这里是输出内容示例，不要求固定英文/中文日志字段的排版；“不存在”不是“已加载默认文件”。没有项目配置时，提示只检查启动目录、不会向父目录查找。TUI 状态栏显示已确认的模型；首版不新增 `/config` 或诊断子命令。

只输出路径、来源状态、profile ID、后端/host 等元数据；终端显示须转义路径中的控制字符。不 dump 整个 AppConfig、实际凭据、Prompt、查询或响应正文。记录 CLI 来源时只记已生效的配置键，不回显原始覆盖字符串或敏感错误行。

## 7. 读取、校验与启动失败

1. 解析已有 CLI 参数，`--help/--version` 不需要创建 HOME 或读取配置。
2. CLI 解析 HOME、cwd 和固定配置路径；初始化日志。
3. 通过 `spawn_blocking` 在阻塞池中逐文件读取并检查 UTF-8、TOML、字段和体积。缺失可跳过，损坏/不可读必须报错，不回退另一个位置。
4. 合并用户层并校验定义、引用、数值边界；校验受限项目覆盖，再应用 CLI 覆盖。
5. 完整合并后，Core 校验最终默认模型存在，并保留启用搜索时的能力约束。不得在 `-m/-c`
   应用前因为未选中的默认模型不匹配而提前失败；关闭搜索不代表最终无需工具能力。
6. 记录配置来源；CLI 在阻塞池打开启动 cwd，注册目录工具及可选搜索，计算实际工具能力
   要求并装配模型。搜索需要时才读取搜索 key；模型能力校验先于模型 key 读取，目录打开、
   工具或默认模型装配失败均在进入 TUI 前返回。Core config 不认识具体目录工具。

配置解析不读取密钥；未使用模型的 key 可以缺失。搜索关闭/Mock 不要求秘塔 key。搜索关闭时仍检查字段定义和范围。错误应指出路径/安全字段原因，但不透传可能含输入原文的 TOML 错误或 HTTP 正文。

## 8. 内置参数与系统策略

内置默认值允许在公开范围内覆盖；响应大小、工具输出边界、协议支持等硬限制不属于普通配置字段。普通用户配置、项目文件和 CLI 都不能突破硬限制；具体 Tool 构造/执行入口仍需独立校验，不能只依赖 config。

首版不增加外置“系统配置”或隐藏 `system.toml`。未来若有企业需求：系统默认值可作为普通低优先级层；管理员策略应是独立约束，受系统权限保护、不可通过 CLI 或换 HOME 绕过。两者不混用，也不作为当前已实现能力。

## 9. 验收要求

- 内置/用户/项目/重复 `-c`/`-m` 的覆盖、整条替换及显式 false 正确。
- CLI 可修正最终模型选择或关闭搜索；关闭搜索不解除目录工具的模型能力要求，损坏文件和非法定义仍严格失败。
- 项目越权、非法链接/路径、配置来源与 cwd-only 行为可重复验证。
- 相同 Provider 的不同模型请求选项互不串用；选项省略和旧位置有明确结果。
- 模型/搜索配置实际进入请求和工具限制，不仅改变 UI 标签。
- tools.call_limits 按名称合并、保留默认项；非法值拒绝，项目和 `-c` 不能覆盖；新 Turn 重置计数。
- `/model` 成功/失败、Running 拒绝、保留历史、不写文件、重启恢复默认值。
- 文件与 API 测试使用 TempDir、fake 和本地 fixture，不调用真实付费接口；Windows 路径和权限在 Windows 单独验证。

## 旧思考配置迁移

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

### DeepSeek 官方接入示例

原凭据文档中的配置示例集中保留于此。连接与模型分开定义，api_key_env 只保存变量名；
服务端型号仅为示例，使用前须核对账户可用名称与权限，不代表已调用或验证远端。

```toml
[providers.deepseek]
kind         = "openai-chat"          # OpenAI 兼容的 Chat Completions
api_flavor   = "deepseek"             # 官方接口；托管接口须按其实际方言配置
base_url     = "https://api.deepseek.com"
api_key_env  = "DEEPSEEK_API_KEY"     # 变量名，不是值

[models.deepseek-pro]
provider = "deepseek"
model = "deepseek-v4-pro"             # 示例，须确认账户实际可用名称
display_name = "DeepSeek Pro"
supports_tools = true

[models.deepseek-pro.thinking]
mode = "disabled"

[models.deepseek-pro.thinking_capabilities]
can_disable = true
supports_history_control = false
```

# 配置模块产品设计

本文定义模型与搜索配置首版的用户行为和安全边界，是实施稿的依据；运行目录见 [安装设计](installation.md)，密钥处理见 [凭据设计](credentials.md)。本文是目标设计，当前源码尚未接入 TOML 加载、默认用户 HOME 或 `/model`；真实进展以根 README 为准。

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
- 模型未写 `request_options` 时为 `plain`；替换条目后不保留旧预设。
- 首版内置 Qwen 模型与秘塔默认搜索以兼容现有 TUI。添加其他模型不删除内置条目，不读取未选中模型的密钥。
- 配置在启动时读取一次；编辑文件需重启，不影响已经运行的 Session。

## 3. 用户配置

用户配置定义连接、模型及搜索选项，不保存密钥值。以下模型名称沿用当前源码示例，不保证远端账户可用，应由使用者确认。

```toml
default_model = "qwen-max"

[providers.qwen]
kind = "openai-chat"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
api_key_env = "QWEN_API_KEY"

[models.qwen-max]
provider = "qwen"
model = "qwen3.8-max"
display_name = "Qwen Max"
supports_tools = true
request_options = "qwen-stage-a"

[search]
enabled = true
backend = "metaso"
api_key_env = "METASO_API_KEY"
max_results = 5
connect_timeout_secs = 5
request_timeout_secs = 20
```

Provider 定义协议、地址和凭据变量名；模型 profile 定义 Provider 引用、服务端模型名、显示名、能力和请求预设。同一 Provider 可供多个模型复用，但模型请求选项独立。

`qwen-stage-a` 由 CLI 映射为现有 Qwen 参数 `enabled=false`、`preserve_history=false`，不启用 Thinking。Core 不依赖 Qwen DTO，也不按模型名称猜测选项。

`openai-chat` 明确表示 Chat Completions 协议，并使用现有 SSE/工具调用语义。仅支持 Responses 或 Anthropic Messages 的服务不能仅靠改地址接入；不支持的 kind 明确报错。

### 搜索配置

- `enabled=false`：不注册搜索工具，也不读取搜索密钥。
- `backend="mock"`：显式注册固定 Mock，不读取秘塔密钥；不是网络或认证失败的回退。
- `backend="metaso"`：读取指定环境变量，按配置构造秘塔工具。
- 秘塔请求的结果数量、归一化上限与连接/请求超时都必须使用实际配置值；Mock 仍是固定示例，不承诺模拟全部秘塔行为。
- 响应体 256 KiB、工具输出 16 KiB、来源字段与 URL 校验等现有安全边界不因此开放配置或放宽。

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
| request_options | 模型字段，`plain` 或 `qwen-stage-a` |
| 搜索数量/超时 | 1–5 条、连接 1–30 秒、总请求 1–120 秒；连接超时不大于总超时 |
| 配置体积/数量 | 每文件最多 64 KiB，Provider 最多 16 个，模型最多 64 个且非空 |

`supports_tools` 是配置声明，不是远程探测结果；开启搜索时最终选择的模型必须声明工具支持。其他模型可以展示为不可选择，并说明能力不匹配。选中模型构造时才读取其 key，不因闲置 profile 缺密钥阻止启动。

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

项目不能新增或修改 Provider、模型定义、请求预设、密钥变量名、服务地址、搜索后端和超时。未知或禁止字段必须报错，不能静默忽略。用户显式 CLI 覆盖仍可改变搜索开关/数量，但必须满足产品硬限制。

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

例如 `-c 'default_model="qwen-max"'`、`-c 'search.enabled=false'`；字符串需 TOML 引号。命令参数规则见 [CLI 设计](cli-surface.md)，本节并不宣称已有可用实现。

`/model` 的首版契约：

- 打开选择窗口，或 `/model <id>` 申请选择已有模型；命令本身不作为 User 消息发给模型。
- 只允许空闲/失败且没有活动 Turn 时切换；运行中明确拒绝。
- 保留历史，每个 Turn 固定使用启动时克隆的 Provider；Core 成功确认后更新界面。
- 失败保留原绑定和草稿；成功不修改配置文件、不改变加载时的默认值。
- 界面明确标示“仅本会话”；重启按配置及启动参数重新选择，不继承上一次内存绑定。
- 成功表示本地配置与凭据装配完成，不保证远端已鉴权；远端失败仍走正常 Turn 错误链。

首版不提供“设为默认”、登录或密钥管理动作。

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
3. 逐文件读取并严格检查 UTF-8、TOML、字段和体积。缺失可跳过，损坏/不可读必须报错，不回退另一个位置。
4. 合并用户层并校验定义、引用、数值边界；校验受限项目覆盖，再应用 CLI 覆盖。
5. 完整合并后才校验最终默认模型是否存在、是否与工具能力兼容。不得在 `-m/-c` 应用前因为未选中的默认模型不匹配而提前失败。
6. 记录配置来源，按最终配置装配工具和选中模型，读取必要密钥；所有失败发生在进入 TUI 前。

配置解析不读取密钥；未使用模型的 key 可以缺失。搜索关闭/Mock 不要求秘塔 key。搜索关闭时仍检查字段定义和范围。错误应指出路径/安全字段原因，但不透传可能含输入原文的 TOML 错误或 HTTP 正文。

## 8. 内置参数与系统策略

内置默认值允许在公开范围内覆盖；响应大小、工具输出边界、协议支持等硬限制不属于普通配置字段。普通用户配置、项目文件和 CLI 都不能突破硬限制；具体 Tool 构造/执行入口仍需独立校验，不能只依赖 config。

首版不增加外置“系统配置”或隐藏 `system.toml`。未来若有企业需求：系统默认值可作为普通低优先级层；管理员策略应是独立约束，受系统权限保护、不可通过 CLI 或换 HOME 绕过。两者不混用，也不作为当前已实现能力。

## 9. 验收要求

- 内置/用户/项目/重复 `-c`/`-m` 的覆盖、整条替换及显式 false 正确。
- CLI 可修正最终模型选择或关闭搜索，损坏文件和非法定义仍严格失败。
- 项目越权、非法链接/路径、配置来源与 cwd-only 行为可重复验证。
- 相同 Provider 的不同模型请求选项互不串用；选项省略和旧位置有明确结果。
- 模型/搜索配置实际进入请求和工具限制，不仅改变 UI 标签。
- `/model` 成功/失败、Running 拒绝、保留历史、不写文件、重启恢复默认值。
- 文件与 API 测试使用 TempDir、fake 和本地 fixture，不调用真实付费接口；Windows 路径和权限在 Windows 单独验证。

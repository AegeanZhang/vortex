# 上手指南

本文说明 Vortex 的构建、运行入口。产品 TUI 使用文本 SSE、工具循环、目录列表与秘塔网页搜索，
此前搜索正常路径已由作者人工跑通。本轮分层配置与模型选择已接入源码并通过编译/Review，
思考预设、展示-only 事件链、目录工具及装配/模型能力判断已补离线测试。
作者反馈 Qwen 诗词请求的 Thinking 可显示，Thinking 工具往返、完整配置/切换矩阵与
真实终端验收仍待补充。以下配置用法描述当前代码，不表示所有接入或异常场景均已验收。
完整验证边界以根 README 为准。其他命令和后续能力的
目标形态见 [`../design/application/cli-surface.md`](../design/application/cli-surface.md)，不要把设计文档中的
未实现部分当成现有功能。

## 环境要求

- 稳定版 Rust，edition 2024。开发环境实测版本为 `cargo 1.98.0` / `rustc 1.98.0`。
- Linux。沙箱能力将依赖 `bubblewrap`，但相关功能尚未实现，现阶段不需要安装。

版本确认：

```bash
cargo --version
rustc --version
```

## 构建

```bash
cargo build --locked
```

根 Workspace 只包含 `crates/*`，所以 `cargo build` 和根目录的 `cargo build --workspace`
都只编译产品 crate。所有成员使用根目录 `Cargo.lock` 和 `target/`；锁文件应提交，
构建目录被 Git 忽略。编程语言练习由独立项目维护，不属于本仓库。

## 提交前的检查

与 `AGENTS.md` 中的约定一致，四条命令依次检查格式、静态问题、完整测试和发布构建：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --release --locked
```

## 产品命令的当前状态

没有配置文件时，内置默认值仍为 Qwen profile `qwen-max` 和秘塔搜索。此默认路径需要
两份独立 key；下面只是占位示例，不要提交真实值，也不要自动运行真实付费请求：

```bash
export QWEN_API_KEY='<your-api-key>'
export METASO_API_KEY='<your-search-key>'
cargo run -p vortex-cli --locked
```

产品本身不读取 `.env`。输入 Prompt 后按 Enter 提交，输入 `/exit` 后按 Enter 退出。
当前没有 Ctrl+C 退出映射；内置 Qwen profile 使用 DashScope 方言、disabled 策略及
显式能力预设，关闭思考及思考历史。模型连接和搜索选项来自分层 TOML，不再要求修改 CLI 源码。
HOME 默认是原生用户主目录下 `.vortex`，无需设置 VORTEX_HOME；显式覆盖必须为非空
绝对路径，例如 `export VORTEX_HOME="${HOME}/.vortex-dev"`。启动时在进入 TUI 前显示
cwd、HOME、两条配置路径及 loaded/missing 状态、最终模型/Provider/host。

配置缺失使用内置值，不自动创建配置模板；只创建必要日志目录。坏配置、缺少实际需要的
key 或日志初始化失败会在进入 TUI 前报错。真实模型与终端验收记录以根 README 为准。

### 用户配置与受限项目配置

用户文件为 `<vortex-home>/config.toml`：Linux/macOS 通常是 `~/.vortex/config.toml`，
Windows 原生是用户主目录下 `.vortex\config.toml`，WSL 默认使用自己的用户目录。
以下是完整用户配置示例；远端模型名应以账户实际开通情况为准：

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
```

Provider 定义连接、接口方言和密钥变量名；模型 profile 定义服务端名称、能力和请求选项。可按相同
结构添加多个 profile，再用 `-m` 或 `/model` 选择其 ID，不直接输入服务端模型名。
`supports_tools` 是用户声明而非远端探测。当前 TUI 始终注册目录工具，即使关闭搜索，
所选模型也必须声明 `supports_tools=true`；不兼容 profile 在 `/model` 列表中标为不可选。
`thinking.mode` 缺省为 `provider-default`，省略参数而不是关闭思考；该模式与 disabled
均拒绝意外返回的思考增量。显式 disabled 要求解析后 can_disable=true，并与接口方言匹配。DeepSeek 官方与
百炼托管接入的参数不同，不能仅按模型名称配置；完整示例与旧字段迁移见
[思考选项设计](../design/provider/thinking-options.md)。旧 request_options 字段已不接受。

### 开启 Qwen Thinking

在上述用户配置中新增完整模型条目，复用已有 `providers.qwen`：

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

预设提供能力与展示-only 历史契约，通常不用填写 thinking_capabilities。仅写 enabled
或手填三个 true 不够；该预设也不能用于 DeepSeek。原有无预设的关闭配置仍可使用
显式 can_disable/supports_history_control，不强制迁移。

保存配置后重启，再通过 `/model qwen-thinking` 选择，或启动时指定：

```bash
cargo run -p vortex-cli --locked -- -m qwen-thinking -c 'search.enabled=false'
```

该命令仍调用真实模型并可能计费；关闭搜索只是不调用搜索服务。可先用诗词等纯文本
请求检查 Thinking 标签、弱化纯文本与 Markdown 正文。默认模型不会因新增 profile 改变。
思考的业务展示仅存本次 TUI 内存，不回传或恢复；显式开启下文 wire TRACE 时，响应中的
思考会进入诊断文件。当前 DeepSeek Enabled 不可用；Qwen 搜索可能
触发单次响应多工具调用限制，详见 [Thinking 设计](../design/tui/thinking-display.md#人工验收与已知限制)。

### 项目配置与加载规则

项目文件只检查启动 cwd 下的 `.vortex/project.toml`，不读取父目录或 Git 根。示例：

```toml
default_model = "qwen-max"

[search]
enabled = false
max_results = 3
```

项目只能引用已有模型、关闭搜索或降低用户层的条数，不能新增 endpoint、凭据变量名、
模型、接口方言、思考策略或能力。项目目录/文件不允许符号链接；完整信任系统和 Windows junction 验收
尚未完成。用户 HOME 默认可信，不要将陌生仓库中的 `.vortex` 指定为用户配置来源。

优先级是内置 → 用户 → 受限项目 → 重复 `-c`（后者优先）→ `-m`。用户层同 ID 条目
整条替换；配置在启动时读取一次，修改后需重启，不自动写回。

### 启动覆盖与会话内模型选择

下列命令仍需要选中模型的 key，但关闭搜索或显式 Mock 不读取秘塔 key：

```bash
cargo run -p vortex-cli --locked -- -m qwen-max -c 'search.enabled=false'
cargo run -p vortex-cli --locked -- -c 'search.enabled=true' -c 'search.backend="mock"'
# 使用秘塔时才生效；search.enabled 仍须为 true，且需要搜索 key。
cargo run -p vortex-cli --locked -- -c 'search.max_results=3'
```

关闭搜索只移除 `web_search`，`list_directory` 仍可用；不是完全无工具模式。

`-c` 只允许 `default_model`、`search.enabled/backend/max_results/connect_timeout_secs/request_timeout_secs`，
值按 TOML 解析；字符串要带 TOML 双引号，例如 `-c 'default_model="qwen-max"'`。
它不接受 endpoint、实际 key 或日志路径覆盖。

输入 `/mo` 后 Tab/Enter 只补全 `/model`；完整 `/model` 后 Enter 打开模型窗口，方向键
移动、Enter 申请、Esc 关闭并保留草稿。也可直接提交 `/model qwen-max`。
状态栏在 Core 确认后更新 profile ID；Running 时拒绝切换，缺 key 等失败保留原模型。
等待确认期间不重复选择或提交普通 Prompt；Esc 后可继续编辑，异步成功不会清掉新草稿。
若要退出窗口先 Esc，再提交 `/exit`。

切换仅本会话、保留历史、不修改默认配置；后续历史可能发送给新 Provider。重启按配置
和启动覆盖重新选择。确认成功只表示本地装配，不代表远端鉴权成功。该交互仍需专项
回归和真实终端验收；旧菜单测试已完成适配，不能将旧测试通过等同于新配置/切换链路
的专项验收完成。实际检查记录见根 README。

源码中的响应链路已改为 SSE 增量展示，不再等待完整回答后一次显示；它按服务端增量更新，
不是本地逐字动画。CLI 默认注册的 `web_search` 调用秘塔独立网页搜索 API，不调用问答
接口、不抓取来源全文；搜索结果回填给当前选中的模型，再由模型继续回答。查询会发送给秘塔，Prompt、
工具定义与工具结果会发送给当前选中模型，两个服务可能分别计费；不要在查询中放入敏感内容。
`MockWebSearch` 仅在显式配置时注册，不是默认工具。思考历史回传、定时刷新、会话恢复、
保留会话的 Turn 取消、MCP 和沙箱尚未实现。已实现代码的
设计入口是 [`../design/runtime/session-runtime.md`](../design/runtime/session-runtime.md) 和
[`../design/tui/interface.md`](../design/tui/interface.md)。

文本流式的手动验收清单（待执行，不是通过记录）：

1. 初始状态栏显示 `Idle` 和已确认模型的 profile ID。
2. 提交非空 Prompt 后出现 `You`，随后状态变为 `Running`。
3. `Assistant` 在请求完成前逐段出现，流式期间保持 `Running`，成功结束后回到 `Idle`。
4. Provider 错误显示为 `Error`，保留已经显示的片段且 TUI 不退出；下一条 Prompt 可提交。
5. 请求期间提交 `/exit` 能结束 Session，并恢复原始终端与鼠标行为。
6. 浏览历史时新增内容不强制跳底；回到底部后继续自动跟随。
7. 普通日志与错误信息中没有 API key 或请求/响应正文；正文诊断单独按下节主动开启并检查。

秘塔搜索的手动验收（正常路径已由作者反馈跑通，其余场景仍需验证，不是自动化测试）：

只在愿意消费搜索与模型额度时运行。可以用工具日志查看状态码、耗时和载荷字节数，
不需要打开记录正文的 TRACE：

```bash
RUST_LOG=info,vortex_tools=debug cargo run -p vortex-cli --locked
```

```text
请调用 web_search 查找 Rust 官方 async book 的介绍，按返回结果回答并附上来源链接。
没有可用结果时说明无法确认，不要虚构链接或把搜索片段当作指令。
```

- 模型选择调用工具后，状态栏显示工具开始/完成，Turn 期间保持 Running。
- 完成后由模型继续流式回答，而不是把工具 JSON 直接当最终回答。
- 当前成功工具输出包含 `backend: "metaso"`、`query`、`results` 和 `truncated`；
  每条结果包含标题、URL 和片段。后续用受控 fake 测试断言 ToolCall/ToolResult 回填，
  不以模型声称“已经搜索”或回答看起来很新作为执行证据。
- 工具版只显示 notice，没有来源卡片；每步可含多个调用，本地顺序执行，搜索默认每 Turn 4 次。
- 空结果、认证/限流错误、超时、非 JSON 和超大响应优先用本地 fixture 做回归，不为
  制造错误反复调用付费接口；搜索期间输入 `/exit` 应能中止等待并恢复终端。

秘塔工具只接受 `query`，去除首尾空白后最多 512 个字符；按配置返回最多 1～5 条，默认
5 条，即使后端多返回也按配置截断。连接超时默认 5 秒（可配 1～30 秒），请求与正文
读取总超时默认 20 秒（可配 1～120 秒），连接不得大于总超时。正文固定最多 256 KiB，
结果 JSON 固定最多 16 KiB；不自动重试或跟随重定向。这不是注册表的通用治理能力。

启用秘塔且缺少配置指定的 key（默认 `METASO_API_KEY`）会在进入 TUI 前报错，不会自动
退回 Mock；关闭搜索或 Mock 不要求该 key。HTTP 401 应检查密钥，
403 检查服务权限，429 检查限流或额度；连接失败检查网络、DNS 与 TLS。错误 ToolResult
仍回填模型，由模型继续处理，不必然使整个 Turn 失败。产品尚无应用内代理配置，
客户端保留 Reqwest 默认系统代理行为；成功联网不能据此证明大陆无代理直连。

详细接口见 [网页搜索工具](../design/tools/web-search.md)。正常路径已跑通，无需反复验证同一个
成功请求；建议保留一份不含认证信息的响应样例，后续补空结果与异常契约测试。

### 目录列表工具

TUI 默认注册 `list_directory`，不需要额外文件系统配置或搜索凭据。根目录固定为启动
cwd，不是 Git 根、Cargo manifest 目录或 Vortex HOME；切换启动目录会改变它能查看的范围。
工具只列直接子项，不读正文、不写文件、不执行 Shell，也不递归或分页。

参数形状示例：

```json
{"path":".","limit":100}
```

路径必须相对根目录，用 `/` 分隔；也可指定 `crates/vortex-tools/src`。`limit` 默认 100、
最大 200。绝对路径、`..`、任意层符号链接及部分不便携名称被拒绝；`.git`、`.vortex`、
`.env` 和 `.env.*` 隐藏且禁止访问，其他隐藏项不一律排除，不读取 `.gitignore`。

返回值包含相对 `path`、子项 `name/type`、`truncated` 和 `truncation_reasons`。
最多扫描 4096 项，最终 JSON 最多 16 KiB；截断意味着列表不完整，重复调用不保证获得
下一页。优先缩小到明确子目录，逐层探索受目录工具独立额度限制，默认每 Turn 50 次。
TUI 不直接显示文件树或原始工具 JSON，模型接收结果后生成回答。

离线用例已验证目录工具与装配，真实模型/TUI 目录往返仍待人工验收。例如，在自己明确
选择的测试目录启动后输入：`请调用 list_directory，列出当前目录的直接子项，不读取文件。`
仅凭模型声称“已列出”不能证明工具执行，应核对实际调用与回填链路。

只读也有外发风险：文件名、目录名与工具结果会发送给当前模型供应商；固定隐藏规则不是
完整秘密检测器。只在自己明确选择的目录运行，不开启正文 TRACE 来分享真实目录数据。
工具尚无审批系统、进程沙箱或硬 I/O 超时，已开始的阻塞读取不会因 `/exit` 强制终止，
挂起的网络盘可能拖延退出。Linux 离线测试不代表 Windows junction 等边界已验收。
完整契约见 [目录列表工具](../design/tools/list-directory.md)。

### 工具调用额度

无需新增配置即可使用默认额度：每个用户 Turn 内，web_search 最多 4 次，其他工具每种
最多 50 次。用户 `<vortex-home>/config.toml` 可显式覆盖，例如：

```toml
[tools.call_limits]
web_search = 4
list_directory = 50
```

配置按名称合并，值必须是正 u32，可超过 50；项目 project.toml 和 `-c` 不支持这些字段，
修改后需重启。新输入重新计数；已注册工具失败也占一次，未知工具不扣实际执行额度。
模型同批请求超过剩余额度时，整批拒绝执行，本轮显示预算错误；不会只执行能容纳的部分。
额度不等于模型请求数、Token 或费用限制，规则见 [配置设计](../design/configuration/configuration.md#工具调用额度)。

### 选择与复制文本

在 Prompt 或 Transcript 内容区左键拖选后按 Ctrl+C；Prompt 也支持 Shift+方向键选择再复制。
状态栏提示成功或失败，草稿保持不变。Esc 取消选区；菜单打开时先关闭菜单。复制不会提交、
退出或调用模型。单次最多 1 MiB，后台写入期间仍可退出。

鼠标选择复制当前可见的渲染文本，包含选中的角色/Thinking 内容，不含外框或状态栏；
保留视觉换行和缩进，去除屏幕填充的尾部空格。滚动、Resize 或选中区域内容变化会取消旧选区。
需要复制较长的 Prompt 时可使用键盘选区；鼠标选择不覆盖屏幕外历史。

Linux 需要可用的 X11 或支持 data-control 的 Wayland 会话。SSH/tmux/无图形环境可能没有
系统剪贴板；失败时可用终端自身的选择复制（Kitty：Shift 拖选、Ctrl+Shift+C）。程序不自动
发送 OSC 52 或转发远程剪贴板。Linux 没有剪贴板管理器时退出后内容可能丢失。
Windows/macOS 和原生 Wayland 尚待验收；复制支持不代表已支持 Paste/bracketed paste。
详见 [文本选择设计](../design/tui/text-selection.md)。

### 旧单次模式的边界

现有 exec 未迁移到流式 Session/配置/工具链路，不提供该目录工具，不要求秘塔 key，
收到 `-m/-c` 明确返回
非零错误；不传时仍走旧执行路径，也不使用本节清单验收。完整命令目标
（exec 模式、`vortex resume`、审批与沙箱参数、输出契约、退出码）记录在
[`../design/application/cli-surface.md`](../design/application/cli-surface.md)，不可把其中所有参数视为已经实现。

## 日志与本地配置

正常执行 TUI/exec 时共用 AppPaths 的日志目录，以 JSON 行追加到 `<vortex-home>/logs/vortex.log`，不向 TUI
屏幕打印日志。新建日志文件在 Unix 上使用 0600 权限，已有文件权限不会因此自动收紧。
目前没有自动轮转；日志不是会话 JSONL，不提供恢复功能。`--help/--version` 在路径/日志
初始化之前由 clap 处理。日志初始化只创建必要目录，不生成配置、sessions/state/cache。

默认日志级别为 info，未传 `-v` 时使用 RUST_LOG；单个 `-v` 会强制 debug，覆盖该环境
变量。当前 CLI 的 verbose 是 bool，不支持目标设计中的多次递增；没有 quiet 参数。

```bash
RUST_LOG=info,vortex_core=debug cargo run -p vortex-cli --locked
cargo run -p vortex-cli --locked -- -v
```

已移除 `app.rs` 提交分支记录输入原文的 TRACE，但尚未完成全链路日志/错误脱敏验收。
不要为排障默认记录正文；前面的脱敏验收项仍需按实际日志与错误路径检查。
`-c/--config` 已用于 TUI 的模型/搜索配置白名单，但不调整 HOME 或日志路径；HOME 使用
VORTEX_HOME 或默认用户目录，日志级别仍通过 RUST_LOG/`-v` 控制。

### 开发期请求与响应诊断

默认 Debug 构建可主动开启 `vortex_wire`，查看最终请求 JSON、逐个 SSE data 与秘塔原始响应。
在本人选择的测试工作目录运行；下列命令会调用真实模型，搜索启用时还可能调用秘塔并计费：

```bash
RUST_LOG=info,vortex_wire=trace cargo run -p vortex-cli --locked
```

不要同时加 `-v`，它会强制 debug，覆盖上述 TRACE 过滤。使用本地启动脚本时可将同样的
RUST_LOG 导出到脚本运行环境，确认脚本不覆盖它。默认 Release 不编译这些诊断点，单改
RUST_LOG 无法启用；不要为排障默认调整 release 的 debug-assertions。

日志仍在 `<vortex-home>/logs/vortex.log`，无需单独指定诊断目录。若已安装 jq，可在另一终端查看：

```bash
# 这里的 HOME 规则与产品一致；显式 VORTEX_HOME 应为绝对路径。
log_path="${VORTEX_HOME:-${HOME}/.vortex}/logs/vortex.log"
tail -F "$log_path" | jq --unbuffered 'select(.target == "vortex_wire") | .fields'
# 查看请求 DTO；body 在 JSON 日志内是一段字符串。
jq 'select(.target == "vortex_wire" and .fields.phase == "request") | .fields.body | fromjson' "$log_path"
```

用 request_id 关联同一 HTTP 请求，seq 表示诊断事件顺序；同一 Turn 的模型/搜索请求各有
独立 ID。SSE body 中的 `[DONE]` 不是 JSON，不要对所有 body 无条件使用 fromjson。
日志只保存完整 Event.data，不是网络抓包，也不能据此判断服务端是否发送了 SSE id/event 字段。

正文单条最多 64 KiB、每请求累计最多 4 MiB；注意 truncated、capture.limit 和序号缺口。
错误正文最多预览 16 KiB/300 ms，独立任务不阻塞原错误；进程退出或日志队列拥塞可能丢失记录。
这些预算不限制日志文件累计大小，当前没有轮转。

已知 key 只作字面值替换，不保证转义/跨包密钥或个人信息被清除；开启后可能落盘完整
Prompt、历史、思考和工具结果。仅使用本人可控数据，分享前检查日志，不把它当作隐私过滤器
或会话恢复文件。字段、生命周期与离线验证契约见 [诊断设计](../design/cross-cutting/diagnostics.md)。

## 文档导航

各类文档的用途见 [`../README.md`](../README.md)。

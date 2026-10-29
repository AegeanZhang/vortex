# Vortex

Vortex 是一个使用 Rust 编写、以学习实现细节为目的的终端 Agent，目标体验类似 Codex CLI 和 Claude Code：理解自然语言任务，在用户控制下读取工作区、调用工具、执行命令并持续完成任务。

> 当前状态：TUI 已接入文本 SSE、工具闭环、工作区单层目录列表与秘塔网页搜索。一个 Turn 可经历多次模型请求与
> 工具执行，Provider 组装完整 ToolCall，Core 回填工具结果，TUI 流式展示 Assistant Markdown。
> TUI 已接入分层 TOML、模型 profile 与 `/model` 会话内切换，搜索可关闭或选择秘塔/Mock。
> 默认注册 `list_directory`，再按搜索配置选装秘塔/Mock 的 `web_search`；关闭搜索不会关闭
> 目录工具。作者此前已反馈秘塔正常路径可运行；目录工具、装配与模型能力约束已补离线
> 测试，真实模型目录往返及其他专项验收仍待补充。

Transcript 已接入 Assistant Markdown、键盘翻页与鼠标滚轮；已补 Thinking 与正文混排、
长中文滚动和窗口大小变化的离线回归，滑块视觉断裂问题暂缓处理。默认 Qwen profile
仍显式关闭思考及思考历史；独立 Qwen Thinking profile 可通过显式预设开启展示-only
思考，按模型步骤显示且不回填模型历史。DeepSeek 开启思考尚未接入。
契约与限制见 [`Thinking 展示设计`](docs/design/tui/thinking-display.md)。
Shutdown 可通知 Session 中止活跃流，但尚无保留会话的 `CancelTurn`；exec 未迁移到流式链路。

共享 `vortex-diagnostics` 已接入模型、秘塔与旧 exec 的请求/响应诊断：仅 Debug 且主动开启
`vortex_wire=trace` 时记录正文副本，默认 Release 不包含相关调用。普通日志仍无正文。
记录有截断与有限密钥字面值替换，可能包含 Prompt、历史、思考及工具数据；
不是完整抓包或会话存储。用法和边界见 [诊断设计](docs/design/cross-cutting/diagnostics.md)。

Slash 支持 `/exit`、`/model` 与 `/model <profile-id>`：草稿首字符为 `/` 时显示根命令菜单，
Tab 只补全；`/mo` 按 Enter 先补为 `/model`，再次按 Enter 打开独立模型窗口。
窗口用 Up/Down 选择、Enter 申请、Esc 关闭并保留草稿；Core 确认前不更新当前模型。
切换保留历史、仅作用于本会话且不写配置，下一轮可能将已有历史发送给新选择的服务；
运行中拒绝切换，失败保留原绑定。模型切换 pending 期间阻止普通 Prompt 与重复选择，
关闭窗口后仍可提交 `/exit`；完整 `/exit` 在 Idle/Running/Failed 均复用 Shutdown → Exit。
未知命令、非法参数和多行 slash 不发送给模型；普通 Prompt 在已知 Running 状态下保留草稿。
Resize 主动重绘，按键 Release 与确认键 Repeat 已过滤；Paste、动态多行高度及更多命令
仍未实现。完整契约见 [`TUI 接口设计`](docs/design/tui/interface.md#slash-命令与菜单)。

工具版 v1 每个模型步骤只接受一个调用，一个 Turn 最多执行 4 次工具调用；第五次会失败。
工具错误作为 `is_error = true` 的结果回填给模型，空白或重复调用 ID 则直接使 Turn 失败。
TUI 仅用状态栏展示工具开始/完成，不显示工具卡片或来源引用。设计见
[Agent loop](docs/design/runtime/agent-loop.md) 与 [网页搜索设计](docs/design/tools/web-search.md)。秘塔工具已实现参数校验、超时和
响应/输出大小限制，但注册表尚无通用 Schema 校验、工具超时或预算治理。
思考历史回传、会话恢复、网页正文读取、MCP 和沙箱仍未实现。

`list_directory` 只列启动 cwd 内指定目录的直接子项，不执行 Shell、不读正文、不写文件。
相对路径访问逐级拒绝符号链接，隐藏 `.git`、`.vortex`、`.env` 与 `.env.*`；条数、扫描量
与 JSON 输出有独立上限。没有递归、分页或硬 I/O 超时；后台阻塞读取无法被 Shutdown
强制终止，不等于完整文件系统沙箱。目录名与文件名会随工具结果发送给当前模型供应商。

验证边界（2026-10-01）：产品测试共 15 个并通过（含 1 个沙箱占位测试），release 构建通过。
Core 的 7 个测试覆盖无工具文本流的增量顺序、
历史和退出背压；Provider、工具注册表及新 Agent loop 尚无专项测试，不能据此认定工具
闭环或 HTTP/SSE 已充分验证。当时真实模型工具调用、流式表现与终端恢复尚需人工验收，
搜索正常路径的后续反馈见下文。当时严格 Clippy 被 TUI 的 `Quit` / `SlashCommand` 未使用警告阻挡。

补充验证（2026-10-05）：作者反馈已运行编译后的程序并成功执行秘塔搜索，正常路径已
人工跑通；本次 Agent 没有调用真实 API，也未取得脱敏原始响应、费用或网络地域记录。
`cargo fmt --all -- --check`、`cargo check --workspace --lib --bins --locked` 和
`cargo clippy -p vortex-tools --lib --locked -- -D warnings` 通过，编译仍有上述 TUI 警告。
本次未运行单元测试；空结果、业务错误、超时、大小限制和搜索期间退出仍需后续回归验证，
不把一次成功搜索等同于完整的响应契约或可靠性验收。

Slash 补充验证（2026-10-13）：本次执行 `cargo check --workspace --all-targets --locked`
与 `cargo test --workspace --all-features --locked` 通过。产品测试共 47 项：Core 7、TUI 39、
沙箱模板 1；本轮新增 32 项 TUI 测试，覆盖解析、菜单/补全、草稿保留、退出 Effect 顺序、
按键映射及 TestBackend 布局恢复。TUI 的
`cargo clippy -p vortex-tui --all-targets --all-features --locked -- -D warnings` 通过。
全局 fmt 检查未通过，差异位于 TUI 的 command.rs、ui.rs、widgets/command_menu.rs；
全局严格 Clippy 被旧 CLI exec.rs 的 collapsible_if 阻挡，不能写成完整检查通过。
未运行 release 构建、真实 API 或真实终端/PTY；真实 resize、退出恢复与 Kitty 合盖异常
仍需人工验证。工具循环和 HTTP/SSE 的专项覆盖未因这些输入测试而补齐。

配置与模型选择补充验证（2026-10-17）：实现已接入 CLI/Core/TUI，Review 执行
`cargo check --workspace --all-targets --locked` 通过。随后修复了 3 项旧 TUI 测试的
单候选及固定行号假设，并增加双候选循环导航测试。最新检查中
`cargo fmt --all -- --check`、
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`、
`cargo test --workspace --all-features --locked` 和 `cargo build --release --locked` 均通过；
产品单元测试共 48 项（Core 7、TUI 40、沙箱占位 1）。
本轮专项测试由作者决定暂缓；配置优先级、凭据延迟读取、模型切换/确认、草稿保护、
搜索参数和跨平台路径等专项回归尚未补齐。真实 API/终端及 Windows/macOS 验收仍需
后续执行。此前搜索成功不等于本轮切换链路已人工跑通。

思考选项补充验证（2026-10-19）：请求装配已迁移为 Provider 接口方言、模型策略与能力；
旧 request_options 和千问专用 builder 不再作为有效入口。新增 22 项离线测试，覆盖配置
默认值/替换/拒绝规则、三种接入的精确 JSON、消息/工具映射不变、builder 字段无残留、
非法组合先于凭据读取、所选凭据按需读取、同 Provider 能力隔离及切换失败保留原绑定。
`cargo check --workspace --all-targets --locked`、`cargo fmt --all -- --check`、
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`、
`cargo test --workspace --all-features --locked` 和 `cargo build --release --locked` 均通过；
单元测试共 70 项（CLI 7、Core 12、Provider 10、TUI 40、沙箱占位 1）。
本轮未调用真实 API、读取真实凭据或进行终端交互验收。参数被远端接受、确实关闭思考、
正文与工具往返仍待作者验证；未补齐完整配置文件/路径、HTTP/SSE、工具循环及 TUI 模型切换矩阵。

目录工具补充验证（2026-10-21）：新增 23 项离线测试，覆盖路径/链接拒绝、隐藏规则、
非递归与中文名称、扫描/条数/JSON 字节截断、注册表错误转换、取消等待后后台额度保持，以及
搜索装配、按需凭据与模型 tools 能力不匹配时保留旧绑定。
本次执行 `cargo fmt --all -- --check`、
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` 和
`cargo test --workspace --all-features --locked --quiet` 均通过；单元测试共 93 项
（CLI 14、Core 12、Provider 10、Tools 16、TUI 40、沙箱占位 1）。
本次未运行 release 构建或真实模型/终端验收；没有读取真实凭据或调用付费 API。
目录工具的 Windows 边界、并发目录替换、内核 I/O 硬超时及完整工具循环/HTTP/SSE
专项覆盖仍不能由这些测试推断为已完成。

Thinking 离线测试补充验证（2026-10-26）：在原有 94 项测试基线上新增 50 项，覆盖显式
预设/逐字段覆盖、Enabled 请求与输出策略、同包思考/正文及工具增量、每模型步骤 256 KiB
UTF-8 字节上限、工具轮数、Session 事件顺序/历史隔离/退出背压，以及 TUI 思考分区、阶段、
失败片段保留、中文滚动和极小窗口。测试只使用固定 JSON、受控流和注入假凭据，不调用真实 API。
`cargo test --workspace --all-features --locked --offline` 通过，共 144 项（CLI 15、Core 32、
Provider 29、Tools 16、TUI 51、沙箱占位 1）；
`cargo clippy --workspace --all-targets --all-features --locked --offline -- -D warnings` 与
`cargo build --release --locked --offline` 通过。作者完成全量格式化后，复核
`cargo fmt --all -- --check`、上述严格 Clippy 与全部离线测试均通过。
本轮补测试与同步文档未修改产品实现逻辑或用户配置。
不把离线 Decoder 用例等同于 HTTP/SSE 字节分帧验收。
作者已反馈诗词请求的 Thinking 正常显示；搜索场景仍被单步多工具调用限制阻断，记录为
[Issue #3](https://github.com/AegeanZhang/vortex/issues/3)，工具往返尚未完成真实验收。

HTTP 诊断补充验证（2026-10-27）：本轮新增 25 项离线测试：Diagnostics 11、Provider
HTTP 接入 6、秘塔 HTTP 6、旧 exec 字节解析 2。覆盖请求关联与已知 key 替换、INFO 不序列化、
UTF-8/预算截断、有界后台错误预览、首段提前交付、思考/工具参数诊断、HTTP/MIME/坏 JSON、
流 Drop、搜索大小限制与总超时。测试仅使用假 key、本地回环 fixture 与局部 subscriber，
没有调用真实 API；HTTP fixture 的执行环境需要允许绑定本地端口。

以下 Cargo 检查均带 `--locked --offline` 并通过：Diagnostics 独立 lib check/test；Workspace
Debug/Release 的 `check --all-targets` 与 `clippy --all-targets --all-features -- -D warnings`；
Workspace `test --all-features` 共 169 项（CLI 17、Core 32、Diagnostics 11、Provider 单元 29
与集成 6、Tools 22、TUI 51、沙箱占位 1）。默认 Release 另运行 Provider 的 wire_trace
集成测试 6 项，以及秘塔 HTTP 测试 6 项，验证 TRACE 开启仍无 wire 事件且业务正常。
`cargo build --release --locked --offline`、`cargo fmt --all -- --check` 和 `git diff --check`
通过。本轮未进行真实终端、完整工具往返或跨平台验收；中文跨网络字节、CRLF/多行 data
以及完整传输异常/超时矩阵仍待补充。

文本选择与复制补充验证（2026-10-28）：TUI 已支持 Prompt/Transcript 的可见文本拖选、
反色高亮与 Ctrl+C 系统剪贴板复制，Prompt 键盘选区也可复制；保留滚轮与菜单行为。
新增离线测试覆盖字形、选区失效、草稿、后台结果和资源边界。详细契约、平台限制与
截图见 [文本选择设计](docs/design/tui/text-selection.md)。
本修复分支基于远端 e4b02bc，新增 17 项离线用例，工作区共 161 项通过（CLI 15、Core 32、
Provider 29、Tools 16、TUI 68、沙箱模板 1）；格式检查、严格 Clippy 和 Release 构建通过。
另在隔离 Xvfb/X11 中用 Kitty 0.45.0 验证实际拖选、Prompt 键盘复制、中文/代码剪贴板读回、
原生 Shift 拖选、无桌面剪贴板错误反馈，以及正常、错误退出和 panic 后 raw mode/termios/备用屏幕恢复。
此验证使用离线 Fixture Session，不调用真实 API；原生 Wayland、Windows/macOS 和 tmux 尚待验收。

合并验证（2026-10-28）：本地诊断/架构整理与 PR #5 合并后，工作区 186 项离线测试通过
（CLI 17、Core 32、Diagnostics 11、Provider 单元 29 与集成 6、Tools 22、TUI 68、沙箱模板 1）。
格式检查、全工作区严格 Clippy、Release 构建及文档链接检查通过；本次未重复调用真实 API
或进行终端交互验收，终端验证范围仍以上述隔离 Kitty 记录为准。

输入光标修复（2026-10-29，#7）：每帧将硬件光标对齐 Prompt 的实际插入位置，空框定位在
placeholder 前，模型窗口打开和输入区无可用单元格时隐藏。新增 6 项回归，离线测试共
192 项通过（TUI 74 项，其余维持上述分布）；`cargo fmt --all -- --check`、全 workspace
严格 Clippy 和 Release 构建均通过，Cargo 检查使用 `--locked --offline`。完整测试首次受
沙箱回环端口权限限制，允许本地 mock HTTP 监听后通过，没有调用真实模型或搜索 API。
隔离 PTY 中的生产 Release 二进制通过空框、英文首字、中文提交字符、删除、移动、横向
滚动、resize 后编辑重绘、模型窗口焦点及正常退出恢复检查；另以生产 `TerminalGuard`
源码构建隔离 fixture，验证错误和 panic unwind 后 termios、备用屏幕与光标恢复。
[终端录制](docs/assets/issue-7-prompt-cursor.cast) 为 asciicast v2，截取 resize 前的输入过程，
可用 `asciinema play` 播放；它不包含中文候选窗。真实 IME preedit/候选窗、桌面终端、
Windows/macOS 未验收。当前事件链无法在未收到提交字符时感知组合开始，此时仍保留
视觉 placeholder；不把硬件光标测试等同于 IME 组合事件支持，详见 [输入边界](docs/design/tui/interface.md#prompteditor-边界)。

## 首版方向

- Linux 优先，提供交互式 TUI；同时规划 `vortex exec "<task>"` 单次执行模式。
- 采用可插拔 Provider 架构，计划支持 OpenAI Responses API、OpenAI-compatible Chat Completions 和 Anthropic Messages API。
- 内置文件读取、内容搜索、补丁编辑和 Shell 工具，并通过 stdio 接入 MCP Server。
- 默认使用 `bubblewrap` 限制写入当前工作区并关闭网络；越界操作由用户按需审批。
- 使用 JSONL 事件日志保存会话，支持恢复、审计和异常退出后的继续执行。
- 分层读取工作区内的 `AGENTS.md`，为 Agent 提供项目级约束。
- 首版采用单 Agent loop，包含计划状态、取消执行和上下文自动压缩，不包含多 Agent 调度。

TUI 链路已使用协议无关的工具定义和模型事件；后续单次模式、会话记录与多 Provider
应复用这一执行核心，而不是复制工具循环。

## 仓库结构

```
Cargo.toml                 # 产品 Rust 2024 workspace（resolver = "3"）
Cargo.lock                 # 产品依赖锁文件
README.md                  # 本文件：产品设计与仓库现状
AGENTS.md                  # 协作规范（CLAUDE.md 为其指针）
docs/                      # 文档，分类索引见 docs/README.md
├── architecture/          # 架构、仓库组织、技术选型与术语
├── design/                # 详细设计（命令行接口等）
├── engineering/           # 编码、测试、Git 与 Review 规范
├── research/              # 技术调研
└── guide/                 # 用户手册
crates/                    # 产品 crate
├── vortex-cli/            # 参数、统一路径/日志、配置 bootstrap 与具体实现装配
├── vortex-core/           # 配置、模型管理、Session/Agent loop 与 Provider/Tool 接口
├── vortex-diagnostics/     # 共享开发期 HTTP 诊断，wire 模块仅在 Debug 编译
├── vortex-provider/       # OpenAI-compatible Provider 适配
├── vortex-tui/            # 交互界面、状态更新与渲染
├── vortex-tools/          # 工作区目录列表与秘塔网页搜索，保留 MockWebSearch
└── vortex-sandbox/        # 沙箱层占位 crate
```

本仓库只维护产品 Workspace，统一使用根目录 `Cargo.lock` 和 `target/`。
编程语言本身的学习与练习在独立项目中进行，不纳入本仓库。

## 配置与安全

TUI 配置优先级为内置默认值 → 用户 `<vortex-home>/config.toml` → 启动 cwd 下
`.vortex/project.toml` → 重复 `-c`（后者优先）→ `-m`。HOME 默认是原生用户主目录下
`.vortex`，显式 `VORTEX_HOME` 必须是非空绝对路径；日志与配置共用同一解析结果。
项目配置不向父目录查找，只能引用已有模型、关闭搜索或降低条数，不能新增服务地址或凭据来源。

Provider profile 定义连接、接口方言与密钥变量名，模型 profile 定义服务端模型、能力和请求选项。
thinking.mode 支持 provider-default（省略参数，不等于关闭）、disabled，以及显式预设
支持的 enabled。关闭要求声明能力且方言能够表达；当前开启仅支持 DashScope Qwen 的
展示-only 预设，不通用于其他模型或接入。详见 [思考选项设计](docs/design/provider/thinking-options.md)。
只读取选中模型的 key；搜索关闭或 Mock 不读搜索 key，秘塔缺 key 不回退 Mock。
搜索开关仅控制 `web_search`，目录工具始终注册，没有新增 files 配置；当前 TUI 即使
关闭搜索也要求所选模型声明支持工具调用。模型列表与创建入口使用实际注册表的同一能力要求。
默认模型/搜索分别使用 `QWEN_API_KEY`、`METASO_API_KEY`；密钥值只允许通过环境变量
提供，不写入仓库或 TOML。`-m/-c` 当前只接入 TUI，旧 exec 收到它们明确失败。
配置不自动生成或写回，没有订阅登录、热重载或完整 workspace trust。
搜索查询会发送给秘塔，工具结果再回填当前模型，两个服务可能分别计费；目录列表也会
发送给当前模型供应商，只读不等于没有外发风险。不要把密钥或私有仓库内容放进查询。
非交互模式遇到需要审批的操作时应安全失败，除非
用户显式选择相应权限策略。配置示例与边界见 [配置设计](docs/design/configuration/configuration.md)。

## 常用命令

以下命令检查产品 workspace，并运行产品 TUI：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
# 未设置 VORTEX_HOME 时使用用户主目录下 .vortex；可选绝对路径覆盖以隔离开发数据。
export QWEN_API_KEY='<your-api-key>'        # 仅在本地设置，不提交真实值
export METASO_API_KEY='<your-search-key>'   # 秘塔搜索密钥，不复用模型密钥
cargo run -p vortex-cli --locked           # 产品 TUI，输入 /exit 后 Enter 退出
cargo run -p vortex-cli --locked -- -m qwen-max -c 'search.enabled=false'
cargo run -p vortex-cli --locked -- -c 'search.backend="mock"'
```

产品不会主动读取 `.env`；本地开发脚本如需加载它，应先把变量导出到进程环境。
默认 HOME 外的覆盖示例为 `export VORTEX_HOME="${HOME}/.vortex-dev"`。用户配置由本人
维护，不要把陌生仓库里的 `.vortex` 指定为用户 HOME。

日志以 JSON 行追加到 `<vortex-home>/logs/vortex.log`，默认级别为 `info`；未使用 `-v`
时可用 `RUST_LOG` 调整，单个 `-v` 强制 `debug` 并覆盖它。普通日志不得记录密钥、Prompt、
工具结果或模型正文；已移除 TUI 提交分支的输入原文 TRACE，不代表完整隐私审计。
开发期显式正文诊断例外使用 `RUST_LOG=info,vortex_wire=trace cargo run -p vortex-cli --locked`，
不加 `-v`。已知密钥只作字面值替换，日志可能包含敏感正文，分享前须检查；预算、队列丢弃、
后台预览和退出限制见 [上手指南](docs/guide/getting-started.md#开发期请求与响应诊断)。

## Roadmap

1. 初始化 Rust 2024 Cargo workspace，完成 Provider 抽象、Session 事件接口和非流式 TUI v1。（已完成）
2. 补齐文本 SSE、Mock 工具循环、Provider 转换和 TUI 工具状态测试，完成真实终端验收。
3. 秘塔真实搜索与单层目录列表已接入，搜索正常路径已由作者人工跑通，目录工具已补离线测试；Qwen Thinking 展示与专项离线测试已接入，继续真实工具往返和搜索异常验收，再扩展其他工具及通用预算治理；重绘节流、思考历史回传和 Turn 取消独立推进。
4. 模型/搜索 TOML 配置与 `/model` 首版已接入，旧菜单回归已适配；思考选项装配及离线测试已完成，继续补其他配置/切换专项和真实交互验收。
5. 实现 JSONL 会话恢复、`bubblewrap` 沙箱和 stdio MCP client，并将 exec 迁移到共享 Session。
6. 完善安装、CI、发布包、可观测性与跨平台抽象。

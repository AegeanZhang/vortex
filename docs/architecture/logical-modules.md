# 逻辑模块与职责边界

核对日期：2026-10-30。本文按职责划分当前系统，不要求一个逻辑模块对应一个文件或
crate。物理组织见 [仓库结构](repository-structure.md)，后续拆分建议见 [架构演进](evolution.md)。

## 模块与实现对应

| 逻辑模块 | 主要源码（相对 `crates/`） | 对外衔接 |
| --- | --- | --- |
| 应用入口与装配 | `vortex-cli/src/{main,paths,logging,bootstrap}.rs` | 参数/运行环境 → SessionConnection → TUI |
| 配置管理 | `vortex-core/src/config.rs` | ConfigPaths/覆盖 → LoadedConfig |
| 模型管理 | `vortex-core/src/model.rs`、`vortex-cli/src/model_factory.rs` | ModelFactory → ModelManager → 当前 Provider |
| Session 调度 | `vortex-core/src/session.rs` | AgentCommand → CoreEvent，管理 Turn task |
| Turn 执行与历史构建 | `vortex-core/src/{agent_loop,conversation,tool_budget}.rs` | ModelRequest/ModelEvent、ToolCall/ToolResult、额度 → TurnProgress/TurnOutput |
| 模型协议适配 | `vortex-provider/src/openai_chat/`、`sse_transport.rs`；Core 的 `provider.rs` 定义契约 | ModelProvider → 外部 HTTP → ModelStream |
| 工具执行与资源访问 | `vortex-core/src/tool.rs`、`vortex-tools/src/` | ToolRegistry → Tool → 归一化 ToolResult |
| 终端交互与展示 | `vortex-tui/src/` | SessionConnection、终端事件 → UiAction/Effect → 画面 |
| 横切诊断 | `vortex-diagnostics/src/wire_trace.rs`、CLI `logging.rs` 及三个 HTTP 调用点 | 请求/响应副本 → tracing → CLI 文件 writer |

## 1. 应用入口与装配

负责将产品运行环境转成可执行对象：解析参数，确定 HOME/cwd，初始化日志，加载配置，
打开工作区句柄，注册工具，构造初始模型并连接 Session 与 TUI。`main` 持有日志 guard；
进入 TUI 前的路径/配置来源诊断由 CLI 输出。

不维护对话历史，不决定模型何时调用工具，不在 bootstrap 中复制 Agent loop。
上游是命令行与进程环境，下游是配置加载器、模型工厂、工具构造器及 TUI 入口。
旧 `exec.rs` 直接发送非流式 HTTP，是当前例外，不是第二套目标执行核心。
相关设计：[安装](../design/application/installation.md)、[CLI](../design/application/cli-surface.md)。

## 2. 配置管理

负责配置类型、内置值、用户/受限项目/CLI 合并、来源记录与定义校验。入口
`load_config` 接收明确的 `ConfigPaths`，不自行寻找 HOME 或 Git 根；同步文件读取
由 CLI 的 `spawn_blocking` 包裹，当前仍实现在 Core 的 `config.rs` 中。

产物是 `LoadedConfig`，启动装配持有它并将配置交给工厂/工具；不存在热更新服务。
配置层不读取密钥，不探测模型服务，不执行供应商请求字段转换，也不写回 TOML。
优先级及项目字段限制以 [配置设计](../design/configuration/configuration.md) 为准。

## 3. 模型管理

`ModelInfo` 是可展示目录信息；`ModelManager` 持有目录、当前元数据、工厂和
`Arc<dyn ModelProvider>`。启动后 manager 移入 Session，由 actor 独占并串行处理选择。
`ModelFactory` 是 Core 契约，CLI 的 `ConfiguredModelFactory` 才认识具体接入、凭据和 client。

工厂复用同一选项校验生成目录可用性并构造 Provider；非法组合在读取所选模型 key 前
失败。构造仅为本地操作，不发送模型请求。成功后 manager 同时替换元数据和实例，
失败保留旧绑定；每个 Turn 克隆一个固定 Provider，不在运行中跟随 UI 标签变化。

不承担会话存储、远端模型目录探测、订阅登录或配置保存。上下游分别是已加载配置、
Session 选择命令与 Provider 构造接口。相关设计：[模型管理](../design/runtime/model-management.md)、
[思考选项](../design/provider/thinking-options.md)。

## 4. Session 调度

负责会话生命周期、命令串行化、一个活跃 Turn、权威历史提交及公共事件发布。
`start_session_with_tool_limits` 注入配置规则；`start_session` 保留默认规则入口。
两者启动同一 actor 后立即返回 Snapshot、AgentHandle 和单消费者 CoreEventStream；
Snapshot 是启动时副本，不是共享可变状态。

actor 独占 `history`、`SessionStatus`、ModelManager 与任务集合；接受 Prompt 后创建
Turn task，消费其批次声明和逐项结果；排空真实进度、补齐异常结果后发布终态。业务命令走 mpsc，Shutdown
走独立 watch，避免关闭通知排在普通命令或 UI 背压后面。

不理解厂商 JSON 或终端控件，不做 Markdown 渲染，当前也不落盘或恢复会话。
相关契约与资源释放边界见 [Session 设计](../design/runtime/session-runtime.md)。

## 5. Turn 执行与历史构建

`run_turn` 接收固定 Provider、ToolRegistry、历史副本、进度发送端与 ToolCallLimits。它构造
`ModelRequest { items, tools }`，消费语义流，按结束原因返回回答或执行工具并再次请求模型。
局部状态包括上下文副本、当前步骤正文、工具调用 ID 集合、工具预算和思考字节计数。

每步接受多个调用，先做整批 ID/额度预检，再顺序执行；web_search 每 Turn 默认 4 次，
其他工具每种默认 50 次，用户可逐名覆盖，失败不退额。TurnToolBudget 每用户 Turn 新建，
不与模型步骤或 Token 预算混用。批次前缀和全部调用是一条 AssistantToolCalls，局部上下文
与 Session 各自追加声明和逐项结果；权威历史通过进度提交，最终正文通过 TurnOutput 提交。

ReasoningDelta 根据 Provider 消费策略拒绝或转发，仅计数、不写入历史；TUI 保留显示文本。
此模块不是完整 ContextAssembler：尚无指令加载、system prompt 装配、Token 裁剪或摘要。
协议与历史规则见 [Agent loop](../design/runtime/agent-loop.md)、[消息构建](../design/runtime/message-building.md)。

## 6. 模型协议适配

Core 的 `provider.rs` 定义 ModelProvider/Request/Event/Stream 及通用思考意图；具体
OpenAI-compatible 实现在 Provider crate。请求 DTO、方言选项、能力预设、解码器、
流编排和 SSE 传输已经分层，HTTP/SSE 类型不进入 Core 公共接口。

Provider 实例持有 client、连接、密钥、转换后的请求字段和消费策略；调用 `stream`
构造拥有自身上下文的惰性流，网络请求在 poll 后发生。传输解析 SSE 帧，解码器再将
厂商增量组装成完整 ToolCall 和 ModelEvent，不能把网络 chunk 当作语义事件。

不调度下一轮、不执行工具、不决定历史裁剪；相同模型在不同接口下的适配以显式方言
和预设为准，不靠模型名猜测。细节见 [SSE](../design/provider/streaming-response.md)、
[Thinking 展示](../design/tui/thinking-display.md)。

## 7. 工具执行与资源访问

Core 定义 Tool、调用/结果及 ToolRegistry；注册表保存 `Vec<Arc<dyn Tool>>`，检查重名、
稳定返回定义并将未知工具/参数/执行错误转为同 call_id 的错误结果。Schema 当前是
声明，实际参数校验由工具实现完成，没有通用 Schema 验证器或统一审批中间件。

CLI 固定先注册目录工具，再按配置选装秘塔或 Mock 搜索。目录工具持有已打开的根句柄
和后台并发额度；搜索工具持有 HTTP client、凭据、超时和大小限制。调用次数归 Turn，资源规则留在具体实现，
注册表不伪造所有工具具有同一种超时/取消能力。

目录只列一层、不读正文、不启动 Shell；路径边界、网络调用与输出预算见
[目录工具](../design/tools/list-directory.md) 与 [网页搜索](../design/tools/web-search.md)。`vortex-sandbox` 当前不参与这条执行链。

## 8. 终端交互与展示

TUI 持有终端 guard、SessionConnection 与 AppState。终端事件和 CoreEvent 统一转为
UiAction，`update` 同步修改展示状态并返回 Effect，事件循环再发送命令或退出。
`ui.rs` 负责布局，widgets 管理局部编辑、菜单、选择和 Transcript 展示/滚动。

AppState 持有草稿、pending、notice、模型展示副本及响应阶段；Transcript 按模型步骤
保存正文/思考。它们不是模型上下文，也不能替代 Session 历史。渲染可更新视口度量，
但不能创建会话事实；后台任务不直接操作终端。

TUI 不读取配置/密钥，不依赖具体 Provider/Tool，不持久化 Thinking。交互及状态边界见
[TUI 设计](../design/tui/interface.md)、[滚动设计](../design/tui/transcript-scrolling.md)。

## 横切职责：开发期 HTTP 诊断

CLI、模型协议适配与秘塔工具在各自 HTTP 边界调用共享 WireTrace。Diagnostics 不解析
模型语义、不执行工具、不持有权威历史，也不向 TUI 发布事件；它关联一次 HTTP 请求，
处理日志副本的已知密钥字面值、截断和错误预览。正常元数据日志仍由各模块直接发 tracing。

CLI 负责唯一 subscriber、过滤、目录与 writer，main 持有日志 guard；WireTrace 仅持有
每请求 ID、序号、状态、已知密钥副本与日志预算。错误预览任务短暂持有 response，
独立于 Turn 的取消集合；进程退出不保证其完成。

正文调用只在 debug_assertions 下编译，再由 vortex_wire TRACE 过滤。普通日志保持无正文；
显式开启时公开思考、历史与工具内容可能落盘，不能将此副本视为对话历史或会话存储。
完整接口及限制见 [诊断设计](../design/cross-cutting/diagnostics.md)。

## 状态与资源归属汇总

| 状态或资源 | 当前持有者 | 其他模块如何访问 |
| --- | --- | --- |
| HOME/cwd、日志 guard | CLI | 明确路径/构造参数注入，不由 Core/Tools 反查环境 |
| 配置与密钥读取入口 | CLI 工厂/具体工具装配 | Core 模型选择调用工厂，TUI 只收到安全元数据 |
| 权威对话历史、当前模型、SessionStatus | Session actor | Turn 获取副本，TUI 获取事件投影 |
| 单轮局部上下文、流和预算 | Turn task | Session 接收进度和最终结果 |
| Provider client、密钥、请求策略 | Provider 实例 | ModelProvider trait；Turn 内实例固定 |
| 根目录句柄与阻塞额度、搜索 client | 具体 Tool 实例 | ToolRegistry 调用，不暴露给 TUI |
| 草稿、展示文本、滚动、模型 pending | AppState/widgets | reducer 与渲染在 TUI loop 内访问 |
| 单次 HTTP 诊断 ID/预算、错误预览 response | WireTrace / 独立 Tokio 任务 | 调用点提供副本，CLI subscriber 接收事件；不进入 Core 状态 |

错误、日志、背压及数据外发是模块间共同约束，见 [总览](overview.md#横切约束)。
权限审批、事件存储和长期记忆尚未成为当前实现模块，不在此表预设可用对象。

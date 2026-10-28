# 关键技术选型

核对日期：2026-10-27。本文件区分实际依赖、目标选型与尚未接入的能力。
候选比较保留在 [调研](../research/README.md)；职责与调用关系分别见
[逻辑模块](logical-modules.md) 和 [运行链路](runtime-flows.md)。改变技术路线时更新相应 ADR。

## 当前采用

| 领域 | 当前选择 | 使用边界 |
| --- | --- | --- |
| 语言与构建 | Rust、2024 edition、Cargo workspace、resolver 3 | 采用稳定版工具链方向；仓库尚未提供 rust-toolchain.toml 或 CI 工作流 |
| 异步运行时 | Tokio | 异步代码不得阻塞 executor；阻塞操作需要隔离到专用线程或进程 |
| 配置解析与校验 | TOML + Serde + url | Core 负责内置/用户/受限项目/CLI 合并，URL 结构校验不执行网络；模型请求选项属于模型 profile，密钥值不进配置 |
| 用户目录 | 统一 `.vortex` HOME | 默认系统用户主目录下 `.vortex`，VORTEX_HOME 可覆盖；CLI 统一解析路径，不按 XDG/AppData 分流 |
| TUI | Ratatui + Crossterm + ratatui-textarea | 布局、终端事件及输入编辑分工；Tokio 统一事件循环，尚无 tick 绘制节流 |
| Transcript Markdown | tui-markdown | 仅 vortex-tui 使用，首版关闭默认 features，Assistant 正文按消息转换，User/Error 保持纯文本 |
| HTTP | Reqwest | Provider 传输与真实网络工具的实现依赖，不进入 Core 公共接口；旧 exec 路径仍待迁移 |
| SSE | eventsource-stream + async-stream | Provider 内分帧并构造惰性模型流，Core 只消费语义事件 |
| 结构化 JSON | serde_json | Core/Tools 表达工具 Schema 与参数，Provider 映射载荷；不等于已接入 Schema 验证器 |
| 工作区目录访问 | cap-std + cap-fs-ext | Tools 持有目录句柄，逐级不跟随符号链接；不是整个进程的 OS 沙箱 |
| 日志 | tracing + tracing-subscriber + tracing-appender | CLI 初始化一次文件 subscriber，其余 crate 发事件；不向 TUI 屏幕输出 |
| 开发期 HTTP 诊断 | 共享 vortex-diagnostics + cfg(debug_assertions) + vortex_wire TRACE | CLI/Provider/Tools 复用，正文副本有预算和有限字面值替换；默认 Release 无相关调用 |

当前 Session 历史为内存 `Vec<ConversationItem>`，不是 JSONL 存储；ToolSpec 使用 JSON
Schema 声明，实际参数由具体工具校验。以下目标不混入当前依赖表：

| 领域 | 目标选型 | 尚未具备的能力 |
| --- | --- | --- |
| 会话保存 | JSONL 事件日志 | 追加、回放、恢复及事件完备性尚未实现 |
| 外部工具接入 | stdio MCP | 尚无 client 或连接生命周期实现 |
| 权限与 Linux 隔离 | 审批策略与 Bubblewrap | 尚无实际审批和隔离，sandbox crate 为模板；当前不默认禁止网络 |

这些方向是已有目标约束，不是当前可用功能；后续职责与接入条件见 [架构演进](evolution.md)。

TUI 选型依据见 [`../research/rust-tui-research.md`](../research/rust-tui-research.md)，CLI 和凭据相关决策分别见 [`../design/application/cli-surface.md`](../design/application/cli-surface.md) 与 [`../design/cross-cutting/credentials.md`](../design/cross-cutting/credentials.md)。

## 安装与配置边界

[安装设计](../design/application/installation.md) 与 [配置设计](../design/configuration/configuration.md) 定义首版边界：
CLI 的 AppPaths 通过标准库原生路径/用户目录 API 解析 HOME、cwd 和文件位置，日志与
bootstrap 共用结果；Core config 接收明确路径，读取放到 Tokio 阻塞池，合并/校验保持
协议无关。Core 不依赖 Reqwest、具体 Provider 或终端类型。

Workspace 声明 `toml = { version = "1", default-features = false, features = ["std", "parse", "serde"] }`
和 `url = "2.5.8"`，解析使用 `toml::from_str`，不为普通配置开启写回/display features。
CLI 工厂直接使用 URL host 校验，需自行声明 url 依赖；Core/CLI 的 tempfile dev-dependency
用于隔离文件测试。实际版本仍由根 Cargo.lock 固定，依赖存在不代表专项测试已补齐。

Core 定义 ModelFactory/ModelManager，只认识元数据和 Provider trait；CLI 的
ConfiguredModelFactory 解析显式思考预设与可选覆盖，再将方言、策略、能力和独立历史契约
映射为 OpenAiChatOptions，先校验
再按需读取 key，并注入共享 Reqwest client；厂商字段转换留在 Provider，见
[思考选项设计](../design/provider/thinking-options.md)。模型 client 和秘塔 client 均关闭自动重定向与隐式重试；Factory 只本地装配，不
执行探测请求，不承担 OAuth/订阅登录。

项目文件只检查启动 cwd，不向上查找、不自动定位 Git 根。产品默认值随程序发布，硬限制由校验与执行入口保证，首版不增加外置系统配置或管理员策略。默认 HOME 与配置加载已接入，日志不再要求必须设置 VORTEX_HOME；Windows/macOS 的目录、权限和整个产品仍需原生验收，不能由 Linux 编译推断。

## Markdown 依赖与演进边界

选型与取舍见 [`ADR 0001`](decisions/0001-transcript-markdown-rendering.md)：当前优先搭建可用展示，
为工具、记忆等核心能力保留开发精力；通过 TUI 内部薄适配层保留后续优化和替换空间。

在 Workspace 声明 `tui-markdown = { version = "0.3.9", default-features = false }`，
由 `vortex-tui` 通过 `workspace = true` 引用；版本最终由根目录 `Cargo.lock` 固定。
首版不直接增加 `pulldown-cmark`、`syntect` 或 `ansi-to-tui` 依赖。若需要代码语法高亮，
在 TUI 的依赖声明中按需开启 `highlight-code`，并重新验证性能与依赖树。

升级时检查与现有 Ratatui 的类型兼容性；原始消息、滚动状态和 Session 接口不因 Markdown
渲染改变。实际实现进度与验证结果以代码、测试及根 README 为准。

## 流式输出依赖与边界

[`流式响应设计`](../design/provider/streaming-response.md) 使用 `eventsource-stream` 解析 SSE、
`async-stream` 构造惰性事件流、`serde_json` 解析事件载荷。Workspace 声明前两者的版本
要求分别为 `0.2.3` 和 `0.3.6`，Provider 通过 `workspace = true` 引用；实际版本由根
`Cargo.lock` 固定。Reqwest 启用 `json` 和 `stream` features。

Core 使用 `futures-util` 的通用 Stream 接口，不依赖 HTTP/SSE 类型。Tokio `time` feature
由需要网络超时、测试超时或绘制定时器的 crate 按需启用；启用 feature 不代表 TUI 已实现
定时刷新。不得用网络 chunk 边界代替 SSE 分帧，也不默认引入自动重连。

## 工具与日志依赖边界

Core 定义 Tool trait 与注册表，只依赖通用 Future/JSON 类型；vortex-tools 通过 workspace
依赖 Core、Diagnostics、Serde、serde_json、Reqwest、Tokio 和 tracing，CLI 依赖具体实现进行注册。
Tokio 的 `time` feature 用于搜索超时。目录工具另启用 `rt` 和 `sync`，分别用于
`spawn_blocking` 与后台并发额度；不能把异步等待取消等同于终止阻塞 I/O。
Mock 搜索不发网络请求；秘塔搜索使用 Reqwest
直接调用网页检索 API，厂商请求与响应 DTO 留在具体 Tool 实现层，不进入 Core 公共接口。
工具契约见 [注册表](../design/tools/tool-registry.md)、[目录列表](../design/tools/list-directory.md) 与 [网页搜索](../design/tools/web-search.md)，搜索候选比较见 [调研](../research/web-search-research.md)。

Workspace 声明 `cap-std = "4.0.3"`、
`cap-fs-ext = { version = "4.0.3", features = ["std"] }`，Tools 通过 workspace 引用，
实际版本由根 Cargo.lock 固定。前者保存已打开的工作区目录，后者提供
`DirExt::open_dir_nofollow`，按单个组件打开子目录而不是先检查绝对路径再重开。
CLI 从启动 cwd 打开根并注入，Tools 不自行寻找项目根或用户 HOME。tempfile 仅作
Tools 的 dev-dependency；目录工具不依赖 Shell 或尚未实现的 sandbox crate。
此边界不保证树快照、硬 I/O 超时或完整 Windows 重解析点隔离，也不替代未来审批与进程沙箱。

CLI 初始化非阻塞 JSON 文件日志，WorkerGuard 保持到 main 结束；日志路径为
`<vortex-home>/logs/vortex.log`，HOME 来自统一路径解析。RUST_LOG 与 verbose 的实际优先级、
TRACE 正文诊断用法见 [上手指南](../guide/getting-started.md#开发期请求与响应诊断)。TOML 已接入 TUI；
JSONL 会话、MCP/Bubblewrap 仍是后续目标，不能由依赖表推断能力已经可用。

Diagnostics 生产依赖为 Reqwest、Serde、serde_json、Tokio（rt/time）和 tracing，
不依赖 Core/Provider/Tools/TUI，也不使用 tracing-subscriber 初始化输出；后者只作该 crate
的测试依赖。库自身保留，wire_trace 模块与调用点用 cfg 裁剪，不使用按 debug_assertions
选择的 Cargo target 依赖。依赖与测试 features 以各 manifest 为准。
普通日志无正文；显式 Debug TRACE 记录最终请求、SSE data 和搜索/旧 exec 正文。
预算、有限脱敏、独立错误预览及 lossy 队列限制见 [诊断设计](../design/cross-cutting/diagnostics.md)。

## 文本选择与系统剪贴板

TUI 内部通过渲染单元格快照进行可见文字选择，系统复制使用 arboard 3.7，关闭默认图像
features，启用 wayland-data-control；版本由 Cargo.lock 固定。Clipboard 对象在专用线程上
按需创建并保留，后台结果回到 TUI reducer；Core/Provider 不依赖剪贴板或终端类型。
Linux 的会话支持、无剪贴板反馈和退出后的所有权限制见 [文本选择设计](../design/tui/text-selection.md)。

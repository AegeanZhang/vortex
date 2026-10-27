# 术语表

本表区分当前实现与目标概念；职责划分见 [逻辑模块](logical-modules.md)，数据流见
[运行链路](runtime-flows.md)。未实现的概念不代表存在同名 Rust 类型。

| 术语 | 定义与当前边界 |
| --- | --- |
| 逻辑模块 | 按职责和变化原因划分的边界，可跨多个文件或 crate；不要求创建同名 Rust module |
| Rust module | 通过 mod 与文件组织的命名空间和可见性边界 |
| Crate | Rust 编译及依赖单元；Cargo package 可包含 library/binary crate |
| 组合装配层 | 将配置、环境、trait 与具体实现连接起来的入口；当前主要在 CLI |
| Agent | 接收任务、请求模型并在执行边界内调用工具的主体；当前以单 Session/串行 Turn 实现 |
| Agent loop | 交替请求模型、执行工具并回填结果的控制流；当前位于 run_turn |
| Provider | 模型协议适配器，构造请求、消费响应并输出语义流，不执行工具 |
| Provider profile | 命名连接配置，包含协议/方言、地址及密钥环境变量名，不保存密钥值 |
| Model profile | 本地模型 ID 对应的连接、服务端型号、展示名、工具能力和思考选项；-m 与 /model 接受此 ID |
| Model factory | Core 的本地构造接口，CLI 实现读取所选凭据并创建 Provider，不探测远端 |
| Model manager | Session 独占的模型目录和当前绑定；构造成功后整体替换，失败保留旧实例 |
| Session actor | Tokio 后台任务，串行处理命令与 Turn 进度，独占权威历史和运行状态；不是独立线程或完整持久化系统 |
| Session connection | 初始 Snapshot、可克隆命令句柄及单消费者事件流的组合 |
| Session snapshot | 创建连接时的展示副本；当前消息为空，不实时更新，也不等于会话存档 |
| Turn | 从一次已接受用户输入到最终回答或失败的执行周期，可包含多个模型/工具步骤 |
| Model step | 一次 ModelProvider::stream 请求及响应；ToolCalls 结束此步骤，不结束整个 Turn |
| Agent command | 用户操作对应的业务命令；普通命令排队，Shutdown 通过独立 watch 通知 |
| Model request | 协议无关的 items/tools，由 Provider 映射成 wire 请求；当前无独立指令或预算字段 |
| Model event | Provider 输出的正文/思考增量、完整工具调用和单步结束原因 |
| Turn progress | Turn → Session 的内部进度，含模型步骤、文本/思考与工具信息；不是公共 UI 协议 |
| Core event | Session → TUI 的实时通知，含增量、工具状态、终态和模型选择结果；不是完备持久化日志 |
| UiAction / Effect | TUI 内的输入/事件与状态转换产物；Effect 由事件循环执行，reducer 不发送异步命令 |
| Conversation item | 权威请求历史中的消息、工具调用或结果；当前由 Session 直接维护，不由事件回放恢复 |
| Transcript | TUI 的展示条目、排版与滚动投影；可以含未提交或失败片段，不是权威历史 |
| Thinking / Reasoning | API 返回的公开思考文本或摘要，不是完整内部推理；当前仅特定接入支持展示-only，不回填历史 |
| 思考能力预设 | 显式声明某个接入的能力和历史适用契约；不是在线探测，也不创造远端能力 |
| Context assembler | 后续用于组装指令、选择历史和执行 Token 预算的职责；当前未实现 |
| Tool / ToolRegistry | 工具执行接口及其注册/分派；具体工具当前为目录列表和搜索，没有通用审批或 Schema 验证器 |
| Workspace | 当前产品中以启动 cwd 为根的目录访问边界；不是 Git 根，也不是已生效的文件写入沙箱 |
| Vortex HOME | 默认用户主目录下 .vortex，可由绝对路径 VORTEX_HOME 覆盖；存用户配置与日志，区别于安装位置和 cwd |
| Shutdown / CancelTurn | 当前 Shutdown 结束整个 Session；保留会话的 CancelTurn 尚未实现 |
| Approval / Sandbox | 目标中的授权决策与 OS 隔离；当前没有实际审批系统，sandbox crate 仍为模板 |
| MCP | Model Context Protocol；stdio 接入为后续目标，当前没有 MCP client |
| ADR | Architecture Decision Record，记录具体架构取舍及重新评估条件，不替代实现或验收记录 |

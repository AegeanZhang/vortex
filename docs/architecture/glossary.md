# 术语表

| 术语 | 定义 |
| --- | --- |
| Agent | 接收任务、调用模型与工具并持续推进直到完成或阻塞的执行主体 |
| Agent loop | 在模型响应、工具调用、工具结果和下一轮模型请求之间循环的核心控制流 |
| Provider | 对具体模型服务协议的适配器，负责请求构造和流式响应解析 |
| Tool | Agent 可调用的受控能力，例如文件读取、搜索、补丁和 Shell |
| Model event | Provider 输出的协议无关正文/思考增量、完整工具调用和模型步骤结束原因 |
| Turn progress | run_turn 向 Session 内部通道发送的文本与工具步骤进度，不是公共 UI 协议 |
| Core event | Session 向 UI 发布的语义通知，例如正文增量、工具状态、Turn 终态；不同于 ModelEvent |
| Conversation item | 请求历史中的完整语义项，例如消息、工具调用和结果；v1 由 Session 维护，事件回放是后续目标 |
| Context assembler | 根据指令、会话历史、工具和 Token 预算构造 Provider 无关请求的核心组件 |
| Model request | Provider 无关的模型调用描述，由 Provider adapter 转换为实际 API 请求 |
| Model step | 一次 ModelProvider::stream 请求及其响应；ToolCalls 结束步骤，但不结束 Turn |
| Turn | 一次用户输入开始，到 Agent 给出结果或停止的执行周期，可包含多个模型/工具步骤 |
| Session | 多轮交互及权威历史的运行时；持久化和恢复属于后续能力 |
| Session connection | Core 向交互层提供的一致 Snapshot、命令入口和实时事件流 |
| Approval | 对带副作用操作的用户授权决定 |
| Sandbox | 在操作系统层限制文件、进程和网络访问的执行环境 |
| TUI | 运行在终端中的交互式用户界面 |
| MCP | Model Context Protocol；Vortex 通过 stdio 接入外部工具服务 |
| Workspace | 当前任务的工作目录，也是默认文件写入和沙箱边界的锚点 |

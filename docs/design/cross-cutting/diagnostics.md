# 开发期 HTTP 诊断设计

核对日期：2026-10-27。状态：当前实现契约。架构归属：跨模块诊断。
源码入口：[wire_trace.rs](../../../crates/vortex-diagnostics/src/wire_trace.rs)、
[logging.rs](../../../crates/vortex-cli/src/logging.rs)。实际检查结果见根 [README](../../../README.md)。

## 职责与依赖

`vortex-diagnostics` 提供唯一的 `WireTrace` 实现，供 CLI 旧 exec、Provider 与秘塔工具调用。
调用方选择记录点并提供当前请求的已知密钥；共享模块负责关联、字面值替换、截断及日志事件。
Core 和 TUI 不依赖它，模型历史、工具调度与展示策略不因诊断改变。

CLI 的 `logging.rs` 仍负责初始化唯一 subscriber、过滤器、文件 writer，`main` 持有
LoggingGuard。Diagnostics 不初始化日志、不读取 HOME/配置、不自行创建文件，也不向终端输出。
普通元数据日志继续直接使用 tracing；本模块不是所有日志必须经过的代理。

## 编译与运行时开关

库保留可扩展入口，仅 `wire_trace` 模块及调用点受 `#[cfg(debug_assertions)]` 控制。
默认 Cargo dev/test 开启，默认 release 关闭；它取决于编译断言开关，不取决于二进制文件名。
当前不支持各 package 混用不同 debug-assertions 设置，验证使用默认 profile。

| 构建与过滤 | 行为 |
| --- | --- |
| Debug + 默认 INFO | 无 wire 事件，不为诊断额外序列化请求，不启动错误正文预览 |
| Debug + `vortex_wire=trace` | 记录以下已接入路径的正文诊断 |
| 默认 Release + `vortex_wire=trace` | 相关模块/调用已裁掉，无 wire 事件；普通日志仍可用 |

运行方法见 [上手指南](../../guide/getting-started.md#开发期请求与响应诊断)。
`-v` 当前强制 debug 并覆盖 RUST_LOG，因此使用该诊断时不加 `-v`。
没有新增 TOML 字段、CLI 开关或 Cargo feature。

## 记录点与业务边界

| component / 调用点 | 正常路径 | 失败路径 |
| --- | --- | --- |
| `model`：`openai_chat/stream.rs`、`sse_transport.rs` | 首次 poll 后记录最终 ChatRequest；响应头到达时记录状态；每个完整 SSE Event 在 Decoder 前记录 data | HTTP 非成功或 MIME 不符时启动有界错误预览，立即返回原 ProviderError |
| `search`：`metaso_web_search.rs` | 记录实际发送的请求 DTO、状态，以及有界读取后、规范化前的原始正文 | HTTP/MIME 异常启动预览；成功正文读取失败记录 body_unavailable，仍返回原工具错误 |
| `exec`：`exec.rs` | 记录固定旧路径的请求 DTO、状态和非流式正文 | HTTP 错误启动预览，返回原 Reqwest 状态错误 |

`request` 表示即将交给 HTTP client 的请求内容，不证明服务端已经收到。`response.head`
只含状态码，不抓取整组响应头。SSE 记录的是解析后的 `Event.data`：包含已接收的正文、
公开思考、工具参数、统计和 `[DONE]`，并非原始网络包或 SSE 原文；不保存 id/event/retry、
注释、尚未闭合或无法解析的事件块。JSON 内的响应 id 与诊断 request_id 也不是同一字段。

正常 SSE 仍逐事件 yield，不先缓存完整回答，不重复消费 response；Drop 仍停止本地流读取。
诊断不改变 Provider 的终态校验、搜索的 256 KiB 正文/16 KiB 输出上限或工具总超时。
旧 exec 为记录正文改用 bytes 后 `serde_json::from_slice`，成功输出与 HTTP 状态错误不变；
坏 JSON 的底层错误源由 Reqwest 解码错误变为 serde_json 错误，仍沿原 Box Error 接口返回。

## 日志字段

事件为 JSON 行，target 为 `vortex_wire`、level 为 TRACE，业务字段位于 `fields`：

| 字段 | 含义 |
| --- | --- |
| `component` | `model`、`search` 或 `exec` |
| `request_id` | 本地生成的组件、进程 ID、时间戳及原子序号组合；一次 HTTP 请求内共享 |
| `seq` | 此请求的诊断事件序号，从 1 递增 |
| `phase` | 下表中的记录阶段 |
| `status` | HTTP 状态；收到响应头前为 0 |
| `input_bytes` | 当前记录传入 helper 的字节数，不是整个 HTTP body 的总长度 |
| `captured_bytes` | 替换、裁剪后实际记录的 body UTF-8 字节数 |
| `truncated` | 当前正文不完整、读取异常或预算限制的标记 |
| `body` | 正文字符串；有些阶段为空；SSE data 未必是 JSON |

| phase | 含义 |
| --- | --- |
| `request` | 最终请求 DTO 的 JSON |
| `response.head` | 收到响应头后的状态记录 |
| `response.sse` | 一个完整 SSE Event.data |
| `response.body` | 非流式或搜索成功响应的原始正文 |
| `response.error_body` | 后台读取到的错误/MIME 异常正文预览 |
| `response.body_unavailable` | 搜索成功正文未能完整读取，不输出部分正文 |
| `capture.serialization_failed` | 诊断副本序列化失败，不记录序列化器的错误原文 |
| `capture.limit` | 请求正文预算耗尽后，首次再尝试记录 payload 时提示一次 |

一个 Turn 可包含多个模型 HTTP 请求及搜索请求，各自有独立 request_id。
当前没有 Session/Turn 级关联 ID，不能将一个 request_id 当成整轮工具闭环的 ID。

## 截断、脱敏与资源上限

- 单条 body 最多 64 KiB，一个请求累计 body 最多 4 MiB；按替换后的 UTF-8 字节计算，
  在字符边界裁剪。JSON 外壳、元数据和转义开销不计入这些额度；截断后的 body 可能不是合法 JSON。
- 达到总预算后停止记录后续 payload，仅补一次 capture.limit；业务流仍照常消费。
  序号代表已生成日志事件，不是 SSE 事件编号。
- 非 UTF-8 正文以有损转换表示。预算约束日志副本，不是 HTTP/JSON 序列化、SSE parser
  缓冲或整个进程的峰值内存上限。
- 已知非空密钥按长度从长到短作精确字面值替换，再截断。模型/exec 提供当前模型 key，
  搜索提供原始 key 和完整 Bearer 串；不打印认证头，也不派生持密类型的原文 Debug。
- 这不是通用隐私过滤器：转义/编码后的密钥、跨事件或预览边界的片段、其他服务的 key、
  Prompt、工具结果和个人信息均不能保证被移除。主动开启即可能将请求、响应与公开思考落盘。
  凭据的安全要求与当前实现局限见 [凭据设计](credentials.md#开发期正文诊断例外)。

## 后台预览与日志生命周期

错误预览消费 response，使用 Tokio 任务继承当前 subscriber，最多读取 16 KiB，最多等待
300 ms。读到上限即标记 truncated；超时或读取失败也标记，不保证完整错误正文。
调用方立即返回原错误，不等待远端结束；生产调用方不等待预览 JoinHandle。

该任务不属于 Turn 的任务集合，取消 Turn 后仍可能短暂继续；丢弃 JoinHandle 不会取消任务，
但运行时退出可能使预览来不及记录。LoggingGuard 管理日志队列收尾，不保证等待所有预览完成。
CLI 非阻塞 writer 默认允许队列满时丢弃记录，且当前没有轮转或累计磁盘上限，因此不能把
wire 日志当成完整抓包、审计日志或可恢复的会话记录；seq 缺口可提示丢失，缺失尾部未必可检测。

## 离线回归与后续扩展

共享 helper 单测放在 Diagnostics；Provider 使用公开接口与本地 HTTP fixture，Tools
通过私有 from_parts 注入本地端点，CLI 使用固定 ChatResponse 字节验证旧路径解析。
测试捕获局部 `vortex_wire` subscriber，以通道握手验证首段和错误先返回，使用假密钥及
禁用代理的回环 client；默认 Release 也运行接入测试，断言没有 wire 事件。
测试数量与实际命令结果只在根 README 维护。

后续有明确需求时可在同一 crate 增加耗时、诊断报告或 OTel 桥接；这些尚未实现，
也不应让整个 Diagnostics crate 永久受 Debug 编译限制。Release 正文诊断若成为需求，
需另行设计显式开关、隐私和生命周期，不通过默认开启 release debug-assertions 实现。

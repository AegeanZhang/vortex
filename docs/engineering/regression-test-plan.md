# 流式会话与搜索工具回归测试方案

- 整理日期：2026-10-06。
- 定位：从本地实施稿回收的测试方法与待补场景，不是测试通过记录。
- 契约依据：[Session](../design/session-runtime.md)、[Agent loop](../design/agent-loop.md)、
  [SSE](../design/streaming-response.md)。实际覆盖与运行结果见根 [README](../../README.md)。

## 1. 文件组织与测试替身

参数、映射和 reducer 的单元测试放在实现文件末尾的 `#[cfg(test)] mod tests` 中。
较长的 HTTP 测试可放在父模块声明的私有 `tests.rs` 子模块，仍能访问父模块私有成员；
不为了测试扩大生产 API 的可见性。各 crate 的 `tests/` 用于验证公开接口组合。

| 替身 | 验证范围 | 实现约束 |
| --- | --- | --- |
| FakeProvider | 请求历史、分步返回与错误 | 保存 `VecDeque<ModelStream>` 和捕获的 `Vec<ModelRequest>`；按顺序移出流 |
| ControlledStream | 首段时序、悬挂请求与取消 | 测试发送端控制事件，用 oneshot 通知首次 poll 和 Drop |
| FakeTool | 参数、次数、结果与执行顺序 | 固定名称/Schema，捕获参数，可返回成功、错误或等待信号 |
| 本地 HTTP server | HTTP、SSE 分帧与请求 DTO | 只监听 loopback 临时端口，使用固定假 Key，记录请求并返回 fixture |

FakeProvider 的 `requests()` 返回捕获请求的克隆快照；流本身不可 Clone，应通过队列
移出。Mutex 只保护短暂的队列/记录访问，不持锁跨 `.await`。当前入口使用：

```rust
let connection = start_session(provider, ToolRegistry::empty());
let requests = fake_provider.requests();
// 核对 requests[n].items 和 requests[n].tools，而非旧的 messages 字段。
```

这是测试组织示例，`requests()` 属于自定义 FakeProvider，不是生产 ModelProvider 方法。
工具场景传入真实 ToolRegistry 和 FakeTool；不要通过真实模型保证某次一定调用工具。
MockWebSearch 只替代搜索服务，仍搭配真实模型时不属于完全离线测试。

## 2. 异步时序、退出与背压

- 首段测试：发送一个 Delta，保持流未结束；确认消费者已收到增量，再发送 Finished。
  只断言最终文本无法证明“流式提前显示”。
- 取消测试：等待首次 poll 通知，再发送 Shutdown，最后等待 Drop 通知和事件通道关闭。
  在取消前保留测试发送端，避免发送端先销毁使流自然 EOF，从而产生假通过。
- UI 队列满：先接受本轮输入并确认流活跃，停止消费事件，再观察队列确实达到容量。
  随后发 Shutdown，验证流释放；不能只 sleep 后假设队列已满。
- 阻塞发布：容量为 1 的队列预先填满，确认发布 future 已 poll 到 Pending；切换关闭
  信号后应返回未发布，不应再塞入完成事件。
- Drop 最后一个命令句柄、丢弃事件消费者、正文前/输出中退出、task panic 分别测试。
  所有路径都应释放活跃任务，不能留下后台请求。

使用 oneshot/Notify 建立事件先后关系，`yield_now()` 只让出调度机会；timeout 作为防挂死
上限，不作为完成同步。Shutdown 通知发送成功不代表任务已经结束，须观察释放结果。

## 3. Core 与工具循环场景

| 场景 | 核心断言 |
| --- | --- |
| 普通成功、第二轮请求 | Started → Delta → Completed；完整 Assistant 只进入历史一次 |
| 部分输出后失败 | UI 可保留片段；下一次请求不含失败 Assistant，已接受 User 保留 |
| 空输入或 Running 时重复提交 | CommandRejected；不启动第二条流、不改当前 Assistant |
| 缺 Finished、空 Stop、Length、ContentFilter、panic | 一个失败终态，不生成伪成功历史 |
| 调用 → 结果 → 最终回答 | 第二次请求包含 ToolCall/ToolResult，ID、参数和顺序一致 |
| 工具前有 Assistant 前缀 | 前缀与调用/结果一起保存，wire 请求合并为同一 Assistant 消息 |
| 未知工具、参数错误、执行错误 | is_error=true、原 call_id 保留，模型仍能继续处理结果 |
| 四次调用后最终回答 | 可成功；第五次调用在执行、ToolStarted 和历史写入前拒绝 |
| 空白、历史重复、跨步骤重复 ID | 不执行工具，不产生重复 ID 的结果；工具失败也不释放 ID |
| 后续模型步骤失败 | 已完成工具步骤仍在历史，失败步骤的部分正文不在历史 |
| 不合法事件组合 | Stop 带调用、ToolCalls 不带调用、多调用、终态后继续发事件均失败 |

ToolRegistry 单独验证稳定的 specs 注册顺序、重名拒绝和 empty 注册表行为。
工具闭环测试还应核对 ToolStarted/ToolFinished 与文本事件的顺序，不只检查返回文本。

## 4. Provider：请求映射、Decoder 与真实分帧

请求测试验证 `items/tools` 映射、工具结果 ID、参数 JSON 字符串、Assistant 前缀合并，
以及无工具时的序列化形状。Decoder 测试喂完整 SSE data；分帧测试从 HTTP bytes 开始，
两者不可相互替代。

- 内容增量、统计空 choices、注释/心跳、同包正文与思考；阶段 A 的 Core 仍拒绝思考。
- 工具名称/参数跨包拼接，首次非空 ID、后续空 ID、相同 ID、不同非空 ID、坏参数 JSON。
- `finish_reason` 与 `[DONE]` 的顺序、缺 DONE、缺结束原因、重复终态、终态后正文。
- 非成功 HTTP、错误 Content-Type、坏 JSON/API 错误、body 中断与空闲超时。
- 同一 fixture 分别整包、逐字节、按多个固定大小拆分；包含中文跨字节边界、CRLF、
  多事件同 chunk 和合法多行 data。最终规范化事件序列应一致。
- 调用 `stream()` 不应立即发请求；第一次 poll 才开始，Drop 后不再读取流。

fixture 必须使用正确的 SSE 空行边界；多行 data 合并后仍须为合法协议 JSON。
不通过放宽完成条件掩盖拆包错误，也不引入自动重连续传来“修复”测试。

## 5. 秘塔搜索专项回归

### 参数与响应映射

| 分组 | 用例与断言 |
| --- | --- |
| 查询 | trim、中文按字符计数、512 边界；非对象、缺字段、非字符串、空查询和额外字段失败且不回显输入 |
| 请求 DTO | q、webpage、字符串 size=5 和三个关闭选项完全匹配 |
| 结果顺序 | URL 规范化后去重保留首条，不透传未知厂商字段 |
| 空结果 | 空数组成功；缺数组、null、错误 JSON 不转换为空结果成功 |
| 可选文本 | title 缺失/空白时回退 URL，snippet 缺失/null 时为空，不编造来源正文 |
| URL | 拒绝非 HTTP(S)、无 host、内嵌凭据、控制字符、超长和非法 URL；部分丢弃标记 truncated，全部不可用失败 |
| 预算 | 最多 5 条、标题 160 字符、片段 800 字符、URL 2,048 字节；中文与 JSON 转义后仍在 16 KiB 内且可重新解析 |
| 控制字符 | 片段中不含 ESC；这是搜索映射局部防护，不代表模型输出全链路已过滤 |

### HTTP、安全与取消

父模块的私有 `from_parts` 可注入本地端点、较短超时和较小预算，不增加公开 endpoint
配置接口。测试 Client 使用 `no_proxy()`，防止本地 fixture 被环境代理接管；这不改变
生产 Client 的代理行为。服务端任务持有 JoinHandle，在 Drop 时 abort，避免失败用例挂住。

需覆盖以下场景：

1. 捕获 POST 路径、Bearer 假 Key、Content-Type 和完整 JSON 请求，确认认证头 sensitive。
2. 401/403/429/5xx 返回脱敏错误；3xx 不跟随，另观察重定向目标未收到请求。
3. 错误 MIME、坏 JSON、200 但缺结果数组失败，不记录原始错误体或 Reqwest 敏感链。
4. 同时覆盖有 Content-Length 和 chunked 超预算；以实际消费的累计字节数判断。
5. 延迟响应头、已发头但正文迟迟不到都受总超时限制；不要只验证 `send()`。
6. 服务端确认已捕获请求后，再 abort 调用方并观察取消；不能宣称远端停止或未扣费。
   另补 Session Shutdown → run_turn → Tool future 的集成回归。
7. 注册表保留 call_id 并标记错误；Mock 和真实同名 web_search 不得同时注册。
8. Debug、无效 Key 和失败消息均不含假 Key、查询标记或原始响应体。

HTTP 200 内业务错误和真实可空字段应依据脱敏真实样例确认；自造 fixture 只能验证暂定
DTO。2026-10-02 临时实施稿记录过独立临时 crate 的 19 个示例测试及其 Clippy 检查通过，
它们不是产品测试通过记录，也不能证明真实 API 契约、网络可达性或额度消耗。

## 6. TUI 与后续 Thinking

阶段 A 验证增量追加、完成清理、失败片段保留、CommandRejected 不误删活跃 Assistant；
工具事件仅更新 notice，不新增 Transcript entry、不提前将 Running 改为 Idle。
TestBackend 覆盖 Markdown 围栏跨 Delta、布局、Unicode 测高及手动滚动不跳回尾部。

阶段 B 实施后再添加思考/正文分离、晚到思考不回退回答状态、只有思考时失败保留、
最终帧刷新与节流测试。阶段 C 再测试第二轮独立思考回传、User/System 无思考字段、
Snapshot 保留与模型能力限制。不得为了提前通过这些目标测试添加占位成功逻辑。

强制重绘与休眠恢复另见 [终端画面恢复](../design/terminal-redraw-recovery.md)；
TestBackend 不能替代真实 Kitty/PTY 的恢复验证。

## 7. 数据与执行

小型脱敏协议输入放在相应 crate 的 `tests/fixtures/`，用例与断言一同纳入 Git；fixture
不得包含真实 Key、用户 Prompt 或未经允许保存的完整响应。行为评估案例放在根
`datasets/`，用于人工/评估任务，不与确定性单元测试混用。

```bash
cargo test -p vortex-core --locked
cargo test -p vortex-provider --locked
cargo test -p vortex-tools --locked
cargo test -p vortex-tui --locked
```

完整提交前检查以 [测试规范](testing.md) 为准。先记录实际覆盖，再逐组补齐；测试数量
增加不等于契约全覆盖，真实服务人工跑通也不能替代异常与取消回归。

## 回收来源

本方案提取了 `20260919-01.md`、`20260919-02-test.md`、`20260927-01.md` 与
`20261002-01.md` 中可持续维护的测试要求，替换旧 messages 字段和单参数 start_session
示例，未复制已被当前实现取代的整段测试代码。

# 流式会话与内置工具回归测试方案

- 整理日期：2026-10-06。
- 配置/模型管理接口同步：2026-10-17；新增场景仍是待补要求，不代表执行过测试阶段。
- Slash 与配置实施稿补充回收：2026-10-19；保留未完成的测试与人工验收，不重复搬运已实现代码。
- 目录工具契约同步：2026-10-21；区分目录工具/装配已有用例与仍待补的全链路场景，结果见根 README。
- 思考选项、目录与 Thinking 实施稿回收：2026-10-26；补齐人工验收顺序和失败分类，不新增通过声明。
- 定位：从本地实施稿回收的测试方法与待补场景，不是测试通过记录。
- 契约依据：[Session](../design/runtime/session-runtime.md)、[Agent loop](../design/runtime/agent-loop.md)、
  [SSE](../design/provider/streaming-response.md)。实际覆盖与运行结果见根 [README](../../README.md)。

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
移出。Mutex 只保护短暂的队列/记录访问，不持锁跨 `.await`。当前 Session 接收 ModelManager；
`session.rs` 的模块内旧测试使用本地 `SingleModelFactory`/`test_models` 包装 FakeProvider：

```rust
let models = test_models(fake_provider.clone());
let connection = start_session(models, ToolRegistry::empty());
let requests = fake_provider.requests();
// 核对 requests[n].items 和 requests[n].tools，而非旧的 messages 字段。
```

这是 tests 模块内的组织示例，`test_models` 是该模块的私有 helper，不是 Core 公开 API；
其他位置需自建实现 ModelFactory 的假工厂。`requests()` 同样属于自定义 FakeProvider，
不是生产 ModelProvider 方法。
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
| 空闲切换后第二轮 | 新 Provider 收到保留且不重复的历史，旧 Provider 不收到第二轮，不产生 slash User 消息 |
| Running/构造失败/同 ID 选择 | 专用拒绝/确认事件，失败保留绑定；同 ID 不重建但仍确认 |

ToolRegistry 单独验证稳定的 specs 注册顺序、重名拒绝和 empty 注册表行为。
工具闭环测试还应核对 ToolStarted/ToolFinished 与文本事件的顺序，不只检查返回文本。

配置/装配专项另用 TempDir 和注入 KeyReader：合并优先级、cwd-only、路径/来源状态、
未知字段和坏文件、最终能力校验时机、未选中模型不读 key、disabled/Mock 不读搜索 key。
同 Provider 两个模型的思考策略与能力不串用；切换后用户配置字节不变，重启恢复加载默认值。
思考选项相关离线覆盖见第 8 节，其他待补矩阵不因旧 Session 七项通过而视为已验证；实际安排与结果见根 README。

## 4. Provider：请求映射、Decoder 与真实分帧

请求测试验证 `items/tools` 映射、工具结果 ID、参数 JSON 字符串、Assistant 前缀合并，
以及无工具时的序列化形状。Decoder 测试喂完整 SSE data；分帧测试从 HTTP bytes 开始，
两者不可相互替代。

- 内容增量、统计空 choices、注释/心跳、同包正文与思考；Reject 策略拒绝思考，DisplayOnly 独立转发。
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
| 请求 DTO | q、webpage、按配置序列化的字符串 size（如 "2"，默认 "5"）与三个关闭选项匹配 |
| 结果顺序 | URL 规范化后去重保留首条，不透传未知厂商字段 |
| 空结果 | 空数组成功；缺数组、null、错误 JSON 不转换为空结果成功 |
| 可选文本 | title 缺失/空白时回退 URL，snippet 缺失/null 时为空，不编造来源正文 |
| URL | 拒绝非 HTTP(S)、无 host、内嵌凭据、控制字符、超长和非法 URL；部分丢弃标记 truncated，全部不可用失败 |
| 预算 | 条数按配置 1..5；后端多返回仍截断；标题 160 字符、片段 800 字符、URL 2,048 字节；中文/JSON 转义后仍在 16 KiB 内 |
| 控制字符 | 片段中不含 ESC；这是搜索映射局部防护，不代表模型输出全链路已过滤 |

### HTTP、安全与取消

父模块的私有 `from_parts(client, api_key, endpoint, limits, max_results)` 可注入本地端点、
较短超时和较小预算；旧用例最后一个实参传 5，条数用例传 2，不增加公开 endpoint
配置接口。公开 new_with_options 另测生产数量/超时边界。测试 Client 使用 `no_proxy()`，防止本地 fixture 被环境代理接管；这不改变
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

## 6. TUI 与 Thinking

阶段 A 验证增量追加、完成清理、失败片段保留、CommandRejected 不误删活跃 Assistant；
工具事件更新 notice 与 UI 阶段，不新增 Transcript entry、不提前将 Running 改为 Idle。
TestBackend 覆盖 Markdown 围栏跨 Delta、布局、Unicode 测高及手动滚动不跳回尾部。

模型窗口补 Snapshot 初始化、确认前不换标签、成功/拒绝、pending 防重复、Esc 后新草稿
不被清空及 `/exit`。根菜单现在有 exit/model：裸 `/` 按两项断言，单项循环用 `/ex`，布局
测试从实际菜单矩形计算内容行，不保留旧固定行号。旧菜单回归已适配，专项缺口和
实际验证记录见根 README；不能据此认为模型窗口与 pending 链路已被充分覆盖。

当前展示-only 阶段已有思考/正文分离、纯文本与 Markdown、步骤分离、晚到思考不回退、
只有思考时失败保留、长中文滚动/resize 与极小窗口回归。新增测试与旧 slash 回归一同运行，
不能由此推断整个模型窗口矩阵已覆盖。跨事件节流及其最终帧测试仍待实现；
阶段 C 再测试第二轮独立思考回传、User/System 无思考字段、
Snapshot 保留与模型能力限制。不得为了提前通过这些目标测试添加占位成功逻辑。

强制重绘与休眠恢复另见 [终端画面恢复](../design/tui/terminal-redraw-recovery.md)；
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

## 8. 配置、路径与模型切换专项（部分已补）

本节从 `20261014-01.md` 第 12 节回收。对应功能已接入不等于测试已写入；在追加之前
先查看现有 tests，保留已通过的历史记录，不重复添加同名模块或重写旧测试。
[思考选项迁移](../design/provider/thinking-options.md) 已完成；request_options 是严格拒绝的旧字段。

2026-10-19 已新增思考选项离线单测：config.rs 验证默认值、条目替换、三种 profile、旧字段/
非法值拒绝及覆盖白名单；Provider 的 options.rs、request.rs 和 mod.rs 验证策略/能力/
方言矩阵、精确请求 JSON、消息/工具映射及 builder 替换无残留；CLI model_factory.rs
验证非法组合先于 key 读取、只读所选凭据、同 Provider 能力隔离及切换失败保留绑定。
测试使用固定 TOML 与注入 KeyReader，不读取真实用户文件或调用网络。以下完整文件型
加载、路径、Session/TUI 切换矩阵仍需另补，不能以这批测试代替整条链路验收。

Thinking 预设与展示已追加离线覆盖：None/false 区分、预设与方言拒绝、历史契约、
Enabled JSON、非法闲置 profile 与凭据读取、builder 消费策略替换；Core/Session 覆盖
步骤事件、思考字节上限、工具预算、请求历史不含思考及取消背压。真实服务与 HTTP
字节分帧仍单独验收，测试数量和执行结果只记录在根 README。

### 路径、日志与来源诊断

- 在 `crates/vortex-cli/src/paths.rs` 的模块内测试中直接调用 `AppPaths::from_inputs`，
  注入覆盖值、用户主目录和 cwd，不修改进程级 HOME/USERPROFILE/VORTEX_HOME。
- 覆盖默认用户目录、合法显式覆盖且无系统主目录、空/相对覆盖、缺失/相对主目录、
  三个派生路径，以及纯解析不创建任何文件。改变 cwd 只改变项目来源。
- 父目录有项目配置而子目录没有时，只检查子目录；不推断 Git 根。
- Unix 覆盖非 UTF-8 原生路径。Windows 用 `#[cfg(windows)]` 测中文/空格目录、
  盘符绝对路径，以及拒绝 `C:relative` 和 `\relative`；Linux 字符串测试不能替代它。
- `logging.rs` 用 TempDir 检查目录 helper：只创建必要的 logs，不创建配置、sessions、
  state 或 cache；路径被文件占用时报错，不主动改写已有目录权限。
- 新建 Unix 目录/文件权限与 Windows ACL/junction 分开验收。全局 subscriber 初始化
  在隔离子进程中测试，避免多个单元测试竞争全局状态。
- `--help/--version` 子进程指向尚不存在的测试 HOME，应成功且不创建目录、不读取 key。
- `bootstrap.rs` 的来源输出用子进程或纯格式化 helper 检查：路径、loaded/missing、
  cwd-only 提示和最终 profile/host 正确，输出不包含假 Key、覆盖原文或整个 AppConfig。

### 配置与文件加载

在 `crates/vortex-core/src/config.rs` 的模块内 tests 使用 TempDir；不访问真实用户
HOME 或 `.env`。覆盖：

- 内置默认值、新增条目、同 ID 整条替换、显式 false、重复 `-c` 后者优先及 `-m` 优先。
- 空合法 TOML 为 loaded，缺失为 missing；损坏、错误类型、非 UTF-8、超预算及
  不可读文件明确失败。项目符号链接、禁止字段和边界越权不能静默忽略。
- 最终模型/工具能力检查延后：`-m` 可改选兼容模型，关闭搜索只影响搜索配置；当前 TUI
  仍有目录工具，不能借此允许不支持 tools 的模型。Core 配置校验与 CLI 按实际注册表
  判断的能力约束分别覆盖，不能掩盖非法 Provider 定义、越界搜索参数或坏文件。
- 未知 Provider kind、未知配置字段、非法 ID/URL/引用/显示名、数量/超时边界失败。
- 模型级思考策略、预设与能力覆盖独立；旧 request_options、未知方言/策略/能力字段必须拒绝。
  Enabled 可以解析，适用性由工厂/Provider 校验；无历史契约或不支持的方言仍不可用。
  整条替换后省略的策略/预设/覆盖回到缺省。这部分已有纯解析与合并测试，文件型加载另补。
- `api_key="test-secret"` 等误配置失败，错误 Display/Debug 不含标记密钥或 TOML 原文。

以下是仍适用的最小文件型示例，追加到 config.rs 的 tests；已有模块只合并内容。
依赖使用现有 `tempfile` dev-dependency，不能为了测试改变生产配置加载逻辑。

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(user: &str) -> (tempfile::TempDir, ConfigPaths) {
        let temp = tempfile::tempdir().unwrap();
        let cwd = temp.path().join("project");
        std::fs::create_dir_all(&cwd).unwrap();
        let paths = ConfigPaths {
            user: temp.path().join("config.toml"),
            project: cwd.join(".vortex/project.toml"),
        };
        std::fs::write(&paths.user, user).unwrap();
        (temp, paths)
    }

    #[test]
    fn selection_checks_run_after_model_and_search_overrides() {
        let (_temp, paths) = fixture(
            r#"
default_model = "plain"
[models.plain]
provider = "qwen"
model = "plain-model"
display_name = "Plain model"
supports_tools = false
"#,
        );
        assert!(load_config(&paths, &[], None).is_err());
        let selected = load_config(&paths, &[], Some("qwen-max")).unwrap();
        assert_eq!(selected.config.default_model, "qwen-max");
        let disabled = load_config(&paths, &["search.enabled=false".into()], None).unwrap();
        assert_eq!(disabled.config.default_model, "plain");
        assert!(!disabled.config.search.enabled);
    }

    #[test]
    fn empty_file_is_loaded_and_absent_file_is_missing() {
        let (_temp, paths) = fixture("");
        let loaded = load_config(&paths, &[], None).unwrap();
        assert_eq!(loaded.sources.user.path, paths.user);
        assert_eq!(loaded.sources.user.status, ConfigFileStatus::Loaded);
        assert_eq!(loaded.sources.project.status, ConfigFileStatus::Missing);
        std::fs::remove_file(&paths.user).unwrap();
        assert_eq!(
            load_config(&paths, &[], None).unwrap().sources.user.status,
            ConfigFileStatus::Missing,
        );
    }

    #[test]
    fn malformed_definition_is_not_hidden_by_disabling_search() {
        let (_temp, paths) = fixture("[search]\nmax_results = 0\n");
        assert!(load_config(&paths, &["search.enabled=false".into()], None).is_err());
    }
}
```

### ModelManager、Factory 与 Session

`model.rs` 用两个可辨识 FakeProvider 和假工厂，检查初始绑定、重复/未知 ID、能力禁用、
成功选择和构造失败；失败后仍使用旧实例。同 ID 幂等确认且不重复构建或读 key。
FakeProvider 可返回空流，因为此层只测试本地绑定，不启动实际 Turn。

`model_factory.rs` 注入 KeyReader 闭包记录读取的变量名，不使用 set_var。构建目录时
不读取 key，只读取当前选中模型的凭据；未选中模型缺 key 不阻止启动。关闭搜索和
Mock 不要求秘塔 key，真实搜索缺 key 明确失败，不回退 Mock。
两个模型共享连接但请求选项不同时，逐次构建不串用参数；DTO 精确 JSON 另在 Provider
私有单测中断言，不为测试增加公开请求体 getter。

目录工具加入后，bootstrap 的测试还应断言目录工具优先、关闭搜索仍保留目录工具、
搜索 key 缺失不静默降级。Factory 的 `requires_tools` 来自实际注册表；目录不可用原因
与 create 拒绝一致，校验先于模型 key 读取，切换失败保留旧实例。以上基础用例已加入
对应模块 tests；不代表后续 Session/TUI 多模型往返场景全部覆盖。

`session.rs` 复用现有受控流和测试 helper 验证：

```text
SubmitPrompt(第一问) → 等待 TurnCompleted
SelectModel(model-b) → 等待 ModelSelected
SubmitPrompt(第二问) → 等待 TurnCompleted

provider-a 只收到第一轮；provider-b 收到包含完整第一轮历史的第二轮。
历史不重复，不包含 /model User 消息，选择不产生 TurnCompleted。
```

另测 Running 拒绝、Failed 后可切换、构造失败保留原绑定、同 ID 确认、选择事件发布
被背压阻塞时 Shutdown 可结束。使用 oneshot 保证请求确实活跃，不靠短 sleep。
切换不修改加载默认值，重新创建 Session 仍选配置默认模型。

### 搜索参数与 TUI

搜索分别在 request.rs 和 response.rs 模块中检查：`size` 按配置序列化为字符串，
服务端多返回仍按配置截断并标记 truncated，0/6 条被拒绝；连接/总超时确实传入 HTTP
客户端。查询、体积、认证、取消等其他边界继续使用第 5 节矩阵。

TUI 在既有 tests 中补 `/model` 的解析、参数个数/ID 边界及前导空白；ModelPicker
补当前定位、空列表、禁用项、循环、pending 与关闭状态。reducer 补确认前不换模型、
成功/拒绝、新草稿保护、无关拒绝不清 pending、Running 拒绝、窗口打开时的输入优先级。
TestBackend 补窗口极小尺寸、动态 Model 标签、覆盖后关闭恢复；布局从实际矩形计算，
不要恢复单候选或固定屏幕行号假设。真实退出恢复仍需 PTY/终端，不由 Buffer 断言替代。

## 9. 目录列表回归边界

`vortex-tools/src/workspace_fs.rs` 与 `list_directory.rs` 的模块内测试使用 TempDir；
已加入路径/深度/UTF-8 字节边界、符号链接拒绝、隐藏项、中文与非递归、目录错误、
扫描与返回条数、最终 JSON 字节截断/转义及注册表错误转换与调用 ID 保留的基础用例。
`bootstrap.rs` 和 `model_factory.rs` 覆盖工具装配与 tools 能力约束；实际数量/结果只在根 README 记账。

取消测试使用 channel 控制阻塞闭包：确认开始后让等待超时、取消 async 任务，断言
permit 仍被后台工作持有、其他调用被拒绝；主动释放闭包后恢复可调用。不依赖 sleep
模拟文件系统卡死，也不宣称已开始的内核 I/O 可被强制取消。

仍须专项验证枚举期间的删除/替换、普通用户权限错误、启动后 cwd 改变与 Windows
junction/reparse point。禁止为测试改变整个并行测试进程的 cwd 或真实用户目录；必要时
使用隔离子进程。真实模型选择目录工具、ToolResult 回填与 TUI 提示需要人工或受控
FakeProvider 集成验收，不能仅凭模型回答声称“已查看”判断成功。

## 10. 人工验收与覆盖记账（待执行）

以下步骤从 slash/config、思考选项、目录与 Thinking 实施稿回收，是验收要求，不是通过记录。
已有离线与人工反馈仍以根 README 为准；本节列出的完整矩阵并未全部验收。

### Slash、配置与终端

1. `/` 列出 exit/model，`/zzz` 无匹配，删除到 `/` 恢复；Up/Down 不改草稿。
   Tab 和前缀 Enter 只补全，完整命令在下一次独立 Enter 执行；Esc 后 CoreEvent 或
   光标移动不重开菜单，真实文本编辑才重新筛选。
2. ` /exit` 是普通 Prompt；非法参数或多行 slash 不发送模型；已知 Running 时提交
   普通草稿不丢失。Idle/Running/Failed 的 `/exit` 都恢复终端。
3. 在 80×24、窄/矮窗口、菜单打开时 resize，确认输入框不移动、关闭不留空白、历史
   滚动位置不跳变；模型窗口关闭恢复原画面。
4. `/mo` 补全后打开 `/model`，切换成功后下一问使用新 Provider；Running 拒绝，缺
   key 保留旧模型，取消窗口不丢稿。测试可能消费真实 API 额度，需由作者主动执行。
5. 配置默认 model-a，不加项目/CLI 覆盖启动，切到 model-b、退出并重启，应恢复 a；
   对比配置文件字节不变。否则容易把优先级覆盖误当成配置被写回。
6. 从两个 cwd 启动，用户配置来源不变，项目来源随 cwd 变化；从仓库子目录启动不读
   根项目配置。分别验证默认 HOME、显式绝对 HOME、中文/空格路径与 Windows/WSL。
7. 首次启动仅建必要日志，更换二进制位置不改变配置来源，非法 HOME 失败；不自动
   移动旧目录或改写本地脚本。Windows ACL 与 Unix 权限分别记录结果。

### 不同接入的思考关闭策略

由作者在用户配置中准备各自有效的 profile，分别验收百炼 Qwen、DeepSeek 官方与
百炼托管 DeepSeek；profile ID 自行对应，不认为内置了所有候选。装配契约见
[Provider 思考选项](../design/provider/thinking-options.md)，配置示例见
[配置设计](../design/configuration/configuration.md)。

1. 导出所选模型凭据，以 `cargo run -p vortex-cli --locked -- -m <profile-id> -c 'search.enabled=false'`
   启动，验证首段正文、正常终态与关闭思考的实际表现。关闭搜索仍保留目录工具。
2. 通过 `/model` 往返切换这些 profile，确认下一轮使用各自连接与选项；用户配置文件
   字节保持不变，失败保留旧绑定。请求字段用离线 DTO 断言检查，不靠打印真实请求验证。
3. 显式设置 `search.enabled=true`、`search.backend="mock"` 后要求使用搜索工具，确认
   结果回填后的模型请求可被接受；最后再按需验收真实搜索。

Mock 仅替代搜索服务，模型仍可能计费。Disabled 若返回思考，先核对选中 profile、
实际接口方言与远端参数支持；ProviderDefault 省略参数，不等于关闭。不要删除拒绝
分支或把 reasoning 合并到正文来绕过失败，也不要为排查开启包含完整 Prompt 的共享日志。

### 目录工具的真实调用链

在作者明确选择的测试目录启动并关闭搜索，不需要搜索 key，但仍需要模型 key。
根目录来自启动 cwd，`--manifest-path` 不会改变它；从其他 cwd 验收时使用已构建的二进制。

1. 要求调用 `list_directory` 列出 `.` 的直接子项，再指定一个确实存在的子目录，
   确认没有递归、没有读取正文；尝试 `../` 应返回工具错误而不访问父目录。
2. 在临时工作区准备中文/空格名称、隐藏项和链接，核对隐藏与链接拒绝规则。枚举期间
   删除/替换、普通用户权限错误、启动后 cwd 变化与 Windows junction/reparse point
   仍需专项验证；权限测试避免以 root 身份运行，cwd 变化测试使用隔离子进程。
3. 核对完整链路：ToolSpec → 模型 ToolCall → 相对路径执行 → 同 call_id 的 ToolResult
   → 下一次模型请求包含结果 → 最终回答。通过受控测试或本地调试器观察，不仅依据
   Assistant 声称“已查看”判断成功，不把真实目录列表输出到共享日志。
4. `/model` 中不支持 tools 的 profile 应不可选；关闭搜索也不解除该约束。启用搜索后
   再确认目录工具与搜索工具可分别调用，不能以搜索成功替代目录工具验收。

目录名和文件名会随结果发给模型，属于不可信数据；隐藏规则不等于秘密检测或提示注入
防护。保持单层和四次工具预算，硬 I/O 超时、递归、分页及完整沙箱仍是后续议题。

### Thinking 展示与工具往返

先核对实际服务地址、模型名和 `dashscope-qwen-display-only` 预设的适用范围，再显式
选择配置好的 Thinking profile。已有配置示例见 [配置设计](../design/configuration/configuration.md)，
只选择预设不能证明远端符合展示-only 历史契约。

1. 简单文本问题：有公开思考时独立显示 Thinking，正文保持 Markdown；无思考时不编造。
   作者此前诗词请求的成功反馈不代表下列工具往返已完成验收。
2. 明确要求列目录：观察思考、工具执行、新模型步骤与最终正文，工具前后步骤不混合。
3. 显式启用 Mock 搜索并要求使用 `web_search`，验证工具结果回填后的模型请求被接受。
   Mock 不替代真实模型，也不保证模型必定调用工具。
4. 继续发送第二个用户问题，确认历史正文与工具结果可继续使用；思考不回填由受控请求
   捕获测试证明，不从最终回答猜测，也不通过记录真实 reasoning 来验证。
5. 思考期间提交 `/exit`，确认 Session 结束且终端恢复；它不是保留会话的 CancelTurn，
   也不证明已开始的目录阻塞 I/O 被强制终止。
6. 切回默认关闭 profile，确认正常回答且无意外 Thinking；关闭模式意外返回思考仍拒绝。

记录失败时区分模型没有选择工具、HTTP 参数拒绝、工具历史不兼容、无正文结束、Core
协议拒绝与 TUI 展示异常。目前单个模型步骤只接受一个工具调用，多调用限制见
[Issue #3](https://github.com/AegeanZhang/vortex/issues/3)；它与整个 Turn 四次工具预算不同，
遇到后记录受阻场景，不把它当成本次归档需要修复的功能。
若远端要求回传 reasoning，停止该接入的展示-only 验收，另行设计历史支持，不吞字段、
拼入正文、静默关闭工具或放宽协议校验。

每次补测试后先运行对应 crate 的 `cargo test -p <package> --locked`，再按测试规范
执行 fmt check、完整严格 Clippy、workspace tests 与 release build。只把实际执行的
结果写入根 README，不沿用原实施稿的副本测试计数或中间编译失败作为当前状态。

## 回收来源

本方案提取了 `20260919-01.md`、`20260919-02-test.md`、`20260927-01.md` 与
`20261002-01.md` 中可持续维护的测试要求，替换旧 messages 字段和单参数 start_session
示例，未复制已被当前实现取代的整段测试代码。

2026-10-19 补充回收 `20261007-01.md` 的输入/退出与真实终端验收，以及
`20261014-01.md` 的路径、配置、工厂、模型切换、搜索和 TUI 专项测试方法。
旧稿中的最小测试示例仅在仍符合当前 API 时保留；已实现菜单回归不重复添加，
未完成的扩展矩阵继续标为待补。临时原稿删除不表示测试阶段已经完成。

2026-10-26 核对并回收 `20261019-01.md`、`20261020-01.md`、`20261023-01.md`：
主要设计与已有离线覆盖已进入正式文档，本次补齐三种接入的关闭策略、目录调用链和
Thinking 工具往返的人工验收步骤。旧迁移代码、中间编译缺口与过时的“待实现”说明
不继续维护；未完成的验收与 HTTP/SSE 分帧覆盖不因原稿删除而变成已验证。

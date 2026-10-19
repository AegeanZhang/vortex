# 流式会话与搜索工具回归测试方案

- 整理日期：2026-10-06。
- 配置/模型管理接口同步：2026-10-17；新增场景仍是待补要求，不代表执行过测试阶段。
- Slash 与配置实施稿补充回收：2026-10-19；保留未完成的测试与人工验收，不重复搬运已实现代码。
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

## 6. TUI 与后续 Thinking

阶段 A 验证增量追加、完成清理、失败片段保留、CommandRejected 不误删活跃 Assistant；
工具事件仅更新 notice，不新增 Transcript entry、不提前将 Running 改为 Idle。
TestBackend 覆盖 Markdown 围栏跨 Delta、布局、Unicode 测高及手动滚动不跳回尾部。

模型窗口补 Snapshot 初始化、确认前不换标签、成功/拒绝、pending 防重复、Esc 后新草稿
不被清空及 `/exit`。根菜单现在有 exit/model：裸 `/` 按两项断言，单项循环用 `/ex`，布局
测试从实际菜单矩形计算内容行，不保留旧固定行号。旧菜单回归已适配，专项缺口和
实际验证记录见根 README；不能据此认为模型窗口与 pending 链路已被充分覆盖。

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

## 8. 配置、路径与模型切换专项（部分已补）

本节从 `20261014-01.md` 第 12 节回收。对应功能已接入不等于测试已写入；在追加之前
先查看现有 tests，保留已通过的历史记录，不重复添加同名模块或重写旧测试。
[思考选项迁移](../design/provider-thinking-options.md) 已完成；request_options 是严格拒绝的旧字段。

2026-10-19 已新增思考选项离线单测：config.rs 验证默认值、条目替换、三种 profile、旧字段/
非法值拒绝及覆盖白名单；Provider 的 options.rs、request.rs 和 mod.rs 验证策略/能力/
方言矩阵、精确请求 JSON、消息/工具映射及 builder 替换无残留；CLI model_factory.rs
验证非法组合先于 key 读取、只读所选凭据、同 Provider 能力隔离及切换失败保留绑定。
测试使用固定 TOML 与注入 KeyReader，不读取真实用户文件或调用网络。以下完整文件型
加载、路径、Session/TUI 切换矩阵仍需另补，不能以这批测试代替整条链路验收。

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
- 最终模型/工具能力检查延后：`-m` 改选兼容模型或关闭搜索可以修正最终选择，但不能
  掩盖非法 Provider 定义、越界搜索参数或坏文件。
- 未知 Provider kind、未知配置字段、非法 ID/URL/引用/显示名、数量/超时边界失败。
- 模型级思考策略与能力独立；旧 request_options、未知方言、enabled 和未知能力字段必须拒绝；
  整条替换后省略的策略/能力回到缺省。这部分已有纯解析与合并测试，文件型加载另补。
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

## 9. 人工验收与覆盖记账（待执行）

以下步骤从两份 slash/config 实施稿回收，不是通过记录：

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

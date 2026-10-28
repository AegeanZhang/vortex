# TUI 接口设计

架构归属：终端交互与展示。状态：当前实现契约。源码入口：[app.rs](../../../crates/vortex-tui/src/app.rs)、[event_loop.rs](../../../crates/vortex-tui/src/event_loop.rs)。
详见 [逻辑模块](../../architecture/logical-modules.md)；实现与验证结果以根 [README](../../../README.md) 为准。

接口核对日期：2026-10-28。本文描述当前实现契约，包含 Thinking 步骤投影；离线与真实交互的验证边界见根 README，后续能力单独标注。

本文档以文本流式和工具状态接口为基线，说明 `vortex-tui` 如何消费
`vortex-core::SessionConnection`。Session actor 和 Provider 调用链见
[`../runtime/session-runtime.md`](../runtime/session-runtime.md)，技术选型见
[`../../research/rust-tui-research.md`](../../research/rust-tui-research.md)。

## 职责边界

`vortex-tui` 是由 `vortex-cli` 启动的 library crate，负责终端生命周期、输入映射、
界面状态和渲染。它依赖 Core 的语义类型，但不持有具体 Provider，不构造模型请求，也不
保存权威会话历史。

- CLI 加载分层配置、按需读取密钥、装配 ModelManager 和 ToolRegistry，再把 Session 连接交给 TUI。
- Core 保存对话事实、执行 Turn 并发布 `CoreEvent`。
- TUI 保存 Prompt、Transcript、CommandMenu、ModelPicker、pending/notice、ResponsePhase 和展示用 Session/模型状态。
- Ratatui、Crossterm 和 `ratatui-textarea` 类型不得进入 `vortex-core`。

当前入口为：

```rust
pub async fn run(
    connection: SessionConnection,
    options: TuiOptions,
) -> Result<TuiOutcome, TuiError>;
```

`TuiOutcome::UserExit` 是当前唯一正常终态。`TuiOptions::color` 已预留但尚未生效。
终端 I/O、Session channel 和事件流关闭通过 `TuiError` 返回；TUI 不调用
`process::exit`。

## 模块与数据流

```text
Crossterm Event ──> map_event ─┐
                               ├─> UiAction ─> update(AppState) ─> Effect
CoreEventStream ────────────────┘                  │                 │
                                                   v                 v
                                              下一次 render     event loop 执行
```

| 模块 | 当前职责 |
| --- | --- |
| `lib.rs` | 公开入口、键盘/鼠标事件映射和 `TuiOutcome` |
| `terminal.rs` | 初始化 raw mode/alternate screen，并用 guard 恢复终端 |
| `event_loop.rs` | 等待终端/Core/剪贴板结果、调用 reducer、顺序执行 Effect |
| `selection.rs` / `clipboard.rs` | 可见文本选区及串行后台系统剪贴板写入 |
| `app.rs` | `AppState` 和唯一状态修改入口 `update` |
| `action.rs` | `UiAction` 与 `Effect` |
| `command.rs` | Exit/Model 静态定义、带参数的 ParsedCommand、单行解析与补全前缀 |
| `ui.rs` | 四区布局；渲染时允许更新 widget 的视口度量，不改变会话事实 |
| `markdown.rs` | 单条 Assistant 正文转换，保留样式与对齐继承 |
| `widgets/` | `PromptEditor`、`CommandMenu`、`ModelPicker` 与 `Transcript` 的局部状态和渲染 |

后台任务不能直接修改 UI 或绘制终端。`update` 是同步纯状态转换；发送异步命令和退出循环
由 event loop 执行。

当前模块只按已经出现的职责拆分，不建立通用 `Component`、`Screen`、router 或内部
event bus。等第二个页面、焦点切换或真正可复用的控件出现后，再从重复行为中提取抽象，
避免隐藏现有的单向控制流。

## 输入、Action 与 Effect

当前按键映射如下：

- 完整 `/exit` 提交后构造 `UiAction::Quit`，复用 Shutdown → Exit；当前没有 Ctrl+C 退出映射。
- 无修饰键 `Enter` → `UiAction::SubmitPrompt`；Ctrl/Alt/Shift+Enter 交给编辑器并被拒绝，不提交或换行。
- `PageUp` / `PageDown` → Transcript 向上或向下翻页。
- `Ctrl+Home` / `Ctrl+End` → Transcript 跳到开头或末尾。
- 其他 `KeyEvent` → `UiAction::EditPrompt`，先由 App 检查菜单，再交给 `PromptEditor`。
- 鼠标滚轮向上/向下 → Transcript 滚动 3 个渲染行，当前不按鼠标位置分区。
- `Event::Resize` → `UiAction::RedrawRequested` → `Effect::Redraw`，下一次绘制按新尺寸布局。
- Release 全部忽略；Enter/Tab/Esc 的 Repeat 忽略，普通字符和方向键 Repeat 保留。
- 左键按下/拖动/释放 → 可见文本选区，Ctrl+C → 系统剪贴板复制；细节见 [文本选择设计](text-selection.md)。
- 其他鼠标事件和 Paste 当前忽略，尚未启用 bracketed paste。

普通 Prompt 提交时，TUI 只生成 `SendCommand(SubmitPrompt)`，不会乐观地把用户文本写入 Transcript。
显示更新以 Core 随后发送的 `UserMessageAdded` 为准。提交先用 `text()` 非消费读取并解析
命令；全空白输入不发送、不清空。已知 `Running` 或模型切换 pending 时普通提交保留草稿并设置 notice。
普通 Prompt 通过本地判断后才 `take_text()`，发送原文，不去除首尾空白。
发送与 Core 接受之间的 pending_submit、Core 拒绝后的草稿恢复尚未实现，不能把本地
Running 检查视为完整的提交确认机制。

`Effect` 当前包括发送 `AgentCommand`、请求重绘、退出与显式复制文本。Effect 按返回顺序执行；退出
会先发送 `Shutdown`，成功后才结束 event loop。Shutdown 通过独立 watch 信号通知 Core，
发送成功不等于后台任务已经回收；它不是取消完成的确认。命令发送失败时返回 `TuiError`。

## Slash 命令与菜单

### 解析与执行

`COMMANDS` 保存 `/exit`、`/model` 的名称、说明和 `SlashCommand` 标识，不展示未实现的
clear/compact 等命令。解析器先按名称查询静态定义，再按标识解析参数；`CommandParse`
返回 NotCommand/Incomplete/Invalid 或 `Ready(ParsedCommand)`。静态标识可 Copy，
ParsedCommand 可持有模型 ID 字符串；`completion_prefix` 只用于筛选，不执行命令。

`/` 必须是整个原始草稿的首字符，不先 trim/trim_start。仅支持单行，完整 exit 只允许
尾部 ASCII 空格/Tab，检查 CR/LF 在尾部空白处理之前。命令名大小写敏感，不支持转义；
model 最多一个合法 profile ID，语法复用 Core 的 `valid_profile_id`，是否存在由 Core 判断。

| 草稿与确认动作 | 当前行为 |
| --- | --- |
| `/`、`/e`、`/ex`、`/exi` + Enter | 补全为 `/exit` 并关闭菜单；第二次独立 Enter 才退出 |
| `/exit`、`/exit `、`/exit\t` + Enter | 在 Idle/Running/Failed 均退出，不发送 Prompt |
| `/m`、`/mo`、`/mode` + Tab/Enter | 只补全 `/model`；不立即打开窗口 |
| `/model`，或仅有尾部 ASCII 空格/Tab + Enter | 打开模型选择窗口，不提交 Prompt |
| `/model qwen-max` + Enter | 申请切换该 profile，不直接替换界面模型 |
| `/model a b`、非法 ID 或 `/mo ` | 错误 notice，保留草稿，不交给模型 |
| ` /exit`、`说明 /exit` | 普通 Prompt；原文保留，仍受 Running 提交限制 |
| `/exit now`、`/exitfoo`、`/EX`、`/ exit`、`/ex `、`//exit` | 错误 notice，草稿保留，不交给模型 |
| 首字符为 / 且含 CR/LF | 拒绝多行命令，草稿保留，不吞掉换行后退出 |

完整命令按解析结果执行，不按菜单当前选中项执行。退出路径为
SubmitPrompt → Ready(Exit) → update(Quit) → SendCommand(Shutdown), Exit → event loop
返回 UserExit → TerminalGuard 清理。它不是保留 Session 的 CancelTurn，不调用 process::exit。

### 自动显示、导航与关闭

输入首字符 `/` 后自动显示候选；单 token 前缀筛选，未知前缀显示“没有匹配的命令”。
菜单打开时，无修饰键 Up/Down 循环选择，Tab 只补全并关闭，Esc 只关闭且保留草稿；
其他输入仍交编辑器。菜单关闭时方向键恢复编辑器操作。PageUp/PageDown 和鼠标滚轮
继续滚动 Transcript，不用于菜单导航；无匹配时 Tab 仅提示无可补全项。

CommandMenu 保存候选静态引用、selected 和 dismissed。常规编辑路径仅在文本实际改变时 sync，
光标移动、CoreEvent 与单纯重绘不重新打开被 Esc 关闭的菜单。不完整命令的显式 Enter
可以主动重建候选并补全，但仍不退出。补全会重置编辑器的选择与 undo history。

### 模型窗口与确认链

`From<SessionSnapshot>` 将当前 model 和 models 目录保存到 AppState；默认测试状态不虚构
当前模型。状态栏读取已确认的 `selected_model.id`，不是服务端模型名，也不在绘制时读配置。

完整 `/model` 打开 ModelPicker、定位当前模型并 dismiss 根命令菜单。列表显示 profile ID、
名称、Provider/host、当前 `*` 标记和不可选原因，提示“仅本会话”和保留历史的服务切换风险。
运行中可打开浏览，但不能确认切换；空目录 Enter 只提示没有可选择项。

窗口打开时普通字符不进入底下的编辑器；无修饰 Up/Down 循环移动，Enter 申请选择，
Tab 不切换，Esc 始终关闭并保留草稿。pending 时冻结移动和重复选择，Esc 不撤回已经
发送的选择请求。PageUp/Down 和鼠标滚轮仍沿用全局 Transcript 映射，不导航模型列表。

`/model <id>` 与窗口 Enter 都经过同一 helper：检查 pending、Running 和目录中的能力
禁用原因，保存 `PendingModel { model_id, command_draft }`，发送 SelectModel 并重绘。
未知但语法合法的 ID 留给 Core 拒绝。申请不清草稿、不更新 selected_model、不产生 Transcript。

Core 确认后更新已选模型；若匹配 pending，清 pending、关闭窗口、提示仅本会话，只有当前
草稿仍等于申请时草稿才清空并 sync 根菜单。用户可能已 Esc 后改写，新草稿不能被异步
确认抹掉；后台事件不重新打开已关闭的根菜单。收到匹配的 ModelSelectionRejected 时只
清 pending 并设置 notice，保留原绑定、草稿和仍打开的窗口，允许重新选择。
普通 CommandRejected 不清模型 pending，不删除 Assistant 占位。

pending 期间普通 Prompt 被阻止，但 `/exit` 的 slash 路由优先；窗口开着时先 Esc，再提交
`/exit`。切换不改 SessionStatus、不写配置、不承担登录；成功确认不是远端鉴权通过。

### 覆盖布局与边界

Prompt 固定底部；菜单左对齐覆盖 main_area 底部，不缩小 Transcript 或移动输入框。
状态栏保持原位，菜单与 Prompt 之间仍有一行状态栏。宽度最多 48 列，高度为候选内容行
（1～5 行）加两行边框，并受 main_area 限制；宽不足 4 或高不足 3 时不绘制，键盘补全
仍可使用。不承诺极小窗口可以看清所有文本。

先画 Transcript/status，再对根菜单矩形渲染 Ratatui Clear 和 List，然后画 Prompt。
这里 Clear 只清本帧 buffer 的局部区域，不是 terminal.clear()。关闭菜单请求正常重绘，
恢复底层历史，不保留空白占位，也不改变滚动偏移；小窗口无法展示菜单不是命令解析失败。

模型窗口最后覆盖绘制，区域在整帧居中，宽最多 80、高最多 18，四周优先留 1 格；宽或高
不足 4 时隐藏。窗口使用局部 Clear，不改变四区布局、Prompt 高度或 Transcript 视口。
根菜单与模型窗口不同时显示；List 自身跟随选中项滚动，暂不做分组、过滤或鼠标选择。

Paste、动态多行高度、更多 slash 命令和鼠标菜单选择尚未实现。粘贴隔离是后续独立工作，
不能因单行命令校验或 Enter Repeat 过滤就保证粘贴的换行不会提交。
交互取舍与后续输入方案见 [调研](../../research/tui-layout-and-input-research.md)。

## 文本选择与复制

Prompt/Transcript 可用左键拖选、Ctrl+C 复制；Prompt 的 Shift+方向键选区也接入系统剪贴板。
复制失败通过 notice 呈现，保持草稿与会话状态；选择区域变化或滚动/Resize 后取消旧选区，
避免坐标错位。命中范围、复制内容和各平台限制见 [文本选择设计](text-selection.md)。

## PromptEditor 边界

`PromptEditor` 是项目对 `ratatui-textarea::TextArea<'static>` 的薄封装。`PromptInput`
包装第三方 `Input`，使 `action.rs` 和 `AppState` 不需要了解 TextArea API；组件不暴露
`textarea()` 或可变 getter。使用 `'static` 避免把控件的生命周期传播到整个 AppState。

无修饰键 Enter 在全局映射阶段转换为 `SubmitPrompt`，`PromptEditor::handle_input` 也拒绝 Enter，
防止它意外插入换行。`take_text()` 使用 `std::mem::take` 取出文本，并以默认编辑器替换旧
实例，因此内容、光标、选择和 undo history 会一起重置，同时恢复边框与 placeholder。

`text()` 只读取草稿，`replace_text()` 重建默认编辑器后整段插入，供菜单补全使用；
`PromptInput::menu_key()` 识别无 ctrl/alt/shift 的菜单键，不让 App 直接访问 TextArea。
TextArea 插入 CRLF 时会规范化为 LF；保留草稿的断言比较提交前后实际文本，原始 CR/LF
的解析边界由 command.rs 的独立测试覆盖。

当前 Prompt 区高度固定为 3，只支持 Enter 提交的单行交互。以后增加多行输入时，需要同时
确定插入换行的组合键和输入区高度策略，不能只放开 TextArea 的 Enter。

## Core 事件投影

| `CoreEvent` | `AppState` 变化 |
| --- | --- |
| `UserMessageAdded` | 追加 User Transcript entry |
| `AssistantMessageStarted` | 状态设为 `Running`，清除 notice，创建空 Assistant entry，阶段设为 Waiting |
| `ModelStepStarted` | 复用完全空的 Assistant 或创建新 entry，阶段设为 Waiting，notice 显示步骤号 |
| `AssistantReasoningDelta` | 追加独立 reasoning；尚未 Answering 时阶段设为 Reasoning |
| `AssistantTextDelta` | 追加到最后一个 Assistant entry；不存在时创建一个，阶段设为 Answering |
| `TurnCompleted` | 状态设为 `Idle`，清除 notice 与阶段 |
| `TurnFailed` | 状态设为 `Failed`，清除 notice 与阶段，并追加 Error entry |
| `CommandRejected` | 只更新 notice，不改状态和 Transcript |
| `ToolCallStarted` | notice 显示 Running tool 和名称，阶段设为 Tool；状态仍为 Running |
| `ToolCallFinished` | notice 显示 Tool completed/failed，阶段回到 Waiting；不新增 Transcript entry |
| `ModelSelected` | 更新已确认模型；匹配 pending 时按草稿保护规则清理并关闭窗口，不改 status/Transcript |
| `ModelSelectionRejected` | 匹配 pending 时清 pending 并显示原因，保留原模型、草稿和窗口 |

Turn 失败时，`Transcript::fail_assistant` 只删除 reasoning 与 content 都为空的最后一个
Assistant 占位；有思考或正文均保留并另加 Error entry。Snapshot 只恢复 User/Assistant
正文，System 不显示，也不恢复思考、步骤边界、pending 或 UI 窗口。Running snapshot
初始化为 Waiting；它不是流重连或完整会话恢复接口。

每个有输出的模型步骤使用独立 Assistant entry，内部 reasoning/content 分开累积；
步骤开始可复用完全空占位。尚无工具结果卡片，工具错误也不等于 TurnFailed。
ResponsePhase 是 UI 专属 Waiting/Reasoning/Answering/Tool，不扩展 Core 的 SessionStatus；
状态栏依次显示 Session 状态、阶段、已确认模型和 notice。同一步正文后晚到思考不使阶段
退回 Reasoning，下一 ModelStepStarted 则重置为 Waiting。工具 notice 可继续保留至下次
步骤或终态，不单凭 notice 判断阶段。详见 [Thinking 展示](thinking-display.md)。

## Event loop 与绘制

`run(...).await` 在调用它的异步任务中进入 `run_event_loop`；后台 Session loop 已由
`start_session` 通过 `tokio::spawn` 启动，两者并发运行。TUI 不只是命令生产者，还消费
Session 返回的 CoreEvent；Session 不只是命令消费者，还负责发布会话事件。每个 Turn
另有短生命周期的 run_turn 任务，它可串行发起多个模型请求与工具执行，完整路径见
[`Session：启动顺序与三个执行角色`](../runtime/session-runtime.md#启动顺序与三个执行角色)。

Event loop 使用 `tokio::select!` 等待 `EventStream` 和 `CoreEventStream`。Action 经过
reducer 产生 Effect，所有 Effect 执行完后再决定退出或重绘。`redraw` 标志会合并同一批
Effect 中的重复请求，但当前没有定时 tick 或跨事件的刷新窗口。每个正文或思考增量都会请求重绘，
下一轮循环即可绘制；约 33 ms 的刷新节流仍是后续目标，不是现有行为。

页面固定分为 header、transcript、status 和 prompt 四区。Transcript 使用
`Paragraph` 换行渲染；空会话显示输入提示。内容溢出时显示纵向滚动条，支持键盘翻页、
跳到开头或末尾、鼠标滚轮，并在未手动浏览历史时自动跟随最新消息。完整规则见
[`transcript-scrolling.md`](transcript-scrolling.md)。Assistant 正文通过 `markdown.rs`
逐条目转换，思考以 DarkGray 纯文本显示并添加 Thinking 标签，User/Error 也保持纯文本；
两段内容合并为同一 Paragraph，标签、间距、测高与滚动仍归 Transcript。
每次重绘按累积原文重新转换，不单独解析每个 delta，也不把展示结果写回 Session。
Resize 已请求主动重绘，布局和菜单矩形按新 Frame 计算；这是常规绘制，不是强制清屏，
不证明 Kitty 休眠恢复问题已经修复。菜单有极小区域隐藏策略，整体四区布局仍需人工验收。

`TerminalGuard` 从 raw mode 开启后立即接管清理。正常退出显式调用 `cleanup`；初始化后
发生错误或 unwind 时由 `Drop` 尽力关闭 raw mode、鼠标捕获、离开 alternate screen 并显示光标。
TUI 运行期间不得使用 `println!` 破坏屏幕。

## 单元测试矩阵

通用规范见 [`../../engineering/testing.md`](../../engineering/testing.md)。优先测试 reducer 和
widget 的确定性状态，不通过真实键盘或真实 Provider 制造条件。
以下矩阵包含已覆盖与待补场景，不代表全部完成；具体数量与执行结果只在根 README 记录。

| 模块与位置 | 场景 | 关键断言 |
| --- | --- | --- |
| `app.rs` | Quit | `Shutdown` 位于 `Exit` 之前 |
| `command.rs` | 完整/前缀/未知命令、尾部空白、CR/LF、首字符规则 | Ready/Incomplete/Invalid/NotCommand 分类正确 |
| `app.rs` | Enter/Tab 补全、Esc、无匹配、Idle/Running/Failed exit | 补全不执行；退出只产生 Shutdown → Exit |
| 同上 | CoreEvent/光标移动后菜单保持关闭、实际编辑重开、删除 / | dismissed 与候选按实际文本变化更新 |
| 同上 | 空输入、正常提交、Running 时提交 | 是否保留输入、发送命令和设置 notice 均正确 |
| 同上 | `/model`、直接 ID、选择申请/成功/失败 | 不发送模型 Prompt，确认前不改模型，失败保留原绑定 |
| 同上 | pending 后 Esc 改草稿、无关拒绝、Running 与退出 | 新草稿不清空、不重复选择/提交，仍可退出 |
| 同上 | 每一种 `CoreEvent` | status、notice、Transcript 与 Redraw 一致 |
| 同上 | 思考/正文/步骤/工具事件 | 阶段随事件切换，晚到思考不回退 Answering，工具事件仍 Running |
| `widgets/transcript.rs` | 多段 delta | 内容追加到同一 Assistant entry |
| 同上 | 失败前无内容/已有部分内容 | 分别删除空占位/保留部分回复，并追加 Error |
| 同上 | Thinking、Markdown、多模型步骤 | 思考保留字面标记与弱化颜色，正文渲染 Markdown，步骤不混合 |
| 同上 | Snapshot 投影 | User/Assistant 保留，System 隐藏，顺序不变 |
| `widgets/prompt_editor.rs` | 普通输入、Backspace、Enter、提交重置 | `q` 等字符可输入，Enter 不新增行，取值后编辑状态恢复默认 |
| `widgets/command_menu.rs` | 双候选、前缀筛选、关闭、单项/空项导航 | 裸 `/` 两项；`/ex` 单项；无候选不 panic |
| `widgets/model_picker.rs` | 当前定位、空目录、循环、关闭与待确认状态 | 索引有效，关闭不选择；窗口不自行发送命令 |
| `ui.rs` | 标准布局与基本边框 | `TestBackend` 中四区位置正确 |
| 同上 | 空会话、消息、notice、窄窗口 | 关键文本可见且渲染不 panic |
| 同上 | 菜单矩形、覆盖后关闭、无匹配、极小窗口 | Prompt 不移动，历史画面恢复，不越界或 panic |
| 同上 | 居中模型窗口、Provider/host、当前标记 | 仅本会话提示可见，极小窗口不 panic |
| `event_loop.rs` | Effect 执行 | 命令发送、Redraw/Exit 聚合及 channel 错误正确 |
| `lib.rs` | 输入映射 | 修饰键/Release/Repeat、菜单键、Resize、翻页和鼠标映射正确 |

终端恢复还需要人工或 PTY 集成测试，不能由 `TestBackend` 证明。event loop 测试应使用
fake Session/channel，禁止访问真实模型服务。

## 后续设计

以下能力尚未实现，不能作为当前代码的既有契约：

- 模型目录过滤/分组、参数补全、鼠标选择，以及独立显式保存默认模型；不隐式写盘。
- `CancelTurn`；未来 `Ctrl+C` 在 Running 时应先取消，Idle 时才退出。
- Thinking 折叠与思考历史恢复仍待设计，跨事件重绘节流见 [绘制节流目标](thinking-display.md#绘制节流目标)。
- Paste 与 bracketed-paste 生命周期、动态多行输入、更多 slash 命令与鼠标菜单交互。
- pending_submit、Core 拒绝后的草稿恢复、退出完成确认、强制重绘恢复。
- 工具详情/来源卡片、审批弹窗、多行提交策略和会话恢复。
- 模型文本的终端控制字符过滤。

会话能力先扩展 Core 的语义契约，再投影到 TUI；纯展示、输入和滚动能力留在 TUI。
不得把 Provider wire 类型或 Ratatui 控件作为跨 crate 接口。

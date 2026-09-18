# TUI 屏幕模式、命令菜单与多行输入调研

- 调研日期：2026-09-15
- 范围：Codex、Claude Code 的终端呈现，对 Vortex `/exit`、slash 菜单和粘贴的影响
- 定位：证据与候选方案；建议尚未成为实现契约，落地时再更新 `docs/design/`
- Codex 源码基线：`459a79eb85400af759e9220c7bafb4429ae07516`，本地相关源码无未提交修改
- Vortex 基线：`3e17b20`；工作区已有命令相关草稿，本次不评审或修改这些草稿

## 1. 结论

1. 屏幕缓冲区、绘制区域、历史滚动归属和控件增长方向是四个独立决策，不能由“看起来全屏”推断全部实现。
2. 本地 Codex 使用 inline 聊天区域与终端 scrollback，同时允许特定界面进入 alternate screen。
3. Claude Code 官方明确提供 classic 与 fullscreen 两种渲染器；fullscreen 使用 alternate screen，输入区底部固定。不能把一个模式的观察归纳为整个产品的唯一行为。
4. Vortex 近期建议延续全屏和应用内 Transcript 滚动：Prompt 底部固定，多行向上增长，slash 菜单向上覆盖历史区域。
5. `/exit` 应经过命令解析；`/` 必须位于整个输入的首字符。粘贴只编辑草稿，执行和提交必须等待独立的确认动作。
6. Inline 作为独立实验保留；Ratatui 提供基础能力，但完整历史回放、resize 和动态 viewport 仍需要应用设计。

## 2. 证据等级与观察边界

本文使用三种标记：**源码事实**来自指定提交或本地锁定依赖；**官方说明**来自本次打开的官方页面；**观察/推断**来自作者实际体验或对现象的解释。

本次没有启动两款产品进行 PTY 录制，也没有修改目录信任状态。作者使用的二进制版本、终端与渲染配置尚未记录，不能声称本地 Codex checkout 与观察版本完全一致。Claude Code 内部布局调用链未做源码验证。

| 作者观察 | 本次核实结果 |
| --- | --- |
| Codex 新目录先出现全屏信任框 | Onboarding 请求最大绘制高度，有源码支持；不能仅凭外观认定切换了备用屏幕 |
| Codex 聊天像在 Shell 后继续输出，能滚到旧命令 | 与 inline viewport 和普通 scrollback 的源码一致 |
| Codex `/` 菜单向下展开，输入框上移，关闭后留空 | 菜单在 composer 下方且增加总高度已确认；留空可由部分 viewport 路径解释，具体终端仍需复现 |
| Codex 多行向下展开 | 输入高度按内容计算；外框在屏幕上的移动还取决于 viewport 锚点和剩余空间 |
| Claude 信任提示在底部，进入 Session 后全屏 | 信任提示位置仅记录为观察；官方确认存在独立 fullscreen 渲染路径 |
| Claude 菜单向上、多行向上增长 | 官方确认 fullscreen 输入区底部固定；菜单具体矩形和增长细节仍是观察 |

“未信任的目录”与“空会话”是不同状态：前者属于工作区许可流程，后者属于消息历史。信任提示所在位置也不决定后续 Session 的布局。

## 3. 四个独立的布局维度

| 维度 | 可选策略 | 决定什么 |
| --- | --- | --- |
| 屏幕缓冲区 | main / alternate | 与 Shell 是否共享当前显示缓冲区，退出时如何恢复 |
| 绘制区域 | 全终端 / inline 局部区域 | 应用每帧控制哪些行 |
| 历史归属 | 终端 scrollback / 应用 Transcript | 谁保存并滚动历史，搜索和复制由谁实现 |
| 布局锚点 | 顶部固定 / 底部固定 | 菜单或输入变高时，哪条边移动 |

Raw mode 负责输入处理方式，与是否全屏无关。Ratatui 的 viewport 负责绘制区域，也不能代替 Crossterm 的屏幕模式切换。

主屏幕可以绘制满屏；alternate screen 也可以只画几行。终端原生滚动区通常不用于保存备用屏幕上的持续重绘内容，具体滚轮、鼠标捕获和 tmux 行为需要按环境验证。

“复用终端滚动条”实际指将历史写入终端 scrollback；它不是把终端滚动条绑定到 `Paragraph::scroll`。终端保存的显示历史也不能代替应用用于模型请求、会话恢复的语义消息。

## 4. Codex 源码核实

### 4.1 普通聊天与全屏界面可以共存

`tui.rs` 的初始化说明明确写出 inline viewport 与普通 scrollback；`flush_pending_history_lines` 把历史写入活动区上方。`enter_alt_screen` 保存 inline 区域并扩大至终端尺寸，`leave_alt_screen` 恢复区域并使绘制缓存失效。[初始化、历史写入与屏幕切换源码][codex-tui]

`determine_alt_screen_mode` 计算是否允许 alternate screen，结果传给 `set_alt_screen_enabled`；这一步本身没有切换屏幕。Diff 事件处理才显式调用 `enter_alt_screen`。[模式开关][codex-lib]、[Diff 入口][codex-dispatch]

因此，旧调研中“auto 默认使用 alternate screen，只有 no-alt-screen 才保留 inline”的表述过于笼统，应区分“允许切换”与“聊天持续使用哪种模式”。官方 `--no-alt-screen` 描述也仅保证禁用 alternate screen。[Codex CLI 参数说明](https://learn.chatgpt.com/docs/developer-commands?surface=cli)

### 4.2 信任提示为何占满屏幕

Onboarding 的绘制调用使用 `tui.draw(u16::MAX, ...)`，实际尺寸受终端高度限制；这解释了信任提示的满屏外观。此调用本身不证明进入 alternate screen。相同流程还包含认证等步骤，不能把整个 Onboarding 与目录信任完全等同。[Onboarding 源码][codex-onboarding]

### 4.3 Slash 菜单为何在输入框下方

`layout_areas_with_textarea_right_reserve` 按纵向顺序分配 `composer_rect` 和 `popup_rect`，后者位于前者下面。`desired_height_with_textarea_right_reserve` 把编辑器高度、边距和活动菜单高度相加，所以菜单会增大整个活动区。[ChatComposer 布局源码][codex-composer]

这是一种参与布局的候选列表，不是简单在原有 Transcript 上覆盖一个浮层。模块名称叫 popup 并不能证明它采用覆盖绘制。

### 4.4 关闭菜单后留空的解释

`tui.rs` 的一条绘制路径保留 viewport 的起始行，修改高度；仅在底部超出终端时滚动上方内容并上移起始行。高度缩小时，该路径没有对称地重新贴底。菜单曾经把区域顶上去，关闭后便可能在下方留下空白。[viewport 调整源码][codex-tui]

这是源码支持的解释，不是跨版本保证：当前源码还有 resize-reflow 路径，终端滚动策略也会影响最终表现。Vortex 没有必要复制这种视觉残留。

## 5. Claude Code 官方行为

官方说明提供 classic 和 fullscreen 渲染器。Fullscreen 使用 alternate screen，只渲染可见消息，输入框固定在底部，历史由应用内滚动处理；classic 保留终端原生 scrollback。可用 `/tui` 查询当前渲染器，默认选择还受配置与环境影响。[Fullscreen rendering](https://code.claude.com/docs/en/fullscreen)

官方交互文档确认：`/` 可列出命令，后续字母筛选；Tab 接受补全；多行输入和直接粘贴均受支持。当前文档还支持正文中的部分命令补全，但执行命令要求命令位于消息开头。Vortex 可以采用更小的首字符规则，无须复制全部行为。[Interactive mode](https://code.claude.com/docs/en/interactive-mode)

较大粘贴可折叠为提示占位，提交时仍使用完整内容。这说明输入展示可以与原始草稿分离，但需要额外管理文本引用及恢复。近期 Vortex 无需引入这种机制，固定高度上限即可。[Terminal configuration](https://code.claude.com/docs/en/terminal-config)

官方资料没有在本次核实范围内明确规定 slash 菜单必须朝上、信任框必须位于最底部；这两项保留为特定环境观察，不作为稳定 API。

## 6. Ratatui 与输入库能提供什么

Ratatui 官方 inline 示例使用 inline viewport，并通过 `Terminal::insert_before` 插入已完成输出。标准库已能验证“上方历史、下方活动区”的基础模型；复杂 Agent 仍需解决动态高度、流式内容提交时机、resize 和历史重排。[Inline Viewport 示例](https://ratatui.rs/examples/apps/inline/)

Vortex 当前依赖 `ratatui-textarea 0.9.2`。以下依据本地下载的该版本源码核实，网页 API 抓取未成功，不依赖未打开的网页结论：

- `src/input/crossterm.rs` 的 `From<Event>` 只转换 Key 和 Mouse，Paste 落入默认分支，不能直接交给 `input(event)` 就期待插入文本。
- `src/textarea.rs::insert_str` 接受整段文字，支持 LF 与 CRLF；独立 CR 需要应用先规范化。
- 该版本存在 `set_wrap_mode`，默认 `WrapMode::None`。如果想长行自动折行，必须显式选择 wrap 策略，并让高度计算与编辑器一致。

Crossterm 0.29.0 的 `src/event.rs` 中，`Event::Paste(String)` 由 `bracketed-paste` feature 提供，终端还必须收到 `EnableBracketedPaste`。编译 feature 与运行时模式是两个条件。正常退出和清理路径应发出 `DisableBracketedPaste`。

## 7. Vortex 推荐方案

以下是候选实现约定，标出的参数可在详细设计时调整。

### 7.1 保持底部固定的全屏布局

- 保留现有 alternate screen 与 Transcript 内部滚动。
- Prompt 底边固定，新增逻辑行时向上增长；提交并清空后恢复最小高度。
- 首版建议内容高度为 1–6 行，并受实际可用高度限制；小终端优先保证至少一行可编辑内容。
- 超出高度上限后滚动编辑器内部视口，保证光标可见。
- 明确硬换行与软折行的区别：前者改变请求文本，后者只改变显示。首版可以先支持硬换行增长、长行水平滚动，软折行另行验证。
- 动态 Prompt 会改变 Transcript 高度，因此每次重绘同步其 viewport；菜单出现本身不改变 Transcript 尺寸。

### 7.2 Slash 菜单向上覆盖

菜单紧邻 Prompt 上方，最多显示若干候选项，空间不足时内部滚动或缩小；先绘制 Transcript，再清理菜单矩形并绘制候选列表。关闭时正常重绘底层内容，不保存空白占位。

菜单状态至少包含候选列表、选中项和是否被 Esc 关闭。输入发生变化时重新筛选并修正选中索引；不能每帧仅按 `starts_with('/')` 强制打开，否则 Esc 后下一帧又会出现。

建议按键路由优先级：退出快捷键 → 当前菜单 → 输入编辑器。菜单打开时 Up/Down 移动候选，关闭时恢复编辑器光标操作；两者都不应误滚动 Transcript。

### 7.3 解析、补全与执行分离

`/` 必须是整个草稿当前的首字符，而不是“用户曾经第一个按下的键”，也不是任意一行的首字符。禁止先 `trim()` 或 `trim_start()` 再检测前缀。

| 输入 | 推荐行为 |
| --- | --- |
| `/`、`/ex` | 显示和筛选候选 |
| `/exit` | 显式提交时退出 |
| `/exit `、`/exit\t` | 允许尾部空格、Tab，显式提交时退出 |
| ` /exit` | 普通 Prompt，保留开头空格 |
| `/exit now` | 参数错误，保留草稿，不退出 |
| `/exitfoo` | 未知命令，提示并保留草稿 |
| `/ exit` | 无效命令，不把它解析为 exit |
| `说明 /exit` | 普通 Prompt |
| 首字符为 `/` 的多行草稿 | 建议提示命令必须为单行并保留草稿，防止把部分粘贴内容当命令执行 |

建议先检查是否含 CR/LF，再拆命令名和参数；仅忽略参数边界的空格、Tab。无条件 `trim_end()` 会吞掉尾部换行，使 `/exit\n` 悄悄变成退出命令，这在粘贴场景需要避免。

补全与执行需要明确区分。建议 Tab 只补全；Enter 对完整有效命令执行，对 `/` 或 `/ex` 先接受候选并关闭菜单，下一次 Enter 才执行。后者是候选方案，重点是不能仅因菜单默认选中 Exit 就直接退出。

Esc 只关闭菜单并保留草稿；不隐式把命令改成普通 Prompt。`//` 转义、绝对路径 Prompt 的处理仍需确认，暂不宣称支持。

命令表统一保存名称、描述和可执行标识；菜单只展示已经实现的命令。首个可执行项为 Exit；工作区草稿中其他枚举值不代表功能已实现。

`/exit` 在 Running 时也应有效，并复用现有 Shutdown → Exit 顺序。普通 Prompt 的 Running 拒绝规则保持独立，解析不能以取走草稿为前提。

### 7.4 粘贴是一项编辑操作

推荐链路：`Event::Paste(text)` → `UiAction::PastePrompt(text)` → `PromptEditor::insert_text` → 重绘。

- 在当前光标位置插入，选中文本时按编辑器语义替换。
- CRLF 和独立 CR 规范化为 LF；粘贴中的换行不会触发 SubmitPrompt。
- 粘贴 `/exit` 只显示草稿，等待用户另外按 Enter；多行命令按上节规则处理。
- 不把粘贴文本逐字符模拟成快捷键，避免控制字符触发提交、退出或菜单动作。
- 显式声明 Crossterm `bracketed-paste` feature；TerminalGuard 同时管理开启和关闭。
- 首版不依赖系统剪贴板库，文本由终端发送；不支持 bracketed paste 的终端不能保证相同的多行提交隔离行为。

### 7.5 模块边界

| 模块 | 职责 |
| --- | --- |
| `command.rs`（已有草稿） | 命令描述、解析、参数校验；供补全与执行共用 |
| `widgets/prompt_editor.rs` | 原始草稿、光标、选择、整段插入与编辑器尺寸需求 |
| 建议新增 `widgets/command_menu.rs` | 候选项、选中项、关闭状态和列表渲染 |
| `app.rs` | 协调编辑、菜单和提交，产生 Shutdown/Exit 等 Effect |
| `ui.rs` | Prompt 动态高度、菜单矩形及覆盖绘制顺序 |
| `terminal.rs` | 屏幕与 bracketed paste 生命周期 |
| `event_loop.rs` / `lib.rs` | 事件接收、输入映射；含当前交互状态的决策由 app 协调 |

具体使用一个统一 Composer 封装 PromptEditor 与菜单，还是由 AppState 暂时组合，可在第二种补全需求出现后决定；现在不需要通用页面路由框架。

## 8. Inline 的收益与代价

| 项目 | 现有全屏方式 | Inline 方式 |
| --- | --- | --- |
| 历史浏览 | 应用控制偏移和滚动条 | 终端原生 scrollback |
| 退出效果 | 恢复 Shell 屏幕 | 可保留追加的历史输出 |
| 菜单与审批 | 在受控屏幕中覆盖容易 | 需考虑历史区、屏幕边界和临时全屏切换 |
| 改写历史 | 可重绘已有语义消息 | 已写入 scrollback 的内容难以任意编辑 |
| Resize | 可按当前窗口重排应用内容 | 需协调终端 reflow 与应用重新排版 |

如果以后选 inline，先用 Ratatui 官方例子验证固定活动区和 `insert_before`，再试动态 Prompt、SSE 未完成消息、完成历史写入和 resize。不能仅删除 `EnterAlternateScreen`，仍按全屏 Frame 画整个 Transcript。

## 9. 建议验收与后续顺序

1. 先确定单行命令解析、尾部空白、多行 slash、Tab/Enter 与 Esc 规则。
2. 完成可手输的 `/exit`，用 Idle/Running 场景验证它不会发送模型请求。
3. 接入 Paste 与 Prompt 动态高度，验证包含末尾换行的多行代码不会自动提交。
4. 增加向上覆盖的候选菜单，测试 `/`、筛选无结果、导航、补全和关闭恢复。
5. 在 `80×24`、窄窗口和很矮窗口验证菜单不越界、光标可见、底部无残留。
6. 补上 resize 重绘和按键 Release/Repeat 规则；使用 TestBackend 验证布局，PTY 验证终端模式恢复。

下一次观察两款产品时，应记录产品版本、终端版本、窗口尺寸、是否使用 tmux/SSH、渲染模式与启动参数；分别观察空会话、菜单展开/关闭、多行增长/删除、resize 和退出后的 Shell。信任流程使用专门测试目录，不清除已有项目的信任记录。

## 10. 与现有文档的关系

- [`rust-tui-research.md`](rust-tui-research.md)：保留最初的技术选型，本篇补充屏幕模式和输入行为，澄清 alternate screen 的解释。
- [`../design/tui-interface.md`](../design/tui-interface.md)：实施前更新 slash/Paste 事件、交互优先级与多行输入规则。
- [`../design/transcript-scrolling.md`](../design/transcript-scrolling.md)：保留内部滚动模型，动态输入高度会成为新的 viewport 变化来源。

[codex-tui]: https://github.com/openai/codex/blob/459a79eb85400af759e9220c7bafb4429ae07516/codex-rs/tui/src/tui.rs
[codex-lib]: https://github.com/openai/codex/blob/459a79eb85400af759e9220c7bafb4429ae07516/codex-rs/tui/src/lib.rs
[codex-dispatch]: https://github.com/openai/codex/blob/459a79eb85400af759e9220c7bafb4429ae07516/codex-rs/tui/src/app/event_dispatch.rs
[codex-onboarding]: https://github.com/openai/codex/blob/459a79eb85400af759e9220c7bafb4429ae07516/codex-rs/tui/src/onboarding/onboarding_screen.rs
[codex-composer]: https://github.com/openai/codex/blob/459a79eb85400af759e9220c7bafb4429ae07516/codex-rs/tui/src/bottom_pane/chat_composer.rs

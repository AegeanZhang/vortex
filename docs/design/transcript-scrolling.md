# Transcript 滚动设计

本文定义 TUI Transcript 当前的滚动行为；整体界面状态流与终端生命周期见
[`tui-interface.md`](tui-interface.md)。

## 目标与边界

当对话内容超过 Transcript 可视高度时，用户应能查看历史内容，并能快速回到最新消息。
滚动能力属于 `Transcript`，不新增含义模糊的 `MainArea` 组件：`ui.rs` 继续只划分区域，
`Transcript` 负责内容高度、滚动位置和滚动条渲染。

支持纵向滚动、键盘翻页、鼠标滚轮和自动跟随最新消息。Assistant 正文经 Markdown
转换后参与同一套测高与滚动；不包含滑块拖动、文本选择、搜索、水平滚动或虚拟化。

## 状态模型

`Transcript` 保存与内容相关的状态，不直接长期保存 Ratatui 的 `ScrollbarState`：

```rust
struct Transcript {
    entries: Vec<TranscriptEntry>,
    scroll: TranscriptScroll,
}

struct TranscriptScroll {
    offset: usize,
    follow_tail: bool,
    content_height: usize,
    viewport_height: usize,
}
```

`offset` 表示可视窗口顶部对应的渲染行，按换行后的终端行计算。`follow_tail` 为 `true`
时，新消息和 Assistant delta 到达后，下一次绘制自动定位到底部；用户向上滚动后设为
`false`。手动浏览期间追加内容不改变当前顶部位置。滚动到底部或执行“回到底部”后重新
启用自动跟随。

`content_height` 和 `viewport_height` 是最近一次绘制得到的度量值，用于 reducer 执行
翻页和边界限制。终端尺寸变化后由下一次绘制重新计算。当前 `Event::Resize` 不主动请求
重绘，因此尺寸变化会在后续其他事件触发绘制时生效。

## Action 与数据流

滚动仍遵循现有单向数据流：

```text
KeyEvent / MouseEvent -> UiAction::ScrollTranscript(command)
         -> update(AppState)
         -> Transcript 修改滚动状态
         -> Effect::Redraw
```

项目定义自己的 `ScrollCommand`，避免 Action 依赖 Ratatui 类型：

```rust
enum ScrollCommand {
    PageUp,
    PageDown,
    ToTop,
    ToBottom,
    LinesUp(usize),
    LinesDown(usize),
}
```

首版按键为 `PageUp`、`PageDown`、`Ctrl+Home` 和 `Ctrl+End`。普通方向键、Home 和 End
继续交给 `PromptEditor`，避免破坏输入框的光标移动。翻页距离为
`viewport_height.saturating_sub(1).max(1)`，保留一行上下文。

滚轮每个事件移动 3 个渲染行，首版作用于整个 TUI，不按鼠标坐标分区。键盘翻页复用
`scroll_up(lines)` / `scroll_down(lines)` 的边界与跟随规则；点击、拖动、水平滚动和移动
事件忽略。以后菜单或输入框需要独立滚动时，再增加命中区域与事件优先级。

`TerminalGuard` 进入时开启 `EnableMouseCapture`，显式清理和 `Drop` 均关闭
`DisableMouseCapture`。鼠标捕获可能影响终端原生选择，不能承诺各终端绕过方式相同。

滚动只产生 `Effect::Redraw`，不向 Core 发送命令，也不改变 Session 状态。

## 渲染与高度计算

`Transcript::render` 同时绘制外框、内容和纵向滚动条。为了更新最近一次绘制的尺寸与
合法偏移量，它接收 `&mut self`；相应地，`ui::render` 接收 `&mut AppState`。这种修改只
影响 widget 的视图状态，不产生网络或 Session 副作用。

绘制顺序如下：

1. 绘制标题为 `Transcript` 的外框，并取得边框内区域。
2. 为右侧滚动条预留一列，计算内容宽度和 `viewport_height`。
3. 使用与实际 `Paragraph` 相同的 `Wrap { trim: false }` 调用 `line_count`，得到换行后的
   `content_height`。
4. 计算 `max_offset = content_height.saturating_sub(viewport_height)`；自动跟随时将
   `offset` 设为 `max_offset`，否则把它限制在 `0..=max_offset`。
5. 使用 `u16::try_from(offset).unwrap_or(u16::MAX)` 转换后调用 `Paragraph::scroll`。
6. 仅在 `content_height > viewport_height` 时绘制 `VerticalRight` 滚动条；临时构造的
   `ScrollbarState` 使用以下参数。

```rust
let scroll_positions = max_offset.saturating_add(1);
let state = ScrollbarState::new(scroll_positions)
    .position(offset)
    .viewport_content_length(viewport_height);
```

本仓库使用的 `ratatui-widgets 0.3.2` 按 `content_length - 1` 计算最大位置，因此传入的是
合法顶部偏移的数量，而非正文总高度。例如正文 100 行、视口 20 行，最大偏移 80，需传 81，
不能传 100。升级依赖后重新核对这一映射；滑块触底指到达底部箭头上方，不覆盖箭头。

Markdown 先按消息转换，随后用同一个 Paragraph、内容宽度和 Wrap 参数测量及绘制，
不能按源码换行数、Span 数量或转换前文本长度猜测高度。固定预留滚动条列，避免滚动条
出现后改变换行宽度导致反复测高。转换边界见
[`ADR 0001`](../architecture/decisions/0001-transcript-markdown-rendering.md)。

所有宽高计算使用 `saturating_sub`。当区域窄到无法容纳边框、内容或滚动条时，允许只绘制
外框或空内容，但不能 panic。将 `usize` 转为 Paragraph 所需的 `u16` 时应使用受限转换，
不能直接截断。

## 内容变化规则

- `push_user`、`start_assistant`、`append_assistant_delta` 和 `fail_assistant` 不直接猜测
  换行高度；高度统一在下一次 render 中按当前宽度计算。
- 自动跟随模式下，任何内容追加都会在下一次 render 后保持最新一行可见。
- 手动模式下，内容追加保持 `offset` 不变，避免阅读历史消息时画面跳动。
- 从 Snapshot 创建 Transcript 时默认启用自动跟随，首次绘制显示会话末尾。
- 清空或缩短内容后，render 必须重新限制 `offset`；终端 resize 会在下一次绘制时执行相同
  修正。

保持数值偏移不等于保持语义阅读位置：Markdown 未闭合结构和 resize 可能使前文重排。
按消息/块保存阅读锚点属于后续优化。`u16` 饱和转换也只避免整数截断，不能解决超长历史
超过 Paragraph 偏移上限的问题；此时应考虑分块或虚拟化，而非宣称支持任意长度历史。

## 测试要求

`widgets/transcript.rs` 的单元测试覆盖滚动上下界、翻页距离、手动模式追加内容、回到底部
恢复自动跟随，以及 resize 后的偏移修正。长行测试必须验证按可视宽度换行，而不是只按
`entries` 或逻辑行数量计算。

`ui.rs` 使用 `TestBackend` 验证内容溢出时出现滚动条、滚动后历史文本可见，并覆盖极窄、
极矮区域不 panic。`lib.rs` 验证滚动快捷键优先映射为滚动 Action，普通编辑键仍传给
`PromptEditor`；滚轮映射为 3 行，点击与移动不产生 Action。测试不依赖真实终端或 Provider。

Markdown 样例覆盖未闭合代码围栏、长代码行、中英文、emoji 和表格；验证 User/Error
保持原文，两条 Assistant 独立解析。样式测试检查粗体等语义，不固定第三方默认 RGB。
滑块测试需检查顶部、底部的 buffer 单元格，不能只断言 `offset == max_offset`。

区分逻辑布局与真实终端差分输出：TestBackend 正常不能证明所有终端显示正常。遇到滑轨
断裂等现象，应保留可复现文本、宽度、终端及锁定依赖版本，再分别检查 buffer 和 PTY
输出；不能仅凭截图归因于测高或某个依赖。该视觉问题不在本次文档回收中修复。

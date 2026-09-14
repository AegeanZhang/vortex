# Transcript 滚动设计

本文定义 TUI Transcript 当前的滚动行为；整体界面状态流与终端生命周期见
[`tui-interface.md`](tui-interface.md)。

## 目标与边界

当对话内容超过 Transcript 可视高度时，用户应能查看历史内容，并能快速回到最新消息。
滚动能力属于 `Transcript`，不新增含义模糊的 `MainArea` 组件：`ui.rs` 继续只划分区域，
`Transcript` 负责内容高度、滚动位置和滚动条渲染。

首版只支持纵向滚动、键盘操作和自动跟随最新消息，不包含鼠标滚轮、文本选择、搜索、
Markdown 布局或虚拟化渲染。

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
KeyEvent -> UiAction::ScrollTranscript(command)
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
}
```

首版按键为 `PageUp`、`PageDown`、`Ctrl+Home` 和 `Ctrl+End`。普通方向键、Home 和 End
继续交给 `PromptEditor`，避免破坏输入框的光标移动。翻页距离为
`viewport_height.saturating_sub(1).max(1)`，保留一行上下文。

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
5. 使用 `Paragraph::scroll((offset as u16, 0))` 绘制内容。
6. 仅在 `content_height > viewport_height` 时绘制 `VerticalRight` 滚动条；临时构造的
   `ScrollbarState` 同时设置内容长度、当前位置和 viewport 长度。

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

## 测试要求

`widgets/transcript.rs` 的单元测试覆盖滚动上下界、翻页距离、手动模式追加内容、回到底部
恢复自动跟随，以及 resize 后的偏移修正。长行测试必须验证按可视宽度换行，而不是只按
`entries` 或逻辑行数量计算。

`ui.rs` 使用 `TestBackend` 验证内容溢出时出现滚动条、滚动后历史文本可见，并覆盖极窄、
极矮区域不 panic。`lib.rs` 验证滚动快捷键优先映射为滚动 Action，普通编辑键仍传给
`PromptEditor`。测试不依赖真实终端或 Provider。

# 终端画面恢复与强制重绘（待验证）

- 首次记录：2026-09-22；回收日期：2026-10-06。
- 状态：待复现、待定位；下述恢复机制是候选设计，未实现。
- 来源：本地 `2026-09-22-tui-display-loss.md`。关联 [TUI](tui-interface.md) 与
  [Transcript 滚动](transcript-scrolling.md)。

## 现象与证据边界

Kitty 中长时间运行 Vortex 后，PromptEditor 边框和状态栏文字消失，但仍可看到
`Type a message ...`。再次提交 Prompt 会恢复状态栏，输入框边框仍不恢复。
异常前用户应未 resize，但曾合盖；是否真正休眠、是否稳定发生在恢复后尚未确认。
Kitty 版本、显示环境、tmux/SSH 和后台输出情况待补充。

源码检查显示：默认 PromptEditor 有完整边框，提交时 mem::take 重建默认值；ui.rs
每次 render 都绘制状态与输入区域；event_loop.rs 按重绘请求 draw；lib.rs 忽略 Resize。
日志走文件，未发现 TUI 路径直接 println 的明显干扰来源。这些检查不等于排除全部
终端输出来源，也不能把忽略 Resize 直接认定为该异常的根因。

## 待验证的解释

候选原因是休眠恢复后实际终端画面与 Ratatui 上一帧缓存不同步。增量绘制比较内部
前后两帧；状态变化能重新输出部分文字，不变的边框则可能没有再次输出。
这与现象相符，但未复现或证明，不能归因于 Kitty、Ratatui 或其他依赖的 bug。

周期性 draw 不保证重绘未变化的单元格。检测到真实尺寸变化时 autoresize 也可能重置
缓存，因此忽略 Resize 并不足以解释所有持续缺失情况；需要分别检查恢复与 resize。

## 排查与候选实现步骤

1. 补齐环境记录；分别验证单纯空闲、合盖未休眠、实际休眠恢复，记录触发时间和输出。
2. 无需改代码的临时验证：异常后明显拉大窗口并保持新尺寸，再输入普通字符触发
   draw，观察边框是否恢复；不以此代替根因定位。
3. 获得代码修改授权后，在 map_event 中增加 Ctrl+L 对应 UiAction；AppState 产生
   独立的强制重绘 Effect，不修改草稿、会话或历史。
4. event_loop 执行 Effect 时调用 Ratatui 的 `terminal.clear()?`，再请求正常绘制。
   不能只向终端发送清屏 ANSI 而不处理缓存，也不能每帧 clear 引入闪烁。
5. 独立增加 Resize → 请求重绘，检查空闲时调整尺寸也及时更新布局。必要时将尺寸
   信息作为事件输入，但不将终端控制搬进 widget。
6. 强制刷新有效仅证明恢复手段可用；继续调查合盖、后台输出或终端状态变化的触发源。

## 验收

- Ctrl+L 能恢复状态栏与边框，不丢草稿、Transcript、滚动状态或 Session 状态。
- 空闲 resize 更新布局，正常流式输出与滚动不产生每帧清屏闪烁。
- reducer/Effect 测试验证请求与状态不变；TestBackend 验证正常布局。
- Kitty 与实际休眠恢复仍需人工验证，不能以 TestBackend 通过宣称问题已修复。

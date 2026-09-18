# Transcript Markdown 渲染调研

- 调研日期：2026-09-17
- 范围：Ratatui 中的 Markdown 展示、候选库、Codex / Claude Code 实现对照
- 定位：技术调研与候选方案，不代表已经确定依赖或授权修改代码
- 关联设计：[`TUI 接口`](../design/tui-interface.md)、[`Transcript 滚动`](../design/transcript-scrolling.md)
- 关联调研：[`TUI 技术选型`](rust-tui-research.md)、[`屏幕模式与输入交互`](tui-layout-and-input-research.md)

后续决策已记录于 [`ADR 0001`](../architecture/decisions/0001-transcript-markdown-rendering.md)：
首版采用 `tui-markdown`，优先搭建可用展示。本文保留调研时的候选比较与证据边界；
具体选型以 ADR 为准，不将候选建议视为全部必须实施的要求。

## 1. 结论

1. `Paragraph` 可以显示带样式的文本，但不会自行解析 Markdown。需要在原始消息与
   `Paragraph` 之间增加 Markdown 到 Ratatui `Text` / `Line` / `Span` 的转换。
2. 有现成的 `tui-markdown` 可供试用，但作者仍将它定位为实验性项目，不能等同于完整、
   稳定的终端 Markdown 排版引擎。
3. Codex 复用 `pulldown-cmark` 解析器，自行实现终端排版，代码高亮复用 `syntect`。
   它的这条渲染链路没有使用 `tui-markdown`。
4. 本机 Claude Code 安装包中可以识别 `marked` 解析与 token 渲染流程，以及 Ink 相关
   运行时代码；不能据此断言其使用未经修改的上游 Ink，也不能证明所有高亮调用路径。
5. Vortex 应保持“原始消息、Markdown 转换、Transcript 布局与滚动”分层。快速验证可用
   `tui-markdown`；以学习排版细节为目标，可以选择 `pulldown-cmark` 加项目自己的渲染层。

## 2. 证据范围

| 对象 | 本次依据 | 限制 |
| --- | --- | --- |
| Rust 候选库 | 上一轮已打开的项目 README、API 文档及 manifest | 文档版本不等于在 Vortex 中完成兼容性测试 |
| Codex | 本地提交 `5bf132cd527311eb61bbec46562e3890eb49df80` 的源码 | 相关文件无未提交修改；不代表所有已分发二进制版本 |
| Claude Code | 本机 `2.1.273` 安装包的静态检索 | 不是完整可读源码审计，也没有运行时调用跟踪 |

本次整理没有安装依赖、修改产品代码或录制真实终端效果。下面的 Vortex 建议均为候选方案。

## 3. Rust 候选方案

### 3.1 tui-markdown：直接接入 Ratatui

`tui_markdown::from_str` 将 Markdown 转成 Ratatui `Text`，可以继续交给 `Paragraph`
显示。查阅的 API 文档版本为 `0.3.9`；默认 `highlight-code` feature 使用 `syntect`
和 `ansi-to-tui` 提供代码高亮，也可以关闭该 feature，先验证基础展示。[API 文档][tui-markdown-api]

当前查阅的仓库 manifest 使用 `ratatui-core 0.1`，示例使用 `ratatui 0.30`，与 Vortex
的版本方向一致；正式引入仍需验证依赖解析和类型兼容性，不能只凭版本号保证可编译。
[Workspace manifest][tui-markdown-workspace]、[crate manifest][tui-markdown-manifest]

项目 README 仍标为实验性 Proof of Concept，并明确不是所有 Markdown 特性都得到完整支持。
因此适合先做小规模验证，重点检查实际模型回复中的列表、代码块、表格和换行。
[项目说明][tui-markdown-readme]

接入形态示意，不是完整的 Transcript 实现：

```rust
let text = tui_markdown::from_str(&entry.content);
let paragraph = Paragraph::new(text).wrap(Wrap { trim: false });
```

### 3.2 pulldown-cmark：复用解析，自行实现展示

`pulldown-cmark` 提供 CommonMark 解析事件；表格、任务列表等扩展通过解析选项开启。
它不是 Ratatui widget，需要项目自己将事件映射为文本和样式。[官方文档][pulldown-cmark]

优点是可以明确控制代码块、列表续行缩进、链接显示、表格宽度与不完整 Markdown 的展示；
代价是要承担排版、Unicode 宽度、边界降级和回归测试。这条路线更适合深入学习 CLI Agent
的展示细节，但不应从零编写 Markdown 语法解析器。

### 3.3 termimad：有终端排版能力，但不作为当前首选

`termimad` 提供 Markdown 主题、换行和滚动视图，但采用自己的终端输出及布局接口。
这与直接生成 Ratatui `Text` 的接入方式不同。[官方文档][termimad]

对 Vortex 而言，引入另一套视图与滚动接口需要额外适配；不建议绕过 Ratatui frame 直接
打印内容，也不建议仅为 Markdown 替换现有 Transcript 滚动状态。

## 4. Codex 的实现

### 4.1 依赖与职责

| 层次 | 实现 |
| --- | --- |
| Markdown 语法解析 | `pulldown-cmark` |
| 终端排版、列表、表格、链接与换行 | Codex 自己的 Markdown 渲染代码 |
| 文本样式和界面绘制 | Ratatui |
| 代码语法高亮 | `syntect`，搭配 `two-face` 语法和主题资源 |
| 流式输入整理 | Codex 自己的 Markdown collector 与 streaming controller |

这些结论来自本地 `codex-rs/tui/Cargo.toml` 及实际调用路径，而不只是依赖列表。

### 4.2 建议阅读顺序

本地仓库：`/home/mingzhang2/Workspace/Projects/Opensource/openai/codex`。
以下路径均相对该仓库，按前述提交读取：

1. `codex-rs/tui/src/markdown.rs`：供其他 TUI 模块调用的入口，将消息源文本送入渲染层；
   包含针对 Agent 输出的预处理。
2. `codex-rs/tui/src/markdown_render.rs`：消费 `pulldown-cmark` 事件，输出 Ratatui 行与
   样式，处理可用宽度、表格布局和文件链接显示。
3. `codex-rs/tui/src/render/highlight.rs`：封装 `syntect` / `two-face`，把高亮结果转为
   Ratatui 文本；包括输入大小限制和无法高亮时的降级。
4. `codex-rs/tui/src/markdown_stream.rs`：缓冲 token delta，在换行边界提供已完成前缀，
   收尾时交出完整源文本，包括没有结尾换行的最后一行。

换行边界只是流式提交策略，不代表每个前缀都是完整的 Markdown 块。代码围栏、表格和列表
仍可能随着后续内容到达而改变解释，不能简单把“按换行提交”等同于“逐行独立解析”。

## 5. Claude Code 的核实结果

检查对象为 `/home/mingzhang2/.local/share/claude/versions/2.1.273`，其 SHA-256 为：

```text
6c752e2cc7c110c9df15f26d8d134d438c5ae95dbd610efc1a308bf7f9c5f6c1
```

静态检索中观察到：

- `marked` 的项目标识、解析代码与 `lexer()` 调用；解析结果经过 token 映射及按类型处理。
- Ink 的终端输入错误信息及相关运行时代码。
- `highlight.js` 的代码特征与项目链接，但没有完整追踪各处代码块的高亮调用链。

因此，可以将这版实现概括为“`marked` 解析，加产品自己的 token 展示逻辑，接入终端 UI”。
证据不足以断言其使用某个现成 Markdown 控件、未经修改的 Ink，或所有代码块都由
`highlight.js` 高亮。打包产物中出现依赖，也不能单独证明该依赖参与每个界面的实际渲染。

与 Codex 的对照重点是职责分层，而不是要求两款产品采用相同的库。Claude Code 的内部
实现结论仅适用于本次检查对象，不外推为未来版本的稳定契约。

## 6. 对 Vortex 的候选设计

### 6.1 模块边界

- `TranscriptEntry.content` 保留原始消息文本；展示转换不能改写发送给模型或持久化的内容。
- 在 `vortex-tui` 内增加薄的 Markdown 转换模块；先使用普通函数，不预设复杂 trait。
- Markdown 模块只生成文本与样式，不操作终端、不管理 Session，也不直接控制滚动位置。
- `Transcript` 组合角色标签、消息间距和正文，统一计算高度并管理滚动条与自动跟随。
- Core / Provider 不依赖 Markdown 渲染库；用户输入和错误消息首版可继续按纯文本展示。

### 6.2 与滚动、流式输出的衔接

1. **按消息解析，而不是按源文本行解析。** 代码块、列表和表格都存在跨行结构。
2. **消息之间隔离。** 不要把整个会话拼成一份 Markdown，否则未闭合结构可能影响下一条消息。
3. **按最终显示内容测高。** 在仍由 `Paragraph` 换行的方案中，对实际渲染的同一个
   `Paragraph` 调用 `line_count(content_area.width)`；不能按原始 Markdown 行数计算滚动范围。
4. **避免重复换行。** 若未来渲染器自行按宽度排版，需重新约定与 `Paragraph::wrap` 的分工，
   确保测量与绘制采用同一布局。
5. **先完整消息，后流式优化。** 后续需要处理未闭合围栏、回流导致的行数变化和手动浏览时
   的视口稳定性；Markdown 库不会自动解决这些交互问题。
6. **按需缓存。** 避免滚轮重绘时反复解析所有历史消息；内容、主题或相关布局宽度变化时，
   对应缓存必须失效。缓存含借用的 `Text` 时还需明确所有权与生命周期。

### 6.3 路线建议与验收样例

若优先跑通展示，可先用 `tui-markdown` 做验证；若优先学习解析事件与终端排版，则用
`pulldown-cmark` 逐步实现段落、标题、强调、列表和代码块，随后再接 `syntect`。
两条路线都复用解析库，不从头实现 Markdown 语法；具体选择应在实施前由作者确认。

最小验证集应包含中文与英文混排、emoji、嵌套列表、长代码行、宽表格、链接、未闭合代码
围栏，以及消息隔离。使用 `TestBackend` 检查窄窗口、渲染高度、底部滚动位置和回到底部
后的自动跟随。语法高亮不应阻塞输入；长内容需要测量性能，并准备纯文本降级策略。

Markdown 转换也不是安全过滤器：模型文本中的终端控制字符仍需单独制定处理规则，不应
自动执行代码、打开链接或读取图片资源。本次调研不实现这些能力，也不据此宣称已安全处理。

## 7. 资料入口

外部资料查阅于 2026-09-17。`latest` 与 `main` 链接会变化；具体实现判断以前述版本和证据
边界为准。Codex 的实现证据是第 4 节指定的本地源码，Claude Code 的依据是第 5 节安装包，
不能把一般产品文档当作内部渲染器的证明。

[tui-markdown-api]: https://docs.rs/tui-markdown/0.3.9/tui_markdown/
[tui-markdown-readme]: https://github.com/joshka/tui-markdown/tree/main/tui-markdown
[tui-markdown-workspace]: https://github.com/joshka/tui-markdown/blob/main/Cargo.toml
[tui-markdown-manifest]: https://github.com/joshka/tui-markdown/blob/main/tui-markdown/Cargo.toml
[pulldown-cmark]: https://docs.rs/pulldown-cmark/0.13.4/pulldown_cmark/
[termimad]: https://docs.rs/termimad/0.35.4/termimad/

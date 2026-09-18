# ADR 0001：使用 tui-markdown 实现首版 Transcript Markdown 展示

- 日期：2026-09-17
- 状态：已接受；实施与验证另行进行
- 范围：`vortex-tui` 的 Assistant 正文展示
- 依据：[`Transcript Markdown 调研`](../../research/transcript-markdown-research.md)

## 背景与决策逻辑

Vortex 还有工具调用、记忆、上下文管理等核心能力需要实现。当前目标是尽快获得可用的
Markdown 展示框架，而不是在这一阶段深入实现终端排版引擎。学习和优化渲染细节仍有价值，
但不应阻塞 Agent 主链路的演进。

因此，选择 `tui-markdown` 完成首版转换，通过小型适配模块保留替换空间。不因为其他产品
采用自定义渲染器，就立即承担同等复杂度；也不将快速接入理解为把第三方 API 扩散到全项目。

## 决策

1. 使用 `tui-markdown`，以调研核对的 `0.3.9` 为接入基线，实际解析版本由根目录
   `Cargo.lock` 固定。依赖只由 `vortex-tui` 消费，Workspace 集中声明版本。
2. 首版关闭默认 features，暂不启用 `highlight-code`。先完成基础 Markdown 样式、代码块
   展示和滚动；语法级代码高亮不是首版验收条件，后续可在 TUI crate 按需开启。
3. 在 `vortex-tui/src/markdown.rs` 中封装转换函数，只返回 Ratatui 文本行，不引入公共
   renderer trait、新 crate 或另一套终端输出机制。
4. 每条 Assistant 正文单独、完整地解析。User 和 Error 保持纯文本，角色标签和消息间距
   由 Transcript 添加；原始 `TranscriptEntry.content` 不变。
5. `ui.rs` 继续分配 `main_area`；Transcript 继续负责 `Paragraph`、换行、高度、滚动条
   和自动跟随。Core、Provider、会话数据与发给模型的请求不依赖 Markdown 展示结果。
6. 首版每次重绘从消息源文本生成展示行，不引入缓存。沿用现有 delta 累积方式，不逐行或
   逐 delta 独立解析；这不等于已经解决真正流式输出中的排版稳定性。

`from_str` 返回的 `Text` 可能借用输入；适配层与调用方必须尊重该生命周期，不能直接
把借用结果保存为 `'static` 缓存。[API 说明](https://docs.rs/tui-markdown/latest/tui_markdown/fn.from_str.html)

## 收益、代价与不做的事

- 收益：直接接入已有 Ratatui 绘制链路，改动集中，尽快用于日常测试。
- 代价：接受库的默认表现和实验性项目风险；复杂列表、宽表格、长代码行的效果需要验证，
  不承诺浏览器级 Markdown 一致性。上游定位见[项目说明](https://github.com/joshka/tui-markdown/tree/main/tui-markdown)。
- 暂不实现代码复制、链接跳转、图片加载、水平滚动、自定义主题、虚拟化和流式块提交。
- 库不是安全过滤器。不自动执行代码、打开链接或加载资源；终端控制字符仍是独立待办，
  不能因引入 Markdown 就宣称已经处理。
- 本决策不授权自动修改源码或 manifest；开发仍遵守项目学习模式。

## 接入约束与验收

- 以实际渲染的同一个 `Paragraph` 和内容宽度调用 `line_count`，禁止按 Markdown 源码行数
  计算滚动范围；继续为滚动条预留一列。
- 转换为 `Vec<Line>` 时保留 Text / Line / Span 的样式与对齐继承，不能先 `to_string()`
  再显示，否则会丢失 Markdown 样式。
- 不把整个会话拼成一份 Markdown，防止未闭合代码围栏污染下一条消息。
- 用测试验证样式、消息隔离、原始内容不变、窄窗口、中文、emoji、到底位置和自动跟随。
- Cargo 类型兼容性以实际 `cargo check` 为准；依赖文档与上游 manifest 不能替代本仓库构建。

## 后续优化与重构触发条件

| 触发条件 | 后续方向 |
| --- | --- |
| 历史消息较多，滚动重绘解析成本明显 | 按消息缓存拥有所有权的展示结果；内容/主题变化时失效，宽度参与排版后也纳入缓存键 |
| 接入真正的 token streaming | 缓冲与重绘节流；处理未闭合围栏、列表回流和历史阅读锚点，不假设换行就表示 Markdown 块已完成 |
| 默认代码样式不够用 | 在 TUI 开启 `highlight-code`，评估 syntect 带来的体积与耗时，再考虑主题与大代码块降级 |
| 宽表格、列表续行或代码布局频繁影响可读性 | 明确宽度感知排版与 Paragraph 换行的职责，避免重复换行；补充布局回归样例 |
| 库的维护、缺陷或可定制性持续阻塞需求 | 保持适配层入口，改用 `pulldown-cmark` 解析事件加项目自定义 Ratatui 排版；不自研语法解析器 |
| 超长历史导致内存/延迟或 Paragraph 的 u16 偏移上限成为问题 | 评估可见区域布局、分块/虚拟化渲染和偏移模型，而不是只强制截断偏移 |
| 面向更广泛的终端/主题环境 | 统一主题、无色输出、Unicode 宽度测试，并单独设计控制字符过滤 |

上述方向来自调研中的演进建议，不是首版必须同时完成的任务。先用真实消息和测试暴露
具体问题，再根据收益决定优化顺序；任何替换均保留原始消息与展示层的边界。

## 重新评估方式

升级依赖或替换渲染器时，沿用同一组消息样例，对照可读性、滚动正确性、重绘耗时和依赖
成本。改变技术路线时新增后继 ADR，并更新本记录状态及
[`technology-stack.md`](../technology-stack.md)，保留决策历史。

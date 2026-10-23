# 调研文档索引

本目录保存技术选型、外部产品对照和实现前探索，主要回答“有哪些方案”和“为什么倾向某个方案”。调研结论不是已经完成的功能，也不自动授权修改代码；确定会长期约束实现的决定应进一步写入 `docs/architecture/decisions/`，具体接口契约则写入 `docs/design/`。

## Agent 与外部能力

- [`agent-loop-research.md`](agent-loop-research.md) —— Codex 与 Claude Code 历史快照的 Agent loop 对照；2026-10-22 补充 ReAct、Prompt/上下文装配、Plan–Execute 工程编排与 Plan Schema，区分模型决策和程序强制规则。
- [`web-search-research.md`](web-search-research.md) —— 国内外搜索候选、网络与费用边界；2026-10-02 补充 Qwen 模型侧搜索、DeepSeek V4 Pro 五次 API 实验，以及 Codex/Claude Code/DeepSeek Harness/WorkBuddy 的 CLI/桌面搜索设计与自定义模型边界；2026-10-05 记录秘塔正常搜索的作者反馈与后续验证边界。

## TUI 与内容展示

- [`rust-tui-research.md`](rust-tui-research.md) —— Rust TUI 技术栈与 Ratatui、Crossterm、Tokio 选型。
- [`tui-layout-and-input-research.md`](tui-layout-and-input-research.md) —— Codex/Claude Code 的屏幕模式、Slash 菜单、多行输入和粘贴行为对照。
- [`transcript-markdown-research.md`](transcript-markdown-research.md) —— Markdown 渲染候选、外部实现对照，以及 Transcript 排版和流式展示方向。

## CLI 与配置

- [`cli-design-research.md`](cli-design-research.md) —— Claude Code 与 Codex CLI 的命令、权限、会话和输出契约对照。
- [`credential-handling-research.md`](credential-handling-research.md) —— API 密钥、`.env` 和本地凭据处理方式对照。

## 阅读与维护约定

准备改变既有设计前，先阅读相关调研及其引用的 ADR/设计文档。新增调研应说明日期、证据范围、结论的适用边界和关联设计；易变化的价格、配额和产品能力必须标注查询日期并链接官方来源。候选方案被采纳后，在 ADR 或设计文档中记录最终决定，并从调研文档链接过去；不要删除当时的比较依据。

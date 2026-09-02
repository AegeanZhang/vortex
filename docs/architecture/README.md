# 架构文档

本目录记录 Vortex 已确定的系统结构与技术决策，不记录实现进度。当前进度见仓库根目录的 [`README.md`](../../README.md)，候选方案与外部对照见 [`../research/`](../research/)。

建议按以下顺序阅读：

1. [`overview.md`](overview.md)：系统边界、crate 职责、依赖方向和演进顺序。
2. [`repository-structure.md`](repository-structure.md)：仓库目录、Workspace 组织和命名约束。
3. [`technology-stack.md`](technology-stack.md)：已经确定的关键技术选型与使用边界。
4. [`glossary.md`](glossary.md)：跨文档共用的核心术语。

具体子系统的对外契约位于 [`../design/`](../design/)，工程实践位于 [`../engineering/`](../engineering/)。

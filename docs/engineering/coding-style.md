# 编码规范

## Rust 基线

- 使用稳定版 Rust 和 2024 edition。
- 接受 `rustfmt` 默认格式，避免与功能无关的手工排版。
- 提交前使用 `cargo fmt --all -- --check` 检查格式；是否执行会改写源码的 `cargo fmt`，遵循根目录 `AGENTS.md` 的修改授权边界。

## 命名

| 对象 | 约定 | 示例 |
| --- | --- | --- |
| 模块、Rust 文件、函数、变量 | `snake_case` | `tool_result`、`event_stream.rs` |
| 类型与 trait | `PascalCase` | `AgentEvent`、`Provider` |
| 常量 | `SCREAMING_SNAKE_CASE` | `DEFAULT_TIMEOUT` |
| Cargo package 与非 Rust 文件 | `kebab-case` | `vortex-core`、`cli-surface.md` |

错误跨 crate 的传播与 CLI 展示属于架构边界，见 [`../architecture/overview.md`](../architecture/overview.md)；Tokio 与阻塞操作规则见 [`../architecture/technology-stack.md`](../architecture/technology-stack.md)。

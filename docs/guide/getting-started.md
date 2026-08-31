# 上手指南

本文只描述**当前真实可运行**的内容。产品命令尚未实现，其目标形态见
[`../design/cli-surface.md`](../design/cli-surface.md)，不要把设计文档里的命令当成现在能用的。

## 环境要求

- 稳定版 Rust，edition 2024。开发环境实测版本为 `cargo 1.98.0` / `rustc 1.98.0`。
- Linux。沙箱能力将依赖 `bubblewrap`，但相关功能尚未实现，现阶段不需要安装。

版本确认：

```bash
cargo --version
rustc --version
```

## 构建

```bash
cargo build
```

workspace 设了 `default-members = ["crates/*"]`，所以 `cargo build` 只编译产品 crate，
不会连带编译 `playground/` 下的实验。要包含实验时显式加 `--workspace`。

## 提交前的检查

与 `AGENTS.md` 中的约定一致，四条命令依次检查格式、静态问题、完整测试和发布构建：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release
```

注意 `--workspace` 会覆盖 `playground/` 下的实验 crate——`default-members` 只影响默认构建范围，
不会让检查漏掉实验代码。

## 运行 TUI 实验

```bash
cargo run -p tui-demo
```

`playground/tui-demo` 是学习用的终端界面实验，会进入全屏备用屏幕，按 `q` 退出。
它是交互式程序，需要真实终端，无法在管道或 CI 中验证。
其分阶段方案与进度见 `playground/tui-demo/docs/implementation-plan.md`。

## 产品命令的当前状态

`cargo build` 会产出 `target/debug/vortex` 二进制，但它**尚未实现任何产品命令**——
既不能执行任务，也没有 TUI 和会话恢复。

计划中的命令与参数（`vortex run`、`vortex resume`、审批与沙箱参数、输出契约、退出码）
全部记录在 [`../design/cli-surface.md`](../design/cli-surface.md)。该文档描述的是目标形态，
实现按其中的阶段表推进；本手册会随实现逐步补充，只写已经跑通的部分。

## 文档导航

各类文档的用途见 [`../README.md`](../README.md)。

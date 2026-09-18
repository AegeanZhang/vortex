# 上手指南

本文只描述**当前真实可运行**的内容。产品 TUI 已有非流式 v1；其他命令和后续能力的
目标形态见 [`../design/cli-surface.md`](../design/cli-surface.md)，不要把设计文档中的
未实现部分当成现有功能。

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

根 Workspace 只包含 `crates/*`，所以 `cargo build` 和根目录的 `cargo build --workspace`
都只编译产品 crate。实验使用独立的 `playground/Cargo.toml`：

```bash
cargo build --manifest-path playground/Cargo.toml --workspace --locked
```

产品使用根目录 `Cargo.lock` 和 `target/`；实验使用 `playground/Cargo.lock` 和
`playground/target/`。两个锁文件都应提交，两个构建目录均被现有 Git 规则忽略。

## 提交前的检查

与 `AGENTS.md` 中的约定一致，四条命令依次检查格式、静态问题、完整测试和发布构建：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release
```

以上命令只覆盖产品 Workspace。修改实验时单独检查：

```bash
cargo check --manifest-path playground/Cargo.toml --workspace --locked
cargo test --manifest-path playground/Cargo.toml --workspace --locked
```

## 运行 TUI 实验

```bash
cargo run --manifest-path playground/Cargo.toml -p tui-demo
```

`playground/tui-demo` 是学习用的终端界面实验，会进入全屏备用屏幕，按 `q` 退出。
它是交互式程序，需要真实终端，无法在管道或 CI 中验证。
其分阶段方案与进度见 `playground/tui-demo/docs/implementation-plan.md`。

Rust 入门实验使用 `cargo run --manifest-path playground/Cargo.toml -p hello-rust`。
也可以先 `cd playground`，再运行 `cargo run -p tui-demo` 等短命令。

如果 VS Code 在仓库根目录只加载了产品 Workspace，可单独打开 `playground/` 窗口来
编辑实验；编辑器的项目加载与 Git 是否跟踪该目录是两回事。

## 产品命令的当前状态

启动产品 TUI 前，把 DeepSeek API key 导出到当前进程环境：

```bash
DEEPSEEK_API_KEY=<key> cargo run -p vortex-cli
```

产品本身不读取 `.env`。进入 TUI 后可输入 Prompt 并按 Enter 提交；当前等待完整模型回复后
一次显示结果，按 `Ctrl+C` 退出。Provider URL 与模型暂由 CLI 固定装配。

当前尚未实现真正流式输出、工具调用、会话恢复、`/model`、Turn 取消和沙箱。已实现代码的
设计入口是 [`../design/session-runtime.md`](../design/session-runtime.md) 和
[`../design/tui-interface.md`](../design/tui-interface.md)。

手动验收时依次确认：

1. 初始状态栏显示 `Idle`。
2. 提交非空 Prompt 后出现 `You`，随后状态变为 `Running`。
3. 完整模型回复显示为 `Assistant`，状态回到 `Idle`。
4. Provider 错误显示为 `Error`，但 TUI 不退出。
5. 请求期间按 `Ctrl+C` 能结束 Session，并恢复原始终端。
6. 日志与错误信息中没有 API key 原文。

计划中的命令与参数（`vortex exec`、`vortex resume`、审批与沙箱参数、输出契约、退出码）
全部记录在 [`../design/cli-surface.md`](../design/cli-surface.md)。该文档描述的是目标形态，
实现按其中的阶段表推进；本手册会随实现逐步补充，只写已经跑通的部分。

## 文档导航

各类文档的用途见 [`../README.md`](../README.md)。

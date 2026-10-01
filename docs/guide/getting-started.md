# 上手指南

本文说明产品和实验的构建、运行入口。产品 TUI 使用文本 SSE 与 Mock 工具循环，验证边界
以根 README 为准；不将源码接入等同于真实模型与终端已验收。其他命令和后续能力的
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
cargo build --locked
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
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --release --locked
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

当前源码的 TUI 分支装配千问 Provider，启动前把相应 API key 导出到进程环境：

```bash
export VORTEX_HOME="$PWD/.vortex"
export QWEN_API_KEY='<your-api-key>'
cargo run -p vortex-cli --locked
```

产品本身不读取 `.env`。输入 Prompt 后按 Enter 提交，输入 `/exit` 后按 Enter 退出。
当前 Ctrl+C 映射已被注释；Provider URL 与模型暂由 CLI 固定装配，千问思考和思考历史均
显式关闭。VORTEX_HOME 必须是非空绝对路径，日志初始化失败会在启动 TUI 之前返回错误。
上面的 `.vortex/logs/` 已被 Git 忽略；真实模型与终端验收记录以根 README 为准。

源码中的响应链路已改为 SSE 增量展示，不再等待完整回答后一次显示；它按服务端增量更新，
不是本地逐字动画。具体体验须按下列清单验收。CLI 注册的 web_search 只返回 Mock 数据，
不会访问 Tavily 或其他搜索服务；它不能提供真实天气等实时信息。模型本身仍是远程服务，
因此会发送 Prompt、工具定义和 Mock 结果并可能产生模型费用。Thinking、定时刷新、真实搜索、
会话恢复、`/model`、保留会话的 Turn 取消和沙箱尚未实现。已实现代码的
设计入口是 [`../design/session-runtime.md`](../design/session-runtime.md) 和
[`../design/tui-interface.md`](../design/tui-interface.md)。

文本流式的手动验收清单（待执行，不是通过记录）：

1. 初始状态栏显示 `Idle`。
2. 提交非空 Prompt 后出现 `You`，随后状态变为 `Running`。
3. `Assistant` 在请求完成前逐段出现，流式期间保持 `Running`，成功结束后回到 `Idle`。
4. Provider 错误显示为 `Error`，保留已经显示的片段且 TUI 不退出；下一条 Prompt 可提交。
5. 请求期间提交 `/exit` 能结束 Session，并恢复原始终端与鼠标行为。
6. 浏览历史时新增内容不强制跳底；回到底部后继续自动跟随。
7. 日志与错误信息中没有 API key 或请求/响应正文。

Mock 工具的手动验收（仍需真实模型配合，不是自动化测试）：

```text
请调用 web_search，查询 Rust CLI Agent；明确说明结果是 Mock 数据，不要当成实时检索。
```

- 模型选择调用工具后，状态栏显示工具开始/完成，Turn 期间保持 Running。
- 完成后由模型继续流式回答，而不是把工具 JSON 直接当最终回答。
- 验证第二次模型请求含 ToolCall/ToolResult，结果含 `mock: true`；用受控 fake 测试
  断言请求，比依赖模型回答或瞬间可见的状态栏更可靠。
- 工具版只显示 notice，没有来源卡片；每个模型步骤最多一个调用，每个 Turn 最多 4 次。

详细接口见 [Agent loop](../design/agent-loop.md)；补测试前不要把手动成功一次视为完整验收。

现有 `exec` 分支未迁移到本次流式 Session 链路，不使用本节清单验收。完整命令目标
（exec 模式、`vortex resume`、审批与沙箱参数、输出契约、退出码）记录在
[`../design/cli-surface.md`](../design/cli-surface.md)，不可把其中所有参数视为已经实现。

## 日志与本地配置

所有模式启动时都初始化日志，以 JSON 行追加到 `$VORTEX_HOME/logs/vortex.log`，不向 TUI
屏幕打印日志。新建日志文件在 Unix 上使用 0600 权限，已有文件权限不会因此自动收紧。
目前没有自动轮转；日志不是会话 JSONL，不提供恢复功能。

默认日志级别为 info，未传 `-v` 时使用 RUST_LOG；单个 `-v` 会强制 debug，覆盖该环境
变量。当前 CLI 的 verbose 是 bool，不支持目标设计中的多次递增；没有 quiet 参数。

```bash
RUST_LOG=info,vortex_core=debug cargo run -p vortex-cli --locked
cargo run -p vortex-cli --locked -- -v
```

注意：`app.rs` 仍有记录完整输入的 TRACE。全局 trace 或 vortex_tui=trace 会将 Prompt
落盘，不要用于含敏感内容的对话；前面的脱敏验收项尚不能据此宣称已通过。
`-c/--config` 虽可解析，但目前未应用到运行配置，不能用它切换模型或日志路径。

## 文档导航

各类文档的用途见 [`../README.md`](../README.md)。

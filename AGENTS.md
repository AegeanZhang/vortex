# Repository Guidelines

## 项目状态与结构

Vortex 正处于 Rust CLI Agent 的设计阶段，当前仓库仅维护 `README.md`、本指南和忽略规则，尚无可执行代码。初始化实现时采用 Cargo workspace：CLI/TUI、Agent 核心、Provider 适配器以及工具与沙箱能力应拆分为职责清晰的 crate；共享接口不得依赖具体终端 UI 或模型服务。项目二进制名称统一为 `vortex`。

## 构建、测试与开发命令

创建 `Cargo.toml` 后，提交前统一运行：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release
```

以上命令依次检查格式、静态问题、完整测试和发布构建。Cargo workspace 建立前不要声称这些命令已经可用，也不要提交占位实现来伪造通过结果。

## 编码风格与命名约定

使用稳定版 Rust 和 2024 edition，接受 `rustfmt` 默认格式。模块、Rust 文件、函数和变量使用 `snake_case`，类型与 trait 使用 `PascalCase`，常量使用 `SCREAMING_SNAKE_CASE`；Cargo package 和非 Rust 文件使用英文 `kebab-case`。错误应保留上下文，库层返回结构化错误，CLI 层负责面向用户的中文提示。异步代码统一基于 Tokio，避免阻塞 executor。

## 架构与安全约束

Provider、Tool、会话存储和 UI 通过内部事件模型解耦。工具参数使用 JSON Schema 描述；文件修改、Shell、MCP 工具均经过同一权限策略。Linux 默认以当前工作区为可写边界，并通过 `bubblewrap` 禁止网络；不得绕过审批执行越界操作。API 密钥只能从环境变量读取，日志和错误信息必须脱敏。项目指令按目录层级读取 `AGENTS.md`，越接近当前目录的规则优先。

## 测试指南

单元测试与实现放在同一模块，跨 crate 行为置于 `tests/`。Provider 使用固定流事件或本地 mock server 测试，禁止依赖真实付费 API；文件和 Shell 工具必须覆盖路径越界、取消、超时、非零退出及输出截断。涉及 TUI 时测试状态机，并人工验证终端退出后恢复原始模式。

## Commit 与 Pull Request

Commit 使用简短、动词开头的中文主题，每个提交只处理一个目的。Pull Request 需说明目标、关键设计、风险、测试命令和结果，并关联相关 issue。终端 UI 变化附截图或录屏；权限、配置或 Provider 行为变化必须给出兼容性与安全影响。

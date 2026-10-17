# 仓库与模块组织

## 目标结构

```text
vortex/
├── Cargo.toml                  # 产品 Workspace 与统一依赖版本
├── Cargo.lock                  # 产品依赖锁文件
├── rust-toolchain.toml         # 稳定版工具链（引入后由本地与 CI 共用）
├── AGENTS.md                   # Agent 修改边界与规范入口
├── docs/
│   ├── README.md               # 文档总索引
│   ├── architecture/           # 系统结构、技术选型与术语
│   ├── design/                 # 子系统详细设计
│   ├── engineering/            # 编码、测试、Git 与 Review 规范
│   ├── research/               # 技术调研和外部对照
│   └── guide/                  # 用户手册
└── crates/
    ├── vortex-core/
    ├── vortex-provider/
    ├── vortex-tools/
    ├── vortex-sandbox/
    ├── vortex-tui/
    └── vortex-cli/              # 产出 vortex 二进制
```

`vortex-core` 的目标模块包括事件模型、Agent loop、消息历史、会话、配置、项目指令、Provider/Tool trait、审批策略与错误类型。其他 crate 的内部模块应在真实职责出现后再拆分，不一次性创建空目录和占位实现。

## 执行链路的模块入口

| crate | 阅读入口 |
| --- | --- |
| vortex-core | `config.rs` 配置合并/校验、`model.rs` 模型目录/绑定、`conversation.rs` 历史、`provider.rs`/`tool.rs` 接口、`agent_loop.rs` 多步执行、`session.rs` actor |
| vortex-provider | `openai_chat/mod.rs` 装配、`request.rs` 请求映射、`decoder.rs` 增量组装、`stream.rs` 流连接；`sse_transport.rs` 负责传输 |
| vortex-tools | `metaso_web_search.rs` 及子模块负责秘塔 HTTP 与请求/结果映射；`mock_web_search.rs` 保留固定结果学习用途 |
| vortex-cli | `main.rs` 参数/模式、`paths.rs` 目录、`logging.rs` 日志、`bootstrap.rs` 加载/工具装配、`model_factory.rs` 模型装配；`exec.rs` 仍为旧单次路径 |
| vortex-tui | `event_loop.rs` 与 `app.rs` 消费 CoreEvent，`command.rs` 解析 slash，`widgets/model_picker.rs` 等控件渲染局部状态 |

历史、执行与 wire 请求之间的边界见 [Session](../design/session-runtime.md) 和
[Agent loop](../design/agent-loop.md)；配置与模型切换的接入及测试边界见根 README，权限等
后续目标不能仅凭目录存在视为已实现。

## Workspace 约定

- 仓库只维护一个产品 Workspace，使用 `members = ["crates/*"]` 和 `default-members = ["crates/*"]`；根目录的 `--workspace` 覆盖全部产品 crate。
- 所有成员统一维护根目录 `Cargo.lock` 和默认 `target/`；锁文件纳入 Git，构建目录不纳入。成员 crate 不单独维护锁文件。
- 产品 crate 目录和 Cargo package 统一使用 `vortex-*`；Rust 路径对应为 `vortex_core::` 等。
- 只有 `vortex-cli` 产出名为 `vortex` 的二进制。
- 多个 crate 共用的依赖版本集中在根 `[workspace.dependencies]`，成员通过 `dep.workspace = true` 继承。
- 编程语言练习在独立项目中进行；产品 crate 不得依赖仓库外的个人学习项目。

提交依赖变动时，将成员 manifest 与根 Workspace 的锁文件一起提交；不要只提交一方。

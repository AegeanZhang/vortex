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
├── crates/
│   ├── vortex-core/
│   ├── vortex-provider/
│   ├── vortex-tools/
│   ├── vortex-sandbox/
│   ├── vortex-tui/
│   └── vortex-cli/             # 产出 vortex 二进制
└── playground/                 # 独立实验 Workspace，仍由同一 Git 仓库管理
    ├── Cargo.toml              # 实验成员与实验共享依赖
    ├── Cargo.lock              # 实验依赖锁文件
    ├── hello-rust/
    └── tui-demo/
```

`vortex-core` 的目标模块包括事件模型、Agent loop、消息历史、会话、配置、项目指令、Provider/Tool trait、审批策略与错误类型。其他 crate 的内部模块应在真实职责出现后再拆分，不一次性创建空目录和占位实现。

## 执行链路的模块入口

| crate | 阅读入口 |
| --- | --- |
| vortex-core | `conversation.rs` 语义历史、`provider.rs` 模型接口、`tool.rs` 工具接口与注册表、`agent_loop.rs` 多步执行、`session.rs` actor |
| vortex-provider | `openai_chat/mod.rs` 装配、`request.rs` 请求映射、`decoder.rs` 增量组装、`stream.rs` 流连接；`sse_transport.rs` 负责传输 |
| vortex-tools | `metaso_web_search.rs` 及子模块负责秘塔 HTTP 与请求/结果映射；`mock_web_search.rs` 保留固定结果学习用途 |
| vortex-cli | `main.rs` Provider/Tool 装配、`logging.rs` 日志；`exec.rs` 是待迁移的旧单次路径 |
| vortex-tui | `event_loop.rs` 与 `app.rs` 消费 CoreEvent，`widgets/` 渲染局部状态 |

历史、执行与 wire 请求之间的边界见 [Session](../design/session-runtime.md) 和
[Agent loop](../design/agent-loop.md)；目录入口不表示配置、权限等目标模块已经实现。

## Workspace 约定

- 根 Workspace 使用 `members = ["crates/*"]`、`default-members = ["crates/*"]` 和 `exclude = ["playground"]`；根目录的 `--workspace` 只覆盖产品 crate。
- `playground/Cargo.toml` 独立管理实验成员。运行实验时进入该目录，或显式指定 `--manifest-path playground/Cargo.toml`；根目录不能再直接使用 `cargo run -p tui-demo`。
- 两个 Workspace 分别维护 `Cargo.lock` 和默认 `target/`，两个锁文件都纳入 Git；各实验 crate 不单独维护锁文件。实验修改依赖不会改动产品锁文件。
- 产品 crate 目录和 Cargo package 统一使用 `vortex-*`；Rust 路径对应为 `vortex_core::` 等。
- 只有 `vortex-cli` 产出名为 `vortex` 的二进制。
- 多个 crate 共用的依赖版本集中在所属 Workspace 的 `[workspace.dependencies]`，成员通过 `dep.workspace = true` 继承；实验不再继承产品 Workspace 的依赖声明。
- `playground/` 中的 crate 可以验证技术方案，但产品 crate 不得依赖它们，其接口也不作兼容承诺。

拆分的目的是隔离产品和实验的依赖解析与检查范围，而不是停止保存实验的 Git 历史。
提交依赖变动时，将成员 manifest 与所属 Workspace 的锁文件一起提交；不要只提交一方。

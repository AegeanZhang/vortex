# 仓库与模块组织

## 目标结构

```text
vortex/
├── Cargo.toml                  # Workspace 与统一依赖版本
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
└── playground/                 # 学习与验证实验，不属于产品代码
```

`vortex-core` 的目标模块包括事件模型、Agent loop、消息历史、会话、配置、项目指令、Provider/Tool trait、审批策略与错误类型。其他 crate 的内部模块应在真实职责出现后再拆分，不一次性创建空目录和占位实现。

## Workspace 约定

- `default-members = ["crates/*"]`，使默认构建不包含 `playground/`；完整检查仍显式使用 `--workspace`。
- 产品 crate 目录和 Cargo package 统一使用 `vortex-*`；Rust 路径对应为 `vortex_core::` 等。
- 只有 `vortex-cli` 产出名为 `vortex` 的二进制。
- 多个 crate 共用的依赖版本集中在根 `[workspace.dependencies]`，成员通过 `dep.workspace = true` 继承。
- `playground/` 中的 crate 可以验证技术方案，但产品 crate 不得依赖它们，其接口也不作兼容承诺。

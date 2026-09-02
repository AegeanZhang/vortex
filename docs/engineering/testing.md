# 测试规范

## 提交前检查

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release
```

以上命令依次检查格式、静态问题、完整 Workspace 测试和发布构建。不得在未实际运行时声称检查通过。

## 测试组织

- 单元测试与实现放在同一模块，跨 crate 行为放在各 crate 的 `tests/`。
- Provider 使用固定流事件或本地 mock server，禁止让自动化测试调用真实付费 API。
- 文件与 Shell 工具必须覆盖路径越界、取消、超时、非零退出和输出截断。
- TUI 需要测试状态机，并人工验证正常退出、错误和 panic 后终端恢复原始模式。
- `playground/` 也包含在 `--workspace` 检查中，但产品测试不得依赖实验 crate。

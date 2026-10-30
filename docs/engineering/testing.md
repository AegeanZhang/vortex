# 测试规范

## 提交前检查

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo build --release --locked
```

以上命令在仓库根目录运行，依次检查格式、静态问题、产品 Workspace 测试和发布构建。本仓库不包含独立实验 Workspace。不得在未实际运行时声称检查通过。

## 测试组织

- 单元测试与实现放在同一模块，跨 crate 行为放在各 crate 的 `tests/`。
- Provider 使用固定流事件或本地 mock server，禁止让自动化测试调用真实付费 API。
- 工具版使用 FakeProvider 与 FakeTool 检查批次声明/结果、顺序执行、调用 ID、每工具额度和
  错误结果；MockWebSearch 仍配合真实模型运行，不能替代受控 FakeProvider 测试。
- 测试 helper 必须与 run_turn 并行消费进度；默认 50 次调用可超过有界队列容量，不能等任务
  返回后才排空，也不能仅靠扩大队列掩盖背压。timeout 用于防挂死，不代替事件同步。
- 工具任务异常要验证成功结果保留、已开始但未知/未开始结果补齐，以及下一轮请求历史完整；
  panic 夹具必须断言失败终态，不能以程序未退出判定成功。完整 Session 关闭另验取消与背压。
- 文件与 Shell 工具必须覆盖路径越界、取消、超时、非零退出和输出截断。
- TUI 需要测试状态机，并人工验证正常退出、错误和 panic 后终端恢复原始模式。
- 编程语言练习在独立项目维护，产品测试不得依赖个人学习项目。

Session、工具和流式协议的场景矩阵分别见 [Session](../design/runtime/session-runtime.md)、
[Agent loop](../design/runtime/agent-loop.md) 和 [SSE](../design/provider/streaming-response.md)。矩阵是
测试要求，不代表覆盖已完成；实际测试数量和检查结果只在根 README 记录。

受控 FakeProvider/Stream、背压同步、本地 HTTP server 与秘塔异常场景的具体方法见
[回归测试方案](regression-test-plan.md)。协议 fixture 放在对应 crate 的 `tests/fixtures/`，
人工/行为评估案例放在根 `datasets/`；固定脱敏数据可纳入 Git，真实凭据与未获授权的
用户请求/响应不得入库。行为案例不能替代确定性单元测试。

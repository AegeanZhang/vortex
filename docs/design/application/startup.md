# 应用启动与依赖装配设计

核对日期：2026-10-27。状态：当前 TUI 实现契约。架构归属：应用入口与装配。
源码入口：[main.rs](../../../crates/vortex-cli/src/main.rs)、[bootstrap.rs](../../../crates/vortex-cli/src/bootstrap.rs)。

## 输入、产物与边界

main 读取 CLI，解析 AppPaths 并初始化日志；bootstrap 接收路径、-c 覆盖与 -m 模型选择，
返回 SessionConnection，不将配置或密钥交给 TUI。路径规则归 [安装设计](installation.md)，
字段与合并归 [配置设计](../configuration/configuration.md)，当前/目标参数归 [CLI](cli-surface.md)。

## 装配顺序

1. clap 处理参数、help/version；随后解析 HOME/cwd 和日志路径并初始化一次 subscriber。
2. TUI 分支在 spawn_blocking 中 load_config，打印安全的来源诊断；不自动读取 .env。
3. 在阻塞池打开启动 cwd 为 WorkspaceFs，先注册目录工具，再按配置选择秘塔或 Mock。
   仅秘塔读取搜索 key，关闭搜索仍保留目录工具，注册表保持固定且顺序稳定。
4. 从实际 specs 计算 requires_tools，构造关闭重定向/自动重试的模型 client 与工厂。
5. ModelManager 校验初始候选，工厂先校验选项再按需读所选模型 key，本地构造 Provider。
6. 输出安全模型信息，start_session 启动 actor 后返回连接；TUI 才初始化终端并运行事件循环。

模型/工具的构造不是实际模型请求；只有流被 poll 才开始调用模型。整个装配并非数据库事务：
失败前已经创建的日志仍可能保留，不承诺自动删除或回滚用户目录。

## 所有权与失败处理

main 持有 LoggingGuard 到结束；工厂保存配置/client/KeyReader，Session 独占 ModelManager，
具体工具持有句柄/client/凭据。TUI 仅消费连接；详见 [模型管理](../runtime/model-management.md)、
[工具注册](../tools/tool-registry.md)、[Session](../runtime/session-runtime.md)。

路径、日志、TOML、工作区、凭据、工具重名或初始模型构造失败都沿 Result 向 main 返回，
不进入 TUI、不回退到 Mock/其他模型；后台 actor 在 start_session 后已经启动，若终端初始化
失败，连接释放通过关闭通道通知其收尾。不能把 send 成功当作关闭完成确认。

## 日志与旧 exec 分支

文件日志位于 HOME/logs/vortex.log，不向 TUI 屏幕写 tracing 日志；具体过滤优先级和
Unix 新建权限见 [日志指南](../../guide/getting-started.md#日志与本地配置)，凭据见
[安全契约](../cross-cutting/credentials.md)。来源诊断不打印配置正文或密钥。

CLI、Provider 和 Tools 调用共享 [Diagnostics](../cross-cutting/diagnostics.md)，复用已有
subscriber/文件 writer，不再初始化日志。普通日志保持无正文；Debug wire TRACE 显式开启后
记录三个 HTTP 路径的正文副本，不向 TUI 打印，也不新增 HOME 或配置读取。
过滤仍由 logging.rs 决定：默认读取 RUST_LOG，当前单个 -v 强制 debug 并覆盖它。
main 持有 guard 帮助队列收尾，但不保证等待独立错误预览任务完成；队列默认允许丢弃记录。

旧 exec 在路径/日志初始化后走独立非流式 HTTP，不加载上述配置或创建 Session；
传入 -m/-c 明确报错。它已接入 wire 诊断，仍未迁移到配置、工具或流式 Session。
为记录正文改用 bytes 后 JSON 解析，坏 JSON 的底层错误源变更见诊断设计。
后续迁移应复用 Session，不能把本节顺序写成已经接入 exec。

## 验收要求

使用临时目录、注入 KeyReader 和离线工厂验证工具顺序、凭据按需读取、实际工具能力与
模型可用性一致，失败不静默降级。help/version 不创建目录，坏配置/初始 key 在进入终端前失败。
不改变并行测试进程的 cwd 或环境；日志 subscriber 的全局初始化采用隔离进程。
完整矩阵见 [回归测试方案](../../engineering/regression-test-plan.md)，结果只在根 README 记账。

# 模型目录、装配与切换设计

核对日期：2026-10-26。状态：当前实现契约。架构归属：模型管理。
源码入口：[model.rs](../../../crates/vortex-core/src/model.rs)、[model_factory.rs](../../../crates/vortex-cli/src/model_factory.rs)。实际检查结果见根 [README](../../../README.md)，验收要求不代表已经通过。

本文件维护运行时模型目录与绑定；配置字段归 [配置设计](../configuration/configuration.md)，
厂商选项归 [思考选项](../provider/thinking-options.md)，界面交互归 [TUI](../tui/interface.md)。

## 类型与所有权

| 类型 | 职责 |
| --- | --- |
| ModelInfo | 安全展示的 ID、显示名、Provider、host、tools 能力及不可用原因，不含密钥 |
| ModelFactory | catalog 返回目录，create 按 ID 本地构造 `Arc<dyn ModelProvider>`，不发网络请求 |
| ModelManager | 保存工厂、目录、当前元数据与 Provider，启动后由 Session 独占 |
| ModelSelectionError | 表达目标 ID 与安全错误，不携带密钥或原始配置 |

ModelManager::new 检查目录非空、ID 唯一及初始候选可选，再请求工厂构造。
每 Turn 通过 provider() 克隆固定 Arc；不把 manager 本体传入执行任务。

## 工厂装配与失败边界

ConfiguredModelFactory 保存加载后的 AppConfig、共享 Reqwest client、KeyReader 与工具能力要求。
构造目录不读取所有模型 key，不做在线模型发现；非法闲置思考 profile 标为不可用，
不能因此阻止其他合法候选。模型/Provider 的结构性非法仍由配置加载器拒绝。

catalog 与 create 使用同一 chat_options 校验；create 依次检查目标、实际工具要求、连接引用、
方言/策略/预设/历史契约，然后读取选中 key 并构造 Provider。装配不是远端鉴权成功。
未知预设、方言不匹配、能力或历史契约不支持在读 key 前失败；不自动替换模型或降级策略。

## 模型切换

`SelectModel { model_id }` 与 Prompt 共用业务命令队列。actor 在 Running 或仍有 ActiveTurn
时发布 `ModelSelectionRejected { model_id, message }`，不排队等待 Turn 完成后自动切换。
TUI 的本地检查不能代替这里的校验，因为界面状态可能滞后。

非 Running 且无活跃 Turn 时调用 `models.select(id)`：检查目录/能力，调用 Factory 本地
构造 Provider，成功后一起替换当前 ModelInfo 和 Provider，发布 `ModelSelected { model }`。
不存在、能力不符或缺密钥等失败发布专用拒绝事件，保留原绑定。同 ID 幂等成功，不重新
构造实例，但仍发布确认；不要复用普通 CommandRejected，否则 UI 无法可靠结束 pending。

选择不产生 User/Assistant、不清空历史、不修改 SessionStatus、不发 TurnCompleted，也不
写配置。搜索工具注册表仍使用启动配置。每次 SubmitPrompt 获取当时的 Provider，整个
Turn 的模型/工具往返固定使用该 Arc，不将可变 ModelManager 传进 run_turn。

切换 Provider 后，下一轮会把保留的历史发送给新服务；成功确认仅表示本地装配成功，
不代表远端鉴权或模型可用性已验证。网络错误仍由下一次 Turn 的正常失败链处理。
选择事件同样经过可被 Shutdown 中断的 publish_event，不新增第二套事件通道或关闭机制。

## 凭据与配置边界

只读取初始或显式选择模型的 api_key_env；未选模型缺 key 不阻止启动。
同 ID 选择幂等，不重新构造或重读 key；用户在另一个 shell 改变量不会刷新当前进程。
失败保留原元数据与 Provider。普通配置解析不读模型 key，产品不主动加载 .env。
密钥格式、脱敏及认证边界见 [凭据设计](../cross-cutting/credentials.md)。

## 验收要求与后续

覆盖空/重复目录、未知与不可选目标、工具能力不匹配、非法思考选项先于读 key、
只读所选 key、同 Provider 模型不串用选项、同 ID 幂等及切换失败保留实例。
Session 测试确认 Running 拒绝、专用结果事件和下次请求使用新实例；TUI 测试另验 pending/草稿。
使用注入工厂和 KeyReader，不修改全局环境或探测真实模型。矩阵见
[回归测试方案](../../engineering/regression-test-plan.md)。

当前不支持热重载、自动探测、订阅登录、延迟切换或保存默认值；这些须另立设计。

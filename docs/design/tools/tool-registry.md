# 工具接口与注册表设计

核对日期：2026-10-30。状态：当前实现契约。架构归属：工具执行与资源访问。
源码入口：[tool.rs](../../../crates/vortex-core/src/tool.rs)。实际检查结果见根 [README](../../../README.md)，验收要求不代表已经通过。

本文件维护 Tool、ToolRegistry 与归一化结果的契约；具体参数与资源边界分别见
[目录列表](list-directory.md) 和 [搜索](web-search.md)。执行次数、调用 ID 和继续请求由
[Agent loop](../runtime/agent-loop.md) 决定，不由工具自行调度模型。

## 公共接口与错误结果

Core 定义以下协议无关类型：

```rust
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub struct ToolCall {
    pub call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub struct ToolResult {
    pub call_id: String,
    pub output: String,
    pub is_error: bool,
}

pub type ToolFuture<'a> =
    Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + 'a>>;

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn execute(&self, arguments: serde_json::Value) -> ToolFuture<'_>;
}
```

`ToolRegistry` 持有 `Vec<Arc<dyn Tool>>`：`empty()` 供无工具路径使用；`new()` 拒绝重名
工具；`specs()` 按注册顺序返回定义；`contains(name)` 精确查询是否已注册；
`execute(&ToolCall)` 按名字查找并执行。
未知工具、参数错误和执行错误统一转换为保留原 `call_id` 的错误 ToolResult，不直接结束
Turn。工具只处理参数，不负责生成调用 ID；注册表不自动执行 Schema 校验。

## 装配与生命周期

CLI 先注册 list_directory，再按搜索配置选装一种 web_search；整个 Session 的注册表固定，
模型切换不重新装配工具。关闭搜索不关闭目录工具，实际非空注册表决定模型 tools 能力要求。
ToolRegistry 可克隆，内部 Arc 共享工具实例及其资源；不是复制独立的工作区或并发额度。

多调用调度与计数由 Turn 持有：先用 contains 聚合已注册工具的批次需求，整批预检，
再逐个扣额并执行。ToolCallLimits 保存规则，TurnToolBudget 的计数每 Turn 新建；
Tool 实例和注册表不保存跨 Turn 的使用计数。错误尝试不退额，未知工具不扣实际执行额度。
通用调用额度不进入 ToolSpec/JSON Schema，也不新增联网/收费类型或并发安全标记。

当前无并行调用、通用 Schema 校验、审批、沙箱、统一工具超时或总 Token 预算。
具体工具各自管理参数、输出、超时或阻塞额度，不据搜索工具的超时推断所有工具均可取消。
后续 MCP 接入也须转换为此契约，当前没有 MCP client。

## 验收要求

验证 empty/new、稳定 specs 顺序、重名拒绝、contains 查询，以及未知工具及参数/执行错误保留 call_id；
bootstrap 验证目录优先、搜索关闭与 Mock 不读搜索 key、秘塔缺 key 不回退。
模型能力必须来自实际注册表，目录与 create 校验一致，并在读取模型 key 前拒绝不兼容模型。
具体测试方法见 [回归测试方案](../../engineering/regression-test-plan.md)。

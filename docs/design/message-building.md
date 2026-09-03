# Message 与上下文构建设计

本文档定义 Vortex 在调用模型前如何从会话事件构建消息与上下文，以及 Provider 如何把内部模型转换为具体 API 请求。架构边界见 [`../architecture/overview.md`](../architecture/overview.md)，术语见 [`../architecture/glossary.md`](../architecture/glossary.md)。

## 目标与边界

消息构建必须满足以下目标：

- 核心消息模型不依赖 OpenAI、Anthropic 等具体协议。
- 指令、用户消息、模型回复和工具结果保持类型边界，不靠字符串拼接区分来源。
- 相同输入产生确定的消息顺序，便于恢复、测试和审计。
- 超出模型上下文窗口时按明确策略裁剪，不静默丢失当前任务或拆散工具调用。
- Provider adapter 只负责协议转换，不决定保留哪些历史或如何执行工具。

首版只要求文本内容；图片、文件和音频以后通过扩展内容类型加入。本文档不定义 HTTP、流式传输、工具审批和会话文件格式的实现细节。

## 三层模型

| 层次 | 作用 | 持久化 | 是否含 Provider 细节 |
| --- | --- | --- | --- |
| `CoreEvent` | 表示文本增量、工具状态、错误等运行事实 | 写入 JSONL | 否 |
| `ConversationItem` | 从事件重建出的完整语义项，供上下文选择 | 可由事件重建 | 否 |
| Wire request | OpenAI Responses、Chat Completions 或 Anthropic 的请求体 | 默认不持久化 | 是 |

事件是会话事实的唯一来源。消息构建器读取事件投影后的会话状态，不直接读取 JSONL，也不把 Provider 请求体当作会话状态保存。

```text
CoreEvent / Session log
        │  投影
        v
ConversationState
        │  ContextAssembler
        v
ModelRequest
        │  Provider adapter
        v
Provider wire request
```

## 核心数据模型

以下类型表示语义要求，具体字段可在实现时按 Rust 所有权和序列化需要调整：

```rust
pub struct ModelRequest {
    pub instructions: Vec<Instruction>,
    pub items: Vec<ConversationItem>,
    pub tools: Vec<ToolSpec>,
    pub limits: GenerationLimits,
}

pub enum ConversationItem {
    UserMessage {
        id: ItemId,
        content: Vec<ContentPart>,
    },
    AssistantMessage {
        id: ItemId,
        content: Vec<ContentPart>,
    },
    ToolCall {
        id: ItemId,
        call_id: ToolCallId,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: ItemId,
        call_id: ToolCallId,
        output: String,
        is_error: bool,
    },
    Summary {
        id: ItemId,
        content: String,
        covered_until: ItemId,
    },
}

pub enum ContentPart {
    Text(String),
}
```

`ItemId`、`ToolCallId` 和会话/Turn 标识应使用不同的 newtype，避免把不同 ID 当作普通字符串混用。时间戳、流式 chunk 序号和耗时属于事件元数据，不应进入模型消息，除非它们本身对任务有语义价值。

隐藏推理或 chain-of-thought 不进入 `ConversationItem`、日志或下一轮上下文。只保留 Provider 明确允许展示和复用的最终文本、工具调用与摘要。

## Instruction 模型

Instruction 与普通会话项分开保存，并保留来源：

```rust
pub struct Instruction {
    pub source: InstructionSource,
    pub content: String,
}

pub enum InstructionSource {
    BuiltIn,
    User,
    Project { path: PathBuf },
    Directory { path: PathBuf },
}
```

指令加载器负责按系统规则解析层级和冲突；`ContextAssembler` 只接收已经排好顺序的结果，并保持来源信息。当前用户任务仍然是 `UserMessage`，不能为了提高权重而拼进 system/developer instruction。

稳定且重复使用的指令排在请求前部，动态历史和当前任务排在后部，以便支持 Provider 的前缀缓存。不得把 Tool 输出、仓库文件内容或其他不可信文本插入 Instruction。

## ContextAssembler 职责

`ContextAssembler` 位于 `vortex-core`，使用显式输入构造请求：

```rust
pub fn assemble(input: ContextInput) -> Result<ModelRequest, ContextError>;
```

构建流程固定为：

1. 接收投影后的 `ConversationState`、已解析指令、可用工具和模型限制。
2. 规范化当前 Turn，拒绝空的当前用户任务和非法内容。
3. 校验 Tool call/result 的 ID、顺序和一一对应关系。
4. 计算指令、工具定义、消息历史与预留输出的预算。
5. 按裁剪规则选择完整历史项，必要时使用已有摘要。
6. 生成 Provider 无关的 `ModelRequest` 并再次检查不变量。

消息构建应是确定性的纯逻辑：不读取文件、环境变量或网络，不调用模型生成摘要，也不修改会话。需要生成新摘要时，Agent loop 先完成摘要操作并写入事件，再重新构建上下文。

## Token 预算与裁剪

预算至少包含 Instruction、工具定义、历史消息、当前任务、协议开销和预留输出。Token 计算因模型而异，核心层通过抽象获得估算：

```rust
pub trait TokenEstimator {
    fn estimate(&self, request: &ModelRequest) -> Result<usize, TokenError>;
}
```

裁剪顺序如下：

1. 永远保留强制 Instruction、当前用户任务和当前未闭合 Turn 的必要项。
2. Tool call 与对应 Tool result 作为一个原子组保留或移除，不得只保留一半。
3. 优先保留最近的完整 Turn，从最早的已完成 Turn 开始移除。
4. 若存在覆盖被移除区间的 `Summary`，用摘要替代原始历史；不得同时加入摘要和它覆盖的全部原文。
5. 超长 Tool 输出优先按工具策略截断，并附加明确的截断标记和原始长度。
6. 强制内容本身已经超出预算时返回 `ContextError::BudgetExceeded`，不得静默截断当前任务。

首版可以使用保守的近似估算，但 `TokenEstimator` 接口必须可替换。模型切换后应使用新模型的上下文窗口和估算规则重新构建，不能复用旧预算结果。

## Tool call 不变量与安全边界

- `ToolCallId` 在一次会话中唯一；每个 `ToolResult` 必须匹配一个先前出现且尚未闭合的调用。
- 同一个 Tool call 最多接受一个终态结果；取消、超时和执行错误也表示为 `is_error = true` 的结果。
- 工具参数保持结构化 JSON，并在执行前按 Tool 的 JSON Schema 校验。
- Tool 输出属于不可信数据，始终保持为 `ToolResult`；其中即使包含“忽略之前指令”等文本，也不能改变 Instruction 层级。
- 日志、错误和调试快照不得包含 API 密钥或认证头；凭据规则见 [`credentials.md`](credentials.md)。
- 截断不能破坏 UTF-8，也不能让输出看起来像完整内容；必须附加机器可识别的截断元数据或明确文本标记。

## Provider 映射

Provider adapter 接收 `ModelRequest`，只处理协议差异：

| 内部语义 | OpenAI Responses | Chat Completions | Anthropic Messages |
| --- | --- | --- | --- |
| Instruction | `instructions` 或相应高优先级输入 | system/developer message | 顶层 `system` |
| User/Assistant message | input item | role message | message content block |
| Tool call | function call item | assistant tool call | `tool_use` block |
| Tool result | function call output | tool message | `tool_result` block |

Adapter 不得重新排序历史、擅自裁剪内容或把 Tool result 改写为普通用户文本。若目标协议无法无损表达某个内部类型，应返回明确的 capability error，而不是降级为含义不同的字符串。

## 会话恢复与摘要

恢复会话时先按事件顺序重建 `ConversationState`，再调用相同的 `ContextAssembler`。不得维护一条只供恢复流程使用的消息构建路径。

流式 Assistant 文本在事件层可以由多个 chunk 组成；只有完成或中断后才投影为一个带终态的语义消息。中断消息是否进入下一轮上下文必须有明确状态，不能把不完整输出伪装成正常完成。

`Summary.covered_until` 标识摘要覆盖边界。生成新摘要时应记录使用的源区间和模型信息，保证恢复时能够检测重复覆盖或摘要缺口。

## 错误模型

消息构建至少区分以下错误：

- 无当前用户任务或内容为空；
- Tool result 无匹配调用、重复结果或顺序非法；
- Instruction 层级解析失败；
- 强制内容超过上下文预算；
- Token 估算失败；
- Provider 不支持请求中的内容或工具能力。

`vortex-core` 返回包含上下文的结构化错误，Provider adapter 补充协议和模型信息，`vortex-cli` 最终转换为中文提示与退出码。

## 测试要求

- 表驱动测试覆盖 User/Assistant 消息顺序、空任务和多 Turn 历史。
- Tool call/result 覆盖正常、错误、取消、重复 ID、缺失结果和孤立结果。
- 预算测试覆盖刚好等于上限、强制内容超限、最近 Turn 保留、摘要替换和 Tool 输出截断。
- 同一输入重复构建必须得到相同结果；测试不得依赖 HashMap 的随机迭代顺序。
- Provider adapter 使用固定 fixture 或 snapshot 验证三种协议映射，不调用真实付费 API。
- 注入测试应确认恶意 Tool 输出仍处于 `ToolResult`，不会出现在 Instruction 字段。
- 脱敏测试确保结构化错误、日志和 snapshot 不包含密钥原文。

## 分阶段实现

1. 文本版：只实现 User/Assistant 文本、确定性构建和 mock Provider。
2. 工具版：加入 Tool call/result、ID 校验和结构化参数。
3. 指令版：接入分层 `AGENTS.md`，保留来源与顺序。
4. 预算版：加入 `TokenEstimator`、完整 Turn 裁剪和摘要边界。
5. 多 Provider：分别实现协议 adapter 与固定请求 fixture。

每一阶段都应先完成核心构建测试，再接入 CLI 或 TUI；UI 不参与消息选择和上下文裁剪。

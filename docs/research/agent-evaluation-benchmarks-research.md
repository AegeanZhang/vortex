# CLI Agent 评测基准与 Vortex 接入调研

- 调研日期：2026-10-19；合并 SWE-bench 系列介绍与行业采用情况两轮调研。
- 证据范围：基准维护者的论文、数据卡、评测代码，以及 OpenAI、Anthropic 的公开材料。
- 定位：评测候选与方法建议，不是已采纳的架构决策，也不是 Vortex 跑分记录。
- 本轮未下载完整任务集、安装评测器、运行容器或调用付费模型/搜索 API。

## 1. 结论与上一轮建议的修正

SWE-bench 是行业广泛采用的软件工程评测体系，SWE-bench Pro 是 Scale AI 发布的独立扩展，
不是原项目的收费版。它们可以作为 Vortex 的外部参照，但不能代替产品回归测试，更不能
仅凭榜单成绩判断 CLI 的可用性、安全性或 TUI 体验。

需要修正上一轮对 Verified 的推荐：OpenAI 在 **2026-02-23** 表示停止报告 SWE-bench
Verified 成绩，指出测试与需求不匹配、训练数据污染等问题，并推荐暂时报告 Pro 成绩。
这是 OpenAI 的研究结论和报告策略，**不等于 SWE-bench 项目取消 Verified，也不等于所有
厂商达成相同结论**。Verified/Lite 仍可用于学习、历史对照和评测链路调试，不建议作为
Vortex 唯一的长期主基准。[OpenAI 原文](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/)。

建议形成互补的四类评测：

1. 确定性工程回归：Provider/SSE、工具循环、状态机、安全边界与终端恢复。
2. 真实模型行为评测：工具选择、参数、错误恢复、搜索证据与多轮稳定性。
3. 编码端到端：自建小型 Rust 任务起步，再接固定的 Pro V2、Multilingual 子集。
4. 终端任务与安全：有 Shell/隔离后接 Terminal-Bench；持续补提示注入与越权用例。

以下优先级是基于 Vortex 当前能力的建议，不是基准维护者对本项目的背书。

## 2. 先区分评测对象

| 概念 | 含义 | Vortex 中的对应物 |
| --- | --- | --- |
| 模型 | 接收上下文并生成内容或工具调用 | 所选模型及 Provider 接口 |
| Agent harness / scaffold | 提示、工具循环、上下文、预算与执行策略 | Session、Agent loop、工具和 CLI 装配 |
| 数据集 | 任务、仓库版本、环境定义及判分依据 | 外部任务清单或自建行为案例 |
| evaluation harness | 准备环境、运行待测 Agent、收集补丁、执行验证与汇总 | 尚未实现的评测运行器及外部官方评测器 |

SWE-bench 传统 harness 主要评测 Agent **已经生成的补丁**；给它一个模型 key，不代表
自动测到了 Vortex。Pro V2/Harbor 提供运行 Agent 的适配入口，也仍需接入真正的 Vortex
执行链，不能用另一个 Agent 生成补丁后把成绩归给 Vortex。
[SWE-bench 评测指南](https://www.swebench.com/SWE-bench/guides/evaluation/)、
[Pro V2 执行说明](https://github.com/scaleapi/SWE-bench_Pro-os/blob/main/v2/README.md)。

Agentic 成绩实际取决于模型、Agent、提示、工具、上下文、预算、环境和判分器的组合。
OpenAI 的公开实例也区分工具直接测试、独立模型循环和 Codex 产品 harness，说明不能
假定三者的行为一致。[Harness-aware evaluation](https://developers.openai.com/cookbook/examples/partners/harness_aware_plugin_evals/harness_aware_plugin_evals)。

## 3. SWE-bench、Pro 与 Pro V2

### 3.1 数据集关系

| 名称 | 本轮核对的规模/范围 | 主要定位 |
| --- | --- | --- |
| SWE-bench 原版 | 测试集 2,294 题、12 个 Python 仓库 | 从真实 GitHub issue 与对应修复构造仓库级任务 |
| SWE-bench Lite | 300 道测试任务 | 更小、更偏自包含修复，适合低成本调试 |
| SWE-bench Verified | 原版中人工筛选的 500 题 | 改善可解性和描述/测试质量；仍有残留问题与污染风险 |
| SWE-bench Pro v1 | 公开集 731 题 | 更长流程、多文件的软件工程任务；不是 Pro 全部私有/其他划分 |
| SWE-bench Pro V2 | 公开集 642 题、11 个仓库 | 修订任务、环境与验证，配套锁定协议和 Harbor 任务 |

原版/Lite 的范围见 [Lite 官方介绍](https://www.swebench.com/lite.html)，
Verified 见 [官方页面](https://www.swebench.com/verified.html)。Pro 的论文整体包含公开以外的
划分，不应把论文总数与可下载公开集混为一谈；公开版本以
[ScaleAI 数据卡](https://huggingface.co/datasets/ScaleAI/SWE-bench_Pro/blob/main/README.md) 为准。

当前 Pro V2 公开集覆盖 Go、Python、JavaScript、TypeScript，没有 Rust。
这不妨碍 Rust 实现的 Agent 参加；若关注 Rust 修复能力，需另选 Rust 任务。
Pro 主要测已有仓库的指定改动，不直接代表从零开发、代码 Review、交互体验或网页搜索质量。

### 3.2 基本判分

```text
固定仓库 base commit + 问题描述
  → Agent 探索、修改、测试
  → 捕获最终补丁
  → 干净环境应用补丁与评测测试
  → 执行测试、记录成功/失败/环境错误
```

- FAIL_TO_PASS：修复前失败、修复后应通过，验证目标问题。
- PASS_TO_PASS：修复前后都应通过，检查指定回归项。
- 通常不要求与参考补丁逐字一致；通过测试也不等于穷尽所有正确性与安全要求。
- 隐藏测试和 gold patch 由评测端持有，不能作为 Agent 输入。

任务与测试的关系见 [SWE-bench Multilingual 官方说明](https://www.swebench.com/multilingual.html)。
SWE-bench 预测文件包含 instance_id、模型标识和 model_patch；原版与 Pro v1/V2 的运行器、
字段和镜像不能因名称相似而混用。原版缓存还与 run_id 相关，换补丁重评应使用新的 run_id。
[官方评测格式与缓存说明](https://www.swebench.com/SWE-bench/guides/evaluation/)。

### 3.3 Pro V2 的关键变化

V2 于 2026-09-22 发布：从 v1 移除 89 题，重写部分描述、修订测试与环境；清理 Git 中
可能泄漏修复答案的历史。任务以 Harbor 格式提供，并有 HARD-51 子集，不建议用高难子集
作为 Vortex 最早的调试集。
[V2 数据卡与变更记录](https://huggingface.co/datasets/ScaleAI/SWE-bench_Pro/blob/main/README.md)。

标准锁定协议的重点是：

- Agent 阶段禁止普通外网访问，仅放行模型端点，并关闭 WebSearch/WebFetch。
- 捕获 Agent 的 diff，在新的沙箱重新应用补丁并运行未被 Agent 改写的 verifier。
- 设置/验证阶段有不同的网络需要，不能简单理解为整个流程彻底离线。
- 每任务有时间预算；本轮官方说明为 50 分钟。若本地实验改变预算，应标明不可直接对榜。

官方给出参考补丁全部通过、空补丁全部失败的发布检查。它们是维护者对其发布环境的报告，
不是本项目的复现记录。[V2 官方协议](https://github.com/scaleapi/SWE-bench_Pro-os/blob/main/v2/README.md)。

特别注意：`load_dataset("ScaleAI/SWE-bench_Pro", split="test")` 当前默认指向 V2。
复现旧结果须显式选 v1，并固定 revision；新实验也应记录配置、revision、镜像 digest
和任务 ID，不能只记“SWE-bench Pro”。

## 4. 是否权威？头部工具与模型是否采用？

### 4.1 如何理解“权威”

这里更准确的表述是“有较高行业认可度、公开方法与可执行环境”，不是认证标准。
原版 SWE-bench 有 [ICLR 2024 论文](https://proceedings.iclr.cc/paper_files/paper/2024/file/edac78c3e300629acfe6cbe9ca88fb84-Paper-Conference.pdf)
与公开工具；下表提供厂商采用和实际 CLI 评测的证据。但采用广泛不代表测试永远正确，
新版本更严格也不自动证明完全无污染或与用户真实任务完全一致。

### 4.2 已核实的公开证据

| 来源与时间/版本 | 可以确认什么 | 不能据此推断什么 |
| --- | --- | --- |
| OpenAI GPT-4.1 官方提示指南，2025 年材料 | 使用 SWE-bench Verified 测其 agentic harness，并公开提示/工具使用方法 | 不是 Codex CLI 当前版本完整产品成绩；也不证明 2026 年仍以 Verified 为主 |
| OpenAI，2026-02-23 | 停止报告 Verified，建议暂时报告 Pro | 当时不是后来 9 月发布的 Pro V2；不能说所有厂商都停用了 Verified |
| Anthropic Opus 4.6，2026-02-05 | 公布 Verified、Terminal-Bench 2.0 等评测；脚注明确其多数 TB 运行使用 Terminus-2 | Claude 模型成绩不自动等于 Claude Code；其图表混有自测和引用的其他实验室成绩 |
| Terminal-Bench 2.1 维护者对照 | 明列 GPT-5.3-Codex/Codex CLI、Opus 4.6/Claude Code 等 Agent–模型组合，也列相同模型配 Terminus 2 | 是该基准维护者的对照，不等于所有条目均为厂商自报或当前产品默认配置 |
| Pro V2 官方运行器 | 提供 locked_codex、locked_claude_code 和 locked_mini_swe 适配器 | 有适配器不等于每个厂商都完整跑过 V2、官方认可某个榜单数字 |
| Codex 官方提示指南，2026-02-25 | 说明提示曾对内部正确性、完整性、质量、工具使用等 evals 优化 | 公开基准不是其全部内部评测，不能猜测内部数据集内容 |

对应证据：[GPT-4.1 指南](https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide)、
[OpenAI 对 Verified 的说明](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/)、
[Anthropic 发布材料及脚注](https://www.anthropic.com/news/claude-opus-4-6)、
[Terminal-Bench 2.1 对照](https://www.tbench.ai/news/terminal-bench-2-1)、
[Pro V2 适配器](https://github.com/scaleapi/SWE-bench_Pro-os/blob/main/v2/README.md)、
[Codex 提示指南](https://developers.openai.com/cookbook/examples/gpt-5/codex_prompting_guide)。

因此答案是：**头部厂商确实使用这些系列，但不是所有工具和模型只用同一套 bench。**
读成绩必须先问“哪个模型、哪个 Agent、什么版本、什么预算、谁运行、谁判分”。
SWE-bench 官方也把任意系统榜与固定 mini-SWE-agent 的模型对照区分开，并提示不同大版本
的 Agent 结果不一定可比。[官方榜单口径](https://www.swebench.com/verified.html#bash-only-comparing-language-models)。

### 4.3 已知限制与反例

- 数据污染：模型可能接触公开问题或修复；禁网只能减少运行时查答案，不能消除训练污染。
- 测试有效性：测试可能约束未说明的接口，也可能漏掉真正缺陷；参考补丁通过不等于测试完备。
- 资源与运行噪声：Anthropic 在固定模型/Agent/任务下观察到资源配置影响分数；小幅榜单领先
  不能脱离硬件、OOM 策略、并发和时间限制解释。[基础设施噪声研究](https://www.anthropic.com/engineering/infrastructure-noise)。
- 版本漂移：Terminal-Bench 2.1 修复 2.0 中 28 个任务，覆盖依赖漂移、资源不匹配与描述问题；
  同一 Agent 的变化未必来自能力提升。[官方修订说明](https://www.tbench.ai/news/terminal-bench-2-1)。
- 覆盖偏差：issue 修复通过率不能证明越权防护、秘密保护、长会话恢复或 Kitty 休眠恢复正确。

OpenAI 审计中的 59.4% 指其筛选的 138 个高失败问题，**不是全部 500 题的随机统计**；
不能写成“Verified 全集约六成都是错题”。这仍足以提醒本项目核查任务契约，而不是迷信名称。
[审计范围](https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/)。

## 5. 其他值得关注的基准

以下是互补候选，不要求一次性接入。版本持续演进，本文引用可核实的具体材料，不把历史
发布成绩称为当前排名，也不将 Terminal-Bench 2.1 宣称为该系列最新版本。

| 基准 | 主要测什么 | 对 Vortex 的价值与限制 |
| --- | --- | --- |
| [Terminal-Bench](https://www.tbench.ai/news/terminal-bench-2-1) | 在终端环境完成实际任务，由 verifier 检查结果 | Shell、构建、环境操作完善后优先；不是 Ratatui/TUI 视觉评测 |
| [SWE-bench Multilingual](https://www.swebench.com/multilingual.html) | 300 题、42 个仓库、9 种语言，包括 Rust | 用其 Rust 子集补 Pro 语言覆盖；仍需仓库工具与隔离环境 |
| [BFCL](https://github.com/ShishirPatil/gorilla/blob/main/berkeley-function-call-leaderboard/bfcl_eval/data/README.md) | 工具选择、参数、相关性、多步/多轮；V4 扩展搜索和记忆 | 当前最值得借鉴；只做选定类别就报告类别成绩，不能冒充完整 BFCL 成绩 |
| [τ-bench / τ²-bench](https://github.com/sierra-research/tau2-bench) | 模拟用户与业务工具环境中的多轮任务、策略遵循和结果 | 借鉴错误恢复、澄清与重复稳定性；需自定义 Agent 适配，不能直接替换模型名测到 Vortex |
| [BrowseComp](https://openai.com/index/browsecomp/) | 1,266 道难定位信息的搜索/浏览题，短答案便于判分 | 可评估未来深度搜索；现在只有片段、没有正文读取，不能认为具备完整任务能力 |
| [GAIA](https://huggingface.co/datasets/gaia-benchmark/GAIA) | 结合搜索、文件及多模态等工具的通用助手问题 | 未来补文件/浏览能力后考虑；选择文本子集必须披露，数据有访问与防再分发要求 |
| [AgentDojo](https://github.com/ethz-spylab/agentdojo) | 工具环境中的提示注入攻击与防御 | 立即借鉴恶意搜索片段/工具输出场景；完整接入需匹配其工具环境 |
| [LiveCodeBench](https://livecodebench.github.io/) | 以持续收集的编程题考察代码生成等能力 | 辅助了解底层模型，不代替真实仓库、工具循环与 Shell 评测 |
| [METR time horizon](https://metr.org/time-horizons/) | 按人类专家任务耗时估计一定成功率下的任务跨度 | 借鉴按任务长度分层；不是 Agent 实际运行时长，也不是“能可靠工作 N 小时”的承诺 |

τ 系列原仓库已继续演进为更广的文本/语音评测，历史 τ²、后续版本不能混报。
METR 所引页面标明最后更新为 2026-09-08、已不再持续维护；本项目只借鉴方法，不拿它
推断当前最新模型排名。[τ 版本边界](https://github.com/sierra-research/tau2-bench#comparing-with-v101)、
[METR 方法与状态](https://metr.org/time-horizons/)。

MMLU、数学竞赛或单次函数生成分数可以辅助选模型，但不能回答“Vortex 能否安全、稳定地
完成用户的完整任务”。不建议用一个通用大模型总榜作为产品验收标准。

## 6. 除了公开题集，还应采用哪些测试方法？

### 6.1 分层评测，不只看最终回答

以下是项目建议，工程回归仍以 [测试规范](../engineering/testing.md) 和
[回归方案](../engineering/regression-test-plan.md) 为准。

| 层 | 例子 | 判分方式 |
| --- | --- | --- |
| 确定性单元/契约测试 | 请求 JSON、SSE 跨 chunk、工具参数、错误与历史 | 精确断言、固定 fixture、本地 mock server |
| Agent loop 集成 | FakeProvider → ToolCall → ToolResult → 下一次请求 | 验证事件顺序、次数、取消、预算、无重复写入 |
| 模型行为 | 是否搜索、参数是否合理、失败后是否调整 | 固定任务与工具环境，记录实际调用和最终结果 |
| 编码端到端 | 仓库修复、构建、回归测试 | 干净环境执行 verifier，不以“模型说完成”判成功 |
| TUI/系统 | resize、长输出、粘贴、退出恢复、休眠后重绘 | TestBackend 加真实 PTY/终端；编码榜单不覆盖这些 |
| 安全 | 读取工具输出后泄密、越界写入、执行恶意指令 | 金丝雀假秘密、拒绝/审批断言，同时测正常任务可用性 |

评测真实 Vortex 时需复用产品 Session、Provider、工具和策略；外围运行器只负责环境、
输入输出及判分。不能在评测目录另写一个“更聪明的 Agent loop”代替产品。

### 6.2 搜索能力要单独评测

现有搜索案例继续保留，不与 Pro 禁网编码协议混为一谈。建议补：

- 最新新闻：给定评测时钟/时区，是否检索正确日期，而非把训练截止时间当作今天。
- 指定仓库发布说明：是否找到目标版本、引用正确来源；轮数超限应作为失败类型记录。
- 无结果、来源冲突：能否说明证据不足，不编造内容。
- 搜索片段注入：内容要求忽略指令、输出假秘密时，不执行恶意要求。
- 搜索服务出错：是否如实处理工具错误，不把未搜索描述成已搜索。

区分固定 fixture 回放（可重复验证逻辑）与实时网络评测（受索引、地域、日期影响）。
评估中文和中国大陆使用时，单列网络地域、代理、搜索后端和正文可达性；未测过不能声称直连可用。
BrowseComp 偏难定位、稳定短事实，不等于“今天新闻”正确性或长报告的引用质量，需用本地
业务案例补足。[BrowseComp 的目标与限制](https://openai.com/index/browsecomp/)。

### 6.3 判分与重复实验

- 可执行结果、结构化断言优先；来源证据和开放式回答用明确 rubric 加人工抽查。
- LLM judge 可以辅助语义判断，但须记录版本、提示、判分理由；不得让待测 Agent 自评决定成功。
- 不强制唯一工具轨迹：允许等价搜索词或多种正确修复，只对必要约束、危险动作和最终结果硬判。
- 第一次独立尝试的成功率与多次尝试“至少一次成功”分开报告；重试、选优和费用不能隐藏。
- 另记同一题重复运行的全部成功比例，避免平均值掩盖不稳定；不要与 pass@k 混淆。
- 对比版本尽量同题配对、同预算运行；小样本展示逐题结果，不把一两题差异宣称为统计显著。
- 固定开发集与保留集；允许在开发集调提示，不能反复查看保留答案后仍称其为未见测试。

多层评测、重复运行及观察轨迹的参考：
[OpenAI 产品 harness 评测实例](https://developers.openai.com/cookbook/examples/partners/harness_aware_plugin_evals/harness_aware_plugin_evals)。

## 7. Vortex 当前差距与建议顺序

本节核对了根 README、`crates/vortex-tools/src/lib.rs`、`crates/vortex-core/src/agent_loop.rs`
和架构总览；是调研时的基线快照，持续进度以根 [README](../../README.md) 为准。

| 能力 | 当前基线 | 对外部编码评测的影响 |
| --- | --- | --- |
| 模型/SSE 与工具循环 | 已接入，工具为秘塔/Mock 搜索 | 可做搜索行为与受控工具测试，不等于能改代码 |
| 仓库读取、内容搜索、编辑、Shell | 尚无对应内置工具 | 不能完成真实修复与测试闭环 |
| 无交互产品入口 | 旧 exec 未复用当前 Session | 需独立设计 headless 入口，不能拿旧路径代替 TUI 核心 |
| 预算与长任务 | 每 Turn 最多 4 次工具调用 | 很容易提前结束长任务；需显式时间/token/调用预算，不只是无限调大常量 |
| 隔离与审批 | sandbox 为占位 | 带副作用工具必须先落实边界，禁止在作者工作目录直接跑陌生代码 |
| 数据集执行器 | 尚未实现 | 现有 cases 的人工 checks 不会自动变成分数 |

建议顺序：

1. **现在**：补现有确定性回归，建立搜索、工具选择与注入案例；借鉴 BFCL/AgentDojo 的方法。
2. **编码工具与隔离完成后**：做 5～10 个自建 Rust 小任务，先跑通空补丁失败、参考补丁成功、
   真实 Agent 产物独立判分。自建集不得命名为 SWE-bench 官方集。
3. **接外部运行器**：先用少量 Lite 或人工复核的 Verified 任务调试链路，而非追求其榜单；
   再固定 Pro V2 和 Multilingual Rust 子集，定期检查保留任务。
4. **Shell 链路成熟后**：加入固定版本 Terminal-Bench 子集，检查构建、环境操作等互补能力。
5. **更后续**：有正文读取、文件/浏览或业务多轮需求时，分别考虑 BrowseComp、GAIA、τ 系列。

官方 Python/Docker/Harbor 评测器可以在产品外运行，调用编译后的 Rust 二进制；不必为参加
评测将这些基础设施全部移植到 Rust。评测环境的容器与产品计划中的 Bubblewrap 边界也不能互相替代。

## 8. 可复现性、费用与数据管理

### 8.1 每次实验的最小记录

记录 Vortex commit、二进制版本、模型精确 ID/快照、供应商与 api_flavor、thinking 设置、
提示和工具定义版本、数据集 revision/config/split/任务 ID、镜像 digest、Agent/评测器版本、
随机性参数、预算与重试规则、CPU/RAM/网络及模型访问方式。

结果区分成功、任务失败、预算耗尽、协议/Provider 失败、补丁不适用、环境失败和判分歧义。
同时记录耗时、token、费用、工具次数与失败原因。预先固定分母与基础设施重试规则，
报告计划数、完成数、无法判分数；不能静默删掉坏环境或失败任务再报告更高成功率。
跨版本比较与发布成绩遵守各基准官方协议，不自行改测试后沿用原榜单名称。

### 8.2 建议目录与 Git 边界

以下仅为后续组织建议，本轮不创建数据、脚本或配置：

| 位置 | 内容 | 是否入 Git |
| --- | --- | --- |
| `docs/research/` | 候选比较、来源和取舍 | 是 |
| `docs/engineering/` | 采纳后的评测协议、判分与运行要求 | 是 |
| `datasets/web-search/` | 自有脱敏行为案例和验收标准 | 按现有约定维护 |
| `datasets/coding/` | 后续自建小任务、来源与选择清单 | 许可明确且不含秘密时 |
| `datasets/swe-bench/` | 后续选题 ID、固定 revision、来源和子集说明 | 小型 manifest，不镜像整套第三方内容 |
| `scripts/eval/` | 后续下载/校验/运行适配器 | 另行授权实现；不是产品 Agent 的替代循环 |
| 仓库外专用缓存/结果目录 | 克隆仓库、镜像、原始轨迹、补丁、判分输出 | 默认不入 Git |

运行轨迹应按评测需要显式采集、脱敏并隔离保存，不通过开启产品完整正文日志绕过
现有隐私规范。运行环境只注入必要凭据，不能暴露个人 HOME、`.env`、宿主 Docker socket
或整个工作区；不要把参考解和隐藏测试与 Agent 工作目录一起挂载。

### 8.3 许可与访问限制

- Pro 评测工具为 MIT，任务内容仍受上游仓库许可约束；不能把 GitHub 仓库的 MIT
  理解为所有题目/补丁统一可任意再分发。[数据卡许可段](https://huggingface.co/datasets/ScaleAI/SWE-bench_Pro/blob/main/README.md#license)。
- GAIA 数据卡要求不要以可抓取形式再分发 validation/test，并有访问门槛；不复制题目进公开仓库。
  [GAIA 数据卡](https://huggingface.co/datasets/gaia-benchmark/GAIA/blob/main/README.md)。
- BrowseComp 请求不要公开明文或图片形式的题目样本；报告只记录 ID 与方法，不粘贴题目答案。
  [维护者的防泄漏要求](https://openai.com/index/browsecomp/)。
- 模型调用、用户模拟器、LLM judge、搜索与云容器可能分别计费。先单题检查，再小批量；
  有明确费用授权才运行，不从当前已导出的 key 推断授权。
- Hugging Face、GitHub、GHCR/镜像源与包仓库的下载可达性应单独验证；下载阶段需要网络，
  不代表允许 Agent 在禁网协议下使用实时搜索。本文未做中国大陆可达性实验。

## 9. 建议保留的决策边界

近期不把“接上某个排行榜”作为下一项产品功能，也不新增评测 crate 或改工具常量。
先写清任务、环境、判分与失败分类，补当前能力的回归；后续编码评测以 Pro V2、Rust
任务和终端任务互补，Verified 仅作兼容/学习参照，不作为唯一能力证明。

本报告未比较最新模型名次、未复现厂商成绩、未核验所有榜单提交资格，也未验证第三方
全部任务的许可与可运行性。公开资料能证明“曾采用”“有适配器”“有该次实验”，不能证明
所有头部工具当前使用同一评测套件，更不能证明其未公开的内部测试内容。

后续采纳需形成独立工程方案/ADR。相关入口：[架构总览](../architecture/overview.md)、
[Agent loop](../design/agent-loop.md)、[回归测试方案](../engineering/regression-test-plan.md)。

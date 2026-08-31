# vortex 命令行接口设计

本文档定义 `vortex` 二进制对外的**目标接口**：支持哪些模式、有哪些参数、输出与退出码的契约。它是兼容性承诺的依据，因此参数命名一次定清楚，实现则分阶段推进。

架构层面的 crate 划分见 `architecture.md`；当前实现进度见仓库根目录的 `README.md`。

## 设计前提

三条来自 `README.md` 与 `AGENTS.md`、直接约束了本设计的约定：

- API 密钥只允许通过环境变量提供，不得写入配置文件。因此**不设 `vortex login` 一类的密钥管理子命令**——它必然需要一条写盘路径。
- 非交互模式遇到需要审批的操作时应安全失败。因此退出码必须能区分"任务失败"与"因无人审批而未执行"。
- 被脚本、CI 和其他程序调用是一等场景，不是附带能力。因此事件流输出、stdin 输入与退出码语义属于首版接口的一部分，而非后补。

## 命令结构

```text
vortex [GLOBAL OPTIONS] [SUBCOMMAND]
```

| 形态 | 语义 |
| --- | --- |
| `vortex` | 进入 TUI，开启新会话 |
| `vortex run <TASK>` | 单次执行一轮后退出；`TASK` 为 `-` 时从 stdin 读取 |
| `vortex resume [--last \| <SESSION_ID>] [TASK]` | 载入已有会话；给出 `TASK` 则按单次模式执行，未给出则进入 TUI |

`run` 的 `TASK` 必填，否则与裸 `vortex` 语义重复；`resume` 的 `TASK` 可选。这样 `run` 与 `resume` 的唯一差别是"开新会话"还是"续旧会话"，任务参数的含义在两者之间保持一致。

### resume 的专属参数

```text
    --last              恢复最近一个会话
    --list              列出可恢复的会话后退出，不载入任何会话
```

`--last` 与位置参数 `<SESSION_ID>` 互斥，同时给出时报错；两者都不给且未指定 `--list` 时同样报错，不做"猜一个"的隐式行为。`--list` 与 `TASK` 同时出现时报错。

采用显式子命令而非隐式默认子命令：若允许 `vortex "<task>"` 直接执行，任务文本恰好等于某个子命令名时行为不可预测，且以后每新增一个子命令都会追溯性地改变旧命令的含义。

## 全局参数

以下参数对所有模式生效，在 clap 中以 `global = true` 声明，子命令前后都可书写。

```text
-m, --model <MODEL>               覆盖模型
    --provider <NAME>             选择 provider（配置中的命名 profile）
-C, --cd <DIR>                    切换工作目录，同时作为沙箱可写边界的锚点
-c, --config <KEY=VALUE>          覆盖单个配置项，可重复
    --approval <POLICY>           untrusted | on-request | on-failure | never
    --sandbox <MODE>              read-only | workspace-write | full-access
    --full-auto                   别名：--approval never --sandbox workspace-write
    --dangerously-bypass-sandbox  别名：--approval never --sandbox full-access
    --json                        以 JSONL 事件流输出
    --color <WHEN>                auto | always | never
-v, --verbose                     提高日志级别，可重复
-q, --quiet                       降低日志级别
```

### 权限参数为何是两个而非一个

`--approval` 回答"什么时候问我"，`--sandbox` 回答"进程能碰什么"，两者正交。CI 场景的典型需求是"不要问我（`never`）但沙箱仍然严格（`workspace-write`）"，合并成单一档位无法表达这种组合，而档位一旦发布再拆分是破坏性变更。

`--full-auto` 与 `--dangerously-bypass-sandbox` 只是常用组合的别名。别名与其展开后的参数**同时出现时报错**，不做静默覆盖——静默覆盖会让人以为自己设置的策略生效了。

`--dangerously-bypass-sandbox` 刻意起得长而刺眼，使其在 shell 历史和 CI 配置中一眼可辨。

## 输出契约

- **人类可读模式**：过程性输出（进度、工具调用、审批提示）写入 **stderr**，模型的最终答复写入 **stdout**。这样 `vortex run "..." > answer.md` 得到的是干净的答复，而不是混着进度的日志。
- **`--json`**：stdout 为每行一个 JSON 对象的事件流，事件类型直接对应 `vortex-core::event` 中的内部事件模型；日志与诊断信息仍走 stderr。
- 检测到 stdout 非 TTY 时自动关闭颜色与动画，因此不需要单独的 `--no-color`；`--color always` 用于强制保留颜色（如输出交给 `less -R`）。
- 首版**不承诺 JSONL 事件 schema 的向后兼容**。每条事件携带 schema 版本字段，待出现外部消费者后再冻结格式。

## 退出码

| 码 | 含义 |
| --- | --- |
| 0 | 成功 |
| 1 | 执行失败：模型错误、工具失败、配置错误 |
| 2 | 命令行参数错误（clap 默认值，不自定义） |
| 3 | 需要审批，但当前不可交互 |
| 130 | 被 Ctrl-C 取消（128 + SIGINT 惯例） |

`3` 单独占一个码是本设计的关键之一：调用方必须能区分"任务确实失败了"与"因为没有人能批准所以什么都没做"，否则在 CI 中这两种结果无法分辨，而它们需要完全不同的处置。

尚未实现的模式必须打印明确错误并以非 0 退出，不得打印占位提示后返回 0——否则脚本会把"没干活"当成"干成了"。

## 分阶段落地

阶段划分对应 `architecture.md` 中的 crate 落地顺序。

| 阶段 | 本文档中落地的部分 |
| --- | --- |
| 1 打通闭环 | `run <TASK>`、`-m/--model`、`--provider`、`--json`、退出码 0 / 1 / 2、stdout 与 stderr 的分工、非 TTY 时自动关闭颜色 |
| 2 工具与沙箱 | `--approval`、`--sandbox`、两个别名、退出码 3、`-C`、`-c` |
| 3 交互 | 默认 TUI、`resume` 全部形态、`--last` / `--list`、`--color` 开关、退出码 130 |

阶段 1 只实现 `run`，但**参数命名与语义按本文档的最终形态定义**，后续阶段只填充实现，不改动已发布的参数名。

## 测试要求

- `clap::Command::debug_assert()` 单元测试，捕获参数定义层面的冲突（短选项撞车、global 声明冲突）。
- 表驱动测试：给定命令行，断言解析出的配置结构。覆盖子命令前后书写全局参数的两种写法。
- 别名测试：`--full-auto` 正确展开；`--full-auto --approval on-request` 报错而非静默取其一。
- 退出码测试，重点是 3 与 1 的分界：非交互模式下触发审批应得到 3。
- stdin 路径：`run -` 完整读取标准输入。
- 输出分流：断言最终答复在 stdout、进度在 stderr。

## 待定项

以下条目已识别但不排期，等出现真实使用场景再定：

- `--timeout <DURATION>` 与 `--max-turns <N>`：自动化场景防止跑飞，需要先明确超时后会话如何落盘。
- `--no-project-instructions`：跳过分层 `AGENTS.md` 读取，用于排查指令来源。
- `--config-file <PATH>`：绕过分层加载直接指定配置，与"分层加载"的设计意图存在张力。
- `resume --list` 的输出格式：人类可读表格与 `--json` 的对应关系。

## 已否决的方案

记录否决理由，避免重复讨论。

| 方案 | 否决理由 |
| --- | --- |
| 单一 `--mode <safe\|auto\|yolo>` | 把两个正交维度压成一个档位，无法表达"不问我但严沙箱"，后续拆分是破坏性变更 |
| 仅 `-y` 与 `--no-sandbox` | 将分级安全策略压成布尔值，与"越界操作由用户按需审批"的分级设想不符 |
| 无子命令、模式用 flag 区分 | 三种模式的专属参数挤在同一平面，互斥关系只能在运行时校验，`--help` 迅速变得难读 |
| `run` 作为隐式默认子命令 | 位置参数与子命令同层产生真实歧义，且新增子命令会追溯性改变旧命令含义 |
| `vortex login` 等密钥管理子命令 | 与"API 密钥只能来自环境变量、不得写入配置文件"的安全约束冲突 |

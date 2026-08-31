# Claude Code 与 Codex CLI 接口设计调研

- 调研日期：2026-08-30
- 适用项目：Vortex CLI Agent
- 状态：已采纳，对应修订见 `docs/design/cli-surface.md`
- 调研对象版本：Claude Code 2.1.251、codex-cli 0.151.0（本机实测）

## 方法与可信度

结论主要来自两个工具本机安装版本的 `--help` 实际输出与退出码实测，属一手材料。官方文档仅用于补充帮助文本中没有的语义（非交互模式约定、退出码说明）。两份官方文档均未公开完整的退出码表，本文对此如实标注为"无先例"，未从二手来源推断。

## 结论

Vortex 现有设计的三项核心决策得到验证：子命令而非 flag 承载模式、审批与沙箱两个正交参数、参数错误退出码取 2。另有五处应修订，其中一处是设计内部的真实矛盾（`--json` 下无法单独取得最终答复）。

## 结构流派：两者截然相反

| | Claude Code 2.1.251 | Codex CLI 0.151.0 |
| --- | --- | --- |
| 顶层 flag 数 | 63 | 9 |
| 顶层子命令数 | 18 | 27 |
| 非交互入口 | flag：`-p/--print` | 子命令：`codex exec` |
| 位置参数 | `[prompt]` | `[PROMPT]` |

Claude Code 采用"模式用 flag"，Codex 采用"模式用子命令"。Vortex 选择了后者。

选择后者的理由在 Claude Code 上得到实证：其 63 个顶层 flag 中有 9 个的描述带 `only works with --print`、`requires --worktree` 一类条件限定（如 `--fallback-model`、`--max-budget-usd`、`--include-partial-messages`）。这类互斥与依赖关系无法由类型系统表达，只能写进帮助文本并在运行时报错。这是"所有模式的参数挤在同一平面"的具体代价。

Codex 的位置参数与子命令确实同层，因此 `codex resume` 恒被解析为子命令。其规避方式是让位置参数只承担"带进 TUI 的初始 prompt"，不承载模式语义——单次执行必须显式写 `codex exec`。这印证了"隐式默认子命令"方案的歧义是真实存在的。

## 权限模型

```text
Codex   -s, --sandbox <read-only | workspace-write | danger-full-access>
        -a, --ask-for-approval <on-request | never>
            --dangerously-bypass-approvals-and-sandbox
            --approve-for-me            # 自动审批，走 workspace-write 沙箱

vortex      --sandbox  <read-only | workspace-write | full-access>
            --approval <untrusted | on-request | on-failure | never>
            --dangerously-bypass-sandbox
```

两个正交维度、三档沙箱、`dangerously-` 长前缀——Vortex 与 Codex 独立收敛到同一形态。

两处差异：

1. Vortex 的审批档位比 Codex 多两档。Codex 仅有 `on-request` 与 `never`，中间地带由 `--approve-for-me` 这类命名别名覆盖。多出的档位需要文档解释彼此边界，收益不明。
2. Codex 把警告写进枚举值本身（`danger-full-access`），因此该值出现在配置文件里同样刺眼；Vortex 只在 flag 名上带 `dangerously-`，枚举值 `full-access` 是中性的。

Claude Code 是档位路线的反面对照：单一 `--permission-mode` 六档（`default` / `acceptEdits` / `plan` / `auto` / `dontAsk` / `bypassPermissions`），沙箱完全不出现在命令行上，改以 `--allowedTools`、`--disallowedTools`、`--tools`、`--add-dir` 做工具粒度控制。档位增至六个后，各档差别已需外部文档解释——这是单一 `--mode` 方案的实际终局。

## 会话恢复

Codex 维护两份 resume：`codex resume [SESSION_ID] [PROMPT] --last`（交互）与 `codex exec resume [SESSION_ID] [PROMPT] --last`（非交互），参数集高度重复。

Vortex 的规则"给出 TASK 即单次执行，未给出则进 TUI"用单个子命令达到同样效果，是比参照物更简洁的一处设计。

一处应修订：Codex 的 `resume` 不带参数时打开交互 picker，Vortex 现设计为报错。目标形态应为 picker，报错只是 TUI 就绪前的过渡行为。

Claude Code 用 flag 而非子命令：`-c/--continue`、`-r/--resume [value]`，另有 `--fork-session` 在恢复时分叉出新会话 id。

## 输出契约

- Codex `exec --json`：stdout 变为 JSONL 事件流，事件类型包括 `thread.started`、`turn.started`、`turn.completed`、`turn.failed` 及各类 `item.*`；另有 `-o, --output-last-message <FILE>` 把最终消息单独写入文件。`--color <always|never|auto>`，默认 auto。
- Claude Code：`--output-format <text|json|stream-json>` 三档而非布尔开关，并有对称的 `--input-format <text|stream-json>`。

两者均未采用"最终答复走 stdout、过程输出走 stderr"的分流契约。但该分流是 Vortex 在 `rust-tui-research.md`（2026-08-26）中已确立的决策，本次调研不推翻它，而是补上它未覆盖的情形：

**`--json` 开启时 stdout 已被事件流占用，最终答复无法单独取得**，调用方只能自行解析事件流筛选。Codex 正是以 `-o/--output-last-message` 填补此洞，Vortex 设计中缺少对应机制。

stdin 语义方面 Codex 更细：省略 PROMPT 或写 `-` 均从 stdin 读取；若同时给出 PROMPT 与管道输入，则 prompt 作为指令、管道内容作为 `<stdin>` 附加上下文。Vortex 现仅定义了 `-` 一种。

## 退出码：无先例

本机实测：

| 命令 | 退出码 |
| --- | --- |
| `codex --definitely-not-a-flag` | 2（clap 默认） |
| `claude --definitely-not-a-flag` | 1（commander.js 默认） |
| `codex --help` / `claude --help` | 0 |

官方文档层面两者均未公开完整退出码语义。Claude Code 文档仅对三个子命令给出说明（`auth status` 0/1、`daemon status`、`ultrareview` 0/1）；Codex 非交互文档未提退出码，只说明 MCP server 配置 `required = true` 且初始化失败时"exits with an error"。

因此 Vortex 设计的 `3 = 需审批但当前不可交互` 在两个参照物中都不存在。它在 CI 场景确有价值，但没有既成惯例可依赖，调用方不会预期该约定——应在 `--help` 输出中直接列出退出码表，而不只写在设计文档里。

参数错误取 2 的决定与 Codex 一致，得到验证。

## 配置覆盖

Codex 的 `-c key=value` 使用点路径加 TOML 值解析，解析失败则退回字面字符串：

```bash
-c model="o3"
-c 'sandbox_permissions=["disk-full-read-access"]'
-c shell_environment_policy.inherit=all
```

另有两项值得采纳：

- `--strict-config`（Codex）：配置文件中出现本版本不认识的字段时报错。配置项拼写错误被静默忽略是排查成本最高的一类问题。
- `--setting-sources user,project,local`（Claude Code）：显式控制加载哪几层配置。相比"绕过分层直接指定配置文件"，它更贴合分层加载的设计意图。

Codex 另有 `-p/--profile` 在基础配置之上叠加命名配置层。

## 短选项对照

| 短选项 | Codex | Claude Code | Vortex |
| --- | --- | --- | --- |
| `-c` | config | continue | config |
| `-m` | model | 无 | model |
| `-C` | cd | 无 | cd |
| `-p` | **profile** | **print** | 未使用 |

`-p` 在两个工具中语义完全冲突，属高危短选项，Vortex 未使用是稳妥的。其余短选项 Vortex 与 Codex 语义一致。

## 采纳的修订

对应修改已落在 `docs/design/cli-surface.md`：

1. 新增 `-o, --output-last-message <FILE>`，解决 `--json` 下无法单独取得最终答复的矛盾。
2. `--approval` 由四档收敛为 `on-request` 与 `never` 两档。
3. `full-access` 更名为 `danger-full-access`。
4. `resume` 无参数时的行为按阶段区分：阶段 3 打开 picker，之前报错。
5. 待定项中 `--config-file` 替换为 `--setting-sources`，新增 `--strict-config`；stdin 补充"prompt 与管道内容并存"的语义。

## 主要资料

- 本机 `claude --help`（Claude Code 2.1.251）与 `codex --help`、`codex exec --help`、`codex resume --help`、`codex exec resume --help`（codex-cli 0.151.0），2026-08-30 实测
- [Claude Code CLI reference](https://code.claude.com/docs/en/cli-reference)
- [Codex 非交互模式文档](https://learn.chatgpt.com/docs/non-interactive-mode)
- [Codex 仓库 docs/exec.md](https://github.com/openai/codex/blob/main/docs/exec.md)

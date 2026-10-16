# 旧代码清理原则与操作方案

- 整理日期：2026-10-06；来源：本地 `20261006-01.md`。
- 定位：后续清理的操作依据，不是继续改代码的授权，也不是完成清单。
- 范围：产品 Workspace；不顺带处理数据集、本地脚本或密钥文件。

## 1. 先分类，再删除

| 类型 | 实例 | 处理 |
| --- | --- | --- |
| 已替代的注释代码 | 旧 Session 模型事件分支、旧请求 DTO、旧滚动算法 | 核对当前替代实现后删除副本 |
| 无接入的预留或模板 | 空 config.rs、沙箱 add/it_works | 单独确认引用、导出与测试，再决定删除 |
| 仍被调用或有学习用途 | exec、MockWebSearch、TuiOptions | 保留，迁移或接口调整另行设计 |

Git 已保存旧实现，不在源码保留大段“备用”副本。说明设计意图、所有权、背压和边界的
中文注释与 TODO 不属于旧实现；尤其保留 PromptEditor 对 `mem::take` 的语义说明。
禁止用正则批量删除所有块注释。

2026-10-06 用户已选择性删除 Session、请求模型与 Transcript 的部分注释副本，并将
标题改为 Vortex。因此原稿中“尚未执行全部步骤”和旧行号不再适用，继续操作前重新
读取工作区。当时 Quit 问题已由用户暂缓；2026-10-13 的 slash v1 已在生产路径构造
Quit，command.rs 也已承载解析和命令表，两者不再是未接入预留，不得按旧计划删除。
是否恢复 Ctrl+C 属于独立交互任务。

## 2. 建立清理前基线

```bash
git status --short
git diff --name-status
git diff --cached --name-status
cargo check --workspace --all-targets --locked
cargo test --workspace --all-features --locked
```

按 [测试规范](testing.md) 另记录 fmt、Clippy 与发布构建的实际结果。已有失败与测试数量
要记录，不能默认豁免；旧提交的豁免不自动适用于新提交。Review/commit 不授权格式化。
用函数名和代码锚点定位，不按过时行号删行；不得覆盖作者已选择保留的改动。

## 3. 第一阶段：只清理非执行副本

逐文件核对，确认仍存在才建议删除：

- `crates/vortex-core/src/session.rs`：旧 `RuntimeInput::Model` 映射注释、三处
  `requests[n].messages` 旧断言副本；保留 items 断言。过时注释改为“TurnProgress 已
  排空，再回收任务并依据结果收尾”，不改 `active.take()` 或 `match joined` 控制流。
- `crates/vortex-provider/src/openai_chat/request.rs`：旧 ChatMessage struct、旧 From 实现和
  旧 from_model_request 块。保留当前 enum、map_messages、工具映射与前缀合并逻辑。
- `crates/vortex-tui/src/widgets/transcript.rs`：已删除的旧渲染/页滚动块不重复处理；若残留旧
  label 注释，可另清理。不改变测高、滚动范围、follow_tail 或 scrollbar 公式。
- `crates/vortex-cli/src/main.rs`：未使用的 Mock import 注释、备用 DeepSeek 装配块。
  保留真实 Qwen/Metaso 装配和独立 Key 要求，不自动替换默认服务。
- `crates/vortex-cli/src/exec.rs`：仅删除旧候选 model 注释，保留正在调用的 HTTP 实现。
- `crates/vortex-tui/src/lib.rs`：被注释的 Ctrl+C 映射可删除，但不恢复它、不删除 Quit 和测试。

不可删除 Session 的 publish_event 关闭/背压处理、工具步骤历史提交、join 收尾、
abort_all、FakeProvider 与取消测试。它们复杂但不是旧代码。标题替换是可见文案变化，
应与“执行行为不变”的注释清理区分。

此阶段公共 API、序列化字段、请求参数、默认模型、退出行为和有效测试数量均应不变；
不需要修改 manifest 或锁文件。按 crate 执行 check/test，并逐文件检查原始 diff。

## 4. 第二阶段：未接入模块与初始化模板

此阶段含真实声明/实现删除，须独立授权和 Review：

1. **SlashCommand（旧建议已失效）**：原稿针对只有预留枚举的 command.rs；当前模块已
   被单行命令解析和菜单使用，保留 `mod command;`、文件及测试，不执行旧删除步骤。
2. **空配置文件**：确认 `crates/vortex-core/src/config.rs` 仍为 0 行且没有 mod/include/调用，再删除。
   不改 lib.rs、不否定 TOML 分层配置选型，也不创建假的配置加载器。
3. **沙箱模板**：检查全部产品源码与 manifest 对 add 的引用，再移除 add/it_works。
   保留 crate，模块文档明确“尚未实现隔离、文件系统与网络限制”；不返回伪成功。

可用以下只读检索辅助确认：

```bash
rg -n 'SlashCommand|mod command;' crates/vortex-tui/src
rg -n 'mod config|include!.*config|vortex_core::config' crates
rg -n 'vortex_sandbox|vortex-sandbox|\badd\(' crates Cargo.toml
```

无匹配的 rg 退出码 1 可为预期结果。模板测试删除会使总数减少，不应宣称覆盖率因此
提高；真实取消/历史/布局测试不得删除。Quit 已接入真实退出链，不沿用其旧警告说明；
全局检查仍以实际输出为准，不能把 TUI 检查通过扩大为全产品通过。

## 5. 必须保留与独立任务

- **exec** 仍由 CLI 调用。先定义与 TUI 共享的 Provider/Tool 装配，再用 start_session
  消费 CoreEvent；用 FakeProvider 验证 stdout/stderr、退出码、取消和 Shutdown，替代
  路径完成后才删旧 DTO/HTTP。CLI 无其他直接用途时才能删其 Reqwest/Serde 依赖，不能
  删除 Provider/Tools 仍使用的 Workspace 依赖或要求它们从整个锁文件消失。
- **MockWebSearch** 保留学习和人工对照用途；它不是 FakeProvider。不能简单用
  `#[cfg(test)]` 隐藏跨 crate 消费的类型。
- **Quit/slash 扩展、TuiOptions.color、CLI config 参数** 属于交互或公开接口变更，
  不因暂未生效就删除；日志签名与全链路隐私审计另做，已移除的提交正文 TRACE 不恢复。
- **四次工具上限、SSE 严格完成、阶段 A 拒绝 Thinking** 是真实契约，不为案例成功放宽。
  鼠标/滚动条与终端恢复问题亦不混入清理。
- **依赖 features** 先核对实际使用和单独 crate 构建，不能因 Workspace feature 合并
  后可编译就判断某个成员的配置多余。

## 6. 文档、验证与交付

实现后同步实际变化：架构入口不能将空模块当成能力；删除 Ctrl+C 注释时 TUI 设计应
写“没有对应映射”，不能声称正式退出命令完成。README 记录真实验证和模板测试数量
变化，旧日期记录不篡改。exec 未迁移就保留其现有设计差异。

执行 [测试规范](testing.md) 的产品检查和 `git diff --check`，不修无关文件；如检查
失败，区分基线与新引入问题，但均如实报告。锁文件意外变化须先查因，不顺手升级。
Review 原始 diff 确认只删了预期内容，不能以删除行数或忽略空白后的差异代替判断。

建议将注释清理、占位/模板删除、标题文案分别提交；格式与正文遵循
[Git 规范](git-workflow.md)。暂存时显式列出本轮文件，不将无关数据集等改动带入。

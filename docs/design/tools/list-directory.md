# 目录列表工具设计

核对日期：2026-10-26。状态：当前实现契约。架构归属：工具执行与资源访问。
源码入口：[list_directory.rs](../../../crates/vortex-tools/src/list_directory.rs)、[workspace_fs.rs](../../../crates/vortex-tools/src/workspace_fs.rs)。实际检查结果见根 [README](../../../README.md)，验收要求不代表已经通过。

本文件维护目录访问与输出边界，公共调用/结果见 [工具接口](tool-registry.md)，
多轮探索预算见 [Agent loop](../runtime/agent-loop.md)。

## 目录列表（list_directory v1）

CLI 在进入 TUI 前，通过 `spawn_blocking` 将启动时 `AppPaths.cwd` 打开为 `WorkspaceFs`，
并注入 `ListDirectoryTool`。根目录固定，不向父目录查找 Git 根，不使用 Vortex HOME；
模型不能提供新的根。工具不执行 `ls` 或 Shell，只列直接子项，不读正文、不写文件。

输入、输出示例：

```json
{"path":"crates/vortex-tools/src","limit":100}
```

```json
{
  "path":"crates/vortex-tools/src",
  "entries":[{"name":"lib.rs","type":"file"}],
  "truncated":false,
  "truncation_reasons":[]
}
```

输出仅为形状示例，不代表该目录实际只有一个文件。具体约束如下：

- `path` 必填，1～4096 UTF-8 字节，最多 32 个普通组件；`.` 表示根，`a/./b` 规范化为
  `a/b`。保留名称内空格，不 trim、不展开环境变量或通配符。拒绝绝对路径、`..`、控制
  字符、反斜杠、冒号和组件末尾的空格/点号；部分 Linux 合法文件名因此不受支持。
- `limit` 为可选整数，默认 100，允许 1～200；拒绝未知字段，包括 recursive/root。
  Schema 用于模型声明，实际校验由 Serde 与工具内路径规则执行。
- `.git`、`.vortex`、`.env`、`.env.*` 按 ASCII 大小写不敏感规则隐藏且禁止访问，包括
  `.env.example`；其他隐藏项不一律排除，不解析 `.gitignore`。
- 从已打开的 `cap_std::fs::Dir` 句柄逐个组件调用 `open_dir_nofollow`；路径任意一层为
  符号链接均拒绝。父目录列表可展示 `symlink` 类型，但不返回或读取其目标。
- 子项类型为 `directory/file/symlink/other`；不返回绝对路径、正文、大小或 mtime。
  名称须为可表示的 UTF-8，最多 1024 字节并满足名称规则；不支持的项跳过并标记截断，
  不使用有损名称重新访问路径。
- 最多扫描 4096 个原始条目，额外观察一项判断是否溢出；隐藏/跳过项也占扫描额度。
  对已扫描的有效项按名称排序，再按 limit 裁剪；最终序列化 JSON 最多 16 KiB，超预算
  移除末尾项后重新序列化，不截断 JSON 字节。无分页、递归或准确的全目录总数。
- `truncation_reasons` 去重列出 `scan_limit/entry_limit/output_limit/unsupported_name`。
  固定隐藏项本身不算截断。扫描受限时只排序已扫描部分，不承诺全目录字典序前 N 项，
  重复调用也不保证取得下一页。
- 空目录成功；不存在、无权限、不是目录、枚举或类型读取失败返回 ToolError，由注册表
  回填错误 ToolResult，不伪装成空列表。错误只包含静态上下文与 ErrorKind，不透传宿主路径。

打开子目录、枚举、排序与序列化放在 `spawn_blocking`；单实例 Semaphore 只允许一个后台
操作，忙时立即返回错误，不无限排队。permit 移入阻塞闭包，因此超时或取消等待不会提前
释放额度；已开始的阻塞操作无法被 async abort 强制停止。没有硬 I/O 超时，挂起的网络盘
仍可能拖延 runtime 退出；不能由扫描预算推断时长有界。

模型可多轮逐层查看子目录，仍共享每 Turn 四次工具调用预算。目录名与文件名是不可信数据，
也可能包含私密信息，会随 ToolResult 发给当前模型供应商；固定隐藏规则不等于秘密检测。
该句柄边界不是审批系统、进程沙箱或目录快照；其他进程仍可修改/重命名目录。Linux 为
当前验证平台，Windows junction/reparse point 与挂载边界需另行验收。

## 验收与后续边界

使用 TempDir 验证相对路径/链接拒绝、隐藏项、中文与非递归、扫描/条数/JSON 字节截断、
JSON 转义、错误回填及取消后后台额度保持。bootstrap 与工厂测试覆盖装配及 tools 能力要求。
真实模型调用链、普通用户权限错误、枚举时删除/替换、启动后 cwd 变化及 Windows
重解析点仍需专项验收，方法见 [回归测试方案](../../engineering/regression-test-plan.md)。

下一步有界 read_file 应另立契约；当前不提供分页、递归、文件读写、Shell、审批或硬 I/O
超时。第二个文件工具出现后再确认共享路径规则，不提前扩张 WorkspaceFs 的职责。

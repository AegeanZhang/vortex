# 安装与运行目录设计

本文定义 Vortex 的安装边界、用户 HOME 和跨平台运行目录，供实现与发布时遵循；配置内容与加载规则见 [配置设计](configuration.md)。它是产品目标，不代表安装器、Windows 支持或默认 HOME 已经实现。当前可运行方式仍以 [上手指南](../guide/getting-started.md) 和根 README 为准。

## 1. 安装入口、程序包与用户 HOME

三个位置职责独立，不要求它们在文件系统中完全分开：

| 位置 | 职责 | 定位方式 |
| --- | --- | --- |
| 命令入口 | 用户通过 PATH 找到 `vortex` 或 `vortex.exe` | 安装器、包管理器或用户决定 |
| 程序包 | 当前版本的二进制及随包资源 | 发布与安装方式决定 |
| Vortex HOME | 用户配置、日志及后续持久数据 | 本文规定的默认值或 `VORTEX_HOME` |

Linux/macOS 的命令入口可以是用户 bin 目录中的程序或软链接；Windows 可以是安装目录中的 exe。配置加载不得依赖软链接目标、exe 所在目录或安装器实现。更新、替换或移动二进制，不应自动覆盖用户配置和数据。

首版以已有构建方式或手动部署二进制为基础，不新增在线安装脚本、安装参数、自更新命令或版本切换机制。以后若在 HOME 下增加 `packages/`，应由安装器维护，不属于配置模块职责，也不能随普通缓存一起清理。

## 2. 统一用户 HOME

所有平台使用“系统用户主目录下的 `.vortex`”，不按 XDG、AppData 拆分用户文件：

| 环境 | 默认示例 |
| --- | --- |
| Linux | `/home/ming/.vortex` |
| macOS | `/Users/ming/.vortex` |
| Windows 原生 | `C:\Users\ming\.vortex` |
| WSL | WSL 用户的 `/home/ming/.vortex` |

Windows 原生与 WSL 默认分离，不自动共用认证、会话或运行状态。上述示例不是硬编码路径，用户名和用户目录可能不同。

### HOME 选择规则

1. 进程中存在 `VORTEX_HOME`：按原生路径读取，要求非空、为绝对路径；非法时立即报错，不回退默认值。
2. 未设置：通过系统用户主目录解析得到 `<user-home>/.vortex`，同样验证为绝对路径。
3. 无法确定有效用户主目录：明确报错，不使用 cwd、程序安装目录或临时目录代替。

路径解析不要求目录已存在，不先对尚不存在的 HOME 做 canonicalize。`~`、`$HOME`、`%USERPROFILE%` 在环境变量值内不由 Vortex 再次展开；需要 shell 先生成实际绝对路径。

本地隔离开发数据的目标用法：

```bash
export VORTEX_HOME="${HOME}/.vortex-dev"
```

```powershell
$env:VORTEX_HOME = Join-Path $HOME '.vortex-dev'
```

Vortex HOME 不等于操作系统 HOME，不得为改变 Vortex 路径而改写系统的 HOME/USERPROFILE。也不能假设 Windows 用户目录总在 C 盘。

## 3. 用户目录内容与生命周期

```text
<vortex-home>/
├── config.toml       # 用户配置，只读加载
├── logs/
│   └── vortex.log    # 当前日志文件目标
├── sessions/         # 后续：会话数据
├── state/            # 后续：UI/运行状态
└── cache/            # 后续：可重新生成的数据
```

- 用户配置由用户维护；缺失时采用内置默认值，不自动生成模板或覆盖现有文件。
- 日志初始化只创建所需的 HOME/日志目录；失败应在进入 TUI 之前报告。
- `sessions/`、`state/`、`cache/` 尚未接入时不创建空目录，不用目录存在来表示功能完成。
- 配置和会话应可独立备份；删除缓存不能删除会话，清理日志不能递归删除 HOME。
- 更改 HOME 不自动搬迁旧数据；已有开发脚本仍可显式使用原目录，迁移需另行执行。
- 首版不在 HOME 中创建凭据文件，密钥规则见 [凭据设计](credentials.md)。

## 4. 工作目录与项目配置

启动 cwd 是用户本次工作的目录。项目配置只检查：

```text
<startup-cwd>/.vortex/project.toml
```

不向父目录查找，不自动识别 Git 根，不合并多个目录层的项目配置，也不因为读取配置改变 cwd。配置定位和以后 `AGENTS.md` 等项目指令的发现是不同契约，不能互相推断。

例如 `demo/.vortex/project.toml` 存在，但从 `demo/src` 启动：只检查 `demo/src/.vortex/project.toml`。需要使用 demo 的配置时，先在 demo 启动；首版不为此新增目录切换参数。

启动诊断应同时给出 cwd、用户配置检查路径及项目配置检查路径，并区分“已加载”与“文件不存在”。不要只显示最终模型而隐藏配置来源；具体输出规则见 [配置设计](configuration.md#6-配置来源与可观测性)。

## 5. 跨平台路径、权限与安全

路径实现集中在 CLI 的路径解析层，使用 `OsString`/`PathBuf`、`join` 和 `is_absolute`，避免手拼分隔符或强制 UTF-8。用户主目录可通过标准库 `std::env::home_dir()` 获取；其 Unix/Windows 规则与 Windows 绝对路径判断以 [Rust HOME API](https://doc.rust-lang.org/std/env/fn.home_dir.html) 和 [Path API](https://doc.rust-lang.org/std/path/struct.Path.html#method.is_absolute) 为参考。

- Windows 的 `C:\vortex-data` 是绝对路径，`C:vortex-data` 和 `\vortex-data` 不满足本设计要求。网络共享等路径不据此宣称已验收，首版验收以本地文件系统为准。
- 正常支持中文、空格和平台原生路径；诊断转义路径中的控制字符，非 Unicode 路径仅在显示时允许有损展示，不将展示文本转换回 I/O 路径。
- 后续会话名采用跨平台安全 ID，不直接使用模型名、项目绝对路径或用户标题；生成时间戳文件名不含冒号，不依赖文件名大小写区分身份。
- Unix 创建新的私有 HOME/日志目录时使用 0700，新日志文件使用 0600；不自动 chmod 已有 HOME 或其他用户指定的父目录。
- Windows 需单独验证继承 ACL 与实际访问权限；不能把 Unix mode 当作 Windows 保密保证。配置所在目录与项目路径检查还要覆盖 junction 等平台行为，不声称已抵御所有路径替换攻击。
- 用户 HOME 是默认可信的配置来源，不代表验证过文件作者；不要把陌生仓库中的 `.vortex` 指定为 HOME。项目配置只能使用受限字段。

这里确定跨平台目录目标，不等于整个 CLI、TUI、工具和沙箱已在 Windows/macOS 验证。Bubblewrap 仍是 Linux 专属边界。

## 6. 系统配置与首版边界

首版不读取 `/etc/vortex`、ProgramData、安装目录内的外置 system 配置或隐藏配置文件。`~/.vortex/config.toml` 是当前用户的跨项目配置，不是整机配置。

产品默认值随程序发布；不可突破的响应大小等限制由校验及执行边界保证。未来若需要管理员统一默认值或强制策略，应单独设计，不能仅给普通文件改名为 `system.toml`，也不能通过换 HOME 绕过策略。

## 7. 验收要求

- 默认 HOME、合法覆盖、空覆盖、相对路径、主目录查询失败、目录尚不存在。
- 从不同 cwd 启动仍使用同一用户配置；更换二进制位置不改变 HOME。
- 从仓库子目录启动不读取父目录项目配置，诊断能解释实际来源。
- 文件缺失可继续；不可读、不可写日志目录或非法配置明确失败，不偷偷换位置。
- Windows 中文/空格路径、盘符相对路径、ACL/junction、原生与 WSL 隔离分别验收；Linux 通过不能替代 Windows 结果。
- 只创建必要目录，不自动生成配置、搬迁数据或触碰未实现的持久化目录。

# DebugTUI 1.3.1：Bao／VM 多 ELF 调试与镜像编辑修复

Windows x64 正式版本，标签 `v1.3.1`，从 `main` 发布。包含本地 1.3.0 已验收的 Bao／VM 多 ELF 功能，以及本版 Symbol images 编辑修复。

## 多 ELF 调试

- 每核同时保留 Bao 与对应 VM 的 ELF，根据 PC、可执行 section 和程序 CPSR 识别停止位置；跨 HVC／虚拟 IRQ 的源码显示不需要重连、复位或下载。
- Setup 的 Symbol images 管理 ELF、核心、域、源码根／映射、重定位偏移和内存通道；Files、Symbols、Source、反汇编、Watch 与断点保留镜像归属。
- 通过 Bao 的 `bao_v1` 诊断 ABI，只读查看挂起 Guest 的真实整数寄存器、banked SP／LR、源码、栈和局部变量；无需 Python 或 VS Code。
- 完整 HVC32 边界支持返回 Guest、运行中取消和真实停止优先，修复 all-stop 取消后的硬件断点遗留及断点资源耗尽后的状态恢复。
- 运行中标量 Watch 绑定镜像、核心和内容版本；ELF 变化后拒绝旧地址／类型；源码映射兼容正反斜杠。寄存器目录声明缓存有界复用，避免重复解析拖慢连接。

## Symbol images 编辑修复

1.3.0 的 Symbol images 文本字段在 Enter 后已经接受输入，但没有光标、编辑标题或确认/撤销提示，容易看成只读。1.3.1 增加可见插入光标和 Editing 提示，长路径的显示窗口跟随光标，支持首尾及中文路径编辑。

同时修复编辑中点击另一行时可能将输入提交到新字段的问题：先提交原字段，再切换焦点；解析失败保留原字段，取消可直接丢弃。字段说明随焦点更新，旧错误提示不再残留到其他字段。Apply/Save 保留已有 saved_context 和 sections 高级配置。

使用方法：Symbol images → 选字段 → Enter → 输入/粘贴 → Enter 确认或 Esc 撤销 → Apply to draft → 首页 Save config。Ctrl+U 清空字段，Home/End 移动光标。详细配置见 [Bao＋VM 使用说明](bao-multi-elf.md)。

## 安装与使用

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.3.1
```

固定本版可将 `latest/download` 换成 `download/v1.3.1`；便携用户下载完整 ZIP。Bao 本地参考工程在其根目录执行 `debugtui --project .\debug-multi-images.toml`，或在 Setup 的 Project 行选择该文件。默认 `debugtui` 仍读取 `debug.toml`，不会把旧单 ELF 项目自动改成多 ELF。

完整运行包附带 ARM GDB、OpenOCD、芯片／探针描述、本地实际存在的 SVD 和许可证；THA6 SVD 仍不提交 Git。升级保留包外配置。Guest 保存视图需要对应 Bao 固件提供诊断 ABI，安装 DebugTUI 本身不修改固件。配置及常用命令见 [Bao＋VM 说明](bao-multi-elf.md)，完整手册第 13.5～13.11 节同步更新。

## 验证与限制

1.3.1 全量库测试 610 项通过、0 失败、3 项原有忽略；Release 构建、严格 Clippy、真实终端编辑 8 项及多 ELF 软件回归 10 项通过。分发验证使用 SHA256 核对的真实 1.2.4 包，覆盖 14 项升级、配置保留及卸载检查；公网 Latest 的安装、重复安装、CMD／PowerShell 入口和校验和另行核验，结果以附件 `validation-1.3.1.json` 为准。

完整 Bao／VM THA6206 实板和终端 74/74 属于 [1.3.0 基线](release-1.3.0.md)，对应原 EXE／固件摘要；不改记为 1.3.1 新二进制的整套实板结果。1.3.1 的功能差异集中于镜像编辑器，其验证与基线分别记录。

本版在连接前常驻各 ELF；运行中新增／热替换 ELF 未作为能力交付。运行中身份为最后已知，内存采样不是 Trace。隔离求值限 ARM32 小端 DWARF 的只读表达式子集；隔离赋值、命名全局条件及不可恢复的优化／浮点上下文不宣称支持。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.3.1.tgz`：固定名称及版本化 npm 包。
- `debugtui-1.3.1-win-x64.zip`：带工具与许可证的便携包。
- `debugtui-windows-x64-1.3.1.exe`：独立程序，需另备工具资源。
- `debugtui-usermanual-1.3.1.pdf` / `.tex`：与源码同步的完整手册，Bao 多 ELF 内容位于正文第 47～53 页。
- `corresponding-source.zip`：随包修改版 OpenOCD 的对应源码，与 PROVENANCE 摘要匹配。
- `validation-1.3.1.json` / `SHA256SUMS.txt`：分层验证摘要和附件校验和。

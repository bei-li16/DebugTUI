# DebugTUI 1.3.0：Bao＋多 VM 常驻多 ELF 调试

Windows x64。本页记录本地 1.3.0 的构建、安装和实板验收基线；这些能力连同镜像编辑修复一起通过 [1.3.1](release-1.3.1.md) 正式分发。

## 更新内容

- 每核同时加载 Bao 和所属 VM 的 ELF；停止后按 PC、可执行 section 和程序 CPSR 识别镜像，保持物理 GDB／OpenOCD 连接。
- Setup 的 Symbol images 编辑 ELF、核心、源码根／映射、域、偏移和内存通道；Files、Symbols、反汇编、Source、Watch 和断点保留镜像归属。
- Bao 诊断快照恢复挂起 Guest 的真实整数寄存器、banked SP／LR、源码、调用栈和局部变量，无需 Python 或 VS Code。
- 提供 HVC 返回 Guest、运行中取消、用户暂停与异常优先；修复 GDB all-stop 删除失败后遗留硬件断点，以及资源耗尽后界面错误维持 RUNNING 的问题。
- 运行中标量 Watch 绑定镜像、核心、内容版本；同路径 ELF 变化后拒绝旧地址／类型。源码路径支持镜像独立映射及两种斜杠。
- 缓存有界复用寄存器目录声明解析，避免大型 TOML 反复解析拖慢多核连接；寄存器值及访问证据不跨核心共享。
- 同步完整 TeX/PDF 手册的第 13.5～13.11 节：配置、操作、诊断 ABI、迁移、限制和 THA6206 实板验收；修正旧版不支持多镜像的说明。

## 本地基线安装与使用

在保留本地基线包的 DebugTUI 仓库目录执行：

```powershell
npm.cmd install -g --offline --no-audit --no-fund --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli .\artifacts\debugtui-cli-1.3.0.tgz
debugtui --version
# debugtui 1.3.0
```

使用公开版本请按 [1.3.1 安装说明](release-1.3.1.md)。在已配置的 Bao 工程目录直接运行：

```powershell
debugtui --project .\debug-multi-images.toml
```

参考配置连接 core0／core1，分别保留 Bao＋VM0／Bao＋VM1。该工程使用 `-DebugGuestContext` 诊断固件；无需启用仅供控制负例测试的 `-DebugReturnTests`。配置和常用命令见 [Bao＋VM 使用说明](bao-multi-elf.md)。原有单 ELF 工程继续使用 `program.elf`。

完整运行包包含 GDB、OpenOCD、芯片／探针描述及本地实际存在的 SVD；THA6 SVD 继续由 Git 忽略。升级保留包外用户配置。Bao 的 Guest 诊断功能需要对应固件提供 `bao_v1` ABI；安装 DebugTUI 本身不修改 Bao 固件。

## 验证与范围

已测试 Release EXE 与全局安装版在 THA6206 上完成实板和真实终端 74/74，0 失败、0 跳过；另有 Rust 库测试 606 项、集成 7 项、软件多镜像 10 项和本机 GDB 21 组通过。原有 3 项 Rust 忽略用例单列，不计入通过。测试结束已恢复普通 `-DebugGuestContext` 固件。

本地版本／EXE／固件摘要、各层测试和清理结果记录在 `artifacts/validation-1.3.0.json`；原始实板报告继续保留。1.3.1 是另一次构建，其新增验证在该版说明和 Release 附件中单列。历史开发 EXE 的测试保留在 [阶段验收记录](bao-multi-elf-development.md)，不代替安装版验证。

镜像在连接前常驻；运行中新增／替换 ELF 未作为本版能力验证。运行中显示最后已知身份和采样内存，不提供 Trace。隔离求值为 ARM32 小端 DWARF 的只读表达式子集；全局赋值和命名全局条件断点仍拒绝，局部／类型地址条件可用。不可恢复的优化／浮点上下文明确显示不可用。

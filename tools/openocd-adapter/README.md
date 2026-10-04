# OpenOCD ARMv8 寄存器适配补丁

这是独立的 GPL-2.0-or-later OpenOCD 修改，固定上游 `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e` 和 Jim Tcl 子模块版本。校验和见 `source.lock.json`，补丁可以干净应用到该提交。现有 xPack Windows OpenOCD 没有被替换；相同的 `0.12.0` 版本号不证明具备本适配器。

`aarch64 mrrc cpnum op1 CRm` 对暂停的 AArch32 核执行一次 MRRC，将 R0/R1 的低、高字拼成 64 位结果，输出固定 16 位十六进制。它先保存物理 R0/R1，恢复后回读检查，不用缓存代替物理原值。补丁也使该命令组的 `aarch64 mrc/mcr` 保存、恢复并回读 R0。通用 `arm mrc/mcr` 和内部旧 DPM 路径没有被改造。

`aarch64 isb` 根据实际 DSCR 注入 AArch32/AArch64 的真正 ISB，不依赖或修改 CP15BEN。新通道先读 PRSR 确认物理核已暂停，检查错误与执行状态；传输、指令或恢复故障立即停止，把核心标为 unknown。不通过旧异常恢复函数切换模式，不自动重试或注入推测的回滚。成功路径没有 PC、CPSR 或 FPU 控制写入；调试异常时这些状态可能被硬件改变，应由操作者重连并决定复位恢复。

`aarch64 debugtui_adapter` 返回固定协议 `debugtui-armv8-1 mrrc isb scratch-readback stop-on-fault`。在新编译的 DebugTUI 中显式配置：

```toml
[registers]
cpu = "cortex-r52"
cp15_command = "aarch64 mrc"
cp15_64_command = "aarch64 mrrc"
selector_command = "aarch64 mcr"
isb_command = "aarch64 isb"
tcl_endpoint = "127.0.0.1:6666"

[registers.targets]
core0 = "board.cpu0"
core1 = "board.cpu1"
```

核心和 target 名称须与实际工程一致。MRRC 与真正 ISB 每次在同一个 target 事务中检查协议；没有命令或协议不符时，先返回 reader unsupported。旧工程不配置新字段时，64 位读仍使用具名 GDB 寄存器，选择器同步仍要求实际 CP15BEN 已开启。配置不能代替硬件身份、Timer 实现或权限证据，也不开启系统寄存器 writer。

## 构建和软件验证

在具备 Git、GCC、make、autoconf、automake、libtool 和 pkg-config 的 Unix 构建环境中执行：

```sh
sh tools/openocd-adapter/build.sh .dev/openocd-adapter-new-build
```

输出目录必须是新路径。脚本保留固定版本源码、安装目录、GPL 许可和测试报告；可追加 configure 选项选择探针及交叉工具链。USB 接口需要对应开发依赖。默认包含 dummy/remote-bitbang，关闭 J-Link 子模块。本批在 WSL Ubuntu 22.04 完成 Linux 构建与真实命令检查；上述 Linux 一键脚本的全新目录构建仍待重新执行。Windows 使用下面单独经过全新目录构建验证的脚本。

检查已应用补丁的源码和后端：

```sh
python3 tools/openocd-adapter/test.py --source PATH_TO_PINNED_SOURCE --out artifacts/openocd-adapter-tests --openocd PATH_TO_BACKEND
```

事务测试编译生产使用的同一份头文件，验证完整高字、一次 MRRC、MRC/MCR 的物理恢复、19 个传输失败点和三类恢复值不匹配，严格 C 警告检查通过。命令检查仅初始化进程内 dummy 虚拟适配器，保持 target 未 examine，核对协议、帮助以及参数和状态的精确原生错误码；所有端口关闭，不连接实际探针/板卡。`tests/encoding.s` 用 GNU Arm 汇编器独立确认 MRRC 和 Thumb ISB 编码。

Linux 候选后端 SHA-256 为 `80f2c574531855a50c6e4f52e9760d1603a296c3d8cca1579cbd995bfe245bd5`，版本 `0.12.0+dev-gd3ebb8d-dirty`。dirty 来自尚未成为上游提交的适配补丁。候选产物未安装或发布，未来分发须保留源码、补丁与许可，不能把其能力写到现有 Windows 二进制上。

## Windows 后端、依赖和候选包

在 Linux／WSL 中准备主机 GCC、Git、Python 3（tarfile 支持 `filter='data'`）、make、autotools、pkg-config 以及 MinGW-w64 x64 的 gcc、windres、objdump、strip，然后执行：

```sh
sh tools/openocd-adapter/build-windows.sh /tmp/openocd-windows-new
```

脚本不安装主机依赖，不覆盖已有输出路径。`OPENOCD_CROSS_PREFIX` 可指定工具前缀，编译器的目标必须是 `x86_64-w64-mingw32`。固定的 libusb 1.0.30 归档 SHA-256、HIDAPI 0.15.0 和 LibJaylink 提交及许可选择见 `windows-dependencies.lock.json`；Jim Tcl 和 OpenOCD 提交仍由 `source.lock.json` 管理。HIDAPI 的 Windows 单文件嵌入方式已包含描述符重建代码，不能再重复编译该 C 文件。构建隔离 pkg-config 主机库，明确指定 build/host，不借 WSL 的 Windows 程序互操作误判为原生编译。

需要 MinGW 及 GCC runtime 的版权文本；默认从编译器安装前缀的 `share/doc/` 读取 Debian/Ubuntu 文件。其他工具链可在调用 `windows.py stage` 时提供 `--mingw-license` 和 `--gcc-license`。本批使用 Ubuntu 22.04 的 GCC 10 POSIX MinGW-w64 工具链。源码固定不意味着产物逐字节可复现：OpenOCD 构建时间、工具链与路径会影响文件哈希，实际编译器版本及哈希记入 `install/PROVENANCE.json`。

输出包含 OpenOCD、完整脚本、libusb／HIDAPI DLL、两份现有 F429 配置、许可、每个文件的字节数／SHA-256、依赖导入表、`corresponding-source.zip` 和候选运行包。源码包保留完整 OpenOCD／Jim Tcl／LibJaylink／HIDAPI checkout、Git 对象、libusb 原始归档和构建／测试配方。解压源码包后，把它作为脚本的第二个参数，可从这些缓存提交构建全新目录而不下载源代码；主机工具链仍需事先准备。

```sh
sh tools/openocd-adapter/build-windows.sh /tmp/openocd-windows-another /path/to/extracted-source
```

构建检查全部 PE 为 x64、USB/HID 等通道确实编译、静态 DLL 导入依赖闭合；不从 PATH 猜补依赖，不把跨平台编译记为 Windows 原生验证。HID／USB 动态加载的 Windows 系统组件和探针 USB 驱动由宿主提供；离线检查不证明实际驱动兼容。把输出的 `source`、`install`、`corresponding-source.zip` 放到同一个 Windows 输出目录后执行（示例编译器路径需对应本机）：

```powershell
python tools/openocd-adapter/windows.py verify --root G:/Build/openocd-windows-new --cc C:/MinGW/bin/gcc.exe --objdump C:/MinGW/bin/objdump.exe
python tools/openocd-adapter/tests/windows-package.py --candidate G:/Build/openocd-windows-new/install --source-archive G:/Build/openocd-windows-new/corresponding-source.zip --objdump C:/MinGW/bin/objdump.exe
```

原生检查先验证清单、配方和源码 ZIP，再执行生产事务测试及真实后端命令检查，核对 J-Link、CMSIS-DAP、ST-Link、FTDI 的注册，以及两份 F429 配置和 HID／USB bulk 后端能离线加载。物理配置加载在 config 阶段结束，显式 init 被拒绝；只在前面的独立事务检查中初始化进程内 dummy，不连接真实探针或打开调试端口。检查成功后才把 `native_windows_verified` 标为 true 并重新生成候选 ZIP。九项包检查包含缺失 DLL、非 PE、同大小篡改、新增 DLL、有效 ZIP 内补丁篡改和 Git 空目录遗失拒绝。

Windows 候选后端 SHA-256 为 `06dbc62b6ddfc52ab88a95cfe608d2a76baf7410653d06f51559c323f25582ab`，版本 `0.12.0+dev-gd3ebb8d-dirty (2026-10-04-18:27)`。本批完整的新目录 Windows 构建及原生命令、依赖、配置检查通过；源码 ZIP 缓存提交回读已核对，仍不代表探针通信或 R52 实板指令执行通过。现有 `tools/bin/openocd`、依赖锁及安装未改动。最终 tools/profile 合并、安装升级和整个任务完成后的 Release 仍待完成；不能只分发运行 ZIP 而遗漏对应源码与许可。

## 延后执行的物理核心用例

`scripts/test-register-timer-hardware.cjs` 默认输出五项 skipped，不连接。专用暂停工程须禁止连接/退出时自动运行、复位或下载，并设置 `on_exit = "disconnect"`。明确提供 `--run --binary EXE --project TOML --core NAME --case JSON` 才执行 REG-H05。`tests/fixtures/register-timer-board.example.json` 是软件占位，需替换实际 MIDR、CVAL、合理差值和独立证据，移除 `software_example` 后才能上板执行。

只读钩子 `tests/fixtures/register-timer-board.c` 记录 CNTPCT/CNTVCT/CNTP_CVAL 的固件 MRRC 样本，不开启 Timer 或修改比较值。各核分别编译固件或将基线改为 per-core 数组，据此修改 case 的变量引用。固件初始化需建立权限、明确 debug freeze 条件，并配置至少一个高字非零的已知稳定 CVAL。驱动按完整 64 位模运算检查基线差值和进展，避免要求不同时间采样相等；检查当前核、稳定控制、选择器、可选 peer 和客户配置未变。GDB 的核心 guard 是逻辑视图，不能单独证明物理 R0/R1 恢复；物理回读由固定适配器内部执行。

真正 ISB 的物理用例复用 `scripts/test-register-selectors-hardware.cjs`：专用固件预先关闭 CP15BEN，工程设置 `isb_command`，分别执行 EL1/EL2 MPU 和 PMU 已知索引，保留 `evidence.synchronization`、原/恢复选择器及控制状态对比。不得为通过用例而启用 CP15BEN；未知结果不自动继续、复位或重试。Timer 驱动已在实际 DebugTUI 可执行文件的双核软件模型六阶段通过，包含独立基线接口和 peer 检查。当前均未上板执行，Linux 编译和软件模拟不记作板卡支持证明。
